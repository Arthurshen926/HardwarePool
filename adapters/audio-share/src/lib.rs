//! Bounded probe and configuration boundary for Audio Share v0.3.4.
//!
//! The upstream process retains its own TCP/UDP PCM data plane. This crate
//! invokes only direct executable paths and never routes media through a shell
//! or CapyIO control message.

use std::{
    fmt,
    io::{self, Read},
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use thiserror::Error;

mod peer_presence;
mod supervisor;
mod transport;

pub use peer_presence::ReceiverTcpPresence;
pub use supervisor::{
    AudioShareSupervisor, ProcessExitReport, ProcessOutputSummary, SupervisorLimits,
    SupervisorStartReport, SupervisorStatus, SupervisorStopReport,
};
pub use transport::{
    AudioSharePrivateFormat, AudioShareTransport, AudioShareTransportConfig,
    AudioShareTransportError, AudioShareTransportSender, AudioShareTransportStats,
};

pub const PINNED_AUDIO_SHARE_VERSION: &str = "0.3.4";
pub const DEFAULT_PROBE_OUTPUT_LIMIT: usize = 64 * 1024;
pub const DEFAULT_PROBE_LINE_LIMIT: usize = 1024;
pub const MAX_ENDPOINTS: usize = 64;
pub const MAX_ENDPOINT_ID_BYTES: usize = 256;
pub const MAX_ENDPOINT_NAME_BYTES: usize = 512;
pub const MIN_SAMPLE_RATE: u32 = 8_000;
pub const MAX_SAMPLE_RATE: u32 = 192_000;
pub const MAX_CHANNELS: u16 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioEncoding {
    F32,
    S8,
    S16,
    S24,
    S32,
}

impl AudioEncoding {
    pub const fn cli_value(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::S8 => "s8",
            Self::S16 => "s16",
            Self::S24 => "s24",
            Self::S32 => "s32",
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct AudioShareConfig {
    executable: PathBuf,
    bind_address: SocketAddr,
    endpoint_id: String,
    encoding: AudioEncoding,
    channels: u16,
    sample_rate: u32,
}

impl fmt::Debug for AudioShareConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AudioShareConfig")
            .field("executable", &"<redacted>")
            .field("bind_address", &"<redacted>")
            .field("endpoint_id", &"<redacted>")
            .field("encoding", &self.encoding)
            .field("channels", &self.channels)
            .field("sample_rate", &self.sample_rate)
            .finish()
    }
}

impl AudioShareConfig {
    pub fn new(
        executable: impl Into<PathBuf>,
        bind_ip: IpAddr,
        port: u16,
        endpoint_id: impl Into<String>,
        encoding: AudioEncoding,
        channels: u16,
        sample_rate: u32,
    ) -> Result<Self, AudioShareError> {
        let executable = executable.into();
        if executable.as_os_str().is_empty() {
            return Err(AudioShareError::EmptyExecutablePath);
        }
        if port == 0 {
            return Err(AudioShareError::ZeroPort);
        }

        let endpoint_id = endpoint_id.into();
        validate_endpoint_id(&endpoint_id)?;
        if !(1..=MAX_CHANNELS).contains(&channels) {
            return Err(AudioShareError::InvalidChannels { channels });
        }
        if !(MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&sample_rate) {
            return Err(AudioShareError::InvalidSampleRate { sample_rate });
        }

        Ok(Self {
            executable,
            bind_address: SocketAddr::new(bind_ip, port),
            endpoint_id,
            encoding,
            channels,
            sample_rate,
        })
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub const fn bind_address(&self) -> SocketAddr {
        self.bind_address
    }

    pub fn endpoint_id(&self) -> &str {
        &self.endpoint_id
    }

    pub fn server_args(&self) -> Vec<String> {
        vec![
            format!("--bind={}", self.bind_address),
            format!("--endpoint={}", self.endpoint_id),
            format!("--encoding={}", self.encoding.cli_value()),
            format!("--channels={}", self.channels),
            format!("--sample-rate={}", self.sample_rate),
        ]
    }
}

fn validate_endpoint_id(endpoint_id: &str) -> Result<(), AudioShareError> {
    if endpoint_id.is_empty() {
        return Err(AudioShareError::EmptyEndpointId);
    }
    if endpoint_id.len() > MAX_ENDPOINT_ID_BYTES {
        return Err(AudioShareError::EndpointIdTooLong {
            actual: endpoint_id.len(),
            limit: MAX_ENDPOINT_ID_BYTES,
        });
    }
    if endpoint_id
        .chars()
        .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(AudioShareError::InvalidEndpointId);
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackEndpoint {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioShareInventory {
    pub version: String,
    pub endpoints: Vec<PlaybackEndpoint>,
}

impl AudioShareInventory {
    pub fn configured_endpoint(
        &self,
        config: &AudioShareConfig,
    ) -> Result<&PlaybackEndpoint, AudioShareError> {
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.id == config.endpoint_id)
            .ok_or_else(|| AudioShareError::ConfiguredEndpointMissing {
                endpoint_id: config.endpoint_id.clone(),
            })
    }
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
            output_bytes: DEFAULT_PROBE_OUTPUT_LIMIT,
            line_bytes: DEFAULT_PROBE_LINE_LIMIT,
        }
    }
}

impl ProbeLimits {
    pub fn validate(self) -> Result<Self, AudioShareError> {
        if self.deadline.is_zero() || self.deadline > Duration::from_secs(30) {
            return Err(AudioShareError::InvalidProbeDeadline);
        }
        if self.output_bytes == 0 || self.output_bytes > DEFAULT_PROBE_OUTPUT_LIMIT {
            return Err(AudioShareError::InvalidProbeOutputLimit);
        }
        if self.line_bytes == 0 || self.line_bytes > DEFAULT_PROBE_LINE_LIMIT {
            return Err(AudioShareError::InvalidProbeLineLimit);
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbeCommandOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub trait ProbeCommandRunner {
    fn run(
        &self,
        executable: &Path,
        args: &[&str],
        limits: ProbeLimits,
    ) -> Result<ProbeCommandOutput, AudioShareError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemProbeCommandRunner;

impl ProbeCommandRunner for SystemProbeCommandRunner {
    fn run(
        &self,
        executable: &Path,
        args: &[&str],
        limits: ProbeLimits,
    ) -> Result<ProbeCommandOutput, AudioShareError> {
        let limits = limits.validate()?;
        let operation = args.join(" ");
        let mut command = Command::new(executable);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_hidden_process(&mut command);

        let mut child = command
            .spawn()
            .map_err(|source| AudioShareError::ProcessSpawn {
                operation: operation.clone(),
                source,
            })?;
        let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AudioShareError::MissingProcessPipe);
        };
        let output_limit = limits.output_bytes;
        let stdout_reader = thread::spawn(move || read_bounded(stdout, output_limit));
        let stderr_reader = thread::spawn(move || read_bounded(stderr, output_limit));

        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() < limits.deadline => {
                    thread::sleep(Duration::from_millis(10));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = join_reader(stdout_reader, "stdout");
                    let _ = join_reader(stderr_reader, "stderr");
                    return Err(AudioShareError::ProcessTimedOut { operation });
                }
                Err(source) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = join_reader(stdout_reader, "stdout");
                    let _ = join_reader(stderr_reader, "stderr");
                    return Err(AudioShareError::ProcessWait { operation, source });
                }
            }
        };

        let stdout = join_reader(stdout_reader, "stdout")?;
        let stderr = join_reader(stderr_reader, "stderr")?;
        if stdout.overflowed {
            return Err(AudioShareError::ProcessOutputTooLarge {
                stream: "stdout",
                limit: limits.output_bytes,
            });
        }
        if stderr.overflowed {
            return Err(AudioShareError::ProcessOutputTooLarge {
                stream: "stderr",
                limit: limits.output_bytes,
            });
        }
        if !status.success() {
            return Err(AudioShareError::ProcessFailed {
                operation,
                exit_code: status.code(),
                stderr: sanitize_diagnostic(&stderr.bytes),
            });
        }

        Ok(ProbeCommandOutput {
            stdout: stdout.bytes,
            stderr: stderr.bytes,
        })
    }
}

#[cfg(windows)]
pub(crate) fn configure_hidden_process(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub(crate) fn configure_hidden_process(_command: &mut Command) {}

#[derive(Debug)]
pub(crate) struct BoundedRead {
    pub(crate) bytes: Vec<u8>,
    pub(crate) overflowed: bool,
}

pub(crate) fn read_bounded(mut reader: impl Read, limit: usize) -> io::Result<BoundedRead> {
    let mut bytes = Vec::with_capacity(limit.min(4096));
    let mut overflowed = false;
    let mut chunk = [0_u8; 4096];
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        let retained = remaining.min(read);
        bytes.extend_from_slice(&chunk[..retained]);
        overflowed |= retained < read;
    }
    Ok(BoundedRead { bytes, overflowed })
}

pub(crate) fn join_reader(
    reader: thread::JoinHandle<io::Result<BoundedRead>>,
    stream: &'static str,
) -> Result<BoundedRead, AudioShareError> {
    reader
        .join()
        .map_err(|_| AudioShareError::ReaderThreadPanicked { stream })?
        .map_err(|source| AudioShareError::ProcessRead { stream, source })
}

fn sanitize_diagnostic(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .map(|character| {
            if character == '\n' || character == '\t' || !character.is_control() {
                character
            } else {
                '\u{fffd}'
            }
        })
        .collect()
}

pub struct AudioShareProbe<R = SystemProbeCommandRunner> {
    runner: R,
    limits: ProbeLimits,
}

impl AudioShareProbe<SystemProbeCommandRunner> {
    pub fn new(limits: ProbeLimits) -> Result<Self, AudioShareError> {
        Ok(Self {
            runner: SystemProbeCommandRunner,
            limits: limits.validate()?,
        })
    }
}

impl<R: ProbeCommandRunner> AudioShareProbe<R> {
    pub fn with_runner(runner: R, limits: ProbeLimits) -> Result<Self, AudioShareError> {
        Ok(Self {
            runner,
            limits: limits.validate()?,
        })
    }

    pub fn inventory(&self, executable: &Path) -> Result<AudioShareInventory, AudioShareError> {
        if executable.as_os_str().is_empty() {
            return Err(AudioShareError::EmptyExecutablePath);
        }
        let version_output = self.runner.run(executable, &["--version"], self.limits)?;
        let version = parse_version_output(&version_output.stdout, self.limits)?;
        if version != PINNED_AUDIO_SHARE_VERSION {
            return Err(AudioShareError::UnsupportedVersion {
                expected: PINNED_AUDIO_SHARE_VERSION,
                actual: version,
            });
        }

        let endpoint_output = self
            .runner
            .run(executable, &["--list-endpoint"], self.limits)?;
        let endpoints = parse_endpoint_output(&endpoint_output.stdout, self.limits)?;
        Ok(AudioShareInventory { version, endpoints })
    }

    pub fn probe_config(
        &self,
        config: &AudioShareConfig,
    ) -> Result<AudioShareInventory, AudioShareError> {
        let inventory = self.inventory(config.executable())?;
        inventory.configured_endpoint(config)?;
        Ok(inventory)
    }
}

pub fn parse_version_output(output: &[u8], limits: ProbeLimits) -> Result<String, AudioShareError> {
    let limits = limits.validate()?;
    let text = bounded_utf8(output, limits, "version")?;
    let mut version = None;
    for line in text.lines() {
        check_line_bound(line, limits)?;
        if let Some(value) = line.trim().strip_prefix("version:") {
            let value = value.trim();
            if value.is_empty() || version.replace(value.to_owned()).is_some() {
                return Err(AudioShareError::MalformedVersionOutput);
            }
        }
    }
    version.ok_or(AudioShareError::MalformedVersionOutput)
}

pub fn parse_endpoint_output(
    output: &[u8],
    limits: ProbeLimits,
) -> Result<Vec<PlaybackEndpoint>, AudioShareError> {
    let limits = limits.validate()?;
    if output.len() > limits.output_bytes {
        return Err(AudioShareError::ParserOutputTooLarge {
            operation: "endpoint list",
            actual: output.len(),
            limit: limits.output_bytes,
        });
    }
    let text = String::from_utf8_lossy(output);
    let mut endpoints = Vec::new();
    let mut declared_total = None;
    let mut default_seen = false;

    for line in text.lines() {
        check_line_bound(line, limits)?;
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("total:") {
            if declared_total.is_some() {
                return Err(AudioShareError::MalformedEndpointOutput);
            }
            declared_total = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| AudioShareError::MalformedEndpointOutput)?,
            );
            continue;
        }

        let (is_default, rest) = if let Some(rest) = trimmed.strip_prefix("* id: ") {
            (true, rest)
        } else if let Some(rest) = trimmed.strip_prefix("id: ") {
            (false, rest)
        } else {
            continue;
        };
        if endpoints.len() == MAX_ENDPOINTS {
            return Err(AudioShareError::TooManyEndpoints {
                limit: MAX_ENDPOINTS,
            });
        }
        if is_default && std::mem::replace(&mut default_seen, true) {
            return Err(AudioShareError::MultipleDefaultEndpoints);
        }
        let (id, name) = rest
            .split_once(" name: ")
            .ok_or(AudioShareError::MalformedEndpointOutput)?;
        validate_endpoint_id(id)?;
        if name.is_empty() || name.len() > MAX_ENDPOINT_NAME_BYTES {
            return Err(AudioShareError::InvalidEndpointName);
        }
        if endpoints
            .iter()
            .any(|endpoint: &PlaybackEndpoint| endpoint.id == id)
        {
            return Err(AudioShareError::DuplicateEndpointId {
                endpoint_id: id.to_owned(),
            });
        }
        endpoints.push(PlaybackEndpoint {
            id: id.to_owned(),
            name: name.to_owned(),
            is_default,
        });
    }

    let declared_total = declared_total.ok_or(AudioShareError::MalformedEndpointOutput)?;
    if declared_total != endpoints.len() {
        return Err(AudioShareError::EndpointCountMismatch {
            declared: declared_total,
            parsed: endpoints.len(),
        });
    }
    if endpoints.is_empty() {
        return Err(AudioShareError::NoPlaybackEndpoints);
    }
    Ok(endpoints)
}

fn bounded_utf8<'a>(
    output: &'a [u8],
    limits: ProbeLimits,
    operation: &'static str,
) -> Result<&'a str, AudioShareError> {
    if output.len() > limits.output_bytes {
        return Err(AudioShareError::ParserOutputTooLarge {
            operation,
            actual: output.len(),
            limit: limits.output_bytes,
        });
    }
    std::str::from_utf8(output).map_err(|_| AudioShareError::NonUtf8VersionOutput)
}

fn check_line_bound(line: &str, limits: ProbeLimits) -> Result<(), AudioShareError> {
    if line.len() > limits.line_bytes {
        return Err(AudioShareError::ProbeLineTooLong {
            actual: line.len(),
            limit: limits.line_bytes,
        });
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum AudioShareError {
    #[error("Audio Share executable path is empty")]
    EmptyExecutablePath,
    #[error("Audio Share bind port must be non-zero")]
    ZeroPort,
    #[error("Audio Share endpoint id is empty")]
    EmptyEndpointId,
    #[error("Audio Share endpoint id is {actual} bytes; limit is {limit}")]
    EndpointIdTooLong { actual: usize, limit: usize },
    #[error("Audio Share endpoint id contains whitespace or control characters")]
    InvalidEndpointId,
    #[error("Audio Share channel count {channels} is outside 1..={MAX_CHANNELS}")]
    InvalidChannels { channels: u16 },
    #[error(
        "Audio Share sample rate {sample_rate} is outside {MIN_SAMPLE_RATE}..={MAX_SAMPLE_RATE}"
    )]
    InvalidSampleRate { sample_rate: u32 },
    #[error("Audio Share probe deadline must be greater than zero and at most 30 seconds")]
    InvalidProbeDeadline,
    #[error("Audio Share probe output limit is invalid")]
    InvalidProbeOutputLimit,
    #[error("Audio Share probe line limit is invalid")]
    InvalidProbeLineLimit,
    #[error("could not spawn Audio Share operation {operation}: {source}")]
    ProcessSpawn {
        operation: String,
        #[source]
        source: io::Error,
    },
    #[error("Audio Share child did not expose the requested pipe")]
    MissingProcessPipe,
    #[error("could not wait for Audio Share operation {operation}: {source}")]
    ProcessWait {
        operation: String,
        #[source]
        source: io::Error,
    },
    #[error("Audio Share operation {operation} timed out")]
    ProcessTimedOut { operation: String },
    #[error("could not read Audio Share {stream}: {source}")]
    ProcessRead {
        stream: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("Audio Share {stream} reader thread panicked")]
    ReaderThreadPanicked { stream: &'static str },
    #[error("Audio Share {stream} exceeded {limit} bytes")]
    ProcessOutputTooLarge { stream: &'static str, limit: usize },
    #[error("Audio Share operation {operation} failed with {exit_code:?}: {stderr}")]
    ProcessFailed {
        operation: String,
        exit_code: Option<i32>,
        stderr: String,
    },
    #[error("Audio Share {operation} output is {actual} bytes; limit is {limit}")]
    ParserOutputTooLarge {
        operation: &'static str,
        actual: usize,
        limit: usize,
    },
    #[error("Audio Share probe line is {actual} bytes; limit is {limit}")]
    ProbeLineTooLong { actual: usize, limit: usize },
    #[error("Audio Share version output is not UTF-8")]
    NonUtf8VersionOutput,
    #[error("Audio Share version output is malformed")]
    MalformedVersionOutput,
    #[error("Audio Share version {actual} is unsupported; expected {expected}")]
    UnsupportedVersion {
        expected: &'static str,
        actual: String,
    },
    #[error("Audio Share endpoint output is malformed")]
    MalformedEndpointOutput,
    #[error("Audio Share endpoint name is empty or too long")]
    InvalidEndpointName,
    #[error("Audio Share endpoint list exceeds {limit} entries")]
    TooManyEndpoints { limit: usize },
    #[error("Audio Share endpoint id is duplicated")]
    DuplicateEndpointId { endpoint_id: String },
    #[error("Audio Share endpoint list contains multiple defaults")]
    MultipleDefaultEndpoints,
    #[error("Audio Share endpoint count declared {declared} but parsed {parsed}")]
    EndpointCountMismatch { declared: usize, parsed: usize },
    #[error("Audio Share reported no playback endpoints")]
    NoPlaybackEndpoints,
    #[error("configured Audio Share endpoint is not present")]
    ConfiguredEndpointMissing { endpoint_id: String },
    #[error("Audio Share supervisor is already running")]
    SupervisorAlreadyRunning,
    #[error("Audio Share supervisor startup deadline is invalid")]
    InvalidSupervisorStartupDeadline,
    #[error("Audio Share supervisor output limit is invalid")]
    InvalidSupervisorOutputLimit,
    #[error("Audio Share process exited before its TCP listener became ready")]
    SupervisorExitedBeforeReady { exit_code: Option<i32> },
    #[error("Audio Share TCP listener did not become ready before the startup deadline")]
    SupervisorStartupTimedOut,
    #[error("Windows TCP owner table query failed with code {code}")]
    PeerTableQueryFailed { code: u32 },
    #[error("Windows TCP owner table exceeded the {limit} byte safety bound")]
    PeerTableTooLarge { limit: usize },
    #[error("Windows TCP owner table returned an invalid bounded layout")]
    InvalidPeerTableLayout,
}

impl fmt::Debug for AudioShareProbe<SystemProbeCommandRunner> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AudioShareProbe")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, net::Ipv4Addr};

    use super::*;

    const ENDPOINT_A: &str = "{0.0.0.00000000}.{67b642a2-bada-4e3d-9479-0fa63c79c1e6}";
    const ENDPOINT_B: &str = "{0.0.0.00000000}.{6facbaa1-8747-4c9e-9f39-6f245028770e}";

    #[derive(Default)]
    struct FakeRunner {
        outputs: BTreeMap<String, ProbeCommandOutput>,
    }

    impl ProbeCommandRunner for FakeRunner {
        fn run(
            &self,
            _executable: &Path,
            args: &[&str],
            _limits: ProbeLimits,
        ) -> Result<ProbeCommandOutput, AudioShareError> {
            self.outputs.get(&args.join(" ")).cloned().ok_or_else(|| {
                AudioShareError::ProcessFailed {
                    operation: args.join(" "),
                    exit_code: Some(2),
                    stderr: "unexpected fake command".to_owned(),
                }
            })
        }
    }

    fn output(stdout: &str) -> ProbeCommandOutput {
        ProbeCommandOutput {
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    fn config(endpoint: &str) -> AudioShareConfig {
        AudioShareConfig::new(
            "as-cmd.exe",
            IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1)),
            65_530,
            endpoint,
            AudioEncoding::F32,
            2,
            44_100,
        )
        .expect("valid config")
    }

    #[test]
    fn config_builds_explicit_non_shell_arguments() {
        let config = config(ENDPOINT_A);
        assert_eq!(
            config.server_args(),
            [
                "--bind=100.64.0.1:65530",
                &format!("--endpoint={ENDPOINT_A}"),
                "--encoding=f32",
                "--channels=2",
                "--sample-rate=44100",
            ]
        );
    }

    #[test]
    fn config_rejects_implicit_or_unbounded_values() {
        assert!(matches!(
            AudioShareConfig::new(
                "as-cmd.exe",
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                0,
                ENDPOINT_A,
                AudioEncoding::F32,
                2,
                48_000
            ),
            Err(AudioShareError::ZeroPort)
        ));
        assert!(matches!(
            AudioShareConfig::new(
                "as-cmd.exe",
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                65_530,
                "default endpoint",
                AudioEncoding::F32,
                2,
                48_000
            ),
            Err(AudioShareError::InvalidEndpointId)
        ));
    }

    #[test]
    fn parses_pinned_version_and_bounded_endpoint_inventory() {
        let version = parse_version_output(
            b"as-cmd\nversion: 0.3.4\nurl: https://github.com/mkckr0/audio-share\n",
            ProbeLimits::default(),
        )
        .expect("version parses");
        assert_eq!(version, PINNED_AUDIO_SHARE_VERSION);

        let list = format!(
            "endpoint list:\n\t* id: {ENDPOINT_A} name: USB Audio\n\t  id: {ENDPOINT_B} name: Realtek Digital Output\ntotal: 2\n"
        );
        let endpoints = parse_endpoint_output(list.as_bytes(), ProbeLimits::default())
            .expect("endpoints parse");
        assert_eq!(endpoints.len(), 2);
        assert!(endpoints[0].is_default);
        assert_eq!(endpoints[1].id, ENDPOINT_B);
    }

    #[test]
    fn endpoint_parser_accepts_lossy_non_utf8_names_but_not_structure() {
        let mut output = format!("endpoint list:\n* id: {ENDPOINT_A} name: ").into_bytes();
        output.extend_from_slice(&[0xff, 0xfe]);
        output.extend_from_slice(b" speaker\ntotal: 1\n");
        let endpoints = parse_endpoint_output(&output, ProbeLimits::default())
            .expect("name is diagnostic display text");
        assert!(endpoints[0].name.contains('\u{fffd}'));
    }

    #[test]
    fn endpoint_parser_rejects_duplicates_count_mismatch_and_long_lines() {
        let duplicate =
            format!("* id: {ENDPOINT_A} name: first\nid: {ENDPOINT_A} name: second\ntotal: 2\n");
        assert!(matches!(
            parse_endpoint_output(duplicate.as_bytes(), ProbeLimits::default()),
            Err(AudioShareError::DuplicateEndpointId { .. })
        ));

        let mismatch = format!("* id: {ENDPOINT_A} name: first\ntotal: 2\n");
        assert!(matches!(
            parse_endpoint_output(mismatch.as_bytes(), ProbeLimits::default()),
            Err(AudioShareError::EndpointCountMismatch { .. })
        ));

        let limits = ProbeLimits {
            line_bytes: 16,
            ..ProbeLimits::default()
        };
        assert!(matches!(
            parse_endpoint_output(mismatch.as_bytes(), limits),
            Err(AudioShareError::ProbeLineTooLong { .. })
        ));
    }

    #[test]
    fn bounded_reader_drains_but_retains_only_the_limit() {
        let input = vec![42_u8; 10_000];
        let result = read_bounded(input.as_slice(), 128).expect("read succeeds");
        assert!(result.overflowed);
        assert_eq!(result.bytes.len(), 128);
    }

    #[test]
    fn probe_rejects_unpinned_version_and_missing_configured_endpoint() {
        let mut wrong_version = FakeRunner::default();
        wrong_version
            .outputs
            .insert("--version".to_owned(), output("version: 0.4.0\n"));
        let probe = AudioShareProbe::with_runner(wrong_version, ProbeLimits::default())
            .expect("probe limits");
        assert!(matches!(
            probe.inventory(Path::new("as-cmd.exe")),
            Err(AudioShareError::UnsupportedVersion { .. })
        ));

        let mut runner = FakeRunner::default();
        runner
            .outputs
            .insert("--version".to_owned(), output("version: 0.3.4\n"));
        runner.outputs.insert(
            "--list-endpoint".to_owned(),
            output(&format!("* id: {ENDPOINT_B} name: output\ntotal: 1\n")),
        );
        let probe =
            AudioShareProbe::with_runner(runner, ProbeLimits::default()).expect("probe limits");
        assert!(matches!(
            probe.probe_config(&config(ENDPOINT_A)),
            Err(AudioShareError::ConfiguredEndpointMissing { .. })
        ));
    }

    #[test]
    #[ignore = "requires a user-supplied, hash-verified Audio Share v0.3.4 executable"]
    fn probes_real_user_supplied_audio_share_cli() {
        let executable = std::env::var_os("CAPYIO_AUDIO_SHARE_EXE")
            .map(PathBuf::from)
            .expect("set CAPYIO_AUDIO_SHARE_EXE explicitly");
        let inventory = AudioShareProbe::new(ProbeLimits::default())
            .expect("probe limits")
            .inventory(&executable)
            .expect("real CLI probe");
        assert_eq!(inventory.version, PINNED_AUDIO_SHARE_VERSION);
        assert!(!inventory.endpoints.is_empty());
    }
}
