#![cfg(target_os = "linux")]
//! Linux-to-Android stereo executable checks: ephemeral certificates, loopback control and
//! inherited FIFO pipes. Ordinary tests never return a media offer or open a
//! capture process. The explicitly ignored graph case additionally requires the
//! hosted harness's private synthetic PipeWire graph and exact opt-in markers.
//! Android policy and shared Rust UDP receive are real; app consumption is simulated.
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
    NATIVE_AUDIO_INBOUND_DRAFT_METADATA_KEY, NativeAudioEndpoint, NativeAudioPreset,
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
    child: Child,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}
impl Process {
    fn start(args: &[&str]) -> Self {
        Self::start_with_env(args, &[])
    }
    fn start_with_env(args: &[&str], environment: &[(&str, &str)]) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_capyio-node"))
            .args(args)
            .env_remove("RUST_LOG")
            .envs(environment.iter().copied())
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
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }
    fn capture() -> Self {
        Self::start(&["linux-capture", "--trusted-lab"])
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
        self.frame(&serde_json::to_vec(&command).unwrap());
    }
    fn collect(&mut self) {
        drain(self.child.stdout.as_mut().unwrap(), &mut self.stdout);
        drain(self.child.stderr.as_mut().unwrap(), &mut self.stderr);
    }
    fn event(&mut self, expected: &str) -> Value {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
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
    fn finish(&mut self) -> ExitStatus {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
            if let Some(status) = self.child.try_wait().unwrap() {
                self.collect();
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
        let speaker = node
            .capabilities
            .values()
            .find(|cap| cap.class == CapabilityClass::Speaker)
            .unwrap();
        let port = speaker
            .ports
            .values()
            .find(|port| port.direction == PortDirection::Sink)
            .unwrap();
        let sink = PortRef {
            node_id: node.id,
            capability_id: speaker.id,
            port_id: port.id,
        };
        let local = NodeId::new();
        let (bundle, client, client_pin) = identities();
        let mut directory = PairedPeerDirectory::new(local).unwrap();
        directory
            .add(
                PairedPeerRecord::new(
                    sink.node_id,
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
            config: json!({"version":1,"node_id":local,"sink":sink,"listen":"127.0.0.1:0","media":"127.0.0.1:40211","target":"synthetic.test","duration_secs":10}),
            directory: directory.encode_json().unwrap(),
            bundle,
            client,
            runtime,
            nodes: ControlTlsNodeBinding::new(local, sink.node_id).unwrap(),
        }
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
                "Ephemeral capture".into(),
                // Sink capabilities publish automatically; this opt-in set is
                // exclusively for Source capabilities. No Android capture requested.
                &BTreeSet::new(),
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
                            "offer only after exact finite consent and Sink lease"
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
    let mut process = Process::capture();
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
    assert_linux_source(&peer, &approval);
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

#[test]
fn ipc_linux_capture_approved_finite_offer_status_and_stop() {
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
    assert_eq!(offer.endpoint, NativeAudioEndpoint::Source);
    assert_eq!(offer.preset, NativeAudioPreset::PcmS16le48000Stereo);
    assert_eq!(offer.port, 40211);
    process.command(json!({"version":1,"command":"status"}));
    let status = process.event("status");
    assert_eq!(status["route_id"], approval["route_id"]);
    assert_eq!(status["packets_sent"], 0);
    // No reciprocal offer: the media/PipeWire worker must never start.
    process.command(json!({"version":1,"command":"stop"}));
    process.event("stopped");
    assert!(process.finish().success());
    assert!(process.stderr.is_empty());
}

#[test]
fn ipc_linux_capture_android_denial_never_acquires_sink_or_offers() {
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
fn ipc_linux_capture_local_stop_while_android_consent_pending() {
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
fn ipc_linux_capture_wrong_nonce_or_route_rejected_without_offer() {
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
fn ipc_linux_capture_launcher_eof_cancels_before_authorization() {
    let (mut process, mut peer, _) = pending();
    drop(process.child.stdin.take());
    // EOF poisons the framed launcher channel, including writes. Verify bounded
    // termination via exit status rather than requiring an event on a closed channel.
    assert!(!process.finish().success());
    assert!(String::from_utf8_lossy(&process.stderr).contains("Linux lab capture launch failed"));
    assert!(
        peer.receive_control_message_until(Instant::now() + BOUND)
            .is_err(),
        "launcher EOF cannot authorize an offer"
    );
}

#[test]
fn ipc_linux_capture_blocked_stdout_exits_within_write_deadlines() {
    let fixture = Fixture::new();
    let mut process = Process::capture();
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
    assert!(String::from_utf8_lossy(&process.stderr).contains("Linux lab capture launch failed"));
}

#[test]
fn ipc_linux_capture_previous_invocation_approval_cannot_be_replayed() {
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
fn ipc_linux_capture_preplayed_approval_rejected_before_peer() {
    let fixture = Fixture::new();
    let mut process = Process::capture();
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
fn ipc_linux_capture_malformed_secret_bootstrap_is_redacted() {
    const MARKER: &str = "PRIVATE-TEST-MARKER-DO-NOT-ECHO";
    let fixture = Fixture::new();
    let mut process = Process::capture();
    process.command(fixture.config.clone());
    process.frame(&fixture.directory);
    process.frame(MARKER.as_bytes());
    let failed = process.event("failed");
    assert_eq!(failed, json!({"version":1,"event":"failed"}));
    assert!(!process.finish().success());
    assert!(!String::from_utf8_lossy(&process.stdout).contains(MARKER));
    let stderr = String::from_utf8_lossy(&process.stderr);
    assert!(!stderr.contains(MARKER));
    assert!(stderr.contains("Linux lab capture launch failed"));
    assert!(
        process.stdout.is_empty(),
        "no listener on invalid secret bootstrap"
    );
}

#[test]
fn ipc_linux_capture_requires_explicit_opt_in_and_default_remains_demo() {
    let mut missing = Process::start(&["linux-capture"]);
    assert!(!missing.finish().success());
    assert!(missing.stdout.is_empty());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--trusted-lab"));
    let mut accidental_secret = Process::start(&[
        "linux-capture",
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

/// Executed only by the private hosted harness, never an ordinary or local pass.
#[test]
#[ignore = "requires the dedicated hosted private-PipeWire stereo capture harness"]
fn hosted_pipewire_linux_capture_android_sink_end_to_end() {
    hosted_capture::run(false);
}

/// Separate actual Audio/Sink gate; retaining the selected-source graph is required.
#[test]
#[ignore = "requires the dedicated hosted private-PipeWire virtual speaker harness"]
fn hosted_pipewire_linux_virtual_speaker_android_sink_end_to_end() {
    hosted_capture::run(true);
}

mod hosted_capture {
    use super::*;
    use capyio_audio::{AudioMediaPacket, AudioMediaStreamBinding, AudioStreamSpec};
    use capyio_native_audio_lan::{
        NativeLanEndpointConfig, NativeLanError, NativeLanReceiveOutcome, NativeLanRouteBinding,
        NativeLanUdpEndpoint,
    };
    use capyio_protocol::RouteCommandAction;
    use std::{
        ffi::OsStr,
        fs::{self, OpenOptions},
        net::{Ipv4Addr, UdpSocket},
        os::unix::fs::{FileTypeExt, PermissionsExt},
        path::{Path, PathBuf},
    };

    const MARKER: &[u8] = b"capyio-hosted-pipewire-capture-v1\n";
    const SOURCE: &str = "capyio.test.stereo.source";
    const WHOLE_BUDGET: Duration = Duration::from_secs(22);
    const MAX_PCM: usize = 2 * 1024 * 1024;
    // Poison only the production Node, never the private server or synthetic app.
    // Python verifies these exact values in /proc/<owner>/environ and their
    // absence in the actual recorder, alongside retained private routing keys.
    const POISONED_ENV: [(&str, &str); 6] = [
        ("PIPEWIRE_NODE", "capyio.poisoned.node"),
        ("PIPEWIRE_AUTOCONNECT", "true"),
        (
            "PIPEWIRE_PROPS",
            r#"{"node.name":"capyio.poisoned.override","media.class":"Audio/Source","node.autoconnect":true,"node.virtual":false}"#,
        ),
        ("PIPEWIRE_QUANTUM", "128/44100"),
        ("PIPEWIRE_LATENCY", "128/44100"),
        ("PIPEWIRE_RATE", "1/44100"),
    ];

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Scenario {
        Waveform,
        ChildExit,
        ServerDisconnect,
        ControlDisconnect,
        Timeout,
        NoInput,
        Expiry,
    }
    impl Scenario {
        fn parse(value: &str) -> Option<Self> {
            match value {
                "waveform" => Some(Self::Waveform),
                "child_exit" => Some(Self::ChildExit),
                "server_disconnect" => Some(Self::ServerDisconnect),
                "control_disconnect" => Some(Self::ControlDisconnect),
                "timeout" => Some(Self::Timeout),
                "no_input" => Some(Self::NoInput),
                "expiry" => Some(Self::Expiry),
                _ => None,
            }
        }
        fn name(self) -> &'static str {
            match self {
                Self::Waveform => "waveform",
                Self::ChildExit => "child_exit",
                Self::ServerDisconnect => "server_disconnect",
                Self::ControlDisconnect => "control_disconnect",
                Self::Timeout => "timeout",
                Self::NoInput => "no_input",
                Self::Expiry => "expiry",
            }
        }
    }

    fn opted_in(test: Option<&OsStr>, capture: Option<&OsStr>, remote: Option<&OsStr>) -> bool {
        test == Some(OsStr::new("1"))
            && capture
                .and_then(OsStr::to_str)
                .and_then(Scenario::parse)
                .is_some()
            && remote == Some(OsStr::new("capyio-test"))
    }
    fn exact_marker(path: &Path) -> bool {
        let metadata = match fs::symlink_metadata(path) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return false,
            Err(error) => panic!("cannot inspect hosted marker: {error}"),
        };
        assert!(metadata.is_file(), "regular non-symlink marker required");
        assert_eq!(metadata.len(), MARKER.len() as u64);
        assert_eq!(fs::read(path).unwrap(), MARKER);
        true
    }
    fn private_runtime(virtual_speaker: bool) -> PathBuf {
        assert_eq!(
            std::env::var("CAPYIO_PIPEWIRE_ENDPOINT_MODE").as_deref(),
            Ok(if virtual_speaker {
                "virtual_speaker"
            } else {
                "selected_source"
            }),
            "graph endpoint mode must match the exact ignored fixture",
        );
        assert!(opted_in(
            std::env::var_os("CAPYIO_PIPEWIRE_TEST").as_deref(),
            std::env::var_os("CAPYIO_PIPEWIRE_CAPTURE").as_deref(),
            std::env::var_os("PIPEWIRE_REMOTE").as_deref(),
        ));
        let root = std::env::var_os("XDG_RUNTIME_DIR").expect("private runtime required");
        for key in [
            "PIPEWIRE_RUNTIME_DIR",
            "PIPEWIRE_CONFIG_DIR",
            "XDG_CONFIG_HOME",
            "HOME",
        ] {
            assert_eq!(std::env::var_os(key).as_ref(), Some(&root));
        }
        let root = PathBuf::from(root);
        let metadata = fs::symlink_metadata(&root).unwrap();
        assert!(root.is_absolute() && metadata.is_dir());
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
        assert!(exact_marker(&root.join(".capyio-pipewire-capture")));
        assert!(
            fs::symlink_metadata(root.join("capyio-test"))
                .unwrap()
                .file_type()
                .is_socket()
        );
        for name in [
            ".capyio-before-consent",
            ".capyio-consent-checked",
            ".capyio-before-android-consent",
            ".capyio-android-consent-checked",
            ".capyio-capture-owner.json",
            ".capyio-capture-owner-ready",
            ".capyio-links-ready",
            ".capyio-fault-ready",
            ".capyio-after-stop",
            ".capyio-stop-checked",
            "received-stereo.pcm",
        ] {
            assert!(
                !root.join(name).try_exists().unwrap(),
                "fresh private markers required"
            );
        }
        root
    }
    fn publish(root: &Path, name: &str) {
        let temporary = root.join(format!("{name}.tmp"));
        let destination = root.join(name);
        assert!(!destination.try_exists().unwrap());
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .unwrap();
        file.write_all(MARKER).unwrap();
        drop(file);
        fs::rename(temporary, destination).unwrap();
    }
    fn wait_marker(root: &Path, name: &str, deadline: Instant) {
        while !exact_marker(&root.join(name)) {
            assert!(
                Instant::now() < deadline,
                "bounded private graph checkpoint {name}"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn checkpoint(root: &Path, name: &str, ack: &str, deadline: Instant) {
        publish(root, name);
        wait_marker(root, ack, deadline);
    }
    fn no_media(peer: &mut AndroidPeer, process: &mut Process, id: RouteId, stage: &str) {
        process.command(json!({"version":1,"command":"status"}));
        assert_eq!(process.event("status")["stage"], stage);
        let until = Instant::now() + Duration::from_millis(150);
        while Instant::now() < until {
            assert!(
                peer.poll_control_message_until(Instant::now() + BOUND)
                    .unwrap()
                    .is_none()
            );
            assert!(peer.runtime.snapshot().port_leases.is_empty());
            assert!(!peer.runtime.audio_should_run(id, now_ms()));
            thread::sleep(Duration::from_millis(2));
        }
    }

    /// Deliberately simulated application boundary. The actual Android coordinator
    /// and shared Rust binding guard, not a fabricated Ready flag, gate every append.
    fn consume(
        host: &AndroidCatalogRuntimeV1,
        id: RouteId,
        guarded: &NativeLanRouteBinding,
        binding: &AudioMediaStreamBinding,
        packet: &AudioMediaPacket,
        pcm: &mut Vec<u8>,
    ) -> Result<(), String> {
        let snapshot = host.snapshot();
        let route = snapshot
            .routes
            .iter()
            .find(|route| route.id == id)
            .ok_or("route absent")?;
        let session = snapshot
            .sessions
            .iter()
            .find(|session| session.id == route.session_id)
            .ok_or("session absent")?;
        if !host.audio_should_run(id, now_ms()) {
            return Err("Android lifecycle is not running".into());
        }
        guarded
            .require_live_context(route, session, now_ms())
            .map_err(|e| e.to_string())?;
        packet
            .validate_against(binding)
            .map_err(|e| e.to_string())?;
        if pcm.len() + packet.payload.len() > MAX_PCM {
            return Err("bounded simulated PCM buffer exceeded".into());
        }
        pcm.extend_from_slice(&packet.payload);
        Ok(())
    }

    pub(super) fn run(virtual_speaker: bool) {
        let root = private_runtime(virtual_speaker); // Reject missing isolation before any IPC.
        let scenario = Scenario::parse(&std::env::var("CAPYIO_PIPEWIRE_CAPTURE").unwrap()).unwrap();
        assert!(
            scenario_allowed(virtual_speaker, scenario),
            "scenario must match endpoint mode"
        );
        let expires = matches!(scenario, Scenario::Timeout | Scenario::Expiry);
        let deadline = Instant::now() + WHOLE_BUDGET;
        let lifetime = Duration::from_secs(if expires { 5 } else { 30 });
        let reservation = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let source_address = reservation.local_addr().unwrap();
        let mut fixture = Fixture::new();
        fixture.config["media"] = json!(source_address.to_string());
        fixture.config["target"] = json!(SOURCE);
        if virtual_speaker {
            fixture.config.as_object_mut().unwrap().remove("target");
            fixture.config["endpoint_mode"] = json!("virtual_speaker");
        }
        fixture.config["duration_secs"] = json!(lifetime.as_secs());
        let mut process = if virtual_speaker {
            Process::start_with_env(&["linux-capture", "--trusted-lab"], &POISONED_ENV)
        } else {
            Process::capture()
        };
        fixture.bootstrap(&mut process);
        let mut peer = fixture.connect(&mut process);
        let approval = process.event("approval_required");
        let source: PortRef = serde_json::from_value(approval["source"].clone()).unwrap();
        let node_name = if virtual_speaker {
            assert_eq!(approval["endpoint_mode"], "virtual_speaker");
            assert!(approval["target"].is_null());
            let owned = format!("capyio.virtual-speaker.{}", source.port_id);
            assert_eq!(approval["node_name"], owned);
            owned
        } else {
            assert_eq!(approval["target"], SOURCE);
            SOURCE.to_owned()
        };
        assert_eq!(approval["format"], "pcm-s16le-48000-stereo");
        assert_eq!(approval["duration_secs"], lifetime.as_secs());
        let id: RouteId = serde_json::from_value(approval["route_id"].clone()).unwrap();
        assert_linux_source(&peer, &approval);
        no_media(&mut peer, &mut process, id, "awaiting_local_approval");
        checkpoint(
            &root,
            ".capyio-before-consent",
            ".capyio-consent-checked",
            deadline,
        );
        process.command(approve(&approval));
        peer.receive_proposal(id);
        no_media(
            &mut peer,
            &mut process,
            id,
            "awaiting_peer_consent_or_lease",
        );
        checkpoint(
            &root,
            ".capyio-before-android-consent",
            ".capyio-android-consent-checked",
            deadline,
        );
        peer.consent(id, true);
        let offered = peer.negotiate_to_offer_with_lifetime(id, lifetime);
        assert_eq!(
            process.event("awaiting_peer_offer")["route_id"],
            approval["route_id"]
        );
        let offer = *offered.native_audio_data.as_ref().unwrap().as_ref();
        assert_eq!(offer.endpoint, NativeAudioEndpoint::Source);
        assert_eq!(offer.preset, NativeAudioPreset::PcmS16le48000Stereo);
        assert_eq!(offer.port, source_address.port());
        assert_eq!(offered.route.state, RouteState::Starting);
        assert!(!peer.runtime.audio_should_run(id, now_ms()));
        assert_eq!(peer.runtime.snapshot().port_leases.len(), 1);
        // The reservation itself proves the Linux UDP sender has not been opened.
        assert_eq!(reservation.local_addr().unwrap(), source_address);
        let sink_socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let sink_address = sink_socket.local_addr().unwrap();
        peer.runtime
            .prepare_audio_endpoint(id, sink_address.port())
            .unwrap();
        assert!(!peer.runtime.audio_should_run(id, now_ms()));
        drop(reservation);
        peer.runtime
            .publish_audio_control(&mut peer.link, now_ms())
            .unwrap();
        let ReceivedControlMessage::RouteCommand(start) =
            peer.receive_control_message_until(deadline).unwrap()
        else {
            panic!("actual Linux owner Start required")
        };
        assert_eq!(start.route_id, id);
        assert_eq!(start.action, RouteCommandAction::Start);
        let started = peer
            .runtime
            .stage_audio_route_command(&peer.link, &start, now_ms())
            .unwrap();
        assert_eq!(started.state, RouteState::Starting);
        assert!(peer.runtime.audio_should_run(id, now_ms()));
        peer.link.send_route_status(&started).unwrap();
        let ReceivedControlMessage::RouteStatus(ready) =
            peer.receive_control_message_until(deadline).unwrap()
        else {
            panic!("actual capture worker readiness status required")
        };
        assert_eq!(ready.route.state, RouteState::Active);
        peer.runtime
            .receive_audio_route_status(&peer.link, &ready, "127.0.0.1", now_ms())
            .unwrap();
        let snapshot = peer.runtime.snapshot();
        let route = snapshot.routes.iter().find(|route| route.id == id).unwrap();
        let session = snapshot
            .sessions
            .iter()
            .find(|session| session.id == route.session_id)
            .unwrap();
        let binding = AudioMediaStreamBinding {
            session_id: route.session_id,
            route_id: id,
            stream_id: offer.stream_id,
            stream_epoch: u32::try_from(route.epoch).unwrap(),
            selected_spec: AudioStreamSpec::media_balanced(),
        };
        let guarded =
            NativeLanRouteBinding::from_live_context(binding.clone(), route, session, now_ms())
                .unwrap();
        let mut receiver = NativeLanUdpEndpoint::from_socket(
            sink_socket,
            NativeLanEndpointConfig {
                peer: source_address,
                read_timeout: Duration::from_millis(10),
                inflight_packet_capacity: 4,
            },
            binding.clone(),
        )
        .unwrap();
        let owner_path = root.join(".capyio-capture-owner.json");
        let owner = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(owner_path)
            .unwrap();
        let ownership = if virtual_speaker {
            json!({"owner_pid":process.child.id(),"source_port_id":source.port_id,
                   "node_name":node_name,"endpoint_mode":"virtual_speaker"})
        } else {
            json!({"owner_pid":process.child.id()})
        };
        serde_json::to_writer(owner, &ownership).unwrap();
        publish(&root, ".capyio-capture-owner-ready");
        wait_marker(&root, ".capyio-links-ready", deadline);
        let mut pcm = Vec::new();
        let mut last: Option<AudioMediaPacket> = None;
        let mut packets = 0_u64;
        // An unlinked real Sink must remain stoppable without ever producing a
        // packet. No synthetic PCM is supplied to its stdout or sender.
        if scenario == Scenario::NoInput {
            let observe = Instant::now() + Duration::from_millis(350);
            while Instant::now() < observe {
                assert!(
                    matches!(
                        receiver.receive(),
                        Ok(NativeLanReceiveOutcome::Pending) | Err(NativeLanError::ReceiveTimeout)
                    ),
                    "no packet may originate from an unlinked virtual speaker"
                );
                assert!(peer.runtime.audio_should_run(id, now_ms()));
                assert!(process.child.try_wait().unwrap().is_none());
            }
        }
        while scenario != Scenario::NoInput
            && (expires
                || packets
                    < if scenario == Scenario::Waveform {
                        650
                    } else {
                        1
                    })
        {
            assert!(Instant::now() < deadline, "bounded stereo receive");
            if expires && !peer.runtime.audio_should_run(id, now_ms()) {
                break;
            }
            match receiver.receive() {
                Ok(NativeLanReceiveOutcome::Packet(packet)) => {
                    assert_eq!(
                        packet.sequence, packets,
                        "no hidden missing/repeated packet"
                    );
                    assert_eq!(packet.first_sample_index, packets * 480);
                    assert_eq!(packet.source_timestamp_micros, packets * 10_000 + 1);
                    assert_eq!(packet.sample_count, 480);
                    assert_eq!(packet.payload.len(), 480 * 2 * 2);
                    assert!(!packet.discontinuity);
                    if let Some(previous) = &last {
                        assert!(packet.source_timestamp_micros > previous.source_timestamp_micros);
                    }
                    if let Err(error) =
                        consume(&peer.runtime, id, &guarded, &binding, &packet, &mut pcm)
                    {
                        assert!(expires, "unexpected consumption rejection: {error}");
                        break;
                    }
                    last = Some(packet);
                    packets += 1;
                }
                Ok(NativeLanReceiveOutcome::Pending) | Err(NativeLanError::ReceiveTimeout) => {}
                other => panic!("exact stereo datagram admission failed: {other:?}"),
            }
        }
        assert!(
            scenario == Scenario::NoInput || packets > 0,
            "actual production graph/packetizer traffic required outside no-input Stop"
        );
        if !expires {
            process.command(json!({"version":1,"command":"status"}));
            let status = process.event("status");
            assert_eq!(status["state"], "active");
            assert!(status["packets_sent"].as_u64().unwrap() >= packets);
            if scenario == Scenario::NoInput {
                assert_eq!(status["packets_sent"], 0);
                assert!(pcm.is_empty());
            }
        }
        if matches!(scenario, Scenario::ChildExit | Scenario::ServerDisconnect) {
            publish(&root, ".capyio-fault-ready");
        }
        let AndroidPeer { link, mut runtime } = peer;
        let link = if scenario == Scenario::ControlDisconnect {
            runtime.close_control_link(link).unwrap();
            None
        } else {
            Some(link)
        };
        if scenario == Scenario::Waveform {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(root.join("received-stereo.pcm"))
                .unwrap();
            file.write_all(&pcm).unwrap();
        }
        if matches!(scenario, Scenario::Waveform | Scenario::NoInput) {
            process.command(json!({"version":1,"command":"stop"}));
            process.event("stopped");
            assert!(process.finish().success());
            assert!(process.stderr.is_empty());
        } else {
            process.event("failed");
            assert!(!process.finish().success());
            assert!(
                String::from_utf8_lossy(&process.stderr)
                    .contains("Linux lab capture launch failed")
            );
        }
        if let Some(link) = link {
            runtime.close_control_link(link).unwrap();
        }
        runtime.stop_audio_direction("speaker").unwrap();
        assert!(!runtime.audio_should_run(id, now_ms()));
        assert!(runtime.snapshot().port_leases.is_empty());
        assert!(
            runtime
                .snapshot()
                .routes
                .iter()
                .all(|route| matches!(route.state, RouteState::Stopped | RouteState::Offline))
        );
        assert!(
            runtime
                .snapshot()
                .sessions
                .iter()
                .all(|session| session.state == SessionState::Closed)
        );
        assert!(
            runtime
                .prepare_audio_endpoint(id, sink_address.port())
                .is_err()
        );
        // Drain already received traffic before sending NEW correctly bound traffic
        // from the released exact source address. Neither replay nor missing payload
        // can stand in for the post-Stop lifecycle rejection assertion.
        while !matches!(receiver.receive(), Err(NativeLanError::ReceiveTimeout)) {
            assert!(Instant::now() < deadline);
        }
        let mut adversary = NativeLanUdpEndpoint::bind(
            source_address,
            NativeLanEndpointConfig {
                peer: sink_address,
                read_timeout: Duration::from_millis(10),
                inflight_packet_capacity: 2,
            },
            binding.clone(),
        )
        .unwrap();
        let mut fresh = last.unwrap_or_else(|| {
            assert_eq!(scenario, Scenario::NoInput);
            // Deliberately NEW post-Stop traffic, not capture evidence. It must
            // pass wire admission and be rejected by actual Android lifecycle.
            AudioMediaPacket {
                stream_id: binding.stream_id,
                stream_epoch: binding.stream_epoch,
                sequence: 0,
                source_timestamp_micros: 1,
                first_sample_index: 0,
                sample_count: 480,
                discontinuity: false,
                payload: vec![0; 480 * 2 * 2],
            }
        });
        fresh.sequence += 1000;
        fresh.first_sample_index = fresh.sequence * 480;
        fresh.source_timestamp_micros += 10_000_000;
        fresh.validate_against(&binding).unwrap();
        adversary.send_packet(&fresh).unwrap();
        let before = pcm.len();
        loop {
            assert!(Instant::now() < deadline, "bounded fresh post-Stop packet");
            match receiver.receive().unwrap() {
                NativeLanReceiveOutcome::Pending => {}
                NativeLanReceiveOutcome::Packet(packet) => {
                    assert_eq!(packet, fresh);
                    assert!(consume(&runtime, id, &guarded, &binding, &packet, &mut pcm).is_err());
                    break;
                }
                other => panic!("fresh packet must be syntactically valid: {other:?}"),
            }
        }
        assert_eq!(pcm.len(), before, "no simulated app consumption after Stop");
        drop(adversary);
        drop(receiver);
        assert!(
            UdpSocket::bind(source_address).is_ok(),
            "Linux sender address released"
        );
        assert!(
            UdpSocket::bind(sink_address).is_ok(),
            "simulated Android socket released"
        );
        checkpoint(
            &root,
            ".capyio-after-stop",
            ".capyio-stop-checked",
            deadline,
        );
        eprintln!(
            "{}=passed scenario={} android_policy=true shared_rust_receiver=true simulated_app=true fresh_post_stop=true",
            if virtual_speaker {
                "capyio_virtual_speaker_fixture"
            } else {
                "capyio_capture_fixture"
            },
            scenario.name()
        );
    }

    fn scenario_allowed(virtual_speaker: bool, scenario: Scenario) -> bool {
        if virtual_speaker {
            scenario != Scenario::Timeout
        } else {
            !matches!(scenario, Scenario::NoInput | Scenario::Expiry)
        }
    }

    #[test]
    fn pure_virtual_speaker_scenarios_and_all_six_environment_overrides_are_explicit() {
        for name in [
            "waveform",
            "no_input",
            "child_exit",
            "server_disconnect",
            "control_disconnect",
            "expiry",
        ] {
            let scenario = Scenario::parse(name).unwrap();
            assert_eq!(scenario.name(), name);
            assert!(scenario_allowed(true, scenario));
        }
        assert!(!scenario_allowed(true, Scenario::Timeout));
        assert!(!scenario_allowed(false, Scenario::NoInput));
        assert!(!scenario_allowed(false, Scenario::Expiry));
        assert_eq!(POISONED_ENV.len(), 6);
        assert_eq!(
            POISONED_ENV
                .iter()
                .map(|(key, _)| *key)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "PIPEWIRE_NODE",
                "PIPEWIRE_AUTOCONNECT",
                "PIPEWIRE_PROPS",
                "PIPEWIRE_QUANTUM",
                "PIPEWIRE_LATENCY",
                "PIPEWIRE_RATE"
            ])
        );
        assert!(POISONED_ENV.iter().all(|(_, value)| !value.is_empty()));
    }

    #[test]
    fn pure_capture_harness_requires_private_remote_and_exact_scenario() {
        for name in [
            "waveform",
            "child_exit",
            "server_disconnect",
            "control_disconnect",
            "timeout",
        ] {
            assert_eq!(Scenario::parse(name).unwrap().name(), name);
            assert!(opted_in(
                Some(OsStr::new("1")),
                Some(OsStr::new(name)),
                Some(OsStr::new("capyio-test"))
            ));
        }
        for name in ["", "1", "auto", "WAVEFORM", "waveform "] {
            assert!(Scenario::parse(name).is_none());
        }
        assert!(!opted_in(
            None,
            Some(OsStr::new("waveform")),
            Some(OsStr::new("capyio-test"))
        ));
        assert!(!opted_in(
            Some(OsStr::new("1")),
            Some(OsStr::new("waveform")),
            Some(OsStr::new("pipewire-0"))
        ));
    }
}

fn assert_linux_source(peer: &AndroidPeer, approval: &Value) {
    let source: PortRef = serde_json::from_value(approval["source"].clone()).unwrap();
    let sink: PortRef = serde_json::from_value(approval["sink"].clone()).unwrap();
    let snapshot = peer.runtime.snapshot();
    assert_eq!(snapshot.peers.len(), 1);
    let node = &snapshot.peers[0];
    assert_eq!(node.platform, Platform::Linux);
    assert_eq!(source.node_id, node.id);
    assert_eq!(sink.node_id, snapshot.local_node.id);
    assert_eq!(node.capabilities.len(), 1);
    let capability = node.capabilities.get(&source.capability_id).unwrap();
    assert_eq!(capability.class, CapabilityClass::Speaker);
    assert_eq!(capability.ports.len(), 1);
    let port = capability.ports.get(&source.port_id).unwrap();
    assert_eq!(port.direction, PortDirection::Source);
    assert_eq!(port.profile, ProfileId::audio_frames_v1());
    assert_eq!(
        port.formats,
        vec![FormatDescriptor::new("pcm-s16le-48000-stereo")]
    );
    assert!(
        !capability
            .metadata
            .contains_key(NATIVE_AUDIO_INBOUND_DRAFT_METADATA_KEY)
    );
}

#[test]
fn ipc_linux_capture_malformed_and_truncated_framing_fails_closed() {
    for bytes in [
        vec![0, 0, 0, 0],
        vec![255, 255, 255, 255],
        vec![0, 0, 0, 10, b'{'],
    ] {
        let mut process = Process::capture();
        process
            .child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(&bytes)
            .unwrap();
        drop(process.child.stdin.take());
        assert!(!process.finish().success());
        assert!(
            process.stdout.is_empty(),
            "invalid FIFO frame cannot create a listener"
        );
        assert!(
            String::from_utf8_lossy(&process.stderr).contains("Linux lab capture launch failed")
        );
    }
}

#[test]
fn ipc_linux_capture_no_reciprocal_offer_expires_without_media() {
    let mut fixture = Fixture::new();
    fixture.config["duration_secs"] = json!(3);
    let mut process = Process::capture();
    fixture.bootstrap(&mut process);
    let mut peer = fixture.connect(&mut process);
    let approval = process.event("approval_required");
    process.command(approve(&approval));
    let id = serde_json::from_value(approval["route_id"].clone()).unwrap();
    peer.receive_proposal(id);
    peer.consent(id, true);
    peer.negotiate_to_offer_with_lifetime(id, Duration::from_secs(3));
    process.event("awaiting_peer_offer");
    process.event("failed");
    assert!(!process.finish().success());
    assert!(!peer.runtime.audio_should_run(id, now_ms()));
    let AndroidPeer { link, mut runtime } = peer;
    runtime.close_control_link(link).unwrap();
    assert!(runtime.snapshot().port_leases.is_empty());
}

#[test]
fn ipc_linux_capture_insufficient_lease_headroom_fails_before_offer() {
    let mut fixture = Fixture::new();
    fixture.config["duration_secs"] = json!(1);
    let mut process = Process::capture();
    fixture.bootstrap(&mut process);
    let mut peer = fixture.connect(&mut process);
    let approval = process.event("approval_required");
    process.command(approve(&approval));
    let id = serde_json::from_value(approval["route_id"].clone()).unwrap();
    peer.receive_proposal(id);
    peer.consent(id, true);
    let deadline = Instant::now() + BOUND;
    let mut finite_grant = false;
    loop {
        match peer.link.receive_control_message_until(deadline) {
            Ok(ReceivedControlMessage::RouteStatus(status)) => {
                assert_eq!(status.route.id, id);
                assert!(status.native_audio_data.is_none());
                assert_eq!(status.route.state, RouteState::Draft);
                let route = peer
                    .runtime
                    .receive_audio_route_status(&peer.link, &status, "127.0.0.1", now_ms())
                    .unwrap();
                let AuthorizationState::Authorized {
                    expires_at_ms: Some(expiry),
                } = route.authorization
                else {
                    panic!("finite grant required");
                };
                assert!(expiry <= now_ms() + 1000);
                finite_grant = true;
                peer.link.send_route_status(&route).unwrap();
            }
            Ok(_) => panic!("insufficient grant must not acquire lease or offer media"),
            Err(_) => break,
        }
    }
    assert!(finite_grant);
    process.event("failed");
    assert!(!process.finish().success());
    assert!(peer.runtime.snapshot().port_leases.is_empty());
    assert!(!peer.runtime.audio_should_run(id, now_ms()));
    let AndroidPeer { link, mut runtime } = peer;
    runtime.close_control_link(link).unwrap();
    assert!(runtime.snapshot().port_leases.is_empty());
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
