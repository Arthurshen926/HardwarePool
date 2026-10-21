#![cfg(target_os = "linux")]
//! Hosted executable checks only: ephemeral certificates, loopback control and
//! inherited FIFO pipes. Ordinary tests never return a media offer or open an
//! audio process. The explicitly ignored playback and virtual-microphone cases
//! additionally require the hosted harness's private synthetic PipeWire graph
//! and exact opt-in markers.
#[cfg(feature = "paired-store")]
#[path = "support/paired_store.rs"]
mod paired_store;
#[cfg(feature = "paired-store")]
#[path = "support/playback_console.rs"]
mod playback_console;

use capyio_android_jni::AndroidCatalogRuntimeV1;
use capyio_control_credentials::ControlCredentialBundle;
use capyio_control_link::{CatalogControlLink, ReceivedControlMessage};
use capyio_control_tls::{
    AuthenticatedControlClientChannel, ControlTlsClientIdentity, ControlTlsEndpoint,
    ControlTlsNodeBinding,
};
use capyio_core::*;
use capyio_pairing::{
    CertificateFingerprint, CredentialHandle, PairedPeerDirectory, PairedPeerRecord,
};
use capyio_protocol::{
    NATIVE_AUDIO_INBOUND_DRAFT_METADATA_KEY, NATIVE_AUDIO_INBOUND_DRAFT_MONO_V1,
    NativeAudioDataOffer, NativeAudioEndpoint, NativeAudioPreset,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io::{self, Read, Write},
    ops::{Deref, DerefMut},
    os::fd::AsFd,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const BOUND: Duration = Duration::from_secs(5);
fn nonblocking(fd: impl AsFd) {
    let flags = rustix::fs::fcntl_getfl(&fd).unwrap();
    rustix::fs::fcntl_setfl(fd, flags | rustix::fs::OFlags::NONBLOCK).unwrap();
}
fn drain(reader: &mut impl Read, bytes: &mut Vec<u8>) {
    let mut buffer = [0; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => bytes.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => panic!("child output read failed: {error}"),
        }
        assert!(bytes.len() <= 1_048_576, "bounded executable output");
    }
}
struct Process {
    #[cfg(feature = "paired-store")]
    pairing: Option<paired_store::PersistedPairing>,
    #[cfg(feature = "paired-store")]
    console: Option<playback_console::Console>,
    child: Child,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}
impl Process {
    fn start(args: &[&str]) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_capyio-node"))
            .args(args)
            .env_remove("RUST_LOG")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        nonblocking(child.stdin.as_ref().unwrap());
        nonblocking(child.stdout.as_ref().unwrap());
        nonblocking(child.stderr.as_ref().unwrap());
        Self {
            child,
            #[cfg(feature = "paired-store")]
            pairing: None,
            #[cfg(feature = "paired-store")]
            console: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }
    fn playback() -> Self {
        Self::start(&["linux-playback", "--trusted-lab"])
    }
    fn frame(&mut self, body: &[u8]) {
        let mut frame = Vec::from(u32::try_from(body.len()).unwrap().to_be_bytes());
        frame.extend_from_slice(body);
        let deadline = Instant::now() + BOUND;
        let mut offset = 0;
        while offset < frame.len() {
            assert!(Instant::now() < deadline, "bounded bootstrap write");
            match self.child.stdin.as_mut().unwrap().write(&frame[offset..]) {
                Ok(0) => panic!("closed launcher input"),
                Ok(count) => offset += count,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    thread::sleep(Duration::from_millis(2))
                }
                Err(_) => panic!("launcher input write failed"),
            }
        }
        frame.fill(0);
    }
    fn command(&mut self, command: Value) {
        #[cfg(feature = "paired-store")]
        if self.console.is_some() {
            match command["command"].as_str().unwrap() {
                "approve" => {
                    let request = command["request_id"].as_str().unwrap();
                    let route = command["route_id"].as_str().unwrap();
                    self.wait_console_prompt(request, route);
                    let console = self.console.as_mut().unwrap();
                    console.send(format!("start {request} {route} {}\n", console.peer).as_bytes());
                }
                "status" => self.console.as_mut().unwrap().send(b"status\n"),
                "stop" => self.console.as_mut().unwrap().send(b"stop\n"),
                _ => panic!("console adapter accepts only existing public playback operations"),
            }
            return;
        }
        self.frame(&serde_json::to_vec(&command).unwrap());
    }
    fn collect(&mut self) {
        #[cfg(feature = "paired-store")]
        if let Some(console) = &mut self.console {
            console.collect(&mut self.child, &mut self.stdout, &mut self.stderr);
            if let Some(pairing) = &self.pairing {
                pairing.assert_no_leak(&console.tty);
            }
        } else {
            drain(self.child.stdout.as_mut().unwrap(), &mut self.stdout);
            drain(self.child.stderr.as_mut().unwrap(), &mut self.stderr);
        }
        #[cfg(not(feature = "paired-store"))]
        {
            drain(self.child.stdout.as_mut().unwrap(), &mut self.stdout);
            drain(self.child.stderr.as_mut().unwrap(), &mut self.stderr);
        }
        #[cfg(feature = "paired-store")]
        if let Some(pairing) = &self.pairing {
            pairing.assert_no_leak(&self.stdout);
            pairing.assert_no_leak(&self.stderr);
        }
    }
    fn event(&mut self, expected: &str) -> Value {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
            #[cfg(feature = "paired-store")]
            if let Some(console) = &mut self.console
                && let Some(value) = console.next_event()
            {
                assert_eq!(value["event"], expected);
                if expected == "listening" {
                    let node = console.identify_node(&self.child);
                    if let Some(pairing) = &self.pairing {
                        for pid in [self.child.id(), node] {
                            for name in ["cmdline", "environ"] {
                                pairing.assert_no_leak(
                                    &std::fs::read(format!("/proc/{pid}/{name}")).unwrap(),
                                );
                            }
                        }
                    }
                }
                return value;
            }
            if self.stdout.len() >= 4 {
                let length = u32::from_be_bytes(self.stdout[..4].try_into().unwrap()) as usize;
                assert!((1..=4096).contains(&length), "bounded local event");
                if self.stdout.len() >= length + 4 {
                    let value: Value = serde_json::from_slice(&self.stdout[4..length + 4]).unwrap();
                    self.stdout.drain(..length + 4);
                    assert_eq!(value["version"], 1);
                    assert_eq!(value["event"], expected);
                    return value;
                }
            }
            assert!(
                Instant::now() < deadline,
                "missing bounded local event {expected}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
    #[cfg(feature = "paired-store")]
    fn wait_console_prompt(&mut self, request: &str, route: &str) {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
            let console = self.console.as_ref().unwrap();
            if console.prompt_seen(request, route) {
                console.assert_hidden();
                // begin_prompt flushes once more immediately after writing.
                thread::sleep(Duration::from_millis(25));
                return;
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "console exited before exact fresh prompt"
            );
            assert!(Instant::now() < deadline, "bounded fresh console prompt");
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn owner_pid(&mut self) -> u32 {
        #[cfg(feature = "paired-store")]
        if let Some(console) = &mut self.console {
            return console.identify_node(&self.child);
        }
        self.child.id()
    }
    fn finish(&mut self) -> ExitStatus {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
            if let Some(status) = self.child.try_wait().unwrap() {
                self.collect();
                #[cfg(feature = "paired-store")]
                if let Some(console) = &self.console {
                    console.assert_finished();
                }
                #[cfg(feature = "paired-store")]
                if let Some(pairing) = &self.pairing {
                    let state = pairing.service.state.lock().unwrap();
                    assert!(state.reads > 0, "actual protected credential read required");
                    assert_eq!(state.forbidden, 0);
                    eprintln!(
                        "capyio_audio_paired_store=passed protected_read=true secret_redacted=true"
                    );
                }
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "executable must terminate promptly"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        // No blocking reader/writer threads survive assertions or timeout paths.
        #[cfg(feature = "paired-store")]
        if self.console.is_some() {
            // SAFETY: the console owns the new setsid process group, including
            // its Node and audio child. Never target an existing user group.
            unsafe {
                libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
struct Fixture {
    config: Value,
    directory: Vec<u8>,
    bundle: ControlCredentialBundle,
    client: ControlTlsClientIdentity,
    runtime: AndroidCatalogRuntimeV1,
    source: PortRef,
    nodes: ControlTlsNodeBinding,
}
impl Fixture {
    fn new() -> Self {
        // Actual Java-produced Android catalog schema and production Runtime.
        // This fixture represents available platform permission, not Route consent:
        // the production Android local consent gate remains Pending until resolved.
        let runtime = AndroidCatalogRuntimeV1::new(include_str!(
            "../../../platform/android/fixtures/android-core-catalog-v1.json"
        ))
        .unwrap();
        let node = runtime.snapshot().local_node;
        let microphone = node
            .capabilities
            .values()
            .find(|cap| cap.class == CapabilityClass::Microphone)
            .unwrap();
        let port = microphone
            .ports
            .values()
            .find(|port| port.direction == PortDirection::Source)
            .unwrap();
        let source = PortRef {
            node_id: node.id,
            capability_id: microphone.id,
            port_id: port.id,
        };
        let local = NodeId::new();
        let (bundle, client, client_pin) = identities();
        let mut directory = PairedPeerDirectory::new(local).unwrap();
        directory
            .add(
                PairedPeerRecord::new(
                    source.node_id,
                    "Ephemeral peer",
                    "capyio-test-client",
                    CredentialHandle::new("ephemeral.test.only").unwrap(),
                    client_pin,
                    1,
                )
                .unwrap(),
            )
            .unwrap();
        Self {
            config: json!({"version":1,"node_id":local,"source":source,"listen":"127.0.0.1:0","media":"127.0.0.1:40211","target":"synthetic.test","duration_secs":10}),
            directory: directory.encode_json().unwrap(),
            bundle,
            client,
            runtime,
            source,
            nodes: ControlTlsNodeBinding::new(local, source.node_id).unwrap(),
        }
    }
    fn launch(&self, paired: bool) -> Process {
        assert!(
            hosted_console_scenario().is_none() || paired,
            "console graph requires protected pairing"
        );
        if paired {
            #[cfg(feature = "paired-store")]
            {
                let local = serde_json::from_value(self.config["node_id"].clone()).unwrap();
                let pairing =
                    paired_store::PersistedPairing::new(local, &self.directory, &self.bundle);
                if hosted_console_scenario().is_some() {
                    let (child, console) = playback_console::Console::start(
                        &pairing,
                        &self.config,
                        hosted_console_scenario().as_deref() == Some("pretyped"),
                        "playback",
                        &[],
                    );
                    return Process {
                        child,
                        console: Some(console),
                        pairing: Some(pairing),
                        stdout: vec![],
                        stderr: vec![],
                    };
                }
                let args = pairing.args("linux-playback");
                let mut process =
                    Process::start(&args.iter().map(String::as_str).collect::<Vec<_>>());
                process.pairing = Some(pairing);
                process.command(self.config.clone());
                process.command(json!({"version":1,"credential_source":"paired-store"}));
                return process;
            }
            #[cfg(not(feature = "paired-store"))]
            panic!("paired audio acceptance requires the paired-store build feature");
        }
        let mut process = Process::playback();
        self.bootstrap(&mut process);
        process
    }
    fn bootstrap(&self, process: &mut Process) {
        process.command(self.config.clone());
        process.frame(&self.directory);
        let mut encoded = self.bundle.encode().unwrap();
        process.frame(&encoded);
        encoded.fill(0);
    }
    fn connect(mut self, process: &mut Process) -> AndroidPeer {
        let listening = process.event("listening");
        let client = AuthenticatedControlClientChannel::connect_fresh_session(
            ControlTlsEndpoint {
                address: listening["address"].as_str().unwrap().parse().unwrap(),
                io_timeout: BOUND,
            },
            "localhost",
            self.client,
            self.nodes,
        )
        .unwrap();
        let link = self
            .runtime
            .admit_dialed_catalog_exchange(
                client,
                "Ephemeral playback".into(),
                &BTreeSet::from([self.source.capability_id]),
            )
            .unwrap();
        AndroidPeer {
            link,
            runtime: self.runtime,
        }
    }
}
struct AndroidPeer {
    link: CatalogControlLink,
    runtime: AndroidCatalogRuntimeV1,
}
impl Deref for AndroidPeer {
    type Target = CatalogControlLink;
    fn deref(&self) -> &Self::Target {
        &self.link
    }
}
impl DerefMut for AndroidPeer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.link
    }
}
impl AndroidPeer {
    fn receive_proposal(&mut self, expected: RouteId) {
        let ReceivedControlMessage::RouteStatus(status) = self
            .link
            .receive_control_message_until(Instant::now() + BOUND)
            .unwrap()
        else {
            panic!("expected real Draft proposal before any offer")
        };
        assert_eq!(status.route.id, expected);
        assert_eq!(status.route.state, RouteState::Draft);
        assert_eq!(status.route.authorization, AuthorizationState::Pending);
        assert!(status.native_audio_data.is_none());
        let route = self
            .runtime
            .receive_audio_route_status(&self.link, &status, "127.0.0.1", now_ms())
            .unwrap();
        assert_eq!(route.authorization, AuthorizationState::Pending);
        assert!(self.runtime.snapshot().port_leases.is_empty());
        self.link.send_route_status(&route).unwrap();
        let requests: Value =
            serde_json::from_slice(&self.runtime.resource_requests_json(now_ms()).unwrap())
                .unwrap();
        assert_eq!(
            requests.as_array().unwrap().len(),
            1,
            "actual Android consent request exists"
        );
    }
    fn consent(&mut self, id: RouteId, allow: bool) {
        self.runtime
            .resolve_touchpad_request(id, allow, now_ms())
            .unwrap();
        self.runtime
            .publish_audio_control(&mut self.link, now_ms())
            .unwrap();
    }
    fn negotiate_to_offer(&mut self, expected: RouteId) -> capyio_protocol::ValidatedRouteStatus {
        self.negotiate_to_offer_with_lifetime(expected, Duration::from_secs(10))
    }
    fn negotiate_to_offer_with_lifetime(
        &mut self,
        expected: RouteId,
        lifetime: Duration,
    ) -> capyio_protocol::ValidatedRouteStatus {
        let deadline = Instant::now() + BOUND;
        let mut narrowed = false;
        let mut leased = false;
        loop {
            match self.link.receive_control_message_until(deadline).unwrap() {
                ReceivedControlMessage::RouteStatus(status) => {
                    assert_eq!(status.route.id, expected);
                    let route = self
                        .runtime
                        .receive_audio_route_status(&self.link, &status, "127.0.0.1", now_ms())
                        .unwrap();
                    if status.native_audio_data.is_some() {
                        assert!(
                            narrowed && leased,
                            "offer only after exact finite consent and Source lease"
                        );
                        assert_eq!(route.authorization, status.route.authorization);
                        assert_eq!(
                            route.state,
                            RouteState::Draft,
                            "capture must wait for explicit Start"
                        );
                        assert!(
                            !self.runtime.audio_should_run(expected, now_ms()),
                            "actual Android lifecycle must not run before endpoint preparation and Start"
                        );
                        return *status;
                    }
                    assert_eq!(status.route.state, RouteState::Draft);
                    let AuthorizationState::Authorized {
                        expires_at_ms: Some(expiry),
                    } = route.authorization
                    else {
                        panic!("finite Android consent retained")
                    };
                    assert!(
                        expiry > now_ms()
                            && expiry <= now_ms() + u64::try_from(lifetime.as_millis()).unwrap()
                    );
                    assert_eq!(route.authorization, status.route.authorization);
                    narrowed = true;
                    self.link.send_route_status(&route).unwrap();
                }
                ReceivedControlMessage::PortLeaseRequest(request) => {
                    assert!(narrowed, "no remote lease before finite grant agreement");
                    assert_eq!(request.request().route_id, expected);
                    self.runtime
                        .answer_audio_port_lease(&mut self.link, request, now_ms())
                        .unwrap();
                    assert!(!self.runtime.snapshot().port_leases.is_empty());
                    leased = true;
                }
                _ => panic!("unexpected message during real Android negotiation"),
            }
        }
    }
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
fn pending() -> (Process, AndroidPeer, Value) {
    let fixture = Fixture::new();
    let mut process = Process::playback();
    fixture.bootstrap(&mut process);
    let mut peer = fixture.connect(&mut process);
    assert!(
        peer.runtime.snapshot().peers[0]
            .capabilities
            .values()
            .all(|capability| {
                !capability
                    .metadata
                    .contains_key(NATIVE_AUDIO_INBOUND_DRAFT_METADATA_KEY)
            }),
        "omitting proposal_mode retains Linux-led admission without inbound advertisement"
    );
    let approval = process.event("approval_required");
    assert_eq!(approval["target"], "synthetic.test");
    assert_eq!(approval["duration_secs"], 10);
    assert_eq!(
        approval["session_id"],
        serde_json::to_value(peer.session_id()).unwrap()
    );
    process.command(json!({"version":1,"command":"status"}));
    assert_eq!(process.event("status")["stage"], "awaiting_local_approval");
    let observation = Instant::now() + Duration::from_millis(150);
    while Instant::now() < observation {
        assert!(
            // Observation duration is not a framing deadline: a scheduler pause
            // at its boundary must not poison an otherwise idle healthy channel.
            peer.poll_control_message_until(Instant::now() + BOUND)
                .unwrap()
                .is_none(),
            "no offer before local approval"
        );
        thread::sleep(Duration::from_millis(2));
    }
    (process, peer, approval)
}
fn approve(approval: &Value) -> Value {
    json!({"version":1,"command":"approve","request_id":approval["request_id"],"route_id":approval["route_id"]})
}

/// Incoming-mode checks use the production Android Start/coordinator, not a
/// fabricated Ready peer. No ordinary test publishes a reciprocal endpoint,
/// starts capture, opens UDP, or launches a playback process.
mod incoming_draft {
    use super::*;

    fn sink(peer: &AndroidPeer) -> PortRef {
        let snapshot = peer.runtime.snapshot();
        assert_eq!(snapshot.peers.len(), 1);
        let node = &snapshot.peers[0];
        assert_eq!(node.platform, Platform::Linux);
        assert_eq!(node.capabilities.len(), 1);
        let capability = node.capabilities.values().next().unwrap();
        assert_eq!(capability.class, CapabilityClass::Speaker);
        assert_eq!(capability.ports.len(), 1);
        let port = capability.ports.values().next().unwrap();
        assert_eq!(port.direction, PortDirection::Sink);
        assert_eq!(port.profile, ProfileId::audio_frames_v1());
        assert_eq!(
            port.formats,
            vec![FormatDescriptor::new("pcm-s16le-48000-mono")]
        );
        PortRef {
            node_id: node.id,
            capability_id: capability.id,
            port_id: port.id,
        }
    }

    fn no_control_or_capture(peer: &mut AndroidPeer, route: Option<RouteId>) {
        let deadline = Instant::now() + Duration::from_millis(150);
        while Instant::now() < deadline {
            assert!(
                peer.poll_control_message_until(Instant::now() + BOUND)
                    .unwrap()
                    .is_none(),
                "no lease, media offer or Start before independent finite consent"
            );
            assert!(peer.runtime.snapshot().port_leases.is_empty());
            if let Some(id) = route {
                assert!(!peer.runtime.audio_should_run(id, now_ms()));
            }
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn ready() -> (Process, AndroidPeer) {
        ready_with_store(false)
    }

    fn ready_with_store(paired: bool) -> (Process, AndroidPeer) {
        let mut fixture = Fixture::new();
        fixture.config["proposal_mode"] = json!("incoming_draft");
        let mut process = fixture.launch(paired);
        let mut peer = fixture.connect(&mut process);
        let destination = sink(&peer);
        assert_eq!(
            peer.runtime.snapshot().peers[0].capabilities[&destination.capability_id]
                .metadata
                .get(NATIVE_AUDIO_INBOUND_DRAFT_METADATA_KEY)
                .map(String::as_str),
            Some(NATIVE_AUDIO_INBOUND_DRAFT_MONO_V1)
        );
        assert!(peer.runtime.snapshot().routes.is_empty());
        process.command(json!({"version":1,"command":"status"}));
        // event() also rejects an approval_required event emitted too early.
        assert_eq!(process.event("status")["stage"], "awaiting_incoming_draft");
        no_control_or_capture(&mut peer, None);
        (process, peer)
    }

    fn begin(peer: &mut AndroidPeer) -> Route {
        let destination = sink(peer);
        let route = peer
            .runtime
            .begin_audio_use(destination.node_id, "microphone")
            .unwrap();
        assert_eq!(route.session_id, peer.session_id());
        assert_eq!(route.sink, destination);
        assert_eq!(route.state, RouteState::Draft);
        assert_eq!(route.authorization, AuthorizationState::Pending);
        assert_eq!(route.epoch, 0);
        assert!(route.selected_format.is_none());
        assert!(route.selected_qos_mode.is_none());
        assert!(peer.runtime.snapshot().port_leases.is_empty());
        assert!(!peer.runtime.audio_should_run(route.id, now_ms()));
        route
    }

    fn publish(peer: &mut AndroidPeer) {
        peer.runtime
            .publish_audio_control(&mut peer.link, now_ms())
            .unwrap();
    }

    fn pending() -> (Process, AndroidPeer, Route, Value) {
        pending_with_store(false)
    }

    fn pending_with_store(paired: bool) -> (Process, AndroidPeer, Route, Value) {
        let (mut process, mut peer) = ready_with_store(paired);
        let route = begin(&mut peer);
        publish(&mut peer);
        let approval = process.event("approval_required");
        assert_eq!(approval["route_id"], json!(route.id));
        assert_eq!(approval["session_id"], json!(route.session_id));
        assert_eq!(approval["source"], json!(route.source));
        assert_eq!(approval["sink"], json!(route.sink));
        assert_eq!(approval["target"], "synthetic.test");
        assert_eq!(approval["duration_secs"], 10);
        assert_eq!(approval["format"], "pcm-s16le-48000-mono");
        process.command(json!({"version":1,"command":"status"}));
        assert_eq!(process.event("status")["stage"], "awaiting_local_approval");
        no_control_or_capture(&mut peer, Some(route.id));
        (process, peer, route, approval)
    }

    fn receive_local_grant(peer: &mut AndroidPeer, proposal: &Route) -> Route {
        let ReceivedControlMessage::RouteStatus(status) = peer
            .receive_control_message_until(Instant::now() + BOUND)
            .unwrap()
        else {
            panic!("Linux must send its finite Draft grant before requesting a Source lease")
        };
        assert_eq!(status.route.id, proposal.id);
        assert_eq!(status.route.session_id, proposal.session_id);
        assert_eq!(status.route.source, proposal.source);
        assert_eq!(status.route.sink, proposal.sink);
        assert_eq!(status.route.state, RouteState::Draft);
        assert_eq!(status.route.epoch, 0);
        assert!(status.native_audio_data.is_none());
        let AuthorizationState::Authorized {
            expires_at_ms: Some(expiry),
        } = status.route.authorization
        else {
            panic!("incoming Android initiation requires a finite Linux grant first")
        };
        assert!(expiry > now_ms() && expiry <= now_ms() + 10_000);
        let local = peer
            .runtime
            .receive_audio_route_status(&peer.link, &status, "127.0.0.1", now_ms())
            .unwrap();
        assert_eq!(local.authorization, status.route.authorization);
        assert_eq!(local.state, RouteState::Draft);
        assert!(peer.runtime.snapshot().port_leases.is_empty());
        assert!(!peer.runtime.audio_should_run(local.id, now_ms()));
        let requests: Value =
            serde_json::from_slice(&peer.runtime.resource_requests_json(now_ms()).unwrap())
                .unwrap();
        assert!(
            requests.as_array().unwrap().is_empty(),
            "the actual Android Start is local intent, not a second fabricated consent"
        );
        local
    }

    fn offer(peer: &mut AndroidPeer, expected: &Route) -> capyio_protocol::ValidatedRouteStatus {
        let deadline = Instant::now() + BOUND;
        let mut leased = false;
        loop {
            match peer.receive_control_message_until(deadline).unwrap() {
                ReceivedControlMessage::RouteStatus(status) => {
                    assert_eq!(status.route.id, expected.id);
                    assert_eq!(status.route.session_id, expected.session_id);
                    assert_eq!(status.route.source, expected.source);
                    assert_eq!(status.route.sink, expected.sink);
                    assert_eq!(status.route.authorization, expected.authorization);
                    let local = peer
                        .runtime
                        .receive_audio_route_status(&peer.link, &status, "127.0.0.1", now_ms())
                        .unwrap();
                    assert_eq!(local.authorization, expected.authorization);
                    assert!(!peer.runtime.audio_should_run(expected.id, now_ms()));
                    if let Some(offer) = &status.native_audio_data {
                        assert!(
                            leased,
                            "no media offer before owner-authoritative Source lease"
                        );
                        assert_eq!(status.route.state, RouteState::Starting);
                        assert_eq!(status.route.epoch, 1);
                        assert_eq!(offer.endpoint, NativeAudioEndpoint::Sink);
                        assert_eq!(offer.preset, NativeAudioPreset::PcmS16le48000Mono);
                        assert_eq!(offer.port, 40211);
                        assert_eq!(local.state, RouteState::Draft);
                        return *status;
                    }
                    assert!(!leased, "Draft grant agreement precedes Source lease");
                    assert_eq!(status.route.state, RouteState::Draft);
                    assert_eq!(status.route.epoch, 0);
                    peer.send_route_status(&local).unwrap();
                }
                ReceivedControlMessage::PortLeaseRequest(request) => {
                    assert!(!leased, "one bounded Source lease acquisition");
                    assert_eq!(request.request().route_id, expected.id);
                    assert_eq!(request.request().port, expected.source);
                    peer.runtime
                        .answer_audio_port_lease(&mut peer.link, request, now_ms())
                        .unwrap();
                    assert_eq!(peer.runtime.snapshot().port_leases.len(), 1);
                    leased = true;
                }
                _ => panic!("unexpected message during incoming Android negotiation"),
            }
        }
    }

    fn stopped(process: &mut Process, peer: &mut AndroidPeer) {
        process.command(json!({"version":1,"command":"stop"}));
        process.event("stopped");
        assert!(process.finish().success());
        assert!(process.stderr.is_empty());
        assert!(
            peer.receive_control_message_until(Instant::now() + BOUND)
                .is_err(),
            "dedicated Session closes without media or Start after Stop"
        );
    }

    fn rejected(process: &mut Process, peer: &mut AndroidPeer) {
        process.event("failed");
        assert!(!process.finish().success());
        assert!(process.stdout.is_empty(), "rejection cannot issue approval");
        assert!(peer.runtime.snapshot().port_leases.is_empty());
        assert!(
            peer.receive_control_message_until(Instant::now() + BOUND)
                .is_err(),
            "invalid input cannot produce a grant, lease, offer or Start"
        );
    }

    #[test]
    fn ipc_linux_incoming_actual_android_start_requires_independent_local_approval() {
        actual_android_start_requires_independent_local_approval(false);
    }

    #[cfg(feature = "paired-store")]
    #[test]
    fn ipc_linux_incoming_paired_store_requires_independent_local_approval() {
        actual_android_start_requires_independent_local_approval(true);
    }

    fn actual_android_start_requires_independent_local_approval(paired: bool) {
        let (mut process, mut peer, proposal, approval) = pending_with_store(paired);
        process.command(approve(&approval));
        let grant = receive_local_grant(&mut peer, &proposal);
        // Linux local consent alone cannot acquire the Android microphone.
        no_control_or_capture(&mut peer, Some(proposal.id));
        publish(&mut peer);
        offer(&mut peer, &grant);
        assert_eq!(
            process.event("awaiting_peer_offer")["route_id"],
            json!(proposal.id)
        );
        process.command(json!({"version":1,"command":"status"}));
        let status = process.event("status");
        assert_eq!(status["route_id"], json!(proposal.id));
        assert_eq!(status["packets_written"], 0);
        // Prepare only synthetic endpoint metadata. Do not publish it, bind a
        // socket, or open a device. Linux must remain waiting for its peer offer.
        peer.runtime
            .prepare_audio_endpoint(proposal.id, 40212)
            .unwrap();
        assert!(!peer.runtime.audio_should_run(proposal.id, now_ms()));
        stopped(&mut process, &mut peer);
        peer.runtime.stop_audio_direction("microphone").unwrap();
        assert!(peer.runtime.snapshot().port_leases.is_empty());
        assert!(!peer.runtime.audio_should_run(proposal.id, now_ms()));
        assert!(
            peer.runtime
                .prepare_audio_endpoint(proposal.id, 40212)
                .is_err(),
            "Stop must discard the previously prepared endpoint"
        );
        let delayed_start = capyio_protocol::ValidatedRouteCommand {
            message_id: MessageId::new(),
            session_id: proposal.session_id,
            route_id: proposal.id,
            action: capyio_protocol::RouteCommandAction::Start,
        };
        assert!(
            peer.runtime
                .stage_audio_route_command(&peer.link, &delayed_start, now_ms())
                .is_err(),
            "a fresh delayed Start cannot revive stopped Android capture"
        );
        assert!(!peer.runtime.audio_should_run(proposal.id, now_ms()));
        let AndroidPeer { link, mut runtime } = peer;
        runtime.close_control_link(link).unwrap();
    }

    #[test]
    fn ipc_linux_incoming_explicit_linux_led_mode_does_not_advertise_or_accept_start() {
        let mut fixture = Fixture::new();
        fixture.config["proposal_mode"] = json!("linux_led");
        let mut process = Process::playback();
        fixture.bootstrap(&mut process);
        let mut peer = fixture.connect(&mut process);
        let destination = sink(&peer);
        assert!(
            !peer.runtime.snapshot().peers[0].capabilities[&destination.capability_id]
                .metadata
                .contains_key(NATIVE_AUDIO_INBOUND_DRAFT_METADATA_KEY)
        );
        assert!(
            peer.runtime
                .begin_audio_use(destination.node_id, "microphone")
                .is_err(),
            "a compatible catalog alone must not enable Android-led initiation"
        );
        assert!(peer.runtime.snapshot().routes.is_empty());
        process.event("approval_required");
        stopped(&mut process, &mut peer);
    }

    #[test]
    fn ipc_linux_incoming_smaller_android_grant_is_narrowed_before_lease() {
        let (mut process, mut peer, proposal, approval) = pending();
        process.command(approve(&approval));
        let mut grant = receive_local_grant(&mut peer, &proposal);
        let AuthorizationState::Authorized {
            expires_at_ms: Some(expiry),
        } = grant.authorization
        else {
            unreachable!()
        };
        grant.authorization = AuthorizationState::Authorized {
            expires_at_ms: Some(expiry - 2_000),
        };
        // Test the peer's right to restrict consent. Its actual Android mirror
        // adopts the restriction only on Linux's exact Draft acknowledgement.
        peer.send_route_status(&grant).unwrap();
        offer(&mut peer, &grant);
        process.event("awaiting_peer_offer");
        stopped(&mut process, &mut peer);
    }

    #[test]
    fn ipc_linux_incoming_stop_before_draft_or_local_approval_or_peer_grant() {
        let (mut process, mut peer) = ready();
        stopped(&mut process, &mut peer);
        assert!(peer.runtime.snapshot().routes.is_empty());

        let (mut process, mut peer, proposal, _) = pending();
        stopped(&mut process, &mut peer);
        assert!(peer.runtime.snapshot().port_leases.is_empty());
        assert!(!peer.runtime.audio_should_run(proposal.id, now_ms()));

        let (mut process, mut peer, proposal, approval) = pending();
        process.command(approve(&approval));
        receive_local_grant(&mut peer, &proposal);
        stopped(&mut process, &mut peer);
        assert!(peer.runtime.snapshot().port_leases.is_empty());
        assert!(!peer.runtime.audio_should_run(proposal.id, now_ms()));
    }

    #[test]
    fn ipc_linux_incoming_android_cancellation_before_local_approval_is_terminal() {
        let (mut process, mut peer, proposal, _) = pending();
        peer.runtime.stop_audio_direction("microphone").unwrap();
        publish(&mut peer);
        rejected(&mut process, &mut peer);
        assert!(!peer.runtime.audio_should_run(proposal.id, now_ms()));
    }

    #[test]
    fn ipc_linux_incoming_launcher_eof_before_local_approval_cannot_grant() {
        let (mut process, mut peer, _, _) = pending();
        drop(process.child.stdin.take());
        assert!(!process.finish().success());
        assert!(peer.runtime.snapshot().port_leases.is_empty());
        assert!(
            peer.receive_control_message_until(Instant::now() + BOUND)
                .is_err()
        );
    }

    #[test]
    fn ipc_linux_incoming_wrong_nonce_or_route_cannot_approve() {
        for field in ["request_id", "route_id"] {
            let (mut process, mut peer, _, approval) = pending();
            let mut command = approve(&approval);
            command[field] = json!(MessageId::new());
            process.command(command);
            rejected(&mut process, &mut peer);
        }
    }

    #[test]
    fn ipc_linux_incoming_previous_launch_nonce_and_sink_cannot_be_reused() {
        let (mut previous, mut previous_peer, old_route, old_approval) = pending();
        stopped(&mut previous, &mut previous_peer);
        let (mut process, mut peer, route, approval) = pending();
        assert_ne!(old_route.session_id, route.session_id);
        assert_ne!(old_route.sink.capability_id, route.sink.capability_id);
        assert_ne!(old_route.sink.port_id, route.sink.port_id);
        assert_ne!(old_approval["request_id"], approval["request_id"]);
        let mut replay = approve(&approval);
        replay["request_id"] = old_approval["request_id"].clone();
        process.command(replay);
        rejected(&mut process, &mut peer);
    }

    #[test]
    fn ipc_linux_incoming_rejects_malformed_proposals_before_approval() {
        for mutation in [
            "source_node",
            "source_capability",
            "source_port",
            "sink_node",
            "sink_capability",
            "sink_port",
            "profile",
            "backend",
            "epoch",
            "state",
            "authorized",
            "denied",
            "format",
            "qos",
            "compatible_formats",
            "compatible_qos",
            "diagnostic",
        ] {
            let (mut process, mut peer) = ready();
            let mut proposal = begin(&mut peer);
            match mutation {
                "source_node" => proposal.source.node_id = NodeId::new(),
                "source_capability" => proposal.source.capability_id = CapabilityId::new(),
                "source_port" => proposal.source.port_id = PortId::new(),
                "sink_node" => proposal.sink.node_id = NodeId::new(),
                "sink_capability" => proposal.sink.capability_id = CapabilityId::new(),
                "sink_port" => proposal.sink.port_id = PortId::new(),
                "profile" => proposal.profile = ProfileId::video_frames_v1(),
                "backend" => proposal.backend = RouteBackend::CapyDataPlane,
                "epoch" => proposal.epoch = 1,
                "state" => proposal.state = RouteState::Starting,
                "authorized" => {
                    proposal.authorization = AuthorizationState::Authorized {
                        expires_at_ms: Some(now_ms() + 10_000),
                    }
                }
                "denied" => proposal.authorization = AuthorizationState::Denied,
                "format" => {
                    proposal.selected_format = Some(FormatDescriptor::new("pcm-s16le-48000-mono"))
                }
                "qos" => proposal.selected_qos_mode = Some(QosMode::Interactive),
                "compatible_formats" => proposal.compatible_formats.clear(),
                "compatible_qos" => proposal.compatible_qos_modes.clear(),
                "diagnostic" => proposal.diagnostic_ids.push(ProblemId::new()),
                _ => unreachable!(),
            }
            peer.send_route_status(&proposal).unwrap();
            rejected(&mut process, &mut peer);
        }
    }

    #[test]
    fn ipc_linux_incoming_unsolicited_media_offer_cannot_create_approval() {
        let (mut process, mut peer) = ready();
        let mut proposal = begin(&mut peer);
        proposal.state = RouteState::Starting;
        proposal.epoch = 1;
        proposal.authorization = AuthorizationState::Authorized {
            expires_at_ms: Some(now_ms() + 10_000),
        };
        peer.send_native_audio_data_offer(
            &proposal,
            NativeAudioDataOffer {
                port: 40212,
                stream_id: StreamId::new(),
                endpoint: NativeAudioEndpoint::Source,
                preset: NativeAudioPreset::PcmS16le48000Mono,
            },
            now_ms(),
        )
        .unwrap();
        rejected(&mut process, &mut peer);
    }

    #[test]
    fn ipc_linux_incoming_duplicate_draft_does_not_create_consent_or_another_approval() {
        let (mut process, mut peer, proposal, _) = pending();
        peer.send_route_status(&proposal).unwrap();
        process.command(json!({"version":1,"command":"status"}));
        // A second approval event would make this exact event assertion fail.
        assert_eq!(process.event("status")["stage"], "awaiting_local_approval");
        no_control_or_capture(&mut peer, Some(proposal.id));
        assert_eq!(
            peer.runtime.snapshot().routes[0].authorization,
            AuthorizationState::Pending
        );
        stopped(&mut process, &mut peer);
    }

    #[test]
    fn ipc_linux_incoming_second_route_cannot_replace_pending_local_approval() {
        let (mut process, mut peer, mut proposal, _) = pending();
        proposal.id = RouteId::new();
        peer.send_route_status(&proposal).unwrap();
        rejected(&mut process, &mut peer);
    }

    #[test]
    fn ipc_linux_incoming_expired_extended_or_drifted_android_grant_cannot_lease() {
        for mutation in [
            "expired",
            "extended",
            "unbounded",
            "source",
            "sink",
            "epoch",
        ] {
            let (mut process, mut peer, proposal, approval) = pending();
            process.command(approve(&approval));
            let mut grant = receive_local_grant(&mut peer, &proposal);
            let AuthorizationState::Authorized {
                expires_at_ms: Some(expiry),
            } = grant.authorization
            else {
                unreachable!()
            };
            match mutation {
                "expired" => {
                    grant.authorization = AuthorizationState::Authorized {
                        expires_at_ms: Some(now_ms() - 1),
                    }
                }
                "extended" => {
                    grant.authorization = AuthorizationState::Authorized {
                        expires_at_ms: Some(expiry + 1),
                    }
                }
                "unbounded" => {
                    grant.authorization = AuthorizationState::Authorized {
                        expires_at_ms: None,
                    }
                }
                "source" => grant.source.port_id = PortId::new(),
                "sink" => grant.sink.port_id = PortId::new(),
                "epoch" => grant.epoch = 1,
                _ => unreachable!(),
            }
            peer.send_route_status(&grant).unwrap();
            rejected(&mut process, &mut peer);
        }
    }
}

#[test]
fn ipc_linux_playback_approved_finite_offer_status_and_stop() {
    let (mut process, mut peer, approval) = pending();
    let before = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    process.command(approve(&approval));
    let route_id = serde_json::from_value(approval["route_id"].clone()).unwrap();
    peer.receive_proposal(route_id);
    let observation = Instant::now() + Duration::from_millis(150);
    while Instant::now() < observation {
        assert!(
            peer.poll_control_message_until(Instant::now() + BOUND)
                .unwrap()
                .is_none(),
            "no offer/lease before actual Android consent"
        );
        thread::sleep(Duration::from_millis(2));
    }
    peer.consent(route_id, true);
    let status = peer.negotiate_to_offer(route_id);
    assert_eq!(
        process.event("awaiting_peer_offer")["route_id"],
        approval["route_id"]
    );
    assert_eq!(
        serde_json::to_value(status.route.id).unwrap(),
        approval["route_id"]
    );
    assert_eq!(status.route.state, RouteState::Starting);
    let AuthorizationState::Authorized {
        expires_at_ms: Some(expiry),
    } = status.route.authorization
    else {
        panic!("finite local authorization required")
    };
    let after = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    assert!((before + 10_000..=after + 10_000).contains(&expiry));
    let offer = status.native_audio_data.unwrap();
    assert_eq!(offer.endpoint, NativeAudioEndpoint::Sink);
    assert_eq!(offer.preset, NativeAudioPreset::PcmS16le48000Mono);
    assert_eq!(offer.port, 40211);
    process.command(json!({"version":1,"command":"status"}));
    let status = process.event("status");
    assert_eq!(status["route_id"], approval["route_id"]);
    assert_eq!(status["packets_written"], 0);
    // No reciprocal offer: the media/PipeWire worker must never start.
    process.command(json!({"version":1,"command":"stop"}));
    process.event("stopped");
    assert!(process.finish().success());
    assert!(process.stderr.is_empty());
}

#[test]
fn ipc_linux_playback_android_denial_never_acquires_source_or_offers() {
    let (mut process, mut peer, approval) = pending();
    process.command(approve(&approval));
    let id = serde_json::from_value(approval["route_id"].clone()).unwrap();
    peer.receive_proposal(id);
    peer.consent(id, false);
    assert!(peer.runtime.snapshot().port_leases.is_empty());
    process.event("failed");
    assert!(!process.finish().success());
    assert!(
        peer.receive_control_message_until(Instant::now() + BOUND)
            .is_err(),
        "denied Android consent must not produce lease/offer/Start"
    );
}

#[test]
fn ipc_linux_playback_local_stop_while_android_consent_pending() {
    let (mut process, mut peer, approval) = pending();
    process.command(approve(&approval));
    let id = serde_json::from_value(approval["route_id"].clone()).unwrap();
    peer.receive_proposal(id);
    process.command(json!({"version":1,"command":"stop"}));
    process.event("stopped");
    assert!(process.finish().success());
    assert!(peer.runtime.snapshot().port_leases.is_empty());
    assert!(
        peer.receive_control_message_until(Instant::now() + BOUND)
            .is_err(),
        "Stop before Android consent must not produce lease/offer/Start"
    );
}

#[test]
fn ipc_linux_playback_wrong_nonce_or_route_rejected_without_offer() {
    for field in ["request_id", "route_id"] {
        let (mut process, mut peer, approval) = pending();
        let mut command = approve(&approval);
        command[field] = serde_json::to_value(MessageId::new()).unwrap();
        process.command(command);
        process.event("failed");
        assert!(!process.finish().success());
        assert!(
            peer.receive_control_message_until(Instant::now() + BOUND)
                .is_err(),
            "invalid consent must not emit an offer"
        );
    }
}

#[test]
fn ipc_linux_playback_launcher_eof_cancels_before_authorization() {
    let (mut process, mut peer, _) = pending();
    drop(process.child.stdin.take());
    // EOF poisons the framed launcher channel, including writes. Verify bounded
    // termination via exit status rather than requiring an event on a closed channel.
    assert!(!process.finish().success());
    assert!(String::from_utf8_lossy(&process.stderr).contains("Linux lab playback launch failed"));
    assert!(
        peer.receive_control_message_until(Instant::now() + BOUND)
            .is_err(),
        "launcher EOF cannot authorize an offer"
    );
}

#[test]
fn ipc_linux_playback_blocked_stdout_exits_within_write_deadlines() {
    let fixture = Fixture::new();
    let mut process = Process::playback();
    fixture.bootstrap(&mut process);
    process.event("listening");
    // Only resize this fixture's pipe, never an inherited user descriptor.
    assert_eq!(
        rustix::pipe::fcntl_setpipe_size(process.child.stdout.as_ref().unwrap(), 4096).unwrap(),
        4096
    );
    // This input fits within one 4096-byte pipe; its status replies exceed it.
    // Deliberately do not read stdout again until the executable has exited.
    let command = serde_json::to_vec(&json!({"version":1,"command":"status"})).unwrap();
    assert!((command.len() + 4) * 100 <= 4096);
    for _ in 0..100 {
        process.frame(&command);
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = process.child.try_wait().unwrap() {
            assert!(!status.success());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "blocked stdout must not strand the launcher"
        );
        thread::sleep(Duration::from_millis(2));
    }
    process.collect();
    assert!(String::from_utf8_lossy(&process.stderr).contains("Linux lab playback launch failed"));
}

#[test]
fn ipc_linux_playback_previous_invocation_approval_cannot_be_replayed() {
    let (mut previous, previous_peer, old_approval) = pending();
    previous.command(json!({"version":1,"command":"stop"}));
    previous.event("stopped");
    assert!(previous.finish().success());
    drop(previous_peer);
    let (mut process, mut peer, approval) = pending();
    assert_ne!(old_approval["request_id"], approval["request_id"]);
    assert_ne!(old_approval["session_id"], approval["session_id"]);
    // Use the current Route but an actually issued nonce from a closed launch.
    let mut stale = approve(&approval);
    stale["request_id"] = old_approval["request_id"].clone();
    process.command(stale);
    process.event("failed");
    assert!(!process.finish().success());
    assert!(
        peer.receive_control_message_until(Instant::now() + BOUND)
            .is_err(),
        "a previous launch cannot authorize this Route"
    );
}

#[test]
fn ipc_linux_playback_preplayed_approval_rejected_before_peer() {
    let fixture = Fixture::new();
    let mut process = Process::playback();
    fixture.bootstrap(&mut process);
    process.command(json!({"version":1,"command":"approve","request_id":MessageId::new(),"route_id":RouteId::new()}));
    process.event("listening");
    process.event("failed");
    assert!(!process.finish().success());
    assert!(
        process.stdout.is_empty(),
        "no later admission or offer event"
    );
}

#[test]
fn ipc_linux_playback_malformed_secret_bootstrap_is_redacted() {
    const MARKER: &str = "PRIVATE-TEST-MARKER-DO-NOT-ECHO";
    let fixture = Fixture::new();
    let mut process = Process::playback();
    process.command(fixture.config.clone());
    process.frame(&fixture.directory);
    process.frame(MARKER.as_bytes());
    let failed = process.event("failed");
    assert_eq!(failed, json!({"version":1,"event":"failed"}));
    assert!(!process.finish().success());
    assert!(!String::from_utf8_lossy(&process.stdout).contains(MARKER));
    let stderr = String::from_utf8_lossy(&process.stderr);
    assert!(!stderr.contains(MARKER));
    assert!(stderr.contains("Linux lab playback launch failed"));
    assert!(
        process.stdout.is_empty(),
        "no listener on invalid secret bootstrap"
    );
}

#[test]
fn ipc_linux_playback_requires_explicit_opt_in_and_default_remains_demo() {
    let mut missing = Process::start(&["linux-playback"]);
    assert!(!missing.finish().success());
    assert!(missing.stdout.is_empty());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--trusted-lab"));
    let mut accidental_secret = Process::start(&[
        "linux-playback",
        "--trusted-lab",
        "PRIVATE_ARG_SECRET_MARKER",
    ]);
    assert!(!accidental_secret.finish().success());
    assert!(
        !String::from_utf8_lossy(&accidental_secret.stderr).contains("PRIVATE_ARG_SECRET_MARKER")
    );
    let mut default = Process::start(&[]);
    assert!(default.finish().success());
    let snapshot: Value = serde_json::from_slice(&default.stdout).unwrap();
    assert_eq!(snapshot["routes"].as_array().unwrap().len(), 4);
    assert!(!String::from_utf8_lossy(&default.stdout).contains("approval_required"));
}

/// Only scripts/run_pipewire_loopback.py --e2e-fixture may run this case. The
/// ordinary suite must report it as ignored, not silently treat a missing daemon
/// or environment opt-in as success. No physical Android/audio hardware is used.
#[test]
#[ignore = "requires the dedicated hosted private-PipeWire end-to-end harness"]
fn hosted_pipewire_android_source_end_to_end() {
    hosted_pipewire::run();
}

/// A separate direct Audio/Source gate, not a renamed null-sink monitor.
#[test]
#[ignore = "requires the dedicated hosted private-PipeWire virtual-microphone harness"]
fn hosted_pipewire_virtual_microphone_end_to_end() {
    hosted_pipewire::run_virtual_microphone();
}

#[test]
#[ignore = "requires the dedicated hosted private-PipeWire console PTY harness"]
fn hosted_console_playback_end_to_end() {
    assert!(
        hosted_console_scenario().is_some(),
        "explicit console profile required"
    );
    hosted_pipewire::run();
}

#[test]
#[ignore = "requires the dedicated hosted private-PipeWire console PTY harness"]
fn hosted_console_virtual_microphone_end_to_end() {
    assert!(
        hosted_console_scenario().is_some(),
        "explicit console profile required"
    );
    hosted_pipewire::run_virtual_microphone();
}

mod hosted_pipewire {
    use super::*;
    use capyio_audio::{AudioMediaPacket, AudioMediaStreamBinding, AudioStreamSpec};
    use capyio_native_audio_lan::{
        NativeLanEndpointConfig, NativeLanRouteBinding, NativeLanUdpEndpoint,
    };
    use capyio_protocol::RouteCommandAction;
    use std::{
        ffi::OsStr,
        fs::{self, OpenOptions},
        net::{Ipv4Addr, UdpSocket},
        os::unix::fs::{FileTypeExt, PermissionsExt},
        path::{Path, PathBuf},
    };

    const MARKER: &[u8] = b"capyio-hosted-pipewire-loopback-v1\n";
    const LIFETIME: Duration = Duration::from_secs(30);
    const WHOLE_BUDGET: Duration = Duration::from_secs(20);
    const FRAME_SAMPLES: u32 = 480;
    const FRAMES: u64 = 500;
    const LATE_FRAMES: u64 = 20;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Scenario {
        Playback,
        Waveform,
        NoConsumer,
        ChildExit,
        ServerDisconnect,
    }

    impl Scenario {
        fn parse(value: &str) -> Option<Self> {
            match value {
                "waveform" => Some(Self::Waveform),
                "no_consumer" => Some(Self::NoConsumer),
                "child_exit" => Some(Self::ChildExit),
                "server_disconnect" => Some(Self::ServerDisconnect),
                _ => None,
            }
        }

        fn name(self) -> &'static str {
            match self {
                Self::Playback => "playback",
                Self::Waveform => "waveform",
                Self::NoConsumer => "no_consumer",
                Self::ChildExit => "child_exit",
                Self::ServerDisconnect => "server_disconnect",
            }
        }

        fn virtual_microphone(self) -> bool {
            self != Self::Playback
        }

        fn injected_failure(self) -> bool {
            matches!(self, Self::ChildExit | Self::ServerDisconnect)
        }
    }

    fn opted_in(test: Option<&OsStr>, e2e: Option<&OsStr>, remote: Option<&OsStr>) -> bool {
        test == Some(OsStr::new("1"))
            && e2e == Some(OsStr::new("1"))
            && remote == Some(OsStr::new("capyio-test"))
    }

    fn exact_marker(path: &Path) -> bool {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return false,
            Err(error) => panic!("cannot inspect hosted marker: {error}"),
        };
        assert!(
            metadata.is_file(),
            "marker must not be a link or special file"
        );
        assert_eq!(metadata.len(), MARKER.len() as u64);
        assert_eq!(fs::read(path).unwrap(), MARKER);
        true
    }

    fn private_runtime() -> PathBuf {
        assert!(
            opted_in(
                std::env::var_os("CAPYIO_PIPEWIRE_TEST").as_deref(),
                std::env::var_os("CAPYIO_PIPEWIRE_E2E").as_deref(),
                std::env::var_os("PIPEWIRE_REMOTE").as_deref(),
            ),
            "requires deliberate hosted synthetic end-to-end opt-in"
        );
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").expect("private runtime required");
        for key in [
            "PIPEWIRE_RUNTIME_DIR",
            "PIPEWIRE_CONFIG_DIR",
            "XDG_CONFIG_HOME",
            "HOME",
        ] {
            assert_eq!(std::env::var_os(key).as_ref(), Some(&runtime));
        }
        let runtime = PathBuf::from(runtime);
        let metadata = fs::symlink_metadata(&runtime).unwrap();
        assert!(runtime.is_absolute() && metadata.is_dir());
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
        assert!(exact_marker(&runtime.join(".capyio-pipewire-loopback")));
        assert!(
            fs::symlink_metadata(runtime.join("capyio-test"))
                .unwrap()
                .file_type()
                .is_socket(),
            "private PipeWire daemon must already be listening"
        );
        for name in [
            ".capyio-before-consent",
            ".capyio-consent-checked",
            ".capyio-before-android-consent",
            ".capyio-android-consent-checked",
            ".capyio-links-ready",
            ".capyio-fault-ready",
            ".capyio-virtual-microphone.json",
            ".capyio-after-stop",
            ".capyio-stop-checked",
            ".capyio-console-start.json",
            ".capyio-console-owner.json",
        ] {
            assert!(
                !runtime.join(name).try_exists().unwrap(),
                "fresh handshake markers required"
            );
        }
        runtime
    }

    fn publish_marker(runtime: &Path, announce: &str) {
        let temporary = runtime.join(format!("{announce}.tmp"));
        let destination = runtime.join(announce);
        assert!(!destination.try_exists().unwrap());
        let mut marker = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .unwrap();
        marker.write_all(MARKER).unwrap();
        drop(marker);
        fs::rename(temporary, destination).unwrap();
    }

    fn checkpoint(
        runtime: &Path,
        announce: &str,
        acknowledged: &str,
        process: &mut Process,
        deadline: Instant,
    ) {
        publish_marker(runtime, announce);
        wait_marker(runtime, acknowledged, process, deadline);
    }

    fn wait_marker(runtime: &Path, name: &str, process: &mut Process, deadline: Instant) {
        while !exact_marker(&runtime.join(name)) {
            assert!(Instant::now() < deadline, "bounded hosted graph handshake");
            assert!(
                process.child.try_wait().unwrap().is_none(),
                "playback owner exited during hosted graph handshake"
            );
            process.collect();
            thread::sleep(Duration::from_millis(5));
        }
    }

    // The same five-second waveform as the component fixture: two seconds of
    // 1 kHz, one second of exact zero PCM, two seconds of 2 kHz, 48 kHz S16LE.
    fn packet(sequence: u64, binding: &AudioMediaStreamBinding) -> AudioMediaPacket {
        assert!(sequence < FRAMES + LATE_FRAMES);
        let frequency = if sequence < 200 {
            1_000.0
        } else if sequence < 300 {
            0.0
        } else {
            2_000.0
        };
        let first_sample = sequence * u64::from(FRAME_SAMPLES);
        let payload = (0..FRAME_SAMPLES)
            .flat_map(|index| {
                let angle =
                    std::f64::consts::TAU * frequency * (first_sample + u64::from(index)) as f64
                        / 48_000.0;
                ((angle.sin() * 8_192.0).round() as i16).to_le_bytes()
            })
            .collect();
        AudioMediaPacket {
            stream_id: binding.stream_id,
            stream_epoch: binding.stream_epoch,
            sequence,
            source_timestamp_micros: sequence * 10_000 + 1,
            first_sample_index: first_sample,
            sample_count: FRAME_SAMPLES,
            discontinuity: false,
            payload,
        }
    }

    fn no_peer_media(peer: &mut AndroidPeer, id: RouteId) {
        let deadline = Instant::now() + Duration::from_millis(150);
        while Instant::now() < deadline {
            assert!(
                peer.poll_control_message_until(Instant::now() + BOUND)
                    .unwrap()
                    .is_none(),
                "no lease, endpoint offer or Start before independent consent"
            );
            assert!(!peer.runtime.audio_should_run(id, now_ms()));
            assert!(peer.runtime.snapshot().port_leases.is_empty());
            thread::sleep(Duration::from_millis(2));
        }
    }

    pub(super) fn run() {
        run_scenario(Scenario::Playback);
    }

    pub(super) fn run_virtual_microphone() {
        let scenario = std::env::var("CAPYIO_PIPEWIRE_VIRTUAL_MICROPHONE")
            .ok()
            .and_then(|value| Scenario::parse(&value))
            .expect("exact hosted virtual microphone scenario opt-in required");
        run_scenario(scenario);
    }

    fn run_scenario(scenario: Scenario) {
        let runtime = private_runtime(); // Fail before any process/socket opens.
        let deadline = Instant::now() + WHOLE_BUDGET;
        let console_scenario = hosted_console_scenario();
        let lifetime = if console_scenario.as_deref() == Some("expiry") {
            Duration::from_secs(4)
        } else {
            LIFETIME
        };
        // Reserve one ephemeral loopback ingress through consent negotiation,
        // then release it immediately before the production owner needs to bind.
        let ingress_reservation = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let ingress = ingress_reservation.local_addr().unwrap();
        let mut fixture = Fixture::new();
        fixture.config["media"] = json!(ingress.to_string());
        if scenario.virtual_microphone() {
            fixture.config.as_object_mut().unwrap().remove("target");
            fixture.config["endpoint_mode"] = json!("virtual_microphone");
        } else {
            fixture.config["target"] = json!("capyio.test.null");
        }
        fixture.config["duration_secs"] = json!(lifetime.as_secs());
        let mut process = fixture.launch(hosted_paired_store());
        let mut peer = fixture.connect(&mut process);
        let approval = process.event("approval_required");
        if let Some(name) = &console_scenario {
            let owner = process.owner_pid();
            let evidence = json!({"console_pid": process.child.id(), "owner_pid": owner,
                "scenario": name, "mode": if scenario.virtual_microphone() {"virtual_microphone"} else {"playback"}});
            fs::write(
                runtime.join(".capyio-console-owner.json"),
                serde_json::to_vec(&evidence).unwrap(),
            )
            .unwrap();
        }
        if scenario.virtual_microphone() {
            assert!(approval["target"].is_null());
            assert_eq!(approval["endpoint_mode"], "virtual_microphone");
            let sink: PortRef = serde_json::from_value(approval["sink"].clone()).unwrap();
            let expected_name = format!("capyio.virtual-microphone.{}", sink.port_id);
            assert_eq!(approval["node_name"], expected_name);
            let catalog = &peer.runtime.snapshot().peers[0];
            let capability = catalog.capabilities.get(&sink.capability_id).unwrap();
            assert_eq!(capability.class, CapabilityClass::Microphone);
            assert_eq!(
                capability.ports.get(&sink.port_id).unwrap().direction,
                PortDirection::Sink,
                "system Source projection does not invert the network Sink"
            );
            assert!(
                !capability
                    .metadata
                    .contains_key(NATIVE_AUDIO_INBOUND_DRAFT_METADATA_KEY),
                "endpoint_mode does not implicitly select incoming proposal mode"
            );
            let mut evidence = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(runtime.join(".capyio-virtual-microphone.json.tmp"))
                .unwrap();
            serde_json::to_writer(
                &mut evidence,
                &json!({
                    "node_name": expected_name,
                    "sink_port_id": sink.port_id,
                    "owner_pid": process.owner_pid(),
                }),
            )
            .unwrap();
            drop(evidence);
            fs::rename(
                runtime.join(".capyio-virtual-microphone.json.tmp"),
                runtime.join(".capyio-virtual-microphone.json"),
            )
            .unwrap();
        } else {
            assert_eq!(approval["target"], "capyio.test.null");
        }
        assert_eq!(approval["duration_secs"], lifetime.as_secs());
        assert_eq!(approval["format"], "pcm-s16le-48000-mono");
        assert_eq!(
            approval["session_id"],
            serde_json::to_value(peer.session_id()).unwrap()
        );
        let id: RouteId = serde_json::from_value(approval["route_id"].clone()).unwrap();
        assert!(peer.runtime.snapshot().routes.is_empty());
        no_peer_media(&mut peer, id);
        process.command(json!({"version":1,"command":"status"}));
        assert_eq!(process.event("status")["stage"], "awaiting_local_approval");
        // The harness proves the endpoint is absent before either consent. In
        // playback mode it also obtains silent live null-sink monitor samples;
        // virtual-microphone mode instead checks the entire graph has no Source.
        checkpoint(
            &runtime,
            ".capyio-before-consent",
            ".capyio-consent-checked",
            &mut process,
            deadline,
        );
        if console_scenario
            .as_deref()
            .is_some_and(console_pending_scenario)
        {
            finish_console_pending(
                console_scenario.as_deref().unwrap(),
                &approval,
                &mut process,
                &mut peer,
                &runtime,
                deadline,
                scenario.virtual_microphone(),
            );
            drop(ingress_reservation);
            assert!(
                UdpSocket::bind(ingress).is_ok(),
                "pending console never retained media ingress"
            );
            return;
        }
        #[cfg(feature = "paired-store")]
        if console_scenario.is_some() {
            process.wait_console_prompt(
                approval["request_id"].as_str().unwrap(),
                approval["route_id"].as_str().unwrap(),
            );
            process
                .console
                .as_ref()
                .unwrap()
                .assert_presentation(&approval);
        }
        let consent_started = Instant::now();
        process.command(approve(&approval));
        peer.receive_proposal(id);
        no_peer_media(&mut peer, id);
        checkpoint(
            &runtime,
            ".capyio-before-android-consent",
            ".capyio-android-consent-checked",
            &mut process,
            deadline,
        );
        peer.consent(id, true);
        let offered = peer.negotiate_to_offer_with_lifetime(id, lifetime);
        assert_eq!(
            process.event("awaiting_peer_offer")["route_id"],
            approval["route_id"]
        );
        let offer = *offered.native_audio_data.as_ref().unwrap().as_ref();
        assert_eq!(offer.endpoint, NativeAudioEndpoint::Sink);
        assert_eq!(offer.preset, NativeAudioPreset::PcmS16le48000Mono);
        assert_eq!(offer.port, ingress.port());
        assert_eq!(offered.route.state, RouteState::Starting);
        assert!(!peer.runtime.audio_should_run(id, now_ms()));
        let leases = peer.runtime.snapshot().port_leases;
        assert_eq!(leases.len(), 1);
        let negotiated_lease = leases[0];
        assert_eq!(negotiated_lease.route_id, id);
        assert_eq!(negotiated_lease.port, offered.route.source);
        assert_eq!(negotiated_lease.session_id, offered.route.session_id);
        let AuthorizationState::Authorized {
            expires_at_ms: Some(grant_expiry),
        } = offered.route.authorization
        else {
            panic!("finite grant required before the actual Source lease")
        };
        assert!(negotiated_lease.expires_at_ms > now_ms());
        assert!(negotiated_lease.expires_at_ms < grant_expiry);
        let lease_deadline = Instant::now()
            + Duration::from_millis(negotiated_lease.expires_at_ms.saturating_sub(now_ms()));

        // This is the synthetic platform endpoint. The real Android coordinator
        // must prepare and publish its own reciprocal Source offer, preserving
        // the actual Session/Route/Stream/epoch instead of fabricating Ready.
        let source_socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let source = source_socket.local_addr().unwrap();
        peer.runtime
            .prepare_audio_endpoint(id, source.port())
            .unwrap();
        assert!(!peer.runtime.audio_should_run(id, now_ms()));
        drop(ingress_reservation);
        let AndroidPeer {
            link,
            runtime: host,
        } = &mut peer;
        host.publish_audio_control(link, now_ms()).unwrap();
        let ReceivedControlMessage::RouteCommand(start) = peer
            .receive_control_message_until(deadline.min(Instant::now() + BOUND))
            .unwrap()
        else {
            panic!("real Linux owner must explicitly Start the prepared Source")
        };
        assert_eq!(start.route_id, id);
        assert_eq!(start.action, RouteCommandAction::Start);
        let AndroidPeer {
            link,
            runtime: host,
        } = &mut peer;
        let started = host
            .stage_audio_route_command(link, &start, now_ms())
            .unwrap();
        assert_eq!(started.state, RouteState::Starting);
        assert_eq!(started.epoch, offered.route.epoch);
        assert!(host.audio_should_run(id, now_ms()));
        link.send_route_status(&started).unwrap();
        let ReceivedControlMessage::RouteStatus(ready) = peer
            .receive_control_message_until(deadline.min(Instant::now() + BOUND))
            .unwrap()
        else {
            panic!("actual Linux worker readiness status required")
        };
        assert_eq!(ready.route.id, id);
        assert_eq!(ready.route.state, RouteState::Active);
        assert!(ready.native_audio_data.is_none());
        let AndroidPeer {
            link,
            runtime: host,
        } = &mut peer;
        host.receive_audio_route_status(link, &ready, "127.0.0.1", now_ms())
            .unwrap();
        let snapshot = host.snapshot();
        let route = snapshot.routes.iter().find(|route| route.id == id).unwrap();
        let session = snapshot
            .sessions
            .iter()
            .find(|session| session.id == route.session_id)
            .unwrap();
        let binding = AudioMediaStreamBinding {
            session_id: route.session_id,
            route_id: route.id,
            stream_id: offer.stream_id,
            stream_epoch: u32::try_from(route.epoch).unwrap(),
            selected_spec: AudioStreamSpec::voice_interactive(),
        };
        let guarded =
            NativeLanRouteBinding::from_live_context(binding.clone(), route, session, now_ms())
                .unwrap();
        let mut sender = NativeLanUdpEndpoint::from_socket(
            source_socket,
            NativeLanEndpointConfig {
                peer: ingress,
                read_timeout: Duration::from_millis(10),
                inflight_packet_capacity: 2,
            },
            binding.clone(),
        )
        .unwrap();
        assert_eq!(sender.metrics().packets_sent, 0);
        wait_marker(&runtime, ".capyio-links-ready", &mut process, deadline);
        process.command(json!({"version":1,"command":"status"}));
        assert_eq!(process.event("status")["packets_written"], 0);

        let signal_started = Instant::now();
        // No-consumer mode proves bounded Stop without any graph link. Existing
        // synthetic full-stdin tests remain the explicit backpressure proof.
        let frames = if matches!(console_scenario.as_deref(), Some("expiry" | "remote_eof")) {
            1
        } else {
            match scenario {
                Scenario::NoConsumer => 50,
                Scenario::ChildExit | Scenario::ServerDisconnect => 1,
                Scenario::Playback | Scenario::Waveform => FRAMES,
            }
        };
        for sequence in 0..frames {
            let when = signal_started + Duration::from_millis(sequence * 10);
            while Instant::now() < when {
                assert!(
                    Instant::now() < deadline,
                    "bounded five-second media signal"
                );
                thread::sleep(Duration::from_millis(1));
            }
            assert!(Instant::now() < deadline);
            let snapshot = peer.runtime.snapshot();
            let route = snapshot.routes.iter().find(|route| route.id == id).unwrap();
            let session = snapshot
                .sessions
                .iter()
                .find(|session| session.id == route.session_id)
                .unwrap();
            assert!(peer.runtime.audio_should_run(id, now_ms()));
            guarded
                .require_live_context(route, session, now_ms())
                .unwrap();
            sender.send_packet(&packet(sequence, &binding)).unwrap();
        }
        assert_eq!(sender.metrics().packets_sent, frames);
        // Require all packets through the real owner, not merely UDP send success.
        loop {
            assert!(Instant::now() < deadline, "bounded receiver drain");
            process.command(json!({"version":1,"command":"status"}));
            let status = process.event("status");
            assert_eq!(status["route_id"], approval["route_id"]);
            assert_eq!(status["state"], "active");
            let written = status["packets_written"].as_u64().unwrap();
            assert!(written <= frames);
            if written == frames {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        if matches!(console_scenario.as_deref(), Some("expiry" | "remote_eof")) {
            if console_scenario.as_deref() == Some("remote_eof") {
                // Close the actual authenticated control channel, not the
                // PipeWire server and not a simulated local status.
                peer.link.close();
            }
            process.event("failed");
            if console_scenario.as_deref() == Some("expiry") {
                assert!(
                    now_ms() >= negotiated_lease.expires_at_ms,
                    "a failure before the actual negotiated lease expiry is not expiry evidence"
                );
                // Production reserves one second of transit headroom from the
                // grant, then uses this exact Source lease expiry for its Sink.
                // Neither idle failure nor waiting for the longer grant is proof.
                assert!(!peer.runtime.audio_should_run(id, now_ms()));
                assert!(
                    Instant::now() <= lease_deadline + Duration::from_secs(2),
                    "the actual finite lease cannot be silently renewed"
                );
                assert!(
                    consent_started.elapsed() <= lifetime + Duration::from_secs(2),
                    "finite authority cannot be silently renewed"
                );
            }
            assert!(
                !process.finish().success(),
                "finite expiry/control EOF must terminate without renewal"
            );
        } else if scenario.injected_failure() {
            // The harness may inject failure only after a real bound packet
            // passed the actual production UDP owner into its pw-cat child.
            publish_marker(&runtime, ".capyio-fault-ready");
            process.event("failed");
            assert!(
                !process.finish().success(),
                "lost child/server cannot masquerade as successful playback"
            );
            if console_scenario.is_none() {
                assert!(
                    String::from_utf8_lossy(&process.stderr)
                        .contains("Linux lab playback launch failed")
                );
            }
        } else {
            // Let the real graph consume its final queued quantum before closing.
            thread::sleep(Duration::from_millis(150));
            process.command(json!({"version":1,"command":"stop"}));
            process.event("stopped");
            assert!(
                process.finish().success(),
                "owner must reap its actual pw-cat child"
            );
            assert!(process.stderr.is_empty());
        }
        if scenario.virtual_microphone() || console_scenario.is_some() {
            // Deliberately bypass the now-stopped Android lifecycle in this
            // adversarial fixture: correctly bound, fresh UDP after Stop still
            // cannot recreate a Source, port, link, child or ingress listener.
            for sequence in FRAMES..FRAMES + LATE_FRAMES {
                sender.send_packet(&packet(sequence, &binding)).unwrap();
                thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(sender.metrics().packets_sent, frames + LATE_FRAMES);
            publish_marker(&runtime, ".capyio-after-stop");
            while !exact_marker(&runtime.join(".capyio-stop-checked")) {
                assert!(
                    Instant::now() < deadline,
                    "bounded post-Stop graph assertion"
                );
                thread::sleep(Duration::from_millis(5));
            }
        }
        drop(sender);
        assert!(
            peer.receive_control_message_until(Instant::now() + BOUND)
                .is_err()
        );
        let AndroidPeer { link, mut runtime } = peer;
        runtime.close_control_link(link).unwrap();
        runtime.stop_audio_direction("microphone").unwrap();
        assert!(!runtime.audio_should_run(id, now_ms()));
        assert!(
            runtime.prepare_audio_endpoint(id, source.port()).is_err(),
            "audio job released"
        );
        let stopped = runtime.snapshot();
        assert!(stopped.port_leases.is_empty());
        assert!(
            stopped
                .sessions
                .iter()
                .all(|session| session.state == SessionState::Closed)
        );
        assert!(
            stopped
                .routes
                .iter()
                .all(|route| matches!(route.state, RouteState::Stopped | RouteState::Offline))
        );
        assert!(
            UdpSocket::bind(ingress).is_ok(),
            "production ingress released"
        );
        assert!(UdpSocket::bind(source).is_ok(), "synthetic Source released");
        assert!(Instant::now() < deadline);
        if let Some(name) = &console_scenario {
            console_marker(name, scenario.virtual_microphone());
        } else if scenario.virtual_microphone() {
            eprintln!(
                "capyio_pipewire_virtual_microphone=passed scenario={} authorized_udp=true stop=true fresh_post_stop=true child_reaped=true",
                scenario.name()
            );
        } else {
            eprintln!("capyio_pipewire_e2e=passed authorized_udp=true stop=true child_reaped=true");
        }
    }

    fn console_pending_scenario(name: &str) -> bool {
        matches!(
            name,
            "no_approval"
                | "pretyped"
                | "mismatched_request"
                | "mismatched_route"
                | "mismatched_peer"
        )
    }

    fn console_marker(name: &str, virtual_microphone: bool) {
        eprintln!(
            "capyio_audio_console=passed mode={} scenario={} actual_tty=true actual_node=true fresh_consent=true protected_read=true secret_redacted=true node_reaped=true",
            if virtual_microphone {
                "virtual_microphone"
            } else {
                "playback"
            },
            name
        );
    }

    fn finish_console_pending(
        name: &str,
        approval: &Value,
        process: &mut Process,
        peer: &mut AndroidPeer,
        runtime: &Path,
        deadline: Instant,
        virtual_microphone: bool,
    ) {
        #[cfg(feature = "paired-store")]
        {
            let request = approval["request_id"].as_str().unwrap();
            let route = approval["route_id"].as_str().unwrap();
            process.wait_console_prompt(request, route);
            process
                .console
                .as_ref()
                .unwrap()
                .assert_presentation(approval);
            assert!(peer.runtime.snapshot().routes.is_empty());
            assert!(peer.runtime.snapshot().port_leases.is_empty());
            let children = fs::read_to_string(format!(
                "/proc/{}/task/{}/children",
                process.owner_pid(),
                process.owner_pid()
            ))
            .unwrap();
            assert!(
                children.trim().is_empty(),
                "no helper before exact console consent"
            );
            if name.starts_with("mismatched_") {
                let console = process.console.as_mut().unwrap();
                let bad_request = MessageId::new().to_string();
                let bad_route = RouteId::new().to_string();
                let bad_peer = NodeId::new().to_string();
                console.send(
                    format!(
                        "start {} {} {}\n",
                        if name == "mismatched_request" {
                            &bad_request
                        } else {
                            request
                        },
                        if name == "mismatched_route" {
                            &bad_route
                        } else {
                            route
                        },
                        if name == "mismatched_peer" {
                            bad_peer
                        } else {
                            console.peer.to_string()
                        }
                    )
                    .as_bytes(),
                );
                assert!(
                    !process.finish().success(),
                    "mismatched exact confirmation rejected"
                );
            } else {
                // Pretyped input is observed still pending, then canceled. This
                // is not a full timeout and cannot be mistaken for approval.
                no_peer_media(
                    peer,
                    serde_json::from_value(approval["route_id"].clone()).unwrap(),
                );
                process.command(json!({"version":1,"command":"stop"}));
                process.event("stopped");
                assert!(process.finish().success());
            }
            assert!(
                peer.receive_control_message_until(Instant::now() + BOUND)
                    .is_err(),
                "rejected/unapproved console must close without a Route proposal"
            );
            assert!(peer.runtime.snapshot().routes.is_empty());
            assert!(peer.runtime.snapshot().port_leases.is_empty());
            publish_marker(runtime, ".capyio-after-stop");
            while !exact_marker(&runtime.join(".capyio-stop-checked")) {
                assert!(
                    Instant::now() < deadline,
                    "bounded negative graph observation"
                );
                thread::sleep(Duration::from_millis(5));
            }
            console_marker(name, virtual_microphone);
        }
        #[cfg(not(feature = "paired-store"))]
        {
            let _ = (
                name,
                approval,
                process,
                peer,
                runtime,
                deadline,
                virtual_microphone,
            );
            panic!("console graph requires paired-store build");
        }
    }

    #[test]
    fn pure_hosted_pipewire_requires_both_opt_ins_and_private_remote() {
        let enabled = Some(OsStr::new("1"));
        let private = Some(OsStr::new("capyio-test"));
        assert!(opted_in(enabled, enabled, private));
        assert!(!opted_in(None, enabled, private));
        assert!(!opted_in(enabled, None, private));
        assert!(!opted_in(enabled, enabled, Some(OsStr::new("pipewire-0"))));
        assert!(!opted_in(Some(OsStr::new("true")), enabled, private));
    }

    #[test]
    fn pure_hosted_virtual_microphone_scenarios_are_explicit_and_separate() {
        for value in ["waveform", "no_consumer", "child_exit", "server_disconnect"] {
            let scenario = Scenario::parse(value).unwrap();
            assert!(scenario.virtual_microphone());
            assert_eq!(scenario.name(), value);
        }
        for value in ["", "1", "playback", "WAVEFORM", "waveform ", "auto"] {
            assert!(Scenario::parse(value).is_none());
        }
        assert!(!Scenario::Playback.virtual_microphone());
        assert!(!Scenario::NoConsumer.injected_failure());
        assert!(Scenario::ChildExit.injected_failure());
        assert!(Scenario::ServerDisconnect.injected_failure());
    }

    #[test]
    fn pure_hosted_pipewire_signal_preserves_dynamic_binding_and_waveform() {
        let binding = AudioMediaStreamBinding {
            session_id: SessionId::new(),
            route_id: RouteId::new(),
            stream_id: StreamId::new(),
            stream_epoch: 7,
            selected_spec: AudioStreamSpec::voice_interactive(),
        };
        for sequence in [0, 199, 200, 299, 300, 499, 500, 519] {
            let packet = packet(sequence, &binding);
            packet.validate_against(&binding).unwrap();
            assert_eq!(packet.first_sample_index, sequence * 480);
            assert_eq!(packet.source_timestamp_micros, sequence * 10_000 + 1);
            let samples: Vec<_> = packet
                .payload
                .chunks_exact(2)
                .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
                .collect();
            if (200..300).contains(&sequence) {
                assert!(samples.iter().all(|sample| *sample == 0));
            } else {
                assert!(samples.iter().any(|sample| sample.abs() >= 8_000));
                let rising = samples
                    .windows(2)
                    .filter(|pair| pair[0] <= 0 && pair[1] > 0)
                    .count();
                assert_eq!(rising, if sequence < 200 { 10 } else { 20 });
            }
        }
    }
}

fn identities() -> (
    ControlCredentialBundle,
    ControlTlsClientIdentity,
    CertificateFingerprint,
) {
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
        KeyUsagePurpose,
    };
    let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];
    let key = KeyPair::generate().unwrap();
    let ca = params.self_signed(&key).unwrap().der().to_vec();
    let issuer = Issuer::new(params, key);
    let leaf = |name: &str, usage| {
        let mut params = CertificateParams::new(vec![name.into()]).unwrap();
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![usage];
        let key = KeyPair::generate().unwrap();
        (
            params.signed_by(&key, &issuer).unwrap().der().to_vec(),
            key.serialize_der(),
        )
    };
    let (server, server_key) = leaf("localhost", ExtendedKeyUsagePurpose::ServerAuth);
    let (client, client_key) = leaf("capyio-test-client", ExtendedKeyUsagePurpose::ClientAuth);
    let server_pin = CertificateFingerprint::from_certificate_der(&server).unwrap();
    let client_pin = CertificateFingerprint::from_certificate_der(&client).unwrap();
    (
        ControlCredentialBundle::new(vec![server, ca.clone()], server_key, ca.clone()).unwrap(),
        ControlTlsClientIdentity {
            expected_peer_certificate: server_pin,
            certificate_chain_der: vec![client, ca.clone()],
            private_key_der: client_key,
            server_ca_der: ca,
        },
        client_pin,
    )
}

fn hosted_paired_store() -> bool {
    match std::env::var_os("CAPYIO_AUDIO_PAIRED_STORE") {
        None => false,
        Some(value) if value == "1" => true,
        Some(_) => panic!("exact paired-store hosted opt-in required"),
    }
}

fn hosted_console_scenario() -> Option<String> {
    match (
        std::env::var_os("CAPYIO_AUDIO_CONSOLE"),
        std::env::var("CAPYIO_AUDIO_CONSOLE_SCENARIO"),
    ) {
        (None, Err(std::env::VarError::NotPresent)) => None,
        (Some(enabled), Ok(scenario))
            if enabled == "1"
                && matches!(
                    scenario.as_str(),
                    "waveform"
                        | "no_approval"
                        | "pretyped"
                        | "mismatched_request"
                        | "mismatched_route"
                        | "mismatched_peer"
                        | "expiry"
                        | "remote_eof"
                ) =>
        {
            Some(scenario)
        }
        _ => panic!("exact console hosted opt-in and scenario required"),
    }
}
