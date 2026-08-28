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

use capyio_audio::AudioStreamSpec;
use thiserror::Error;

pub const PINNED_MICYOU_VERSION: &str = "2.0.1";
pub const REQUIRED_MICYOU_CAPABILITY: &str = "device-index-v1";
pub const DEFAULT_MICYOU_PORT: u16 = 8554;
pub const MAX_OUTPUT_DEVICES: usize = 64;
pub const MAX_DEVICE_NAME_BYTES: usize = 512;
pub const MAX_PROBE_OUTPUT_BYTES: usize = 64 * 1024;
pub const MAX_PROBE_LINE_BYTES: usize = 1024;

#[derive(Clone, Eq, PartialEq)]
pub struct MicYouConfig {
    executable: PathBuf,
    bind_address: SocketAddr,
    output_device_index: NonZeroUsize,
    output_device: String,
}

impl fmt::Debug for MicYouConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MicYouConfig")
            .field("executable", &"<redacted>")
            .field("bind_address", &"<redacted>")
            .field("output_device_index", &self.output_device_index)
            .field("output_device", &"<redacted>")
            .finish()
    }
}

impl MicYouConfig {
    pub fn new(
        executable: impl Into<PathBuf>,
        bind_ip: IpAddr,
        port: u16,
        output_device_index: usize,
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
        let output_device_index =
            NonZeroUsize::new(output_device_index).ok_or(MicYouError::InvalidDeviceIndex)?;
        if output_device_index.get() > MAX_OUTPUT_DEVICES {
            return Err(MicYouError::InvalidDeviceIndex);
        }
        let output_device = output_device.into();
        validate_device_name(&output_device)?;
        Ok(Self {
            executable,
            bind_address: SocketAddr::new(bind_ip, port),
            output_device_index,
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

    pub const fn output_device_index(&self) -> NonZeroUsize {
        self.output_device_index
    }

    pub fn serve_args(&self) -> Vec<String> {
        vec![
            "serve".to_owned(),
            "--port".to_owned(),
            self.bind_address.port().to_string(),
            "--mode".to_owned(),
            "wifi".to_owned(),
            "--device".to_owned(),
            self.output_device.clone(),
            "--device-index".to_owned(),
            self.output_device_index.to_string(),
            "--bind".to_owned(),
            self.bind_address.ip().to_string(),
        ]
    }

    /// Semantic mapping only. MicYou's private negotiated wire is not a
    /// `capyio.audio.frames/1` StandardPort.
    pub fn audio_stream_spec(&self) -> AudioStreamSpec {
        AudioStreamSpec::voice_interactive()
    }
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
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MicYouInventory {
    pub version: String,
    pub output_devices: Vec<MicYouOutputDevice>,
}

impl MicYouInventory {
    pub fn require_configured_device(&self, config: &MicYouConfig) -> Result<(), MicYouError> {
        let device = self
            .output_devices
            .get(config.output_device_index().get() - 1)
            .ok_or(MicYouError::ConfiguredDeviceMissing)?;
        if device.index == config.output_device_index() && device.name == config.output_device() {
            Ok(())
        } else {
            Err(MicYouError::ConfiguredDeviceChanged)
        }
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
        inventory.require_configured_device(config)?;
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
    if lines.next().map(str::trim) != Some("audio output devices:") {
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
        let Some((index, name)) = trimmed.split_once(". ") else {
            return Err(MicYouError::MalformedDeviceOutput);
        };
        let index = index
            .parse::<usize>()
            .map_err(|_| MicYouError::MalformedDeviceOutput)?;
        if index != devices.len() + 1 {
            return Err(MicYouError::MalformedDeviceOutput);
        }
        validate_device_name(name)?;
        devices.push(MicYouOutputDevice {
            index: NonZeroUsize::new(index).ok_or(MicYouError::MalformedDeviceOutput)?,
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
        MicYouProbe::new(self.probe_limits)?.probe_config(&self.config)?;
        self.terminal_exit = None;
        let mut command = Command::new(self.config.executable());
        command
            .args(self.config.serve_args())
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
    #[error("MicYou output device index must be within the bounded one-based inventory")]
    InvalidDeviceIndex,
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
    #[error("the configured MicYou output device index is not present")]
    ConfiguredDeviceMissing,
    #[error("the configured MicYou output device inventory entry changed")]
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
            2,
            device,
        )
        .expect("valid config")
    }

    #[test]
    fn config_builds_explicit_wifi_arguments_and_voice_mapping() {
        let config = config("CABLE Input (VB-Audio Virtual Cable)");
        assert_eq!(
            config.serve_args(),
            [
                "serve",
                "--port",
                "8554",
                "--mode",
                "wifi",
                "--device",
                "CABLE Input (VB-Audio Virtual Cable)",
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
                1,
                "device"
            ),
            Err(MicYouError::InvalidBindAddress)
        ));
        assert!(matches!(
            MicYouConfig::new(
                "micyou-cli",
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                65535,
                1,
                "device"
            ),
            Err(MicYouError::InvalidPort { .. })
        ));
        assert!(matches!(
            MicYouConfig::new(
                "micyou-cli",
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                8554,
                0,
                "device"
            ),
            Err(MicYouError::InvalidDeviceIndex)
        ));
        assert!(matches!(
            MicYouConfig::new(
                "micyou-cli",
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                8554,
                1,
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
            b"audio output devices:\n  1. CABLE Input\n  2. CapyIO Microphone Ingress\n",
            ProbeLimits::default(),
        )
        .expect("devices");
        assert_eq!(devices[0].index.get(), 1);
        assert_eq!(devices[0].name, "CABLE Input");
        assert_eq!(devices[1].index.get(), 2);
        assert_eq!(devices[1].name, "CapyIO Microphone Ingress");
        parse_capabilities_output(b"device-index-v1\n").expect("capability");
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
            b"audio output devices:\n  1. same\n  2. same\n",
            ProbeLimits::default(),
        )
        .expect("duplicate display names remain addressable by index");
        assert_eq!(duplicates.len(), 2);
        assert!(matches!(
            parse_devices_output(
                b"audio output devices:\n  2. skipped\n",
                ProbeLimits::default()
            ),
            Err(MicYouError::MalformedDeviceOutput)
        ));
    }

    #[test]
    fn inventory_requires_the_exact_index_and_expected_name() {
        let inventory = MicYouInventory {
            version: PINNED_MICYOU_VERSION.to_owned(),
            output_devices: parse_devices_output(
                b"audio output devices:\n  1. Speakers\n  2. Speakers\n",
                ProbeLimits::default(),
            )
            .expect("inventory"),
        };
        inventory
            .require_configured_device(&config("Speakers"))
            .expect("second duplicate selected");

        let changed = MicYouConfig::new(
            "micyou-cli.exe",
            IpAddr::V4(Ipv4Addr::new(100, 66, 157, 119)),
            DEFAULT_MICYOU_PORT,
            2,
            "Different Speakers",
        )
        .expect("changed config");
        assert!(matches!(
            inventory.require_configured_device(&changed),
            Err(MicYouError::ConfiguredDeviceChanged)
        ));
    }
}
