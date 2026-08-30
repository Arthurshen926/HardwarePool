#![forbid(unsafe_code)]

//! Bounded external-process boundary for MicYou v2.0.1.
//!
//! The GPL MicYou process retains its private media plane. This crate only
//! verifies, configures and supervises a user-supplied executable directly.

use std::{
    fmt,
    io::{self, Read},
    net::{IpAddr, SocketAddr, TcpStream},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use capyio_audio::{
    AudioMediaStreamBinding, AudioStreamSpec, AudioTransportBackendContract,
    AudioTransportEncodingSupport, AudioTransportFieldFidelity, AudioTransportInteroperability,
    AudioTransportMediaAccess, AudioTransportMetadataFidelity, AudioTransportSecurity,
};
use thiserror::Error;

mod peer_presence;

pub use peer_presence::PeerTcpPresence;

pub const PINNED_MICYOU_VERSION: &str = "2.0.1";
pub const REQUIRED_MICYOU_CAPABILITY: &str = "device-stable-id-v1";
pub const DEFAULT_MICYOU_PORT: u16 = 8554;
pub const MAX_OUTPUT_DEVICES: usize = 64;
pub const MAX_DEVICE_ID_BYTES: usize = 512;
pub const MAX_DEVICE_NAME_BYTES: usize = 512;
pub const MAX_PROBE_OUTPUT_BYTES: usize = 64 * 1024;
pub const MAX_PROBE_LINE_BYTES: usize = 1024;
pub const MICYOU_COMPAT_BACKEND_ID: &str = "dev.capyio.compat.micyou/2.0.1";

#[must_use]
pub fn micyou_compatibility_contract() -> AudioTransportBackendContract {
    let contract = AudioTransportBackendContract {
        backend_id: MICYOU_COMPAT_BACKEND_ID,
        interoperability: AudioTransportInteroperability::AdapterManaged,
        media_access: AudioTransportMediaAccess::OpaqueProcess,
        encodings: AudioTransportEncodingSupport {
            pcm: true,
            opus: true,
        },
        metadata: AudioTransportMetadataFidelity {
            session_route_binding: AudioTransportFieldFidelity::Absent,
            stream_identity: AudioTransportFieldFidelity::Absent,
            stream_epoch: AudioTransportFieldFidelity::Absent,
            sequence: AudioTransportFieldFidelity::Opaque,
            source_timestamp: AudioTransportFieldFidelity::Opaque,
            sample_timeline: AudioTransportFieldFidelity::Opaque,
            discontinuity: AudioTransportFieldFidelity::Opaque,
            selected_stream_spec: AudioTransportFieldFidelity::Partial,
            payload: AudioTransportFieldFidelity::Opaque,
        },
        security: AudioTransportSecurity::default(),
    };
    debug_assert!(contract.validate().is_ok());
    contract
}

/// Control-plane association for the opaque MicYou compatibility process.
///
/// The binding gives the CapyIO Runtime one Route/epoch identity, but MicYou's
/// process does not consume `AudioMediaPacket` and does not carry those IDs on
/// its private wire.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MicYouCompatibilityBinding {
    binding: AudioMediaStreamBinding,
}

impl MicYouCompatibilityBinding {
    #[must_use]
    pub const fn binding(&self) -> &AudioMediaStreamBinding {
        &self.binding
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct MicYouConfig {
    executable: PathBuf,
    bind_address: SocketAddr,
    output_device_id: String,
    output_device: String,
}

impl fmt::Debug for MicYouConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MicYouConfig")
            .field("executable", &"<redacted>")
            .field("bind_address", &"<redacted>")
            .field("output_device_id", &"<redacted>")
            .field("output_device", &"<redacted>")
            .finish()
    }
}

impl MicYouConfig {
    pub fn new(
        executable: impl Into<PathBuf>,
        bind_ip: IpAddr,
        port: u16,
        output_device_id: impl Into<String>,
        output_device: impl Into<String>,
    ) -> Result<Self, MicYouError> {
        let executable = executable.into();
        if executable.as_os_str().is_empty() {
            return Err(MicYouError::EmptyExecutablePath);
        }
        if !matches!(bind_ip, IpAddr::V4(address) if !address.is_unspecified()) {
            return Err(MicYouError::InvalidBindAddress);
        }
        if port == 0 || port == u16::MAX {
            return Err(MicYouError::InvalidPort { port });
        }
        let output_device_id = output_device_id.into();
        validate_device_id(&output_device_id)?;
        let output_device = output_device.into();
        validate_device_name(&output_device)?;
        Ok(Self {
            executable,
            bind_address: SocketAddr::new(bind_ip, port),
            output_device_id,
            output_device,
        })
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub const fn bind_address(&self) -> SocketAddr {
        self.bind_address
    }

    pub fn output_device(&self) -> &str {
        &self.output_device
    }

    pub fn output_device_id(&self) -> &str {
        &self.output_device_id
    }

    fn serve_args(&self, selection: &MicYouOutputDevice) -> Result<Vec<String>, MicYouError> {
        if selection.id != self.output_device_id || selection.name != self.output_device {
            return Err(MicYouError::ConfiguredDeviceChanged);
        }
        Ok(vec![
            "serve".to_owned(),
            "--port".to_owned(),
            self.bind_address.port().to_string(),
            "--mode".to_owned(),
            "wifi".to_owned(),
            "--device".to_owned(),
            self.output_device.clone(),
            "--device-id".to_owned(),
            self.output_device_id.clone(),
            "--device-index".to_owned(),
            selection.index.to_string(),
            "--bind".to_owned(),
            self.bind_address.ip().to_string(),
        ])
    }

    /// Semantic mapping only. MicYou's private negotiated wire is not a
    /// `capyio.audio.frames/1` StandardPort.
    pub fn audio_stream_spec(&self) -> AudioStreamSpec {
        AudioStreamSpec::voice_interactive()
    }

    /// Associates the opaque external process with one conservative CapyIO
    /// voice stream. This validates lifecycle identity but does not claim media
    /// packet visibility or exact private codec negotiation.
    pub fn bind_media_stream(
        &self,
        binding: AudioMediaStreamBinding,
    ) -> Result<MicYouCompatibilityBinding, MicYouError> {
        binding
            .validate()
            .map_err(|error| MicYouError::InvalidMediaBinding(error.to_string()))?;
        if binding.selected_spec != self.audio_stream_spec() {
            return Err(MicYouError::UnsupportedMediaSpec);
        }
        Ok(MicYouCompatibilityBinding { binding })
    }
}

fn validate_device_id(id: &str) -> Result<(), MicYouError> {
    if id.is_empty() {
        return Err(MicYouError::EmptyDeviceId);
    }
    if id.len() > MAX_DEVICE_ID_BYTES {
        return Err(MicYouError::DeviceIdTooLong {
            actual: id.len(),
            limit: MAX_DEVICE_ID_BYTES,
        });
    }
    if id.trim() != id || id.chars().any(|character| character.is_control()) {
        return Err(MicYouError::InvalidDeviceId);
    }
    Ok(())
}

fn validate_device_name(name: &str) -> Result<(), MicYouError> {
    if name.is_empty() {
        return Err(MicYouError::EmptyDeviceName);
    }
    if name.len() > MAX_DEVICE_NAME_BYTES {
        return Err(MicYouError::DeviceNameTooLong {
            actual: name.len(),
            limit: MAX_DEVICE_NAME_BYTES,
        });
    }
    if name.chars().any(|character| character.is_control()) {
        return Err(MicYouError::InvalidDeviceName);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProbeLimits {
    pub deadline: Duration,
    pub output_bytes: usize,
    pub line_bytes: usize,
}

impl Default for ProbeLimits {
    fn default() -> Self {
        Self {
            deadline: Duration::from_secs(5),
            output_bytes: MAX_PROBE_OUTPUT_BYTES,
            line_bytes: MAX_PROBE_LINE_BYTES,
        }
    }
}

impl ProbeLimits {
    fn validate(self) -> Result<Self, MicYouError> {
        if self.deadline.is_zero() || self.deadline > Duration::from_secs(30) {
            return Err(MicYouError::InvalidProbeDeadline);
        }
        if self.output_bytes == 0 || self.output_bytes > MAX_PROBE_OUTPUT_BYTES {
            return Err(MicYouError::InvalidProbeOutputLimit);
        }
        if self.line_bytes == 0 || self.line_bytes > MAX_PROBE_LINE_BYTES {
            return Err(MicYouError::InvalidProbeLineLimit);
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MicYouOutputDevice {
    pub index: NonZeroUsize,
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MicYouInventory {
    pub version: String,
    pub output_devices: Vec<MicYouOutputDevice>,
}

impl MicYouInventory {
    pub fn resolve_configured_device(
        &self,
        config: &MicYouConfig,
    ) -> Result<&MicYouOutputDevice, MicYouError> {
        let mut matches = self
            .output_devices
            .iter()
            .filter(|device| device.id == config.output_device_id());
        let device = matches.next().ok_or(MicYouError::ConfiguredDeviceMissing)?;
        if matches.next().is_some() {
            return Err(MicYouError::DuplicateDeviceId);
        }
        if device.name != config.output_device() {
            return Err(MicYouError::ConfiguredDeviceChanged);
        }
        Ok(device)
    }
}

pub struct MicYouProbe {
    limits: ProbeLimits,
}

impl MicYouProbe {
    pub fn new(limits: ProbeLimits) -> Result<Self, MicYouError> {
        Ok(Self {
            limits: limits.validate()?,
        })
    }

    pub fn inventory(&self, executable: &Path) -> Result<MicYouInventory, MicYouError> {
        if executable.as_os_str().is_empty() {
            return Err(MicYouError::EmptyExecutablePath);
        }
        let version = run_probe(executable, &["--version"], self.limits)?;
        let version = parse_version_output(&version)?;
        if version != PINNED_MICYOU_VERSION {
            return Err(MicYouError::UnsupportedVersion { actual: version });
        }
        let capabilities = run_probe(executable, &["capyio-capabilities"], self.limits)?;
        parse_capabilities_output(&capabilities)?;
        let devices = run_probe(executable, &["devices"], self.limits)?;
        let output_devices = parse_devices_output(&devices, self.limits)?;
        Ok(MicYouInventory {
            version,
            output_devices,
        })
    }

    pub fn probe_config(&self, config: &MicYouConfig) -> Result<MicYouInventory, MicYouError> {
        let inventory = self.inventory(config.executable())?;
        inventory.resolve_configured_device(config)?;
        Ok(inventory)
    }
}

pub fn parse_version_output(output: &[u8]) -> Result<String, MicYouError> {
    let text = std::str::from_utf8(output).map_err(|_| MicYouError::NonUtf8ProbeOutput)?;
    let mut parts = text.split_whitespace();
    match (parts.next(), parts.next(), parts.next()) {
        (Some("micyou-cli"), Some(version), None) if !version.is_empty() => Ok(version.to_owned()),
        _ => Err(MicYouError::MalformedVersionOutput),
    }
}

pub fn parse_capabilities_output(output: &[u8]) -> Result<(), MicYouError> {
    let text = std::str::from_utf8(output).map_err(|_| MicYouError::NonUtf8ProbeOutput)?;
    if text.trim() == REQUIRED_MICYOU_CAPABILITY {
        Ok(())
    } else {
        Err(MicYouError::RequiredCapabilityMissing)
    }
}

pub fn parse_devices_output(
    output: &[u8],
    limits: ProbeLimits,
) -> Result<Vec<MicYouOutputDevice>, MicYouError> {
    let limits = limits.validate()?;
    if output.len() > limits.output_bytes {
        return Err(MicYouError::ProbeOutputTooLarge);
    }
    let text = std::str::from_utf8(output).map_err(|_| MicYouError::NonUtf8ProbeOutput)?;
    if text.trim() == "no audio output devices found" {
        return Ok(Vec::new());
    }
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("audio output devices v2:") {
        return Err(MicYouError::MalformedDeviceOutput);
    }
    let mut devices = Vec::new();
    for line in lines {
        if line.len() > limits.line_bytes {
            return Err(MicYouError::ProbeLineTooLong);
        }
        if devices.len() == MAX_OUTPUT_DEVICES {
            return Err(MicYouError::TooManyDevices);
        }
        let trimmed = line.trim();
        let Some((index, identity)) = trimmed.split_once(". ") else {
            return Err(MicYouError::MalformedDeviceOutput);
        };
        let Some((id, name)) = identity.split_once('\t') else {
            return Err(MicYouError::MalformedDeviceOutput);
        };
        let index = index
            .parse::<usize>()
            .map_err(|_| MicYouError::MalformedDeviceOutput)?;
        if index != devices.len() + 1 {
            return Err(MicYouError::MalformedDeviceOutput);
        }
        validate_device_id(id)?;
        validate_device_name(name)?;
        if devices
            .iter()
            .any(|device: &MicYouOutputDevice| device.id == id)
        {
            return Err(MicYouError::DuplicateDeviceId);
        }
        devices.push(MicYouOutputDevice {
            index: NonZeroUsize::new(index).ok_or(MicYouError::MalformedDeviceOutput)?,
            id: id.to_owned(),
            name: name.to_owned(),
        });
    }
    if devices.is_empty() {
        return Err(MicYouError::MalformedDeviceOutput);
    }
    Ok(devices)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SupervisorLimits {
    pub startup_deadline: Duration,
    pub retained_output_bytes: usize,
}

impl Default for SupervisorLimits {
    fn default() -> Self {
        Self {
            startup_deadline: Duration::from_secs(8),
            retained_output_bytes: MAX_PROBE_OUTPUT_BYTES,
        }
    }
}

impl SupervisorLimits {
    fn validate(self) -> Result<Self, MicYouError> {
        if self.startup_deadline.is_zero() || self.startup_deadline > Duration::from_secs(30) {
            return Err(MicYouError::InvalidStartupDeadline);
        }
        if self.retained_output_bytes == 0 || self.retained_output_bytes > MAX_PROBE_OUTPUT_BYTES {
            return Err(MicYouError::InvalidRetainedOutputLimit);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputSummary {
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub stdout_overflowed: bool,
    pub stderr_overflowed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupervisorStatus {
    Stopped,
    Running { process_id: u32 },
    Exited { exit_code: Option<i32> },
}

struct RunningProcess {
    child: Child,
    stdout: JoinHandle<io::Result<BoundedRead>>,
    stderr: JoinHandle<io::Result<BoundedRead>>,
}

pub struct MicYouSupervisor {
    config: MicYouConfig,
    probe_limits: ProbeLimits,
    limits: SupervisorLimits,
    running: Option<RunningProcess>,
    terminal_exit: Option<Option<i32>>,
}

impl MicYouSupervisor {
    pub fn new(
        config: MicYouConfig,
        probe_limits: ProbeLimits,
        limits: SupervisorLimits,
    ) -> Result<Self, MicYouError> {
        Ok(Self {
            config,
            probe_limits: probe_limits.validate()?,
            limits: limits.validate()?,
            running: None,
            terminal_exit: None,
        })
    }

    pub fn config(&self) -> &MicYouConfig {
        &self.config
    }

    pub fn start(&mut self) -> Result<u32, MicYouError> {
        if matches!(self.status()?, SupervisorStatus::Running { .. }) {
            return Err(MicYouError::AlreadyRunning);
        }
        let inventory = MicYouProbe::new(self.probe_limits)?.probe_config(&self.config)?;
        let selection = inventory.resolve_configured_device(&self.config)?;
        self.terminal_exit = None;
        let mut command = Command::new(self.config.executable());
        command
            .args(self.config.serve_args(selection)?)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_hidden_process(&mut command);
        let mut child = command.spawn().map_err(MicYouError::Spawn)?;
        let process_id = child.id();
        let stdout = child.stdout.take().ok_or(MicYouError::MissingProcessPipe)?;
        let stderr = child.stderr.take().ok_or(MicYouError::MissingProcessPipe)?;
        let limit = self.limits.retained_output_bytes;
        self.running = Some(RunningProcess {
            child,
            stdout: thread::spawn(move || read_bounded(stdout, limit)),
            stderr: thread::spawn(move || read_bounded(stderr, limit)),
        });

        let started = Instant::now();
        loop {
            if let Some(status) = self.try_wait_running()? {
                self.finalize(status)?;
                return Err(MicYouError::ExitedBeforeReady);
            }
            if TcpStream::connect_timeout(&self.config.bind_address(), Duration::from_millis(50))
                .is_ok()
            {
                return Ok(process_id);
            }
            if started.elapsed() >= self.limits.startup_deadline {
                let _ = self.stop()?;
                return Err(MicYouError::StartupTimedOut);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn status(&mut self) -> Result<SupervisorStatus, MicYouError> {
        if self.running.is_none() {
            return Ok(match self.terminal_exit {
                Some(exit_code) => SupervisorStatus::Exited { exit_code },
                None => SupervisorStatus::Stopped,
            });
        }
        if let Some(status) = self.try_wait_running()? {
            let exit_code = status.code();
            self.finalize(status)?;
            Ok(SupervisorStatus::Exited { exit_code })
        } else {
            Ok(SupervisorStatus::Running {
                process_id: self.running.as_ref().expect("checked").child.id(),
            })
        }
    }

    pub fn peer_tcp_presence(&mut self) -> Result<PeerTcpPresence, MicYouError> {
        let SupervisorStatus::Running { process_id } = self.status()? else {
            return Ok(PeerTcpPresence::SupervisorNotRunning);
        };
        peer_presence::peer_tcp_presence(process_id, self.config.bind_address())
    }

    pub fn stop(&mut self) -> Result<Option<OutputSummary>, MicYouError> {
        let Some(mut running) = self.running.take() else {
            self.terminal_exit = None;
            return Ok(None);
        };
        if running
            .child
            .try_wait()
            .map_err(MicYouError::Wait)?
            .is_none()
        {
            running.child.kill().map_err(MicYouError::Wait)?;
        }
        running.child.wait().map_err(MicYouError::Wait)?;
        self.terminal_exit = None;
        collect_output(running).map(Some)
    }

    fn try_wait_running(&mut self) -> Result<Option<ExitStatus>, MicYouError> {
        self.running
            .as_mut()
            .expect("running process")
            .child
            .try_wait()
            .map_err(MicYouError::Wait)
    }

    fn finalize(&mut self, status: ExitStatus) -> Result<OutputSummary, MicYouError> {
        let running = self.running.take().expect("running process");
        self.terminal_exit = Some(status.code());
        collect_output(running)
    }
}

impl Drop for MicYouSupervisor {
    fn drop(&mut self) {
        if let Some(mut running) = self.running.take() {
            let _ = running.child.kill();
            let _ = running.child.wait();
            let _ = collect_output(running);
        }
    }
}

fn run_probe(
    executable: &Path,
    args: &[&str],
    limits: ProbeLimits,
) -> Result<Vec<u8>, MicYouError> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_hidden_process(&mut command);
    let mut child = command.spawn().map_err(MicYouError::Spawn)?;
    let stdout = child.stdout.take().ok_or(MicYouError::MissingProcessPipe)?;
    let stderr = child.stderr.take().ok_or(MicYouError::MissingProcessPipe)?;
    let limit = limits.output_bytes;
    let stdout_reader = thread::spawn(move || read_bounded(stdout, limit));
    let stderr_reader = thread::spawn(move || read_bounded(stderr, limit));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(MicYouError::Wait)? {
            break status;
        }
        if started.elapsed() >= limits.deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(MicYouError::ProbeTimedOut);
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = join_reader(stdout_reader)?;
    let stderr = join_reader(stderr_reader)?;
    if stdout.overflowed || stderr.overflowed {
        return Err(MicYouError::ProbeOutputTooLarge);
    }
    if !status.success() {
        return Err(MicYouError::ProbeFailed {
            exit_code: status.code(),
        });
    }
    Ok(stdout.bytes)
}

struct BoundedRead {
    bytes: Vec<u8>,
    overflowed: bool,
}

fn read_bounded(mut reader: impl Read, limit: usize) -> io::Result<BoundedRead> {
    let mut bytes = Vec::with_capacity(limit.min(4096));
    let mut overflowed = false;
    let mut chunk = [0_u8; 4096];
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        let retained = limit.saturating_sub(bytes.len()).min(read);
        bytes.extend_from_slice(&chunk[..retained]);
        overflowed |= retained < read;
    }
    Ok(BoundedRead { bytes, overflowed })
}

fn join_reader(reader: JoinHandle<io::Result<BoundedRead>>) -> Result<BoundedRead, MicYouError> {
    reader
        .join()
        .map_err(|_| MicYouError::ReaderPanicked)?
        .map_err(MicYouError::Read)
}

fn collect_output(running: RunningProcess) -> Result<OutputSummary, MicYouError> {
    let stdout = join_reader(running.stdout)?;
    let stderr = join_reader(running.stderr)?;
    Ok(OutputSummary {
        stdout_bytes: stdout.bytes.len(),
        stderr_bytes: stderr.bytes.len(),
        stdout_overflowed: stdout.overflowed,
        stderr_overflowed: stderr.overflowed,
    })
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
pub enum MicYouError {
    #[error("MicYou executable path is empty")]
    EmptyExecutablePath,
    #[error("MicYou requires an explicit non-unspecified IPv4 bind address")]
    InvalidBindAddress,
    #[error("MicYou TCP port {port} cannot reserve its following UDP port")]
    InvalidPort { port: u16 },
    #[error("invalid CapyIO media binding for MicYou: {0}")]
    InvalidMediaBinding(String),
    #[error("MicYou compatibility binding accepts only the conservative voice stream mapping")]
    UnsupportedMediaSpec,
    #[error("MicYou output device ID is empty")]
    EmptyDeviceId,
    #[error("MicYou output device ID is {actual} bytes; limit is {limit}")]
    DeviceIdTooLong { actual: usize, limit: usize },
    #[error("MicYou output device ID contains control characters")]
    InvalidDeviceId,
    #[error("MicYou output device name is empty")]
    EmptyDeviceName,
    #[error("MicYou output device name is {actual} bytes; limit is {limit}")]
    DeviceNameTooLong { actual: usize, limit: usize },
    #[error("MicYou output device name contains control characters")]
    InvalidDeviceName,
    #[error("MicYou probe deadline is invalid")]
    InvalidProbeDeadline,
    #[error("MicYou probe output limit is invalid")]
    InvalidProbeOutputLimit,
    #[error("MicYou probe line limit is invalid")]
    InvalidProbeLineLimit,
    #[error("MicYou probe timed out")]
    ProbeTimedOut,
    #[error("MicYou probe output exceeded its bound")]
    ProbeOutputTooLarge,
    #[error("MicYou probe line exceeded its bound")]
    ProbeLineTooLong,
    #[error("MicYou probe output is not UTF-8")]
    NonUtf8ProbeOutput,
    #[error("MicYou version output is malformed")]
    MalformedVersionOutput,
    #[error("MicYou version {actual} is unsupported; expected {PINNED_MICYOU_VERSION}")]
    UnsupportedVersion { actual: String },
    #[error("MicYou CLI lacks required capability {REQUIRED_MICYOU_CAPABILITY}")]
    RequiredCapabilityMissing,
    #[error("MicYou device output is malformed")]
    MalformedDeviceOutput,
    #[error("MicYou output device inventory exceeds its bound")]
    TooManyDevices,
    #[error("MicYou output device inventory contains a duplicate stable ID")]
    DuplicateDeviceId,
    #[error("the configured MicYou output device ID is not present")]
    ConfiguredDeviceMissing,
    #[error("the configured MicYou output device name changed for its stable ID")]
    ConfiguredDeviceChanged,
    #[error("MicYou probe failed with exit code {exit_code:?}")]
    ProbeFailed { exit_code: Option<i32> },
    #[error("could not spawn MicYou: {0}")]
    Spawn(#[source] io::Error),
    #[error("could not wait for MicYou: {0}")]
    Wait(#[source] io::Error),
    #[error("could not read MicYou output: {0}")]
    Read(#[source] io::Error),
    #[error("MicYou child did not expose its output pipes")]
    MissingProcessPipe,
    #[error("MicYou output reader panicked")]
    ReaderPanicked,
    #[error("MicYou supervisor startup deadline is invalid")]
    InvalidStartupDeadline,
    #[error("MicYou retained output limit is invalid")]
    InvalidRetainedOutputLimit,
    #[error("MicYou supervisor is already running")]
    AlreadyRunning,
    #[error("MicYou exited before its TCP listener became ready")]
    ExitedBeforeReady,
    #[error("MicYou TCP listener did not become ready before the deadline")]
    StartupTimedOut,
    #[error("Windows TCP owner table query failed with code {code}")]
    PeerTableQueryFailed { code: u32 },
    #[error("Windows TCP owner table exceeded the {limit} byte safety bound")]
    PeerTableTooLarge { limit: usize },
    #[error("Windows TCP owner table returned an invalid bounded layout")]
    InvalidPeerTableLayout,
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use capyio_audio::AudioUseCase;

    use super::*;

    fn config(device: &str) -> MicYouConfig {
        MicYouConfig::new(
            "micyou-cli.exe",
            IpAddr::V4(Ipv4Addr::new(100, 66, 157, 119)),
            DEFAULT_MICYOU_PORT,
            "{0.0.0.00000000}.{capyio-ingress}",
            device,
        )
        .expect("valid config")
    }

    fn media_binding(spec: AudioStreamSpec) -> AudioMediaStreamBinding {
        AudioMediaStreamBinding {
            session_id: "00000000-0000-4000-8000-00000000b101".parse().unwrap(),
            route_id: "00000000-0000-4000-8000-00000000b102".parse().unwrap(),
            stream_id: "00000000-0000-4000-8000-00000000b103".parse().unwrap(),
            stream_epoch: 4,
            selected_spec: spec,
        }
    }

    #[test]
    fn compatibility_contract_is_opaque_adapter_managed_and_insecure() {
        let contract = micyou_compatibility_contract()
            .validate()
            .expect("contract");
        assert_eq!(
            contract.media_access,
            AudioTransportMediaAccess::OpaqueProcess
        );
        assert_eq!(
            contract.interoperability,
            AudioTransportInteroperability::AdapterManaged
        );
        assert!(contract.encodings.pcm);
        assert!(contract.encodings.opus);
        assert_eq!(
            contract.metadata.payload,
            AudioTransportFieldFidelity::Opaque
        );
        assert_eq!(
            contract.metadata.session_route_binding,
            AudioTransportFieldFidelity::Absent
        );
        assert!(!contract.security.meets_production_baseline());
    }

    #[test]
    fn opaque_process_binding_accepts_only_conservative_voice_semantics() {
        let config = config("CapyIO Microphone Ingress");
        let voice = media_binding(AudioStreamSpec::voice_interactive());
        let bound = config
            .bind_media_stream(voice.clone())
            .expect("voice binding");
        assert_eq!(bound.binding(), &voice);

        let speaker = media_binding(AudioStreamSpec::media_balanced());
        assert!(matches!(
            config.bind_media_stream(speaker),
            Err(MicYouError::UnsupportedMediaSpec)
        ));
    }

    #[test]
    fn config_builds_explicit_wifi_arguments_and_voice_mapping() {
        let config = config("CABLE Input (VB-Audio Virtual Cable)");
        assert_eq!(
            config
                .serve_args(&MicYouOutputDevice {
                    index: NonZeroUsize::new(2).expect("non-zero"),
                    id: "{0.0.0.00000000}.{capyio-ingress}".to_owned(),
                    name: "CABLE Input (VB-Audio Virtual Cable)".to_owned(),
                })
                .expect("matching selection"),
            [
                "serve",
                "--port",
                "8554",
                "--mode",
                "wifi",
                "--device",
                "CABLE Input (VB-Audio Virtual Cable)",
                "--device-id",
                "{0.0.0.00000000}.{capyio-ingress}",
                "--device-index",
                "2",
                "--bind",
                "100.66.157.119",
            ]
        );
        assert_eq!(
            config.audio_stream_spec().qos.use_case,
            AudioUseCase::VoiceInteractive
        );
    }

    #[test]
    fn config_rejects_implicit_network_and_unbounded_device_values() {
        assert!(matches!(
            MicYouConfig::new(
                "micyou-cli",
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                8554,
                "device-id",
                "device"
            ),
            Err(MicYouError::InvalidBindAddress)
        ));
        assert!(matches!(
            MicYouConfig::new(
                "micyou-cli",
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                65535,
                "device-id",
                "device"
            ),
            Err(MicYouError::InvalidPort { .. })
        ));
        assert!(matches!(
            MicYouConfig::new(
                "micyou-cli",
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                8554,
                "",
                "device"
            ),
            Err(MicYouError::EmptyDeviceId)
        ));
        assert!(matches!(
            MicYouConfig::new(
                "micyou-cli",
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                8554,
                " device-id ",
                "device"
            ),
            Err(MicYouError::InvalidDeviceId)
        ));
        assert!(matches!(
            MicYouConfig::new(
                "micyou-cli",
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                8554,
                "device-id",
                "bad\ndevice"
            ),
            Err(MicYouError::InvalidDeviceName)
        ));
    }

    #[test]
    fn parses_exact_version_and_bounded_device_inventory() {
        assert_eq!(
            parse_version_output(b"micyou-cli 2.0.1\n").expect("version"),
            PINNED_MICYOU_VERSION
        );
        let devices = parse_devices_output(
            b"audio output devices v2:\n  1. cable-id\tCABLE Input\n  2. capyio-id\tCapyIO Microphone Ingress\n",
            ProbeLimits::default(),
        )
        .expect("devices");
        assert_eq!(devices[0].index.get(), 1);
        assert_eq!(devices[0].id, "cable-id");
        assert_eq!(devices[0].name, "CABLE Input");
        assert_eq!(devices[1].index.get(), 2);
        assert_eq!(devices[1].name, "CapyIO Microphone Ingress");
        parse_capabilities_output(b"device-stable-id-v1\n").expect("capability");
        assert!(matches!(
            parse_capabilities_output(b"other\n"),
            Err(MicYouError::RequiredCapabilityMissing)
        ));
    }

    #[test]
    fn parser_preserves_duplicate_names_and_rejects_wrong_shape() {
        assert!(matches!(
            parse_version_output(b"micyou-cli 2.0.1 extra\n"),
            Err(MicYouError::MalformedVersionOutput)
        ));
        let duplicates = parse_devices_output(
            b"audio output devices v2:\n  1. first-id\tsame\n  2. second-id\tsame\n",
            ProbeLimits::default(),
        )
        .expect("duplicate display names remain addressable by index");
        assert_eq!(duplicates.len(), 2);
        assert!(matches!(
            parse_devices_output(
                b"audio output devices v2:\n  2. skipped-id\tskipped\n",
                ProbeLimits::default()
            ),
            Err(MicYouError::MalformedDeviceOutput)
        ));
    }

    #[test]
    fn inventory_resolves_stable_id_after_reorder_and_requires_expected_name() {
        let inventory = MicYouInventory {
            version: PINNED_MICYOU_VERSION.to_owned(),
            output_devices: parse_devices_output(
                b"audio output devices v2:\n  1. other-id\tSpeakers\n  2. {0.0.0.00000000}.{capyio-ingress}\tSpeakers\n",
                ProbeLimits::default(),
            )
            .expect("inventory"),
        };
        let selected = inventory
            .resolve_configured_device(&config("Speakers"))
            .expect("stable ID selected");
        assert_eq!(selected.index.get(), 2);
        let inconsistent = MicYouOutputDevice {
            index: selected.index,
            id: "other-id".to_owned(),
            name: selected.name.clone(),
        };
        assert!(matches!(
            config("Speakers").serve_args(&inconsistent),
            Err(MicYouError::ConfiguredDeviceChanged)
        ));

        let reordered = MicYouInventory {
            version: PINNED_MICYOU_VERSION.to_owned(),
            output_devices: parse_devices_output(
                b"audio output devices v2:\n  1. {0.0.0.00000000}.{capyio-ingress}\tSpeakers\n  2. other-id\tSpeakers\n",
                ProbeLimits::default(),
            )
            .expect("reordered inventory"),
        };
        assert_eq!(
            reordered
                .resolve_configured_device(&config("Speakers"))
                .expect("same stable ID")
                .index
                .get(),
            1
        );

        let changed = MicYouConfig::new(
            "micyou-cli.exe",
            IpAddr::V4(Ipv4Addr::new(100, 66, 157, 119)),
            DEFAULT_MICYOU_PORT,
            "{0.0.0.00000000}.{capyio-ingress}",
            "Different Speakers",
        )
        .expect("changed config");
        assert!(matches!(
            inventory.resolve_configured_device(&changed),
            Err(MicYouError::ConfiguredDeviceChanged)
        ));
        let missing = MicYouConfig::new(
            "micyou-cli.exe",
            IpAddr::V4(Ipv4Addr::new(100, 66, 157, 119)),
            DEFAULT_MICYOU_PORT,
            "missing-id",
            "Speakers",
        )
        .expect("missing config is structurally valid");
        assert!(matches!(
            inventory.resolve_configured_device(&missing),
            Err(MicYouError::ConfiguredDeviceMissing)
        ));

        assert!(matches!(
            parse_devices_output(
                b"audio output devices v2:\n  1. duplicate\tfirst\n  2. duplicate\tsecond\n",
                ProbeLimits::default()
            ),
            Err(MicYouError::DuplicateDeviceId)
        ));
    }
}
