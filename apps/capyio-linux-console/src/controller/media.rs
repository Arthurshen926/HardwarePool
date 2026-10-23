//! Finite terminal consent for the existing paired-store audio launchers.
//! This module is a client of the sibling Node, never an audio/store backend.
use super::*;
use capyio_core::{CapabilityId, PortId, RouteId, RouteState, SessionId};
use std::{net::SocketAddrV4, os::unix::fs::OpenOptionsExt, path::Path};

const BOOTSTRAP: Duration = Duration::from_secs(5);
const BEFORE_APPROVAL: Duration = Duration::from_secs(55);
const CONSENT: Duration = Duration::from_secs(30);
const STOP_GRACE: Duration = Duration::from_secs(3);
const FRAME_TIMEOUT: Duration = Duration::from_secs(5);
const SECURITY: &str = "unauthenticated_udp_trusted_lab_only";
const WARNING: &str =
    "WARNING: plaintext unauthenticated UDP media; trusted isolated lab only.\r\n";

pub(super) const SECURE_WARNING: &str = "Experimental pinned TLS 1.3 mono media; explicit endpoint, one finite session at most 60 seconds. No automatic downgrade or renewal.\r\nAuthenticated PCM writes do not prove audibility or application consumption; the Route remains Starting.\r\n";

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq, clap::ValueEnum)]
pub(super) enum MediaSecurity {
    #[default]
    #[serde(rename = "unauthenticated_udp_trusted_lab_only")]
    #[value(name = "unauthenticated_udp_trusted_lab_only")]
    Legacy,
    #[serde(rename = "pinned_tls13_mono_v1")]
    #[value(name = "pinned_tls13_mono_v1")]
    Secure,
    #[serde(rename = "pinned_tls13_mono_300s_v1")]
    #[value(name = "pinned_tls13_mono_300s_v1")]
    Secure300,
    #[serde(rename = "pinned_tls13_stereo_v1")]
    #[value(name = "pinned_tls13_stereo_v1")]
    SecureStereo,
}
impl MediaSecurity {
    pub(super) fn is_secure(self) -> bool {
        self != Self::Legacy
    }

    pub(super) fn max_duration_secs(self) -> u64 {
        match self {
            Self::Secure | Self::SecureStereo => 60,
            Self::Legacy | Self::Secure300 => 300,
        }
    }

    pub(super) fn secure_format(self) -> Option<&'static str> {
        match self {
            Self::Legacy => None,
            Self::Secure => Some(capyio_protocol::SECURE_AUDIO_MONO_FORMAT),
            Self::Secure300 => Some(capyio_protocol::SECURE_AUDIO_MONO_300S_FORMAT),
            Self::SecureStereo => Some(capyio_protocol::SECURE_AUDIO_STEREO_FORMAT),
        }
    }

    pub(super) fn warning(self) -> &'static str {
        match self {
            Self::Legacy => WARNING,
            Self::Secure => SECURE_WARNING,
            Self::SecureStereo => {
                "Experimental pinned TLS 1.3 stereo capture; explicit selected source and fresh independent approval on both peers, 1..60 seconds. No downgrade, renewal or virtual speaker.\r\nSent transport records do not prove Android acceptance or audibility; the Route remains Starting.\r\n"
            }
            Self::Secure300 => {
                "Experimental pinned TLS 1.3 mono media; explicit 300-second profile, one finite session at most 300 seconds. No automatic downgrade or renewal.\r\nAuthenticated PCM writes do not prove audibility or application consumption; the Route remains Starting.\r\nAndroid live microphone support is not available for this profile.\r\n"
            }
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Legacy => SECURITY,
            Self::Secure => "pinned_tls13_mono_v1",
            Self::Secure300 => "pinned_tls13_mono_300s_v1",
            Self::SecureStereo => "pinned_tls13_stereo_v1",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Port {
    node_id: NodeId,
    capability_id: CapabilityId,
    port_id: PortId,
}
impl Port {
    fn valid(self) -> bool {
        !self.node_id.as_uuid().is_nil()
            && !self.capability_id.as_uuid().is_nil()
            && !self.port_id.as_uuid().is_nil()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Direction {
    Playback,
    Capture,
}
impl Direction {
    fn command(self) -> &'static str {
        match self {
            Self::Playback => "linux-playback",
            Self::Capture => "linux-capture",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Playback => "Playback",
            Self::Capture => "Capture",
        }
    }

    fn format(self) -> &'static str {
        match self {
            Self::Playback => "pcm-s16le-48000-mono",
            Self::Capture => "pcm-s16le-48000-stereo",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Mode {
    #[default]
    Playback,
    VirtualMicrophone,
    StereoCapture,
    VirtualSpeaker,
}
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Playback => "playback",
            Self::VirtualMicrophone => "virtual_microphone",
            Self::StereoCapture => "stereo_capture",
            Self::VirtualSpeaker => "virtual_speaker",
        }
    }

    fn direction(self) -> Direction {
        match self {
            Self::Playback | Self::VirtualMicrophone => Direction::Playback,
            Self::StereoCapture | Self::VirtualSpeaker => Direction::Capture,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Proposal {
    #[default]
    LinuxLed,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum CaptureMode {
    #[default]
    SelectedSource,
    VirtualSpeaker,
}

// Deliberately local copies of the distinct nonsecret launcher schemas. Importing
// the platform Host here would give the controller an unnecessary backend boundary.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaybackConfig {
    #[serde(default)]
    media_security: MediaSecurity,
    version: u32,
    node_id: NodeId,
    source: Port,
    listen: SocketAddrV4,
    media: SocketAddrV4,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    endpoint_mode: Mode,
    duration_secs: u64,
    #[serde(default, rename = "proposal_mode")]
    _proposal_mode: Proposal,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureConfig {
    #[serde(default)]
    media_security: MediaSecurity,
    version: u32,
    node_id: NodeId,
    sink: Port,
    listen: SocketAddrV4,
    media: SocketAddrV4,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    endpoint_mode: CaptureMode,
    duration_secs: u64,
}

#[derive(Debug)]
struct Config {
    media_security: MediaSecurity,
    direction: Direction,
    version: u32,
    node_id: NodeId,
    remote: Port,
    listen: SocketAddrV4,
    media: SocketAddrV4,
    target: Option<String>,
    endpoint_mode: Mode,
    duration_secs: u64,
}
impl Config {
    fn parse(bytes: &[u8], local: NodeId, peer: NodeId, direction: Direction) -> io::Result<Self> {
        if bytes.is_empty() || bytes.len() > MAX_FRAME {
            return Err(invalid());
        }
        let value = match direction {
            Direction::Playback => {
                let value: PlaybackConfig = serde_json::from_slice(bytes).map_err(|_| invalid())?;
                Self {
                    direction,
                    version: value.version,
                    node_id: value.node_id,
                    remote: value.source,
                    media_security: value.media_security,
                    listen: value.listen,
                    media: value.media,
                    target: value.target,
                    endpoint_mode: value.endpoint_mode,
                    duration_secs: value.duration_secs,
                }
            }
            Direction::Capture => {
                let value: CaptureConfig = serde_json::from_slice(bytes).map_err(|_| invalid())?;
                Self {
                    direction,
                    version: value.version,
                    node_id: value.node_id,
                    remote: value.sink,
                    media_security: value.media_security,
                    listen: value.listen,
                    media: value.media,
                    target: value.target,
                    endpoint_mode: match value.endpoint_mode {
                        CaptureMode::SelectedSource => Mode::StereoCapture,
                        CaptureMode::VirtualSpeaker => Mode::VirtualSpeaker,
                    },
                    duration_secs: value.duration_secs,
                }
            }
        };
        value.validate(local, peer)?;
        Ok(value)
    }

    fn format(&self) -> &'static str {
        self.media_security
            .secure_format()
            .unwrap_or_else(|| self.direction.format())
    }

    fn validate(&self, local: NodeId, peer: NodeId) -> io::Result<()> {
        // Match the Host's concrete lab-address policy, including its port-1
        // placeholder when TCP listening requests an ephemeral port.
        let mut listen = self.listen;
        if listen.port() == 0 {
            listen.set_port(1);
        }
        let secure = self.media_security.is_secure();
        let mut media = self.media;
        if secure {
            if !(if self.media_security == MediaSecurity::SecureStereo {
                self.endpoint_mode == Mode::StereoCapture
            } else {
                matches!(self.endpoint_mode, Mode::Playback | Mode::VirtualMicrophone)
            }) || self.media.port() != 0
                || self.duration_secs > self.media_security.max_duration_secs()
            {
                return Err(invalid());
            }
            media.set_port(1);
        }
        if self.version != 1
            || self.node_id != local
            || local.as_uuid().is_nil()
            || peer.as_uuid().is_nil()
            || peer == local
            || self.remote.node_id != peer
            || !self.remote.valid()
            || self.endpoint_mode.direction() != self.direction
            || self.listen.ip() != self.media.ip()
            || !lab_address(listen)
            || !lab_address(media)
            || (!secure && listen == self.media)
            || !(1..=300).contains(&self.duration_secs)
        {
            return Err(invalid());
        }
        match (self.endpoint_mode, self.target.as_deref()) {
            (Mode::Playback | Mode::StereoCapture, Some(target)) if explicit_target(target) => {
                Ok(())
            }
            (Mode::VirtualMicrophone | Mode::VirtualSpeaker, None) => Ok(()),
            _ => Err(invalid()),
        }
    }

    fn wire(&self) -> Value {
        let mut value = json!({"version":self.version,"node_id":self.node_id,
            "listen":self.listen,"media":self.media,"target":self.target,
            "duration_secs":self.duration_secs});
        match self.direction {
            Direction::Playback => {
                value["source"] = json!(self.remote);
                if self.media_security.is_secure() {
                    value["media_security"] = json!(self.media_security);
                }
                value["endpoint_mode"] = json!(self.endpoint_mode);
                value["proposal_mode"] = json!(Proposal::LinuxLed);
            }
            Direction::Capture => {
                value["sink"] = json!(self.remote);
                if self.media_security.is_secure() {
                    value["media_security"] = json!(self.media_security);
                }
                // Manifest and event names intentionally differ in the Node API.
                value["endpoint_mode"] = json!(match self.endpoint_mode {
                    Mode::StereoCapture => CaptureMode::SelectedSource,
                    Mode::VirtualSpeaker => CaptureMode::VirtualSpeaker,
                    _ => unreachable!("validated capture configuration"),
                });
            }
        }
        value
    }
}

pub(super) fn validate_export(
    bytes: &[u8],
    local: NodeId,
    peer: NodeId,
    capture: bool,
) -> io::Result<()> {
    Config::parse(
        bytes,
        local,
        peer,
        if capture {
            Direction::Capture
        } else {
            Direction::Playback
        },
    )
    .map(|_| ())
}

fn lab_address(address: SocketAddrV4) -> bool {
    let ip = address.ip();
    let octets = ip.octets();
    let overlay = octets[0] == 100 && (64..=127).contains(&octets[1]);
    address.port() != 0 && (ip.is_private() || ip.is_loopback() || overlay) && octets[3] != 255
}

fn explicit_target(target: &str) -> bool {
    !target.is_empty()
        && target.len() <= 128
        && !target.starts_with('-')
        && target != "0"
        && !target.eq_ignore_ascii_case("auto")
        && !target.eq_ignore_ascii_case("default")
        && target
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.:-".contains(&byte))
}

fn read_config(
    path: &Path,
    local: NodeId,
    peer: NodeId,
    direction: Direction,
) -> io::Result<Config> {
    if !path.is_absolute() {
        return Err(invalid());
    }
    // Open before inspecting metadata, without following the final component.
    // NONBLOCK also prevents a substituted FIFO from hanging before fstat.
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| invalid())?;
    let metadata = file.metadata().map_err(|_| invalid())?;
    let owner = unsafe { libc::geteuid() };
    if !metadata.is_file()
        || (metadata.uid() != owner && metadata.uid() != 0)
        || metadata.mode() & 0o022 != 0
        || metadata.len() == 0
        || metadata.len() > MAX_FRAME as u64
    {
        return Err(invalid());
    }
    let mut bytes = [0_u8; MAX_FRAME + 1];
    let mut used = 0;
    while used < bytes.len() {
        let count = file.read(&mut bytes[used..]).map_err(|_| invalid())?;
        if count == 0 {
            break;
        }
        used += count;
    }
    if used != metadata.len() as usize || used > MAX_FRAME {
        return Err(invalid());
    }
    Config::parse(&bytes[..used], local, peer, direction)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Listening {
    version: u32,
    event: String,
    address: SocketAddrV4,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    version: u32,
    event: String,
    request_id: MessageId,
    route_id: RouteId,
    session_id: SessionId,
    source: Port,
    sink: Port,
    target: Option<String>,
    endpoint_mode: Mode,
    node_name: Option<String>,
    format: String,
    duration_secs: u64,
    approval_timeout_secs: u64,
    media_security: String,
}
impl Approval {
    fn validate(&self, config: &Config) -> io::Result<()> {
        let (local, remote) = match config.direction {
            Direction::Playback => (self.sink, self.source),
            Direction::Capture => (self.source, self.sink),
        };
        let expected_name = match config.endpoint_mode {
            Mode::Playback | Mode::StereoCapture => None,
            Mode::VirtualMicrophone => Some(format!("capyio.virtual-microphone.{}", local.port_id)),
            Mode::VirtualSpeaker => Some(format!("capyio.virtual-speaker.{}", local.port_id)),
        };
        if self.request_id.as_uuid().is_nil()
            || self.route_id.as_uuid().is_nil()
            || self.session_id.as_uuid().is_nil()
            || remote != config.remote
            || !local.valid()
            || local.node_id != config.node_id
            || self.target != config.target
            || self.endpoint_mode != config.endpoint_mode
            || self.node_name != expected_name
            || self.format != config.format()
            || self.duration_secs != config.duration_secs
            || self.approval_timeout_secs != CONSENT.as_secs()
            || self.media_security != config.media_security.name()
        {
            return Err(invalid());
        }
        Ok(())
    }

    fn expected(&self) -> String {
        let remote = match self.endpoint_mode.direction() {
            Direction::Playback => self.source,
            Direction::Capture => self.sink,
        };
        format!(
            "start {} {} {}",
            self.request_id, self.route_id, remote.node_id
        )
    }

    fn command(&self) -> Value {
        json!({"version":1,"command":"approve","request_id":self.request_id,"route_id":self.route_id})
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AwaitingPeer {
    version: u32,
    event: String,
    route_id: RouteId,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Stage {
    Listening,
    Catalog,
    AwaitingLocalApproval,
    AwaitingPeerConsentOrLease,
    AwaitingPeerApproval,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StageStatus {
    version: u32,
    event: String,
    stage: Stage,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RouteStatus {
    version: u32,
    event: String,
    route_id: RouteId,
    state: RouteState,
    packets_written: u64,
    packets_sent: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bytes_written: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    media_security: Option<MediaSecurity>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum TerminalCause {
    Failure,
    LocalStop,
    PeerStop,
    MediaClosed,
    MediaTimeout,
    MediaFailure,
    ControlClosed,
    ControlFailure,
    CaptureClosed,
    CaptureFailure,
    CaptureTimeout,
    ClockRegressed,
    Expired,
    SourceRevoked,
    SessionChanged,
    CatalogChanged,
    NegotiationTimeout,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SimpleEvent {
    version: u32,
    event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    terminal_observed_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    terminal_cause: Option<TerminalCause>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    packets_written: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bytes_written: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    packets_dropped_busy: Option<u64>,
}

// Stereo capture counts local PCM reads separately from complete TLS records.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StereoStatus {
    version: u32,
    event: String,
    route_id: RouteId,
    state: RouteState,
    media_security: MediaSecurity,
    packets_sent: u64,
    packets_captured: u64,
    bytes_read: u64,
    transport_bytes_sent: u64,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StereoTerminal {
    version: u32,
    event: String,
    packets_sent: u64,
    packets_captured: u64,
    bytes_read: u64,
    transport_bytes_sent: u64,
    terminal_cause: TerminalCause,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    terminal_observed_at_ms: Option<u64>,
}

#[derive(Debug)]
enum Event {
    Listening(Listening),
    StereoAwaiting(AwaitingPeer),
    StereoBound(AwaitingPeer),
    StereoStatus(StereoStatus),
    StereoTerminal(StereoTerminal),
    Approval(Box<Approval>),
    AwaitingPeer(AwaitingPeer),
    Stage(StageStatus),
    Route(RouteStatus),
    Stopped(SimpleEvent),
    Failed(SimpleEvent),
}
impl Event {
    fn parse(value: Value) -> io::Result<Self> {
        if value["version"] != 1 {
            return Err(invalid());
        }
        fn decode<T: serde::de::DeserializeOwned>(value: Value) -> io::Result<T> {
            serde_json::from_value(value).map_err(|_| invalid())
        }
        match text(&value, "event")? {
            "awaiting_peer_approval" => Ok(Self::StereoAwaiting(decode(value)?)),
            "bound" => Ok(Self::StereoBound(decode(value)?)),
            "status" if value["media_security"] == "pinned_tls13_stereo_v1" => {
                Ok(Self::StereoStatus(decode(value)?))
            }
            "stopped" | "failed" if value.get("packets_captured").is_some() => {
                Ok(Self::StereoTerminal(decode(value)?))
            }
            "listening" => Ok(Self::Listening(decode(value)?)),
            "approval_required" => {
                // Option fields must be present as explicit nulls when absent.
                exact_keys(
                    &value,
                    &[
                        "version",
                        "event",
                        "request_id",
                        "route_id",
                        "session_id",
                        "source",
                        "sink",
                        "target",
                        "endpoint_mode",
                        "node_name",
                        "format",
                        "duration_secs",
                        "approval_timeout_secs",
                        "media_security",
                    ],
                )?;
                Ok(Self::Approval(decode(value)?))
            }
            "awaiting_peer_offer" => Ok(Self::AwaitingPeer(decode(value)?)),
            "status" if value.get("stage").is_some() => Ok(Self::Stage(decode(value)?)),
            "status" => {
                let mut keys = vec![
                    "version",
                    "event",
                    "route_id",
                    "state",
                    "packets_written",
                    "packets_sent",
                ];
                if value.get("media_security").is_some() {
                    if !matches!(
                        value["media_security"].as_str(),
                        Some("pinned_tls13_mono_v1" | "pinned_tls13_mono_300s_v1")
                    ) || !value["bytes_written"].is_u64()
                    {
                        return Err(invalid());
                    }
                    keys.extend(["media_security", "bytes_written"]);
                }
                exact_keys(&value, &keys)?;
                Ok(Self::Route(decode(value)?))
            }
            "stopped" | "failed" => {
                let counters = ["packets_written", "bytes_written", "packets_dropped_busy"];
                let has_counters = counters.iter().any(|key| value.get(key).is_some());
                if has_counters != value.get("terminal_cause").is_some()
                    || (has_counters && !value["terminal_cause"].is_string())
                {
                    return Err(invalid());
                }
                if counters.iter().any(|key| value.get(key).is_some())
                    && !counters.iter().all(|key| value[key].is_u64())
                {
                    return Err(invalid());
                }
                if value.get("terminal_observed_at_ms").is_some()
                    && !value["terminal_observed_at_ms"].is_u64()
                {
                    return Err(invalid());
                }
                if value["event"] == "stopped" {
                    Ok(Self::Stopped(decode(value)?))
                } else {
                    Ok(Self::Failed(decode(value)?))
                }
            }
            _ => Err(invalid()),
        }
    }

    fn canonical(&self) -> io::Result<String> {
        let result = match self {
            Self::StereoAwaiting(value) | Self::StereoBound(value) => serde_json::to_string(value),
            Self::StereoStatus(value) => serde_json::to_string(value),
            Self::StereoTerminal(value) => serde_json::to_string(value),
            Self::Listening(value) => serde_json::to_string(value),
            Self::Approval(value) => serde_json::to_string(value),
            Self::AwaitingPeer(value) => serde_json::to_string(value),
            Self::Stage(value) => serde_json::to_string(value),
            Self::Route(value) => serde_json::to_string(value),
            Self::Stopped(value) | Self::Failed(value) => serde_json::to_string(value),
        };
        result.map_err(|_| invalid())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Finished {
    Stopped,
    Failed,
}

#[derive(Default)]
struct Session {
    listening: bool,
    approval: Option<Approval>,
    approved: bool,
    offered: bool,
    stereo_waiting: bool,
    stereo_bound: bool,
    captured: u64,
    stopping: bool,
    finished: Option<Finished>,
    packets: u64,
    bytes: u64,
}
impl Session {
    fn accept(&mut self, event: &Event, config: &Config) -> io::Result<()> {
        if self.finished.is_some() {
            return Err(invalid());
        }
        // Immutable per-profile cumulative work is independent of elapsed time.
        let max_packets = config.duration_secs.checked_mul(100).ok_or_else(invalid)?;
        let max_bytes = max_packets.checked_mul(960).ok_or_else(invalid)?;
        if config.media_security.is_secure()
            && !(1..=config.media_security.max_duration_secs()).contains(&config.duration_secs)
        {
            return Err(invalid());
        }
        if let Event::Stopped(value) | Event::Failed(value) = event
            && (!config.media_security.is_secure()
                || config.media_security == MediaSecurity::SecureStereo)
            && (value.terminal_observed_at_ms.is_some() || value.packets_written.is_some())
        {
            return Err(invalid());
        }
        if let Event::Stopped(value) | Event::Failed(value) = event
            && matches!(
                value.terminal_cause,
                Some(
                    TerminalCause::ControlFailure
                        | TerminalCause::CaptureClosed
                        | TerminalCause::CaptureFailure
                        | TerminalCause::CaptureTimeout
                        | TerminalCause::ClockRegressed
                        | TerminalCause::Expired
                        | TerminalCause::SourceRevoked
                        | TerminalCause::SessionChanged
                        | TerminalCause::CatalogChanged
                        | TerminalCause::NegotiationTimeout
                )
            )
        {
            return Err(invalid());
        }
        if let Event::Stopped(value) | Event::Failed(value) = event
            && let (Some(packets), Some(bytes)) = (value.packets_written, value.bytes_written)
            && (!self.offered
                || packets < self.packets
                || bytes < self.bytes
                || packets > max_packets
                || bytes > max_bytes
                || !packets
                    .checked_mul(960)
                    .is_some_and(|complete| bytes >= complete && bytes - complete < 960)
                || value.packets_dropped_busy.unwrap_or(0) > max_packets)
        {
            return Err(invalid());
        }
        match event {
            Event::StereoAwaiting(value) => {
                if config.media_security != MediaSecurity::SecureStereo
                    || !self.approved
                    || self.stereo_waiting
                    || self.offered
                    || self.approval.as_ref().map(|a| a.route_id) != Some(value.route_id)
                {
                    return Err(invalid());
                }
                self.stereo_waiting = true;
            }
            Event::StereoBound(value) => {
                if config.media_security != MediaSecurity::SecureStereo
                    || !self.offered
                    || self.stereo_bound
                    || self.approval.as_ref().map(|a| a.route_id) != Some(value.route_id)
                {
                    return Err(invalid());
                }
                self.stereo_bound = true;
            }
            Event::StereoStatus(value) => {
                if value.media_security != MediaSecurity::SecureStereo
                    || !matches!(value.state, RouteState::Prepared | RouteState::Starting)
                    || (self.stereo_bound && value.state != RouteState::Starting)
                    || self.approval.as_ref().map(|a| a.route_id) != Some(value.route_id)
                {
                    return Err(invalid());
                }
                self.stereo_counts(
                    config,
                    value.packets_sent,
                    value.packets_captured,
                    value.bytes_read,
                    value.transport_bytes_sent,
                )?;
            }
            Event::StereoTerminal(value) => {
                if value.event != "failed" && value.terminal_observed_at_ms.is_none() {
                    return Err(invalid());
                }
                self.stereo_counts(
                    config,
                    value.packets_sent,
                    value.packets_captured,
                    value.bytes_read,
                    value.transport_bytes_sent,
                )?;
                self.finished = Some(if value.event == "stopped" {
                    Finished::Stopped
                } else {
                    Finished::Failed
                });
            }
            Event::Listening(value) => {
                if self.listening
                    || self.approval.is_some()
                    || value.address.ip() != config.listen.ip()
                    || !lab_address(value.address)
                    || (config.listen.port() != 0 && value.address.port() != config.listen.port())
                {
                    return Err(invalid());
                }
                self.listening = true;
            }
            Event::Approval(value) => {
                if !self.listening || self.approval.is_some() || self.approved || self.offered {
                    return Err(invalid());
                }
                value.validate(config)?;
            }
            Event::AwaitingPeer(value) => {
                if !self.approved
                    || (config.media_security == MediaSecurity::SecureStereo
                        && !self.stereo_waiting)
                    || self.offered
                    || self.approval.as_ref().map(|a| a.route_id) != Some(value.route_id)
                {
                    return Err(invalid());
                }
                self.offered = true;
            }
            Event::Stage(value) => {
                let valid = match value.stage {
                    Stage::Listening | Stage::Catalog => self.approval.is_none(),
                    Stage::AwaitingLocalApproval => self.approval.is_some() && !self.offered,
                    Stage::AwaitingPeerConsentOrLease => self.approved && !self.offered,
                    Stage::AwaitingPeerApproval => {
                        config.media_security == MediaSecurity::SecureStereo
                            && self.approved
                            && !self.offered
                    }
                };
                if !self.listening || !valid {
                    return Err(invalid());
                }
            }
            Event::Route(value) => {
                match config.media_security {
                    MediaSecurity::Legacy
                        if value.bytes_written.is_some() || value.media_security.is_some() =>
                    {
                        return Err(invalid());
                    }
                    MediaSecurity::SecureStereo => return Err(invalid()),
                    MediaSecurity::Secure | MediaSecurity::Secure300 => {
                        let bytes = value.bytes_written.ok_or_else(invalid)?;
                        if value.media_security != Some(config.media_security)
                            || value.state != RouteState::Starting
                            || bytes < self.bytes
                            || value.packets_written > max_packets
                            || bytes > max_bytes
                            || !value
                                .packets_written
                                .checked_mul(960)
                                .is_some_and(|complete| bytes >= complete && bytes - complete < 960)
                        {
                            return Err(invalid());
                        }
                    }
                    _ => {}
                }
                let (packets, opposite_packets) = match config.direction {
                    Direction::Playback => (value.packets_written, value.packets_sent),
                    Direction::Capture => (value.packets_sent, value.packets_written),
                };
                if !self.approved
                    || !self.offered
                    || self.approval.as_ref().map(|a| a.route_id) != Some(value.route_id)
                    || value.state == RouteState::Draft
                    || opposite_packets != 0
                    || packets < self.packets
                {
                    return Err(invalid());
                }
                self.packets = packets;
                self.bytes = value.bytes_written.unwrap_or(0);
            }
            Event::Stopped(_) => {
                if config.media_security == MediaSecurity::SecureStereo && self.offered {
                    return Err(invalid());
                }
                if !self.stopping && !self.approved {
                    return Err(invalid());
                }
                self.finished = Some(Finished::Stopped);
            }
            Event::Failed(_) => self.finished = Some(Finished::Failed),
        }
        Ok(())
    }

    fn stereo_counts(
        &mut self,
        config: &Config,
        sent: u64,
        captured: u64,
        pcm: u64,
        wire: u64,
    ) -> io::Result<()> {
        let max = config.duration_secs.checked_mul(100).ok_or_else(invalid)?;
        if config.media_security != MediaSecurity::SecureStereo
            || !self.approved
            || !self.offered
            || sent < self.packets
            || captured < self.captured
            || pcm < self.bytes
            || sent > captured
            || sent > max
            || (pcm != 0 && !self.stereo_bound)
            || sent.checked_mul(2128) != Some(wire)
            || !captured
                .checked_mul(1920)
                .is_some_and(|full| pcm >= full && pcm - full < 1920)
        {
            return Err(invalid());
        }
        self.packets = sent;
        self.captured = captured;
        self.bytes = pcm;
        Ok(())
    }

    fn prompt(&self) -> String {
        if self.stopping || self.finished.is_some() {
            "Stopping this finite session; no automatic restart.\r\n".into()
        } else if let Some(approval) = self.approval.as_ref().filter(|_| !self.approved) {
            format!("Type {} or stop: ", approval.expected())
        } else {
            "Type status, stop or cancel: ".into()
        }
    }

    fn summary(&self, config: &Config) -> String {
        let device = self
            .approval
            .as_ref()
            .and_then(|a| a.node_name.as_deref())
            .or(config.target.as_deref())
            .unwrap_or(match config.direction {
                Direction::Playback => "new owned virtual microphone (not created before approval)",
                Direction::Capture => "new owned virtual speaker (not created before approval)",
            });
        let (source, sink) = match config.direction {
            Direction::Playback => (config.remote.node_id, config.node_id),
            Direction::Capture => (config.node_id, config.remote.node_id),
        };
        let mut result = format!(
            "Source {} -> Sink {}\r\nMode: {}; device: {}; duration: {} seconds\r\n",
            source,
            sink,
            config.endpoint_mode.name(),
            device,
            config.duration_secs
        );
        result.push_str(&format!(
            "Format: {}; media security: {}\r\n",
            config.format(),
            config.media_security.name()
        ));
        result.push_str(config.media_security.warning());
        result
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Input {
    Stop,
    Status,
    Approve,
}
fn input_action(line: &[u8], session: &Session) -> io::Result<Input> {
    if line == b"stop" || line == b"cancel" {
        return Ok(Input::Stop);
    }
    if session.stopping || session.finished.is_some() {
        return Err(invalid());
    }
    if line == b"status" {
        return Ok(Input::Status);
    }
    if !session.approved
        && session
            .approval
            .as_ref()
            .is_some_and(|a| line == a.expected().as_bytes())
    {
        return Ok(Input::Approve);
    }
    Err(invalid())
}

fn operation_deadline(deadline: Instant) -> Instant {
    deadline.min(Instant::now() + Duration::from_secs(1))
}

pub(super) fn run(args: &Args, path: &Path, peer: NodeId) -> io::Result<()> {
    run_direction(args, path, peer, Direction::Playback)
}

pub(super) fn run_capture(args: &Args, path: &Path, peer: NodeId) -> io::Result<()> {
    run_direction(args, path, peer, Direction::Capture)
}

fn run_direction(args: &Args, path: &Path, peer: NodeId, direction: Direction) -> io::Result<()> {
    let config = read_config(path, args.pairing_node, peer, direction)?;
    let _signals = Signals::install()?;
    require_node_capability_for(direction.command())?;
    let mut tty = Terminal::open()?;
    let started = Instant::now();
    let overall =
        started + BEFORE_APPROVAL + Duration::from_secs(config.duration_secs) + STOP_GRACE;
    let mut deadline = started + BEFORE_APPROVAL;
    let mut session = Session::default();
    tty.begin_prompt(
        &(session.summary(&config) + &session.prompt()),
        operation_deadline(deadline),
    )?;
    let mut node = Node::spawn_for(args, direction.command(), Some(peer))?;
    node.set_stop_on_drop();
    node.set_frame_limit(2048)?;
    let bootstrap = (Instant::now() + BOOTSTRAP).min(deadline);
    node.send(&config.wire(), bootstrap)?;
    node.send(
        &json!({"version":1,"credential_source":"paired-store"}),
        bootstrap,
    )?;
    let mut consent_deadline = None;
    let mut partial_since = None;
    let mut final_deadline = None;
    let mut status_requests = 0;
    let mut status_pending = false;
    loop {
        check(deadline)?;
        if let Some(end) = consent_deadline {
            check(end)?;
        }
        let mut received = false;
        if let Some(value) = node.poll()? {
            received = true;
            let event = Event::parse(value)?;
            let status = matches!(
                event,
                Event::Stage(_) | Event::Route(_) | Event::StereoStatus(_)
            );
            if status && !status_pending {
                return Err(invalid());
            }
            session.accept(&event, &config)?;
            let canonical = event.canonical()?;
            let challenge = matches!(event, Event::Approval(_));
            if let Event::Approval(approval) = event {
                session.approval = Some(*approval);
            }
            // Only strict typed public event fields are rendered, never raw
            // stderr, unvalidated JSON, credentials, payload or unknown fields.
            tty.write(
                &format!(
                    "\r\n{} event: {canonical}\r\n{}",
                    direction.label(),
                    session.summary(&config)
                ),
                operation_deadline(deadline),
            )?;
            if challenge && !session.stopping {
                let end = (Instant::now() + CONSENT).min(deadline);
                consent_deadline = Some(end);
                tty.begin_prompt(&session.prompt(), operation_deadline(end))?;
            }
            if status {
                status_pending = false;
                tty.begin_prompt(
                    &session.prompt(),
                    operation_deadline(consent_deadline.unwrap_or(deadline)),
                )?;
            }
            if session.finished.is_some() {
                consent_deadline = None;
                final_deadline = Some(Instant::now() + Duration::from_secs(2));
            }
        }

        if node.frames.bytes.is_empty() {
            partial_since = None;
        } else if partial_since.get_or_insert_with(Instant::now).elapsed() >= FRAME_TIMEOUT {
            return Err(invalid());
        }

        // Drain queued child events before considering a human decision. In
        // particular, a coalesced duplicate challenge or terminal frame cannot
        // sit behind a valid challenge while its approval is sent.
        if received {
            continue;
        }

        if let Some(line) = tty.poll_line()? {
            let action = input_action(line.as_bytes(), &session)?;
            // Reject both coalesced extra input and a second queued partial
            // line before sending any command, especially a resource approval.
            if tty.poll_line()?.is_some() {
                return Err(invalid());
            }
            match action {
                Input::Stop if !session.stopping && session.finished.is_none() => {
                    node.send(
                        &json!({"version":1,"command":"stop"}),
                        operation_deadline(deadline),
                    )?;
                    session.stopping = true;
                    consent_deadline = None;
                    deadline = overall.min(Instant::now() + STOP_GRACE);
                }
                Input::Stop => {}
                Input::Status => {
                    status_requests += 1;
                    if status_requests > 1024 {
                        return Err(invalid());
                    }
                    node.send(
                        &json!({"version":1,"command":"status"}),
                        operation_deadline(deadline),
                    )?;
                    status_pending = true;
                }
                Input::Approve => {
                    if !node.frames.bytes.is_empty() {
                        return Err(invalid());
                    }
                    check(consent_deadline.ok_or_else(invalid)?)?;
                    let approval = session.approval.as_ref().ok_or_else(invalid)?;
                    node.send(&approval.command(), operation_deadline(deadline))?;
                    session.approved = true;
                    consent_deadline = None;
                    // This deadline only narrows the original lifetime. Status,
                    // delayed negotiation and prompts can never renew authority.
                    deadline = overall.min(
                        Instant::now() + Duration::from_secs(config.duration_secs) + STOP_GRACE,
                    );
                }
            }
            if !status_pending {
                tty.begin_prompt(&session.prompt(), operation_deadline(deadline))?;
            }
        }
        // A terminal event does not excuse trailing or partial child output.
        if let Some(status) = node.child.try_wait()? {
            node.reaped = true;
            if node.poll()?.is_some()
                || !node.frames.bytes.is_empty()
                || !status.success()
                || session.finished != Some(Finished::Stopped)
            {
                return Err(invalid());
            }
            return Ok(());
        }
        if final_deadline.is_some_and(|end| Instant::now() >= end) {
            return Err(invalid());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    fn stereo_config() -> Config {
        let mut value = capture_config_json();
        value["media_security"] = json!("pinned_tls13_stereo_v1");
        value["media"] = json!("127.0.0.1:0");
        parse_capture_config(&value).unwrap()
    }

    #[test]
    fn pure_secure_stereo_requires_explicit_finite_selected_source() {
        let config = stereo_config();
        assert_eq!(
            parse_capture_config(&config.wire()).unwrap().media_security,
            MediaSecurity::SecureStereo
        );
        for (key, replacement) in [
            ("duration_secs", json!(0)),
            ("duration_secs", json!(61)),
            ("media", json!("127.0.0.1:1234")),
            ("target", json!("default")),
            ("target", Value::Null),
            ("endpoint_mode", json!("virtual_speaker")),
            ("media_security", json!("pinned_tls13_mono_v1")),
        ] {
            let mut value = config.wire();
            value[key] = replacement;
            assert!(parse_capture_config(&value).is_err(), "{key}");
        }
        let mut playback = secure_config_json();
        playback["media_security"] = json!("pinned_tls13_stereo_v1");
        assert!(parse_config(&playback).is_err());
        assert!(
            config
                .media_security
                .warning()
                .contains("do not prove Android acceptance")
        );
    }

    #[test]
    fn pure_secure_stereo_rejects_cross_bound_approval_and_early_pcm() {
        let config = stereo_config();
        for (key, value) in [
            ("request_id", json!("00000000-0000-0000-0000-000000000000")),
            ("route_id", json!("00000000-0000-0000-0000-000000000000")),
            (
                "source",
                json!({"node_id":NodeId::new(),"capability_id":CapabilityId::new(),"port_id":PortId::new()}),
            ),
            (
                "sink",
                json!({"node_id":NodeId::new(),"capability_id":CapabilityId::new(),"port_id":PortId::new()}),
            ),
            ("media_security", json!("pinned_tls13_mono_v1")),
            ("format", json!("pcm-s16le-48000-stereo")),
            ("target", json!("other.source")),
        ] {
            let mut value_json = approval_json(&config);
            value_json[key] = value;
            let event = Event::parse(value_json).unwrap();
            assert!(
                Session {
                    listening: true,
                    ..Session::default()
                }
                .accept(&event, &config)
                .is_err(),
                "{key}"
            );
        }
        let mut session = Session {
            approved: true,
            offered: true,
            ..Session::default()
        };
        assert!(session.stereo_counts(&config, 0, 0, 1, 0).is_err());
        session.stereo_counts(&config, 0, 0, 0, 0).unwrap();
        session.stereo_bound = true;
        // A final partial local read is not a complete transported record.
        session.stereo_counts(&config, 0, 0, 1919, 0).unwrap();
        assert!(session.stereo_counts(&config, 0, 0, 1920, 0).is_err());
        assert!(
            session
                .stereo_counts(&config, u64::MAX, u64::MAX, u64::MAX, u64::MAX)
                .is_err()
        );
    }

    #[test]
    fn pure_secure_stereo_accepts_all_owner_failure_causes_without_cross_profile_admission() {
        let config = stereo_config();
        let mono = parse_config(&secure_config_json()).unwrap();
        // Exhaustive current SourceOwner::fail/observe and control/media classifiers.
        for cause in [
            "failure",
            "local_stop",
            "peer_stop",
            "media_closed",
            "media_timeout",
            "media_failure",
            "control_closed",
            "control_failure",
            "capture_closed",
            "capture_failure",
            "capture_timeout",
            "clock_regressed",
            "expired",
            "source_revoked",
            "session_changed",
            "catalog_changed",
            "negotiation_timeout",
        ] {
            let value = json!({"version":1,"event":"failed","packets_sent":1,
                "packets_captured":1,"bytes_read":1920,"transport_bytes_sent":2128,
                "terminal_cause":cause,"terminal_observed_at_ms":1});
            let event = Event::parse(value).unwrap();
            let mut session = Session {
                approved: true,
                offered: true,
                stereo_bound: true,
                ..Session::default()
            };
            session.accept(&event, &config).unwrap();
            assert_eq!(session.finished, Some(Finished::Failed), "{cause}");
            assert_eq!((session.packets, session.bytes), (1, 1920));
            assert!(
                Session {
                    approved: true,
                    offered: true,
                    stereo_bound: true,
                    ..Session::default()
                }
                .accept(&event, &mono)
                .is_err()
            );
        }
        for cause in [
            "control_failure",
            "capture_closed",
            "capture_failure",
            "capture_timeout",
            "clock_regressed",
            "expired",
            "source_revoked",
            "session_changed",
            "catalog_changed",
            "negotiation_timeout",
        ] {
            let event=Event::parse(json!({"version":1,"event":"failed","packets_written":1,
                "bytes_written":960,"packets_dropped_busy":0,"terminal_cause":cause,"terminal_observed_at_ms":1})).unwrap();
            assert!(
                Session {
                    approved: true,
                    offered: true,
                    ..Session::default()
                }
                .accept(&event, &mono)
                .is_err(),
                "{cause}"
            );
        }
        assert!(
            Event::parse(json!({"version":1,"event":"failed","packets_sent":0,
            "packets_captured":0,"bytes_read":0,"transport_bytes_sent":0,
            "terminal_cause":"unknown_future_cause","terminal_observed_at_ms":1}))
            .is_err()
        );
    }

    #[test]
    fn pure_secure_stereo_fresh_consent_and_ordered_bound_counters() {
        let config = stereo_config();
        let approval = approval(&config);
        let route = approval.route_id;
        let mut session = Session {
            listening: true,
            approval: Some(approval),
            ..Session::default()
        };
        assert!(input_action(b"start", &session).is_err());
        assert!(matches!(
            input_action(
                session.approval.as_ref().unwrap().expected().as_bytes(),
                &session
            )
            .unwrap(),
            Input::Approve
        ));
        let waiting = json!({"version":1,"event":"awaiting_peer_approval","route_id":route});
        assert!(
            session
                .accept(&Event::parse(waiting.clone()).unwrap(), &config)
                .is_err()
        );
        session.approved = true;
        session
            .accept(&Event::parse(waiting.clone()).unwrap(), &config)
            .unwrap();
        assert!(
            session
                .accept(&Event::parse(waiting).unwrap(), &config)
                .is_err()
        );
        session
            .accept(
                &Event::parse(json!({"version":1,"event":"awaiting_peer_offer","route_id":route}))
                    .unwrap(),
                &config,
            )
            .unwrap();
        let status = json!({"version":1,"event":"status","route_id":route,"state":"starting","media_security":"pinned_tls13_stereo_v1",
            "packets_sent":1,"packets_captured":2,"bytes_read":3840,"transport_bytes_sent":2128});
        assert!(
            session
                .accept(&Event::parse(status.clone()).unwrap(), &config)
                .is_err()
        );
        session
            .accept(
                &Event::parse(json!({"version":1,"event":"bound","route_id":route})).unwrap(),
                &config,
            )
            .unwrap();
        session
            .accept(&Event::parse(status.clone()).unwrap(), &config)
            .unwrap();
        for (key, value) in [
            ("transport_bytes_sent", json!(1920)),
            ("bytes_read", json!(2128)),
            ("packets_sent", json!(3)),
            ("state", json!("active")),
        ] {
            let mut bad = status.clone();
            bad[key] = value;
            assert!(
                session
                    .accept(&Event::parse(bad).unwrap(), &config)
                    .is_err(),
                "{key}"
            );
        }
        let mut extra = status;
        extra["packets_written"] = json!(0);
        assert!(Event::parse(extra).is_err());
        session.accept(&Event::parse(json!({"version":1,"event":"stopped","packets_sent":1,"packets_captured":2,"bytes_read":3840,
            "transport_bytes_sent":2128,"terminal_cause":"local_stop","terminal_observed_at_ms":1})).unwrap(), &config).unwrap();
        assert_eq!(session.finished, Some(Finished::Stopped));
    }

    fn config_json() -> Value {
        json!({"version":1,"node_id":NodeId::new(),
            "source":{"node_id":NodeId::new(),"capability_id":CapabilityId::new(),"port_id":PortId::new()},
            "listen":"127.0.0.1:0","media":"127.0.0.1:40231",
            "target":"capyio.synthetic.sink","duration_secs":20})
    }
    fn parse_config(value: &Value) -> io::Result<Config> {
        Config::parse(
            &serde_json::to_vec(value).unwrap(),
            serde_json::from_value(value["node_id"].clone()).unwrap(),
            serde_json::from_value(value["source"]["node_id"].clone()).unwrap(),
            Direction::Playback,
        )
    }
    fn capture_config_json() -> Value {
        let mut value = config_json();
        let remote = value.as_object_mut().unwrap().remove("source").unwrap();
        value["sink"] = remote;
        value["target"] = json!("capyio.synthetic.source");
        value
    }
    fn parse_capture_config(value: &Value) -> io::Result<Config> {
        Config::parse(
            &serde_json::to_vec(value).unwrap(),
            serde_json::from_value(value["node_id"].clone()).unwrap(),
            serde_json::from_value(value["sink"]["node_id"].clone()).unwrap(),
            Direction::Capture,
        )
    }
    fn approval_json(config: &Config) -> Value {
        let local = Port {
            node_id: config.node_id,
            capability_id: CapabilityId::new(),
            port_id: PortId::new(),
        };
        let (source, sink) = match config.direction {
            Direction::Playback => (config.remote, local),
            Direction::Capture => (local, config.remote),
        };
        let node_name = match config.endpoint_mode {
            Mode::Playback | Mode::StereoCapture => None,
            Mode::VirtualMicrophone => Some(format!("capyio.virtual-microphone.{}", local.port_id)),
            Mode::VirtualSpeaker => Some(format!("capyio.virtual-speaker.{}", local.port_id)),
        };
        json!({"version":1,"event":"approval_required","request_id":MessageId::new(),
            "route_id":RouteId::new(),"session_id":SessionId::new(),"source":source,
            "sink":sink,"target":config.target,"endpoint_mode":config.endpoint_mode,
            "node_name":node_name,
            "format":config.format(),"duration_secs":config.duration_secs,"approval_timeout_secs":30,"media_security":config.media_security.name()})
    }
    fn approval(config: &Config) -> Approval {
        let Event::Approval(value) = Event::parse(approval_json(config)).unwrap() else {
            panic!("expected typed approval")
        };
        *value
    }

    fn secure_config_json() -> Value {
        let mut value = config_json();
        value["media_security"] = json!("pinned_tls13_mono_v1");
        value["media"] = json!("127.0.0.1:0");
        value
    }

    fn secure_virtual_config_json() -> Value {
        let mut value = secure_config_json();
        value["endpoint_mode"] = json!("virtual_microphone");
        value["target"] = Value::Null;
        value
    }

    fn all_secure_configs() -> Vec<Value> {
        [MediaSecurity::Secure, MediaSecurity::Secure300]
            .into_iter()
            .flat_map(|security| {
                [secure_config_json(), secure_virtual_config_json()].map(|mut value| {
                    value["media_security"] = json!(security);
                    value
                })
            })
            .collect()
    }

    #[test]
    fn pure_secure_300_profile_duration_boundaries_and_closed_selection() {
        use clap::ValueEnum;
        assert_eq!(
            MediaSecurity::from_str("pinned_tls13_mono_300s_v1", false).unwrap(),
            MediaSecurity::Secure300
        );
        assert_eq!(MediaSecurity::default(), MediaSecurity::Legacy);
        for mut value in all_secure_configs() {
            let security: MediaSecurity =
                serde_json::from_value(value["media_security"].clone()).unwrap();
            for seconds in [0, 1, 60, 61, 299, 300, 301, u64::MAX] {
                value["duration_secs"] = json!(seconds);
                assert_eq!(
                    parse_config(&value).is_ok(),
                    (1..=security.max_duration_secs()).contains(&seconds)
                );
            }
            value["duration_secs"] = json!(1.5);
            assert!(parse_config(&value).is_err());
        }
        let mut legacy = config_json();
        legacy["duration_secs"] = json!(300);
        assert_eq!(
            parse_config(&legacy).unwrap().media_security,
            MediaSecurity::Legacy
        );
        for mode in ["stereo_capture", "virtual_speaker"] {
            let mut value = secure_config_json();
            value["media_security"] = json!(MediaSecurity::Secure300);
            value["endpoint_mode"] = json!(mode);
            assert!(parse_config(&value).is_err());
        }
    }

    #[test]
    fn pure_secure_300_checked_counters_and_cross_profile_rejection() {
        let mut value = secure_virtual_config_json();
        value["media_security"] = json!(MediaSecurity::Secure300);
        value["duration_secs"] = json!(300);
        let mut config = parse_config(&value).unwrap();
        assert_eq!(
            config.format(),
            capyio_protocol::SECURE_AUDIO_MONO_300S_FORMAT
        );
        assert!(
            Session::default()
                .summary(&config)
                .contains("at most 300 seconds")
        );
        let approved = approval(&config);
        for (field, replacement) in [
            ("media_security", json!(MediaSecurity::Secure)),
            ("format", json!(capyio_protocol::SECURE_AUDIO_MONO_FORMAT)),
            ("duration_secs", json!(60)),
        ] {
            let mut bad = approval_json(&config);
            bad[field] = replacement;
            let Event::Approval(bad) = Event::parse(bad).unwrap() else {
                panic!()
            };
            assert!(bad.validate(&config).is_err());
        }
        let status = json!({"version":1,"event":"status","route_id":approved.route_id,"state":"starting",
            "packets_written":30000,"packets_sent":0,"bytes_written":28800000,"media_security":"pinned_tls13_mono_300s_v1"});
        let mut session = Session {
            approval: Some(approved),
            approved: true,
            offered: true,
            ..Session::default()
        };
        session
            .accept(&Event::parse(status.clone()).unwrap(), &config)
            .unwrap();
        for (field, replacement) in [
            ("packets_written", json!(30001)),
            ("bytes_written", json!(28800001)),
            ("packets_written", json!(u64::MAX)),
            ("bytes_written", json!(u64::MAX)),
            ("state", json!("active")),
            ("media_security", json!(MediaSecurity::Secure)),
        ] {
            let mut bad = status.clone();
            bad[field] = replacement;
            assert!(
                session
                    .accept(&Event::parse(bad).unwrap(), &config)
                    .is_err()
            );
            assert_eq!((session.packets, session.bytes), (30000, 28800000));
        }
        config.duration_secs = u64::MAX;
        assert!(
            session
                .accept(&Event::parse(status).unwrap(), &config)
                .is_err()
        );
    }

    #[test]
    fn pure_secure_virtual_config_requires_owned_endpoint_without_target() {
        let value = secure_virtual_config_json();
        let config = parse_config(&value).unwrap();
        assert_eq!(config.endpoint_mode, Mode::VirtualMicrophone);
        assert_eq!(config.direction, Direction::Playback);
        assert_eq!(config.format(), capyio_protocol::SECURE_AUDIO_MONO_FORMAT);
        assert!(config.wire()["target"].is_null());
        assert_eq!(
            parse_config(&config.wire()).unwrap().endpoint_mode,
            Mode::VirtualMicrophone
        );
        let mut omitted = value.clone();
        omitted.as_object_mut().unwrap().remove("target");
        assert!(parse_config(&omitted).is_ok());
        for (field, replacement) in [
            ("target", json!("synthetic.target")),
            ("target", json!("default")),
            ("endpoint_mode", json!("playback")),
            ("endpoint_mode", json!("stereo_capture")),
            ("endpoint_mode", json!("virtual_speaker")),
            ("node_name", json!("capyio.virtual-microphone.caller")),
            ("media", json!("127.0.0.1:40231")),
            ("duration_secs", json!(61)),
            ("duration_secs", json!(0)),
        ] {
            let mut bad = value.clone();
            bad[field] = replacement;
            assert!(parse_config(&bad).is_err(), "{field}");
        }
        let summary = Session::default().summary(&config);
        assert!(summary.contains("new owned virtual microphone"));
        assert!(summary.contains("application consumption"));
        assert!(!summary.contains(WARNING));
    }

    #[test]
    fn pure_secure_virtual_approval_binds_exact_sink_name_and_closed_fields() {
        let config = parse_config(&secure_virtual_config_json()).unwrap();
        let original = approval_json(&config);
        approval(&config).validate(&config).unwrap();
        for (field, replacement) in [
            ("request_id", json!("00000000-0000-0000-0000-000000000000")),
            ("route_id", json!("00000000-0000-0000-0000-000000000000")),
            ("session_id", json!("00000000-0000-0000-0000-000000000000")),
            ("target", json!("synthetic.target")),
            ("endpoint_mode", json!("playback")),
            ("endpoint_mode", json!("virtual_speaker")),
            ("node_name", Value::Null),
            (
                "node_name",
                json!(format!(
                    "capyio.virtual-microphone.{}",
                    config.remote.port_id
                )),
            ),
            (
                "node_name",
                json!(format!("capyio.virtual-microphone.{}", PortId::new())),
            ),
            (
                "node_name",
                json!(format!(
                    "capyio.virtual-speaker.{}",
                    original["sink"]["port_id"].as_str().unwrap()
                )),
            ),
            ("format", json!("pcm-s16le-48000-mono")),
            ("media_security", json!(SECURITY)),
            ("duration_secs", json!(21)),
            ("approval_timeout_secs", json!(31)),
        ] {
            let mut bad = original.clone();
            bad[field] = replacement;
            let Event::Approval(bad) = Event::parse(bad).unwrap() else {
                panic!()
            };
            assert!(bad.validate(&config).is_err(), "{field}");
        }
        for endpoint in ["source", "sink"] {
            for field in ["node_id", "capability_id", "port_id"] {
                let mut bad = original.clone();
                bad[endpoint][field] = json!("00000000-0000-0000-0000-000000000000");
                let Event::Approval(bad) = Event::parse(bad).unwrap() else {
                    panic!()
                };
                assert!(bad.validate(&config).is_err(), "{endpoint}.{field}");
            }
        }
        for field in ["node_name", "target", "session_id", "media_security"] {
            let mut bad = original.clone();
            bad.as_object_mut().unwrap().remove(field);
            assert!(Event::parse(bad).is_err(), "{field}");
        }
        let mut bad = original;
        bad["profile"] = json!("capyio.audio.frames/1");
        assert!(Event::parse(bad).is_err());
    }

    #[test]
    fn pure_secure_playback_config_is_explicit_and_finite() {
        let value = secure_config_json();
        let config = parse_config(&value).unwrap();
        assert_eq!(config.format(), capyio_protocol::SECURE_AUDIO_MONO_FORMAT);
        assert_eq!(
            parse_config(&config.wire()).unwrap().media_security,
            MediaSecurity::Secure
        );
        for (key, replacement) in [
            ("media_security", json!("tls13_mtls_v1")),
            ("media_security", Value::Null),
            ("media", json!("127.0.0.1:40231")),
            ("endpoint_mode", json!("virtual_microphone")),
            ("proposal_mode", json!("incoming_draft")),
            ("duration_secs", json!(61)),
            ("duration_secs", json!(0)),
            ("target", Value::Null),
            ("target", json!("default")),
        ] {
            let mut bad = value.clone();
            bad[key] = replacement;
            assert!(parse_config(&bad).is_err(), "{key}");
        }
        let mut capture = capture_config_json();
        capture["media_security"] = json!("pinned_tls13_mono_v1");
        assert!(parse_capture_config(&capture).is_err());
        let summary = Session::default().summary(&config);
        assert!(summary.contains("do not prove audibility"));
        assert!(!summary.contains(WARNING));
    }

    #[test]
    fn pure_secure_terminal_observation_is_numeric_and_never_legacy() {
        let secure = parse_config(&secure_config_json()).unwrap();
        let legacy = parse_config(&config_json()).unwrap();
        let value = json!({"version":1,"event":"failed","terminal_observed_at_ms":123});
        let event = Event::parse(value.clone()).unwrap();
        assert!(Session::default().accept(&event, &legacy).is_err());
        let mut final_counts = value.clone();
        final_counts["packets_written"] = json!(2);
        final_counts["bytes_written"] = json!(2000);
        final_counts["packets_dropped_busy"] = json!(1);
        final_counts["terminal_cause"] = json!("media_closed");
        for cause in [
            "failure",
            "local_stop",
            "peer_stop",
            "media_closed",
            "media_timeout",
            "media_failure",
            "control_closed",
        ] {
            let mut candidate = final_counts.clone();
            candidate["terminal_cause"] = json!(cause);
            assert!(Event::parse(candidate).is_ok());
        }
        for cause in [
            Value::Null,
            json!(1),
            json!("raw error detail"),
            json!("eof"),
        ] {
            let mut bad = final_counts.clone();
            bad["terminal_cause"] = cause;
            assert!(Event::parse(bad).is_err());
        }
        let mut missing_cause = final_counts.clone();
        missing_cause
            .as_object_mut()
            .unwrap()
            .remove("terminal_cause");
        assert!(Event::parse(missing_cause).is_err());
        let mut cause_without_counters = value.clone();
        cause_without_counters["terminal_cause"] = json!("failure");
        assert!(Event::parse(cause_without_counters).is_err());
        for key in ["packets_written", "bytes_written", "packets_dropped_busy"] {
            let mut bad = final_counts.clone();
            bad.as_object_mut().unwrap().remove(key);
            assert!(Event::parse(bad).is_err());
            let mut bad = final_counts.clone();
            bad[key] = Value::Null;
            assert!(Event::parse(bad).is_err());
        }
        let counts = Event::parse(final_counts).unwrap();
        let mut owned = Session {
            approved: true,
            offered: true,
            ..Session::default()
        };
        owned.accept(&counts, &secure).unwrap();
        assert!(Session::default().accept(&counts, &secure).is_err());
        assert!(Session::default().accept(&counts, &legacy).is_err());
        let mut session = Session::default();
        session.accept(&event, &secure).unwrap();
        assert!(session.accept(&event, &secure).is_err());
        for invalid in [Value::Null, json!(-1), json!("123"), json!(1.5)] {
            let mut bad = value.clone();
            bad["terminal_observed_at_ms"] = invalid;
            assert!(Event::parse(bad).is_err());
        }
        assert!(
            Session::default()
                .accept(
                    &Event::parse(json!({"version":1,"event":"failed"})).unwrap(),
                    &secure
                )
                .is_ok()
        );
    }

    #[test]
    fn pure_secure_consent_requires_fresh_exact_request_route_peer() {
        for value in all_secure_configs() {
            let config = parse_config(&value).unwrap();
            let current = approval(&config);
            let correct = current.expected();
            let stale = approval(&config).expected();
            let mut session = Session {
                listening: true,
                approval: Some(current),
                ..Session::default()
            };
            assert_eq!(
                input_action(correct.as_bytes(), &session).unwrap(),
                Input::Approve
            );
            assert!(input_action(stale.as_bytes(), &session).is_err());
            assert!(input_action(format!("{correct} extra").as_bytes(), &session).is_err());
            session.approved = true;
            assert!(input_action(correct.as_bytes(), &session).is_err());
            let text = serde_json::to_string(&value).unwrap();
            let duplicate = text.replacen(
                "{",
                "{\"media_security\":\"unauthenticated_udp_trusted_lab_only\",",
                1,
            );
            assert!(
                Config::parse(
                    duplicate.as_bytes(),
                    config.node_id,
                    config.remote.node_id,
                    Direction::Playback
                )
                .is_err()
            );
        }
    }

    #[test]
    fn pure_secure_approval_profile_and_status_are_immutable() {
        for value in all_secure_configs() {
            let config = parse_config(&value).unwrap();
            let original = approval_json(&config);
            for (key, replacement) in [
                ("format", json!("pcm-s16le-48000-mono")),
                ("media_security", json!(SECURITY)),
                ("target", json!("different.sink")),
                ("duration_secs", json!(21)),
            ] {
                let mut bad = original.clone();
                bad[key] = replacement;
                let Event::Approval(bad) = Event::parse(bad).unwrap() else {
                    panic!()
                };
                assert!(bad.validate(&config).is_err());
            }
            let approved = approval(&config);
            let route = approved.route_id;
            let mut session = Session {
                listening: true,
                approval: Some(approved),
                approved: true,
                offered: true,
                ..Session::default()
            };
            let status = json!({"version":1,"event":"status","route_id":route,"state":"starting",
                "packets_written":3,"packets_sent":0,"bytes_written":2880,"media_security":config.media_security});
            session
                .accept(&Event::parse(status.clone()).unwrap(), &config)
                .unwrap();
            for (key, replacement) in [
                ("state", json!("active")),
                ("state", json!("draft")),
                ("route_id", json!(RouteId::new())),
                ("packets_sent", json!(1)),
                ("packets_written", json!(2)),
                ("bytes_written", json!(2879)),
            ] {
                let mut bad = status.clone();
                bad[key] = replacement;
                assert!(
                    session
                        .accept(&Event::parse(bad).unwrap(), &config)
                        .is_err(),
                    "{key}"
                );
                assert_eq!(session.packets, 3);
                assert_eq!(session.bytes, 2880);
            }
            for key in ["bytes_written", "media_security"] {
                let mut bad = status.clone();
                bad.as_object_mut().unwrap().remove(key);
                assert!(Event::parse(bad).is_err());
            }
            let mut partial = status.clone();
            partial["bytes_written"] = json!(3000);
            session
                .accept(&Event::parse(partial).unwrap(), &config)
                .unwrap();
            assert_eq!(session.bytes, 3000);
            let legacy = parse_config(&config_json()).unwrap();
            assert!(
                session
                    .accept(&Event::parse(status).unwrap(), &legacy)
                    .is_err()
            );
        }
    }

    #[test]
    fn pure_playback_config_matches_public_schema_and_exact_peer_selection() {
        let value = config_json();
        let config = parse_config(&value).unwrap();
        assert_eq!(config.endpoint_mode, Mode::Playback);
        assert!(
            config
                .validate(NodeId::new(), config.remote.node_id)
                .is_err()
        );
        assert!(config.validate(config.node_id, NodeId::new()).is_err());
        let mut virtual_mic = value.clone();
        virtual_mic["endpoint_mode"] = json!("virtual_microphone");
        virtual_mic["target"] = Value::Null;
        assert!(parse_config(&virtual_mic).is_ok());
        for mode in ["incoming_draft", "capture", "virtual_speaker"] {
            let mut candidate = value.clone();
            candidate["proposal_mode"] = json!(mode);
            assert!(parse_config(&candidate).is_err());
        }
        let mut candidate = value;
        candidate["source"]["unexpected"] = json!(true);
        assert!(parse_config(&candidate).is_err());
    }

    #[test]
    fn pure_playback_config_rejects_implicit_devices_and_invalid_addresses() {
        let value = config_json();
        for target in [
            "auto", "AUTO", "default", "Default", "0", "-1", "", "x\n", "a b",
        ] {
            let mut candidate = value.clone();
            candidate["target"] = json!(target);
            assert!(parse_config(&candidate).is_err(), "target {target:?}");
        }
        for address in [
            "0.0.0.0:1",
            "224.0.0.1:1",
            "8.8.8.8:1",
            "127.0.0.1:0",
            "127.0.0.255:1",
            "[::1]:1",
        ] {
            let mut candidate = value.clone();
            candidate["media"] = json!(address);
            assert!(parse_config(&candidate).is_err(), "address {address}");
        }
        for seconds in [0, 301, u64::MAX] {
            let mut candidate = value.clone();
            candidate["duration_secs"] = json!(seconds);
            assert!(parse_config(&candidate).is_err());
        }
        for address in [
            "10.2.3.4:2",
            "192.168.2.3:2",
            "100.64.0.1:2",
            "100.127.0.1:2",
        ] {
            assert!(lab_address(address.parse().unwrap()));
        }
        assert!(!lab_address("100.128.0.1:2".parse().unwrap()));
        let mut candidate = value;
        candidate["media"] = json!("127.0.0.1:1");
        assert!(parse_config(&candidate).is_err());
    }

    #[test]
    fn pure_playback_config_file_is_regular_owned_bounded_and_not_a_symlink() {
        let config = parse_config(&config_json()).unwrap();
        let directory = std::env::temp_dir().join(format!(
            "capyio-console-config-test-{}-{}",
            std::process::id(),
            NodeId::new()
        ));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        struct RemoveOwnedFixture(PathBuf);
        impl Drop for RemoveOwnedFixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _fixture = RemoveOwnedFixture(directory.clone());
        let path = directory.join("public-config.json");
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        file.write_all(&serde_json::to_vec(&config.wire()).unwrap())
            .unwrap();
        drop(file);
        assert!(
            read_config(
                &path,
                config.node_id,
                config.remote.node_id,
                Direction::Playback
            )
            .is_ok()
        );
        assert!(
            read_config(
                &path,
                NodeId::new(),
                config.remote.node_id,
                Direction::Playback
            )
            .is_err()
        );
        let link = directory.join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(
            read_config(
                &link,
                config.node_id,
                config.remote.node_id,
                Direction::Playback
            )
            .is_err()
        );
        assert!(
            read_config(
                &directory,
                config.node_id,
                config.remote.node_id,
                Direction::Playback
            )
            .is_err()
        );
        assert!(
            read_config(
                Path::new("relative.json"),
                config.node_id,
                config.remote.node_id,
                Direction::Playback,
            )
            .is_err()
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o620)).unwrap();
        assert!(
            read_config(
                &path,
                config.node_id,
                config.remote.node_id,
                Direction::Playback
            )
            .is_err()
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&path, [b' '; MAX_FRAME + 1]).unwrap();
        assert!(
            read_config(
                &path,
                config.node_id,
                config.remote.node_id,
                Direction::Playback
            )
            .is_err()
        );
        fs::write(&path, []).unwrap();
        assert!(
            read_config(
                &path,
                config.node_id,
                config.remote.node_id,
                Direction::Playback
            )
            .is_err()
        );
    }

    #[test]
    fn pure_playback_approval_binds_every_resource_and_security_field() {
        let config = parse_config(&config_json()).unwrap();
        let valid = approval_json(&config);
        approval(&config).validate(&config).unwrap();
        for (field, replacement) in [
            ("request_id", json!("00000000-0000-0000-0000-000000000000")),
            ("route_id", json!("00000000-0000-0000-0000-000000000000")),
            ("session_id", json!("00000000-0000-0000-0000-000000000000")),
            ("target", json!("other-device")),
            ("endpoint_mode", json!("virtual_microphone")),
            ("node_name", json!("unexpected")),
            ("format", json!("pcm-s16le-48000-stereo")),
            ("duration_secs", json!(21)),
            ("approval_timeout_secs", json!(31)),
            ("media_security", json!("secure")),
        ] {
            let mut candidate = valid.clone();
            candidate[field] = replacement;
            let Event::Approval(value) = Event::parse(candidate).unwrap() else {
                panic!()
            };
            assert!(value.validate(&config).is_err(), "field {field}");
        }
        for endpoint in ["source", "sink"] {
            let mut candidate = valid.clone();
            candidate[endpoint]["node_id"] = json!(NodeId::new());
            let Event::Approval(value) = Event::parse(candidate).unwrap() else {
                panic!()
            };
            assert!(value.validate(&config).is_err());
        }
        let mut candidate = valid.clone();
        candidate["source"]["capability_id"] = json!(CapabilityId::new());
        let Event::Approval(value) = Event::parse(candidate).unwrap() else {
            panic!()
        };
        assert!(value.validate(&config).is_err());
        let mut candidate = valid;
        candidate.as_object_mut().unwrap().remove("node_name");
        assert!(Event::parse(candidate).is_err());
    }

    #[test]
    fn pure_playback_virtual_microphone_requires_exact_owned_sink_identity() {
        let mut value = config_json();
        value["endpoint_mode"] = json!("virtual_microphone");
        value["target"] = Value::Null;
        let config = parse_config(&value).unwrap();
        let mut event = approval(&config);
        event.validate(&config).unwrap();
        event.node_name = Some(format!("capyio.virtual-microphone.{}", PortId::new()));
        assert!(event.validate(&config).is_err());
        event.node_name = None;
        assert!(event.validate(&config).is_err());
    }

    #[test]
    fn pure_playback_events_reject_unknown_fields_and_early_or_rebound_routes() {
        let config = parse_config(&config_json()).unwrap();
        let mut session = Session::default();
        let event = Event::Approval(Box::new(approval(&config)));
        assert!(session.accept(&event, &config).is_err());
        let listening =
            Event::parse(json!({"version":1,"event":"listening","address":"127.0.0.1:40001"}))
                .unwrap();
        session.accept(&listening, &config).unwrap();
        assert!(session.accept(&listening, &config).is_err());
        session.accept(&event, &config).unwrap();
        session.approval = Some(approval(&config));
        assert!(session.accept(&event, &config).is_err());
        let route = session.approval.as_ref().unwrap().route_id;
        let offer =
            Event::parse(json!({"version":1,"event":"awaiting_peer_offer","route_id":route}))
                .unwrap();
        assert!(session.accept(&offer, &config).is_err());
        session.approved = true;
        session.accept(&offer, &config).unwrap();
        assert!(session.accept(&offer, &config).is_err());
        let mut status = json!({"version":1,"event":"status","route_id":route,"state":"active","packets_written":3,"packets_sent":0});
        session
            .accept(&Event::parse(status.clone()).unwrap(), &config)
            .unwrap();
        status["packets_written"] = json!(2);
        assert!(
            session
                .accept(&Event::parse(status.clone()).unwrap(), &config)
                .is_err()
        );
        status["packets_written"] = json!(4);
        status["route_id"] = json!(RouteId::new());
        assert!(
            session
                .accept(&Event::parse(status).unwrap(), &config)
                .is_err()
        );
        for value in [
            json!({"version":1,"event":"stopped","secret":"hidden"}),
            json!({"version":1,"event":"status","stage":"awaiting_incoming_draft"}),
            json!({"version":2,"event":"stopped"}),
            json!({"version":1,"event":"unknown"}),
        ] {
            assert!(Event::parse(value).is_err());
        }
    }

    #[test]
    fn pure_playback_terminal_controls_require_single_use_exact_consent() {
        let config = parse_config(&config_json()).unwrap();
        let mut session = Session::default();
        for line in [
            b"yes".as_slice(),
            b"start",
            b"start stale stale stale",
            b"status\nstop",
        ] {
            assert!(input_action(line, &session).is_err());
        }
        assert_eq!(input_action(b"status", &session).unwrap(), Input::Status);
        assert_eq!(input_action(b"stop", &session).unwrap(), Input::Stop);
        assert_eq!(input_action(b"cancel", &session).unwrap(), Input::Stop);
        session.approval = Some(approval(&config));
        let expected = session.approval.as_ref().unwrap().expected();
        assert_eq!(
            input_action(expected.as_bytes(), &session).unwrap(),
            Input::Approve
        );
        assert!(input_action(format!("{expected} ").as_bytes(), &session).is_err());
        assert!(input_action(format!("{expected}\n{expected}").as_bytes(), &session).is_err());
        let command = session.approval.as_ref().unwrap().command();
        exact_keys(&command, &["version", "command", "request_id", "route_id"]).unwrap();
        assert_eq!(command["command"], "approve");
        session.approved = true;
        assert!(input_action(expected.as_bytes(), &session).is_err());
        session.stopping = true;
        assert!(input_action(b"status", &session).is_err());
        assert_eq!(input_action(b"stop", &session).unwrap(), Input::Stop);
    }

    #[test]
    fn pure_capture_config_matches_distinct_sink_and_selected_source_schema() {
        let value = capture_config_json();
        let config = parse_capture_config(&value).unwrap();
        assert_eq!(config.direction, Direction::Capture);
        assert_eq!(config.endpoint_mode, Mode::StereoCapture);
        assert!(
            config
                .validate(NodeId::new(), config.remote.node_id)
                .is_err()
        );
        assert!(config.validate(config.node_id, NodeId::new()).is_err());
        let encoded = config.wire();
        exact_keys(
            &encoded,
            &[
                "version",
                "node_id",
                "sink",
                "listen",
                "media",
                "target",
                "endpoint_mode",
                "duration_secs",
            ],
        )
        .unwrap();
        assert_eq!(encoded["sink"], value["sink"]);
        assert_eq!(encoded["endpoint_mode"], "selected_source");
        assert!(parse_capture_config(&encoded).is_ok());
        assert!(
            Config::parse(
                &serde_json::to_vec(&encoded).unwrap(),
                config.node_id,
                config.remote.node_id,
                Direction::Playback
            )
            .is_err()
        );
        let playback = parse_config(&config_json()).unwrap();
        assert!(
            Config::parse(
                &serde_json::to_vec(&playback.wire()).unwrap(),
                playback.node_id,
                playback.remote.node_id,
                Direction::Capture
            )
            .is_err()
        );
        for mode in [
            "playback",
            "virtual_microphone",
            "stereo_capture",
            "capture",
            "incoming_draft",
        ] {
            let mut candidate = value.clone();
            candidate["endpoint_mode"] = json!(mode);
            assert!(parse_capture_config(&candidate).is_err(), "mode {mode}");
        }
        for (field, replacement) in [
            ("source", value["sink"].clone()),
            ("proposal_mode", json!("linux_led")),
            ("credentials", json!("not permitted")),
        ] {
            let mut candidate = value.clone();
            candidate[field] = replacement;
            assert!(parse_capture_config(&candidate).is_err(), "field {field}");
        }
        let mut candidate = value;
        candidate["sink"]["unexpected"] = json!(true);
        assert!(parse_capture_config(&candidate).is_err());
    }

    #[test]
    fn pure_capture_config_rejects_implicit_devices_and_invalid_addresses() {
        let value = capture_config_json();
        for target in [
            Value::Null,
            json!("auto"),
            json!("AUTO"),
            json!("default"),
            json!("Default"),
            json!("0"),
            json!("-1"),
            json!(""),
            json!("x\n"),
            json!("a b"),
            json!("x".repeat(129)),
        ] {
            let mut candidate = value.clone();
            candidate["target"] = target;
            assert!(parse_capture_config(&candidate).is_err());
        }
        for address in [
            "0.0.0.0:1",
            "224.0.0.1:1",
            "8.8.8.8:1",
            "127.0.0.1:0",
            "127.0.0.255:1",
            "[::1]:1",
            "127.0.0.2:2",
            "127.0.0.1:1",
        ] {
            let mut candidate = value.clone();
            candidate["media"] = json!(address);
            assert!(
                parse_capture_config(&candidate).is_err(),
                "address {address}"
            );
        }
        for seconds in [0, 301, u64::MAX] {
            let mut candidate = value.clone();
            candidate["duration_secs"] = json!(seconds);
            assert!(parse_capture_config(&candidate).is_err());
        }
        for field in ["node_id", "capability_id", "port_id"] {
            let mut candidate = value.clone();
            candidate["sink"][field] = json!("00000000-0000-0000-0000-000000000000");
            assert!(parse_capture_config(&candidate).is_err());
        }
        for config in [
            parse_config(&config_json()).unwrap(),
            parse_capture_config(&value).unwrap(),
        ] {
            for bytes in [
                Vec::new(),
                vec![b' '; MAX_FRAME + 1],
                b"{}".to_vec(),
                b"{\"version\":1,\"version\":1}".to_vec(),
            ] {
                assert!(
                    Config::parse(
                        &bytes,
                        config.node_id,
                        config.remote.node_id,
                        config.direction
                    )
                    .is_err()
                );
            }
            assert!(
                Config::parse(
                    &serde_json::to_vec(&config.wire()).unwrap(),
                    config.node_id,
                    config.node_id,
                    config.direction
                )
                .is_err()
            );
        }
    }

    #[test]
    fn pure_capture_virtual_speaker_requires_exact_owned_source_identity() {
        let mut value = capture_config_json();
        value["endpoint_mode"] = json!("virtual_speaker");
        assert!(parse_capture_config(&value).is_err());
        value.as_object_mut().unwrap().remove("target");
        let config = parse_capture_config(&value).unwrap();
        assert_eq!(config.wire()["endpoint_mode"], "virtual_speaker");
        assert!(config.wire()["target"].is_null());
        let mut event = approval(&config);
        event.validate(&config).unwrap();
        assert_eq!(
            event.node_name,
            Some(format!("capyio.virtual-speaker.{}", event.source.port_id))
        );
        for name in [
            None,
            Some(format!("capyio.virtual-speaker.{}", event.sink.port_id)),
            Some(format!(
                "capyio.virtual-microphone.{}",
                event.source.port_id
            )),
            Some(format!("capyio.virtual-speaker.{}", PortId::new())),
        ] {
            event.node_name = name;
            assert!(event.validate(&config).is_err());
        }
        value["target"] = Value::Null;
        assert!(parse_capture_config(&value).is_ok());
        for mode in ["stereo_capture", "virtual_speaker", "selected_source"] {
            let mut playback = config_json();
            playback["endpoint_mode"] = json!(mode);
            assert!(parse_config(&playback).is_err());
        }
    }

    #[test]
    fn pure_capture_approval_binds_every_resource_and_security_field() {
        let config = parse_capture_config(&capture_config_json()).unwrap();
        let valid = approval_json(&config);
        approval(&config).validate(&config).unwrap();
        for (field, replacement) in [
            ("request_id", json!("00000000-0000-0000-0000-000000000000")),
            ("route_id", json!("00000000-0000-0000-0000-000000000000")),
            ("session_id", json!("00000000-0000-0000-0000-000000000000")),
            ("target", json!("other-device")),
            ("endpoint_mode", json!("playback")),
            ("endpoint_mode", json!("virtual_speaker")),
            ("node_name", json!("unexpected")),
            ("format", json!("pcm-s16le-48000-mono")),
            ("duration_secs", json!(21)),
            ("approval_timeout_secs", json!(31)),
            ("media_security", json!("secure")),
        ] {
            let mut candidate = valid.clone();
            candidate[field] = replacement;
            let Event::Approval(value) = Event::parse(candidate).unwrap() else {
                panic!()
            };
            assert!(value.validate(&config).is_err(), "field {field}");
        }
        for endpoint in ["source", "sink"] {
            for field in ["node_id", "capability_id", "port_id"] {
                let mut candidate = valid.clone();
                candidate[endpoint][field] = json!("00000000-0000-0000-0000-000000000000");
                let Event::Approval(value) = Event::parse(candidate).unwrap() else {
                    panic!()
                };
                assert!(value.validate(&config).is_err(), "{endpoint}.{field}");
            }
        }
        for field in ["node_id", "capability_id", "port_id"] {
            let mut candidate = valid.clone();
            candidate["sink"][field] = json!(NodeId::new());
            let Event::Approval(value) = Event::parse(candidate).unwrap() else {
                panic!()
            };
            assert!(value.validate(&config).is_err(), "remote {field}");
        }
        let mut candidate = valid.clone();
        candidate["source"] = valid["sink"].clone();
        candidate["sink"] = valid["source"].clone();
        let Event::Approval(value) = Event::parse(candidate).unwrap() else {
            panic!()
        };
        assert!(value.validate(&config).is_err());
        for field in ["target", "node_name"] {
            let mut candidate = valid.clone();
            candidate.as_object_mut().unwrap().remove(field);
            assert!(Event::parse(candidate).is_err());
        }
        for (field, replacement) in [
            ("unexpected", json!(true)),
            ("endpoint_mode", json!("selected_source")),
        ] {
            let mut candidate = valid.clone();
            candidate[field] = replacement;
            assert!(Event::parse(candidate).is_err());
        }
    }

    #[test]
    fn pure_audio_status_counters_bind_direction_and_cannot_regress() {
        for config in [
            parse_config(&config_json()).unwrap(),
            parse_capture_config(&capture_config_json()).unwrap(),
        ] {
            let approved = approval(&config);
            let route = approved.route_id;
            let mut session = Session::default();
            let listening = Event::parse(json!({"version":1,"event":"listening",
                "address":"127.0.0.1:40001"}))
            .unwrap();
            session.accept(&listening, &config).unwrap();
            session
                .accept(&Event::Approval(Box::new(approval(&config))), &config)
                .unwrap();
            session.approval = Some(approved);
            let offer = Event::parse(json!({"version":1,"event":"awaiting_peer_offer",
                "route_id":route}))
            .unwrap();
            let (counter, opposite) = match config.direction {
                Direction::Playback => ("packets_written", "packets_sent"),
                Direction::Capture => ("packets_sent", "packets_written"),
            };
            let mut status = json!({"version":1,"event":"status","route_id":route,
                "state":"active","packets_written":0,"packets_sent":0});
            assert!(session.accept(&offer, &config).is_err());
            assert!(
                session
                    .accept(&Event::parse(status.clone()).unwrap(), &config)
                    .is_err()
            );
            session.approved = true;
            assert!(
                session
                    .accept(&Event::parse(status.clone()).unwrap(), &config)
                    .is_err()
            );
            session.accept(&offer, &config).unwrap();
            status[counter] = json!(3);
            session
                .accept(&Event::parse(status.clone()).unwrap(), &config)
                .unwrap();
            for (field, replacement) in [
                (counter, json!(2)),
                (opposite, json!(1)),
                ("route_id", json!(RouteId::new())),
                ("state", json!("draft")),
            ] {
                let mut candidate = status.clone();
                candidate[field] = replacement;
                assert!(
                    session
                        .accept(&Event::parse(candidate).unwrap(), &config)
                        .is_err()
                );
                assert_eq!(session.packets, 3);
            }
            status[counter] = json!(4);
            session
                .accept(&Event::parse(status).unwrap(), &config)
                .unwrap();
            assert_eq!(session.packets, 4);
            let stopped = Event::parse(json!({"version":1,"event":"stopped"})).unwrap();
            session.accept(&stopped, &config).unwrap();
            assert!(session.accept(&offer, &config).is_err());
        }
    }

    #[test]
    fn pure_capture_terminal_controls_require_fresh_request_route_and_remote_sink() {
        for virtual_speaker in [false, true] {
            let mut value = capture_config_json();
            if virtual_speaker {
                value["endpoint_mode"] = json!("virtual_speaker");
                value["target"] = Value::Null;
            }
            let config = parse_capture_config(&value).unwrap();
            let approved = approval(&config);
            let expected = approved.expected();
            assert_eq!(
                expected,
                format!(
                    "start {} {} {}",
                    approved.request_id, approved.route_id, config.remote.node_id
                )
            );
            let stale = approval(&config).expected();
            let wrong_local_peer = format!(
                "start {} {} {}",
                approved.request_id, approved.route_id, config.node_id
            );
            let wrong_request = format!(
                "start {} {} {}",
                MessageId::new(),
                approved.route_id,
                config.remote.node_id
            );
            let wrong_route = format!(
                "start {} {} {}",
                approved.request_id,
                RouteId::new(),
                config.remote.node_id
            );
            let mut session = Session {
                approval: Some(approved),
                ..Session::default()
            };
            for line in [
                stale,
                wrong_local_peer,
                wrong_request,
                wrong_route,
                format!("{expected} "),
                format!("{expected}\n{expected}"),
                "yes".into(),
                "start".into(),
                "renew".into(),
            ] {
                assert!(
                    input_action(line.as_bytes(), &session).is_err(),
                    "line {line}"
                );
            }
            assert_eq!(
                input_action(expected.as_bytes(), &session).unwrap(),
                Input::Approve
            );
            let command = session.approval.as_ref().unwrap().command();
            exact_keys(&command, &["version", "command", "request_id", "route_id"]).unwrap();
            assert_eq!(command["command"], "approve");
            session.approved = true;
            assert!(input_action(expected.as_bytes(), &session).is_err());
            assert_eq!(input_action(b"status", &session).unwrap(), Input::Status);
            assert_eq!(input_action(b"cancel", &session).unwrap(), Input::Stop);
            session.stopping = true;
            assert!(input_action(b"status", &session).is_err());
            assert_eq!(input_action(b"stop", &session).unwrap(), Input::Stop);
        }
    }

    #[test]
    fn pure_audio_summary_preserves_direction_device_and_plaintext_warning() {
        for direction in [Direction::Playback, Direction::Capture] {
            for owned in [false, true] {
                let mut value = match direction {
                    Direction::Playback => config_json(),
                    Direction::Capture => capture_config_json(),
                };
                if owned {
                    value["target"] = Value::Null;
                    value["endpoint_mode"] = json!(match direction {
                        Direction::Playback => "virtual_microphone",
                        Direction::Capture => "virtual_speaker",
                    });
                }
                let config = match direction {
                    Direction::Playback => parse_config(&value),
                    Direction::Capture => parse_capture_config(&value),
                }
                .unwrap();
                let (source, sink) = match direction {
                    Direction::Playback => (config.remote.node_id, config.node_id),
                    Direction::Capture => (config.node_id, config.remote.node_id),
                };
                let mut session = Session::default();
                let before = session.summary(&config);
                assert!(before.contains(&format!("Source {source} -> Sink {sink}")));
                assert!(before.contains(&format!("Mode: {}", config.endpoint_mode.name())));
                assert!(before.contains("duration: 20 seconds"));
                assert!(before.contains(WARNING));
                if owned {
                    assert!(before.contains("not created before approval"));
                } else {
                    assert!(before.contains(config.target.as_deref().unwrap()));
                }
                session.approval = Some(approval(&config));
                let after = session.summary(&config);
                assert!(after.contains(WARNING));
                if let Some(name) = &session.approval.as_ref().unwrap().node_name {
                    assert!(after.contains(name));
                    assert!(!after.contains("not created before approval"));
                }
            }
        }
    }
}
