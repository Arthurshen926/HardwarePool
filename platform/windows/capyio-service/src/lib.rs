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
mod audio_route_control;
#[cfg(windows)]
pub use audio_route_control::{
    AudioRouteDirection, AudioRouteKey, AudioRouteOperation, AudioRouteServiceClient,
    AudioRouteSnapshot, AudioRouteStart, ControlledAudioRoutes,
};

#[cfg(windows)]
mod camera_ingress;
#[cfg(windows)]
pub use camera_ingress::{CameraIngressService, allocate_camera_producer};
#[cfg(windows)]
mod camera_control;
#[cfg(windows)]
mod control;
pub mod display_worker_failure;
#[cfg(windows)]
pub mod local_pipe;
mod microphone;
#[cfg(windows)]
mod microphone_control;

#[cfg(windows)]
mod display_control;
#[cfg(windows)]
mod display_host;
#[cfg(windows)]
mod display_runtime;
#[cfg(windows)]
pub use display_control::{
    ControlledDisplayRuntime, DisplayControlOperation, DisplayControlSnapshot, DisplayServiceClient,
};
#[cfg(windows)]
pub use display_host::android_gamestream_properties_from_single_display_config;
#[cfg(windows)]
pub use display_runtime::{DisplayExecutionSnapshot, DisplayExecutionState, DisplayServiceRuntime};
#[cfg(windows)]
pub use display_runtime::{DisplayPeerLaunchPrepared, DisplayPeerLaunchRequest};
#[cfg(windows)]
mod display_route;
#[cfg(windows)]
mod display_session;
#[cfg(windows)]
mod display_worker;

#[cfg(windows)]
pub use display_host::{
    DisplayServiceHost, DisplaySessionSettings, LoadedDisplayServiceConfig,
    LoadedSingleDisplayServiceConfig, SingleDisplayServiceHost, SingleDisplaySnapshot,
    SingleDisplayState, SingleDisplayStatusHandle, load_display_service_config,
    load_single_display_service_config,
};
#[cfg(windows)]
pub use display_route::{
    DisplayProjectionControl, DisplayRouteProjectionError, DisplayRouteProjectionRuntime,
    DisplayRouteProjectionSnapshot, DisplayRouteProjectionSpec,
};
#[cfg(windows)]
pub use display_session::{AuthenticatedDisplayProjectionWorker, DisplaySessionWorkerError};
#[cfg(windows)]
pub use display_worker::{
    AuthorizedDisplayWorkerPipe, BoundDisplayWorkerChannel, BoundDisplayWorkerPeer,
    DisplayWorkerChannelError, DisplayWorkerLaunchError, DisplayWorkerPeerError,
    DisplayWorkerPipeError, ExpectedDisplayWorkerProcessId, SpawnedDisplayWorker,
    VerifiedDisplayWorkerExecutable, launch_display_worker_in_session,
    verify_local_system_service_context,
};

#[cfg(windows)]
pub use camera_control::{
    CAMERA_CONTROL_PIPE_NAME, CameraBrokerClient, CameraBrokerProcess, CameraBrokerRuntime,
    CameraBrokerSnapshot, CameraBrokerState, camera_control_server_loop,
    camera_control_server_loop_until_stopped, camera_control_server_loop_until_stopped_for_user,
    current_process_user_sid, wake_camera_control_server,
};
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
    pub single_display_config: Option<PathBuf>,
    pub display_control_user_sid: Option<String>,
    pub audio_route_control_user_sid: Option<String>,
    pub camera_ingress_user_sid: Option<String>,
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
        let mut single_display_config = None;
        let mut display_control_user_sid = None;
        let mut audio_route_control_user_sid = None;
        let mut camera_ingress_user_sid = None;

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--camera-ingress-user-sid" => {
                    let sid = next_value(&mut arguments, "--camera-ingress-user-sid")?;
                    if camera_ingress_user_sid.is_some()
                        || sid.len() > 184
                        || !sid.starts_with("S-1-")
                        || !sid
                            .bytes()
                            .all(|b| b == b'S' || b == b'-' || b.is_ascii_digit())
                    {
                        return Err(ConfigError::InvalidMode);
                    }
                    camera_ingress_user_sid = Some(sid);
                }
                "--audio-route-control-user-sid" => {
                    let sid = next_value(&mut arguments, "--audio-route-control-user-sid")?;
                    if camera_ingress_user_sid.is_some() && console {
                        return Err(ConfigError::InvalidMode);
                    }
                    if audio_route_control_user_sid.is_some()
                        || sid.len() > 184
                        || !sid.starts_with("S-1-")
                        || !sid
                            .bytes()
                            .all(|b| b == b'S' || b == b'-' || b.is_ascii_digit())
                    {
                        return Err(ConfigError::InvalidMode);
                    }
                    audio_route_control_user_sid = Some(sid);
                }
                "--display-control-user-sid" => {
                    let sid = next_value(&mut arguments, "--display-control-user-sid")?;
                    if display_control_user_sid.is_some()
                        || sid.len() > 184
                        || !sid.starts_with("S-1-")
                        || !sid
                            .bytes()
                            .all(|b| b == b'S' || b == b'-' || b.is_ascii_digit())
                    {
                        return Err(ConfigError::InvalidDisplayConfig);
                    }
                    display_control_user_sid = Some(sid);
                }
                "--single-display-config" => {
                    let path =
                        PathBuf::from(next_value(&mut arguments, "--single-display-config")?);
                    if single_display_config.is_some() || !path.is_absolute() {
                        return Err(ConfigError::InvalidDisplayConfig);
                    }
                    single_display_config = Some(path);
                }
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
        if display_control_user_sid.is_some() && single_display_config.is_none() {
            return Err(ConfigError::InvalidDisplayConfig);
        }
        let broker_executable = broker_executable.ok_or(ConfigError::MissingBroker)?;
        if broker_executable.as_os_str().is_empty() {
            return Err(ConfigError::MissingBroker);
        }
        let bind_ip = bind_ip.ok_or(ConfigError::MissingBindIp)?;
        if !is_concrete_unicast(bind_ip) {
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
                if let Some(microphone) = &native_microphone {
                    let speaker_local = SocketAddrV4::new(bind_ip, port);
                    let speaker_peer =
                        SocketAddrV4::new(peer, peer_port.expect("validated native peer port"));
                    // Each native direction owns a separate explicit-peer UDP socket.
                    // Reject shared endpoints before starting either broker child.
                    if speaker_local == microphone.local {
                        return Err(ConfigError::ConflictingNativeLocalEndpoints);
                    }
                    if speaker_peer == microphone.peer {
                        return Err(ConfigError::ConflictingNativePeerEndpoints);
                    }
                }
            }
        }
        if audio_route_control_user_sid.is_some()
            && (mode != ServiceBrokerMode::NativeSpeaker || native_microphone.is_none() || console)
        {
            return Err(ConfigError::InvalidMode);
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
            single_display_config,
            display_control_user_sid,
            audio_route_control_user_sid,
            camera_ingress_user_sid,
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
                        microphone_local: self.native_microphone.as_ref().unwrap().local,
                        microphone_peer: self.native_microphone.as_ref().unwrap().peer,
                        microphone_generation: 0,
                        microphone_problem: None,
                        route_owned: [false; 2],
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
    InvalidDisplayConfig,
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
    ConflictingNativeLocalEndpoints,
    ConflictingNativePeerEndpoints,
    InvalidRunFor,
    ConsoleRunPairRequired,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDisplayConfig => formatter.write_str(
                "--single-display-config requires one absolute trusted-host path",
            ),
            Self::MissingValue(option) => write!(formatter, "missing value for {option}"),
            Self::UnknownArgument(_) => formatter.write_str("unknown service argument"),
            Self::InvalidMode => formatter.write_str("--mode is unsupported"),
            Self::MissingBroker => formatter.write_str("--broker is required"),
            Self::MissingBindIp => formatter.write_str("--bind-ip is required"),
            Self::InvalidBindIp => {
                formatter.write_str("--bind-ip must be a concrete unicast IPv4 literal")
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
            Self::ConflictingNativeLocalEndpoints => formatter.write_str(
                "native speaker and microphone must use distinct local UDP endpoints; check --bind-ip/--port and --microphone-bind-ip/--microphone-port",
            ),
            Self::ConflictingNativePeerEndpoints => formatter.write_str(
                "native speaker and microphone must use distinct peer UDP endpoints; check --peer-ip/--peer-port and --microphone-peer-ip/--microphone-peer-port",
            ),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_endpoint: Option<SocketAddrV4>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_endpoint: Option<SocketAddrV4>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub produced_frames: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumed_frames: Option<u64>,
}

pub trait BrokerProcess {
    fn native_microphone(&mut self, _start: Option<bool>) -> Result<BrokerServiceSnapshot, String> {
        Err("CAPY.WINDOWS_SERVICE.NATIVE_MICROPHONE_UNAVAILABLE".to_owned())
    }
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
    microphone_local: SocketAddrV4,
    microphone_peer: SocketAddrV4,
    microphone_generation: u64,
    microphone_problem: Option<String>,
    // Mic, speaker. The old IU pipe cannot start/stop a SID-owned Route worker.
    route_owned: [bool; 2],
}

trait NativeStartChild {
    #[cfg(test)]
    fn start_child(&mut self) -> Result<(), NativeSpeakerSupervisorError>;
    fn running_child(&mut self) -> Result<bool, NativeSpeakerSupervisorError>;
    #[cfg(test)]
    fn stop_child(&mut self) -> Result<(), NativeSpeakerSupervisorError>;
}

impl NativeStartChild for NativeSpeakerSupervisor {
    #[cfg(test)]
    fn start_child(&mut self) -> Result<(), NativeSpeakerSupervisorError> {
        self.start().map(|_| ())
    }

    fn running_child(&mut self) -> Result<bool, NativeSpeakerSupervisorError> {
        self.status()
            .map(|status| matches!(status, NativeSpeakerSupervisorStatus::Running { .. }))
    }

    #[cfg(test)]
    fn stop_child(&mut self) -> Result<(), NativeSpeakerSupervisorError> {
        self.stop()
    }
}

impl NativeStartChild for NativeMicrophoneSupervisor {
    #[cfg(test)]
    fn start_child(&mut self) -> Result<(), NativeSpeakerSupervisorError> {
        self.start().map(|_| ())
    }

    fn running_child(&mut self) -> Result<bool, NativeSpeakerSupervisorError> {
        self.status()
            .map(|status| matches!(status, NativeSpeakerSupervisorStatus::Running { .. }))
    }

    #[cfg(test)]
    fn stop_child(&mut self) -> Result<(), NativeSpeakerSupervisorError> {
        self.stop()
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NativePairHealth {
    speaker_running: bool,
    microphone_running: bool,
}

#[cfg(test)]
impl NativePairHealth {
    fn any_running(self) -> bool {
        self.speaker_running || self.microphone_running
    }

    fn all_running(self) -> bool {
        self.speaker_running && self.microphone_running
    }
}

#[cfg(test)]
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

#[cfg(test)]
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
        self.speaker.start().map(|_| ())
    }

    fn running_any(&mut self) -> Result<bool, NativeSpeakerSupervisorError> {
        self.speaker.running_child()
    }

    fn all_running(&mut self) -> Result<bool, NativeSpeakerSupervisorError> {
        self.speaker.running_child()
    }

    fn stop(&mut self) -> Result<(), NativeSpeakerSupervisorError> {
        self.speaker.stop()
    }
}

impl BrokerProcess for ServiceBrokerProcess {
    fn native_microphone(&mut self, start: Option<bool>) -> Result<BrokerServiceSnapshot, String> {
        let Self::NativeAudio(process) = self else {
            return Err("CAPY.WINDOWS_SERVICE.NATIVE_MICROPHONE_UNAVAILABLE".to_owned());
        };
        if start.is_some() && process.route_owned[0] {
            return Err("CAPY.AUDIO_ROUTE.OWNED".into());
        }
        if let Some(start) = start {
            if start {
                if !process
                    .microphone
                    .running_child()
                    .map_err(|error| error.to_string())?
                {
                    process.microphone_generation = process
                        .microphone_generation
                        .checked_add(1)
                        .ok_or_else(|| "CAPY.WINDOWS_SERVICE.GENERATION_EXHAUSTED".to_owned())?;
                    process.microphone_problem = None;
                    if let Err(code) = map_native_start(process.microphone.start().map(|_| ())) {
                        process.microphone_problem = Some(code.to_owned());
                        return Err(code.to_owned());
                    }
                }
            } else {
                process
                    .microphone
                    .stop()
                    .map_err(|error| error.to_string())?;
                process.microphone_problem = None;
            }
        }
        let status = process
            .microphone
            .status()
            .map_err(|error| error.to_string())?;
        if matches!(status, NativeSpeakerSupervisorStatus::Exited { .. })
            && process.microphone_problem.is_none()
        {
            process.microphone_problem =
                Some("CAPY.WINDOWS_SERVICE.NATIVE_MICROPHONE_EXITED".to_owned());
        }
        #[cfg(windows)]
        let metrics = capyio_windows_capture_ring::CaptureRingObserver::attach()
            .ok()
            .map(|observer| observer.metrics());
        #[cfg(not(windows))]
        let metrics: Option<capyio_windows_capture_ring::CaptureRingMetrics> = None;
        Ok(BrokerServiceSnapshot {
            schema_version: 1,
            state: match status {
                NativeSpeakerSupervisorStatus::Running { .. } => BrokerServiceState::Active,
                NativeSpeakerSupervisorStatus::Stopped if process.microphone_problem.is_none() => {
                    BrokerServiceState::Stopped
                }
                _ => BrokerServiceState::Failed,
            },
            generation: process.microphone_generation,
            receiver_present: false,
            problem_code: process.microphone_problem.clone(),
            local_endpoint: Some(process.microphone_local),
            peer_endpoint: Some(process.microphone_peer),
            produced_frames: metrics.as_ref().map(|value| value.produced_frames),
            consumed_frames: metrics.as_ref().map(|value| value.consumed_frames),
        })
    }
    fn start(&mut self) -> Result<(), &'static str> {
        if matches!(self, Self::NativeAudio(process) if process.route_owned[1]) {
            return Err("CAPY.AUDIO_ROUTE.OWNED");
        }
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
        if matches!(self, Self::NativeAudio(process) if process.route_owned[1]) {
            return Err("CAPY.AUDIO_ROUTE.OWNED".into());
        }
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
            local_endpoint: None,
            peer_endpoint: None,
            produced_frames: None,
            consumed_frames: None,
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
    fn camera_allocator_requires_explicit_sid_and_service_mode() {
        let base = [
            "service",
            "--broker",
            "worker.exe",
            "--bind-ip",
            "127.0.0.1",
            "--port",
            "40101",
        ];
        let valid = ["--camera-ingress-user-sid", "S-1-5-21-1-2-3-1001"];
        let config = ServiceConfig::parse(base.into_iter().chain(valid)).unwrap();
        assert_eq!(config.camera_ingress_user_sid.as_deref(), Some(valid[1]));
        assert!(ServiceConfig::parse(base.into_iter().chain(valid).chain(["--console"])).is_err());
        assert!(ServiceConfig::parse(base.into_iter().chain(valid).chain(valid)).is_err());
        assert!(
            ServiceConfig::parse(
                base.into_iter()
                    .chain(["--camera-ingress-user-sid", "S-1-5-18)(A;;GA;;;WD"])
            )
            .is_err()
        );
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
        assert_eq!(parsed.single_display_config, None);
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

    fn native_audio_config(endpoints: [&str; 4]) -> Result<ServiceConfig, ConfigError> {
        let [
            speaker_local,
            speaker_peer,
            microphone_local,
            microphone_peer,
        ] = endpoints.map(|endpoint| endpoint.parse::<SocketAddrV4>().expect("test endpoint"));
        ServiceConfig::parse([
            "service".to_owned(),
            "--mode".to_owned(),
            "native-speaker".to_owned(),
            "--broker".to_owned(),
            "speaker.exe".to_owned(),
            "--bind-ip".to_owned(),
            speaker_local.ip().to_string(),
            "--port".to_owned(),
            speaker_local.port().to_string(),
            "--peer-ip".to_owned(),
            speaker_peer.ip().to_string(),
            "--peer-port".to_owned(),
            speaker_peer.port().to_string(),
            "--microphone-broker".to_owned(),
            "microphone.exe".to_owned(),
            "--microphone-bind-ip".to_owned(),
            microphone_local.ip().to_string(),
            "--microphone-port".to_owned(),
            microphone_local.port().to_string(),
            "--microphone-peer-ip".to_owned(),
            microphone_peer.ip().to_string(),
            "--microphone-peer-port".to_owned(),
            microphone_peer.port().to_string(),
        ])
    }

    #[test]
    fn native_audio_rejects_a_shared_local_udp_endpoint_before_starting_brokers() {
        let error = native_audio_config([
            "100.64.0.1:46001",
            "100.64.0.2:46000",
            "100.64.0.1:46001",
            "100.64.0.2:46010",
        ])
        .expect_err("two broker children cannot bind the same local socket");
        assert_eq!(error, ConfigError::ConflictingNativeLocalEndpoints);
        assert!(error.to_string().contains("--microphone-port"));
    }

    #[test]
    fn native_audio_rejects_a_shared_peer_udp_endpoint() {
        let error = native_audio_config([
            "100.64.0.1:46001",
            "100.64.0.2:46000",
            "100.64.0.1:46011",
            "100.64.0.2:46000",
        ])
        .expect_err("Android directions own distinct explicit-peer sockets");
        assert_eq!(error, ConfigError::ConflictingNativePeerEndpoints);
        assert!(error.to_string().contains("--microphone-peer-port"));
    }

    #[test]
    fn native_audio_allows_equal_ports_on_distinct_ip_addresses() {
        native_audio_config([
            "100.64.0.1:46001",
            "100.64.0.2:46000",
            "100.64.0.3:46001",
            "100.64.0.4:46000",
        ])
        .expect("socket identity includes the address, not just the port");
    }

    #[test]
    fn service_config_rejects_non_unicast_bind_addresses() {
        for mode in ["audio-share-compatibility", "native-speaker"] {
            for bind_ip in ["0.0.0.0", "239.1.2.3", "255.255.255.255"] {
                let result = ServiceConfig::parse([
                    "service",
                    "--mode",
                    mode,
                    "--broker",
                    "broker.exe",
                    "--bind-ip",
                    bind_ip,
                    "--port",
                    "46001",
                ]);
                assert_eq!(
                    result.expect_err("bind IP must be unicast"),
                    ConfigError::InvalidBindIp
                );
            }
        }
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
