//! Finite private-terminal consent for the existing paired-store camera launcher.
//! The console is a Node client, not a camera, credential or transport backend.
use super::*;
use capyio_core::{CapabilityId, PortId, RouteId, RouteState, SessionId, StreamId};
use std::{net::SocketAddrV4, os::unix::fs::OpenOptionsExt, path::Path};

const BOOTSTRAP: Duration = Duration::from_secs(5);
const BEFORE_APPROVAL: Duration = Duration::from_secs(55);
const CONSENT: Duration = Duration::from_secs(30);
const STOP_GRACE: Duration = Duration::from_secs(3);
const FRAME_TIMEOUT: Duration = Duration::from_secs(5);
const FORMAT: &str = "cavc-avc-access-unit-v1";
const FINITE_FORMAT: &str = "cavc-avc-finite-720p30-v1";
const SECURITY: &str = "tls13_mtls_v1";
const MAX_FRAMES: u64 = 120;
const MAX_INPUT_BYTES: u64 = 33_554_432;
const FINITE_FPS: u64 = 30;
const FINITE_BYTES_PER_SECOND: u64 = 2_097_152;
const FINITE_BURST_BYTES: u64 = 4_194_304;

#[derive(Clone, Copy, Debug, Serialize, Eq, PartialEq)]
enum StreamProfile {
    #[serde(rename = "finite_720p30_v1")]
    Finite720p30V1,
}

// Omission selects the legacy profile; an explicit null is never omission.
fn deserialize_profile<'de, D>(deserializer: D) -> Result<Option<StreamProfile>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match String::deserialize(deserializer)?.as_str() {
        "finite_720p30_v1" => Ok(Some(StreamProfile::Finite720p30V1)),
        _ => Err(serde::de::Error::custom(
            "unsupported camera stream profile",
        )),
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

// Intentionally a local copy of the nonsecret Host schema. No platform backend
// dependency and no audio field, executable path or credential fallback.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Config {
    version: u32,
    node_id: NodeId,
    source: Port,
    listen: SocketAddrV4,
    duration_secs: u64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_profile"
    )]
    stream_profile: Option<StreamProfile>,
}
impl Config {
    fn parse(bytes: &[u8], local: NodeId, peer: NodeId) -> io::Result<Self> {
        if bytes.is_empty() || bytes.len() > MAX_FRAME {
            return Err(invalid());
        }
        let shape: Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        let fields = if shape.get("stream_profile").is_some() {
            &[
                "version",
                "node_id",
                "source",
                "listen",
                "duration_secs",
                "stream_profile",
            ][..]
        } else {
            &["version", "node_id", "source", "listen", "duration_secs"][..]
        };
        exact_keys(&shape, fields)?;
        port_shape(&shape["source"])?;
        // Decode original bytes, not the shape, to retain duplicate-field checks.
        let config: Self = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        config.validate(local, peer)?;
        Ok(config)
    }

    fn validate(&self, local: NodeId, peer: NodeId) -> io::Result<()> {
        if self.version != 1
            || local.as_uuid().is_nil()
            || peer.as_uuid().is_nil()
            || local == peer
            || self.node_id != local
            || self.source.node_id != peer
            || !self.source.valid()
            || !lab_address(self.listen)
            || !(3..=60).contains(&self.duration_secs)
        {
            return Err(invalid());
        }
        Ok(())
    }

    fn wire(&self) -> io::Result<Value> {
        serde_json::to_value(self).map_err(|_| invalid())
    }

    fn format(&self) -> &'static str {
        match self.stream_profile {
            None => FORMAT,
            Some(StreamProfile::Finite720p30V1) => FINITE_FORMAT,
        }
    }

    fn work_limits(&self) -> (u64, u64) {
        match self.stream_profile {
            None => (MAX_FRAMES, MAX_INPUT_BYTES),
            Some(StreamProfile::Finite720p30V1) => (
                self.duration_secs * FINITE_FPS,
                MAX_INPUT_BYTES
                    .min(self.duration_secs * FINITE_BYTES_PER_SECOND + FINITE_BURST_BYTES),
            ),
        }
    }
}

/// Pure validation shared by explicit manifest export; never launches a Node.
pub(super) fn validate_export(bytes: &[u8], local: NodeId, peer: NodeId) -> io::Result<()> {
    Config::parse(bytes, local, peer).map(|_| ())
}

fn port_shape(value: &Value) -> io::Result<()> {
    exact_keys(value, &["node_id", "capability_id", "port_id"])
}

fn lab_address(address: SocketAddrV4) -> bool {
    // This is the camera Host policy, not audio's different overlay policy.
    // Port zero is allowed only for the requested ephemeral control listener.
    let ip = address.ip();
    (ip.is_loopback() || ip.is_private() || ip.is_link_local())
        && !ip.is_broadcast()
        && !ip.is_unspecified()
}

fn read_config(path: &Path, local: NodeId, peer: NodeId) -> io::Result<Config> {
    if !path.is_absolute() {
        return Err(invalid());
    }
    // Inspect the opened descriptor, never a prior path stat. NONBLOCK prevents
    // a substituted FIFO from hanging before regular-file validation.
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
    Config::parse(&bytes[..used], local, peer)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Listening {
    version: u32,
    event: String,
    address: SocketAddrV4,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    version: u32,
    event: String,
    request_id: MessageId,
    route_id: RouteId,
    session_id: SessionId,
    source: Port,
    sink: Port,
    format: String,
    duration_secs: u64,
    approval_timeout_secs: u64,
    media_security: String,
    width: u32,
    height: u32,
    fps: u32,
    rotation: u32,
    max_frames: u64,
    max_input_bytes: u64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_profile"
    )]
    stream_profile: Option<StreamProfile>,
}
impl Approval {
    fn validate(&self, config: &Config) -> io::Result<()> {
        let (max_frames, max_input_bytes) = config.work_limits();
        if self.request_id.as_uuid().is_nil()
            || self.route_id.as_uuid().is_nil()
            || self.session_id.as_uuid().is_nil()
            || self.source != config.source
            || !self.sink.valid()
            || self.sink.node_id != config.node_id
            || self.format != config.format()
            || self.stream_profile != config.stream_profile
            || self.media_security != SECURITY
            || self.duration_secs != config.duration_secs
            || self.approval_timeout_secs != CONSENT.as_secs()
            || self.width != 1280
            || self.height != 720
            || self.fps != 30
            || self.rotation != 0
            || self.max_frames != max_frames
            || self.max_input_bytes != max_input_bytes
        {
            return Err(invalid());
        }
        Ok(())
    }

    fn expected(&self) -> String {
        format!(
            "start {} {} {}",
            self.request_id, self.route_id, self.source.node_id
        )
    }

    fn command(&self) -> Value {
        json!({"version":1,"command":"approve","request_id":self.request_id,"route_id":self.route_id})
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
enum Stage {
    Listening,
    Catalog,
    AwaitingLocalApproval,
    AwaitingPeerConsent,
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
struct CameraStatus {
    version: u32,
    event: String,
    route_id: RouteId,
    state: RouteState,
    queued: u64,
    helper_pid: Option<u32>,
    decoder_pid: Option<u32>,
    node_name: Option<String>,
    consumer_delivery_verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dropped: Option<u64>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SimpleEvent {
    version: u32,
    event: String,
}

#[derive(Debug)]
enum Event {
    Listening(Listening),
    Approval(Box<Approval>),
    Stage(StageStatus),
    Status(CameraStatus),
    Bound(CameraStatus),
    Record(CameraStatus),
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
            "listening" => Ok(Self::Listening(decode(value)?)),
            "approval_required" => {
                port_shape(&value["source"])?;
                port_shape(&value["sink"])?;
                Ok(Self::Approval(decode(value)?))
            }
            "status" if value.get("stage").is_some() => Ok(Self::Stage(decode(value)?)),
            name @ ("status" | "camera_bound" | "camera_record") => {
                // Nulls are part of the protocol, not optional omission. Prevent
                // missing process identity from silently decoding as None.
                let fields = if value.get("dropped").is_some() {
                    if value["dropped"].as_u64().is_none() {
                        return Err(invalid());
                    }
                    &[
                        "version",
                        "event",
                        "route_id",
                        "state",
                        "queued",
                        "helper_pid",
                        "decoder_pid",
                        "node_name",
                        "consumer_delivery_verified",
                        "dropped",
                    ][..]
                } else {
                    &[
                        "version",
                        "event",
                        "route_id",
                        "state",
                        "queued",
                        "helper_pid",
                        "decoder_pid",
                        "node_name",
                        "consumer_delivery_verified",
                    ][..]
                };
                exact_keys(&value, fields)?;
                let variant = match name {
                    "status" => Self::Status,
                    "camera_bound" => Self::Bound,
                    _ => Self::Record,
                };
                Ok(variant(decode(value)?))
            }
            "stopped" => Ok(Self::Stopped(decode(value)?)),
            "failed" => Ok(Self::Failed(decode(value)?)),
            _ => Err(invalid()),
        }
    }

    fn canonical(&self) -> io::Result<String> {
        let result = match self {
            Self::Listening(value) => serde_json::to_string(value),
            Self::Approval(value) => serde_json::to_string(value),
            Self::Stage(value) => serde_json::to_string(value),
            Self::Status(value) | Self::Bound(value) | Self::Record(value) => {
                serde_json::to_string(value)
            }
            Self::Stopped(value) | Self::Failed(value) => serde_json::to_string(value),
        };
        result.map_err(|_| invalid())
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ProcessBinding {
    helper_pid: u32,
    decoder_pid: u32,
    node_name: String,
}
impl ProcessBinding {
    fn parse(value: &CameraStatus) -> io::Result<Option<Self>> {
        match (
            value.helper_pid,
            value.decoder_pid,
            value.node_name.as_deref(),
        ) {
            (None, None, None) => Ok(None),
            (Some(helper), Some(decoder), Some(name)) => {
                let token = name.strip_prefix("capyio.camera.").ok_or_else(invalid)?;
                let id: StreamId = token.parse().map_err(|_| invalid())?;
                if helper == 0
                    || decoder == 0
                    || helper > i32::MAX as u32
                    || decoder > i32::MAX as u32
                    || helper == decoder
                    || id.as_uuid().is_nil()
                    || token != id.to_string()
                {
                    return Err(invalid());
                }
                Ok(Some(Self {
                    helper_pid: helper,
                    decoder_pid: decoder,
                    node_name: name.to_owned(),
                }))
            }
            _ => Err(invalid()),
        }
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
    last_stage: Option<Stage>,
    route_seen: bool,
    bound: Option<ProcessBinding>,
    queued: u64,
    dropped: u64,
    records: u64,
    stopping: bool,
    finished: Option<Finished>,
}
impl Session {
    fn accept(&mut self, event: &Event, config: &Config) -> io::Result<()> {
        if self.finished.is_some() {
            return Err(invalid());
        }
        match event {
            Event::Listening(value) => {
                if self.listening
                    || self.approval.is_some()
                    || value.address.ip() != config.listen.ip()
                    || !lab_address(value.address)
                    || value.address.port() == 0
                    || (config.listen.port() != 0 && value.address.port() != config.listen.port())
                {
                    return Err(invalid());
                }
                self.listening = true;
            }
            Event::Approval(value) => {
                if !self.listening || self.approval.is_some() || self.approved || self.stopping {
                    return Err(invalid());
                }
                value.validate(config)?;
                self.approval = Some((**value).clone());
                self.last_stage = Some(Stage::AwaitingLocalApproval);
            }
            Event::Stage(value) => {
                let valid = match value.stage {
                    Stage::Listening | Stage::Catalog => self.approval.is_none(),
                    Stage::AwaitingLocalApproval => self.approval.is_some() && !self.approved,
                    Stage::AwaitingPeerConsent => self.approved,
                };
                if !self.listening
                    || !valid
                    || self.route_seen
                    || self.last_stage.is_some_and(|stage| value.stage < stage)
                {
                    return Err(invalid());
                }
                self.last_stage = Some(value.stage);
            }
            Event::Status(value) | Event::Bound(value) | Event::Record(value) => {
                let max_frames = self.approval.as_ref().ok_or_else(invalid)?.max_frames;
                let dropped = match (config.stream_profile, value.dropped) {
                    (None, None) => 0,
                    (Some(StreamProfile::Finite720p30V1), Some(dropped)) => dropped,
                    _ => return Err(invalid()),
                };
                let frames = value.queued.checked_add(dropped).ok_or_else(invalid)?;
                if !self.approved
                    || self.approval.as_ref().map(|a| a.route_id) != Some(value.route_id)
                    || value.state != RouteState::Starting
                    || value.consumer_delivery_verified
                    || value.queued < self.queued
                    || dropped < self.dropped
                    || frames > max_frames
                {
                    return Err(invalid());
                }
                let binding = ProcessBinding::parse(value)?;
                match event {
                    Event::Bound(_) => {
                        if self.bound.is_some() || binding.is_none() || frames != 0 {
                            return Err(invalid());
                        }
                        self.bound = binding;
                    }
                    Event::Record(_) => {
                        // The Node emits one Config record (no frame), followed
                        // by one event per accepted AU. EOS emits Stopped only.
                        // Thus duplicate Config/AU events cannot replay progress.
                        if self.bound.is_none()
                            || binding.as_ref() != self.bound.as_ref()
                            || self.records > max_frames
                            || frames != self.records
                        {
                            return Err(invalid());
                        }
                        self.records += 1;
                    }
                    Event::Status(_) => {
                        if binding.as_ref() != self.bound.as_ref()
                            || value.queued != self.queued
                            || dropped != self.dropped
                        {
                            return Err(invalid());
                        }
                    }
                    _ => unreachable!("camera telemetry variant"),
                }
                self.queued = value.queued;
                self.dropped = dropped;
                self.route_seen = true;
            }
            Event::Stopped(_) => {
                if !self.stopping && !self.approved {
                    return Err(invalid());
                }
                self.finished = Some(Finished::Stopped);
            }
            Event::Failed(_) => self.finished = Some(Finished::Failed),
        }
        Ok(())
    }

    fn prompt(&self) -> String {
        if self.stopping || self.finished.is_some() {
            "Stopping this finite camera session; no automatic restart.\r\n".into()
        } else if let Some(approval) = self.approval.as_ref().filter(|_| !self.approved) {
            format!("Type {} or stop: ", approval.expected())
        } else {
            "Type status, stop or cancel: ".into()
        }
    }

    fn summary(&self, config: &Config) -> String {
        let source = config.source;
        let mut result = format!(
            "Remote Source Node {}, Capability {}, Port {}\r\nLocal camera Sink Node {}\r\n",
            source.node_id, source.capability_id, source.port_id, config.node_id
        );
        if let Some(approval) = &self.approval {
            result.push_str(&format!(
                "Sink Capability {}, Port {}; Session {}; Route {}\r\n",
                approval.sink.capability_id,
                approval.sink.port_id,
                approval.session_id,
                approval.route_id
            ));
        }
        let format = config.format();
        let (max_frames, max_input_bytes) = config.work_limits();
        result.push_str(&format!(
            "Format: {format}; media security: {SECURITY}; duration: {} seconds\r\n1280x720, 30 fps, rotation 0; at most {max_frames} frames and {max_input_bytes} encoded bytes.\r\nExperimental finite camera; independent Source consent and both leases remain required.\r\nA bound helper or queued frame is not consumer delivery; Route remains Starting.\r\n",
            config.duration_secs
        ));
        if config.stream_profile.is_some() {
            result.push_str(
                "Stream profile: finite_720p30_v1. Fixed local-monotonic caps: 30 fps with a 3-frame burst; 2 MiB/s with a 4 MiB burst; 32 MiB absolute input; 10 CPU seconds for the decoder.\r\nThese are work ceilings, not guaranteed continuous duration; remaining owner lifetime may lower them without extending either lease.\r\nExperimental Linux and synthetic-peer profile only; production Android does not advertise it.\r\n",
            );
        }
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
    let config = read_config(path, args.pairing_node, peer)?;
    let _signals = Signals::install()?;
    require_node_capability_for("linux-camera")?;
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
    let mut node = Node::spawn_for(args, "linux-camera", Some(peer))?;
    node.set_stop_on_drop();
    node.set_frame_limit(2048)?;
    let bootstrap = (Instant::now() + BOOTSTRAP).min(deadline);
    node.send(&config.wire()?, bootstrap)?;
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
            let status = matches!(event, Event::Stage(_) | Event::Status(_));
            if status && !status_pending {
                return Err(invalid());
            }
            session.accept(&event, &config)?;
            let canonical = event.canonical()?;
            let challenge = matches!(event, Event::Approval(_));
            // Render only exact typed public fields, never raw stderr, unknown
            // JSON, credentials or payload. Record/queue is not delivery proof.
            tty.write(
                &format!(
                    "\r\nCamera event: {canonical}\r\n{}",
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
        // A queued duplicate challenge, terminal event or malformed partial
        // frame must never be left behind an approval sent to the Node.
        if received {
            continue;
        }

        if let Some(line) = tty.poll_line()? {
            let action = input_action(line.as_bytes(), &session)?;
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
                    if status_requests > 1024 || status_pending {
                        return Err(invalid());
                    }
                    node.send(
                        &json!({"version":1,"command":"status"}),
                        operation_deadline(deadline),
                    )?;
                    status_pending = true;
                }
                Input::Approve => {
                    if !node.frames.bytes.is_empty() || status_pending {
                        return Err(invalid());
                    }
                    check(consent_deadline.ok_or_else(invalid)?)?;
                    let approval = session.approval.as_ref().ok_or_else(invalid)?;
                    node.send(&approval.command(), operation_deadline(deadline))?;
                    session.approved = true;
                    session.last_stage = Some(Stage::AwaitingPeerConsent);
                    consent_deadline = None;
                    // Authority is never renewed by progress/status or delayed
                    // negotiation; this only narrows the original envelope.
                    deadline = overall.min(
                        Instant::now() + Duration::from_secs(config.duration_secs) + STOP_GRACE,
                    );
                }
            }
            if !status_pending {
                tty.begin_prompt(&session.prompt(), operation_deadline(deadline))?;
            }
        }
        // Require successful exit after Stopped and no trailing complete or
        // partial frames. Drop sends Stop/EOF and always reaps this owned child.
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

    const NIL: &str = "00000000-0000-0000-0000-000000000000";

    fn config_json() -> Value {
        json!({"version":1,"node_id":NodeId::new(),
            "source":{"node_id":NodeId::new(),"capability_id":CapabilityId::new(),"port_id":PortId::new()},
            "listen":"127.0.0.1:0","duration_secs":20})
    }

    fn config() -> Config {
        parse_config(&config_json()).unwrap()
    }

    fn finite_config(duration_secs: u64) -> Config {
        let mut value = config_json();
        value["duration_secs"] = json!(duration_secs);
        value["stream_profile"] = json!("finite_720p30_v1");
        parse_config(&value).unwrap()
    }

    fn parse_config(value: &Value) -> io::Result<Config> {
        Config::parse(
            &serde_json::to_vec(value).unwrap(),
            serde_json::from_value(value["node_id"].clone()).unwrap(),
            serde_json::from_value(value["source"]["node_id"].clone()).unwrap(),
        )
    }

    fn approval_json(config: &Config) -> Value {
        let (max_frames, max_input_bytes) = config.work_limits();
        let mut value = json!({"version":1,"event":"approval_required","request_id":MessageId::new(),
            "route_id":RouteId::new(),"session_id":SessionId::new(),"source":config.source,
            "sink":{"node_id":config.node_id,"capability_id":CapabilityId::new(),"port_id":PortId::new()},
            "format":config.format(),"duration_secs":config.duration_secs,"approval_timeout_secs":30,
            "media_security":SECURITY,"width":1280,"height":720,"fps":30,"rotation":0,
            "max_frames":max_frames,"max_input_bytes":max_input_bytes});
        if let Some(profile) = config.stream_profile {
            value["stream_profile"] = json!(profile);
        }
        value
    }

    fn listening() -> Event {
        Event::parse(json!({"version":1,"event":"listening","address":"127.0.0.1:40001"})).unwrap()
    }

    fn waiting(config: &Config) -> Session {
        let mut session = Session::default();
        session.accept(&listening(), config).unwrap();
        session
            .accept(&Event::parse(approval_json(config)).unwrap(), config)
            .unwrap();
        session
    }

    fn approved(config: &Config) -> Session {
        let mut session = waiting(config);
        session.approved = true;
        session.last_stage = Some(Stage::AwaitingPeerConsent);
        session
    }

    fn status_json(session: &Session, event: &str, queued: u64) -> Value {
        let (helper, decoder, name) = if let Some(binding) = &session.bound {
            (
                Some(binding.helper_pid),
                Some(binding.decoder_pid),
                Some(binding.node_name.clone()),
            )
        } else if event == "camera_bound" {
            (
                Some(4001),
                Some(4002),
                Some(format!("capyio.camera.{}", StreamId::new())),
            )
        } else {
            (None, None, None)
        };
        let approval = session.approval.as_ref().unwrap();
        let mut value = json!({"version":1,"event":event,"route_id":approval.route_id,
            "state":"starting","queued":queued,"helper_pid":helper,"decoder_pid":decoder,
            "node_name":name,"consumer_delivery_verified":false});
        if approval.stream_profile.is_some() {
            value["dropped"] = json!(session.dropped);
        }
        value
    }

    fn accept(session: &mut Session, config: &Config, value: Value) -> io::Result<()> {
        session.accept(&Event::parse(value)?, config)
    }

    fn bound(config: &Config) -> Session {
        let mut session = approved(config);
        let value = status_json(&session, "camera_bound", 0);
        accept(&mut session, config, value).unwrap();
        session
    }

    #[test]
    fn pure_camera_legacy_wire_is_unchanged_and_has_no_profile_or_dropped_fields() {
        let config = config();
        let wire = config.wire().unwrap();
        assert_eq!(wire.as_object().unwrap().len(), 5);
        assert_eq!(config.stream_profile, None);
        assert_eq!(config.format(), FORMAT);
        assert_eq!(config.work_limits(), (120, 33_554_432));
        assert_eq!(
            serde_json::to_string(&config).unwrap(),
            format!(
                "{{\"version\":1,\"node_id\":\"{}\",\"source\":{},\"listen\":\"127.0.0.1:0\",\"duration_secs\":20}}",
                config.node_id,
                serde_json::to_string(&config.source).unwrap()
            )
        );
        let event = Event::parse(approval_json(&config)).unwrap();
        let Event::Approval(approval) = &event else {
            panic!()
        };
        assert_eq!(
            event.canonical().unwrap(),
            format!(
                concat!(
                    "{{\"version\":1,\"event\":\"approval_required\",\"request_id\":\"{}\",",
                    "\"route_id\":\"{}\",\"session_id\":\"{}\",\"source\":{},\"sink\":{},",
                    "\"format\":\"cavc-avc-access-unit-v1\",\"duration_secs\":20,",
                    "\"approval_timeout_secs\":30,\"media_security\":\"tls13_mtls_v1\",",
                    "\"width\":1280,\"height\":720,\"fps\":30,\"rotation\":0,",
                    "\"max_frames\":120,\"max_input_bytes\":33554432}}"
                ),
                approval.request_id,
                approval.route_id,
                approval.session_id,
                serde_json::to_string(&approval.source).unwrap(),
                serde_json::to_string(&approval.sink).unwrap(),
            )
        );
        let session = approved(&config);
        let event = Event::parse(status_json(&session, "status", 0)).unwrap();
        assert_eq!(
            event.canonical().unwrap(),
            format!(
                concat!(
                    "{{\"version\":1,\"event\":\"status\",\"route_id\":\"{}\",",
                    "\"state\":\"starting\",\"queued\":0,\"helper_pid\":null,",
                    "\"decoder_pid\":null,\"node_name\":null,\"consumer_delivery_verified\":false}}"
                ),
                session.approval.as_ref().unwrap().route_id
            )
        );
    }

    #[test]
    fn pure_camera_finite_profile_duration_derives_exact_work_maxima() {
        for duration in 3..=60 {
            let config = finite_config(duration);
            let wire = config.wire().unwrap();
            assert_eq!(wire.as_object().unwrap().len(), 6);
            assert_eq!(wire["stream_profile"], "finite_720p30_v1");
            assert_eq!(config.format(), FINITE_FORMAT);
            assert_eq!(
                config.work_limits(),
                (
                    duration * 30,
                    (duration * 2_097_152 + 4_194_304).min(33_554_432)
                )
            );
            let Event::Approval(approval) = Event::parse(approval_json(&config)).unwrap() else {
                panic!()
            };
            approval.validate(&config).unwrap();
            assert!(approval.max_frames <= 1800);
            assert!(approval.max_input_bytes <= 33_554_432);
        }
        for (duration, frames, bytes) in [
            (3, 90, 10_485_760),
            (4, 120, 12_582_912),
            (13, 390, 31_457_280),
            (14, 420, 33_554_432),
            (60, 1800, 33_554_432),
        ] {
            assert_eq!(finite_config(duration).work_limits(), (frames, bytes));
        }
        let mut value = finite_config(3).wire().unwrap();
        for duration in [0, 1, 2, 61, u64::MAX] {
            value["duration_secs"] = json!(duration);
            assert!(parse_config(&value).is_err());
        }
    }

    #[test]
    fn pure_camera_profile_rejects_null_unknown_duplicate_and_rate_overrides() {
        let config = finite_config(3);
        for profile in [
            Value::Null,
            json!(""),
            json!("legacy"),
            json!("finite_720p30_v2"),
            json!("FINITE_720P30_V1"),
            json!(1),
            json!(true),
            json!({}),
            json!({"finite_720p30_v1":null}),
            json!([]),
        ] {
            let mut value = config.wire().unwrap();
            value["stream_profile"] = profile.clone();
            assert!(parse_config(&value).is_err());
            let mut value = approval_json(&config);
            value["stream_profile"] = profile;
            assert!(Event::parse(value).is_err());
        }
        let bytes = serde_json::to_string(&config).unwrap();
        for duplicate in [
            "\"stream_profile\":\"finite_720p30_v1\",\"stream_profile\":\"finite_720p30_v1\"",
            "\"stream_profile\":null,\"stream_profile\":\"finite_720p30_v1\"",
            "\"stream_profile\":\"finite_720p30_v1\",\"stream_\\u0070rofile\":\"finite_720p30_v1\"",
        ] {
            let candidate = bytes.replace("\"stream_profile\":\"finite_720p30_v1\"", duplicate);
            assert!(
                Config::parse(candidate.as_bytes(), config.node_id, config.source.node_id).is_err()
            );
        }
        // The rates, bursts and CPU resource cap are fixed locally, not knobs
        // that either a manifest or a purported approval event can override.
        for (field, expected) in [
            ("max_fps", 30),
            ("frame_burst", 3),
            ("max_bytes_per_second", 2_097_152),
            ("byte_burst", 4_194_304),
            ("decoder_cpu_secs", 10),
        ] {
            for replacement in [json!(expected), json!(expected + 1), Value::Null] {
                let mut value = config.wire().unwrap();
                value[field] = replacement.clone();
                assert!(parse_config(&value).is_err(), "config field {field}");
                let mut value = approval_json(&config);
                value[field] = replacement;
                assert!(Event::parse(value).is_err(), "approval field {field}");
            }
        }
    }

    #[test]
    fn pure_camera_profile_approval_rejects_mixed_legacy_and_finite_scope() {
        let finite = finite_config(3);
        let mut value = finite.wire().unwrap();
        value.as_object_mut().unwrap().remove("stream_profile");
        let legacy = parse_config(&value).unwrap();
        for (config, other) in [(&finite, &legacy), (&legacy, &finite)] {
            let Event::Approval(approval) = Event::parse(approval_json(other)).unwrap() else {
                panic!()
            };
            assert!(approval.validate(config).is_err());
        }
        let valid = approval_json(&finite);
        for (field, replacement) in [
            ("format", json!(FORMAT)),
            ("fps", json!(29)),
            ("fps", json!(31)),
            ("max_frames", json!(0)),
            ("max_frames", json!(89)),
            ("max_frames", json!(91)),
            ("max_frames", json!(MAX_FRAMES)),
            ("max_input_bytes", json!(10_485_759)),
            ("max_input_bytes", json!(10_485_761)),
            ("max_input_bytes", json!(MAX_INPUT_BYTES)),
        ] {
            let mut candidate = valid.clone();
            candidate[field] = replacement;
            let Event::Approval(approval) = Event::parse(candidate).unwrap() else {
                panic!()
            };
            assert!(approval.validate(&finite).is_err(), "field {field}");
        }
        let mut missing = valid;
        missing.as_object_mut().unwrap().remove("stream_profile");
        let Event::Approval(approval) = Event::parse(missing).unwrap() else {
            panic!()
        };
        assert!(approval.validate(&finite).is_err());
        assert!(approval.validate(&legacy).is_err());
    }

    #[test]
    fn pure_camera_config_is_distinct_strict_and_exact_peer_bound() {
        let config = config();
        let value = config.wire().unwrap();
        assert_eq!(value, serde_json::to_value(&config).unwrap());
        assert!(
            Config::parse(
                &serde_json::to_vec(&value).unwrap(),
                config.node_id,
                config.source.node_id
            )
            .is_ok()
        );
        assert!(
            config
                .validate(NodeId::new(), config.source.node_id)
                .is_err()
        );
        assert!(config.validate(config.node_id, NodeId::new()).is_err());
        assert!(config.validate(config.node_id, config.node_id).is_err());
        for field in [
            "target",
            "media",
            "sink",
            "endpoint_mode",
            "proposal_mode",
            "password",
            "helper",
            "credential_source",
        ] {
            let mut candidate = value.clone();
            candidate[field] = json!("forbidden");
            assert!(parse_config(&candidate).is_err(), "field {field}");
        }
        for field in ["node_id", "capability_id", "port_id"] {
            let mut candidate = value.clone();
            candidate["source"][field] = json!(NIL);
            assert!(parse_config(&candidate).is_err(), "source {field}");
        }
        let mut candidate = value.clone();
        candidate["source"]["extra"] = json!(true);
        assert!(parse_config(&candidate).is_err());
        let mut candidate = value.clone();
        candidate["source"]["node_id"] = value["node_id"].clone();
        assert!(parse_config(&candidate).is_err());
        let mut candidate = value.clone();
        candidate["node_id"] = json!(NIL);
        assert!(parse_config(&candidate).is_err());
        for field in ["version", "node_id", "source", "listen", "duration_secs"] {
            let mut candidate = value.clone();
            candidate.as_object_mut().unwrap().remove(field);
            assert!(
                Config::parse(
                    &serde_json::to_vec(&candidate).unwrap(),
                    config.node_id,
                    config.source.node_id
                )
                .is_err()
            );
        }
    }

    #[test]
    fn pure_camera_config_rejects_duplicate_keys_arrays_and_oversize() {
        let config = config();
        let bytes = serde_json::to_string(&config).unwrap();
        let duplicated_version = bytes.replacen("\"version\":1", "\"version\":1,\"version\":1", 1);
        let duplicated_port = bytes.replacen(
            "\"port_id\":",
            &format!("\"port_id\":\"{}\",\"port_id\":", config.source.port_id),
            1,
        );
        let duplicate_escaped =
            bytes.replacen("\"version\":1", "\"version\":1,\"vers\\u0069on\":1", 1);
        let source_array = json!([
            config.source.node_id,
            config.source.capability_id,
            config.source.port_id
        ]);
        let mut array_source = config.wire().unwrap();
        array_source["source"] = source_array;
        for candidate in [
            Vec::new(),
            vec![b' '; MAX_FRAME + 1],
            b"{}".to_vec(),
            b"[]".to_vec(),
            duplicated_version.into_bytes(),
            duplicated_port.into_bytes(),
            duplicate_escaped.into_bytes(),
            serde_json::to_vec(&array_source).unwrap(),
            serde_json::to_vec(&json!([
                1,
                config.node_id,
                config.source,
                config.listen,
                config.duration_secs
            ]))
            .unwrap(),
        ] {
            assert!(Config::parse(&candidate, config.node_id, config.source.node_id).is_err());
        }
    }

    #[test]
    fn pure_camera_listener_policy_and_duration_match_camera_not_audio() {
        let value = config_json();
        for address in [
            "127.0.0.1:0",
            "10.1.2.3:2",
            "172.16.2.3:0",
            "192.168.0.2:1",
            "169.254.2.3:0",
        ] {
            let mut candidate = value.clone();
            candidate["listen"] = json!(address);
            assert!(parse_config(&candidate).is_ok(), "address {address}");
        }
        for address in [
            "0.0.0.0:0",
            "8.8.8.8:1",
            "224.0.0.1:1",
            "255.255.255.255:1",
            "100.64.0.1:1",
            "[::1]:1",
        ] {
            let mut candidate = value.clone();
            candidate["listen"] = json!(address);
            assert!(parse_config(&candidate).is_err(), "address {address}");
        }
        for duration in [0, 1, 2, 61, u64::MAX] {
            let mut candidate = value.clone();
            candidate["duration_secs"] = json!(duration);
            assert!(parse_config(&candidate).is_err());
        }
        for duration in [3, 60] {
            let mut candidate = value.clone();
            candidate["duration_secs"] = json!(duration);
            assert!(parse_config(&candidate).is_ok());
        }
    }

    #[test]
    fn pure_camera_config_file_requires_regular_owned_bounded_no_symlink() {
        let config = config();
        let directory = std::env::temp_dir().join(format!(
            "capyio-camera-console-config-{}-{}",
            std::process::id(),
            NodeId::new()
        ));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _fixture = Fixture(directory.clone());
        let path = directory.join("camera.json");
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        file.write_all(&serde_json::to_vec(&config).unwrap())
            .unwrap();
        drop(file);
        let read = |path: &Path| read_config(path, config.node_id, config.source.node_id);
        assert!(read(&path).is_ok());
        assert!(read_config(&path, config.node_id, NodeId::new()).is_err());
        let link = directory.join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read(&link).is_err());
        assert!(read(&directory).is_err());
        assert!(read(Path::new("relative.json")).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o620)).unwrap();
        assert!(read(&path).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o602)).unwrap();
        assert!(read(&path).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&path, [b' '; MAX_FRAME + 1]).unwrap();
        assert!(read(&path).is_err());
        fs::write(&path, []).unwrap();
        assert!(read(&path).is_err());
    }

    #[test]
    fn pure_camera_approval_binds_every_scope_and_security_field() {
        let config = config();
        let valid = approval_json(&config);
        for (field, replacement) in [
            ("request_id", json!(NIL)),
            ("route_id", json!(NIL)),
            ("session_id", json!(NIL)),
            ("format", json!("pcm-s16le-48000-mono")),
            ("media_security", json!("plaintext")),
            ("duration_secs", json!(config.duration_secs + 1)),
            ("approval_timeout_secs", json!(31)),
            ("width", json!(640)),
            ("height", json!(480)),
            ("fps", json!(60)),
            ("rotation", json!(90)),
            ("max_frames", json!(121)),
            ("max_input_bytes", json!(MAX_INPUT_BYTES + 1)),
        ] {
            let mut candidate = valid.clone();
            candidate[field] = replacement;
            let Event::Approval(approval) = Event::parse(candidate).unwrap() else {
                panic!()
            };
            assert!(approval.validate(&config).is_err(), "field {field}");
        }
        for (endpoint, field, replacement) in [
            ("source", "node_id", json!(NodeId::new())),
            ("source", "capability_id", json!(CapabilityId::new())),
            ("source", "port_id", json!(PortId::new())),
            ("sink", "node_id", json!(NodeId::new())),
            ("sink", "capability_id", json!(NIL)),
            ("sink", "port_id", json!(NIL)),
        ] {
            let mut candidate = valid.clone();
            candidate[endpoint][field] = replacement;
            let Event::Approval(approval) = Event::parse(candidate).unwrap() else {
                panic!()
            };
            assert!(approval.validate(&config).is_err(), "{endpoint}.{field}");
        }
        let Event::Approval(approval) = Event::parse(valid).unwrap() else {
            panic!()
        };
        approval.validate(&config).unwrap();
    }

    #[test]
    fn pure_camera_events_reject_unknown_missing_malformed_and_audio_fields() {
        let config = config();
        let valid = approval_json(&config);
        for field in valid.as_object().unwrap().keys() {
            let mut candidate = valid.clone();
            candidate.as_object_mut().unwrap().remove(field);
            assert!(Event::parse(candidate).is_err(), "missing {field}");
        }
        for field in ["target", "media", "endpoint_mode", "password", "node_name"] {
            let mut candidate = valid.clone();
            candidate[field] = json!("must not print");
            assert!(Event::parse(candidate).is_err());
        }
        for endpoint in ["source", "sink"] {
            let mut candidate = valid.clone();
            candidate[endpoint]["extra"] = json!(true);
            assert!(Event::parse(candidate).is_err());
            let mut candidate = valid.clone();
            candidate[endpoint] = json!([
                config.source.node_id,
                config.source.capability_id,
                config.source.port_id
            ]);
            assert!(Event::parse(candidate).is_err());
        }
        for candidate in [
            json!({"version":2,"event":"stopped"}),
            json!({"version":1,"event":"stopped","secret":"must not print"}),
            json!({"version":1,"event":"status","stage":"awaiting_peer_consent_or_lease"}),
            json!({"version":1,"event":"awaiting_peer_offer","route_id":RouteId::new()}),
            json!({"version":1,"event":"unknown"}),
            Value::Null,
            json!([]),
        ] {
            assert!(Event::parse(candidate).is_err());
        }
        let session = approved(&config);
        let status = status_json(&session, "status", 0);
        for field in status.as_object().unwrap().keys() {
            let mut candidate = status.clone();
            candidate.as_object_mut().unwrap().remove(field);
            assert!(Event::parse(candidate).is_err(), "missing {field}");
        }
        for (field, replacement) in [
            ("queued", json!(-1)),
            ("helper_pid", json!(u64::MAX)),
            ("decoder_pid", json!("123")),
            ("consumer_delivery_verified", json!(0)),
        ] {
            let mut candidate = status.clone();
            candidate[field] = replacement;
            assert!(Event::parse(candidate).is_err());
        }
        let encoded = Event::parse(status).unwrap().canonical().unwrap();
        assert!(encoded.contains("\"consumer_delivery_verified\":false"));
        assert!(!encoded.contains("must not print"));
    }

    #[test]
    fn pure_camera_preapproval_and_replayed_challenges_never_grant() {
        let config = config();
        let mut session = Session::default();
        let event = Event::parse(approval_json(&config)).unwrap();
        assert!(session.accept(&event, &config).is_err());
        session.accept(&listening(), &config).unwrap();
        assert!(session.accept(&listening(), &config).is_err());
        session.accept(&event, &config).unwrap();
        assert!(!session.approved);
        assert!(session.accept(&event, &config).is_err());
        assert!(
            session
                .accept(&Event::parse(approval_json(&config)).unwrap(), &config)
                .is_err()
        );
        for kind in ["status", "camera_bound", "camera_record"] {
            let value = status_json(&session, kind, 0);
            assert!(accept(&mut session, &config, value).is_err());
        }
        assert!(
            session
                .accept(
                    &Event::parse(json!({"version":1,"event":"stopped"})).unwrap(),
                    &config
                )
                .is_err()
        );
        session.stopping = true;
        session
            .accept(
                &Event::parse(json!({"version":1,"event":"stopped"})).unwrap(),
                &config,
            )
            .unwrap();
        assert!(session.accept(&event, &config).is_err());
    }

    #[test]
    fn pure_camera_input_requires_single_use_current_request_route_peer() {
        let config = config();
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
        session = waiting(&config);
        let approval = session.approval.as_ref().unwrap();
        let expected = approval.expected();
        assert_eq!(
            input_action(expected.as_bytes(), &session).unwrap(),
            Input::Approve
        );
        for wrong in [
            format!("{expected} "),
            format!("{expected}\n{expected}"),
            format!(
                "start {} {} {}",
                MessageId::new(),
                approval.route_id,
                approval.source.node_id
            ),
            format!(
                "start {} {} {}",
                approval.request_id,
                RouteId::new(),
                approval.source.node_id
            ),
            format!(
                "start {} {} {}",
                approval.request_id,
                approval.route_id,
                NodeId::new()
            ),
        ] {
            assert!(input_action(wrong.as_bytes(), &session).is_err());
        }
        let command = approval.command();
        exact_keys(&command, &["version", "command", "request_id", "route_id"]).unwrap();
        assert_eq!(command["command"], "approve");
        session.approved = true;
        assert!(input_action(expected.as_bytes(), &session).is_err());
        session.stopping = true;
        assert!(input_action(b"status", &session).is_err());
        assert_eq!(input_action(b"stop", &session).unwrap(), Input::Stop);
    }

    #[test]
    fn pure_camera_listening_and_stage_progress_are_exact_and_monotonic() {
        let config = config();
        for address in ["127.0.0.1:0", "127.0.0.2:4", "0.0.0.0:4", "8.8.8.8:4"] {
            let mut session = Session::default();
            assert!(
                accept(
                    &mut session,
                    &config,
                    json!({"version":1,"event":"listening","address":address})
                )
                .is_err()
            );
        }
        let mut exact = parse_config(&config_json()).unwrap();
        exact.listen.set_port(40002);
        assert!(Session::default().accept(&listening(), &exact).is_err());
        let stage =
            |stage| Event::parse(json!({"version":1,"event":"status","stage":stage})).unwrap();
        let mut session = Session::default();
        assert!(session.accept(&stage("listening"), &config).is_err());
        session.accept(&listening(), &config).unwrap();
        session.accept(&stage("catalog"), &config).unwrap();
        assert!(session.accept(&stage("listening"), &config).is_err());
        assert!(
            session
                .accept(&stage("awaiting_local_approval"), &config)
                .is_err()
        );
        session
            .accept(&Event::parse(approval_json(&config)).unwrap(), &config)
            .unwrap();
        session
            .accept(&stage("awaiting_local_approval"), &config)
            .unwrap();
        assert!(
            session
                .accept(&stage("awaiting_peer_consent"), &config)
                .is_err()
        );
        session.approved = true;
        session
            .accept(&stage("awaiting_peer_consent"), &config)
            .unwrap();
        assert!(
            session
                .accept(&stage("awaiting_local_approval"), &config)
                .is_err()
        );
        let status = status_json(&session, "status", 0);
        accept(&mut session, &config, status).unwrap();
        assert!(
            session
                .accept(&stage("awaiting_peer_consent"), &config)
                .is_err()
        );
    }

    #[test]
    fn pure_camera_process_binding_requires_exact_native_shape_and_single_bound() {
        let config = config();
        let mut session = approved(&config);
        let valid = status_json(&session, "camera_bound", 0);
        for (field, replacement) in [
            ("helper_pid", Value::Null),
            ("decoder_pid", Value::Null),
            ("node_name", Value::Null),
            ("helper_pid", json!(0)),
            ("decoder_pid", json!(0)),
            ("helper_pid", json!(4002)),
            ("helper_pid", json!(i32::MAX as u64 + 1)),
            ("node_name", json!(format!("capyio.camera.{NIL}"))),
            ("node_name", json!("capyio.camera.not-a-uuid")),
            (
                "node_name",
                json!(format!(
                    "capyio.camera.{}",
                    "12345678-1234-4234-8234-ABCDEF123456"
                )),
            ),
            (
                "node_name",
                json!(format!("capyio.camera.{}\n", StreamId::new())),
            ),
            (
                "node_name",
                json!(format!("other.camera.{}", StreamId::new())),
            ),
            ("queued", json!(1)),
        ] {
            let mut candidate = valid.clone();
            candidate[field] = replacement;
            assert!(
                accept(&mut session, &config, candidate).is_err(),
                "field {field}"
            );
        }
        accept(&mut session, &config, valid.clone()).unwrap();
        assert!(accept(&mut session, &config, valid).is_err());
        let record = status_json(&session, "camera_record", 0);
        for (field, replacement) in [
            ("helper_pid", json!(5001)),
            ("decoder_pid", json!(5002)),
            (
                "node_name",
                json!(format!("capyio.camera.{}", StreamId::new())),
            ),
        ] {
            let mut candidate = record.clone();
            candidate[field] = replacement;
            assert!(accept(&mut session, &config, candidate).is_err());
        }
        accept(&mut session, &config, record).unwrap();
        let mut status = status_json(&session, "status", 0);
        status["helper_pid"] = Value::Null;
        status["decoder_pid"] = Value::Null;
        status["node_name"] = Value::Null;
        assert!(accept(&mut session, &config, status).is_err());
    }

    #[test]
    fn pure_camera_telemetry_is_starting_only_and_never_delivery_proof() {
        let config = config();
        let mut session = bound(&config);
        for event in ["status", "camera_record"] {
            let valid = status_json(&session, event, 0);
            for state in [
                "draft", "prepared", "active", "stopping", "stopped", "failed", "offline",
            ] {
                let mut candidate = valid.clone();
                candidate["state"] = json!(state);
                assert!(
                    accept(&mut session, &config, candidate).is_err(),
                    "{event} {state}"
                );
            }
            let mut candidate = valid.clone();
            candidate["consumer_delivery_verified"] = json!(true);
            assert!(accept(&mut session, &config, candidate).is_err());
            let mut candidate = valid;
            candidate["route_id"] = json!(RouteId::new());
            assert!(accept(&mut session, &config, candidate).is_err());
        }
        let mut before = approved(&config);
        let value = status_json(&before, "status", 0);
        accept(&mut before, &config, value).unwrap();
        let value = status_json(&before, "status", 1);
        assert!(accept(&mut before, &config, value).is_err());
        let value = status_json(&before, "camera_record", 0);
        assert!(accept(&mut before, &config, value).is_err());
    }

    #[test]
    fn pure_camera_record_and_status_counters_reject_replay_regression_and_overrun() {
        let config = config();
        let mut session = bound(&config);
        let config_record = status_json(&session, "camera_record", 0);
        accept(&mut session, &config, config_record.clone()).unwrap();
        assert!(accept(&mut session, &config, config_record).is_err());
        for count in 1..=MAX_FRAMES {
            let next = status_json(&session, "camera_record", count);
            accept(&mut session, &config, next.clone()).unwrap();
            assert!(accept(&mut session, &config, next).is_err());
            let status = status_json(&session, "status", count);
            accept(&mut session, &config, status).unwrap();
            let rollback = status_json(&session, "status", count - 1);
            assert!(accept(&mut session, &config, rollback).is_err());
        }
        assert_eq!(session.queued, MAX_FRAMES);
        assert_eq!(session.records, MAX_FRAMES + 1);
        let overflow = status_json(&session, "camera_record", MAX_FRAMES + 1);
        assert!(accept(&mut session, &config, overflow).is_err());
        let mut session = bound(&config);
        let gap = status_json(&session, "camera_record", 1);
        assert!(accept(&mut session, &config, gap).is_err());
        let ahead = status_json(&session, "status", 1);
        assert!(accept(&mut session, &config, ahead).is_err());
    }

    #[test]
    fn pure_camera_finite_profile_preserves_exact_single_use_consent() {
        let config = finite_config(60);
        let challenge = Event::parse(approval_json(&config)).unwrap();
        let mut session = Session::default();
        assert!(session.accept(&challenge, &config).is_err());
        assert!(input_action(b"start stale stale stale", &session).is_err());
        session.accept(&listening(), &config).unwrap();
        session.accept(&challenge, &config).unwrap();
        assert!(session.accept(&challenge, &config).is_err());
        for event in ["status", "camera_bound", "camera_record"] {
            let candidate = status_json(&session, event, 0);
            assert!(accept(&mut session, &config, candidate).is_err());
        }
        let approval = session.approval.as_ref().unwrap();
        let expected = approval.expected();
        assert_eq!(
            input_action(expected.as_bytes(), &session).unwrap(),
            Input::Approve
        );
        for wrong in [
            format!("{expected} "),
            format!("{expected}\n{expected}"),
            format!(
                "start {} {} {}",
                MessageId::new(),
                approval.route_id,
                approval.source.node_id
            ),
            format!(
                "start {} {} {}",
                approval.request_id,
                RouteId::new(),
                approval.source.node_id
            ),
            format!(
                "start {} {} {}",
                approval.request_id,
                approval.route_id,
                NodeId::new()
            ),
        ] {
            assert!(input_action(wrong.as_bytes(), &session).is_err());
        }
        exact_keys(
            &approval.command(),
            &["version", "command", "request_id", "route_id"],
        )
        .unwrap();
        session.approved = true;
        assert!(input_action(expected.as_bytes(), &session).is_err());
        assert!(session.accept(&challenge, &config).is_err());
        session.stopping = true;
        assert_eq!(input_action(b"stop", &session).unwrap(), Input::Stop);
        assert!(input_action(expected.as_bytes(), &session).is_err());
        session
            .accept(
                &Event::parse(json!({"version":1,"event":"stopped"})).unwrap(),
                &config,
            )
            .unwrap();
        assert!(session.accept(&challenge, &config).is_err());
    }

    #[test]
    fn pure_camera_finite_record_counters_include_drops_without_delivery_claims() {
        for duration in [3, 60] {
            let config = finite_config(duration);
            let max_frames = config.work_limits().0;
            let mut session = bound(&config);
            let config_record = status_json(&session, "camera_record", 0);
            accept(&mut session, &config, config_record.clone()).unwrap();
            assert!(accept(&mut session, &config, config_record).is_err());
            for count in 1..=max_frames {
                let dropped = u64::from(count % 3 == 0);
                let queued = session.queued + 1 - dropped;
                let mut record = status_json(&session, "camera_record", queued);
                record["dropped"] = json!(session.dropped + dropped);
                accept(&mut session, &config, record.clone()).unwrap();
                assert!(accept(&mut session, &config, record).is_err());
                let status = status_json(&session, "status", session.queued);
                accept(&mut session, &config, status.clone()).unwrap();
                let mut ahead = status.clone();
                ahead["dropped"] = json!(session.dropped + 1);
                assert!(accept(&mut session, &config, ahead).is_err());
                if session.dropped > 0 {
                    let mut rollback = status.clone();
                    rollback["dropped"] = json!(session.dropped - 1);
                    assert!(accept(&mut session, &config, rollback).is_err());
                }
                if session.queued > 0 {
                    let mut rollback = status;
                    rollback["queued"] = json!(session.queued - 1);
                    assert!(accept(&mut session, &config, rollback).is_err());
                }
            }
            assert_eq!(session.queued + session.dropped, max_frames);
            assert_eq!(session.records, max_frames + 1);
            assert!(session.dropped > 0);
            let overflow = status_json(&session, "camera_record", session.queued + 1);
            assert!(accept(&mut session, &config, overflow).is_err());
            let mut overflow = status_json(&session, "camera_record", u64::MAX);
            overflow["dropped"] = json!(1);
            assert!(accept(&mut session, &config, overflow).is_err());
            for event in ["status", "camera_record"] {
                let mut active = status_json(&session, event, session.queued);
                active["state"] = json!("active");
                assert!(accept(&mut session, &config, active).is_err());
                let mut delivery = status_json(&session, event, session.queued);
                delivery["consumer_delivery_verified"] = json!(true);
                assert!(accept(&mut session, &config, delivery).is_err());
            }
            let mut session = bound(&config);
            let mut gap = status_json(&session, "camera_record", 0);
            gap["dropped"] = json!(1);
            assert!(accept(&mut session, &config, gap).is_err());
        }
    }

    #[test]
    fn pure_camera_dropped_telemetry_is_exactly_profile_bound() {
        let legacy = config();
        let finite = finite_config(3);
        for event in ["status", "camera_bound", "camera_record"] {
            let mut session = approved(&legacy);
            let mut value = status_json(&session, event, 0);
            value["dropped"] = json!(0);
            assert!(accept(&mut session, &legacy, value).is_err());
            let mut session = approved(&finite);
            let valid = status_json(&session, event, 0);
            let mut value = valid.clone();
            value.as_object_mut().unwrap().remove("dropped");
            assert!(accept(&mut session, &finite, value).is_err());
            for dropped in [Value::Null, json!(-1), json!("0"), json!(0.5), json!(true)] {
                let mut value = valid.clone();
                value["dropped"] = dropped;
                assert!(Event::parse(value).is_err());
            }
        }
        let mut session = approved(&finite);
        for event in ["status", "camera_bound"] {
            let mut value = status_json(&session, event, 0);
            value["dropped"] = json!(1);
            assert!(accept(&mut session, &finite, value).is_err());
        }
        let status = status_json(&session, "status", 0);
        accept(&mut session, &finite, status.clone()).unwrap();
        assert!(
            Event::parse(status)
                .unwrap()
                .canonical()
                .unwrap()
                .contains("\"dropped\":0")
        );
    }

    #[test]
    fn pure_camera_finite_presentation_discloses_caps_and_no_duration_guarantee() {
        let config = finite_config(3);
        let summary = waiting(&config).summary(&config);
        for expected in [
            "finite_720p30_v1",
            FINITE_FORMAT,
            SECURITY,
            "1280x720, 30 fps, rotation 0",
            "at most 90 frames and 10485760 encoded bytes",
            "local-monotonic caps: 30 fps with a 3-frame burst",
            "2 MiB/s with a 4 MiB burst",
            "32 MiB absolute input",
            "10 CPU seconds for the decoder",
            "work ceilings, not guaranteed continuous duration",
            "without extending either lease",
            "independent Source consent and both leases remain required",
            "not consumer delivery; Route remains Starting",
            "production Android does not advertise it",
        ] {
            assert!(summary.contains(expected), "missing {expected}");
        }
    }

    #[test]
    fn pure_camera_terminal_event_is_final_and_presentation_has_exact_scope() {
        let config = config();
        let mut session = approved(&config);
        let approval = session.approval.as_ref().unwrap();
        let summary = session.summary(&config);
        for expected in [
            config.source.node_id.to_string(),
            config.source.capability_id.to_string(),
            config.source.port_id.to_string(),
            config.node_id.to_string(),
            approval.sink.capability_id.to_string(),
            approval.sink.port_id.to_string(),
            approval.route_id.to_string(),
            approval.session_id.to_string(),
            FORMAT.into(),
            SECURITY.into(),
            "1280x720, 30 fps, rotation 0".into(),
            "at most 120 frames and 33554432 encoded bytes".into(),
            "not consumer delivery; Route remains Starting".into(),
        ] {
            assert!(summary.contains(&expected), "missing {expected}");
        }
        session
            .accept(
                &Event::parse(json!({"version":1,"event":"stopped"})).unwrap(),
                &config,
            )
            .unwrap();
        assert_eq!(session.finished, Some(Finished::Stopped));
        assert!(
            session
                .accept(
                    &Event::parse(json!({"version":1,"event":"stopped"})).unwrap(),
                    &config
                )
                .is_err()
        );
        let mut failed = Session::default();
        failed
            .accept(
                &Event::parse(json!({"version":1,"event":"failed"})).unwrap(),
                &config,
            )
            .unwrap();
        assert_eq!(failed.finished, Some(Finished::Failed));
        assert!(failed.accept(&listening(), &config).is_err());
    }
}
