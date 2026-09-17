use std::{
    ffi::{OsStr, c_void},
    fmt,
    fs::File,
    io::{self, Read},
    mem,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Path, PathBuf},
    process::Child,
    ptr, thread,
    time::{Duration, Instant},
};

use capyio_display_engine_host::{
    DISPLAY_WORKER_CONFIG_ACK_BYTES, DISPLAY_WORKER_CONFIG_BYTES, DISPLAY_WORKER_CONTROL_ACK_BYTES,
    DISPLAY_WORKER_LEASE_ACK_BYTES, DISPLAY_WORKER_START_ACK_BYTES, DisplayWorkerControlCommand,
    DisplayWorkerControlRecord, DisplayWorkerEngineConfigRecord, DisplayWorkerLease,
    DisplayWorkerPipeName, DisplayWorkerStartAcknowledgement, DisplayWorkerStartRecord,
    verify_display_worker_config_ack, verify_display_worker_control_ack,
    verify_display_worker_lease_ack, verify_display_worker_start_ack,
};
use capyio_windows_display::InteractiveDisplaySessionId;
use sha2::{Digest, Sha256};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_BROKEN_PIPE, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_DATA,
        ERROR_PIPE_CONNECTED, ERROR_PIPE_LISTENING, GENERIC_READ, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE, LocalFree, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SDDL_REVISION_1,
        },
        GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_SHARE_READ,
        OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, SYNCHRONIZE, WriteFile,
    },
    System::{
        Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock},
        Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
            PIPE_NOWAIT, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        },
        RemoteDesktop::{ProcessIdToSessionId, WTSQueryUserToken},
        Threading::{
            CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW, GetProcessId, OpenProcess,
            PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW, TerminateProcess,
            WaitForSingleObject,
        },
    },
};

const WORKER_PIPE_RETRY_INTERVAL: Duration = Duration::from_millis(5);
const MAX_WORKER_PIPE_ACCEPT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_WORKER_PIPE_EXCHANGE_TIMEOUT: Duration = Duration::from_secs(5);
const WORKER_TERMINATION_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_STRING_SID_UNITS: usize = 256;
const LOCAL_SYSTEM_SID: &str = "S-1-5-18";

/// Refuses the privileged production fixture unless its caller is exactly
/// LocalSystem in Session 0. Administrator or interactive execution is not a
/// substitute for the service context.
pub fn verify_local_system_service_context() -> Result<(), DisplayWorkerPipeError> {
    let token = UserToken::current_process()?;
    let sid = token_user_sid(token.0)?;
    if sid != LOCAL_SYSTEM_SID {
        return Err(DisplayWorkerPipeError::CallerIsNotLocalSystem);
    }
    let mut session = u32::MAX;
    // SAFETY: the current PID is live and session points to writable storage.
    if unsafe { ProcessIdToSessionId(std::process::id(), &mut session) } == 0 {
        return Err(DisplayWorkerPipeError::QueryCallerSession(
            io::Error::last_os_error(),
        ));
    }
    if session != 0 {
        return Err(DisplayWorkerPipeError::CallerIsNotSessionZero(session));
    }
    Ok(())
}

/// A single-instance local worker pipe whose DACL admits only LocalSystem and
/// the user represented by the exact authorized interactive Session token.
pub struct AuthorizedDisplayWorkerPipe {
    pipe: File,
}

impl AuthorizedDisplayWorkerPipe {
    pub fn create(
        pipe_name: &DisplayWorkerPipeName,
        authorized_session: InteractiveDisplaySessionId,
    ) -> Result<Self, DisplayWorkerPipeError> {
        let token = UserToken::query(authorized_session).map_err(DisplayWorkerPipeError::Token)?;
        Self::create_for_token(pipe_name, token.0)
    }

    fn create_for_token(
        pipe_name: &DisplayWorkerPipeName,
        token: HANDLE,
    ) -> Result<Self, DisplayWorkerPipeError> {
        let user_sid = token_user_sid(token)?;
        let mut descriptor = WorkerPipeSecurityDescriptor::new(&user_sid)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(mem::size_of::<SECURITY_ATTRIBUTES>())
                .expect("SECURITY_ATTRIBUTES size fits u32"),
            lpSecurityDescriptor: descriptor.as_mut_ptr(),
            bInheritHandle: 0,
        };
        let name =
            wide_null(OsStr::new(pipe_name.as_str())).map_err(DisplayWorkerPipeError::Token)?;
        // SAFETY: name, descriptor and attributes remain live for the call.
        // First-instance and remote-client rejection prevent namespace reuse
        // and network access; the returned handle is immediately owned.
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                u32::try_from(DISPLAY_WORKER_CONFIG_BYTES).expect("worker config fits u32"),
                u32::try_from(DISPLAY_WORKER_LEASE_ACK_BYTES).expect("worker ack fits u32"),
                0,
                &attributes,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(DisplayWorkerPipeError::Create(io::Error::last_os_error()));
        }
        // SAFETY: handle is the unique successful CreateNamedPipeW result.
        let pipe = unsafe { File::from_raw_handle(handle) };
        Ok(Self { pipe })
    }

    /// Waits for the service-spawned worker while disconnecting any same-user
    /// process that reaches the pipe first. Both the wait and retry interval
    /// are fixed and bounded.
    pub fn accept_spawned_worker(
        &self,
        worker: &SpawnedDisplayWorker,
        authorized_session: InteractiveDisplaySessionId,
        timeout: Duration,
    ) -> Result<BoundDisplayWorkerPeer, DisplayWorkerPipeError> {
        if timeout.is_zero() || timeout > MAX_WORKER_PIPE_ACCEPT_TIMEOUT {
            return Err(DisplayWorkerPipeError::InvalidAcceptTimeout);
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(DisplayWorkerPipeError::InvalidAcceptTimeout)?;
        loop {
            if !worker.is_running().map_err(DisplayWorkerPipeError::Token)? {
                return Err(DisplayWorkerPipeError::WorkerExited);
            }
            // SAFETY: pipe owns a live non-overlapped, nonblocking server pipe.
            let connected =
                unsafe { ConnectNamedPipe(self.pipe.as_raw_handle() as HANDLE, ptr::null_mut()) };
            let error = if connected == 0 {
                // SAFETY: GetLastError immediately follows ConnectNamedPipe.
                unsafe { GetLastError() }
            } else {
                ERROR_PIPE_CONNECTED
            };
            if connected != 0 || error == ERROR_PIPE_CONNECTED {
                match BoundDisplayWorkerPeer::bind_connected_pipe(
                    self,
                    worker.expected_process_id(),
                    authorized_session,
                ) {
                    Ok(peer) => return Ok(peer),
                    Err(_) => self.disconnect(),
                }
            } else if error == ERROR_NO_DATA {
                self.disconnect();
            } else if error != ERROR_PIPE_LISTENING {
                return Err(DisplayWorkerPipeError::Connect(io::Error::last_os_error()));
            }
            if Instant::now() >= deadline {
                return Err(DisplayWorkerPipeError::AcceptTimedOut);
            }
            thread::sleep(WORKER_PIPE_RETRY_INTERVAL);
        }
    }

    fn disconnect(&self) {
        // SAFETY: pipe owns the live pipe instance. A not-connected result is
        // harmless after a racing client exit.
        unsafe { DisconnectNamedPipe(self.pipe.as_raw_handle() as HANDLE) };
    }
}

impl AsRawHandle for AuthorizedDisplayWorkerPipe {
    fn as_raw_handle(&self) -> std::os::windows::io::RawHandle {
        self.pipe.as_raw_handle()
    }
}

/// Owns the production pipe, spawned process and independently retained peer
/// identity as one revocable service-side display-worker boundary.
pub struct BoundDisplayWorkerChannel {
    pipe: AuthorizedDisplayWorkerPipe,
    worker: SpawnedDisplayWorker,
    peer: BoundDisplayWorkerPeer,
    state: DisplayWorkerChannelState,
}

#[derive(Clone, Copy)]
enum DisplayWorkerChannelState {
    Bound,
    LeaseAcknowledged(DisplayWorkerLease),
    ConfigurationAcknowledged(DisplayWorkerLease),
    StartAcknowledged {
        lease: DisplayWorkerLease,
        next_control_sequence: u32,
    },
    ControlExchangeFaulted,
    StopAcknowledged,
}

impl BoundDisplayWorkerChannel {
    pub fn launch(
        executable: &VerifiedDisplayWorkerExecutable,
        pipe_name: &DisplayWorkerPipeName,
        authorized_session: InteractiveDisplaySessionId,
        accept_timeout: Duration,
    ) -> Result<Self, DisplayWorkerChannelError> {
        let pipe = AuthorizedDisplayWorkerPipe::create(pipe_name, authorized_session)
            .map_err(DisplayWorkerChannelError::CreatePipe)?;
        let worker = launch_display_worker_in_session(executable, pipe_name, authorized_session)
            .map_err(DisplayWorkerChannelError::Launch)?;
        Self::bind_spawned(pipe, worker, authorized_session, accept_timeout)
    }

    fn bind_spawned(
        pipe: AuthorizedDisplayWorkerPipe,
        mut worker: SpawnedDisplayWorker,
        authorized_session: InteractiveDisplaySessionId,
        accept_timeout: Duration,
    ) -> Result<Self, DisplayWorkerChannelError> {
        match pipe.accept_spawned_worker(&worker, authorized_session, accept_timeout) {
            Ok(peer) => Ok(Self {
                pipe,
                worker,
                peer,
                state: DisplayWorkerChannelState::Bound,
            }),
            Err(accept) => match worker.terminate_and_wait(WORKER_TERMINATION_TIMEOUT) {
                Ok(()) => Err(DisplayWorkerChannelError::Accept(accept)),
                Err(cleanup) => Err(DisplayWorkerChannelError::AcceptCleanup { accept, cleanup }),
            },
        }
    }

    /// Sends the fixed non-secret lease only after process and Session binding,
    /// then verifies the acknowledgement over the complete encoded record.
    pub fn exchange_lease(
        &mut self,
        lease: &DisplayWorkerLease,
        timeout: Duration,
    ) -> Result<(), DisplayWorkerChannelError> {
        if !matches!(self.state, DisplayWorkerChannelState::Bound) {
            return Err(DisplayWorkerChannelError::InvalidExchangeState);
        }
        let deadline = bounded_deadline(timeout, MAX_WORKER_PIPE_EXCHANGE_TIMEOUT)?;
        let record = lease.encode();
        pipe_write_all(self.pipe.as_raw_handle() as HANDLE, &record, deadline)?;
        let mut acknowledgement = [0_u8; DISPLAY_WORKER_LEASE_ACK_BYTES];
        pipe_read_exact(
            self.pipe.as_raw_handle() as HANDLE,
            &mut acknowledgement,
            deadline,
        )?;
        verify_display_worker_lease_ack(&record, &acknowledgement)
            .map_err(|_| DisplayWorkerChannelError::InvalidLeaseAcknowledgement)?;
        self.state = DisplayWorkerChannelState::LeaseAcknowledged(*lease);
        Ok(())
    }

    /// Sends the one-shot secret record only after the exact lease has been
    /// acknowledged and accepts only a response bound to every record byte.
    pub fn exchange_start(
        &mut self,
        record: DisplayWorkerStartRecord,
        timeout: Duration,
    ) -> Result<(), DisplayWorkerChannelError> {
        let DisplayWorkerChannelState::ConfigurationAcknowledged(lease) = self.state else {
            return Err(DisplayWorkerChannelError::InvalidExchangeState);
        };
        record
            .validate_for(&lease)
            .map_err(|_| DisplayWorkerChannelError::InvalidStartRecord)?;
        let deadline = bounded_deadline(timeout, MAX_WORKER_PIPE_EXCHANGE_TIMEOUT)?;
        pipe_write_all(
            self.pipe.as_raw_handle() as HANDLE,
            record.as_bytes(),
            deadline,
        )?;
        let mut acknowledgement_bytes = [0_u8; DISPLAY_WORKER_START_ACK_BYTES];
        pipe_read_exact(
            self.pipe.as_raw_handle() as HANDLE,
            &mut acknowledgement_bytes,
            deadline,
        )?;
        let acknowledgement =
            DisplayWorkerStartAcknowledgement::take_from(&mut acknowledgement_bytes);
        verify_display_worker_start_ack(&record, &lease, &acknowledgement)
            .map_err(|_| DisplayWorkerChannelError::InvalidStartAcknowledgement)?;
        self.state = DisplayWorkerChannelState::StartAcknowledged {
            lease,
            next_control_sequence: 1,
        };
        Ok(())
    }

    /// Requests an immediate encoded recovery point from the running engine.
    /// An ambiguous exchange permanently faults this channel so a command is
    /// never replayed with the same sequence.
    pub fn request_key_frame(
        &mut self,
        timeout: Duration,
    ) -> Result<(), DisplayWorkerChannelError> {
        self.exchange_control(DisplayWorkerControlCommand::RequestKeyFrame, timeout)
    }

    /// Stops the native engine while the authenticated service channel still
    /// exists and waits for the worker's post-quiescence acknowledgement.
    pub fn graceful_stop(&mut self, timeout: Duration) -> Result<(), DisplayWorkerChannelError> {
        self.exchange_control(DisplayWorkerControlCommand::GracefulStop, timeout)?;
        self.state = DisplayWorkerChannelState::StopAcknowledged;
        Ok(())
    }

    fn exchange_control(
        &mut self,
        command: DisplayWorkerControlCommand,
        timeout: Duration,
    ) -> Result<(), DisplayWorkerChannelError> {
        let DisplayWorkerChannelState::StartAcknowledged {
            lease,
            next_control_sequence,
        } = self.state
        else {
            return Err(DisplayWorkerChannelError::InvalidExchangeState);
        };
        let record = DisplayWorkerControlRecord::new(&lease, command, next_control_sequence)
            .map_err(|_| DisplayWorkerChannelError::InvalidControlRecord)?;
        let following_sequence = next_control_sequence
            .checked_add(1)
            .ok_or(DisplayWorkerChannelError::ControlSequenceExhausted)?;
        let deadline = bounded_deadline(timeout, MAX_WORKER_PIPE_EXCHANGE_TIMEOUT)?;

        // From this point, any I/O error is ambiguous. Do not permit a replay.
        self.state = DisplayWorkerChannelState::ControlExchangeFaulted;
        pipe_write_all(
            self.pipe.as_raw_handle() as HANDLE,
            record.as_bytes(),
            deadline,
        )?;
        let mut acknowledgement = [0_u8; DISPLAY_WORKER_CONTROL_ACK_BYTES];
        pipe_read_exact(
            self.pipe.as_raw_handle() as HANDLE,
            &mut acknowledgement,
            deadline,
        )?;
        verify_display_worker_control_ack(&record, &lease, &acknowledgement)
            .map_err(|_| DisplayWorkerChannelError::InvalidControlAcknowledgement)?;
        self.state = DisplayWorkerChannelState::StartAcknowledged {
            lease,
            next_control_sequence: following_sequence,
        };
        Ok(())
    }

    /// Sends the complete non-secret engine policy between lease and secret
    /// phases and binds its acknowledgement to every configuration byte.
    pub fn exchange_configuration(
        &mut self,
        record: &DisplayWorkerEngineConfigRecord,
        timeout: Duration,
    ) -> Result<(), DisplayWorkerChannelError> {
        let DisplayWorkerChannelState::LeaseAcknowledged(lease) = self.state else {
            return Err(DisplayWorkerChannelError::InvalidExchangeState);
        };
        DisplayWorkerEngineConfigRecord::decode(*record.as_bytes(), &lease)
            .map_err(|_| DisplayWorkerChannelError::InvalidEngineConfiguration)?;
        let deadline = bounded_deadline(timeout, MAX_WORKER_PIPE_EXCHANGE_TIMEOUT)?;
        pipe_write_all(
            self.pipe.as_raw_handle() as HANDLE,
            record.as_bytes(),
            deadline,
        )?;
        let mut acknowledgement = [0_u8; DISPLAY_WORKER_CONFIG_ACK_BYTES];
        pipe_read_exact(
            self.pipe.as_raw_handle() as HANDLE,
            &mut acknowledgement,
            deadline,
        )?;
        verify_display_worker_config_ack(record, &lease, &acknowledgement)
            .map_err(|_| DisplayWorkerChannelError::InvalidConfigurationAcknowledgement)?;
        self.state = DisplayWorkerChannelState::ConfigurationAcknowledged(lease);
        Ok(())
    }

    #[must_use]
    pub const fn process_id(&self) -> u32 {
        self.peer.process_id()
    }

    pub fn is_running(&self) -> Result<bool, DisplayWorkerChannelError> {
        let worker_running = self
            .worker
            .is_running()
            .map_err(DisplayWorkerChannelError::Launch)?;
        let peer_running = self
            .peer
            .is_running()
            .map_err(DisplayWorkerChannelError::Peer)?;
        if worker_running != peer_running {
            return Err(DisplayWorkerChannelError::ProcessStateMismatch);
        }
        Ok(worker_running)
    }

    pub fn wait_for_exit(&self, timeout: Duration) -> Result<(), DisplayWorkerChannelError> {
        self.worker
            .wait_for_exit(timeout)
            .map_err(DisplayWorkerChannelError::Launch)
    }

    pub fn terminate_and_wait(&mut self) -> Result<(), DisplayWorkerChannelError> {
        self.worker
            .terminate_and_wait(WORKER_TERMINATION_TIMEOUT)
            .map_err(DisplayWorkerChannelError::Launch)
    }
}

impl Drop for BoundDisplayWorkerChannel {
    fn drop(&mut self) {
        let _ = self.worker.terminate_and_wait(WORKER_TERMINATION_TIMEOUT);
    }
}

fn bounded_deadline(
    timeout: Duration,
    maximum: Duration,
) -> Result<Instant, DisplayWorkerChannelError> {
    if timeout.is_zero() || timeout > maximum {
        return Err(DisplayWorkerChannelError::InvalidExchangeTimeout);
    }
    Instant::now()
        .checked_add(timeout)
        .ok_or(DisplayWorkerChannelError::InvalidExchangeTimeout)
}

fn pipe_write_all(
    pipe: HANDLE,
    buffer: &[u8],
    deadline: Instant,
) -> Result<(), DisplayWorkerChannelError> {
    let mut offset = 0;
    while offset < buffer.len() {
        let mut written = 0_u32;
        // SAFETY: buffer is live for the bounded synchronous write and pipe is
        // owned by the bound channel.
        let result = unsafe {
            WriteFile(
                pipe,
                buffer[offset..].as_ptr(),
                u32::try_from(buffer.len() - offset).expect("worker record fits u32"),
                &mut written,
                ptr::null_mut(),
            )
        };
        if result != 0 && written > 0 {
            offset += written as usize;
            continue;
        }
        // SAFETY: GetLastError immediately follows WriteFile.
        let error = unsafe { GetLastError() };
        if result == 0 && error == ERROR_BROKEN_PIPE {
            return Err(DisplayWorkerChannelError::PipeClosed);
        }
        if Instant::now() >= deadline {
            return Err(DisplayWorkerChannelError::ExchangeTimedOut);
        }
        if result != 0 || error == ERROR_NO_DATA {
            thread::sleep(WORKER_PIPE_RETRY_INTERVAL);
            continue;
        }
        return Err(DisplayWorkerChannelError::Write(io::Error::last_os_error()));
    }
    Ok(())
}

fn pipe_read_exact(
    pipe: HANDLE,
    buffer: &mut [u8],
    deadline: Instant,
) -> Result<(), DisplayWorkerChannelError> {
    let mut offset = 0;
    while offset < buffer.len() {
        let mut read = 0_u32;
        // SAFETY: buffer is live for the bounded synchronous read and pipe is
        // owned by the bound channel.
        let result = unsafe {
            ReadFile(
                pipe,
                buffer[offset..].as_mut_ptr(),
                u32::try_from(buffer.len() - offset).expect("worker ack fits u32"),
                &mut read,
                ptr::null_mut(),
            )
        };
        if result != 0 && read > 0 {
            offset += read as usize;
            continue;
        }
        // SAFETY: GetLastError immediately follows ReadFile.
        let error = unsafe { GetLastError() };
        if result == 0 && error == ERROR_BROKEN_PIPE {
            return Err(DisplayWorkerChannelError::PipeClosed);
        }
        if Instant::now() >= deadline {
            return Err(DisplayWorkerChannelError::ExchangeTimedOut);
        }
        if result != 0 || error == ERROR_NO_DATA {
            thread::sleep(WORKER_PIPE_RETRY_INTERVAL);
            continue;
        }
        return Err(DisplayWorkerChannelError::Read(io::Error::last_os_error()));
    }
    Ok(())
}

pub struct VerifiedDisplayWorkerExecutable {
    canonical_path: PathBuf,
    _locked_file: File,
}

impl VerifiedDisplayWorkerExecutable {
    pub fn open(path: &Path, expected_sha256: [u8; 32]) -> Result<Self, DisplayWorkerLaunchError> {
        validate_worker_path(path)?;
        let canonical_path = path
            .canonicalize()
            .map_err(DisplayWorkerLaunchError::ExecutableFile)?;
        validate_worker_path(&canonical_path)?;
        let wide_path = wide_null(canonical_path.as_os_str())?;
        // SAFETY: the canonical path is NUL-terminated. Share-read permits the
        // image loader while denying replacement/write/delete until spawn ends.
        let handle = unsafe {
            CreateFileW(
                wide_path.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ,
                ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(DisplayWorkerLaunchError::LockExecutable(
                io::Error::last_os_error(),
            ));
        }
        // SAFETY: handle is the unique successful CreateFileW result.
        let mut locked_file = unsafe { File::from_raw_handle(handle) };
        if hash_reader(&mut locked_file)? != expected_sha256 {
            return Err(DisplayWorkerLaunchError::HashMismatch);
        }
        Ok(Self {
            canonical_path,
            _locked_file: locked_file,
        })
    }

    #[must_use]
    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }
}

pub struct SpawnedDisplayWorker {
    process_handle: HANDLE,
    process_id: u32,
}

impl SpawnedDisplayWorker {
    #[must_use]
    pub const fn expected_process_id(&self) -> ExpectedDisplayWorkerProcessId {
        ExpectedDisplayWorkerProcessId(self.process_id)
    }

    pub fn is_running(&self) -> Result<bool, DisplayWorkerLaunchError> {
        // SAFETY: this object owns the process handle and a zero timeout does
        // not block the service loop.
        match unsafe { WaitForSingleObject(self.process_handle, 0) } {
            WAIT_TIMEOUT => Ok(true),
            WAIT_OBJECT_0 => Ok(false),
            WAIT_FAILED => Err(DisplayWorkerLaunchError::QueryProcessState(
                io::Error::last_os_error(),
            )),
            status => Err(DisplayWorkerLaunchError::UnexpectedWaitStatus(status)),
        }
    }

    pub fn wait_for_exit(&self, timeout: Duration) -> Result<(), DisplayWorkerLaunchError> {
        let milliseconds = u32::try_from(timeout.as_millis())
            .map_err(|_| DisplayWorkerLaunchError::InvalidWaitTimeout)?;
        // SAFETY: this object owns the process handle and the caller supplied a
        // finite timeout representable by the Windows API.
        match unsafe { WaitForSingleObject(self.process_handle, milliseconds) } {
            WAIT_OBJECT_0 => Ok(()),
            WAIT_TIMEOUT => Err(DisplayWorkerLaunchError::WaitTimedOut),
            WAIT_FAILED => Err(DisplayWorkerLaunchError::QueryProcessState(
                io::Error::last_os_error(),
            )),
            status => Err(DisplayWorkerLaunchError::UnexpectedWaitStatus(status)),
        }
    }

    pub fn terminate_and_wait(
        &mut self,
        timeout: Duration,
    ) -> Result<(), DisplayWorkerLaunchError> {
        if !self.is_running()? {
            return Ok(());
        }
        // SAFETY: this object owns the exact CreateProcessAsUserW process
        // handle. Termination is used only for bounded service cleanup.
        if unsafe { TerminateProcess(self.process_handle, 1) } == 0 && self.is_running()? {
            return Err(DisplayWorkerLaunchError::TerminateProcess(
                io::Error::last_os_error(),
            ));
        }
        self.wait_for_exit(timeout)
    }

    #[cfg(test)]
    fn retain_for_test(process_id: u32) -> Result<Self, DisplayWorkerLaunchError> {
        // SAFETY: the test owns the child PID and requests only supervision and
        // exact cleanup rights with handle inheritance disabled.
        let process_handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION
                    | SYNCHRONIZE
                    | windows_sys::Win32::System::Threading::PROCESS_TERMINATE,
                0,
                process_id,
            )
        };
        if process_handle.is_null() {
            return Err(DisplayWorkerLaunchError::RetainProcess(
                io::Error::last_os_error(),
            ));
        }
        Ok(Self {
            process_handle,
            process_id,
        })
    }
}

impl Drop for SpawnedDisplayWorker {
    fn drop(&mut self) {
        if !self.process_handle.is_null() {
            // SAFETY: this object exclusively owns the CreateProcessAsUserW
            // process handle.
            unsafe { CloseHandle(self.process_handle) };
            self.process_handle = ptr::null_mut();
        }
    }
}

/// Starts the hash-locked CapyIO worker in the exact authorized user Session.
/// This function is intended only for the LocalSystem service boundary.
pub fn launch_display_worker_in_session(
    executable: &VerifiedDisplayWorkerExecutable,
    pipe_name: &DisplayWorkerPipeName,
    authorized_session: InteractiveDisplaySessionId,
) -> Result<SpawnedDisplayWorker, DisplayWorkerLaunchError> {
    let token = UserToken::query(authorized_session)?;
    let environment = UserEnvironment::create(token.0)?;
    let application = wide_null(executable.canonical_path.as_os_str())?;
    let mut command_line = worker_command_line(executable.canonical_path(), pipe_name)?;
    let mut desktop = wide_null(OsStr::new(r"winsta0\default"))?;
    let current_directory = executable
        .canonical_path
        .parent()
        .ok_or(DisplayWorkerLaunchError::InvalidExecutablePath)?;
    let current_directory = wide_null(current_directory.as_os_str())?;
    let startup = STARTUPINFOW {
        cb: u32::try_from(mem::size_of::<STARTUPINFOW>()).expect("STARTUPINFOW size fits u32"),
        lpDesktop: desktop.as_mut_ptr(),
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    // SAFETY: token and environment are owned guards; all strings are live and
    // NUL-terminated; command_line is writable as required; no handles inherit.
    let created = unsafe {
        CreateProcessAsUserW(
            token.0,
            application.as_ptr(),
            command_line.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            CREATE_UNICODE_ENVIRONMENT,
            environment.0,
            current_directory.as_ptr(),
            &startup,
            &mut process,
        )
    };
    if created == 0 {
        return Err(DisplayWorkerLaunchError::CreateProcess(
            io::Error::last_os_error(),
        ));
    }
    // The initial thread is not used; the process handle is retained for exact
    // PID binding and supervision.
    // SAFETY: both handles are successful CreateProcessAsUserW outputs.
    unsafe { CloseHandle(process.hThread) };
    if process.hProcess.is_null() || process.dwProcessId == 0 {
        if !process.hProcess.is_null() {
            // SAFETY: process.hProcess is still owned on this invalid result.
            unsafe { CloseHandle(process.hProcess) };
        }
        return Err(DisplayWorkerLaunchError::InvalidCreatedProcess);
    }
    Ok(SpawnedDisplayWorker {
        process_handle: process.hProcess,
        process_id: process.dwProcessId,
    })
}

fn token_user_sid(token: HANDLE) -> Result<String, DisplayWorkerPipeError> {
    let mut required = 0_u32;
    // SAFETY: the first call intentionally supplies no buffer to obtain the
    // exact required byte count for TOKEN_USER and its referenced SID.
    let sized = unsafe { GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut required) };
    if sized != 0
        || required < u32::try_from(mem::size_of::<TOKEN_USER>()).expect("TOKEN_USER size fits u32")
    {
        return Err(DisplayWorkerPipeError::QueryTokenUser(
            io::Error::last_os_error(),
        ));
    }
    // SAFETY: GetLastError immediately follows GetTokenInformation.
    if unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
        return Err(DisplayWorkerPipeError::QueryTokenUser(
            io::Error::last_os_error(),
        ));
    }
    let word_size = mem::size_of::<usize>();
    let words = usize::try_from(required)
        .expect("token buffer length fits usize")
        .div_ceil(word_size);
    let mut buffer = vec![0_usize; words];
    let mut returned = required;
    // SAFETY: the usize allocation is sufficiently aligned and spans at least
    // required bytes; returned points to writable size storage.
    if unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            required,
            &mut returned,
        )
    } == 0
        || returned > required
    {
        return Err(DisplayWorkerPipeError::QueryTokenUser(
            io::Error::last_os_error(),
        ));
    }
    // SAFETY: successful TokenUser output begins with a fully initialized
    // TOKEN_USER whose SID remains inside the live buffer.
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    if sid.is_null() {
        return Err(DisplayWorkerPipeError::InvalidUserSid);
    }
    let mut string_sid = ptr::null_mut();
    // SAFETY: sid is backed by the live token-information buffer and Windows
    // allocates string_sid for release by LocalFree.
    if unsafe { ConvertSidToStringSidW(sid, &mut string_sid) } == 0 {
        return Err(DisplayWorkerPipeError::ConvertUserSid(
            io::Error::last_os_error(),
        ));
    }
    let string_sid = LocalWideString(string_sid);
    let units = (0..MAX_STRING_SID_UNITS)
        .position(|index| {
            // SAFETY: ConvertSidToStringSidW returns a NUL-terminated SID; the
            // fixed upper bound prevents an unbounded scan on malformed data.
            unsafe { *string_sid.0.add(index) == 0 }
        })
        .ok_or(DisplayWorkerPipeError::InvalidUserSid)?;
    // SAFETY: the scan above proved units initialized UTF-16 code units before
    // the terminator in the LocalAlloc buffer.
    let wide = unsafe { std::slice::from_raw_parts(string_sid.0, units) };
    let value = String::from_utf16(wide).map_err(|_| DisplayWorkerPipeError::InvalidUserSid)?;
    validate_string_sid(&value)?;
    Ok(value)
}

fn validate_string_sid(value: &str) -> Result<(), DisplayWorkerPipeError> {
    let mut parts = value.split('-');
    let valid = value.len() < MAX_STRING_SID_UNITS
        && parts.next() == Some("S")
        && parts.next() == Some("1")
        && {
            let numeric = parts.collect::<Vec<_>>();
            numeric.len() >= 2
                && numeric
                    .iter()
                    .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        };
    if !valid {
        return Err(DisplayWorkerPipeError::InvalidUserSid);
    }
    Ok(())
}

struct LocalWideString(*mut u16);

impl Drop for LocalWideString {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this guard owns the ConvertSidToStringSidW allocation.
            unsafe { LocalFree(self.0.cast()) };
        }
    }
}

struct WorkerPipeSecurityDescriptor(PSECURITY_DESCRIPTOR);

impl WorkerPipeSecurityDescriptor {
    fn new(user_sid: &str) -> Result<Self, DisplayWorkerPipeError> {
        validate_string_sid(user_sid)?;
        let sddl = format!("D:P(A;;GA;;;SY)(A;;GA;;;{user_sid})");
        let sddl = wide_null(OsStr::new(&sddl)).map_err(DisplayWorkerPipeError::Token)?;
        let mut descriptor = ptr::null_mut();
        // SAFETY: SDDL is immutable and NUL-terminated; Windows allocates the
        // self-relative descriptor returned through descriptor.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(DisplayWorkerPipeError::CreateAcl(io::Error::last_os_error()));
        }
        Ok(Self(descriptor))
    }

    fn as_mut_ptr(&mut self) -> *mut c_void {
        self.0.cast()
    }
}

impl Drop for WorkerPipeSecurityDescriptor {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this guard owns the converted security descriptor.
            unsafe { LocalFree(self.0.cast()) };
        }
    }
}

struct UserToken(HANDLE);

impl UserToken {
    fn query(session: InteractiveDisplaySessionId) -> Result<Self, DisplayWorkerLaunchError> {
        let mut token = ptr::null_mut();
        // SAFETY: the Session ID is validated interactive and token points to
        // writable handle storage. LocalSystem/SE_TCB is enforced by Windows.
        if unsafe { WTSQueryUserToken(session.get(), &mut token) } == 0 {
            return Err(DisplayWorkerLaunchError::QueryUserToken(
                io::Error::last_os_error(),
            ));
        }
        Ok(Self(token))
    }

    fn current_process() -> Result<Self, DisplayWorkerPipeError> {
        let mut token = ptr::null_mut();
        // SAFETY: GetCurrentProcess is a valid pseudo-handle and token points
        // to writable storage for a non-inheritable TOKEN_QUERY handle.
        if unsafe {
            windows_sys::Win32::System::Threading::OpenProcessToken(
                windows_sys::Win32::System::Threading::GetCurrentProcess(),
                windows_sys::Win32::Security::TOKEN_QUERY,
                &mut token,
            )
        } == 0
        {
            return Err(DisplayWorkerPipeError::OpenCurrentToken(
                io::Error::last_os_error(),
            ));
        }
        Ok(Self(token))
    }
}

impl Drop for UserToken {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this guard exclusively owns the WTSQueryUserToken result.
            unsafe { CloseHandle(self.0) };
        }
    }
}

struct UserEnvironment(*mut c_void);

impl UserEnvironment {
    fn create(token: HANDLE) -> Result<Self, DisplayWorkerLaunchError> {
        let mut environment = ptr::null_mut();
        // SAFETY: token is a live primary user token and environment points to
        // writable pointer storage. Service environment inheritance is disabled.
        if unsafe { CreateEnvironmentBlock(&mut environment, token, 0) } == 0 {
            return Err(DisplayWorkerLaunchError::CreateEnvironment(
                io::Error::last_os_error(),
            ));
        }
        Ok(Self(environment))
    }
}

impl Drop for UserEnvironment {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this guard exclusively owns the CreateEnvironmentBlock
            // result and the child has copied it before this guard is dropped.
            unsafe { DestroyEnvironmentBlock(self.0) };
        }
    }
}

fn validate_worker_path(path: &Path) -> Result<(), DisplayWorkerLaunchError> {
    if !path.is_absolute() || path.extension().and_then(OsStr::to_str) != Some("exe") {
        return Err(DisplayWorkerLaunchError::InvalidExecutablePath);
    }
    let text = path.as_os_str().to_string_lossy();
    let extended_unc = text
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(r"\\?\UNC\"));
    let ordinary_unc = text.starts_with(r"\\") && !text.starts_with(r"\\?\");
    if ordinary_unc || extended_unc || text.starts_with("//") {
        return Err(DisplayWorkerLaunchError::NetworkPathRejected);
    }
    Ok(())
}

fn hash_reader(reader: &mut File) -> Result<[u8; 32], DisplayWorkerLaunchError> {
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(DisplayWorkerLaunchError::ExecutableFile)?;
        if count == 0 {
            return Ok(digest.finalize().into());
        }
        digest.update(&buffer[..count]);
    }
}

fn wide_null(value: &OsStr) -> Result<Vec<u16>, DisplayWorkerLaunchError> {
    let mut wide = value.encode_wide().collect::<Vec<_>>();
    if wide.contains(&0) {
        return Err(DisplayWorkerLaunchError::InteriorNul);
    }
    wide.push(0);
    Ok(wide)
}

fn worker_command_line(
    executable: &Path,
    pipe_name: &DisplayWorkerPipeName,
) -> Result<Vec<u16>, DisplayWorkerLaunchError> {
    let mut command = Vec::new();
    command.push(b'"' as u16);
    command.extend(executable.as_os_str().encode_wide());
    command.extend(OsStr::new(r#"" --lease-pipe ""#).encode_wide());
    command.extend(OsStr::new(pipe_name.as_str()).encode_wide());
    command.push(b'"' as u16);
    if command.contains(&0) {
        return Err(DisplayWorkerLaunchError::InteriorNul);
    }
    command.push(0);
    Ok(command)
}

#[derive(Debug)]
pub enum DisplayWorkerPipeError {
    Token(DisplayWorkerLaunchError),
    OpenCurrentToken(io::Error),
    CallerIsNotLocalSystem,
    QueryCallerSession(io::Error),
    CallerIsNotSessionZero(u32),
    QueryTokenUser(io::Error),
    ConvertUserSid(io::Error),
    InvalidUserSid,
    CreateAcl(io::Error),
    Create(io::Error),
    InvalidAcceptTimeout,
    Connect(io::Error),
    WorkerExited,
    AcceptTimedOut,
}

impl fmt::Display for DisplayWorkerPipeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Token(error) => {
                write!(formatter, "display worker token boundary failed: {error}")
            }
            Self::OpenCurrentToken(error) => {
                write!(formatter, "could not open current process token: {error}")
            }
            Self::CallerIsNotLocalSystem => {
                formatter.write_str("display worker fixture caller is not LocalSystem")
            }
            Self::QueryCallerSession(error) => {
                write!(formatter, "could not query fixture caller Session: {error}")
            }
            Self::CallerIsNotSessionZero(session) => write!(
                formatter,
                "display worker fixture caller is in Session {session}, expected Session 0"
            ),
            Self::QueryTokenUser(error) => {
                write!(
                    formatter,
                    "could not query display worker user SID: {error}"
                )
            }
            Self::ConvertUserSid(error) => {
                write!(
                    formatter,
                    "could not convert display worker user SID: {error}"
                )
            }
            Self::InvalidUserSid => formatter.write_str("display worker user SID is invalid"),
            Self::CreateAcl(error) => {
                write!(
                    formatter,
                    "could not create display worker pipe ACL: {error}"
                )
            }
            Self::Create(error) => {
                write!(formatter, "could not create display worker pipe: {error}")
            }
            Self::InvalidAcceptTimeout => {
                formatter.write_str("display worker pipe accept timeout is outside bounds")
            }
            Self::Connect(error) => {
                write!(formatter, "could not connect display worker pipe: {error}")
            }
            Self::WorkerExited => {
                formatter.write_str("spawned display worker exited before pipe binding")
            }
            Self::AcceptTimedOut => formatter.write_str("display worker pipe accept timed out"),
        }
    }
}

impl std::error::Error for DisplayWorkerPipeError {}

#[derive(Debug)]
pub enum DisplayWorkerChannelError {
    CreatePipe(DisplayWorkerPipeError),
    Launch(DisplayWorkerLaunchError),
    Accept(DisplayWorkerPipeError),
    AcceptCleanup {
        accept: DisplayWorkerPipeError,
        cleanup: DisplayWorkerLaunchError,
    },
    Peer(DisplayWorkerPeerError),
    InvalidExchangeTimeout,
    Write(io::Error),
    Read(io::Error),
    PipeClosed,
    ExchangeTimedOut,
    InvalidLeaseAcknowledgement,
    InvalidEngineConfiguration,
    InvalidConfigurationAcknowledgement,
    InvalidStartRecord,
    InvalidStartAcknowledgement,
    InvalidControlRecord,
    InvalidControlAcknowledgement,
    ControlSequenceExhausted,
    InvalidExchangeState,
    ProcessStateMismatch,
}

impl fmt::Display for DisplayWorkerChannelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreatePipe(error) => write!(formatter, "create worker pipe: {error}"),
            Self::Launch(error) => write!(formatter, "launch display worker: {error}"),
            Self::Accept(error) => write!(formatter, "accept display worker: {error}"),
            Self::AcceptCleanup { accept, cleanup } => write!(
                formatter,
                "accept display worker failed ({accept}); child cleanup also failed ({cleanup})"
            ),
            Self::Peer(error) => write!(formatter, "display worker peer failed: {error}"),
            Self::InvalidExchangeTimeout => {
                formatter.write_str("display worker exchange timeout is outside bounds")
            }
            Self::Write(error) => write!(formatter, "write display worker lease: {error}"),
            Self::Read(error) => write!(formatter, "read display worker acknowledgement: {error}"),
            Self::PipeClosed => formatter.write_str("display worker pipe closed during exchange"),
            Self::ExchangeTimedOut => formatter.write_str("display worker exchange timed out"),
            Self::InvalidLeaseAcknowledgement => {
                formatter.write_str("display worker lease acknowledgement is invalid")
            }
            Self::InvalidEngineConfiguration => {
                formatter.write_str("display worker engine configuration is invalid")
            }
            Self::InvalidConfigurationAcknowledgement => formatter
                .write_str("display worker engine configuration acknowledgement is invalid"),
            Self::InvalidStartRecord => {
                formatter.write_str("display worker secret start record is invalid")
            }
            Self::InvalidStartAcknowledgement => {
                formatter.write_str("display worker secret start acknowledgement is invalid")
            }
            Self::InvalidControlRecord => {
                formatter.write_str("display worker control record is invalid")
            }
            Self::InvalidControlAcknowledgement => {
                formatter.write_str("display worker control acknowledgement is invalid")
            }
            Self::ControlSequenceExhausted => {
                formatter.write_str("display worker control sequence is exhausted")
            }
            Self::InvalidExchangeState => {
                formatter.write_str("display worker exchange is out of order or repeated")
            }
            Self::ProcessStateMismatch => {
                formatter.write_str("display worker retained handles disagree on process state")
            }
        }
    }
}

impl std::error::Error for DisplayWorkerChannelError {}

#[derive(Debug)]
pub enum DisplayWorkerLaunchError {
    InvalidExecutablePath,
    NetworkPathRejected,
    InteriorNul,
    ExecutableFile(io::Error),
    LockExecutable(io::Error),
    HashMismatch,
    QueryUserToken(io::Error),
    CreateEnvironment(io::Error),
    CreateProcess(io::Error),
    InvalidCreatedProcess,
    RetainProcess(io::Error),
    TerminateProcess(io::Error),
    InvalidWaitTimeout,
    WaitTimedOut,
    QueryProcessState(io::Error),
    UnexpectedWaitStatus(u32),
}

impl fmt::Display for DisplayWorkerLaunchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidExecutablePath => formatter.write_str(
                "display worker must be an absolute local path with a lowercase .exe extension",
            ),
            Self::NetworkPathRejected => {
                formatter.write_str("network display worker paths are rejected")
            }
            Self::InteriorNul => formatter.write_str("display worker launch value contains NUL"),
            Self::ExecutableFile(error) => write!(formatter, "display worker file error: {error}"),
            Self::LockExecutable(error) => {
                write!(formatter, "could not lock display worker image: {error}")
            }
            Self::HashMismatch => formatter.write_str("display worker SHA-256 mismatch"),
            Self::QueryUserToken(error) => {
                write!(
                    formatter,
                    "could not query authorized Session token: {error}"
                )
            }
            Self::CreateEnvironment(error) => {
                write!(
                    formatter,
                    "could not create display worker environment: {error}"
                )
            }
            Self::CreateProcess(error) => {
                write!(
                    formatter,
                    "could not create display worker process: {error}"
                )
            }
            Self::InvalidCreatedProcess => {
                formatter.write_str("created display worker identity is invalid")
            }
            Self::RetainProcess(error) => {
                write!(
                    formatter,
                    "could not retain display worker process: {error}"
                )
            }
            Self::TerminateProcess(error) => {
                write!(
                    formatter,
                    "could not terminate display worker process: {error}"
                )
            }
            Self::InvalidWaitTimeout => {
                formatter.write_str("display worker wait timeout is outside Windows bounds")
            }
            Self::WaitTimedOut => formatter.write_str("display worker exit wait timed out"),
            Self::QueryProcessState(error) => {
                write!(formatter, "could not query spawned display worker: {error}")
            }
            Self::UnexpectedWaitStatus(status) => {
                write!(
                    formatter,
                    "spawned display worker returned wait status {status}"
                )
            }
        }
    }
}

impl std::error::Error for DisplayWorkerLaunchError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpectedDisplayWorkerProcessId(u32);

impl ExpectedDisplayWorkerProcessId {
    /// Captures the PID from the exact child process owned by the service.
    pub fn from_spawned_child(child: &Child) -> Result<Self, DisplayWorkerPeerError> {
        Self::new(child.id())
    }

    fn new(process_id: u32) -> Result<Self, DisplayWorkerPeerError> {
        if process_id == 0 {
            return Err(DisplayWorkerPeerError::InvalidProcessId);
        }
        Ok(Self(process_id))
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

pub struct BoundDisplayWorkerPeer {
    process_handle: HANDLE,
    process_id: u32,
    session_id: InteractiveDisplaySessionId,
}

impl BoundDisplayWorkerPeer {
    /// Binds a connected local named-pipe client to its kernel-reported process
    /// and the exact interactive session authorized for this display Route.
    pub fn bind_connected_pipe(
        pipe: &impl AsRawHandle,
        expected_process: ExpectedDisplayWorkerProcessId,
        authorized_session: InteractiveDisplaySessionId,
    ) -> Result<Self, DisplayWorkerPeerError> {
        let pipe_handle = pipe.as_raw_handle() as HANDLE;
        if pipe_handle.is_null() || pipe_handle == INVALID_HANDLE_VALUE {
            return Err(DisplayWorkerPeerError::InvalidPipeHandle);
        }
        let mut client_process_id = 0_u32;
        // SAFETY: pipe_handle is required to be a connected named-pipe server
        // handle and the output points to writable u32 storage.
        if unsafe { GetNamedPipeClientProcessId(pipe_handle, &mut client_process_id) } == 0 {
            return Err(DisplayWorkerPeerError::QueryPipeClient(
                io::Error::last_os_error(),
            ));
        }
        if client_process_id == 0 {
            return Err(DisplayWorkerPeerError::InvalidProcessId);
        }
        if client_process_id != expected_process.get() {
            return Err(DisplayWorkerPeerError::UnexpectedProcess {
                expected: expected_process.get(),
                actual: client_process_id,
            });
        }
        // SAFETY: access is limited to query and wait/synchronize, handle
        // inheritance is disabled and the PID came from the kernel pipe endpoint.
        let process_handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
                0,
                client_process_id,
            )
        };
        if process_handle.is_null() {
            return Err(DisplayWorkerPeerError::OpenClientProcess(
                io::Error::last_os_error(),
            ));
        }
        let guard = ProcessHandle(process_handle);
        // SAFETY: guard owns a live process handle opened above.
        let stable_process_id = unsafe { GetProcessId(guard.0) };
        if stable_process_id != client_process_id {
            return Err(DisplayWorkerPeerError::ProcessIdentityChanged);
        }
        let mut session_id = u32::MAX;
        // SAFETY: the PID is held live through guard and the output points to
        // writable u32 storage.
        if unsafe { ProcessIdToSessionId(stable_process_id, &mut session_id) } == 0 {
            return Err(DisplayWorkerPeerError::QueryClientSession(
                io::Error::last_os_error(),
            ));
        }
        validate_peer_session(session_id, authorized_session)?;
        let process_handle = guard.0;
        std::mem::forget(guard);
        Ok(Self {
            process_handle,
            process_id: stable_process_id,
            session_id: authorized_session,
        })
    }

    #[must_use]
    pub const fn process_id(&self) -> u32 {
        self.process_id
    }

    #[must_use]
    pub const fn session_id(&self) -> InteractiveDisplaySessionId {
        self.session_id
    }

    /// Polls the retained worker process without blocking the service loop.
    pub fn is_running(&self) -> Result<bool, DisplayWorkerPeerError> {
        // SAFETY: this object owns a live query/synchronize process handle and
        // a zero timeout performs a nonblocking state observation.
        match unsafe { WaitForSingleObject(self.process_handle, 0) } {
            WAIT_TIMEOUT => Ok(true),
            WAIT_OBJECT_0 => Ok(false),
            WAIT_FAILED => Err(DisplayWorkerPeerError::QueryProcessState(
                io::Error::last_os_error(),
            )),
            status => Err(DisplayWorkerPeerError::UnexpectedWaitStatus(status)),
        }
    }
}

impl Drop for BoundDisplayWorkerPeer {
    fn drop(&mut self) {
        if !self.process_handle.is_null() {
            // SAFETY: this object exclusively owns the process handle.
            unsafe { CloseHandle(self.process_handle) };
            self.process_handle = ptr::null_mut();
        }
    }
}

struct ProcessHandle(HANDLE);

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the guard exclusively owns the successful OpenProcess
            // result.
            unsafe { CloseHandle(self.0) };
        }
    }
}

fn validate_peer_session(
    actual_session: u32,
    authorized_session: InteractiveDisplaySessionId,
) -> Result<(), DisplayWorkerPeerError> {
    let actual = InteractiveDisplaySessionId::new(actual_session)
        .map_err(|_| DisplayWorkerPeerError::NonInteractiveSession(actual_session))?;
    if actual != authorized_session {
        return Err(DisplayWorkerPeerError::SessionMismatch {
            expected: authorized_session.get(),
            actual: actual.get(),
        });
    }
    Ok(())
}

#[derive(Debug)]
pub enum DisplayWorkerPeerError {
    InvalidPipeHandle,
    QueryPipeClient(io::Error),
    InvalidProcessId,
    UnexpectedProcess { expected: u32, actual: u32 },
    OpenClientProcess(io::Error),
    ProcessIdentityChanged,
    QueryClientSession(io::Error),
    NonInteractiveSession(u32),
    SessionMismatch { expected: u32, actual: u32 },
    QueryProcessState(io::Error),
    UnexpectedWaitStatus(u32),
}

impl fmt::Display for DisplayWorkerPeerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPipeHandle => formatter.write_str("display worker pipe handle is invalid"),
            Self::QueryPipeClient(error) => {
                write!(
                    formatter,
                    "could not query display worker pipe client: {error}"
                )
            }
            Self::InvalidProcessId => formatter.write_str("display worker process ID is invalid"),
            Self::UnexpectedProcess { expected, actual } => write!(
                formatter,
                "display worker pipe client PID is {actual}, expected spawned PID {expected}"
            ),
            Self::OpenClientProcess(error) => {
                write!(
                    formatter,
                    "could not retain display worker process: {error}"
                )
            }
            Self::ProcessIdentityChanged => {
                formatter.write_str("display worker process identity changed")
            }
            Self::QueryClientSession(error) => {
                write!(formatter, "could not query display worker session: {error}")
            }
            Self::NonInteractiveSession(session) => {
                write!(
                    formatter,
                    "display worker session {session} is not interactive"
                )
            }
            Self::SessionMismatch { expected, actual } => write!(
                formatter,
                "display worker is in session {actual}, expected authorized session {expected}"
            ),
            Self::QueryProcessState(error) => {
                write!(
                    formatter,
                    "could not query display worker process state: {error}"
                )
            }
            Self::UnexpectedWaitStatus(status) => {
                write!(
                    formatter,
                    "display worker returned unexpected wait status {status}"
                )
            }
        }
    }
}

impl std::error::Error for DisplayWorkerPeerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_display_engine_host::{
        AuthenticatedEngineLaunch, DisplayWorkerControlCommand, DisplayWorkerControlEvent,
        connect_display_worker_start_client,
    };
    use capyio_display_stream_adapter::{
        DisplayMode, DisplayOnlyPolicy, DisplayStreamSessionRequest, DynamicRange,
        ValidatedDisplayStreamSession, VideoCodec, launch_record::GameStreamLaunchBinding,
    };
    use capyio_windows_display::DisplaySessionContext;
    use std::{
        fs::{File, OpenOptions},
        os::windows::io::FromRawHandle,
        process::{Command, Stdio},
        sync::mpsc,
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };
    use windows_sys::Win32::{
        Foundation::{ERROR_PIPE_CONNECTED, GetLastError},
        Storage::FileSystem::PIPE_ACCESS_DUPLEX,
        System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_TYPE_BYTE, PIPE_WAIT,
        },
    };

    #[test]
    fn peer_session_requires_an_exact_interactive_match() {
        let authorized = InteractiveDisplaySessionId::new(7).unwrap();
        validate_peer_session(7, authorized).unwrap();
        assert!(matches!(
            validate_peer_session(0, authorized),
            Err(DisplayWorkerPeerError::NonInteractiveSession(0))
        ));
        assert!(matches!(
            validate_peer_session(8, authorized),
            Err(DisplayWorkerPeerError::SessionMismatch {
                expected: 7,
                actual: 8
            })
        ));
    }

    #[test]
    fn expected_worker_pid_is_nonzero_and_exact() {
        assert!(matches!(
            ExpectedDisplayWorkerProcessId::new(0),
            Err(DisplayWorkerPeerError::InvalidProcessId)
        ));
        let expected = ExpectedDisplayWorkerProcessId::new(7).unwrap();
        assert_eq!(expected.get(), 7);
    }

    #[test]
    fn worker_image_is_absolute_local_hash_locked_and_command_is_closed() {
        let executable = std::env::current_exe().unwrap();
        let expected_hash = {
            let mut file = File::open(&executable).unwrap();
            hash_reader(&mut file).unwrap()
        };
        let verified = VerifiedDisplayWorkerExecutable::open(&executable, expected_hash).unwrap();
        assert!(verified.canonical_path().is_absolute());
        assert!(matches!(
            VerifiedDisplayWorkerExecutable::open(&executable, [0; 32]),
            Err(DisplayWorkerLaunchError::HashMismatch)
        ));
        assert!(matches!(
            VerifiedDisplayWorkerExecutable::open(Path::new("worker.exe"), expected_hash),
            Err(DisplayWorkerLaunchError::InvalidExecutablePath)
        ));
        assert!(matches!(
            VerifiedDisplayWorkerExecutable::open(
                Path::new(r"\\server\share\worker.exe"),
                expected_hash
            ),
            Err(DisplayWorkerLaunchError::NetworkPathRejected)
        ));

        let pipe = DisplayWorkerPipeName::parse(
            r"\\.\pipe\CapyIO.DisplayWorker.01234567-89ab-cdef-0123-456789abcdef",
        )
        .unwrap();
        let command = worker_command_line(verified.canonical_path(), &pipe).unwrap();
        assert_eq!(command.last(), Some(&0));
        assert!(!command[..command.len() - 1].contains(&0));
        let text = String::from_utf16(&command[..command.len() - 1]).unwrap();
        assert!(text.starts_with('"'));
        assert!(text.contains(r#"" --lease-pipe "\\.\pipe\CapyIO.DisplayWorker."#));
    }

    #[test]
    fn authorized_pipe_uses_exact_user_sid_first_instance_and_pid_binding() {
        assert!(validate_string_sid("S-1-5-21-1-2-3-1001").is_ok());
        assert!(validate_string_sid("S-1-5-21-bad").is_err());
        assert!(validate_string_sid("BA").is_err());

        let token = UserToken::current_process().unwrap();
        let user_sid = token_user_sid(token.0).unwrap();
        validate_string_sid(&user_sid).unwrap();
        WorkerPipeSecurityDescriptor::new(&user_sid).unwrap();

        let pipe_name = DisplayWorkerPipeName::parse(&format!(
            r"\\.\pipe\CapyIO.DisplayWorker.Acl.{}",
            std::process::id()
        ))
        .unwrap();
        let pipe = AuthorizedDisplayWorkerPipe::create_for_token(&pipe_name, token.0).unwrap();
        assert!(matches!(
            AuthorizedDisplayWorkerPipe::create_for_token(&pipe_name, token.0),
            Err(DisplayWorkerPipeError::Create(_))
        ));

        let client_name = pipe_name.as_str().to_owned();
        let client = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if let Ok(file) = OpenOptions::new().read(true).write(true).open(&client_name) {
                    return file;
                }
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(5));
            }
        });
        // SAFETY: access is limited to query/wait, inheritance is disabled and
        // this test transfers the resulting current-process handle to worker.
        let process_handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
                0,
                std::process::id(),
            )
        };
        assert!(!process_handle.is_null());
        let worker = SpawnedDisplayWorker {
            process_handle,
            process_id: std::process::id(),
        };
        let context = DisplaySessionContext::query().unwrap();
        let authorized = InteractiveDisplaySessionId::new(context.process_session_id()).unwrap();
        let peer = pipe
            .accept_spawned_worker(&worker, authorized, Duration::from_secs(2))
            .unwrap();
        assert_eq!(peer.process_id(), std::process::id());
        assert!(matches!(
            pipe.accept_spawned_worker(&worker, authorized, Duration::ZERO),
            Err(DisplayWorkerPipeError::InvalidAcceptTimeout)
        ));
        drop(client.join().unwrap());
    }

    #[test]
    fn bound_channel_exchanges_start_key_frames_and_graceful_stop() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pipe_name = DisplayWorkerPipeName::parse(&format!(
            r"\\.\pipe\CapyIO.DisplayWorker.Channel.{}.{suffix}",
            std::process::id()
        ))
        .unwrap();
        let token = UserToken::current_process().unwrap();
        let pipe = AuthorizedDisplayWorkerPipe::create_for_token(&pipe_name, token.0).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "display_worker::tests::worker_channel_child",
                "--ignored",
            ])
            .env("CAPYIO_TEST_WORKER_PIPE", pipe_name.as_str())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let worker = SpawnedDisplayWorker::retain_for_test(child.id()).unwrap();
        let context = DisplaySessionContext::query().unwrap();
        let authorized = InteractiveDisplaySessionId::new(context.process_session_id()).unwrap();
        let mut channel = BoundDisplayWorkerChannel::bind_spawned(
            pipe,
            worker,
            authorized,
            Duration::from_secs(2),
        )
        .unwrap();

        let session = ValidatedDisplayStreamSession::new(DisplayStreamSessionRequest {
            adapter_instance_id: [11; 16],
            display_capability_id: [12; 16],
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
            9,
            authorized,
            &session,
            GameStreamLaunchBinding {
                session_id: [13; 16],
                route_id: [14; 16],
                stream_id: [15; 16],
                stream_epoch: 16,
                host_ipv4: std::net::Ipv4Addr::LOCALHOST,
            },
        )
        .unwrap();
        channel
            .exchange_lease(&lease, Duration::from_secs(2))
            .unwrap();
        assert!(matches!(
            channel.exchange_lease(&lease, Duration::from_secs(2)),
            Err(DisplayWorkerChannelError::InvalidExchangeState)
        ));
        let premature_start = AuthenticatedEngineLaunch::new(
            session.request().adapter_instance_id,
            session.request().display_capability_id,
            17,
            [0x42; 16],
            18,
            [0x24; 8],
            19,
        )
        .unwrap()
        .into_worker_start_record(&lease)
        .unwrap();
        assert!(matches!(
            channel.exchange_start(premature_start, Duration::from_secs(2)),
            Err(DisplayWorkerChannelError::InvalidExchangeState)
        ));
        let configuration = DisplayWorkerEngineConfigRecord::new(
            &lease,
            Path::new(r"C:\CapyIO\capyio-display-engine.dll"),
            [0x31; 32],
            &session,
        )
        .unwrap();
        channel
            .exchange_configuration(&configuration, Duration::from_secs(2))
            .unwrap();
        assert!(matches!(
            channel.exchange_configuration(&configuration, Duration::from_secs(2)),
            Err(DisplayWorkerChannelError::InvalidExchangeState)
        ));
        let start = AuthenticatedEngineLaunch::new(
            session.request().adapter_instance_id,
            session.request().display_capability_id,
            17,
            [0x42; 16],
            18,
            [0x24; 8],
            19,
        )
        .unwrap()
        .into_worker_start_record(&lease)
        .unwrap();
        channel
            .exchange_start(start, Duration::from_secs(2))
            .unwrap();
        let repeated = AuthenticatedEngineLaunch::new(
            session.request().adapter_instance_id,
            session.request().display_capability_id,
            20,
            [0x43; 16],
            21,
            [0x25; 8],
            22,
        )
        .unwrap()
        .into_worker_start_record(&lease)
        .unwrap();
        assert!(matches!(
            channel.exchange_start(repeated, Duration::from_secs(2)),
            Err(DisplayWorkerChannelError::InvalidExchangeState)
        ));
        channel.request_key_frame(Duration::from_secs(2)).unwrap();
        channel.request_key_frame(Duration::from_secs(2)).unwrap();
        channel.graceful_stop(Duration::from_secs(2)).unwrap();
        assert!(matches!(
            channel.request_key_frame(Duration::from_secs(2)),
            Err(DisplayWorkerChannelError::InvalidExchangeState)
        ));
        channel.wait_for_exit(Duration::from_secs(2)).unwrap();
        assert!(!channel.is_running().unwrap());
        assert!(child.wait().unwrap().success());
    }

    #[test]
    #[ignore = "subprocess fixture launched by bound_channel_exchanges_start_key_frames_and_graceful_stop"]
    fn worker_channel_child() {
        let pipe_name = std::env::var("CAPYIO_TEST_WORKER_PIPE").unwrap();
        let pipe_name = DisplayWorkerPipeName::parse(&pipe_name).unwrap();
        let prepared = connect_display_worker_start_client(&pipe_name).unwrap();
        let (mut connection, lease, _configuration, _launch) = prepared.into_parts();
        loop {
            match connection.wait_for_control(&lease).unwrap() {
                DisplayWorkerControlEvent::Disconnected => break,
                DisplayWorkerControlEvent::Command { command, record } => {
                    connection.acknowledge_control(&lease, &record).unwrap();
                    if command == DisplayWorkerControlCommand::GracefulStop {
                        break;
                    }
                }
            }
        }
    }

    #[test]
    fn channel_accept_timeout_terminates_and_reaps_exact_owned_child() {
        let pipe_name = DisplayWorkerPipeName::parse(&format!(
            r"\\.\pipe\CapyIO.DisplayWorker.Stall.{}",
            std::process::id()
        ))
        .unwrap();
        let token = UserToken::current_process().unwrap();
        let pipe = AuthorizedDisplayWorkerPipe::create_for_token(&pipe_name, token.0).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "display_worker::tests::worker_channel_stall_child",
                "--ignored",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let worker = SpawnedDisplayWorker::retain_for_test(child.id()).unwrap();
        let context = DisplaySessionContext::query().unwrap();
        let authorized = InteractiveDisplaySessionId::new(context.process_session_id()).unwrap();
        assert!(matches!(
            BoundDisplayWorkerChannel::bind_spawned(
                pipe,
                worker,
                authorized,
                Duration::from_millis(50),
            ),
            Err(DisplayWorkerChannelError::Accept(
                DisplayWorkerPipeError::AcceptTimedOut
            ))
        ));
        assert!(!child.wait().unwrap().success());
    }

    #[test]
    #[ignore = "subprocess fixture launched by channel_accept_timeout_terminates_and_reaps_exact_owned_child"]
    fn worker_channel_stall_child() {
        thread::sleep(Duration::from_secs(10));
    }

    #[test]
    fn connected_pipe_binds_the_kernel_reported_client_process_and_session() {
        let pipe_name = format!(
            r"\\.\pipe\CapyIO.DisplayWorker.Test.{}.{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        );
        let mut wide_name = pipe_name.encode_utf16().collect::<Vec<_>>();
        wide_name.push(0);
        // SAFETY: the name is NUL-terminated and all buffer/mode values are
        // fixed. A successful handle is immediately transferred to File.
        let raw_pipe = unsafe {
            CreateNamedPipeW(
                wide_name.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                256,
                256,
                0,
                ptr::null(),
            )
        };
        assert_ne!(raw_pipe, INVALID_HANDLE_VALUE);
        // SAFETY: raw_pipe is the unique successful CreateNamedPipeW result.
        let server = unsafe { File::from_raw_handle(raw_pipe) };
        let (client_sender, client_receiver) = mpsc::sync_channel(1);
        let client = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            loop {
                if let Ok(file) = OpenOptions::new().read(true).write(true).open(&pipe_name) {
                    client_sender.send(file).unwrap();
                    return;
                }
                assert!(std::time::Instant::now() < deadline);
                thread::sleep(Duration::from_millis(5));
            }
        });
        // SAFETY: server owns a listening named-pipe handle and no overlapped
        // structure is needed for this blocking test connection.
        if unsafe { ConnectNamedPipe(server.as_raw_handle() as HANDLE, ptr::null_mut()) } == 0 {
            // SAFETY: GetLastError immediately follows ConnectNamedPipe.
            assert_eq!(unsafe { GetLastError() }, ERROR_PIPE_CONNECTED);
        }
        let client_file = client_receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        let context = DisplaySessionContext::query().unwrap();
        let authorized = InteractiveDisplaySessionId::new(context.process_session_id()).unwrap();
        let wrong_process = ExpectedDisplayWorkerProcessId::new(
            std::process::id()
                .checked_add(1)
                .expect("test PID can advance"),
        )
        .unwrap();
        assert!(matches!(
            BoundDisplayWorkerPeer::bind_connected_pipe(&server, wrong_process, authorized),
            Err(DisplayWorkerPeerError::UnexpectedProcess { .. })
        ));
        let expected = ExpectedDisplayWorkerProcessId::new(std::process::id()).unwrap();
        let peer =
            BoundDisplayWorkerPeer::bind_connected_pipe(&server, expected, authorized).unwrap();
        assert_eq!(peer.process_id(), std::process::id());
        assert_eq!(peer.session_id(), authorized);
        assert!(peer.is_running().unwrap());
        drop(client_file);
        client.join().unwrap();
    }
}
