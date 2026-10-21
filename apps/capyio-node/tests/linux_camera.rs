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
#[cfg(feature = "paired-store")]
#[path = "support/paired_store.rs"]
mod paired_store;
#[cfg(feature = "paired-store")]
#[path = "support/playback_console.rs"]
mod playback_console;

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
    console: Option<playback_console::Console>,
    child: Child,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    #[cfg(feature = "paired-store")]
    stdout_history: Vec<u8>,
}
impl Process {
    fn start(args: &[&str]) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_capyio-node"))
            .args(args)
            .env_remove("RUST_LOG")
            // The production entry maps all failures to a static message before
            // anyhow renders it. Keep this protocol fixture deterministic even
            // when the CI test runner requests diagnostic stack traces.
            .env("RUST_BACKTRACE", "0")
            .env("RUST_LIB_BACKTRACE", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        nonblocking(child.stdin.as_ref().unwrap());
        nonblocking(child.stdout.as_ref().unwrap());
        nonblocking(child.stderr.as_ref().unwrap());
        Self {
            #[cfg(feature = "paired-store")]
            console: None,
            child,
            stdout: Vec::new(),
            stderr: Vec::new(),
            #[cfg(feature = "paired-store")]
            stdout_history: Vec::new(),
        }
    }
    fn camera() -> Self {
        Self::start(&["linux-camera", "--trusted-lab"])
    }
    fn frame(&mut self, body: &[u8]) {
        assert!(
            !self.is_console(),
            "console fixture cannot access Node control pipes"
        );
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
                _ => panic!("console fixture only accepts terminal Start/status/Stop"),
            }
            return;
        }
        self.frame(&serde_json::to_vec(&command).unwrap());
    }
    fn is_console(&self) -> bool {
        #[cfg(feature = "paired-store")]
        {
            self.console.is_some()
        }
        #[cfg(not(feature = "paired-store"))]
        {
            false
        }
    }
    fn owner_pid(&self) -> u32 {
        #[cfg(feature = "paired-store")]
        if let Some(console) = &self.console {
            return console
                .node_pid
                .expect("actual Node identity observed at listening");
        }
        self.child.id()
    }
    #[cfg(feature = "paired-store")]
    fn wait_console_prompt(&mut self, request: &str, route: &str) {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
            let console = self.console.as_ref().unwrap();
            if console.prompt_seen(request, route) {
                console.assert_hidden();
                // The real controller flushes once after printing its prompt.
                thread::sleep(Duration::from_millis(25));
                return;
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "console exited before consent prompt"
            );
            assert!(Instant::now() < deadline, "bounded fresh consent prompt");
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn collect(&mut self) {
        #[cfg(feature = "paired-store")]
        if let Some(console) = &mut self.console {
            console.collect(&mut self.child, &mut self.stdout, &mut self.stderr);
            return;
        }
        #[cfg(feature = "paired-store")]
        let previous = self.stdout.len();
        drain(self.child.stdout.as_mut().unwrap(), &mut self.stdout);
        #[cfg(feature = "paired-store")]
        {
            self.stdout_history
                .extend_from_slice(&self.stdout[previous..]);
            assert!(
                self.stdout_history.len() <= 1_048_576,
                "bounded output transcript"
            );
        }
        drain(self.child.stderr.as_mut().unwrap(), &mut self.stderr);
    }
    fn next_event(&mut self) -> Value {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
            #[cfg(feature = "paired-store")]
            if let Some(console) = &mut self.console
                && let Some(value) = console.next_event()
            {
                if value["event"] == "listening" {
                    console.identify_node(&self.child);
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
                #[cfg(feature = "paired-store")]
                if let Some(console) = &self.console {
                    console.assert_finished();
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
            // SAFETY: only this fixture's new private console session is targeted.
            unsafe {
                libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
            }
        }
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
    let pid = process.owner_pid();
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
#[cfg(feature = "paired-store")]
fn persisted_fixture(fixture: &Fixture) -> paired_store::PersistedPairing {
    paired_store::PersistedPairing::new(
        serde_json::from_value(fixture.config["node_id"].clone()).unwrap(),
        &fixture.directory,
        &fixture.credentials,
    )
}
#[cfg(feature = "paired-store")]
fn paired_process(store: &paired_store::PersistedPairing) -> Process {
    let args = store.args("linux-camera");
    Process::start(&args.iter().map(String::as_str).collect::<Vec<_>>())
}
#[cfg(feature = "paired-store")]
fn paired_bootstrap(fixture: &Fixture, process: &mut Process) {
    process.command(fixture.config.clone());
    process.command(json!({"version":1,"credential_source":"paired-store"}));
}
#[cfg(feature = "paired-store")]
fn assert_paired_no_leak(store: &paired_store::PersistedPairing, process: &Process) {
    store.assert_no_leak(&process.stdout_history);
    store.assert_no_leak(&process.stderr);
    if let Some(console) = &process.console {
        store.assert_no_leak(&console.tty);
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
        let expected = if process.is_console() {
            "Linux console operation failed; no automatic retry. Check explicit selectors, paired-store build and interactive terminal. Pairing persistence may require inspection."
        } else {
            "Error: Linux camera launch failed"
        };
        assert_eq!(String::from_utf8_lossy(&process.stderr).trim(), expected);
    }
}
#[cfg(feature = "paired-store")]
fn console_pending_scenario(name: &str) -> bool {
    matches!(
        name,
        "no_approval" | "pretyped" | "mismatched_request" | "mismatched_route" | "mismatched_peer"
    )
}
#[cfg(feature = "paired-store")]
fn console_process(
    store: &paired_store::PersistedPairing,
    config: &Value,
    pretyped: bool,
) -> Process {
    let (child, console) = playback_console::Console::start(
        store,
        config,
        pretyped,
        "camera",
        &[("RUST_BACKTRACE", "0"), ("RUST_LIB_BACKTRACE", "0")],
    );
    Process {
        child,
        console: Some(console),
        stdout: vec![],
        stderr: vec![],
        stdout_history: vec![],
    }
}
#[cfg(feature = "paired-store")]
fn assert_console_consent(
    store: &paired_store::PersistedPairing,
    process: &mut Process,
    approval: &Value,
) {
    process.wait_console_prompt(
        approval["request_id"].as_str().unwrap(),
        approval["route_id"].as_str().unwrap(),
    );
    let console = process.console.as_ref().unwrap();
    console.assert_hidden();
    // The public event must identify the exact endpoints and finite secure media
    // contract; the human-readable summary is checked independently below.
    assert_eq!(approval["source"]["node_id"], json!(store.peer));
    assert_eq!(approval["sink"]["node_id"], json!(store.owner));
    assert_eq!(approval["format"], "cavc-avc-access-unit-v1");
    assert_eq!(approval["media_security"], "tls13_mtls_v1");
    assert_eq!(approval["duration_secs"], 10);
    assert_eq!(approval["width"], 1280);
    assert_eq!(approval["height"], 720);
    assert_eq!(approval["fps"], 30);
    assert_eq!(approval["rotation"], 0);
    assert_eq!(approval["max_frames"], 120);
    assert_eq!(approval["max_input_bytes"], 33_554_432);
    assert_eq!(approval["approval_timeout_secs"], 30);
    console.assert_presentation(approval);
    for pid in [process.child.id(), process.owner_pid()] {
        for name in ["cmdline", "environ"] {
            store.assert_no_leak(&std::fs::read(format!("/proc/{pid}/{name}")).unwrap());
        }
    }
    assert_paired_no_leak(store, process);
}
#[cfg(feature = "paired-store")]
fn finish_console_pending(
    name: &str,
    approval: &Value,
    store: &paired_store::PersistedPairing,
    process: &mut Process,
    peer: &mut Peer,
) {
    assert!(peer.runtime.snapshot().routes.is_empty());
    assert!(peer.runtime.snapshot().port_leases.is_empty());
    no_children(process);
    if name.starts_with("mismatched_") {
        let console = process.console.as_mut().unwrap();
        let request = if name == "mismatched_request" {
            MessageId::new().to_string()
        } else {
            approval["request_id"].as_str().unwrap().to_owned()
        };
        let route = if name == "mismatched_route" {
            RouteId::new().to_string()
        } else {
            approval["route_id"].as_str().unwrap().to_owned()
        };
        let peer = if name == "mismatched_peer" {
            NodeId::new()
        } else {
            console.peer
        };
        console.send(format!("start {request} {route} {peer}\n").as_bytes());
        assert!(
            !process.finish().success(),
            "mismatched terminal consent cannot approve"
        );
    } else {
        process.command(json!({"version":1,"command":"status"}));
        assert_eq!(process.event("status")["stage"], "awaiting_local_approval");
        process.wait_console_prompt(
            approval["request_id"].as_str().unwrap(),
            approval["route_id"].as_str().unwrap(),
        );
        peer.idle(process);
        process.command(json!({"version":1,"command":"stop"}));
        terminal(process, true);
    }
    assert!(
        peer.link
            .receive_control_message_until(Instant::now() + BOUND)
            .is_err(),
        "no Route proposal before fresh exact terminal consent"
    );
    assert!(peer.runtime.snapshot().routes.is_empty());
    assert!(peer.runtime.snapshot().port_leases.is_empty());
    console_verified(name, store, process);
    println!("VERIFIED {name}\nCAMERA_STOPPED");
}
#[cfg(feature = "paired-store")]
fn console_verified(name: &str, store: &paired_store::PersistedPairing, process: &Process) {
    assert_paired_no_leak(store, process);
    let state = store.service.state.lock().unwrap();
    assert!(state.reads > 0, "actual protected credential read required");
    assert_eq!(
        state.forbidden, 0,
        "no protected service mutations permitted"
    );
    process.console.as_ref().unwrap().assert_finished();
    println!(
        "CONSOLE_VERIFIED {name} actual_tty=true fresh_consent=true secret_redacted=true node_reaped=true"
    );
}
#[cfg(feature = "paired-store")]
fn camera_expiry_observation_reached(observed_at_ms: u64, lease_expires_at_ms: u64) -> bool {
    observed_at_ms >= lease_expires_at_ms
}
#[cfg(feature = "paired-store")]
fn assert_console_not_early_expiry(process: &mut Process, lease_expires_at_ms: u64) {
    process.collect();
    let terminal_seen = std::str::from_utf8(&process.console.as_ref().unwrap().tty)
        .unwrap()
        .lines()
        .filter_map(|line| line.trim_start_matches('\r').strip_prefix("Camera event: "))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .any(|value| matches!(value["event"].as_str(), Some("failed" | "stopped")));
    if terminal_seen {
        assert!(
            camera_expiry_observation_reached(now_ms(), lease_expires_at_ms),
            "console reported terminal camera state before actual negotiated Source lease expiry"
        );
    }
    if process.child.try_wait().unwrap().is_some() {
        assert!(
            camera_expiry_observation_reached(now_ms(), lease_expires_at_ms),
            "actual console exited before negotiated Source lease expiry"
        );
    }
    // The console can outlive its Node briefly. Observe the Node independently,
    // including an exited-but-unreaped zombie, rather than hiding early failure
    // by waiting for the controller's later cleanup or configured duration.
    let stat = std::fs::read_to_string(format!("/proc/{}/stat", process.owner_pid()));
    let exited = match stat {
        Ok(stat) => matches!(
            stat.rsplit_once(')').unwrap().1.split_whitespace().next(),
            Some("Z" | "X")
        ),
        Err(error) if error.kind() == io::ErrorKind::NotFound => true,
        Err(error) => panic!("actual Node process observation failed: {error}"),
    };
    if exited {
        assert!(
            camera_expiry_observation_reached(now_ms(), lease_expires_at_ms),
            "actual Node exited before negotiated Source lease expiry"
        );
    }
}
#[cfg(feature = "paired-store")]
#[test]
fn pure_camera_console_expiry_requires_actual_negotiated_lease_without_slack() {
    for (observed, expiry, allowed) in [
        (1_999, 2_000, false),
        (2_000, 2_000, true),
        (2_001, 2_000, true),
        (2_000, 3_000, false),
        (0, 0, true),
        (u64::MAX - 1, u64::MAX, false),
    ] {
        assert_eq!(camera_expiry_observation_reached(observed, expiry), allowed);
    }
}

fn assert_camera_not_consumer_proof(event: &Value, id: RouteId) {
    assert_eq!(event["route_id"], json!(id));
    assert_eq!(
        event["state"], "starting",
        "queued records are not consumer delivery or Active"
    );
    assert_eq!(event["consumer_delivery_verified"], false);
}

/// This is run ONLY inside the disposable private PipeWire harness. Its caller
/// independently verifies exact 20-frame NV12 pixels, graph ownership, sentinel
/// continuity, and helper/decoder reaping; Rust asserts the real node outcome.
#[test]
#[ignore = "requires explicit disposable native PipeWire graph harness"]
fn hosted_linux_camera_graph() {
    assert_ne!(
        std::env::var("CAPYIO_CAMERA_CONSOLE").as_deref(),
        Ok("1"),
        "legacy fixture cannot substitute for actual console"
    );
    run_hosted_linux_camera_graph(false);
}
#[cfg(feature = "paired-store")]
#[test]
#[ignore = "requires explicit disposable native PipeWire graph and actual console PTY harness"]
fn hosted_linux_camera_console_graph() {
    assert_eq!(std::env::var("CAPYIO_CAMERA_CONSOLE").as_deref(), Ok("1"));
    run_hosted_linux_camera_graph(true);
}
fn run_hosted_linux_camera_graph(console_mode: bool) {
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
    let supported = if console_mode {
        &[
            "pixels",
            "no_approval",
            "pretyped",
            "mismatched_request",
            "mismatched_route",
            "mismatched_peer",
            "expiry",
            "remote_eof",
            "local_stop",
            "control_stop",
            "source_revoke",
            "partial_cavc",
            "handshake_timeout",
        ][..]
    } else {
        &[
            "pixels",
            "local_stop",
            "control_stop",
            "control_eof",
            "source_revoke",
            "partial_cavc",
            "expiry",
            "handshake_timeout",
        ][..]
    };
    assert!(supported.contains(&scenario.as_str()));
    let paired_mode = match std::env::var("CAPYIO_CAMERA_PAIRED_STORE").as_deref() {
        Ok("1") => true,
        Ok("0") | Err(std::env::VarError::NotPresent) => false,
        _ => panic!("invalid explicit paired-store fixture mode"),
    };
    #[cfg(not(feature = "paired-store"))]
    assert!(
        !paired_mode,
        "paired-store mode requires a feature-enabled driver"
    );
    assert!(
        !console_mode || paired_mode,
        "console requires actual protected paired identity"
    );
    let fixture = Fixture::new();
    #[cfg(feature = "paired-store")]
    let store = paired_mode.then(|| persisted_fixture(&fixture));
    #[cfg(feature = "paired-store")]
    let mut process = match &store {
        Some(store) if console_mode => {
            console_process(store, &fixture.config, scenario == "pretyped")
        }
        Some(store) => paired_process(store),
        None => Process::camera(),
    };
    #[cfg(not(feature = "paired-store"))]
    let mut process = Process::camera();
    if console_mode {
        println!("\nCONSOLE_PID {}", process.child.id());
    } else {
        println!("\nLAUNCHER_PID {}", process.child.id());
    }
    io::stdout().flush().unwrap();
    harness_line("approve synthetic camera");
    #[cfg(feature = "paired-store")]
    if !console_mode {
        if paired_mode {
            paired_bootstrap(&fixture, &mut process);
        } else {
            fixture.bootstrap(&mut process);
        }
    }
    #[cfg(not(feature = "paired-store"))]
    fixture.bootstrap(&mut process);
    let mut peer = fixture.connect(&mut process);
    if console_mode {
        println!("LAUNCHER_PID {}", process.owner_pid());
    }
    if paired_mode {
        // Emitted only after the actual node loaded its store-backed identity
        // and the exact peer completed the existing pinned control handshake.
        println!("PAIRED_STORE_AUTH verified");
        io::stdout().flush().unwrap();
    }
    let approval = process.event("approval_required");
    peer.idle(&process);
    #[cfg(feature = "paired-store")]
    if console_mode {
        let store = store.as_ref().unwrap();
        assert_console_consent(store, &mut process, &approval);
        println!("CONSOLE_CONSENT exact_source_sink=true no_echo=true");
        io::stdout().flush().unwrap();
        // Outer Python verifies the real graph and both exact process identities
        // while the actual console remains at its unapproved terminal prompt.
        harness_line("check consent");
        if console_pending_scenario(&scenario) {
            finish_console_pending(&scenario, &approval, store, &mut process, &mut peer);
            return;
        }
    }
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
    if console_mode {
        process.command(json!({"version":1,"command":"status"}));
        let status = process.event("status");
        assert_camera_not_consumer_proof(&status, id);
        assert!(
            status["helper_pid"].is_null()
                && status["decoder_pid"].is_null()
                && status["node_name"].is_null()
        );
        assert_eq!(status["queued"], 0);
    }
    println!("SECURE_LISTENING no_media_children=true");
    io::stdout().flush().unwrap();
    if scenario == "handshake_timeout" {
        harness_line("start frames");
        let _stalled =
            std::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, offer.port)).unwrap();
        let started = Instant::now();
        terminal(&mut process, false);
        assert!(started.elapsed() < BOUND);
        #[cfg(feature = "paired-store")]
        if let Some(store) = &store {
            assert_paired_no_leak(store, &process);
        }
        #[cfg(feature = "paired-store")]
        if console_mode {
            console_verified(&scenario, store.as_ref().unwrap(), &process);
        }
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
    assert_camera_not_consumer_proof(&bound, id);
    assert_eq!(bound["queued"], 0);
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
    let record = process.event("camera_record");
    assert_camera_not_consumer_proof(&record, id);
    assert_eq!(record["queued"], 0);
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
        let record = process.event("camera_record");
        assert_camera_not_consumer_proof(&record, id);
        assert_eq!(record["queued"], sequence);
        for field in ["helper_pid", "decoder_pid", "node_name"] {
            assert_eq!(
                record[field], bound[field],
                "exact bound helper identities persist"
            );
        }
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
        "control_eof" | "remote_eof" => peer.link.close(),
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
            let AuthorizationState::Authorized {
                expires_at_ms: Some(grant_expiry),
            } = route.authorization
            else {
                panic!("finite negotiated authorization required")
            };
            assert!(
                expiry < grant_expiry,
                "actual negotiated Source lease, not configured duration, bounds cleanup"
            );
            let deadline =
                Instant::now() + Duration::from_millis(expiry.saturating_sub(now_ms())) + BOUND;
            while now_ms() < expiry {
                assert!(
                    Instant::now() < deadline,
                    "bounded actual lease expiry observation"
                );
                #[cfg(feature = "paired-store")]
                if console_mode {
                    assert_console_not_early_expiry(&mut process, expiry);
                }
                if !console_mode {
                    // Preserve the existing legacy launcher's observation gate.
                    if now_ms() + 100 < expiry {
                        assert!(
                            process.child.try_wait().unwrap().is_none(),
                            "launcher failed before actual finite lease expiry"
                        );
                    }
                    process.collect();
                }
                thread::sleep(Duration::from_millis(if console_mode { 2 } else { 10 }));
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
    #[cfg(feature = "paired-store")]
    if let Some(store) = &store {
        assert_paired_no_leak(store, &process);
    }
    #[cfg(feature = "paired-store")]
    if console_mode {
        console_verified(&scenario, store.as_ref().unwrap(), &process);
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

#[cfg(feature = "paired-store")]
#[test]
fn ipc_paired_camera_pinned_peer_without_local_approval_never_activates() {
    let fixture = Fixture::new();
    let store = persisted_fixture(&fixture);
    let mut process = paired_process(&store);
    paired_bootstrap(&fixture, &mut process);
    let mut peer = fixture.connect(&mut process);
    let approval = process.event("approval_required");
    assert_eq!(approval["media_security"], "tls13_mtls_v1");
    peer.idle(&process);
    no_children(&process);
    assert!(peer.runtime.snapshot().port_leases.is_empty());
    process.command(json!({"version":1,"command":"stop"}));
    process.event("stopped");
    assert!(process.finish().success());
    assert_paired_no_leak(&store, &process);
}

#[cfg(feature = "paired-store")]
#[test]
fn ipc_paired_camera_wrong_control_pin_never_spawns_helpers() {
    let mut fixture = Fixture::new();
    let local = serde_json::from_value(fixture.config["node_id"].clone()).unwrap();
    let original = PairedPeerDirectory::decode_json(local, &fixture.directory).unwrap();
    let mut record = original.active_peers().next().unwrap().clone();
    // Keep the valid peer certificate/CA and replace only the persisted exact
    // peer leaf pin. Ordinary chain trust must not bypass stored pairing trust.
    record.peer_certificate_sha256 = identities().2;
    let mut wrong_pin = PairedPeerDirectory::new(local).unwrap();
    wrong_pin.add(record).unwrap();
    fixture.directory = wrong_pin.encode_json().unwrap();
    let store = persisted_fixture(&fixture);
    let mut process = paired_process(&store);
    paired_bootstrap(&fixture, &mut process);
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
                io_timeout: BOUND,
            },
            "localhost",
            fixture.client,
            nodes,
        )
        .is_err()
    );
    failed(&mut process);
    assert_paired_no_leak(&store, &process);
}

#[cfg(feature = "paired-store")]
#[test]
fn ipc_paired_camera_revoked_store_never_opens_listener() {
    let fixture = Fixture::new();
    let store = persisted_fixture(&fixture);
    store.revoke();
    let mut process = paired_process(&store);
    paired_bootstrap(&fixture, &mut process);
    failed(&mut process);
    assert!(!String::from_utf8_lossy(&process.stdout_history).contains("listening"));
    assert_paired_no_leak(&store, &process);
}
