//! Finite terminal consent for the existing paired-store playback launcher.
//! This module is a client of the sibling Node, never an audio/store backend.
use super::*;
use capyio_core::{CapabilityId, PortId, RouteId, RouteState, SessionId};
use std::{net::SocketAddrV4, os::unix::fs::OpenOptionsExt, path::Path};

const BOOTSTRAP: Duration = Duration::from_secs(5);
const BEFORE_APPROVAL: Duration = Duration::from_secs(55);
const CONSENT: Duration = Duration::from_secs(30);
const STOP_GRACE: Duration = Duration::from_secs(3);
const FRAME_TIMEOUT: Duration = Duration::from_secs(5);
const FORMAT: &str = "pcm-s16le-48000-mono";
const SECURITY: &str = "unauthenticated_udp_trusted_lab_only";
const WARNING: &str =
    "WARNING: plaintext unauthenticated UDP media; trusted isolated lab only.\r\n";

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

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Mode {
    #[default]
    Playback,
    VirtualMicrophone,
}
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Playback => "playback",
            Self::VirtualMicrophone => "virtual_microphone",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Proposal {
    #[default]
    LinuxLed,
}

// A deliberately local copy of the nonsecret launcher schema. Importing the
// platform Host here would give the controller an unnecessary backend boundary.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Config {
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
    #[serde(default)]
    proposal_mode: Proposal,
}
impl Config {
    fn parse(bytes: &[u8], local: NodeId, peer: NodeId) -> io::Result<Self> {
        if bytes.is_empty() || bytes.len() > MAX_FRAME {
            return Err(invalid());
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        value.validate(local, peer)?;
        Ok(value)
    }

    fn validate(&self, local: NodeId, peer: NodeId) -> io::Result<()> {
        // Match the Host's concrete lab-address policy, including its port-1
        // placeholder when TCP listening requests an ephemeral port.
        let mut listen = self.listen;
        if listen.port() == 0 {
            listen.set_port(1);
        }
        if self.version != 1
            || self.node_id != local
            || local.as_uuid().is_nil()
            || peer.as_uuid().is_nil()
            || peer == local
            || self.source.node_id != peer
            || !self.source.valid()
            || self.listen.ip() != self.media.ip()
            || !lab_address(listen)
            || !lab_address(self.media)
            || listen == self.media
            || !(1..=300).contains(&self.duration_secs)
        {
            return Err(invalid());
        }
        match (self.endpoint_mode, self.target.as_deref()) {
            (Mode::Playback, Some(target)) if explicit_target(target) => Ok(()),
            (Mode::VirtualMicrophone, None) => Ok(()),
            _ => Err(invalid()),
        }
    }
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

fn read_config(path: &Path, local: NodeId, peer: NodeId) -> io::Result<Config> {
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
    Config::parse(&bytes[..used], local, peer)
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
        let expected_name = match config.endpoint_mode {
            Mode::Playback => None,
            Mode::VirtualMicrophone => {
                Some(format!("capyio.virtual-microphone.{}", self.sink.port_id))
            }
        };
        if self.request_id.as_uuid().is_nil()
            || self.route_id.as_uuid().is_nil()
            || self.session_id.as_uuid().is_nil()
            || self.source != config.source
            || !self.sink.valid()
            || self.sink.node_id != config.node_id
            || self.target != config.target
            || self.endpoint_mode != config.endpoint_mode
            || self.node_name != expected_name
            || self.format != FORMAT
            || self.duration_secs != config.duration_secs
            || self.approval_timeout_secs != CONSENT.as_secs()
            || self.media_security != SECURITY
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
            "status" => Ok(Self::Route(decode(value)?)),
            "stopped" => Ok(Self::Stopped(decode(value)?)),
            "failed" => Ok(Self::Failed(decode(value)?)),
            _ => Err(invalid()),
        }
    }

    fn canonical(&self) -> io::Result<String> {
        let result = match self {
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
    stopping: bool,
    finished: Option<Finished>,
    packets_written: u64,
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
                };
                if !self.listening || !valid {
                    return Err(invalid());
                }
            }
            Event::Route(value) => {
                if !self.approved
                    || !self.offered
                    || self.approval.as_ref().map(|a| a.route_id) != Some(value.route_id)
                    || value.state == RouteState::Draft
                    || value.packets_sent != 0
                    || value.packets_written < self.packets_written
                {
                    return Err(invalid());
                }
                self.packets_written = value.packets_written;
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
            .unwrap_or("new owned virtual microphone (not created before approval)");
        let mut result = format!(
            "Source {} -> Sink {}\r\nMode: {}; device: {}; duration: {} seconds\r\n",
            config.source.node_id,
            config.node_id,
            config.endpoint_mode.name(),
            device,
            config.duration_secs
        );
        result.push_str(WARNING);
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
    require_node_capability_for("linux-playback")?;
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
    let mut node = Node::spawn_for(args, "linux-playback", Some(peer))?;
    node.set_stop_on_drop();
    node.set_frame_limit(2048)?;
    let bootstrap = (Instant::now() + BOOTSTRAP).min(deadline);
    node.send(&config, bootstrap)?;
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
            let status = matches!(event, Event::Stage(_) | Event::Route(_));
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
                    "\r\nPlayback event: {canonical}\r\n{}",
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
        )
    }
    fn approval_json(config: &Config) -> Value {
        let sink = Port {
            node_id: config.node_id,
            capability_id: CapabilityId::new(),
            port_id: PortId::new(),
        };
        json!({"version":1,"event":"approval_required","request_id":MessageId::new(),
            "route_id":RouteId::new(),"session_id":SessionId::new(),"source":config.source,
            "sink":sink,"target":config.target,"endpoint_mode":config.endpoint_mode,
            "node_name":if config.endpoint_mode==Mode::VirtualMicrophone {Some(format!("capyio.virtual-microphone.{}",sink.port_id))}else{None},
            "format":FORMAT,"duration_secs":config.duration_secs,"approval_timeout_secs":30,"media_security":SECURITY})
    }
    fn approval(config: &Config) -> Approval {
        let Event::Approval(value) = Event::parse(approval_json(config)).unwrap() else {
            panic!("expected typed approval")
        };
        *value
    }

    #[test]
    fn pure_playback_config_matches_public_schema_and_exact_peer_selection() {
        let value = config_json();
        let config = parse_config(&value).unwrap();
        assert_eq!(config.endpoint_mode, Mode::Playback);
        assert!(
            config
                .validate(NodeId::new(), config.source.node_id)
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
        file.write_all(&serde_json::to_vec(&config).unwrap())
            .unwrap();
        drop(file);
        assert!(read_config(&path, config.node_id, config.source.node_id).is_ok());
        assert!(read_config(&path, NodeId::new(), config.source.node_id).is_err());
        let link = directory.join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_config(&link, config.node_id, config.source.node_id).is_err());
        assert!(read_config(&directory, config.node_id, config.source.node_id).is_err());
        assert!(
            read_config(
                Path::new("relative.json"),
                config.node_id,
                config.source.node_id
            )
            .is_err()
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o620)).unwrap();
        assert!(read_config(&path, config.node_id, config.source.node_id).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&path, [b' '; MAX_FRAME + 1]).unwrap();
        assert!(read_config(&path, config.node_id, config.source.node_id).is_err());
        fs::write(&path, []).unwrap();
        assert!(read_config(&path, config.node_id, config.source.node_id).is_err());
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
}
