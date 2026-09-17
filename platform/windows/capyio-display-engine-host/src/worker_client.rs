use std::{
    fmt, io, ptr, thread,
    time::{Duration, Instant},
};

use capyio_windows_display::{DisplaySessionError, verify_current_process_display_session};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_NO_DATA, ERROR_PIPE_BUSY,
        ERROR_PIPE_NOT_CONNECTED, GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE,
    },
    Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING, ReadFile, WriteFile},
    System::Pipes::{PIPE_NOWAIT, PIPE_READMODE_BYTE, SetNamedPipeHandleState, WaitNamedPipeW},
};

use crate::{
    AuthenticatedEngineLaunch, DISPLAY_WORKER_CONFIG_BYTES, DISPLAY_WORKER_CONTROL_BYTES,
    DISPLAY_WORKER_LEASE_ACK_BYTES, DISPLAY_WORKER_LEASE_BYTES, DISPLAY_WORKER_START_BYTES,
    DisplayWorkerConfigurationError, DisplayWorkerControlCommand, DisplayWorkerControlRecord,
    DisplayWorkerEngineConfigRecord, DisplayWorkerEngineConfiguration, DisplayWorkerLease,
    DisplayWorkerProtocolError, DisplayWorkerStartRecord, acknowledge_display_worker_start,
    encode_display_worker_config_ack, encode_display_worker_control_ack,
    encode_display_worker_lease_ack,
};

const PIPE_PREFIX: &str = r"\\.\pipe\CapyIO.DisplayWorker.";
const MAX_PIPE_SUFFIX_BYTES: usize = 64;
const EXCHANGE_DEADLINE: Duration = Duration::from_secs(5);
const RETRY_INTERVAL: Duration = Duration::from_millis(5);

pub struct DisplayWorkerPipeName(String);

impl DisplayWorkerPipeName {
    pub fn parse(value: &str) -> Result<Self, DisplayWorkerClientError> {
        let suffix = value
            .strip_prefix(PIPE_PREFIX)
            .ok_or(DisplayWorkerClientError::InvalidPipeName)?;
        if suffix.is_empty()
            || suffix.len() > MAX_PIPE_SUFFIX_BYTES
            || !suffix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.'))
        {
            return Err(DisplayWorkerClientError::InvalidPipeName);
        }
        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Retains the authenticated service channel for the complete engine lifetime.
/// Dropping it closes only this worker's pipe handle.
pub struct DisplayWorkerConnection {
    pipe: OwnedPipe,
    last_control_sequence: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayWorkerControlEvent {
    Disconnected,
    Command {
        command: DisplayWorkerControlCommand,
        record: DisplayWorkerControlRecord,
    },
}

impl DisplayWorkerConnection {
    /// Waits for one complete, lease-bound command or exact service
    /// disconnection. Commands must use an uninterrupted sequence beginning at
    /// one, preventing replay within a retained worker lifetime.
    pub fn wait_for_control(
        &mut self,
        lease: &DisplayWorkerLease,
    ) -> Result<DisplayWorkerControlEvent, DisplayWorkerClientError> {
        let mut bytes = [0_u8; DISPLAY_WORKER_CONTROL_BYTES];
        let mut offset = 0;
        while offset < bytes.len() {
            let mut read = 0_u32;
            // SAFETY: the retained pipe is live and the remaining fixed record
            // storage is writable for this synchronous call.
            let result = unsafe {
                ReadFile(
                    self.pipe.0,
                    bytes[offset..].as_mut_ptr(),
                    u32::try_from(bytes.len() - offset).expect("control record fits u32"),
                    &mut read,
                    ptr::null_mut(),
                )
            };
            if result != 0 && read > 0 {
                offset += read as usize;
                continue;
            }
            // SAFETY: GetLastError immediately follows ReadFile.
            let error = if result == 0 {
                unsafe { GetLastError() }
            } else {
                ERROR_NO_DATA
            };
            if matches!(error, ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED) {
                return if offset == 0 {
                    Ok(DisplayWorkerControlEvent::Disconnected)
                } else {
                    Err(DisplayWorkerClientError::PartialControlRecord)
                };
            }
            if error != ERROR_NO_DATA {
                return Err(DisplayWorkerClientError::Read(
                    io::Error::from_raw_os_error(error as i32),
                ));
            }
            thread::sleep(RETRY_INTERVAL);
        }
        let record = DisplayWorkerControlRecord::from_bytes(bytes);
        let (command, sequence) = record.decode_for(lease)?;
        advance_control_sequence(self.last_control_sequence, sequence)?;
        self.last_control_sequence = sequence;
        Ok(DisplayWorkerControlEvent::Command { command, record })
    }

    pub fn acknowledge_control(
        &self,
        lease: &DisplayWorkerLease,
        record: &DisplayWorkerControlRecord,
    ) -> Result<(), DisplayWorkerClientError> {
        let acknowledgement = encode_display_worker_control_ack(record, lease)?;
        write_all(
            self.pipe.0,
            &acknowledgement,
            Instant::now() + EXCHANGE_DEADLINE,
        )
    }

    /// Waits until the service closes the channel. No post-start command byte is
    /// accepted by this protocol revision; receiving one fails closed and lets
    /// the caller shut down the native session.
    pub fn wait_for_disconnect(&self) -> Result<(), DisplayWorkerClientError> {
        let mut byte = [0_u8; 1];
        loop {
            let mut read = 0_u32;
            // SAFETY: pipe remains exclusively owned by self and byte is live
            // writable storage for the one-byte bounded probe.
            let result = unsafe {
                ReadFile(
                    self.pipe.0,
                    byte.as_mut_ptr(),
                    1,
                    &mut read,
                    ptr::null_mut(),
                )
            };
            if result != 0 && read > 0 {
                return Err(DisplayWorkerClientError::UnexpectedPostStartData);
            }
            // SAFETY: GetLastError immediately follows ReadFile.
            let error = if result == 0 {
                unsafe { GetLastError() }
            } else {
                ERROR_NO_DATA
            };
            if matches!(error, ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED) {
                return Ok(());
            }
            if error != ERROR_NO_DATA {
                return Err(DisplayWorkerClientError::Read(
                    io::Error::from_raw_os_error(error as i32),
                ));
            }
            thread::sleep(RETRY_INTERVAL);
        }
    }
}

fn advance_control_sequence(last: u32, actual: u32) -> Result<u32, DisplayWorkerClientError> {
    let expected = last
        .checked_add(1)
        .ok_or(DisplayWorkerClientError::ControlSequenceExhausted)?;
    if actual != expected {
        return Err(DisplayWorkerClientError::UnexpectedControlSequence { expected, actual });
    }
    Ok(expected)
}

pub struct PreparedDisplayWorker {
    connection: DisplayWorkerConnection,
    lease: DisplayWorkerLease,
    configuration: DisplayWorkerEngineConfiguration,
    launch: AuthenticatedEngineLaunch,
}

impl PreparedDisplayWorker {
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        DisplayWorkerConnection,
        DisplayWorkerLease,
        DisplayWorkerEngineConfiguration,
        AuthenticatedEngineLaunch,
    ) {
        (self.connection, self.lease, self.configuration, self.launch)
    }
}

/// Runs the non-secret worker lease handshake over one bounded local pipe.
/// Native engine creation and launch material are deliberately outside it.
pub fn run_display_worker_lease_client(
    pipe_name: &DisplayWorkerPipeName,
) -> Result<DisplayWorkerLease, DisplayWorkerClientError> {
    let deadline = Instant::now() + EXCHANGE_DEADLINE;
    let pipe = open_pipe(pipe_name, deadline)?;
    set_nonblocking(pipe.0)?;
    let mut lease_record = [0_u8; DISPLAY_WORKER_LEASE_BYTES];
    read_exact(pipe.0, &mut lease_record, deadline)?;
    let (lease, acknowledgement) = acknowledge_display_worker_lease(&lease_record)?;
    write_all(pipe.0, &acknowledgement, deadline)?;
    Ok(lease)
}

/// Completes both phases on the same PID/Session-bound pipe. The second phase
/// has its own bounded deadline so an authenticated service cannot retain a
/// worker indefinitely after the non-secret lease acknowledgement.
pub fn run_display_worker_start_client(
    pipe_name: &DisplayWorkerPipeName,
) -> Result<
    (
        DisplayWorkerLease,
        DisplayWorkerEngineConfiguration,
        AuthenticatedEngineLaunch,
    ),
    DisplayWorkerClientError,
> {
    let prepared = connect_display_worker_start_client(pipe_name)?;
    let (_connection, lease, configuration, launch) = prepared.into_parts();
    Ok((lease, configuration, launch))
}

/// Completes all three startup phases while retaining the exact pipe that
/// carried them. Production workers keep this owner until native shutdown.
pub fn connect_display_worker_start_client(
    pipe_name: &DisplayWorkerPipeName,
) -> Result<PreparedDisplayWorker, DisplayWorkerClientError> {
    let lease_deadline = Instant::now() + EXCHANGE_DEADLINE;
    let pipe = open_pipe(pipe_name, lease_deadline)?;
    set_nonblocking(pipe.0)?;
    let mut lease_record = [0_u8; DISPLAY_WORKER_LEASE_BYTES];
    read_exact(pipe.0, &mut lease_record, lease_deadline)?;
    let (lease, acknowledgement) = acknowledge_display_worker_lease(&lease_record)?;
    write_all(pipe.0, &acknowledgement, lease_deadline)?;

    let config_deadline = Instant::now() + EXCHANGE_DEADLINE;
    let mut config_bytes = [0_u8; DISPLAY_WORKER_CONFIG_BYTES];
    read_exact(pipe.0, &mut config_bytes, config_deadline)?;
    let config_record = DisplayWorkerEngineConfigRecord::from_bytes(config_bytes);
    let configuration = DisplayWorkerEngineConfigRecord::decode(*config_record.as_bytes(), &lease)?;
    let config_acknowledgement = encode_display_worker_config_ack(&config_record, &lease)?;
    write_all(pipe.0, &config_acknowledgement, config_deadline)?;

    let start_deadline = Instant::now() + EXCHANGE_DEADLINE;
    let mut start_bytes = [0_u8; DISPLAY_WORKER_START_BYTES];
    read_exact(pipe.0, &mut start_bytes, start_deadline)?;
    let start = DisplayWorkerStartRecord::take_from(&mut start_bytes);
    let (launch, acknowledgement) = acknowledge_display_worker_start(start, &lease)?;
    write_all(pipe.0, acknowledgement.as_bytes(), start_deadline)?;
    Ok(PreparedDisplayWorker {
        connection: DisplayWorkerConnection {
            pipe,
            last_control_sequence: 0,
        },
        lease,
        configuration,
        launch,
    })
}

pub fn acknowledge_display_worker_lease(
    lease_record: &[u8; DISPLAY_WORKER_LEASE_BYTES],
) -> Result<(DisplayWorkerLease, [u8; DISPLAY_WORKER_LEASE_ACK_BYTES]), DisplayWorkerClientError> {
    let lease = DisplayWorkerLease::decode(lease_record)?;
    verify_current_process_display_session(lease.windows_session_id)?;
    let acknowledgement = encode_display_worker_lease_ack(lease_record)?;
    Ok((lease, acknowledgement))
}

fn open_pipe(
    pipe_name: &DisplayWorkerPipeName,
    deadline: Instant,
) -> Result<OwnedPipe, DisplayWorkerClientError> {
    let mut wide = pipe_name.as_str().encode_utf16().collect::<Vec<_>>();
    wide.push(0);
    loop {
        let remaining = remaining_millis(deadline)?;
        // SAFETY: wide is a live NUL-terminated UTF-16 pipe name.
        let _ = unsafe { WaitNamedPipeW(wide.as_ptr(), remaining) };
        // SAFETY: the validated name is NUL-terminated, no security attributes
        // or template handle are supplied and a successful handle is owned below.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            return Ok(OwnedPipe(handle));
        }
        // SAFETY: GetLastError immediately follows CreateFileW.
        let error = unsafe { GetLastError() };
        if matches!(error, ERROR_PIPE_BUSY | ERROR_FILE_NOT_FOUND) && Instant::now() < deadline {
            thread::sleep(RETRY_INTERVAL);
            continue;
        }
        return Err(DisplayWorkerClientError::Open(io::Error::last_os_error()));
    }
}

fn set_nonblocking(handle: HANDLE) -> Result<(), DisplayWorkerClientError> {
    let mode = PIPE_READMODE_BYTE | PIPE_NOWAIT;
    // SAFETY: handle is a connected named-pipe client and mode points to a live
    // value for this call. Collection settings are unchanged.
    if unsafe { SetNamedPipeHandleState(handle, &mode, ptr::null(), ptr::null()) } == 0 {
        return Err(DisplayWorkerClientError::Configure(
            io::Error::last_os_error(),
        ));
    }
    Ok(())
}

fn read_exact(
    handle: HANDLE,
    buffer: &mut [u8],
    deadline: Instant,
) -> Result<(), DisplayWorkerClientError> {
    let mut offset = 0;
    while offset < buffer.len() {
        ensure_before(deadline)?;
        let mut read = 0_u32;
        // SAFETY: handle is live and the remaining bounded slice is writable.
        let result = unsafe {
            ReadFile(
                handle,
                buffer[offset..].as_mut_ptr(),
                u32::try_from(buffer.len() - offset).expect("worker record fits u32"),
                &mut read,
                ptr::null_mut(),
            )
        };
        if result != 0 && read > 0 {
            offset += read as usize;
            continue;
        }
        // SAFETY: GetLastError immediately follows ReadFile.
        if result == 0 && unsafe { GetLastError() } != ERROR_NO_DATA {
            return Err(DisplayWorkerClientError::Read(io::Error::last_os_error()));
        }
        thread::sleep(RETRY_INTERVAL);
    }
    Ok(())
}

fn write_all(
    handle: HANDLE,
    buffer: &[u8],
    deadline: Instant,
) -> Result<(), DisplayWorkerClientError> {
    let mut offset = 0;
    while offset < buffer.len() {
        ensure_before(deadline)?;
        let mut written = 0_u32;
        // SAFETY: handle is live and the remaining bounded slice is readable.
        let result = unsafe {
            WriteFile(
                handle,
                buffer[offset..].as_ptr(),
                u32::try_from(buffer.len() - offset).expect("worker acknowledgement fits u32"),
                &mut written,
                ptr::null_mut(),
            )
        };
        if result != 0 && written > 0 {
            offset += written as usize;
            continue;
        }
        // SAFETY: GetLastError immediately follows WriteFile.
        if result == 0 && unsafe { GetLastError() } != ERROR_NO_DATA {
            return Err(DisplayWorkerClientError::Write(io::Error::last_os_error()));
        }
        thread::sleep(RETRY_INTERVAL);
    }
    Ok(())
}

fn remaining_millis(deadline: Instant) -> Result<u32, DisplayWorkerClientError> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or(DisplayWorkerClientError::Timeout)?;
    Ok(u32::try_from(remaining.as_millis().max(1)).unwrap_or(u32::MAX))
}

fn ensure_before(deadline: Instant) -> Result<(), DisplayWorkerClientError> {
    if Instant::now() < deadline {
        Ok(())
    } else {
        Err(DisplayWorkerClientError::Timeout)
    }
}

struct OwnedPipe(HANDLE);

impl Drop for OwnedPipe {
    fn drop(&mut self) {
        if self.0 != INVALID_HANDLE_VALUE {
            // SAFETY: this guard exclusively owns the successful CreateFileW
            // result.
            unsafe { CloseHandle(self.0) };
            self.0 = INVALID_HANDLE_VALUE;
        }
    }
}

#[derive(Debug)]
pub enum DisplayWorkerClientError {
    InvalidPipeName,
    Open(io::Error),
    Configure(io::Error),
    Read(io::Error),
    Write(io::Error),
    Timeout,
    UnexpectedPostStartData,
    PartialControlRecord,
    ControlSequenceExhausted,
    UnexpectedControlSequence { expected: u32, actual: u32 },
    Protocol(DisplayWorkerProtocolError),
    Configuration(DisplayWorkerConfigurationError),
    Session(DisplaySessionError),
}

impl fmt::Display for DisplayWorkerClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPipeName => formatter.write_str("display worker pipe name is invalid"),
            Self::Open(error) => write!(formatter, "open display worker pipe failed: {error}"),
            Self::Configure(error) => {
                write!(formatter, "configure display worker pipe failed: {error}")
            }
            Self::Read(error) => write!(formatter, "read display worker lease failed: {error}"),
            Self::Write(error) => {
                write!(
                    formatter,
                    "write display worker acknowledgement failed: {error}"
                )
            }
            Self::Timeout => formatter.write_str("display worker lease exchange timed out"),
            Self::UnexpectedPostStartData => {
                formatter.write_str("display worker received unsupported post-start data")
            }
            Self::PartialControlRecord => {
                formatter.write_str("display worker service disconnected during a control record")
            }
            Self::ControlSequenceExhausted => {
                formatter.write_str("display worker control sequence is exhausted")
            }
            Self::UnexpectedControlSequence { expected, actual } => write!(
                formatter,
                "display worker control sequence mismatch: expected {expected}, received {actual}"
            ),
            Self::Protocol(error) => write!(formatter, "display worker protocol failed: {error}"),
            Self::Configuration(error) => {
                write!(formatter, "display worker configuration failed: {error}")
            }
            Self::Session(error) => write!(formatter, "display worker session failed: {error}"),
        }
    }
}

impl std::error::Error for DisplayWorkerClientError {}

impl From<DisplayWorkerProtocolError> for DisplayWorkerClientError {
    fn from(value: DisplayWorkerProtocolError) -> Self {
        Self::Protocol(value)
    }
}

impl From<DisplayWorkerConfigurationError> for DisplayWorkerClientError {
    fn from(value: DisplayWorkerConfigurationError) -> Self {
        Self::Configuration(value)
    }
}

impl From<DisplaySessionError> for DisplayWorkerClientError {
    fn from(value: DisplaySessionError) -> Self {
        Self::Session(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_display_stream_adapter::{
        DisplayMode, DisplayOnlyPolicy, DisplayStreamSessionRequest, DynamicRange,
        ValidatedDisplayStreamSession, VideoCodec, launch_record::GameStreamLaunchBinding,
    };
    use capyio_windows_display::{DisplaySessionContext, InteractiveDisplaySessionId};
    use std::net::Ipv4Addr;

    #[test]
    fn pipe_name_is_closed_to_the_capyio_namespace() {
        let valid = DisplayWorkerPipeName::parse(
            r"\\.\pipe\CapyIO.DisplayWorker.01234567-89ab-cdef-0123-456789abcdef",
        )
        .unwrap();
        assert!(valid.as_str().ends_with("456789abcdef"));
        for invalid in [
            r"\\.\pipe\Other.Worker.x",
            r"C:\temp\worker",
            r"\\.\pipe\CapyIO.DisplayWorker.",
            r"\\.\pipe\CapyIO.DisplayWorker.x\y",
        ] {
            assert!(matches!(
                DisplayWorkerPipeName::parse(invalid),
                Err(DisplayWorkerClientError::InvalidPipeName)
            ));
        }
    }

    #[test]
    fn lease_acknowledgement_requires_this_process_session() {
        let context = DisplaySessionContext::query().unwrap();
        let current = InteractiveDisplaySessionId::new(context.process_session_id()).unwrap();
        let session = ValidatedDisplayStreamSession::new(DisplayStreamSessionRequest {
            adapter_instance_id: [1; 16],
            display_capability_id: [2; 16],
            mode: DisplayMode {
                width: 1920,
                height: 1080,
                refresh_millihertz: 60_000,
            },
            codec: VideoCodec::Hevc,
            dynamic_range: DynamicRange::Sdr8,
            bitrate_kbps: 30_000,
            port_base: 50_000,
            policy: DisplayOnlyPolicy::CAPYIO,
        })
        .unwrap();
        let lease = DisplayWorkerLease::new(
            7,
            current,
            &session,
            GameStreamLaunchBinding {
                session_id: [3; 16],
                route_id: [4; 16],
                stream_id: [5; 16],
                stream_epoch: 6,
                host_ipv4: Ipv4Addr::LOCALHOST,
            },
        )
        .unwrap();
        let record = lease.encode();
        let (decoded, acknowledgement) = acknowledge_display_worker_lease(&record).unwrap();
        assert_eq!(decoded, lease);
        crate::verify_display_worker_lease_ack(&record, &acknowledgement).unwrap();

        let wrong_session = if current.get() == 1 { 2 } else { 1 };
        let wrong = DisplayWorkerLease {
            windows_session_id: InteractiveDisplaySessionId::new(wrong_session).unwrap(),
            ..lease
        }
        .encode();
        assert!(matches!(
            acknowledge_display_worker_lease(&wrong),
            Err(DisplayWorkerClientError::Session(_))
        ));
    }

    #[test]
    fn control_sequence_is_strict_and_cannot_replay_or_skip() {
        assert_eq!(advance_control_sequence(0, 1).unwrap(), 1);
        assert!(matches!(
            advance_control_sequence(1, 1),
            Err(DisplayWorkerClientError::UnexpectedControlSequence {
                expected: 2,
                actual: 1
            })
        ));
        assert!(matches!(
            advance_control_sequence(1, 3),
            Err(DisplayWorkerClientError::UnexpectedControlSequence {
                expected: 2,
                actual: 3
            })
        ));
        assert!(matches!(
            advance_control_sequence(u32::MAX, 1),
            Err(DisplayWorkerClientError::ControlSequenceExhausted)
        ));
    }
}
