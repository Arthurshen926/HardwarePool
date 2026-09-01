use std::{
    io::Read,
    net::SocketAddrV4,
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use thiserror::Error;

const SPEAKER_READY_PREFIX: &[u8] = b"native_virtual_speaker=true ";
const MICROPHONE_READY_PREFIX: &[u8] = b"native_virtual_microphone=true ";
const MAX_READY_LINE_BYTES: usize = 1_024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeSpeakerSupervisorLimits {
    pub startup_deadline: Duration,
    pub process_output_bytes: usize,
}

impl Default for NativeSpeakerSupervisorLimits {
    fn default() -> Self {
        Self {
            startup_deadline: Duration::from_secs(3),
            process_output_bytes: 16 * 1024,
        }
    }
}

impl NativeSpeakerSupervisorLimits {
    fn validate(self) -> Result<Self, NativeSpeakerSupervisorError> {
        if self.startup_deadline.is_zero() || self.startup_deadline > Duration::from_secs(30) {
            return Err(NativeSpeakerSupervisorError::InvalidStartupDeadline);
        }
        if !(1_024..=1024 * 1024).contains(&self.process_output_bytes) {
            return Err(NativeSpeakerSupervisorError::InvalidOutputLimit);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeSpeakerSupervisorStatus {
    Stopped,
    Running { process_id: u32 },
    Exited { exit_code: Option<i32> },
}

struct BoundedOutput {
    retained_bytes: usize,
    overflowed: bool,
}

struct RunningProcess {
    child: Child,
    stdout: JoinHandle<std::io::Result<BoundedOutput>>,
    stderr: JoinHandle<std::io::Result<BoundedOutput>>,
}

pub struct NativeSpeakerSupervisor {
    executable: PathBuf,
    local: SocketAddrV4,
    peer: SocketAddrV4,
    limits: NativeSpeakerSupervisorLimits,
    running: Option<RunningProcess>,
    terminal_exit: Option<Option<i32>>,
    ready_prefix: &'static [u8],
    reader_thread_prefix: &'static str,
}

impl NativeSpeakerSupervisor {
    pub fn new(
        executable: impl Into<PathBuf>,
        local: SocketAddrV4,
        peer: SocketAddrV4,
        limits: NativeSpeakerSupervisorLimits,
    ) -> Result<Self, NativeSpeakerSupervisorError> {
        Self::new_for_role(
            executable,
            local,
            peer,
            limits,
            SPEAKER_READY_PREFIX,
            "capyio-native-speaker",
        )
    }

    fn new_for_role(
        executable: impl Into<PathBuf>,
        local: SocketAddrV4,
        peer: SocketAddrV4,
        limits: NativeSpeakerSupervisorLimits,
        ready_prefix: &'static [u8],
        reader_thread_prefix: &'static str,
    ) -> Result<Self, NativeSpeakerSupervisorError> {
        let executable = executable.into();
        if executable.as_os_str().is_empty() {
            return Err(NativeSpeakerSupervisorError::MissingExecutable);
        }
        if !is_concrete_unicast(*local.ip())
            || !is_concrete_unicast(*peer.ip())
            || local.port() == 0
            || peer.port() == 0
            || local == peer
        {
            return Err(NativeSpeakerSupervisorError::InvalidEndpoint);
        }
        Ok(Self {
            executable,
            local,
            peer,
            limits: limits.validate()?,
            running: None,
            terminal_exit: None,
            ready_prefix,
            reader_thread_prefix,
        })
    }

    pub fn start(&mut self) -> Result<u32, NativeSpeakerSupervisorError> {
        if matches!(
            self.status()?,
            NativeSpeakerSupervisorStatus::Running { .. }
        ) {
            return Err(NativeSpeakerSupervisorError::AlreadyRunning);
        }
        self.terminal_exit = None;
        let mut command = Command::new(&self.executable);
        command
            .args([self.local.to_string(), self.peer.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_hidden_process(&mut command);
        let mut child = command
            .spawn()
            .map_err(NativeSpeakerSupervisorError::Spawn)?;
        let process_id = child.id();
        let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(NativeSpeakerSupervisorError::MissingProcessPipe);
        };
        let (ready_tx, ready_rx) = mpsc::channel();
        let (startup_error_tx, startup_error_rx) = mpsc::channel();
        let output_limit = self.limits.process_output_bytes;
        let ready_prefix = self.ready_prefix;
        let stdout_name = format!("{}-stdout", self.reader_thread_prefix);
        let stderr_name = format!("{}-stderr", self.reader_thread_prefix);
        let stdout = thread::Builder::new()
            .name(stdout_name)
            .spawn(move || read_stdout(stdout, output_limit, ready_prefix, ready_tx))
            .map_err(|_| NativeSpeakerSupervisorError::ReaderThread)?;
        let stderr = thread::Builder::new()
            .name(stderr_name)
            .spawn(move || read_stderr(stderr, output_limit, startup_error_tx))
            .map_err(|_| NativeSpeakerSupervisorError::ReaderThread)?;
        self.running = Some(RunningProcess {
            child,
            stdout,
            stderr,
        });
        self.wait_until_ready(ready_rx, startup_error_rx)?;
        Ok(process_id)
    }

    pub fn status(
        &mut self,
    ) -> Result<NativeSpeakerSupervisorStatus, NativeSpeakerSupervisorError> {
        let Some(running) = self.running.as_mut() else {
            return Ok(self
                .terminal_exit
                .map_or(NativeSpeakerSupervisorStatus::Stopped, |exit_code| {
                    NativeSpeakerSupervisorStatus::Exited { exit_code }
                }));
        };
        match running
            .child
            .try_wait()
            .map_err(NativeSpeakerSupervisorError::Wait)?
        {
            Some(status) => {
                let exit_code = status.code();
                self.finish_running(status)?;
                Ok(NativeSpeakerSupervisorStatus::Exited { exit_code })
            }
            None => Ok(NativeSpeakerSupervisorStatus::Running {
                process_id: running.child.id(),
            }),
        }
    }

    pub fn stop(&mut self) -> Result<(), NativeSpeakerSupervisorError> {
        let Some(mut running) = self.running.take() else {
            self.terminal_exit = None;
            return Ok(());
        };
        if running
            .child
            .try_wait()
            .map_err(NativeSpeakerSupervisorError::Wait)?
            .is_none()
        {
            running
                .child
                .kill()
                .map_err(NativeSpeakerSupervisorError::Stop)?;
        }
        let status = running
            .child
            .wait()
            .map_err(NativeSpeakerSupervisorError::Wait)?;
        join_output(running.stdout)?;
        join_output(running.stderr)?;
        self.terminal_exit = Some(status.code());
        Ok(())
    }

    fn wait_until_ready(
        &mut self,
        ready: Receiver<Result<(), NativeSpeakerSupervisorError>>,
        startup_error: Receiver<String>,
    ) -> Result<(), NativeSpeakerSupervisorError> {
        let deadline = Instant::now() + self.limits.startup_deadline;
        loop {
            if let Ok(result) = ready.recv_timeout(Duration::from_millis(10)) {
                if result.is_ok() {
                    return Ok(());
                }
                if let Ok(error) = startup_error.recv_timeout(Duration::from_millis(50)) {
                    let _ = self.stop();
                    return Err(NativeSpeakerSupervisorError::BrokerStartup(error));
                }
                let _ = self.stop();
                return result;
            }
            if let Ok(error) = startup_error.try_recv() {
                let _ = self.stop();
                return Err(NativeSpeakerSupervisorError::BrokerStartup(error));
            }
            if !matches!(
                self.status()?,
                NativeSpeakerSupervisorStatus::Running { .. }
            ) {
                let _ = self.stop();
                return Err(NativeSpeakerSupervisorError::ExitedBeforeReady);
            }
            if Instant::now() >= deadline {
                let _ = self.stop();
                return Err(NativeSpeakerSupervisorError::StartupTimedOut);
            }
        }
    }

    fn finish_running(&mut self, status: ExitStatus) -> Result<(), NativeSpeakerSupervisorError> {
        let running = self.running.take().expect("running process must exist");
        join_output(running.stdout)?;
        join_output(running.stderr)?;
        self.terminal_exit = Some(status.code());
        Ok(())
    }
}

pub struct NativeMicrophoneSupervisor(NativeSpeakerSupervisor);

impl NativeMicrophoneSupervisor {
    pub fn new(
        executable: impl Into<PathBuf>,
        local: SocketAddrV4,
        peer: SocketAddrV4,
        limits: NativeSpeakerSupervisorLimits,
    ) -> Result<Self, NativeSpeakerSupervisorError> {
        NativeSpeakerSupervisor::new_for_role(
            executable,
            local,
            peer,
            limits,
            MICROPHONE_READY_PREFIX,
            "capyio-native-microphone",
        )
        .map(Self)
    }

    pub fn start(&mut self) -> Result<u32, NativeSpeakerSupervisorError> {
        self.0.start()
    }

    pub fn status(
        &mut self,
    ) -> Result<NativeSpeakerSupervisorStatus, NativeSpeakerSupervisorError> {
        self.0.status()
    }

    pub fn stop(&mut self) -> Result<(), NativeSpeakerSupervisorError> {
        self.0.stop()
    }
}

impl Drop for NativeSpeakerSupervisor {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn read_stdout(
    mut input: impl Read,
    limit: usize,
    ready_prefix: &'static [u8],
    ready: mpsc::Sender<Result<(), NativeSpeakerSupervisorError>>,
) -> std::io::Result<BoundedOutput> {
    let mut retained_bytes = 0_usize;
    let mut overflowed = false;
    let mut first_line = Vec::new();
    let mut readiness_sent = false;
    let mut buffer = [0_u8; 4_096];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            if !readiness_sent {
                let _ = ready.send(Err(NativeSpeakerSupervisorError::MissingReadyLine));
            }
            break;
        }
        let remaining = limit.saturating_sub(retained_bytes);
        retained_bytes += remaining.min(read);
        overflowed |= read > remaining;
        if !readiness_sent {
            for byte in &buffer[..read] {
                if *byte == b'\n' {
                    let valid = first_line.starts_with(ready_prefix);
                    let _ = ready.send(if valid {
                        Ok(())
                    } else {
                        Err(NativeSpeakerSupervisorError::InvalidReadyLine)
                    });
                    readiness_sent = true;
                    break;
                }
                if first_line.len() >= MAX_READY_LINE_BYTES {
                    let _ = ready.send(Err(NativeSpeakerSupervisorError::InvalidReadyLine));
                    readiness_sent = true;
                    break;
                }
                if *byte != b'\r' {
                    first_line.push(*byte);
                }
            }
        }
    }
    Ok(BoundedOutput {
        retained_bytes,
        overflowed,
    })
}

fn read_stderr(
    mut input: impl Read,
    limit: usize,
    startup_error: mpsc::Sender<String>,
) -> std::io::Result<BoundedOutput> {
    let mut retained_bytes = 0_usize;
    let mut overflowed = false;
    let mut first_line = Vec::new();
    let mut sent = false;
    let mut buffer = [0_u8; 4_096];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            if !sent && !first_line.is_empty() {
                let _ = startup_error.send(String::from_utf8_lossy(&first_line).into_owned());
            }
            break;
        }
        let remaining = limit.saturating_sub(retained_bytes);
        retained_bytes += remaining.min(read);
        overflowed |= read > remaining;
        if !sent {
            for byte in &buffer[..read] {
                if *byte == b'\n' {
                    let _ = startup_error.send(String::from_utf8_lossy(&first_line).into_owned());
                    sent = true;
                    break;
                }
                if first_line.len() >= MAX_READY_LINE_BYTES {
                    let _ = startup_error.send("startup error line exceeded bound".to_owned());
                    sent = true;
                    break;
                }
                if *byte != b'\r' {
                    first_line.push(*byte);
                }
            }
        }
    }
    Ok(BoundedOutput {
        retained_bytes,
        overflowed,
    })
}

fn join_output(
    handle: JoinHandle<std::io::Result<BoundedOutput>>,
) -> Result<(), NativeSpeakerSupervisorError> {
    let output = handle
        .join()
        .map_err(|_| NativeSpeakerSupervisorError::ReaderThread)?
        .map_err(NativeSpeakerSupervisorError::Read)?;
    let _ = (output.retained_bytes, output.overflowed);
    Ok(())
}

fn is_concrete_unicast(ip: std::net::Ipv4Addr) -> bool {
    !ip.is_unspecified() && !ip.is_multicast() && ip != std::net::Ipv4Addr::BROADCAST
}

#[cfg(windows)]
fn configure_hidden_process(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure_hidden_process(_command: &mut Command) {}

#[derive(Debug, Error)]
pub enum NativeSpeakerSupervisorError {
    #[error("native speaker Broker executable is required")]
    MissingExecutable,
    #[error("native speaker endpoints must be distinct concrete IPv4 socket addresses")]
    InvalidEndpoint,
    #[error("native speaker startup deadline is outside 1 ms..=30 s")]
    InvalidStartupDeadline,
    #[error("native speaker output limit is outside 1024..=1048576 bytes")]
    InvalidOutputLimit,
    #[error("native speaker Broker is already running")]
    AlreadyRunning,
    #[error("could not spawn native speaker Broker: {0}")]
    Spawn(std::io::Error),
    #[error("native speaker Broker did not expose stdout/stderr")]
    MissingProcessPipe,
    #[error("could not start native speaker output reader")]
    ReaderThread,
    #[error("could not read native speaker output: {0}")]
    Read(std::io::Error),
    #[error("could not poll native speaker Broker: {0}")]
    Wait(std::io::Error),
    #[error("could not stop native speaker Broker: {0}")]
    Stop(std::io::Error),
    #[error("native speaker Broker exited before readiness")]
    ExitedBeforeReady,
    #[error("native speaker Broker readiness timed out")]
    StartupTimedOut,
    #[error("native speaker Broker omitted its readiness line")]
    MissingReadyLine,
    #[error("native speaker Broker emitted an invalid readiness line")]
    InvalidReadyLine,
    #[error("native speaker Broker rejected startup: {0}")]
    BrokerStartup(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_is_closed_and_bounded() {
        let local = "100.64.0.1:46001".parse().unwrap();
        let peer = "100.64.0.2:46000".parse().unwrap();
        assert!(NativeSpeakerSupervisor::new("broker", local, peer, Default::default()).is_ok());
        assert!(NativeSpeakerSupervisor::new("", local, peer, Default::default()).is_err());
        assert!(NativeSpeakerSupervisor::new("broker", local, local, Default::default()).is_err());
    }

    #[test]
    fn readiness_line_is_exact_and_output_is_bounded() {
        let (sender, receiver) = mpsc::channel();
        let output = read_stdout(
            &b"native_virtual_speaker=true local=100.64.0.1:1\nrest"[..],
            8,
            SPEAKER_READY_PREFIX,
            sender,
        )
        .unwrap();
        assert!(receiver.recv().unwrap().is_ok());
        assert_eq!(output.retained_bytes, 8);
        assert!(output.overflowed);

        let (sender, receiver) = mpsc::channel();
        read_stdout(
            &b"unexpected=true\n"[..],
            1024,
            SPEAKER_READY_PREFIX,
            sender,
        )
        .unwrap();
        assert!(receiver.recv().unwrap().is_err());
    }

    #[test]
    fn microphone_role_requires_its_own_readiness_prefix() {
        let local = "100.64.0.1:46011".parse().unwrap();
        let peer = "100.64.0.2:46010".parse().unwrap();
        assert!(NativeMicrophoneSupervisor::new("broker", local, peer, Default::default()).is_ok());

        let (sender, receiver) = mpsc::channel();
        read_stdout(
            &b"native_virtual_speaker=true local=100.64.0.1:1\n"[..],
            1024,
            MICROPHONE_READY_PREFIX,
            sender,
        )
        .unwrap();
        assert!(receiver.recv().unwrap().is_err());
    }

    #[test]
    fn stderr_startup_line_is_bounded_and_separate_from_readiness() {
        let (sender, receiver) = mpsc::channel();
        let output = read_stderr(&b"bind failed\nmore"[..], 4, sender).unwrap();
        assert_eq!(receiver.recv().unwrap(), "bind failed");
        assert_eq!(output.retained_bytes, 4);
        assert!(output.overflowed);
    }
}
