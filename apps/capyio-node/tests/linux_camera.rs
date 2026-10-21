#![cfg(target_os = "linux")]
//! Actual executable boundary: private inherited bootstrap, paired control,
//! Android Rust consent, dynamic mTLS camera route, and optional native graph.
//! No physical camera, phone, persistent enrollment, or capture is used.
use capyio_android_jni::camera_sender::{CameraSourceConsent, SecureCameraSender};
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
use capyio_protocol::{RouteCommandAction, SecureCameraDataOffer};
use capyio_runtime::NodeRuntime;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io::{self, Read, Write},
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
    fn camera() -> Self {
        Self::start(&["linux-camera", "--trusted-lab"])
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
    fn next_event(&mut self) -> Value {
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
                    return value;
                }
            }
            assert!(
                Instant::now() < deadline,
                "missing bounded local event; stderr={}",
                String::from_utf8_lossy(&self.stderr)
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn event(&mut self, expected: &str) -> Value {
        let value = self.next_event();
        assert_eq!(value["event"], expected);
        value
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
fn no_children(process: &Process) {
    let pid = process.child.id();
    // Read-only /proc scan also works on kernels without task/<pid>/children.
    for entry in std::fs::read_dir("/proc").unwrap().flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|b| b.is_ascii_digit())
        {
            continue;
        }
        let Ok(status) = std::fs::read_to_string(entry.path().join("status")) else {
            continue;
        };
        let parent = status
            .lines()
            .find_map(|line| line.strip_prefix("PPid:"))
            .and_then(|s| s.trim().parse::<u32>().ok());
        assert_ne!(
            parent,
            Some(pid),
            "no media helper before both grants and authenticated binding"
        );
    }
}
fn approval_command(approval: &Value) -> Value {
    json!({"version":1,"command":"approve","request_id":approval["request_id"],"route_id":approval["route_id"]})
}
struct Fixture {
    config: Value,
    directory: Vec<u8>,
    credentials: ControlCredentialBundle,
    client: ControlTlsClientIdentity,
    runtime: NodeRuntime,
    source: PortRef,
}
impl Fixture {
    fn new() -> Self {
        // Same synthetic descriptor preparation as Android camera_job tests;
        // authorization and leases are ALWAYS production Android Rust policy.
        let mut android = capyio_testkit::android_node();
        for adapter in android.adapter_instances.values_mut() {
            adapter
                .supported_route_modes
                .insert(RouteBackend::AdapterManaged);
        }
        let cap = android
            .capabilities
            .values_mut()
            .find(|c| c.class == CapabilityClass::Camera)
            .unwrap();
        cap.availability = Availability::Available;
        let port = cap
            .ports
            .values_mut()
            .find(|p| p.direction == PortDirection::Source)
            .unwrap();
        port.availability = Availability::Available;
        port.interoperability_mode = InteroperabilityMode::AdapterManaged;
        port.formats = vec![FormatDescriptor::new("cavc-avc-access-unit-v1")];
        port.qos_modes.insert(QosMode::Interactive);
        let source = PortRef {
            node_id: android.id,
            capability_id: cap.id,
            port_id: port.id,
        };
        let local = NodeId::new();
        let (credentials, client, client_pin) = identities();
        let mut directory = PairedPeerDirectory::new(local).unwrap();
        directory
            .add(
                PairedPeerRecord::new(
                    source.node_id,
                    "Synthetic Android",
                    "capyio-test-client",
                    CredentialHandle::new("ephemeral.test.only").unwrap(),
                    client_pin,
                    1,
                )
                .unwrap(),
            )
            .unwrap();
        Self {
            config: json!({"version":1,"node_id":local,"source":source,"listen":"127.0.0.1:0","duration_secs":10}),
            directory: directory.encode_json().unwrap(),
            credentials,
            client,
            runtime: NodeRuntime::new(android).unwrap(),
            source,
        }
    }
    fn bootstrap(&self, process: &mut Process) {
        process.command(self.config.clone());
        process.frame(&self.directory);
        let mut encoded = self.credentials.encode().unwrap();
        process.frame(&encoded);
        encoded.fill(0);
    }
    fn connect(mut self, process: &mut Process) -> Peer {
        let event = process.event("listening");
        let nodes = ControlTlsNodeBinding::new(
            serde_json::from_value(self.config["node_id"].clone()).unwrap(),
            self.source.node_id,
        )
        .unwrap();
        let mut directory = PairedPeerDirectory::new(self.source.node_id).unwrap();
        directory
            .add(
                PairedPeerRecord::new(
                    nodes.server_node_id,
                    "Synthetic Linux",
                    "localhost",
                    CredentialHandle::new("ephemeral.test.only").unwrap(),
                    self.client.expected_peer_certificate,
                    1,
                )
                .unwrap(),
            )
            .unwrap();
        let credentials = ControlCredentialBundle::new(
            self.client.certificate_chain_der.clone(),
            self.client.private_key_der.clone(),
            self.client.server_ca_der.clone(),
        )
        .unwrap()
        .encode()
        .unwrap();
        let client = AuthenticatedControlClientChannel::connect_fresh_session(
            ControlTlsEndpoint {
                address: event["address"].as_str().unwrap().parse().unwrap(),
                io_timeout: BOUND,
            },
            "localhost",
            self.client,
            nodes,
        )
        .unwrap();
        let mut link = CatalogControlLink::admit_dialed_new_peer(
            &mut self.runtime,
            client,
            "Synthetic Linux".into(),
        )
        .unwrap();
        let catalog = link
            .prepare_catalog(&self.runtime, &BTreeSet::from([self.source.capability_id]))
            .unwrap();
        link.send_catalog(catalog).unwrap();
        let catalog = link.receive_catalog().unwrap();
        link.apply_catalog(&mut self.runtime, catalog).unwrap();
        Peer {
            runtime: self.runtime,
            link,
            consent: CameraSourceConsent::default(),
            directory: directory.encode_json().unwrap(),
            credentials,
        }
    }
}
struct Peer {
    runtime: NodeRuntime,
    link: CatalogControlLink,
    consent: CameraSourceConsent,
    directory: Vec<u8>,
    credentials: Vec<u8>,
}
impl Peer {
    fn proposal(&mut self, id: RouteId) {
        let ReceivedControlMessage::RouteStatus(status) = self
            .link
            .receive_control_message_until(Instant::now() + BOUND)
            .unwrap()
        else {
            panic!("Draft required")
        };
        assert_eq!(status.route.id, id);
        assert_eq!(status.route.state, RouteState::Draft);
        assert_eq!(status.route.authorization, AuthorizationState::Pending);
        assert!(status.secure_camera_data.is_none());
        self.link
            .apply_route_status(&mut self.runtime, &status, false)
            .unwrap();
        self.consent
            .enqueue(&mut self.runtime, id, now_ms())
            .unwrap();
        assert!(self.runtime.snapshot().port_leases.is_empty());
        let source = self.runtime.route(id).unwrap().source;
        assert!(
            self.runtime
                .acquire_route_port_lease(id, source, now_ms() + 5000, now_ms())
                .is_err()
        );
        self.link
            .send_route_status(self.runtime.route(id).unwrap())
            .unwrap();
    }
    fn grant(&mut self, id: RouteId, allow: bool, ttl: u64) {
        let now = now_ms();
        self.consent
            .resolve(&mut self.runtime, id, allow, now, now + ttl)
            .unwrap();
        self.link
            .send_route_status(self.runtime.route(id).unwrap())
            .unwrap();
    }
    fn offer(&mut self, id: RouteId) -> SecureCameraDataOffer {
        let mut narrowed = false;
        let mut leased = false;
        loop {
            match self
                .link
                .receive_control_message_until(Instant::now() + BOUND)
                .unwrap()
            {
                ReceivedControlMessage::RouteStatus(status) => {
                    assert_eq!(status.route.id, id);
                    if let Some(offer) = status.secure_camera_data {
                        assert!(narrowed && leased);
                        assert_eq!(status.route.state, RouteState::Starting);
                        assert!(offer.port > 0);
                        self.runtime.begin_route_start(id, now_ms()).unwrap();
                        assert_eq!(self.runtime.route(id).unwrap().epoch, status.route.epoch);
                        return offer;
                    }
                    assert_eq!(status.route.state, RouteState::Draft);
                    let AuthorizationState::Authorized {
                        expires_at_ms: Some(expiry),
                    } = status.route.authorization
                    else {
                        panic!("finite narrowing required")
                    };
                    self.runtime
                        .restrict_draft_authorization(id, expiry, now_ms())
                        .unwrap();
                    assert_eq!(
                        self.runtime.route(id).unwrap().authorization,
                        status.route.authorization
                    );
                    self.link
                        .send_route_status(self.runtime.route(id).unwrap())
                        .unwrap();
                    narrowed = true;
                }
                ReceivedControlMessage::PortLeaseRequest(request) => {
                    assert!(narrowed && !leased);
                    self.link
                        .answer_port_lease_request(&mut self.runtime, request, now_ms())
                        .unwrap();
                    self.runtime
                        .prepare_route(
                            id,
                            Some(FormatDescriptor::new("cavc-avc-access-unit-v1")),
                            QosMode::Interactive,
                            now_ms(),
                        )
                        .unwrap();
                    leased = true;
                }
                _ => panic!("unexpected negotiation message"),
            }
        }
    }
    fn idle(&mut self, process: &Process) {
        let deadline = Instant::now() + Duration::from_millis(100);
        while Instant::now() < deadline {
            no_children(process);
            assert!(
                self.link
                    .poll_control_message_until(Instant::now() + BOUND)
                    .unwrap()
                    .is_none(),
                "no lease/offer before independent consent"
            );
            assert!(self.runtime.snapshot().port_leases.is_empty());
            thread::sleep(Duration::from_millis(2));
        }
    }
}
fn pending() -> (Process, Peer, Value) {
    let fixture = Fixture::new();
    let mut process = Process::camera();
    fixture.bootstrap(&mut process);
    let mut peer = fixture.connect(&mut process);
    let approval = process.event("approval_required");
    assert_eq!(approval["media_security"], "tls13_mtls_v1");
    assert_eq!(approval["format"], "cavc-avc-access-unit-v1");
    peer.idle(&process);
    (process, peer, approval)
}
fn failed(process: &mut Process) {
    process.event("failed");
    assert_eq!(process.finish().code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&process.stderr).trim(),
        "Error: Linux camera launch failed"
    );
}
#[test]
fn ipc_linux_camera_requires_exact_approval_and_independent_android_consent() {
    let (mut process, mut peer, approval) = pending();
    process.command(approval_command(&approval));
    let id = serde_json::from_value(approval["route_id"].clone()).unwrap();
    peer.proposal(id);
    peer.idle(&process);
    process.command(json!({"version":1,"command":"stop"}));
    process.event("stopped");
    assert!(process.finish().success());
}
#[test]
fn ipc_linux_camera_wrong_request_or_route_fails_closed() {
    for field in ["request_id", "route_id"] {
        let (mut process, mut peer, approval) = pending();
        let mut command = approval_command(&approval);
        command[field] = json!(MessageId::new());
        no_children(&process);
        process.command(command);
        failed(&mut process);
        assert!(
            peer.link
                .receive_control_message_until(Instant::now() + BOUND)
                .is_err()
        );
    }
}
#[test]
fn ipc_linux_camera_android_denial_fails_before_media_children() {
    let (mut process, mut peer, approval) = pending();
    process.command(approval_command(&approval));
    let id = serde_json::from_value(approval["route_id"].clone()).unwrap();
    peer.proposal(id);
    no_children(&process);
    peer.grant(id, false, 5000);
    failed(&mut process);
    assert!(peer.runtime.snapshot().port_leases.is_empty());
}
#[test]
fn ipc_linux_camera_bad_config_and_source_fail_closed() {
    for (field, value) in [
        ("version", json!(2)),
        ("duration_secs", json!(0)),
        ("duration_secs", json!(61)),
        ("listen", json!("0.0.0.0:0")),
        ("media", json!("127.0.0.1:9876")),
        ("helper", json!("/tmp/not-trusted")),
        (
            "source",
            json!({"node_id":NodeId::new(),"capability_id":CapabilityId::new(),"port_id":PortId::new()}),
        ),
    ] {
        let mut fixture = Fixture::new();
        fixture.config[field] = value;
        let mut process = Process::camera();
        // Config errors can close input immediately. Write all three in one syscall.
        let mut bytes = Vec::new();
        for frame in [
            serde_json::to_vec(&fixture.config).unwrap(),
            fixture.directory.clone(),
            fixture.credentials.encode().unwrap(),
        ] {
            bytes.extend_from_slice(&(frame.len() as u32).to_be_bytes());
            bytes.extend(frame);
        }
        process
            .child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(&bytes)
            .unwrap();
        bytes.fill(0);
        failed(&mut process);
    }
}
#[test]
fn ipc_linux_camera_dynamic_offer_still_has_no_media_children_before_binding() {
    let (mut process, mut peer, approval) = pending();
    process.command(approval_command(&approval));
    let id = serde_json::from_value(approval["route_id"].clone()).unwrap();
    peer.proposal(id);
    peer.grant(id, true, 5000);
    let offer = peer.offer(id);
    assert_ne!(offer.port, 0);
    no_children(&process);
    process.command(json!({"version":1,"command":"status"}));
    let status = process.event("status");
    assert!(status["helper_pid"].is_null() && status["decoder_pid"].is_null());
    assert_eq!(status["consumer_delivery_verified"], false);
    process.command(json!({"version":1,"command":"stop"}));
    process.event("stopped");
    assert!(process.finish().success());
}

#[test]
fn ipc_linux_camera_preapproval_eof_is_terminal() {
    let (mut process, mut peer, _) = pending();
    no_children(&process);
    drop(process.child.stdin.take());
    assert_eq!(process.finish().code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&process.stderr).trim(),
        "Error: Linux camera launch failed"
    );
    assert!(
        peer.link
            .receive_control_message_until(Instant::now() + BOUND)
            .is_err()
    );
}
#[test]
fn ipc_linux_camera_partial_bootstrap_has_whole_frame_deadline() {
    let mut process = Process::camera();
    process
        .child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(&[0, 0, 0, 12, b'{'])
        .unwrap();
    // Launcher bootstrap deadline is intentionally larger than ordinary operation bounds.
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        no_children(&process);
        process.collect();
        if process.child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "partial inherited record did not expire"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(process.finish().code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&process.stderr).trim(),
        "Error: Linux camera launch failed"
    );
}
#[test]
fn ipc_linux_camera_wrong_certificate_pin_fails_without_children() {
    let mut fixture = Fixture::new();
    fixture.client.expected_peer_certificate = identities().1.expected_peer_certificate;
    let mut process = Process::camera();
    fixture.bootstrap(&mut process);
    let listening = process.event("listening");
    no_children(&process);
    let nodes = ControlTlsNodeBinding::new(
        serde_json::from_value(fixture.config["node_id"].clone()).unwrap(),
        fixture.source.node_id,
    )
    .unwrap();
    assert!(
        AuthenticatedControlClientChannel::connect_fresh_session(
            ControlTlsEndpoint {
                address: listening["address"].as_str().unwrap().parse().unwrap(),
                io_timeout: BOUND
            },
            "localhost",
            fixture.client,
            nodes
        )
        .is_err()
    );
    failed(&mut process);
}

fn fixture_records(
    stream: capyio_vcamdroid_adapter::AvcStreamKey,
) -> (Vec<u8>, Vec<capyio_vcamdroid_adapter::AvcAccessUnit>) {
    use capyio_vcamdroid_adapter::{AvcRecord, decode_record, encode_config};
    let bytes = include_bytes!("../../../fixtures/linux-avc/solid-720p30.cavc");
    let mut pos = 0;
    let mut config = None;
    let mut units = Vec::new();
    while pos < bytes.len() {
        let length =
            56 + u32::from_be_bytes(bytes[pos + 52..pos + 56].try_into().unwrap()) as usize;
        match decode_record(&bytes[pos..pos + length]).unwrap() {
            AvcRecord::Config { config: c, .. } => {
                config = Some(encode_config(stream, &c).unwrap())
            }
            AvcRecord::AccessUnit(unit) if !unit.end_of_stream => units.push(unit),
            _ => {}
        }
        pos += length;
    }
    (config.unwrap(), units)
}
fn harness_line(expected: &str) {
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    assert_eq!(line.trim(), expected, "private harness checkpoint required");
}
fn terminal(process: &mut Process, success: bool) {
    loop {
        let event = process.next_event();
        if event["event"] == "camera_record" {
            continue;
        }
        assert_eq!(event["event"], if success { "stopped" } else { "failed" });
        break;
    }
    assert_eq!(process.finish().code(), Some(if success { 0 } else { 1 }));
    if !success {
        assert_eq!(
            String::from_utf8_lossy(&process.stderr).trim(),
            "Error: Linux camera launch failed"
        );
    }
}
/// This is run ONLY inside the disposable private PipeWire harness. Its caller
/// independently verifies exact 20-frame NV12 pixels, graph ownership, sentinel
/// continuity, and helper/decoder reaping; Rust asserts the real node outcome.
#[test]
#[ignore = "requires explicit disposable native PipeWire graph harness"]
fn hosted_linux_camera_graph() {
    use capyio_vcamdroid_adapter::{AvcAccessUnit, AvcStreamKey, encode_access_unit};
    assert_eq!(
        std::env::var("CAPYIO_CAMERA_LAUNCHER_HOSTED").as_deref(),
        Ok("1")
    );
    assert!(
        std::env::var("PIPEWIRE_REMOTE").is_ok_and(|s| s == "capyio-test"),
        "private daemon opt-in required"
    );
    let scenario = std::env::var("CAPYIO_CAMERA_SCENARIO").unwrap();
    assert!(
        [
            "pixels",
            "local_stop",
            "control_stop",
            "control_eof",
            "source_revoke",
            "partial_cavc",
            "expiry",
            "handshake_timeout"
        ]
        .contains(&scenario.as_str())
    );
    let fixture = Fixture::new();
    let mut process = Process::camera();
    println!("\nLAUNCHER_PID {}", process.child.id());
    io::stdout().flush().unwrap();
    harness_line("approve synthetic camera");
    fixture.bootstrap(&mut process);
    let mut peer = fixture.connect(&mut process);
    let approval = process.event("approval_required");
    peer.idle(&process);
    process.command(approval_command(&approval));
    let id = serde_json::from_value(approval["route_id"].clone()).unwrap();
    peer.proposal(id);
    peer.idle(&process);
    peer.grant(
        id,
        true,
        if scenario == "expiry" || scenario == "handshake_timeout" {
            4000
        } else {
            10000
        },
    );
    let offer = peer.offer(id);
    no_children(&process);
    println!("SECURE_LISTENING no_media_children=true");
    io::stdout().flush().unwrap();
    if scenario == "handshake_timeout" {
        harness_line("start frames");
        let _stalled =
            std::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, offer.port)).unwrap();
        let started = Instant::now();
        terminal(&mut process, false);
        assert!(started.elapsed() < BOUND);
        println!("VERIFIED handshake_timeout\nCAMERA_STOPPED");
        return;
    }
    let mut sender = SecureCameraSender::connect(
        &peer.runtime,
        &peer.link,
        id,
        offer,
        &peer.directory,
        &peer.credentials,
        now_ms(),
    )
    .unwrap();
    let bound = process.event("camera_bound");
    let helper = bound["helper_pid"].as_u64().unwrap();
    let decoder = bound["decoder_pid"].as_u64().unwrap();
    println!(
        "CAMERA_READY {} {}",
        helper,
        bound["node_name"].as_str().unwrap()
    );
    println!("DECODER_READY {decoder}");
    io::stdout().flush().unwrap();
    harness_line("start frames");
    let binding = sender.binding();
    let stream = AvcStreamKey {
        stream_id: *binding.stream_id.as_uuid().as_bytes(),
        epoch: binding.epoch,
    };
    let (config, units) = fixture_records(stream);
    sender
        .send_record(&peer.runtime, &config, now_ms())
        .unwrap();
    process.event("camera_record");
    let count = if scenario == "pixels" { 90 } else { 3 };
    let start = Instant::now();
    for sequence in 1..=count {
        let unit = AvcAccessUnit {
            stream,
            sequence,
            presentation_time_us: sequence * 33333,
            ..units[(sequence as usize - 1) % units.len()].clone()
        };
        sender
            .send_record(&peer.runtime, &encode_access_unit(&unit).unwrap(), now_ms())
            .unwrap();
        process.event("camera_record");
        if let Some(wait) = (start + Duration::from_nanos(sequence * 33_333_333))
            .checked_duration_since(Instant::now())
        {
            thread::sleep(wait);
        }
    }
    let next_record = encode_access_unit(&AvcAccessUnit {
        stream,
        sequence: count + 1,
        presentation_time_us: (count + 1) * 33333,
        ..units[count as usize % units.len()].clone()
    })
    .unwrap();
    match scenario.as_str() {
        "pixels" | "local_stop" => process.command(json!({"version":1,"command":"stop"})),
        "control_stop" => peer
            .link
            .send_route_command_until(id, RouteCommandAction::Stop, Instant::now() + BOUND)
            .unwrap(),
        "control_eof" => peer.link.close(),
        "source_revoke" => {
            let route = peer.runtime.route(id).unwrap().clone();
            peer.runtime
                .release_route_port_lease(id, route.source, route.session_id)
                .unwrap();
            assert_eq!(
                sender
                    .send_record(&peer.runtime, &next_record, now_ms())
                    .unwrap_err(),
                "camera Source authority revoked",
                "real Source authority rejects an otherwise valid next frame"
            );
            sender.stop();
        }
        "partial_cavc" => {
            assert!(
                sender
                    .send_record(&peer.runtime, b"CAVC", now_ms())
                    .is_err(),
                "production Android guard rejects partial record"
            );
            sender.stop();
        }
        "expiry" => {
            let route = peer.runtime.route(id).unwrap();
            let expiry = peer
                .runtime
                .route_port_lease_claim(id, route.source)
                .unwrap()
                .expires_at_ms;
            while now_ms() < expiry {
                if now_ms() + 100 < expiry {
                    assert!(
                        process.child.try_wait().unwrap().is_none(),
                        "launcher failed before actual finite lease expiry"
                    );
                }
                process.collect();
                thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(
                sender
                    .send_record(&peer.runtime, &next_record, now_ms())
                    .unwrap_err(),
                "camera Source lease expired or clock regressed"
            );
        }
        _ => unreachable!(),
    }
    terminal(
        &mut process,
        matches!(scenario.as_str(), "pixels" | "local_stop"),
    );
    sender.stop();
    for pid in [helper, decoder] {
        assert!(
            !std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "owned media child reaped"
        );
    }
    println!("VERIFIED {scenario}\nCAMERA_STOPPED");
}

#[test]
fn ipc_linux_camera_malformed_approval_commands_fail_closed() {
    for body in [
        br#"[1,"stop",null,null]"#.as_slice(),
        br#"{"version":1,"command":"stop","command":"status"}"#.as_slice(),
        br#"{"version":1,"command":"stop","helper":"/tmp/untrusted"}"#.as_slice(),
    ] {
        let (mut process, mut peer, _) = pending();
        no_children(&process);
        process.frame(body);
        failed(&mut process);
        assert!(
            peer.link
                .receive_control_message_until(Instant::now() + BOUND)
                .is_err()
        );
    }
}
#[test]
fn ipc_linux_camera_expired_or_wrong_source_grant_is_rejected() {
    for wrong_source in [false, true] {
        let (mut process, mut peer, approval) = pending();
        process.command(approval_command(&approval));
        let id = serde_json::from_value(approval["route_id"].clone()).unwrap();
        peer.proposal(id);
        let mut invalid = peer.runtime.route(id).unwrap().clone();
        if wrong_source {
            invalid.source.port_id = PortId::new();
        }
        invalid.authorization = AuthorizationState::Authorized {
            expires_at_ms: Some(if wrong_source {
                now_ms() + 5000
            } else {
                now_ms() - 1
            }),
        };
        no_children(&process);
        peer.link.send_route_status(&invalid).unwrap();
        failed(&mut process);
        assert!(peer.runtime.snapshot().port_leases.is_empty());
    }
}
#[test]
fn ipc_linux_camera_wrong_data_pin_never_spawns_helpers() {
    let (mut process, mut peer, approval) = pending();
    process.command(approval_command(&approval));
    let id = serde_json::from_value(approval["route_id"].clone()).unwrap();
    peer.proposal(id);
    peer.grant(id, true, 5000);
    let offer = peer.offer(id);
    no_children(&process);
    let route = peer.runtime.route(id).unwrap();
    let mut wrong_directory = PairedPeerDirectory::new(route.source.node_id).unwrap();
    wrong_directory
        .add(
            PairedPeerRecord::new(
                route.sink.node_id,
                "Wrong pin",
                "localhost",
                CredentialHandle::new("ephemeral.test.only").unwrap(),
                identities().1.expected_peer_certificate,
                1,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        SecureCameraSender::connect(
            &peer.runtime,
            &peer.link,
            id,
            offer,
            &wrong_directory.encode_json().unwrap(),
            &peer.credentials,
            now_ms()
        )
        .is_err()
    );
    failed(&mut process);
}

#[test]
fn ipc_linux_camera_denied_or_missing_source_lease_fails_before_helpers() {
    for deny in [true, false] {
        let (mut process, mut peer, approval) = pending();
        process.command(approval_command(&approval));
        let id = serde_json::from_value(approval["route_id"].clone()).unwrap();
        peer.proposal(id);
        peer.grant(id, true, 5000);
        let ReceivedControlMessage::RouteStatus(status) = peer
            .link
            .receive_control_message_until(Instant::now() + BOUND)
            .unwrap()
        else {
            panic!("expected finite grant narrowing")
        };
        let AuthorizationState::Authorized {
            expires_at_ms: Some(expiry),
        } = status.route.authorization
        else {
            panic!("finite grant required")
        };
        peer.runtime
            .restrict_draft_authorization(id, expiry, now_ms())
            .unwrap();
        peer.link
            .send_route_status(peer.runtime.route(id).unwrap())
            .unwrap();
        let ReceivedControlMessage::PortLeaseRequest(request) = peer
            .link
            .receive_control_message_until(Instant::now() + BOUND)
            .unwrap()
        else {
            panic!("expected real source lease request")
        };
        assert_eq!(request.request().route_id, id);
        no_children(&process);
        if deny {
            // Local Android policy narrows its consent before it can grant this
            // request. Production control host must answer Denied, not forge a
            // lease beyond that local authority. No media offer may follow.
            let now = now_ms();
            peer.runtime
                .restrict_draft_authorization(id, now + 1000, now)
                .unwrap();
            peer.link
                .answer_port_lease_request(&mut peer.runtime, request, now)
                .unwrap();
            assert!(peer.runtime.snapshot().port_leases.is_empty());
        }
        failed(&mut process);
        assert!(
            peer.link
                .receive_control_message_until(Instant::now() + BOUND)
                .is_err(),
            "denied/missing lease cannot produce data offer"
        );
    }
}

#[test]
fn ipc_linux_camera_configured_source_must_exist_in_authenticated_catalog() {
    let mut fixture = Fixture::new();
    fixture.config["source"]["port_id"] = json!(PortId::new());
    let mut process = Process::camera();
    fixture.bootstrap(&mut process);
    let listening = process.event("listening");
    let nodes = ControlTlsNodeBinding::new(
        serde_json::from_value(fixture.config["node_id"].clone()).unwrap(),
        fixture.source.node_id,
    )
    .unwrap();
    let channel = AuthenticatedControlClientChannel::connect_fresh_session(
        ControlTlsEndpoint {
            address: listening["address"].as_str().unwrap().parse().unwrap(),
            io_timeout: BOUND,
        },
        "localhost",
        fixture.client,
        nodes,
    )
    .unwrap();
    let mut link = CatalogControlLink::admit_dialed_new_peer(
        &mut fixture.runtime,
        channel,
        "Synthetic Linux".into(),
    )
    .unwrap();
    no_children(&process);
    let catalog = link
        .prepare_catalog(
            &fixture.runtime,
            &BTreeSet::from([fixture.source.capability_id]),
        )
        .unwrap();
    link.send_catalog(catalog).unwrap();
    assert!(
        link.receive_catalog().is_err(),
        "unknown Source must fail before approval or peer catalog publication"
    );
    failed(&mut process);
}
