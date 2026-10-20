#![cfg(target_os = "linux")]
//! Hosted executable checks only: ephemeral certificates, loopback control and
//! inherited FIFO pipes. Never return a media offer or open a device/process.
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
use capyio_protocol::{NativeAudioEndpoint, NativeAudioPreset};
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
                    assert!(expiry > now_ms() && expiry <= now_ms() + 10_000);
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
