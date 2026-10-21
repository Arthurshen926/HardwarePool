//! Finite private-terminal consent for the existing paired-store touchpad launcher.
//! The console is a Node client, not a touchpad, credential or transport backend.
use super::*;
use capyio_core::{CapabilityId, PortId, RouteId, RouteState, SessionId};
use std::{net::SocketAddrV4, os::unix::fs::OpenOptionsExt, path::Path};

const BOOTSTRAP: Duration = Duration::from_secs(5);
const BEFORE_APPROVAL: Duration = Duration::from_secs(55);
const CONSENT: Duration = Duration::from_secs(30);
const STOP_GRACE: Duration = Duration::from_secs(3);
const FRAME_TIMEOUT: Duration = Duration::from_secs(5);
const FORMAT: &str = "touchpad-frame-v1";
const SECURITY: &str = "tls13_mtls_v1";

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
}
impl Config {
    fn parse(bytes: &[u8], local: NodeId, peer: NodeId) -> io::Result<Self> {
        if bytes.is_empty() || bytes.len() > MAX_FRAME {
            return Err(invalid());
        }
        let shape: Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        exact_keys(
            &shape,
            &["version", "node_id", "source", "listen", "duration_secs"],
        )?;
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
}

pub(super) fn validate_export(bytes: &[u8], local: NodeId, peer: NodeId) -> io::Result<()> {
    Config::parse(bytes, local, peer).map(|_| ())
}

fn port_shape(value: &Value) -> io::Result<()> {
    exact_keys(value, &["node_id", "capability_id", "port_id"])
}

fn lab_address(address: SocketAddrV4) -> bool {
    // This is the touchpad Host policy, not audio's different overlay policy.
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
    max_contacts: u32,
    width_mm: u32,
    height_mm: u32,
    clickable: bool,
    pressure: bool,
}
impl Approval {
    fn validate(&self, config: &Config) -> io::Result<()> {
        if self.request_id.as_uuid().is_nil()
            || self.route_id.as_uuid().is_nil()
            || self.session_id.as_uuid().is_nil()
            || self.source != config.source
            || !self.sink.valid()
            || self.sink.node_id != config.node_id
            || self.format != FORMAT
            || self.media_security != SECURITY
            || self.duration_secs != config.duration_secs
            || self.approval_timeout_secs != CONSENT.as_secs()
            || self.max_contacts != 5
            || self.width_mm != 120
            || self.height_mm != 70
            || self.clickable
            || !self.pressure
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
    AwaitingSourceLease,
    AwaitingTouchpadOffer,
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
struct TouchpadStatus {
    version: u32,
    event: String,
    route_id: RouteId,
    state: RouteState,
    os_gestures_verified: bool,
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
    Status(TouchpadStatus),
    Bound(TouchpadStatus),
    Active(TouchpadStatus),
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
            name @ ("status" | "touchpad_bound" | "touchpad_active") => {
                // Render only the exact existing public Host schema. Helper
                // identity, ACK counters and payload never arrive on this path.
                exact_keys(
                    &value,
                    &[
                        "version",
                        "event",
                        "route_id",
                        "state",
                        "os_gestures_verified",
                    ],
                )?;
                let variant = match name {
                    "status" => Self::Status,
                    "touchpad_bound" => Self::Bound,
                    _ => Self::Active,
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
            Self::Status(value) | Self::Bound(value) | Self::Active(value) => {
                serde_json::to_string(value)
            }
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
    last_stage: Option<Stage>,
    route_seen: bool,
    bound: bool,
    active: bool,
    active_reported: bool,
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
                    Stage::AwaitingPeerConsent
                    | Stage::AwaitingSourceLease
                    | Stage::AwaitingTouchpadOffer => self.approved,
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
            Event::Status(value) | Event::Bound(value) | Event::Active(value) => {
                if !self.approved
                    || self.approval.as_ref().map(|a| a.route_id) != Some(value.route_id)
                    || !matches!(value.state, RouteState::Starting | RouteState::Active)
                    || value.os_gestures_verified
                    || (self.active && value.state != RouteState::Active)
                    || (value.state == RouteState::Active && !self.bound)
                {
                    return Err(invalid());
                }
                match event {
                    Event::Bound(_) => {
                        // The Host emits this before it starts the worker. It
                        // proves a bound transport, not a helper or input commit.
                        if self.bound || value.state != RouteState::Starting {
                            return Err(invalid());
                        }
                        self.bound = true;
                    }
                    Event::Active(_) => {
                        if !self.bound || self.active_reported || value.state != RouteState::Active
                        {
                            return Err(invalid());
                        }
                        self.active_reported = true;
                    }
                    Event::Status(_) => {}
                    _ => unreachable!("touchpad telemetry variant"),
                }
                // Active comes from the existing Runtime's committed-backend
                // ACK path, never from queue/binding inference here. A solicited
                // status may report Active before the one-shot active event:
                // the Host samples status before checking its notification flag.
                self.active = value.state == RouteState::Active;
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
            "Stopping this finite touchpad session; no automatic restart.\r\n".into()
        } else if let Some(approval) = self.approval.as_ref().filter(|_| !self.approved) {
            format!("Type {} or stop: ", approval.expected())
        } else {
            "Type status, stop or cancel: ".into()
        }
    }

    fn summary(&self, config: &Config) -> String {
        let source = config.source;
        let mut result = format!(
            "Remote Source Node {}, Capability {}, Port {}\r\nLocal touchpad Sink Node {}\r\n",
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
        result.push_str(&format!(
            "Format: {FORMAT}; media security: {SECURITY}; duration: {} seconds\r\nAt most 5 contacts; 120x70 mm; non-clickable; pressure enabled.\r\nExperimental finite AdapterManaged touchpad; independent Source consent and both leases remain required.\r\nNode Active follows committed backend ACK; binding alone is not activation.\r\nOS gestures and physical input are not verified.\r\n",
            config.duration_secs
        ));
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
    run_session(args, path, peer, Mode::Native)
}

#[cfg(feature = "touchpad-test-fixture")]
pub(super) fn run_fixture(args: &Args, path: &Path, peer: NodeId, mode: &str) -> io::Result<()> {
    if !matches!(
        mode,
        "normal" | "fail-create" | "partial-write" | "delayed-commit" | "backpressure"
    ) {
        return Err(invalid());
    }
    run_session(args, path, peer, Mode::Fixture(mode.to_owned()))
}

enum Mode {
    Native,
    #[cfg(feature = "touchpad-test-fixture")]
    Fixture(String),
}
impl Mode {
    fn capability(&self) -> &'static str {
        match self {
            Self::Native => "linux-touchpad",
            #[cfg(feature = "touchpad-test-fixture")]
            Self::Fixture(_) => "linux-touchpad-fixture",
        }
    }
    fn banner(&self) -> &'static str {
        match self {
            Self::Native => "",
            #[cfg(feature = "touchpad-test-fixture")]
            Self::Fixture(_) => {
                "SOFTWARE-ONLY TOUCHPAD FIXTURE: no OS device or gesture acceptance.\r\n"
            }
        }
    }
    fn spawn(&self, args: &Args, peer: NodeId) -> io::Result<Node> {
        match self {
            Self::Native => Node::spawn_for(args, "linux-touchpad", Some(peer)),
            #[cfg(feature = "touchpad-test-fixture")]
            Self::Fixture(mode) => Node::spawn_touchpad_fixture(args, peer, mode),
        }
    }
}

fn run_session(args: &Args, path: &Path, peer: NodeId, mode: Mode) -> io::Result<()> {
    let config = read_config(path, args.pairing_node, peer)?;
    let _signals = Signals::install()?;
    require_node_capability_for(mode.capability())?;
    let mut tty = Terminal::open()?;
    let started = Instant::now();
    let overall =
        started + BEFORE_APPROVAL + Duration::from_secs(config.duration_secs) + STOP_GRACE;
    let mut deadline = started + BEFORE_APPROVAL;
    let mut session = Session::default();
    tty.begin_prompt(
        &(mode.banner().to_owned() + &session.summary(&config) + &session.prompt()),
        operation_deadline(deadline),
    )?;
    let mut node = mode.spawn(args, peer)?;
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
            // JSON, credentials or payload. Active is not OS gesture proof.
            tty.write(
                &format!(
                    "\r\nTouchpad event: {canonical}\r\n{}",
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

    fn parse_config(value: &Value) -> io::Result<Config> {
        Config::parse(
            &serde_json::to_vec(value).unwrap(),
            serde_json::from_value(value["node_id"].clone()).unwrap(),
            serde_json::from_value(value["source"]["node_id"].clone()).unwrap(),
        )
    }

    fn approval_json(config: &Config) -> Value {
        json!({"version":1,"event":"approval_required","request_id":MessageId::new(),
            "route_id":RouteId::new(),"session_id":SessionId::new(),"source":config.source,
            "sink":{"node_id":config.node_id,"capability_id":CapabilityId::new(),"port_id":PortId::new()},
            "format":FORMAT,"duration_secs":config.duration_secs,"approval_timeout_secs":30,
            "media_security":SECURITY,"max_contacts":5,"width_mm":120,"height_mm":70,
            "clickable":false,"pressure":true})
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

    fn status_json(session: &Session, event: &str, state: &str) -> Value {
        json!({"version":1,"event":event,"route_id":session.approval.as_ref().unwrap().route_id,
            "state":state,"os_gestures_verified":false})
    }

    fn accept(session: &mut Session, config: &Config, value: Value) -> io::Result<()> {
        session.accept(&Event::parse(value)?, config)
    }

    fn bound(config: &Config) -> Session {
        let mut session = approved(config);
        let value = status_json(&session, "touchpad_bound", "starting");
        accept(&mut session, config, value).unwrap();
        session
    }

    #[test]
    fn pure_touchpad_config_is_distinct_strict_and_exact_peer_bound() {
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
    fn pure_touchpad_config_rejects_duplicate_keys_arrays_and_oversize() {
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
    fn pure_touchpad_listener_policy_and_duration_match_touchpad_not_audio() {
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
    fn pure_touchpad_config_file_requires_regular_owned_bounded_no_symlink() {
        let config = config();
        let directory = std::env::temp_dir().join(format!(
            "capyio-touchpad-console-config-{}-{}",
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
        let path = directory.join("touchpad.json");
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
    fn pure_touchpad_approval_binds_every_scope_and_security_field() {
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
            ("max_contacts", json!(6)),
            ("width_mm", json!(121)),
            ("height_mm", json!(71)),
            ("clickable", json!(true)),
            ("pressure", json!(false)),
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
    fn pure_touchpad_events_reject_unknown_missing_malformed_and_audio_fields() {
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
        let status = status_json(&session, "status", "starting");
        for field in status.as_object().unwrap().keys() {
            let mut candidate = status.clone();
            candidate.as_object_mut().unwrap().remove(field);
            assert!(Event::parse(candidate).is_err(), "missing {field}");
        }
        for (field, replacement) in [
            ("state", json!("unknown")),
            ("route_id", json!(17)),
            ("os_gestures_verified", json!(0)),
        ] {
            let mut candidate = status.clone();
            candidate[field] = replacement;
            assert!(Event::parse(candidate).is_err());
        }
        let encoded = Event::parse(status).unwrap().canonical().unwrap();
        assert!(encoded.contains("\"os_gestures_verified\":false"));
        assert!(!encoded.contains("must not print"));
    }

    #[test]
    fn pure_touchpad_preapproval_and_replayed_challenges_never_grant() {
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
        for kind in ["status", "touchpad_bound", "touchpad_active"] {
            let value = status_json(&session, kind, "starting");
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
    fn pure_touchpad_input_requires_single_use_current_request_route_peer() {
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
    fn pure_touchpad_listening_and_stage_progress_are_exact_and_monotonic() {
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
        let status = status_json(&session, "status", "starting");
        accept(&mut session, &config, status).unwrap();
        assert!(
            session
                .accept(&stage("awaiting_peer_consent"), &config)
                .is_err()
        );
    }

    #[test]
    fn pure_touchpad_binding_is_single_use_and_never_infers_activation() {
        let config = config();
        let mut session = approved(&config);
        for event in ["status", "touchpad_bound", "touchpad_active"] {
            let active = status_json(&session, event, "active");
            assert!(accept(&mut session, &config, active).is_err(), "{event}");
        }
        let starting = status_json(&session, "status", "starting");
        accept(&mut session, &config, starting).unwrap();
        assert!(!session.bound);
        assert!(!session.active);
        let binding = status_json(&session, "touchpad_bound", "starting");
        accept(&mut session, &config, binding.clone()).unwrap();
        assert!(session.bound);
        assert!(!session.active);
        assert!(!session.active_reported);
        assert!(accept(&mut session, &config, binding).is_err());
        let wrong = status_json(&session, "touchpad_active", "starting");
        assert!(accept(&mut session, &config, wrong).is_err());
        let active = status_json(&session, "touchpad_active", "active");
        accept(&mut session, &config, active.clone()).unwrap();
        assert!(session.active);
        assert!(session.active_reported);
        assert!(accept(&mut session, &config, active).is_err());
    }

    #[test]
    fn pure_touchpad_status_can_precede_active_notification_without_regression() {
        let config = config();
        let mut session = bound(&config);
        let starting = status_json(&session, "status", "starting");
        accept(&mut session, &config, starting.clone()).unwrap();
        let active = status_json(&session, "status", "active");
        accept(&mut session, &config, active.clone()).unwrap();
        assert!(session.active);
        assert!(!session.active_reported);
        // Repeated solicited status is legitimate; a one-shot notification is not.
        accept(&mut session, &config, active.clone()).unwrap();
        assert!(accept(&mut session, &config, starting).is_err());
        let notification = status_json(&session, "touchpad_active", "active");
        accept(&mut session, &config, notification.clone()).unwrap();
        assert!(accept(&mut session, &config, notification).is_err());
        accept(&mut session, &config, active).unwrap();
    }

    #[test]
    fn pure_touchpad_telemetry_requires_exact_route_and_no_os_gesture_claim() {
        let config = config();
        for event in ["status", "touchpad_bound", "touchpad_active"] {
            let mut session = approved(&config);
            let valid = status_json(&session, event, "starting");
            for state in [
                "draft", "prepared", "stopping", "stopped", "failed", "offline",
            ] {
                let mut candidate = valid.clone();
                candidate["state"] = json!(state);
                assert!(
                    accept(&mut session, &config, candidate).is_err(),
                    "{event} {state}"
                );
            }
            let mut candidate = valid.clone();
            candidate["os_gestures_verified"] = json!(true);
            assert!(accept(&mut session, &config, candidate).is_err());
            let mut candidate = valid;
            candidate["route_id"] = json!(RouteId::new());
            assert!(accept(&mut session, &config, candidate).is_err());
        }
        let mut session = bound(&config);
        let active = status_json(&session, "touchpad_active", "active");
        for field in [
            "queued",
            "committed",
            "helper_pid",
            "node_name",
            "consumer_delivery_verified",
        ] {
            let mut candidate = active.clone();
            candidate[field] = json!(1);
            // The Host exposes no counters/helper identity in this schema.
            // Reject invented evidence rather than interpreting it as delivery.
            assert!(Event::parse(candidate).is_err(), "{field}");
        }
        accept(&mut session, &config, active).unwrap();
    }

    #[test]
    fn pure_touchpad_lease_and_offer_stages_are_ordered_not_authority() {
        let config = config();
        let mut session = approved(&config);
        let stage =
            |name| Event::parse(json!({"version":1,"event":"status","stage":name})).unwrap();
        session
            .accept(&stage("awaiting_source_lease"), &config)
            .unwrap();
        assert!(!session.bound && !session.active);
        assert!(
            session
                .accept(&stage("awaiting_peer_consent"), &config)
                .is_err()
        );
        session
            .accept(&stage("awaiting_touchpad_offer"), &config)
            .unwrap();
        assert!(!session.bound && !session.active);
        assert!(
            session
                .accept(&stage("awaiting_source_lease"), &config)
                .is_err()
        );
        let status = status_json(&session, "status", "starting");
        accept(&mut session, &config, status).unwrap();
        assert!(
            session
                .accept(&stage("awaiting_touchpad_offer"), &config)
                .is_err()
        );
        for name in ["awaiting_source_lease", "awaiting_touchpad_offer"] {
            assert!(waiting(&config).accept(&stage(name), &config).is_err());
        }
    }

    #[test]
    fn pure_touchpad_framed_events_reject_duplicate_keys_and_count_overrun() {
        fn frame(payload: &str) -> Vec<u8> {
            let mut bytes = (payload.len() as u32).to_be_bytes().to_vec();
            bytes.extend_from_slice(payload.as_bytes());
            bytes
        }
        let config = config();
        let approval = serde_json::to_string(&approval_json(&config)).unwrap();
        let duplicate = approval.replacen(
            "\"port_id\":",
            &format!("\"port_id\":\"{}\",\"port_id\":", config.source.port_id),
            1,
        );
        let escaped = r#"{"version":1,"event":"stopped","ev\u0065nt":"stopped"}"#;
        for payload in [duplicate.as_str(), escaped] {
            let mut reader = io::Cursor::new(frame(payload));
            assert!(Frames::default().poll(&mut reader).is_err());
        }
        let mut reader = io::Cursor::new(frame(&approval));
        let mut frames = Frames::default();
        let event = Event::parse(frames.poll(&mut reader).unwrap().unwrap()).unwrap();
        assert!(matches!(event, Event::Approval(_)));
        let mut reader = io::Cursor::new(frame(r#"{"version":1,"event":"stopped"}"#));
        frames.count = frames.limit;
        assert!(frames.poll(&mut reader).is_err());
    }

    #[test]
    fn pure_touchpad_stop_and_failed_events_close_every_future_action() {
        let config = config();
        for terminal in ["stopped", "failed"] {
            let mut session = waiting(&config);
            let fresh = session.approval.as_ref().unwrap().expected();
            session.stopping = true;
            assert!(input_action(fresh.as_bytes(), &session).is_err());
            assert!(input_action(b"status", &session).is_err());
            assert_eq!(input_action(b"cancel", &session).unwrap(), Input::Stop);
            let event = Event::parse(json!({"version":1,"event":terminal})).unwrap();
            session.accept(&event, &config).unwrap();
            assert!(session.accept(&event, &config).is_err());
            assert!(session.accept(&listening(), &config).is_err());
            assert!(
                session
                    .accept(&Event::parse(approval_json(&config)).unwrap(), &config)
                    .is_err()
            );
        }
        let session = waiting(&config);
        for line in [
            b"renew".as_slice(),
            b"reconnect",
            b"restart",
            b"revoke peer",
            b"yes",
        ] {
            assert!(input_action(line, &session).is_err());
        }
    }

    #[test]
    fn pure_touchpad_native_mode_has_no_fixture_fallback() {
        assert_eq!(Mode::Native.capability(), "linux-touchpad");
        assert_eq!(Mode::Native.banner(), "");
    }

    #[cfg(feature = "touchpad-test-fixture")]
    #[test]
    fn pure_touchpad_fixture_presentation_is_explicitly_software_only() {
        for name in [
            "normal",
            "fail-create",
            "partial-write",
            "delayed-commit",
            "backpressure",
        ] {
            let mode = Mode::Fixture(name.into());
            assert_eq!(mode.capability(), "linux-touchpad-fixture");
            assert_eq!(
                mode.banner(),
                "SOFTWARE-ONLY TOUCHPAD FIXTURE: no OS device or gesture acceptance.\r\n"
            );
        }
    }

    #[test]
    fn pure_touchpad_terminal_event_is_final_and_presentation_has_exact_scope() {
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
            "At most 5 contacts; 120x70 mm; non-clickable; pressure enabled".into(),
            "Node Active follows committed backend ACK; binding alone is not activation".into(),
            "OS gestures and physical input are not verified".into(),
            "independent Source consent and both leases remain required".into(),
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
