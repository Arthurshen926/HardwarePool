use std::{
    net::{IpAddr, Ipv4Addr, SocketAddrV4},
    path::PathBuf,
    time::Duration,
};

use capyio_audio_share_adapter::{
    AudioShareSupervisor, ReceiverTcpPresence, SupervisorLimits, SupervisorStatus,
};
use capyio_native_audio_lan::{
    NativeMicrophoneSupervisor, NativeSpeakerSupervisor, NativeSpeakerSupervisorError,
    NativeSpeakerSupervisorLimits, NativeSpeakerSupervisorStatus,
};
use serde::{Deserialize, Serialize};

#[cfg(windows)]
mod control;
#[cfg(windows)]
mod local_pipe;
mod microphone;
#[cfg(windows)]
mod microphone_control;

#[cfg(windows)]
pub use capyio_windows_capture_ring::{CaptureRingMetrics, CaptureRingOwner};
#[cfg(windows)]
pub use control::{BrokerServiceClient, control_server_loop, wake_control_server};
pub use microphone::{
    DEFAULT_PHONE_WAIT_POLLS, DEFAULT_STABLE_PHONE_POLLS, MicrophoneHostProcess,
    MicrophoneHostRuntime, MicrophoneHostSnapshot, MicrophoneHostStartError, MicrophoneHostState,
};
#[cfg(windows)]
pub use microphone_control::{
    MICROPHONE_CONTROL_PIPE_NAME, MicrophoneHostClient, microphone_control_server_loop,
    wake_microphone_control_server,
};

pub const SERVICE_NAME: &str = "CapyIOBroker";
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(250);
pub const DEFAULT_STABLE_RECEIVER_POLLS: u8 = 3;
const MAX_CONSOLE_RUN: Duration = Duration::from_secs(600);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceBrokerMode {
    AudioShareCompatibility,
    NativeSpeaker,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeMicrophoneServiceConfig {
    pub broker_executable: PathBuf,
    pub local: SocketAddrV4,
    pub peer: SocketAddrV4,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceConfig {
    pub mode: ServiceBrokerMode,
    pub broker_executable: PathBuf,
    pub bind_ip: Ipv4Addr,
    pub port: u16,
    pub peer_ip: Option<Ipv4Addr>,
    pub peer_port: Option<u16>,
    pub native_microphone: Option<NativeMicrophoneServiceConfig>,
    pub console_run_for: Option<Duration>,
}

impl ServiceConfig {
    pub fn parse<I, S>(arguments: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut arguments = arguments.into_iter().map(Into::into);
        let _program = arguments.next();
        let mut broker_executable = None;
        let mut mode = ServiceBrokerMode::AudioShareCompatibility;
        let mut bind_ip: Option<Ipv4Addr> = None;
        let mut port: Option<u16> = None;
        let mut peer_ip: Option<Ipv4Addr> = None;
        let mut peer_port: Option<u16> = None;
        let mut microphone_broker = None;
        let mut microphone_bind_ip: Option<Ipv4Addr> = None;
        let mut microphone_port: Option<u16> = None;
        let mut microphone_peer_ip: Option<Ipv4Addr> = None;
        let mut microphone_peer_port: Option<u16> = None;
        let mut console = false;
        let mut console_run_for = None;

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--console" => console = true,
                "--mode" => {
                    mode = match next_value(&mut arguments, "--mode")?.as_str() {
                        "audio-share-compatibility" => ServiceBrokerMode::AudioShareCompatibility,
                        "native-speaker" => ServiceBrokerMode::NativeSpeaker,
                        _ => return Err(ConfigError::InvalidMode),
                    };
                }
                "--broker" => {
                    broker_executable =
                        Some(PathBuf::from(next_value(&mut arguments, "--broker")?));
                }
                "--bind-ip" => {
                    let raw = next_value(&mut arguments, "--bind-ip")?;
                    bind_ip = Some(raw.parse().map_err(|_| ConfigError::InvalidBindIp)?);
                }
                "--port" => {
                    let raw = next_value(&mut arguments, "--port")?;
                    port = Some(raw.parse().map_err(|_| ConfigError::InvalidPort)?);
                }
                "--peer-ip" => {
                    let raw = next_value(&mut arguments, "--peer-ip")?;
                    peer_ip = Some(raw.parse().map_err(|_| ConfigError::InvalidPeerIp)?);
                }
                "--peer-port" => {
                    let raw = next_value(&mut arguments, "--peer-port")?;
                    peer_port = Some(raw.parse().map_err(|_| ConfigError::InvalidPeerPort)?);
                }
                "--microphone-broker" => {
                    microphone_broker = Some(PathBuf::from(next_value(
                        &mut arguments,
                        "--microphone-broker",
                    )?));
                }
                "--microphone-bind-ip" => {
                    let raw = next_value(&mut arguments, "--microphone-bind-ip")?;
                    microphone_bind_ip = Some(
                        raw.parse()
                            .map_err(|_| ConfigError::InvalidMicrophoneBindIp)?,
                    );
                }
                "--microphone-port" => {
                    let raw = next_value(&mut arguments, "--microphone-port")?;
                    microphone_port = Some(
                        raw.parse()
                            .map_err(|_| ConfigError::InvalidMicrophonePort)?,
                    );
                }
                "--microphone-peer-ip" => {
                    let raw = next_value(&mut arguments, "--microphone-peer-ip")?;
                    microphone_peer_ip = Some(
                        raw.parse()
                            .map_err(|_| ConfigError::InvalidMicrophonePeerIp)?,
                    );
                }
                "--microphone-peer-port" => {
                    let raw = next_value(&mut arguments, "--microphone-peer-port")?;
                    microphone_peer_port = Some(
                        raw.parse()
                            .map_err(|_| ConfigError::InvalidMicrophonePeerPort)?,
                    );
                }
                "--run-for-ms" => {
                    let raw = next_value(&mut arguments, "--run-for-ms")?;
                    let milliseconds: u64 = raw.parse().map_err(|_| ConfigError::InvalidRunFor)?;
                    let duration = Duration::from_millis(milliseconds);
                    if duration.is_zero() || duration > MAX_CONSOLE_RUN {
                        return Err(ConfigError::InvalidRunFor);
                    }
                    console_run_for = Some(duration);
                }
                _ => return Err(ConfigError::UnknownArgument(argument)),
            }
        }

        if console != console_run_for.is_some() {
            return Err(ConfigError::ConsoleRunPairRequired);
        }
        let broker_executable = broker_executable.ok_or(ConfigError::MissingBroker)?;
        if broker_executable.as_os_str().is_empty() {
            return Err(ConfigError::MissingBroker);
        }
        let bind_ip = bind_ip.ok_or(ConfigError::MissingBindIp)?;
        if bind_ip.is_unspecified() {
            return Err(ConfigError::InvalidBindIp);
        }
        let port = port
            .filter(|port| *port != 0)
            .ok_or(ConfigError::InvalidPort)?;
        let microphone_option_count = [
            microphone_broker.is_some(),
            microphone_bind_ip.is_some(),
            microphone_port.is_some(),
            microphone_peer_ip.is_some(),
            microphone_peer_port.is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count();
        if !matches!(microphone_option_count, 0 | 5) {
            return Err(ConfigError::IncompleteMicrophoneConfig);
        }
        let native_microphone = if microphone_option_count == 5 {
            if mode != ServiceBrokerMode::NativeSpeaker {
                return Err(ConfigError::UnexpectedMicrophoneConfig);
            }
            let broker_executable = microphone_broker.expect("complete microphone config");
            if broker_executable.as_os_str().is_empty() {
                return Err(ConfigError::MissingMicrophoneBroker);
            }
            let local_ip = microphone_bind_ip.expect("complete microphone config");
            let local_port = microphone_port.expect("complete microphone config");
            let peer_ip = microphone_peer_ip.expect("complete microphone config");
            let peer_port = microphone_peer_port.expect("complete microphone config");
            if !is_concrete_unicast(local_ip) {
                return Err(ConfigError::InvalidMicrophoneBindIp);
            }
            if local_port == 0 {
                return Err(ConfigError::InvalidMicrophonePort);
            }
            if !is_concrete_unicast(peer_ip) {
                return Err(ConfigError::InvalidMicrophonePeerIp);
            }
            if peer_port == 0 {
                return Err(ConfigError::InvalidMicrophonePeerPort);
            }
            let local = SocketAddrV4::new(local_ip, local_port);
            let peer = SocketAddrV4::new(peer_ip, peer_port);
            if local == peer {
                return Err(ConfigError::DuplicateMicrophoneEndpoint);
            }
            Some(NativeMicrophoneServiceConfig {
                broker_executable,
                local,
                peer,
            })
        } else {
            None
        };
        match mode {
            ServiceBrokerMode::AudioShareCompatibility => {
                if peer_ip.is_some() || peer_port.is_some() {
                    return Err(ConfigError::UnexpectedPeer);
                }
            }
            ServiceBrokerMode::NativeSpeaker => {
                let peer = peer_ip.ok_or(ConfigError::MissingPeerIp)?;
                if peer.is_unspecified() || peer.is_multicast() || peer == Ipv4Addr::BROADCAST {
                    return Err(ConfigError::InvalidPeerIp);
                }
                if peer_port.is_none_or(|port| port == 0) {
                    return Err(ConfigError::InvalidPeerPort);
                }
                if bind_ip == peer && Some(port) == peer_port {
                    return Err(ConfigError::DuplicateNativeEndpoint);
                }
            }
        }
        Ok(Self {
            mode,
            broker_executable,
            bind_ip,
            port,
            peer_ip,
            peer_port,
            native_microphone,
            console_run_for,
        })
    }

    pub fn supervisor(&self) -> Result<ServiceBrokerProcess, String> {
        match self.mode {
            ServiceBrokerMode::AudioShareCompatibility => {
                AudioShareSupervisor::new_virtual_speaker(
                    self.broker_executable.clone(),
                    IpAddr::V4(self.bind_ip),
                    self.port,
                    SupervisorLimits::default(),
                )
                .map(ServiceBrokerProcess::AudioShareCompatibility)
                .map_err(|error| error.to_string())
            }
            ServiceBrokerMode::NativeSpeaker => {
                let speaker = NativeSpeakerSupervisor::new(
                    self.broker_executable.clone(),
                    SocketAddrV4::new(self.bind_ip, self.port),
                    SocketAddrV4::new(
                        self.peer_ip.expect("validated native peer IP"),
                        self.peer_port.expect("validated native peer port"),
                    ),
                    NativeSpeakerSupervisorLimits::default(),
                )
                .map_err(|error| error.to_string())?;
                if let Some(microphone) = &self.native_microphone {
                    let microphone = NativeMicrophoneSupervisor::new(
                        microphone.broker_executable.clone(),
                        microphone.local,
                        microphone.peer,
                        NativeSpeakerSupervisorLimits::default(),
                    )
                    .map_err(|error| error.to_string())?;
                    Ok(ServiceBrokerProcess::NativeAudio(NativeAudioSupervisor {
                        speaker,
                        microphone,
                    }))
                } else {
                    Ok(ServiceBrokerProcess::NativeSpeaker(speaker))
                }
            }
        }
    }
}

fn is_concrete_unicast(ip: Ipv4Addr) -> bool {
    !ip.is_unspecified() && !ip.is_multicast() && ip != Ipv4Addr::BROADCAST
}

fn next_value<I>(arguments: &mut I, option: &'static str) -> Result<String, ConfigError>
where
    I: Iterator<Item = String>,
{
    arguments.next().ok_or(ConfigError::MissingValue(option))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigError {
    MissingValue(&'static str),
    UnknownArgument(String),
    InvalidMode,
    MissingBroker,
    MissingBindIp,
    InvalidBindIp,
    InvalidPort,
    MissingPeerIp,
    InvalidPeerIp,
    InvalidPeerPort,
    UnexpectedPeer,
    IncompleteMicrophoneConfig,
    UnexpectedMicrophoneConfig,
    MissingMicrophoneBroker,
    InvalidMicrophoneBindIp,
    InvalidMicrophonePort,
    InvalidMicrophonePeerIp,
    InvalidMicrophonePeerPort,
    DuplicateMicrophoneEndpoint,
    DuplicateNativeEndpoint,
    InvalidRunFor,
    ConsoleRunPairRequired,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingValue(option) => write!(formatter, "missing value for {option}"),
            Self::UnknownArgument(_) => formatter.write_str("unknown service argument"),
            Self::InvalidMode => formatter.write_str("--mode is unsupported"),
            Self::MissingBroker => formatter.write_str("--broker is required"),
            Self::MissingBindIp => formatter.write_str("--bind-ip is required"),
            Self::InvalidBindIp => {
                formatter.write_str("--bind-ip must be a non-unspecified IPv4 literal")
            }
            Self::InvalidPort => formatter.write_str("--port must be a non-zero u16"),
            Self::MissingPeerIp => {
                formatter.write_str("--peer-ip is required in native-speaker mode")
            }
            Self::InvalidPeerIp => formatter.write_str("--peer-ip must be a concrete IPv4 literal"),
            Self::InvalidPeerPort => formatter.write_str("--peer-port must be a non-zero u16"),
            Self::UnexpectedPeer => {
                formatter.write_str("peer options are valid only in native-speaker mode")
            }
            Self::IncompleteMicrophoneConfig => formatter.write_str(
                "native microphone broker, local endpoint and peer endpoint must be supplied together",
            ),
            Self::UnexpectedMicrophoneConfig => formatter
                .write_str("native microphone options require native-speaker mode"),
            Self::MissingMicrophoneBroker => {
                formatter.write_str("--microphone-broker must not be empty")
            }
            Self::InvalidMicrophoneBindIp => formatter
                .write_str("--microphone-bind-ip must be a concrete IPv4 literal"),
            Self::InvalidMicrophonePort => {
                formatter.write_str("--microphone-port must be a non-zero u16")
            }
            Self::InvalidMicrophonePeerIp => formatter
                .write_str("--microphone-peer-ip must be a concrete IPv4 literal"),
            Self::InvalidMicrophonePeerPort => {
                formatter.write_str("--microphone-peer-port must be a non-zero u16")
            }
            Self::DuplicateMicrophoneEndpoint => formatter
                .write_str("native microphone local and peer endpoints must differ"),
            Self::DuplicateNativeEndpoint => {
                formatter.write_str("native local and peer endpoints must differ")
            }
            Self::InvalidRunFor => formatter.write_str("--run-for-ms must be between 1 and 600000"),
            Self::ConsoleRunPairRequired => {
                formatter.write_str("--console and --run-for-ms must be supplied together")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerServiceState {
    Stopped,
    Starting,
    WaitingForReceiver,
    Active,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerServiceSnapshot {
    pub schema_version: u8,
    pub state: BrokerServiceState,
    pub generation: u64,
    pub receiver_present: bool,
    pub problem_code: Option<String>,
}

pub trait BrokerProcess {
    fn start(&mut self) -> Result<(), &'static str>;
    fn running(&mut self) -> Result<bool, String>;
    fn activation_ready(&mut self) -> Result<bool, String> {
        self.receiver_present()
    }
    fn activation_requires_receiver(&self) -> bool {
        true
    }
    fn receiver_present(&mut self) -> Result<bool, String>;
    fn stop(&mut self) -> Result<(), String>;
}

impl BrokerProcess for AudioShareSupervisor {
    fn start(&mut self) -> Result<(), &'static str> {
        AudioShareSupervisor::start(self)
            .map(|_| ())
            .map_err(|_| "CAPY.WINDOWS_SERVICE.COMPATIBILITY_BROKER_START_FAILED")
    }

    fn running(&mut self) -> Result<bool, String> {
        AudioShareSupervisor::status(self)
            .map(|status| matches!(status, SupervisorStatus::Running { .. }))
            .map_err(|error| error.to_string())
    }

    fn activation_ready(&mut self) -> Result<bool, String> {
        self.receiver_present()
    }

    fn receiver_present(&mut self) -> Result<bool, String> {
        AudioShareSupervisor::receiver_tcp_presence(self)
            .map(|presence| matches!(presence, ReceiverTcpPresence::Established { .. }))
            .map_err(|error| error.to_string())
    }

    fn stop(&mut self) -> Result<(), String> {
        AudioShareSupervisor::stop(self)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

pub enum ServiceBrokerProcess {
    AudioShareCompatibility(AudioShareSupervisor),
    NativeSpeaker(NativeSpeakerSupervisor),
    NativeAudio(NativeAudioSupervisor),
}

pub struct NativeAudioSupervisor {
    speaker: NativeSpeakerSupervisor,
    microphone: NativeMicrophoneSupervisor,
}

trait NativeStartChild {
    fn start_child(&mut self) -> Result<(), NativeSpeakerSupervisorError>;
    fn running_child(&mut self) -> Result<bool, NativeSpeakerSupervisorError>;
    fn stop_child(&mut self) -> Result<(), NativeSpeakerSupervisorError>;
}

impl NativeStartChild for NativeSpeakerSupervisor {
    fn start_child(&mut self) -> Result<(), NativeSpeakerSupervisorError> {
        self.start().map(|_| ())
    }

    fn running_child(&mut self) -> Result<bool, NativeSpeakerSupervisorError> {
        self.status()
            .map(|status| matches!(status, NativeSpeakerSupervisorStatus::Running { .. }))
    }

    fn stop_child(&mut self) -> Result<(), NativeSpeakerSupervisorError> {
        self.stop()
    }
}

impl NativeStartChild for NativeMicrophoneSupervisor {
    fn start_child(&mut self) -> Result<(), NativeSpeakerSupervisorError> {
        self.start().map(|_| ())
    }

    fn running_child(&mut self) -> Result<bool, NativeSpeakerSupervisorError> {
        self.status()
            .map(|status| matches!(status, NativeSpeakerSupervisorStatus::Running { .. }))
    }

    fn stop_child(&mut self) -> Result<(), NativeSpeakerSupervisorError> {
        self.stop()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NativePairHealth {
    speaker_running: bool,
    microphone_running: bool,
}

impl NativePairHealth {
    fn any_running(self) -> bool {
        self.speaker_running || self.microphone_running
    }

    fn all_running(self) -> bool {
        self.speaker_running && self.microphone_running
    }
}

fn native_pair_health(
    speaker: &mut impl NativeStartChild,
    microphone: &mut impl NativeStartChild,
) -> Result<NativePairHealth, NativeSpeakerSupervisorError> {
    let speaker_running = speaker.running_child()?;
    let microphone_running = microphone.running_child()?;
    Ok(NativePairHealth {
        speaker_running,
        microphone_running,
    })
}

fn start_native_pair(
    speaker: &mut impl NativeStartChild,
    microphone: &mut impl NativeStartChild,
) -> Result<(), NativeSpeakerSupervisorError> {
    speaker.start_child()?;
    if let Err(error) = microphone.start_child() {
        let _ = speaker.stop_child();
        return Err(error);
    }
    Ok(())
}

impl NativeAudioSupervisor {
    fn start(&mut self) -> Result<(), NativeSpeakerSupervisorError> {
        start_native_pair(&mut self.speaker, &mut self.microphone)
    }

    fn running_any(&mut self) -> Result<bool, NativeSpeakerSupervisorError> {
        native_pair_health(&mut self.speaker, &mut self.microphone)
            .map(NativePairHealth::any_running)
    }

    fn all_running(&mut self) -> Result<bool, NativeSpeakerSupervisorError> {
        native_pair_health(&mut self.speaker, &mut self.microphone)
            .map(NativePairHealth::all_running)
    }

    fn stop(&mut self) -> Result<(), NativeSpeakerSupervisorError> {
        let microphone = self.microphone.stop();
        let speaker = self.speaker.stop();
        microphone.and(speaker)
    }
}

impl BrokerProcess for ServiceBrokerProcess {
    fn start(&mut self) -> Result<(), &'static str> {
        match self {
            Self::AudioShareCompatibility(process) => BrokerProcess::start(process),
            Self::NativeSpeaker(process) => map_native_start(process.start().map(|_| ())),
            Self::NativeAudio(process) => map_native_start(process.start()),
        }
    }

    fn running(&mut self) -> Result<bool, String> {
        match self {
            Self::AudioShareCompatibility(process) => BrokerProcess::running(process),
            Self::NativeSpeaker(process) => process
                .status()
                .map(|status| matches!(status, NativeSpeakerSupervisorStatus::Running { .. }))
                .map_err(|error| error.to_string()),
            Self::NativeAudio(process) => process.running_any().map_err(|error| error.to_string()),
        }
    }

    fn activation_ready(&mut self) -> Result<bool, String> {
        match self {
            Self::AudioShareCompatibility(process) => BrokerProcess::activation_ready(process),
            Self::NativeSpeaker(process) => process
                .status()
                .map(|status| matches!(status, NativeSpeakerSupervisorStatus::Running { .. }))
                .map_err(|error| error.to_string()),
            Self::NativeAudio(process) => process.all_running().map_err(|error| error.to_string()),
        }
    }

    fn activation_requires_receiver(&self) -> bool {
        matches!(self, Self::AudioShareCompatibility(_))
    }

    fn receiver_present(&mut self) -> Result<bool, String> {
        match self {
            Self::AudioShareCompatibility(process) => BrokerProcess::receiver_present(process),
            Self::NativeSpeaker(_) | Self::NativeAudio(_) => Ok(false),
        }
    }

    fn stop(&mut self) -> Result<(), String> {
        match self {
            Self::AudioShareCompatibility(process) => BrokerProcess::stop(process),
            Self::NativeSpeaker(process) => process.stop().map_err(|error| error.to_string()),
            Self::NativeAudio(process) => process.stop().map_err(|error| error.to_string()),
        }
    }
}

fn map_native_start(result: Result<(), NativeSpeakerSupervisorError>) -> Result<(), &'static str> {
    result.map_err(|error| match error {
        NativeSpeakerSupervisorError::Spawn(_) => "CAPY.WINDOWS_SERVICE.NATIVE_BROKER_SPAWN_FAILED",
        NativeSpeakerSupervisorError::ExitedBeforeReady => {
            "CAPY.WINDOWS_SERVICE.NATIVE_BROKER_EXITED_BEFORE_READY"
        }
        NativeSpeakerSupervisorError::StartupTimedOut => {
            "CAPY.WINDOWS_SERVICE.NATIVE_BROKER_READY_TIMEOUT"
        }
        NativeSpeakerSupervisorError::MissingReadyLine
        | NativeSpeakerSupervisorError::InvalidReadyLine => {
            "CAPY.WINDOWS_SERVICE.NATIVE_BROKER_READY_INVALID"
        }
        NativeSpeakerSupervisorError::BrokerStartup(detail)
            if detail.contains("CreateFileMappingW") =>
        {
            "CAPY.WINDOWS_SERVICE.NATIVE_RENDER_RING_CREATE_FAILED"
        }
        NativeSpeakerSupervisorError::BrokerStartup(detail)
            if detail.contains("render mapping") =>
        {
            "CAPY.WINDOWS_SERVICE.NATIVE_RENDER_RING_ALREADY_OWNED"
        }
        NativeSpeakerSupervisorError::BrokerStartup(detail) if detail.contains("10048") => {
            "CAPY.WINDOWS_SERVICE.NATIVE_UDP_ADDRESS_IN_USE"
        }
        NativeSpeakerSupervisorError::BrokerStartup(detail) if detail.contains("10013") => {
            "CAPY.WINDOWS_SERVICE.NATIVE_UDP_ACCESS_DENIED"
        }
        NativeSpeakerSupervisorError::BrokerStartup(detail) if detail.contains("10049") => {
            "CAPY.WINDOWS_SERVICE.NATIVE_UDP_ADDRESS_UNAVAILABLE"
        }
        NativeSpeakerSupervisorError::BrokerStartup(detail)
            if detail.contains("native LAN socket operation failed") =>
        {
            "CAPY.WINDOWS_SERVICE.NATIVE_UDP_SOCKET_FAILED"
        }
        NativeSpeakerSupervisorError::BrokerStartup(detail) if detail.contains("Windows ") => {
            "CAPY.WINDOWS_SERVICE.NATIVE_WINDOWS_BOUNDARY_FAILED"
        }
        NativeSpeakerSupervisorError::BrokerStartup(detail) if detail.contains("bind") => {
            "CAPY.WINDOWS_SERVICE.NATIVE_UDP_BIND_FAILED"
        }
        NativeSpeakerSupervisorError::BrokerStartup(_) => {
            "CAPY.WINDOWS_SERVICE.NATIVE_BROKER_REJECTED_STARTUP"
        }
        _ => "CAPY.WINDOWS_SERVICE.NATIVE_BROKER_START_FAILED",
    })
}

pub struct BrokerServiceRuntime<P> {
    process: P,
    state: BrokerServiceState,
    generation: u64,
    stable_receiver_polls: u8,
    receiver_polls: u8,
    receiver_present: bool,
    problem_code: Option<&'static str>,
}

impl<P: BrokerProcess> BrokerServiceRuntime<P> {
    pub fn new(process: P, stable_receiver_polls: u8) -> Result<Self, String> {
        if stable_receiver_polls == 0 {
            return Err("stable receiver poll count must be non-zero".to_owned());
        }
        Ok(Self {
            process,
            state: BrokerServiceState::Stopped,
            generation: 0,
            stable_receiver_polls,
            receiver_polls: 0,
            receiver_present: false,
            problem_code: None,
        })
    }

    pub fn start(&mut self) -> Result<BrokerServiceSnapshot, String> {
        if self.state != BrokerServiceState::Stopped {
            return Err("Broker service Runtime is already started".to_owned());
        }
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| "Broker service generation exhausted".to_owned())?;
        match self.process.start() {
            Ok(()) => {
                self.state = self.pending_state();
                self.problem_code = None;
                Ok(self.snapshot())
            }
            Err(problem_code) => {
                self.state = BrokerServiceState::Failed;
                self.problem_code = Some(problem_code);
                Err(problem_code.to_owned())
            }
        }
    }

    pub fn ensure_started(&mut self) -> Result<BrokerServiceSnapshot, String> {
        match self.state {
            BrokerServiceState::Stopped => self.start(),
            BrokerServiceState::Failed => {
                self.stop()?;
                self.start()
            }
            BrokerServiceState::Starting
            | BrokerServiceState::WaitingForReceiver
            | BrokerServiceState::Active => Ok(self.snapshot()),
        }
    }

    pub fn poll(&mut self) -> BrokerServiceSnapshot {
        if !matches!(
            self.state,
            BrokerServiceState::Starting
                | BrokerServiceState::WaitingForReceiver
                | BrokerServiceState::Active
        ) {
            return self.snapshot();
        }
        match self.process.running() {
            Ok(true) => {}
            Ok(false) | Err(_) => {
                self.fail("CAPY.WINDOWS_SERVICE.BROKER_EXITED");
                return self.snapshot();
            }
        }
        match self.process.activation_ready() {
            Ok(true) => {
                self.receiver_polls = self.receiver_polls.saturating_add(1);
                if self.receiver_polls >= self.stable_receiver_polls {
                    self.state = BrokerServiceState::Active;
                }
            }
            Ok(false) => {
                self.receiver_polls = 0;
                self.state = self.pending_state();
            }
            Err(_) => self.fail("CAPY.WINDOWS_SERVICE.ACTIVATION_OBSERVATION_FAILED"),
        }
        if !matches!(self.state, BrokerServiceState::Failed) {
            match self.process.receiver_present() {
                Ok(present) => self.receiver_present = present,
                Err(_) => self.fail("CAPY.WINDOWS_SERVICE.RECEIVER_OBSERVATION_FAILED"),
            }
        }
        self.snapshot()
    }

    pub fn stop(&mut self) -> Result<BrokerServiceSnapshot, String> {
        let result = self.process.stop();
        self.state = BrokerServiceState::Stopped;
        self.receiver_polls = 0;
        self.receiver_present = false;
        self.problem_code = None;
        result.map(|()| self.snapshot())
    }

    pub fn ensure_stopped(&mut self) -> Result<BrokerServiceSnapshot, String> {
        if self.state == BrokerServiceState::Stopped {
            Ok(self.snapshot())
        } else {
            self.stop()
        }
    }

    pub fn snapshot(&self) -> BrokerServiceSnapshot {
        BrokerServiceSnapshot {
            schema_version: 1,
            state: self.state,
            generation: self.generation,
            receiver_present: self.receiver_present,
            problem_code: self.problem_code.map(str::to_owned),
        }
    }

    fn fail(&mut self, code: &'static str) {
        self.state = BrokerServiceState::Failed;
        self.receiver_polls = 0;
        self.receiver_present = false;
        self.problem_code = Some(code);
        let _ = self.process.stop();
    }

    fn pending_state(&self) -> BrokerServiceState {
        if self.process.activation_requires_receiver() {
            BrokerServiceState::WaitingForReceiver
        } else {
            BrokerServiceState::Starting
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeBroker {
        running: bool,
        ready: bool,
        receiver: bool,
        requires_receiver: bool,
        starts: usize,
        stops: usize,
    }

    #[derive(Default)]
    struct FakeNativeChild {
        fail_start: bool,
        running: bool,
        starts: usize,
        stops: usize,
    }

    impl NativeStartChild for FakeNativeChild {
        fn start_child(&mut self) -> Result<(), NativeSpeakerSupervisorError> {
            self.starts += 1;
            if self.fail_start {
                Err(NativeSpeakerSupervisorError::MissingExecutable)
            } else {
                self.running = true;
                Ok(())
            }
        }

        fn running_child(&mut self) -> Result<bool, NativeSpeakerSupervisorError> {
            Ok(self.running)
        }

        fn stop_child(&mut self) -> Result<(), NativeSpeakerSupervisorError> {
            self.stops += 1;
            self.running = false;
            Ok(())
        }
    }

    impl BrokerProcess for FakeBroker {
        fn start(&mut self) -> Result<(), &'static str> {
            self.running = true;
            self.starts += 1;
            Ok(())
        }

        fn running(&mut self) -> Result<bool, String> {
            Ok(self.running)
        }

        fn activation_ready(&mut self) -> Result<bool, String> {
            Ok(self.ready || self.receiver)
        }

        fn activation_requires_receiver(&self) -> bool {
            self.requires_receiver
        }

        fn receiver_present(&mut self) -> Result<bool, String> {
            Ok(self.receiver)
        }

        fn stop(&mut self) -> Result<(), String> {
            self.running = false;
            self.stops += 1;
            Ok(())
        }
    }

    #[test]
    fn config_is_closed_bounded_and_requires_explicit_ipv4() {
        let parsed = ServiceConfig::parse([
            "service",
            "--broker",
            "broker.exe",
            "--bind-ip",
            "100.64.0.1",
            "--port",
            "65530",
        ])
        .expect("valid service config");
        assert_eq!(parsed.bind_ip, Ipv4Addr::new(100, 64, 0, 1));
        assert_eq!(parsed.port, 65530);
        assert_eq!(parsed.mode, ServiceBrokerMode::AudioShareCompatibility);
        assert!(
            ServiceConfig::parse([
                "service",
                "--broker",
                "x",
                "--bind-ip",
                "0.0.0.0",
                "--port",
                "1"
            ])
            .is_err()
        );
        assert!(
            ServiceConfig::parse([
                "service",
                "--broker",
                "x",
                "--bind-ip",
                "127.0.0.1",
                "--port",
                "1",
                "--shell"
            ])
            .is_err()
        );
    }

    #[test]
    fn native_config_requires_an_explicit_distinct_peer() {
        let parsed = ServiceConfig::parse([
            "service",
            "--mode",
            "native-speaker",
            "--broker",
            "native.exe",
            "--bind-ip",
            "100.64.0.1",
            "--port",
            "46001",
            "--peer-ip",
            "100.64.0.2",
            "--peer-port",
            "46000",
        ])
        .expect("valid native service config");
        assert_eq!(parsed.mode, ServiceBrokerMode::NativeSpeaker);
        assert_eq!(parsed.peer_ip, Some(Ipv4Addr::new(100, 64, 0, 2)));
        assert_eq!(parsed.peer_port, Some(46000));
        assert!(
            ServiceConfig::parse([
                "service",
                "--mode",
                "native-speaker",
                "--broker",
                "native.exe",
                "--bind-ip",
                "100.64.0.1",
                "--port",
                "46001",
            ])
            .is_err()
        );
        assert!(
            ServiceConfig::parse([
                "service",
                "--broker",
                "compat.exe",
                "--bind-ip",
                "100.64.0.1",
                "--port",
                "65530",
                "--peer-ip",
                "100.64.0.2",
                "--peer-port",
                "46000",
            ])
            .is_err()
        );
    }

    #[test]
    fn native_microphone_config_is_complete_closed_and_mode_bound() {
        let parsed = ServiceConfig::parse([
            "service",
            "--mode",
            "native-speaker",
            "--broker",
            "speaker.exe",
            "--bind-ip",
            "100.64.0.1",
            "--port",
            "46001",
            "--peer-ip",
            "100.64.0.2",
            "--peer-port",
            "46000",
            "--microphone-broker",
            "microphone.exe",
            "--microphone-bind-ip",
            "100.64.0.1",
            "--microphone-port",
            "46011",
            "--microphone-peer-ip",
            "100.64.0.2",
            "--microphone-peer-port",
            "46010",
        ])
        .expect("complete native audio config");
        let microphone = parsed.native_microphone.expect("microphone config");
        assert_eq!(
            microphone.broker_executable,
            PathBuf::from("microphone.exe")
        );
        assert_eq!(microphone.local, "100.64.0.1:46011".parse().unwrap());
        assert_eq!(microphone.peer, "100.64.0.2:46010".parse().unwrap());

        assert!(matches!(
            ServiceConfig::parse([
                "service",
                "--mode",
                "native-speaker",
                "--broker",
                "speaker.exe",
                "--bind-ip",
                "100.64.0.1",
                "--port",
                "46001",
                "--peer-ip",
                "100.64.0.2",
                "--peer-port",
                "46000",
                "--microphone-broker",
                "microphone.exe",
            ]),
            Err(ConfigError::IncompleteMicrophoneConfig)
        ));
        assert!(matches!(
            ServiceConfig::parse([
                "service",
                "--broker",
                "compat.exe",
                "--bind-ip",
                "100.64.0.1",
                "--port",
                "46001",
                "--microphone-broker",
                "microphone.exe",
                "--microphone-bind-ip",
                "100.64.0.1",
                "--microphone-port",
                "46011",
                "--microphone-peer-ip",
                "100.64.0.2",
                "--microphone-peer-port",
                "46010",
            ]),
            Err(ConfigError::UnexpectedMicrophoneConfig)
        ));
    }

    #[test]
    fn microphone_start_failure_rolls_back_speaker_child() {
        let mut speaker = FakeNativeChild::default();
        let mut microphone = FakeNativeChild {
            fail_start: true,
            ..FakeNativeChild::default()
        };
        assert!(start_native_pair(&mut speaker, &mut microphone).is_err());
        assert_eq!((speaker.starts, speaker.stops), (1, 1));
        assert_eq!((microphone.starts, microphone.stops), (1, 0));
    }

    #[test]
    fn native_pair_health_preserves_a_surviving_direction() {
        let mut speaker = FakeNativeChild {
            running: true,
            ..FakeNativeChild::default()
        };
        let mut microphone = FakeNativeChild::default();

        let health = native_pair_health(&mut speaker, &mut microphone).expect("health");

        assert!(health.any_running());
        assert!(!health.all_running());
        assert_eq!((speaker.stops, microphone.stops), (0, 0));
    }

    #[test]
    fn console_mode_is_explicit_and_time_bounded() {
        assert!(
            ServiceConfig::parse([
                "service",
                "--broker",
                "x",
                "--bind-ip",
                "127.0.0.1",
                "--port",
                "1",
                "--console"
            ])
            .is_err()
        );
        assert!(
            ServiceConfig::parse([
                "service",
                "--broker",
                "x",
                "--bind-ip",
                "127.0.0.1",
                "--port",
                "1",
                "--console",
                "--run-for-ms",
                "600001"
            ])
            .is_err()
        );
    }

    #[test]
    fn receiver_requires_stable_presence_and_loss_returns_to_waiting() {
        let mut runtime = BrokerServiceRuntime::new(
            FakeBroker {
                requires_receiver: true,
                ..FakeBroker::default()
            },
            3,
        )
        .expect("runtime");
        assert_eq!(
            runtime.start().expect("start").state,
            BrokerServiceState::WaitingForReceiver
        );
        runtime.process.receiver = true;
        assert_eq!(runtime.poll().state, BrokerServiceState::WaitingForReceiver);
        assert_eq!(runtime.poll().state, BrokerServiceState::WaitingForReceiver);
        assert_eq!(runtime.poll().state, BrokerServiceState::Active);
        runtime.process.receiver = false;
        assert_eq!(runtime.poll().state, BrokerServiceState::WaitingForReceiver);
        assert_eq!(
            runtime.stop().expect("stop").state,
            BrokerServiceState::Stopped
        );
    }

    #[test]
    fn transport_readiness_does_not_claim_receiver_presence() {
        let mut runtime = BrokerServiceRuntime::new(FakeBroker::default(), 2).expect("runtime");
        runtime.start().expect("start");
        runtime.process.ready = true;
        assert_eq!(runtime.poll().state, BrokerServiceState::Starting);
        let snapshot = runtime.poll();
        assert_eq!(snapshot.state, BrokerServiceState::Active);
        assert!(!snapshot.receiver_present);
    }

    #[test]
    fn transport_readiness_loss_keeps_the_running_process_alive() {
        let mut runtime = BrokerServiceRuntime::new(FakeBroker::default(), 1).expect("runtime");
        runtime.start().expect("start");
        runtime.process.ready = true;
        assert_eq!(runtime.poll().state, BrokerServiceState::Active);

        runtime.process.ready = false;
        let snapshot = runtime.poll();

        assert_eq!(snapshot.state, BrokerServiceState::Starting);
        assert_eq!(runtime.process.stops, 0);
        assert!(snapshot.problem_code.is_none());
    }

    #[test]
    fn broker_exit_is_failed_and_reaped() {
        let mut runtime = BrokerServiceRuntime::new(FakeBroker::default(), 1).expect("runtime");
        runtime.start().expect("start");
        runtime.process.running = false;
        let snapshot = runtime.poll();
        assert_eq!(snapshot.state, BrokerServiceState::Failed);
        assert_eq!(
            snapshot.problem_code.as_deref(),
            Some("CAPY.WINDOWS_SERVICE.BROKER_EXITED")
        );
        assert_eq!(runtime.process.stops, 1);
    }
}
