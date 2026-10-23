#![cfg(target_os = "linux")]
//! Explicit hosted-only reusable-API proof. The Python owner supplies a private
//! PipeWire graph and independently verifies decoded stereo PCM and owned cleanup.
//! No Android AudioTrack, physical device, Node launcher, keyring or audibility.
use capyio_android_jni::secure_stereo_receiver::{
    PcmSink, SecureStereoSinkAdmission, SecureStereoSinkEvent, SecureStereoSinkOwner,
};
use capyio_android_jni::{
    stereo_job::{
        BOUND as JOB_BOUND, ENABLED as JOB_ENABLED, PENDING as JOB_PENDING, STOPPED as JOB_STOPPED,
        StereoAdmissionGate, StereoAuthority, StereoReceiveJob,
    },
    stereo_negotiation::{StereoNegotiationAction, StereoSinkNegotiation},
};
use capyio_control_credentials::ControlCredentialBundle;
use capyio_control_link::{CatalogControlLink, ReceivedControlMessage};
use capyio_control_tls::{
    AuthenticatedControlClientChannel, AuthenticatedControlServer, ControlTlsClientIdentity,
    ControlTlsEndpoint, ControlTlsNodeBinding, ControlTlsServerIdentity,
};
use capyio_core::*;
use capyio_linux_audio::{
    SecureStereoReceiverAssignedSourceAdmission, SecureStereoSourceAdmission,
    SecureStereoSourceEvent, SecureStereoSourceOwner,
};
use capyio_pairing::CertificateFingerprint;
use capyio_protocol::SECURE_AUDIO_STEREO_FORMAT;
use capyio_runtime::{NodeRuntime, PortLeasePolicy};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const PREFIX: &str = ".capyio-secure-stereo-";
const TICK: Duration = Duration::from_millis(2);
const BOUND: Duration = Duration::from_secs(10);
const PCM_LIMIT: usize = 2 * 1024 * 1024;
const SCENARIOS: [&str; 9] = [
    "waveform",
    "stop",
    "source_revoke",
    "sink_revoke",
    "control_eof",
    "media_eof",
    "expiry",
    "backpressure",
    "partial_write",
];

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap()
}
fn marker(root: &Path, name: &str) -> PathBuf {
    root.join(format!("{PREFIX}{name}"))
}
fn publish(root: &Path, scenario: &str, stage: &str, extra: Value) {
    let mut value = json!({"version":1, "scenario":scenario, "stage":stage,
        "format":SECURE_AUDIO_STEREO_FORMAT, "surface":"reusable_rust_apis", "owner_pid":std::process::id()});
    value
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let destination = marker(root, &format!("{stage}.json"));
    let temporary = destination.with_extension("tmp");
    fs::write(&temporary, serde_json::to_vec(&value).unwrap()).unwrap();
    fs::rename(temporary, destination).unwrap();
}
fn wait_marker(root: &Path, name: &str) {
    let end = Instant::now() + BOUND;
    while !marker(root, name).is_file() {
        assert!(
            Instant::now() < end,
            "hosted stereo observation {name} missing"
        );
        thread::sleep(TICK);
    }
}
fn no_children() {
    for entry in fs::read_dir("/proc").unwrap().flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        let Ok(status) = fs::read_to_string(entry.path().join("status")) else {
            continue;
        };
        let parent = status
            .lines()
            .find_map(|line| line.strip_prefix("PPid:"))
            .and_then(|value| value.trim().parse::<u32>().ok());
        assert_ne!(
            parent,
            Some(std::process::id()),
            "owned helper exists outside authority"
        );
    }
}
fn pending(root: &Path, scenario: &str, stage: &str, source: &NodeRuntime, sink: &NodeRuntime) {
    no_children();
    assert!(source.snapshot().port_leases.is_empty() && sink.snapshot().port_leases.is_empty());
    publish(
        root,
        scenario,
        stage,
        json!({"helper_absent":true,"lease_count":0,"pcm_bytes_accepted":0}),
    );
}

/// Two fresh leaves, each with both TLS usages. Control roles are intentionally
/// opposite media roles: Linux accepts control but dials Android's media listener.
fn identities() -> (
    ControlTlsServerIdentity,
    ControlTlsClientIdentity,
    ControlTlsClientIdentity,
    ControlTlsServerIdentity,
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
    let leaf = || {
        let mut params = CertificateParams::new(vec!["localhost".into()]).unwrap();
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ];
        let key = KeyPair::generate().unwrap();
        (
            params.signed_by(&key, &issuer).unwrap().der().to_vec(),
            key.serialize_der(),
        )
    };
    let (linux, linux_key) = leaf();
    let (android, android_key) = leaf();
    let linux_pin = CertificateFingerprint::from_certificate_der(&linux).unwrap();
    let android_pin = CertificateFingerprint::from_certificate_der(&android).unwrap();
    let bundle = |leaf: &Vec<u8>, key: &Vec<u8>| {
        ControlCredentialBundle::new(vec![leaf.clone(), ca.clone()], key.clone(), ca.clone())
            .unwrap()
    };
    (
        ControlTlsServerIdentity::from_bundle(android_pin, bundle(&linux, &linux_key)),
        ControlTlsClientIdentity::from_bundle(android_pin, bundle(&linux, &linux_key)),
        ControlTlsClientIdentity::from_bundle(linux_pin, bundle(&android, &android_key)),
        ControlTlsServerIdentity::from_bundle(linux_pin, bundle(&android, &android_key)),
    )
}
fn runtime(platform: Platform, direction: PortDirection) -> (NodeRuntime, PortRef) {
    let mut node = NodeDescriptor::new(
        NodeId::new(),
        "Synthetic secure stereo API",
        platform,
        "not probed",
        env!("CARGO_PKG_VERSION"),
        [ProtocolVersion::new(
            1,
            capyio_protocol::PROTOCOL_MINOR as u16,
        )],
    );
    let adapter = AdapterInstanceId::new();
    let capability = CapabilityId::new();
    let port = PortId::new();
    node.add_adapter(AdapterInstanceDescriptor {
        id: adapter,
        adapter_type: "capyio.synthetic.secure-stereo".into(),
        display_name: "Synthetic explicit stereo adapter".into(),
        deployment_mode: AdapterDeploymentMode::Sidecar,
        version: env!("CARGO_PKG_VERSION").into(),
        state: AdapterState::Ready,
        health: AdapterHealth::Unknown,
        owned_capabilities: BTreeSet::new(),
        supported_route_modes: BTreeSet::from([RouteBackend::AdapterManaged]),
    })
    .unwrap();
    node.add_capability(CapabilityDescriptor {
        id: capability,
        adapter_instance_id: adapter,
        display_name: "Synthetic explicit stereo endpoint".into(),
        class: if direction == PortDirection::Source {
            CapabilityClass::Microphone
        } else {
            CapabilityClass::Speaker
        },
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::None,
        metadata: BTreeMap::new(),
        ports: BTreeMap::from([(
            port,
            PortDescriptor {
                id: port,
                capability_id: capability,
                display_name: "Secure stereo only".into(),
                direction,
                profile: ProfileId::audio_frames_v1(),
                schema_id: None,
                formats: vec![FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)],
                qos_modes: BTreeSet::from([QosMode::Interactive]),
                clock_domain: None,
                availability: Availability::Available,
                permission_requirement: PermissionRequirement::None,
                interoperability_mode: InteroperabilityMode::AdapterManaged,
            },
        )]),
    })
    .unwrap();
    let node_id = node.id;
    (
        NodeRuntime::new(node).unwrap(),
        PortRef {
            node_id,
            capability_id: capability,
            port_id: port,
        },
    )
}

/// Opaque bounded byte relay only for independently cutting the actual control
/// TCP connection. It does not terminate, inspect, replace or manufacture TLS.
struct ControlRelay {
    address: SocketAddr,
    stopped: Arc<AtomicBool>,
    sockets: Arc<Mutex<Option<(TcpStream, TcpStream)>>>,
    worker: Option<JoinHandle<()>>,
}
impl ControlRelay {
    fn new(target: SocketAddr) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let stopped = Arc::new(AtomicBool::new(false));
        let sockets = Arc::new(Mutex::new(None));
        let (worker_stop, worker_sockets) = (stopped.clone(), sockets.clone());
        let worker = thread::spawn(move || {
            let end = Instant::now() + BOUND;
            let incoming = loop {
                if worker_stop.load(Ordering::Acquire) {
                    return;
                }
                assert!(Instant::now() < end, "bounded fixture control relay accept");
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(TICK),
                    Err(error) => panic!("fixture control relay accept: {error}"),
                }
            };
            let outgoing = TcpStream::connect_timeout(&target, Duration::from_secs(2)).unwrap();
            for stream in [&incoming, &outgoing] {
                stream
                    .set_read_timeout(Some(Duration::from_millis(50)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream.set_nodelay(true).unwrap();
            }
            *worker_sockets.lock().unwrap() =
                Some((incoming.try_clone().unwrap(), outgoing.try_clone().unwrap()));
            let (mut incoming_read, mut outgoing_write) =
                (incoming.try_clone().unwrap(), outgoing.try_clone().unwrap());
            let forward_stop = worker_stop.clone();
            let forward = thread::spawn(move || {
                let _ = relay_copy(&mut incoming_read, &mut outgoing_write, &forward_stop);
                let _ = outgoing_write.shutdown(Shutdown::Write);
            });
            let (mut incoming, mut outgoing) = (incoming, outgoing);
            let _ = relay_copy(&mut outgoing, &mut incoming, &worker_stop);
            let _ = incoming.shutdown(Shutdown::Write);
            forward.join().unwrap();
        });
        Self {
            address,
            stopped,
            sockets,
            worker: Some(worker),
        }
    }
    fn close(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some((left, right)) = self.sockets.lock().unwrap().as_ref() {
            let _ = left.shutdown(Shutdown::Both);
            let _ = right.shutdown(Shutdown::Both);
        }
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
impl Drop for ControlRelay {
    fn drop(&mut self) {
        self.close();
    }
}
fn relay_copy(
    input: &mut TcpStream,
    output: &mut TcpStream,
    stopped: &AtomicBool,
) -> io::Result<()> {
    let mut bytes = [0; 8192];
    let end = Instant::now() + Duration::from_secs(25);
    while !stopped.load(Ordering::Acquire) && Instant::now() < end {
        let length = match input.read(&mut bytes) {
            Ok(length) => length,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(error) => return Err(error),
        };
        if length == 0 {
            return Ok(());
        }
        output.write_all(&bytes[..length])?;
    }
    Ok(())
}

#[derive(Default)]
struct SinkState {
    bytes: Vec<u8>,
    writes: u64,
    blocked: bool,
    stopped: bool,
}
struct TestSink {
    state: Rc<RefCell<SinkState>>,
    partial: bool,
}
impl PcmSink for TestSink {
    fn try_write_pcm(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut state = self.state.borrow_mut();
        assert!(!state.stopped, "Sink callback after Stop");
        assert_eq!(bytes.len() % 4, 0, "complete stereo sample frames only");
        state.writes += 1;
        if state.blocked {
            return Err(io::Error::from(io::ErrorKind::WouldBlock));
        }
        let count = if self.partial {
            bytes.len().min(960)
        } else {
            bytes.len()
        };
        assert!(
            state.bytes.len() + count <= PCM_LIMIT,
            "fixed fixture PCM bound"
        );
        state.bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn stop(&mut self) -> io::Result<()> {
        self.state.borrow_mut().stopped = true;
        Ok(())
    }
}
fn closed(runtime: &NodeRuntime, session: SessionId) {
    assert_eq!(
        runtime.session(session).unwrap().state,
        SessionState::Closed
    );
    assert!(runtime.snapshot().port_leases.is_empty());
}
fn expected_cause(scenario: &str) -> (&'static str, &'static str) {
    match scenario {
        "waveform" | "partial_write" => ("source", "local_stop"),
        "stop" => ("sink", "peer_stop"),
        "source_revoke" => ("source", "source_revoked"),
        "sink_revoke" => ("sink", "sink_lease_changed"),
        "control_eof" => ("sink", "control_closed"),
        "media_eof" => ("sink", "media_closed"),
        "expiry" => ("source", "expired"),
        "backpressure" => ("sink", "sink_backpressure"),
        _ => panic!("explicit finite scenario required"),
    }
}
fn exact_terminal(
    scenario: &str,
    endpoint: &str,
    cause: Option<&str>,
    observed: u64,
    injected: u64,
    source_expiry: u64,
    sink_expiry: u64,
) {
    let expected = expected_cause(scenario);
    assert_eq!(
        (endpoint, cause),
        (expected.0, Some(expected.1)),
        "exact requested terminal cause, not incidental failure"
    );
    assert!(
        observed >= injected,
        "terminal cause preceded requested injection"
    );
    if scenario == "expiry" {
        assert!(
            observed >= source_expiry.min(sink_expiry),
            "no early-expiry tolerance"
        );
    } else {
        assert!(
            observed < source_expiry.min(sink_expiry),
            "incidental expiry does not prove {scenario}"
        );
    }
}

#[test]
fn pure_secure_stereo_terminal_deadline_has_no_early_tolerance() {
    exact_terminal("expiry", "source", Some("expired"), 1000, 900, 1000, 1100);
    exact_terminal(
        "source_revoke",
        "source",
        Some("source_revoked"),
        999,
        900,
        1000,
        1100,
    );
    assert!(
        std::panic::catch_unwind(|| exact_terminal(
            "expiry",
            "source",
            Some("expired"),
            999,
            900,
            1000,
            1100
        ))
        .is_err()
    );
    assert!(
        std::panic::catch_unwind(|| exact_terminal(
            "media_eof",
            "sink",
            Some("media_timeout"),
            999,
            900,
            1000,
            1100
        ))
        .is_err()
    );
}

#[test]
#[ignore = "requires explicit hosted-private-secure-stereo harness and real private PipeWire graph"]
fn hosted_secure_stereo_linux_capture_android_sink() {
    hosted_stereo_sink(false);
}

#[test]
#[ignore = "requires explicit hosted-private-secure-stereo harness and real private PipeWire graph"]
fn hosted_receiver_assigned_secure_stereo_linux_capture_android_sink() {
    hosted_stereo_sink(true);
}

fn hosted_stereo_sink(receiver_assigned: bool) {
    assert_eq!(
        std::env::var("CAPYIO_SECURE_STEREO_HOSTED").as_deref(),
        Ok("1")
    );
    assert_eq!(std::env::var("CAPYIO_PIPEWIRE_TEST").as_deref(), Ok("1"));
    assert_eq!(
        std::env::var("PIPEWIRE_REMOTE").as_deref(),
        Ok("capyio-test")
    );
    let root = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap());
    assert_eq!(
        fs::read_to_string(root.join(".capyio-pipewire-capture")).unwrap(),
        "capyio-hosted-pipewire-capture-v1\n"
    );
    let scenario = std::env::var("CAPYIO_SECURE_STEREO_SCENARIO").unwrap();
    assert!(SCENARIOS.contains(&scenario.as_str()));
    let (mut source_runtime, source) = runtime(Platform::Linux, PortDirection::Source);
    let (mut sink_runtime, sink) = runtime(Platform::Android, PortDirection::Sink);
    let (control_server, media_client, control_client, media_server) = identities();
    let binding = ControlTlsNodeBinding::new(source.node_id, sink.node_id).unwrap();
    let server = AuthenticatedControlServer::from_listener_fresh_session(
        TcpListener::bind("127.0.0.1:0").unwrap(),
        control_server,
        binding,
        Duration::from_secs(2),
    )
    .unwrap();
    let control_address = server.local_addr().unwrap();
    let mut relay = (scenario == "control_eof").then(|| ControlRelay::new(control_address));
    let dial_address = relay
        .as_ref()
        .map_or(control_address, |relay| relay.address);
    let accept = thread::spawn(move || {
        let end = Instant::now() + BOUND;
        let channel = loop {
            assert!(Instant::now() < end, "bounded real control acceptance");
            if let Some(channel) = server.try_accept_until(end).unwrap() {
                break channel;
            }
            thread::sleep(TICK);
        };
        let mut link = CatalogControlLink::admit_accepted_new_peer(
            &mut source_runtime,
            channel,
            "Synthetic Android".into(),
        )
        .unwrap();
        let catalog = link.receive_catalog().unwrap();
        link.apply_catalog(&mut source_runtime, catalog).unwrap();
        let catalog = link
            .prepare_catalog(&source_runtime, &BTreeSet::from([source.capability_id]))
            .unwrap();
        link.send_catalog(catalog).unwrap();
        (source_runtime, link)
    });
    let channel = AuthenticatedControlClientChannel::connect_fresh_session(
        ControlTlsEndpoint {
            address: dial_address,
            io_timeout: Duration::from_secs(2),
        },
        "localhost",
        control_client,
        binding,
    )
    .unwrap();
    let mut sink_link = CatalogControlLink::admit_dialed_new_peer(
        &mut sink_runtime,
        channel,
        "Synthetic Linux".into(),
    )
    .unwrap();
    // This argument selects Source publication, not every local endpoint.
    // Sink-only capabilities remain visible without any Source selection.
    let catalog = sink_link
        .prepare_catalog(&sink_runtime, &BTreeSet::new())
        .unwrap();
    sink_link.send_catalog(catalog).unwrap();
    let catalog = sink_link.receive_catalog().unwrap();
    sink_link.apply_catalog(&mut sink_runtime, catalog).unwrap();
    let (mut source_runtime, mut source_link) = accept.join().unwrap();
    let session = source_link.session_id();
    assert_eq!(session, sink_link.session_id());
    let route = source_runtime
        .create_route(session, source, sink, RouteBackend::AdapterManaged)
        .unwrap();
    source_link
        .send_route_status(source_runtime.route(route).unwrap())
        .unwrap();
    let ReceivedControlMessage::RouteStatus(status) = sink_link
        .receive_control_message_until(Instant::now() + BOUND)
        .unwrap()
    else {
        panic!("authenticated pending stereo Draft required");
    };
    sink_link
        .apply_route_status(&mut sink_runtime, &status, false)
        .unwrap();
    pending(
        &root,
        &scenario,
        "before_source",
        &source_runtime,
        &sink_runtime,
    );
    wait_marker(&root, "source_checked");
    let consent = Duration::from_secs(if scenario == "expiry" { 8 } else { 12 });
    let source_original_end = Instant::now() + consent;
    let source_consent_ms = now_ms() + consent.as_millis() as u64;
    let grant_expiry = now_ms() + 20_000;
    source_runtime
        .authorize_route(route, Some(grant_expiry))
        .unwrap();
    assert_eq!(
        sink_runtime.route(route).unwrap().authorization,
        AuthorizationState::Pending
    );
    pending(
        &root,
        &scenario,
        "before_sink",
        &source_runtime,
        &sink_runtime,
    );
    wait_marker(&root, "sink_checked");
    let sink_original_end = Instant::now() + consent;
    let sink_consent_ms = now_ms() + consent.as_millis() as u64;
    sink_runtime
        .authorize_route(route, Some(grant_expiry))
        .unwrap();
    for runtime in [&mut source_runtime, &mut sink_runtime] {
        runtime
            .prepare_route(
                route,
                Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)),
                QosMode::Interactive,
                now_ms(),
            )
            .unwrap();
        assert!(
            runtime.snapshot().port_leases.is_empty(),
            "owners must acquire fresh one-shot claims"
        );
    }
    assert_eq!(
        source_runtime.route(route).unwrap(),
        sink_runtime.route(route).unwrap()
    );
    sink_runtime
        .set_local_port_lease_policy(sink, PortLeasePolicy::Exclusive)
        .unwrap();
    let stream = StreamId::new();
    let state = Rc::new(RefCell::new(SinkState {
        bytes: Vec::with_capacity(PCM_LIMIT),
        ..Default::default()
    }));
    let sink_admission = SecureStereoSinkAdmission {
        expected: sink_link.tls_binding(),
        route_id: route,
        stream_id: stream,
        local_address: "127.0.0.1".parse().unwrap(),
        lifetime: Duration::from_secs(20),
    };
    let mut sink_owner = SecureStereoSinkOwner::new_until(
        sink_runtime,
        sink_link,
        media_server,
        sink_admission,
        sink_original_end.min(source_original_end),
        TestSink {
            state: state.clone(),
            partial: scenario == "partial_write",
        },
    )
    .unwrap();
    let mut source_owner = make_source_owner(
        source_runtime,
        source_link,
        media_client,
        route,
        (!receiver_assigned).then_some(stream),
        source_original_end,
    );
    if receiver_assigned {
        assert_eq!(source_owner.selected_stream_id(), None);
    }
    let mut bound = None;
    let mut injected = None;
    let mut sink_bound = false;
    let mut pcm_enabled = false;
    let mut freeze_source = false;
    let mut terminal = None;
    let run_end = Instant::now() + Duration::from_secs(14);
    while terminal.is_none() {
        assert!(
            Instant::now() < run_end,
            "bounded actual secure stereo lifecycle"
        );
        if !freeze_source {
            match source_owner.poll() {
                Ok(Some(SecureStereoSourceEvent::Bound)) => {
                    assert!(bound.is_none());
                    assert_eq!(source_owner.selected_stream_id(), Some(stream));
                    let evidence = json!({"helper_pid":source_owner.helper_pid().unwrap(),
                        "source_lease_expires_at_ms":source_owner.source_lease_expires_at_ms().unwrap(),
                        "sink_lease_expires_at_ms":source_owner.sink_lease_expires_at_ms().unwrap(),
                        "source_consent_end_ms":source_consent_ms,"sink_consent_end_ms":sink_consent_ms,
                        "grant_expires_at_ms":grant_expiry});
                    publish(&root, &scenario, "bound", evidence.clone());
                    bound = Some(evidence);
                }
                Ok(_) => {}
                Err(error) => {
                    assert!(
                        injected.is_some(),
                        "Source failed before requested injection: {error}"
                    );
                    terminal = Some((
                        "source",
                        source_owner.terminal_cause().unwrap(),
                        source_owner.terminal_observed_at_ms().unwrap(),
                    ));
                }
            }
        }
        // Near original expiry keep the observer deterministic: the Source polls
        // through its actual exact pinned deadline. No synthetic deadline drives
        // production admission and no tolerance substitutes for that observation.
        let near_expiry = scenario == "expiry"
            && bound.as_ref().is_some_and(|value| {
                now_ms().saturating_add(40)
                    >= value["source_lease_expires_at_ms"]
                        .as_u64()
                        .unwrap()
                        .min(value["sink_lease_expires_at_ms"].as_u64().unwrap())
            });
        if terminal.is_none() && !near_expiry {
            match sink_owner.poll() {
                Ok(Some(SecureStereoSinkEvent::Bound)) => {
                    assert!(!sink_bound);
                    sink_bound = true;
                }
                Ok(Some(SecureStereoSinkEvent::Stopped)) => {
                    terminal = Some((
                        "sink",
                        sink_owner.terminal_cause().unwrap(),
                        sink_owner.terminal_observed_at_ms().unwrap(),
                    ));
                }
                Ok(_) => {}
                Err(error) => {
                    assert!(
                        injected.is_some(),
                        "Sink failed before requested injection: {error}"
                    );
                    terminal = Some((
                        "sink",
                        sink_owner.terminal_cause().unwrap(),
                        sink_owner.terminal_observed_at_ms().unwrap(),
                    ));
                }
            }
        }
        if terminal.is_some() {
            break;
        }
        // The real TLS binding and helper must exist first. Continue supervising
        // control and original finite authority while Python verifies the exact
        // helper and distinct channel ports, but before it creates either link.
        // Acknowledge the first unchanged 250 ms read permit, then Python creates
        // only the two already-verified links. Its later graph dump cannot build
        // a buffered startup burst because the real Sink is already consuming.
        if !pcm_enabled && sink_bound && bound.is_some() && marker(&root, "sink_ready").is_file() {
            assert_eq!(sink_owner.metrics().records_admitted, 0);
            assert_eq!(sink_owner.metrics().pcm_bytes_accepted, 0);
            assert!(state.borrow().bytes.is_empty());
            sink_owner.enable_pcm().unwrap();
            pcm_enabled = true;
            publish(
                &root,
                &scenario,
                "pcm_enabled",
                json!({
                    "helper_pid":source_owner.helper_pid().unwrap(),
                    "records_admitted":sink_owner.metrics().records_admitted,
                    "pcm_bytes_accepted":sink_owner.metrics().pcm_bytes_accepted,
                }),
            );
        }
        if let Some(evidence) = &bound {
            let source_expiry = source_owner.source_lease_expires_at_ms().unwrap();
            let sink_expiry = source_owner.sink_lease_expires_at_ms().unwrap();
            assert_eq!(sink_owner.source_lease_expires_at_ms(), Some(source_expiry));
            assert_eq!(sink_owner.sink_lease_expires_at_ms(), Some(sink_expiry));
            assert!(source_expiry <= source_consent_ms.min(sink_consent_ms));
            assert!(sink_expiry <= source_consent_ms.min(sink_consent_ms));
            assert_eq!(
                source_owner.runtime().route(route).unwrap().state,
                RouteState::Starting
            );
            assert_eq!(
                sink_owner.runtime().route(route).unwrap().state,
                RouteState::Starting
            );
            let target = if matches!(scenario.as_str(), "waveform" | "partial_write") {
                600
            } else {
                100
            };
            if injected.is_none()
                && marker(&root, "links_ready").is_file()
                && sink_owner.metrics().records_accepted >= target
            {
                injected = Some(now_ms());
                match scenario.as_str() {
                    "waveform" | "partial_write" => {
                        source_owner.stop().unwrap();
                        terminal = Some((
                            "source",
                            source_owner.terminal_cause().unwrap(),
                            source_owner.terminal_observed_at_ms().unwrap(),
                        ));
                    }
                    "stop" => {
                        source_owner.request_stop().unwrap();
                        freeze_source = true;
                    }
                    "source_revoke" => {
                        source_owner
                            .runtime_mut()
                            .release_route_port_lease(route, source, session)
                            .unwrap();
                    }
                    "sink_revoke" => {
                        sink_owner
                            .runtime_mut()
                            .release_route_port_lease(route, sink, session)
                            .unwrap();
                        freeze_source = true;
                    }
                    "control_eof" => {
                        relay.as_mut().expect("owned control-only relay").close();
                        freeze_source = true;
                    }
                    "media_eof" => {
                        source_owner.stop_handle().stop();
                        freeze_source = true;
                    }
                    "backpressure" => {
                        state.borrow_mut().blocked = true;
                    }
                    "expiry" => {
                        assert!(source_expiry.min(sink_expiry) < grant_expiry);
                    }
                    _ => unreachable!(),
                }
                assert_eq!(
                    source_expiry,
                    evidence["source_lease_expires_at_ms"].as_u64().unwrap()
                );
            }
        }
        thread::sleep(TICK);
    }
    assert!(pcm_enabled, "actual first-media permit required");
    let (endpoint, cause, observed) = terminal.unwrap();
    let mut result = bound.expect("actual authenticated capture helper required");
    exact_terminal(
        &scenario,
        endpoint,
        Some(cause),
        observed,
        injected.unwrap(),
        result["source_lease_expires_at_ms"]
            .as_u64()
            .unwrap()
            .min(result["source_consent_end_ms"].as_u64().unwrap())
            .min(result["sink_consent_end_ms"].as_u64().unwrap())
            .min(result["grant_expires_at_ms"].as_u64().unwrap()),
        result["sink_lease_expires_at_ms"].as_u64().unwrap(),
    );
    // Snapshot preserved terminal evidence before caller cleanup, never relabel a
    // cleanup failure as the requested Stop/revoke/EOF/expiry/backpressure cause.
    let source_metrics = source_owner.metrics();
    let sent = source_owner.packets_sent();
    let sink_metrics = sink_owner.metrics();
    source_owner.stop().unwrap();
    sink_owner.stop().unwrap();
    closed(source_owner.runtime(), session);
    closed(sink_owner.runtime(), session);
    if let Some(relay) = &mut relay {
        relay.close();
    }
    let helper = result["helper_pid"].as_u64().unwrap();
    assert!(!Path::new(&format!("/proc/{helper}")).exists());
    no_children();
    let before = (state.borrow().bytes.len(), state.borrow().writes);
    assert!(state.borrow().stopped);
    // Repeat terminal polls: neither API may write/capture after reconciliation.
    let _ = source_owner.poll();
    let _ = sink_owner.poll();
    assert_eq!(source_owner.metrics(), source_metrics);
    assert_eq!(source_owner.packets_sent(), sent);
    assert_eq!(sink_owner.metrics(), sink_metrics);
    assert_eq!((state.borrow().bytes.len(), state.borrow().writes), before);
    fs::write(
        root.join("received-secure-stereo.pcm"),
        &state.borrow().bytes,
    )
    .unwrap();
    assert_eq!(
        state.borrow().bytes.len() as u64,
        sink_metrics.pcm_bytes_accepted
    );
    result.as_object_mut().unwrap().extend(json!({"terminal_endpoint":endpoint,"terminal_cause":cause,
        "terminal_observed_at_ms":observed,"injected_at_ms":injected.unwrap(),
        "packets_captured":source_metrics.packets_captured,"capture_bytes_read":source_metrics.bytes_read,
        "source_records_sent":sent,"records_admitted":sink_metrics.records_admitted,
        "records_accepted":sink_metrics.records_accepted,"pcm_bytes_accepted":sink_metrics.pcm_bytes_accepted,
        "sink_write_calls":state.borrow().writes,"source_session_closed":true,"sink_session_closed":true,
        "leases_released":true,"helper_reaped":true,"sink_stopped":true,"post_stop_unchanged":true})
        .as_object().unwrap().clone());
    publish(&root, &scenario, "stopped", result);
    wait_marker(&root, "stop_checked");
}

// Separate shared-Runtime/ACK bridge proof. The reusable-owner fixture above is
// intentionally unchanged; neither marker nor scenario can select this test.
const JOB_PREFIX: &str = ".capyio-secure-stereo-job-";
const JOB_SCENARIOS: [&str; 2] = ["waveform", "partial_ack"];
const CONTROL_GENERATION: u64 = 17;
const JOB_GENERATION: u64 = 23;

fn job_marker(root: &Path, name: &str) -> PathBuf {
    root.join(format!("{JOB_PREFIX}{name}"))
}
fn publish_job(root: &Path, scenario: &str, stage: &str, extra: Value) {
    let mut value = json!({"version":1,"scenario":scenario,"stage":stage,
        "format":SECURE_AUDIO_STEREO_FORMAT,"surface":"shared_runtime_receive_job",
        "owner_pid":std::process::id()});
    value
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let destination = job_marker(root, &format!("{stage}.json"));
    let temporary = destination.with_extension("tmp");
    fs::write(&temporary, serde_json::to_vec(&value).unwrap()).unwrap();
    fs::rename(temporary, destination).unwrap();
}
fn wait_job_marker(root: &Path, name: &str) {
    let end = Instant::now() + BOUND;
    while !job_marker(root, name).is_file() {
        assert!(
            Instant::now() < end,
            "hosted receive-job observation {name} missing"
        );
        thread::sleep(TICK);
    }
}
fn job_pending(root: &Path, scenario: &str, stage: &str, source: &NodeRuntime, sink: &NodeRuntime) {
    no_children();
    assert!(source.snapshot().port_leases.is_empty() && sink.snapshot().port_leases.is_empty());
    publish_job(
        root,
        scenario,
        stage,
        json!({"helper_absent":true,"lease_count":0,"pcm_bytes_accepted":0}),
    );
}

#[test]
#[ignore = "requires explicit hosted-private-secure-stereo-job harness and real private PipeWire graph"]
fn hosted_secure_stereo_linux_capture_android_receive_job() {
    hosted_stereo_job(false);
}

#[test]
#[ignore = "requires explicit hosted-private-secure-stereo harness and real private PipeWire graph"]
fn hosted_receiver_assigned_secure_stereo_linux_capture_android_receive_job() {
    hosted_stereo_job(true);
}

fn hosted_stereo_job(receiver_assigned: bool) {
    assert_eq!(
        std::env::var("CAPYIO_SECURE_STEREO_JOB_HOSTED").as_deref(),
        Ok("1")
    );
    assert_eq!(std::env::var("CAPYIO_PIPEWIRE_TEST").as_deref(), Ok("1"));
    assert_eq!(
        std::env::var("PIPEWIRE_REMOTE").as_deref(),
        Ok("capyio-test")
    );
    let root = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap());
    assert_eq!(
        fs::read_to_string(root.join(".capyio-pipewire-capture")).unwrap(),
        "capyio-hosted-pipewire-capture-v1\n"
    );
    assert_eq!(
        fs::read_to_string(root.join(".capyio-secure-stereo-job")).unwrap(),
        "capyio-hosted-secure-stereo-job-v1\n"
    );
    let scenario = std::env::var("CAPYIO_SECURE_STEREO_JOB_SCENARIO").unwrap();
    assert!(JOB_SCENARIOS.contains(&scenario.as_str()));
    let (mut source_runtime, source) = runtime(Platform::Linux, PortDirection::Source);
    let (mut sink_runtime, sink) = runtime(Platform::Android, PortDirection::Sink);
    let (control_server, media_client, control_client, media_server) = identities();
    let binding = ControlTlsNodeBinding::new(source.node_id, sink.node_id).unwrap();
    let server = AuthenticatedControlServer::from_listener_fresh_session(
        TcpListener::bind("127.0.0.1:0").unwrap(),
        control_server,
        binding,
        Duration::from_secs(2),
    )
    .unwrap();
    let control_address = server.local_addr().unwrap();
    let accept = thread::spawn(move || {
        let end = Instant::now() + BOUND;
        let channel = loop {
            assert!(Instant::now() < end, "bounded real control acceptance");
            if let Some(channel) = server.try_accept_until(end).unwrap() {
                break channel;
            }
            thread::sleep(TICK);
        };
        let mut link = CatalogControlLink::admit_accepted_new_peer(
            &mut source_runtime,
            channel,
            "Synthetic Android".into(),
        )
        .unwrap();
        let catalog = link.receive_catalog().unwrap();
        link.apply_catalog(&mut source_runtime, catalog).unwrap();
        let catalog = link
            .prepare_catalog(&source_runtime, &BTreeSet::from([source.capability_id]))
            .unwrap();
        link.send_catalog(catalog).unwrap();
        (source_runtime, link)
    });
    let channel = AuthenticatedControlClientChannel::connect_fresh_session(
        ControlTlsEndpoint {
            address: control_address,
            io_timeout: Duration::from_secs(2),
        },
        "localhost",
        control_client,
        binding,
    )
    .unwrap();
    let mut sink_link = CatalogControlLink::admit_dialed_new_peer(
        &mut sink_runtime,
        channel,
        "Synthetic Linux".into(),
    )
    .unwrap();
    // This argument selects Source publication, not every local endpoint.
    // Sink-only capabilities remain visible without any Source selection.
    let catalog = sink_link
        .prepare_catalog(&sink_runtime, &BTreeSet::new())
        .unwrap();
    sink_link.send_catalog(catalog).unwrap();
    let catalog = sink_link.receive_catalog().unwrap();
    sink_link.apply_catalog(&mut sink_runtime, catalog).unwrap();
    let (mut source_runtime, mut source_link) = accept.join().unwrap();
    let session = source_link.session_id();
    assert_eq!(session, sink_link.session_id());
    let route = source_runtime
        .create_route(session, source, sink, RouteBackend::AdapterManaged)
        .unwrap();
    source_link
        .send_route_status(source_runtime.route(route).unwrap())
        .unwrap();
    let ReceivedControlMessage::RouteStatus(status) = sink_link
        .receive_control_message_until(Instant::now() + BOUND)
        .unwrap()
    else {
        panic!("authenticated pending stereo Draft required");
    };
    sink_link
        .apply_route_status(&mut sink_runtime, &status, false)
        .unwrap();
    job_pending(
        &root,
        &scenario,
        "before_source",
        &source_runtime,
        &sink_runtime,
    );
    wait_job_marker(&root, "source_checked");
    let consent = Duration::from_secs(12);
    let source_original_end = Instant::now() + consent;
    let source_consent_ms = now_ms() + consent.as_millis() as u64;
    let grant_expiry = now_ms() + 20_000;
    source_runtime
        .authorize_route(route, Some(grant_expiry))
        .unwrap();
    assert_eq!(
        sink_runtime.route(route).unwrap().authorization,
        AuthorizationState::Pending
    );
    job_pending(
        &root,
        &scenario,
        "before_sink",
        &source_runtime,
        &sink_runtime,
    );
    wait_job_marker(&root, "sink_checked");
    let sink_original_end = Instant::now() + consent;
    let sink_consent_ms = now_ms() + consent.as_millis() as u64;
    sink_runtime
        .authorize_route(route, Some(grant_expiry))
        .unwrap();
    for runtime in [&mut source_runtime, &mut sink_runtime] {
        runtime
            .prepare_route(
                route,
                Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)),
                QosMode::Interactive,
                now_ms(),
            )
            .unwrap();
        assert!(
            runtime.snapshot().port_leases.is_empty(),
            "owners must acquire fresh one-shot claims"
        );
    }
    assert_eq!(
        source_runtime.route(route).unwrap(),
        sink_runtime.route(route).unwrap()
    );
    sink_runtime
        .set_local_port_lease_policy(sink, PortLeasePolicy::Exclusive)
        .unwrap();

    let stream = StreamId::new();
    // The media identity is the counterpart of the exact credentials used for
    // this admitted control connection, never a cached role-specific identity.
    assert_eq!(
        media_server.expected_peer_certificate,
        sink_link.authenticated_peer_certificate().unwrap()
    );
    let control_binding = sink_link.tls_binding();
    let runtime = Arc::new(Mutex::new(sink_runtime));
    let control_live = Arc::new(AtomicBool::new(true));
    let gate = StereoAdmissionGate::new();
    let mut source_owner = make_source_owner(
        source_runtime,
        source_link,
        media_client,
        route,
        (!receiver_assigned).then_some(stream),
        source_original_end,
    );
    if receiver_assigned {
        assert_eq!(source_owner.selected_stream_id(), None);
    }
    let (mut negotiation, request) = {
        let guard = runtime.lock().unwrap();
        StereoSinkNegotiation::prepare(
            &guard,
            &sink_link,
            SecureStereoSinkAdmission {
                expected: control_binding,
                route_id: route,
                stream_id: stream,
                local_address: "127.0.0.1".parse().unwrap(),
                lifetime: Duration::from_secs(20),
            },
            sink_original_end.min(source_original_end),
            CONTROL_GENERATION,
            now_ms(),
            Instant::now(),
        )
        .unwrap()
    };
    // Prepared control action is consume-once; no Runtime guard crosses send.
    assert!(runtime.try_lock().is_ok());
    let sent = sink_link
        .send_prepared_port_lease_request_until(request, negotiation.operation_deadline())
        .unwrap();
    {
        let guard = runtime.lock().unwrap();
        negotiation
            .validate(
                &guard,
                &sink_link,
                CONTROL_GENERATION,
                now_ms(),
                Instant::now(),
            )
            .unwrap();
        negotiation.commit_source_request(sent).unwrap();
    }
    let mut job: Option<Arc<StereoReceiveJob>> = None;
    let mut identity = Some(media_server);
    let mut bound = None;
    let mut enabled = false;
    let state = Rc::new(RefCell::new(SinkState {
        bytes: Vec::with_capacity(PCM_LIMIT),
        ..Default::default()
    }));
    let mut output = TestSink {
        state: state.clone(),
        partial: scenario == "partial_ack",
    };
    let mut buffer = [0; 1920];
    let mut pending_ack = None;
    let mut awaiting_confirmation = false;
    let mut read_permits = 0_u64;
    let mut copied_bytes = 0_u64;
    let mut acknowledgements = 0_u64;
    let mut busy_ack_retries = 0_u64;
    let mut partial_writes = 0_u64;
    let run_end = Instant::now() + Duration::from_secs(14);
    loop {
        assert!(
            Instant::now() < run_end,
            "bounded actual receive-job capture proof"
        );
        match source_owner
            .poll()
            .expect("real Source must remain authorized until explicit Stop")
        {
            Some(SecureStereoSourceEvent::Bound) => {
                assert!(bound.is_none());
                assert_eq!(source_owner.selected_stream_id(), Some(stream));
                let evidence = json!({"helper_pid":source_owner.helper_pid().unwrap(),
                    "source_lease_expires_at_ms":source_owner.source_lease_expires_at_ms().unwrap(),
                    "sink_lease_expires_at_ms":source_owner.sink_lease_expires_at_ms().unwrap(),
                    "source_consent_end_ms":source_consent_ms,"sink_consent_end_ms":sink_consent_ms,
                    "grant_expires_at_ms":grant_expiry,"control_generation":CONTROL_GENERATION,
                    "job_generation":JOB_GENERATION,"route_state":"Starting"});
                publish_job(&root, &scenario, "bound", evidence.clone());
                bound = Some(evidence);
            }
            Some(SecureStereoSourceEvent::Stopped) => panic!("unexpected Source Stop"),
            _ => {}
        }
        // Network/control polling never borrows the authoritative Runtime.
        let deadline = job.as_ref().map_or_else(
            || negotiation.operation_deadline(),
            |job| {
                job.snapshot()
                    .monotonic_deadline()
                    .min(Instant::now() + Duration::from_millis(250))
            },
        );
        let message = match sink_link.poll_control_message_until(deadline) {
            Ok(message) => message,
            Err(error) => {
                gate.invalidate();
                control_live.store(false, Ordering::Release);
                if let Some(job) = &job {
                    job.fail();
                }
                panic!("unexpected current control failure: {error}");
            }
        };
        if let Some(message) = message {
            assert!(
                job.is_none(),
                "unexpected control mutation after exact job admission"
            );
            let action = {
                let mut guard = runtime.lock().unwrap();
                negotiation
                    .accept_control(
                        &mut guard,
                        &mut sink_link,
                        message,
                        CONTROL_GENERATION,
                        now_ms(),
                        Instant::now(),
                    )
                    .unwrap()
            };
            match action {
                StereoNegotiationAction::SendLeaseResponse(response) => {
                    assert!(runtime.try_lock().is_ok());
                    sink_link
                        .send_prepared_port_lease_response_until(
                            response,
                            negotiation.operation_deadline(),
                        )
                        .unwrap();
                    let guard = runtime.lock().unwrap();
                    negotiation
                        .validate(
                            &guard,
                            &sink_link,
                            CONTROL_GENERATION,
                            now_ms(),
                            Instant::now(),
                        )
                        .unwrap();
                    negotiation.commit_sink_response().unwrap();
                }
                StereoNegotiationAction::None => {}
                StereoNegotiationAction::PeerStop => panic!("unexpected negotiation Stop"),
            }
        }
        if job.is_none() && negotiation.ready() {
            let snapshot = {
                let mut guard = runtime.lock().unwrap();
                negotiation
                    .finish(
                        &mut guard,
                        &sink_link,
                        CONTROL_GENERATION,
                        now_ms(),
                        Instant::now(),
                    )
                    .unwrap()
            };
            let shared = Arc::clone(&runtime);
            let live = Arc::clone(&control_live);
            let authority: StereoAuthority = Arc::new(move |snapshot| {
                let guard = shared.lock().unwrap();
                if !live.load(Ordering::Acquire) {
                    return Err("original control generation closed".into());
                }
                snapshot.validate(&guard, CONTROL_GENERATION, now_ms(), Instant::now())
            });
            let shared = Arc::clone(&runtime);
            let live = Arc::clone(&control_live);
            let failure = Arc::new(move || {
                live.store(false, Ordering::Release);
                shared.lock().unwrap().close_session(session).unwrap();
            });
            assert!(runtime.try_lock().is_ok());
            let current = StereoReceiveJob::start(
                7,
                JOB_GENERATION,
                snapshot,
                identity.take().unwrap(),
                CONTROL_GENERATION,
                gate.clone(),
                authority,
                failure,
            )
            .unwrap();
            assert_eq!(current.state(), JOB_PENDING);
            sink_link
                .send_secure_stereo_audio_data_offer_until(
                    current.snapshot().route(),
                    current.offer(),
                    now_ms(),
                    current.operation_deadline(),
                )
                .unwrap();
            // This callback performs the fresh shared-Runtime validation only
            // after successful publication and outside every Runtime guard.
            current.offer_committed().unwrap();
            job = Some(current);
        }
        if let Some(current) = &job {
            assert!(matches!(
                current.state(),
                JOB_PENDING | JOB_BOUND | JOB_ENABLED
            ));
            let guard = runtime.lock().unwrap();
            current
                .reconcile(&guard, CONTROL_GENERATION, now_ms())
                .unwrap();
            assert_eq!(guard.route(route).unwrap().state, RouteState::Starting);
            assert_eq!(
                guard.route(route).unwrap(),
                source_owner.runtime().route(route).unwrap()
            );
            let sink_claim = guard.route_port_lease_claim(route, sink).unwrap();
            assert_eq!(
                Some(sink_claim.expires_at_ms),
                source_owner.sink_lease_expires_at_ms()
            );
            drop(guard);
            if !enabled
                && current.state() == JOB_BOUND
                && bound.is_some()
                && job_marker(&root, "sink_ready").is_file()
            {
                assert_eq!(current.metrics().records_admitted, 0);
                assert_eq!(current.metrics().pcm_bytes_accepted, 0);
                assert!(state.borrow().bytes.is_empty());
                current.enable().unwrap();
                enabled = true;
                publish_job(
                    &root,
                    &scenario,
                    "pcm_enabled",
                    json!({
                    "helper_pid":source_owner.helper_pid().unwrap(),"records_admitted":0,
                    "pcm_bytes_accepted":0,"job_generation":JOB_GENERATION}),
                );
            }
            if enabled {
                if awaiting_confirmation {
                    let actual = state.borrow().bytes.len() as u64;
                    assert!(current.accepted_bytes() <= actual);
                    if current.accepted_bytes() == actual {
                        awaiting_confirmation = false;
                    }
                }
                if !awaiting_confirmation && pending_ack.is_none() {
                    let accepted = current.accepted_bytes();
                    if current.metrics().records_accepted >= 600
                        && job_marker(&root, "links_ready").is_file()
                    {
                        assert_eq!(accepted, state.borrow().bytes.len() as u64);
                        break;
                    }
                    if let Some(permit) = current.try_read(JOB_GENERATION, &mut buffer).unwrap() {
                        assert_eq!(permit.generation, JOB_GENERATION);
                        assert!(permit.token > 0 && permit.length > 0);
                        assert_eq!(permit.offset + permit.length, 1920);
                        assert_eq!(permit.offset % 4, 0);
                        // Exposure alone is neither a write nor an acceptance.
                        assert_eq!(current.accepted_bytes(), accepted);
                        assert_eq!(state.borrow().bytes.len() as u64, accepted);
                        read_permits += 1;
                        copied_bytes += permit.length as u64;
                        let written = output.try_write_pcm(&buffer[..permit.length]).unwrap();
                        assert!(written > 0 && written <= permit.length && written % 4 == 0);
                        partial_writes += u64::from(written < permit.length);
                        pending_ack = Some((permit, written));
                    }
                }
                if let Some((permit, written)) = pending_ack {
                    // A busy ACK retries this exact token/count, not the write.
                    if current
                        .acknowledge(permit.generation, permit.token, permit.offset, written)
                        .unwrap()
                    {
                        pending_ack = None;
                        acknowledgements += 1;
                        awaiting_confirmation = true;
                    } else {
                        busy_ack_retries += 1;
                    }
                }
            }
        }
        thread::sleep(TICK);
    }
    let job = job.unwrap();
    assert!(enabled && pending_ack.is_none() && !awaiting_confirmation);
    assert_eq!(job.accepted_bytes(), state.borrow().bytes.len() as u64);
    assert_eq!(read_permits, state.borrow().writes);
    assert_eq!(acknowledgements, state.borrow().writes);
    let mut result = bound.expect("actual authenticated receive-job capture helper required");
    let source_expiry = result["source_lease_expires_at_ms"].as_u64().unwrap();
    let sink_expiry = result["sink_lease_expires_at_ms"].as_u64().unwrap();
    assert!(source_expiry <= source_consent_ms.min(sink_consent_ms));
    assert!(sink_expiry <= source_consent_ms.min(sink_consent_ms));
    let stopped_at = now_ms();
    assert!(stopped_at < source_expiry.min(sink_expiry));
    // Metrics have caught up to all actual writes before admission closes. No
    // incidental EOF or coarse worker failure is relabelled as a lifecycle cause.
    job.stop();
    assert_eq!(job.state(), JOB_STOPPED);
    let worker_end = Instant::now() + Duration::from_millis(250);
    while !job.worker_exited() {
        assert!(
            Instant::now() < worker_end,
            "stereo worker must stop within original operation budget"
        );
        thread::sleep(TICK);
    }
    let sink_metrics = job.metrics();
    assert_eq!(
        sink_metrics.pcm_bytes_accepted,
        state.borrow().bytes.len() as u64
    );
    job.reap_finished();
    assert!(job.finished() && job.worker_exited());
    let source_metrics = source_owner.metrics();
    let sent = source_owner.packets_sent();
    source_owner.stop().unwrap();
    output.stop().unwrap();
    // Close the generation before publishing the shared Runtime mutation.
    control_live.store(false, Ordering::Release);
    gate.invalidate();
    sink_link.close();
    runtime.lock().unwrap().close_session(session).unwrap();
    closed(source_owner.runtime(), session);
    closed(&runtime.lock().unwrap(), session);
    let helper = result["helper_pid"].as_u64().unwrap();
    assert!(!Path::new(&format!("/proc/{helper}")).exists());
    no_children();
    let before = (state.borrow().bytes.len(), state.borrow().writes);
    assert!(state.borrow().stopped);
    assert!(job.try_read(JOB_GENERATION, &mut buffer).is_err());
    assert_eq!(job.state(), JOB_STOPPED);
    assert_eq!(job.metrics(), sink_metrics);
    assert_eq!(source_owner.metrics(), source_metrics);
    assert_eq!(source_owner.packets_sent(), sent);
    assert_eq!((state.borrow().bytes.len(), state.borrow().writes), before);
    fs::write(
        root.join("received-secure-stereo-job.pcm"),
        &state.borrow().bytes,
    )
    .unwrap();
    result.as_object_mut().unwrap().extend(json!({
        "terminal_state":"Stopped","stopped_at_ms":stopped_at,
        "packets_captured":source_metrics.packets_captured,"capture_bytes_read":source_metrics.bytes_read,
        "source_records_sent":sent,"records_admitted":sink_metrics.records_admitted,
        "records_accepted":sink_metrics.records_accepted,"pcm_bytes_accepted":sink_metrics.pcm_bytes_accepted,
        "actual_sink_bytes":state.borrow().bytes.len(),"sink_write_calls":state.borrow().writes,
        "read_permits":read_permits,"pcm_bytes_copied":copied_bytes,"acknowledgements":acknowledgements,
        "busy_ack_retries":busy_ack_retries,"partial_writes":partial_writes,
        "copy_without_acceptance_checked":true,"worker_ack_confirmed":true,
        "source_session_closed":true,"sink_session_closed":true,"leases_released":true,
        "helper_reaped":true,"sink_stopped":true,"worker_reaped":true,"post_stop_unchanged":true
    }).as_object().unwrap().clone());
    publish_job(&root, &scenario, "stopped", result);
    wait_job_marker(&root, "stop_checked");
}

/// `None` is a typed receiver-assigned admission, never a dummy StreamId.
fn make_source_owner(
    runtime: NodeRuntime,
    link: CatalogControlLink,
    identity: ControlTlsClientIdentity,
    route_id: RouteId,
    strict_stream: Option<StreamId>,
    original_end: Instant,
) -> SecureStereoSourceOwner {
    let expected = link.tls_binding();
    match strict_stream {
        Some(stream_id) => SecureStereoSourceOwner::new_until(
            runtime,
            link,
            identity,
            "localhost".into(),
            SecureStereoSourceAdmission {
                expected,
                route_id,
                stream_id,
                lifetime: Duration::from_secs(20),
                pipewire_target: "capyio.test.stereo.source".into(),
            },
            original_end,
        ),
        None => SecureStereoSourceOwner::new_receiver_assigned_until(
            runtime,
            link,
            identity,
            "localhost".into(),
            SecureStereoReceiverAssignedSourceAdmission {
                expected,
                route_id,
                lifetime: Duration::from_secs(20),
                pipewire_target: "capyio.test.stereo.source".into(),
            },
            original_end,
        ),
    }
    .unwrap()
}

/// Real authenticated control/TLS fixture with independently generated receiver
/// IDs. No PipeWire process is permitted: these tests stop before media Bound.
struct WireSource {
    owner: SecureStereoSourceOwner,
    sink: CatalogControlLink,
    route: Route,
}
impl WireSource {
    fn new(strict: Option<StreamId>) -> Self {
        let (mut source_runtime, source) = runtime(Platform::Linux, PortDirection::Source);
        let (mut sink_runtime, sink) = runtime(Platform::Android, PortDirection::Sink);
        let (control_server, media_client, control_client, _media_server) = identities();
        let binding = ControlTlsNodeBinding::new(source.node_id, sink.node_id).unwrap();
        let server = AuthenticatedControlServer::from_listener_fresh_session(
            TcpListener::bind("127.0.0.1:0").unwrap(),
            control_server,
            binding,
            Duration::from_secs(2),
        )
        .unwrap();
        let control_address = server.local_addr().unwrap();
        let accept = thread::spawn(move || {
            let end = Instant::now() + BOUND;
            let channel = loop {
                assert!(Instant::now() < end, "bounded real control acceptance");
                if let Some(channel) = server.try_accept_until(end).unwrap() {
                    break channel;
                }
                thread::sleep(TICK);
            };
            let mut link = CatalogControlLink::admit_accepted_new_peer(
                &mut source_runtime,
                channel,
                "Synthetic Android".into(),
            )
            .unwrap();
            let catalog = link.receive_catalog().unwrap();
            link.apply_catalog(&mut source_runtime, catalog).unwrap();
            let catalog = link
                .prepare_catalog(&source_runtime, &BTreeSet::from([source.capability_id]))
                .unwrap();
            link.send_catalog(catalog).unwrap();
            (source_runtime, link)
        });
        let channel = AuthenticatedControlClientChannel::connect_fresh_session(
            ControlTlsEndpoint {
                address: control_address,
                io_timeout: Duration::from_secs(2),
            },
            "localhost",
            control_client,
            binding,
        )
        .unwrap();
        let mut sink_link = CatalogControlLink::admit_dialed_new_peer(
            &mut sink_runtime,
            channel,
            "Synthetic Linux".into(),
        )
        .unwrap();
        // This argument selects Source publication, not every local endpoint.
        // Sink-only capabilities remain visible without any Source selection.
        let catalog = sink_link
            .prepare_catalog(&sink_runtime, &BTreeSet::new())
            .unwrap();
        sink_link.send_catalog(catalog).unwrap();
        let catalog = sink_link.receive_catalog().unwrap();
        sink_link.apply_catalog(&mut sink_runtime, catalog).unwrap();
        let (mut source_runtime, mut source_link) = accept.join().unwrap();
        let session = source_link.session_id();
        assert_eq!(session, sink_link.session_id());
        let route = source_runtime
            .create_route(session, source, sink, RouteBackend::AdapterManaged)
            .unwrap();
        source_link
            .send_route_status(source_runtime.route(route).unwrap())
            .unwrap();
        let ReceivedControlMessage::RouteStatus(status) = sink_link
            .receive_control_message_until(Instant::now() + BOUND)
            .unwrap()
        else {
            panic!("authenticated pending stereo Draft required");
        };
        sink_link
            .apply_route_status(&mut sink_runtime, &status, false)
            .unwrap();

        let grant_expiry = now_ms() + 20_000;
        for runtime in [&mut source_runtime, &mut sink_runtime] {
            runtime.authorize_route(route, Some(grant_expiry)).unwrap();
            runtime
                .prepare_route(
                    route,
                    Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)),
                    QosMode::Interactive,
                    now_ms(),
                )
                .unwrap();
        }
        sink_runtime
            .set_local_port_lease_policy(sink, PortLeasePolicy::Exclusive)
            .unwrap();
        let mut owner = make_source_owner(
            source_runtime,
            source_link,
            media_client,
            route,
            strict,
            Instant::now() + Duration::from_secs(10),
        );
        let deadline = Instant::now() + Duration::from_millis(200);
        let ReceivedControlMessage::PortLeaseRequest(request) =
            sink_link.receive_control_message_until(deadline).unwrap()
        else {
            panic!("one authenticated Sink Acquire required")
        };
        sink_link
            .answer_port_lease_request_until(&mut sink_runtime, request, now_ms(), deadline)
            .unwrap();
        let request_id = sink_link
            .send_port_lease_request_until(
                route,
                source,
                capyio_protocol::PortLeaseAction::Acquire,
                5_000,
                deadline,
            )
            .unwrap();
        let mut response_seen = false;
        let mut start_seen = false;
        while !start_seen {
            assert!(
                Instant::now() < deadline,
                "bounded original lease negotiation"
            );
            owner.poll().unwrap();
            match sink_link.poll_control_message_until(deadline).unwrap() {
                Some(ReceivedControlMessage::PortLeaseResponse(response)) => {
                    assert!(!response_seen);
                    assert_eq!(response.request_message_id, request_id);
                    assert_eq!(
                        response.outcome,
                        capyio_protocol::PortLeaseOutcome::Acquired
                    );
                    response_seen = true;
                }
                Some(ReceivedControlMessage::RouteCommand(command)) => {
                    assert_eq!(command.action, capyio_protocol::RouteCommandAction::Start);
                    assert_eq!(command.route_id, route);
                    assert!(response_seen);
                    start_seen = true;
                }
                None => thread::sleep(TICK),
                _ => panic!("unexpected authenticated negotiation message"),
            }
        }
        let route = owner.runtime().route(route).unwrap().clone();
        assert_eq!(route.state, RouteState::Starting);
        assert!(owner.source_lease_expires_at_ms().is_some());
        assert!(owner.sink_lease_expires_at_ms().is_some());
        assert_eq!(owner.helper_pid(), None);
        Self {
            owner,
            sink: sink_link,
            route,
        }
    }
    fn send_offer(&mut self, listener: &TcpListener, stream_id: StreamId) {
        self.sink
            .send_secure_stereo_audio_data_offer_until(
                &self.route,
                capyio_protocol::SecureAudioDataOffer {
                    port: listener.local_addr().unwrap().port(),
                    stream_id,
                    transport: capyio_protocol::SecureAudioTransport::Tls13MtlsStereoV1,
                },
                now_ms(),
                Instant::now() + Duration::from_millis(100),
            )
            .unwrap();
    }
    fn consume_first(&mut self, stream: StreamId) {
        let end = Instant::now() + Duration::from_millis(150);
        while self.owner.selected_stream_id().is_none() {
            assert!(Instant::now() < end, "authenticated offer was not consumed");
            assert_ne!(
                self.owner.poll().unwrap(),
                Some(SecureStereoSourceEvent::Bound)
            );
            thread::sleep(TICK);
        }
        assert_eq!(self.owner.selected_stream_id(), Some(stream));
        assert_eq!(self.owner.helper_pid(), None);
        assert_eq!(self.owner.packets_sent(), 0);
    }
    fn terminal(&mut self) -> io::Error {
        let end = Instant::now() + Duration::from_secs(1);
        loop {
            assert!(Instant::now() < end, "bounded terminal observation");
            match self.owner.poll() {
                Err(reason) => return reason,
                Ok(event) => assert_ne!(event, Some(SecureStereoSourceEvent::Bound)),
            }
            assert_eq!(self.owner.helper_pid(), None);
            thread::sleep(TICK);
        }
    }
    fn assert_terminal(&mut self, stream: Option<StreamId>, cause: &str) {
        assert!(self.owner.is_closed());
        assert_eq!(self.owner.terminal_cause(), Some(cause));
        assert_eq!(self.owner.selected_stream_id(), stream);
        assert_eq!(self.owner.helper_pid(), None);
        assert_eq!(self.owner.packets_sent(), 0);
        closed(self.owner.runtime(), self.route.session_id);
        assert!(self.owner.poll().is_err());
        assert_eq!(self.owner.terminal_cause(), Some(cause));
        assert_eq!(self.owner.selected_stream_id(), stream);
    }
}
fn wire_listener() -> TcpListener {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    listener
}
fn accept_pending(listener: &TcpListener) -> TcpStream {
    let end = Instant::now() + Duration::from_millis(100);
    loop {
        match listener.accept() {
            Ok((socket, _)) => return socket,
            Err(reason) if reason.kind() == io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < end, "media TCP connect missing");
                thread::sleep(TICK);
            }
            Err(reason) => panic!("media accept failed: {reason}"),
        }
    }
}

#[test]
fn wire_receiver_assigned_learns_authenticated_offer() {
    let mut fixture = WireSource::new(None);
    assert_eq!(fixture.owner.selected_stream_id(), None);
    let receiver_stream = StreamId::new();
    let listener = wire_listener();
    fixture.send_offer(&listener, receiver_stream);
    fixture.consume_first(receiver_stream);
    let _pending = accept_pending(&listener);
    // Selection alone is not media Bound, Active, capture, or audibility.
    assert_eq!(
        fixture
            .owner
            .runtime()
            .route(fixture.route.id)
            .unwrap()
            .state,
        RouteState::Starting
    );
    fixture.owner.stop().unwrap();
    fixture.assert_terminal(Some(receiver_stream), "local_stop");
}

#[test]
fn wire_receiver_assigned_strict_constructor_rejects_independent_id() {
    let strict = StreamId::new();
    let mut fixture = WireSource::new(Some(strict));
    let receiver_stream = StreamId::new();
    assert_ne!(strict, receiver_stream);
    let listener = wire_listener();
    fixture.send_offer(&listener, receiver_stream);
    let reason = fixture.terminal();
    assert_eq!(
        reason.to_string(),
        "stereo offer precedes exact authority or is repeated"
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    fixture.assert_terminal(None, "failure");
}

#[test]
fn wire_receiver_assigned_failed_media_tls_is_terminal() {
    let mut fixture = WireSource::new(None);
    let stream = StreamId::new();
    let listener = wire_listener();
    fixture.send_offer(&listener, stream);
    fixture.consume_first(stream);
    let mut invalid_tls = accept_pending(&listener);
    invalid_tls
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let mut hello = [0; 4096];
    assert!(invalid_tls.read(&mut hello).unwrap() > 0);
    // Malformed TLS handshake record, not a mere unaccepted-listener timeout.
    invalid_tls
        .write_all(&[22, 3, 3, 0, 4, 255, 0, 0, 0])
        .unwrap();
    let reason = fixture.terminal();
    assert_eq!(reason.kind(), io::ErrorKind::InvalidData);
    assert!(
        reason
            .to_string()
            .contains("received unexpected handshake message")
    );
    assert!(reason.to_string().contains("0xff"));
    fixture.assert_terminal(Some(stream), "media_failure");
}

#[test]
fn wire_receiver_assigned_duplicate_offer_cannot_replace_first() {
    for changed in [false, true] {
        let mut fixture = WireSource::new(None);
        let first = StreamId::new();
        let listener = wire_listener();
        fixture.send_offer(&listener, first);
        fixture.consume_first(first);
        let _pending = accept_pending(&listener);
        let second = if changed { StreamId::new() } else { first };
        fixture.send_offer(&listener, second);
        let reason = fixture.terminal();
        assert!(
            !reason.to_string().contains("deadline"),
            "duplicate is not timeout evidence"
        );
        fixture.assert_terminal(Some(first), "control_failure");
    }
}

#[test]
fn wire_receiver_assigned_source_lease_loss_rejects_offer() {
    let mut fixture = WireSource::new(None);
    let listener = wire_listener();
    fixture
        .owner
        .runtime_mut()
        .release_route_port_lease(
            fixture.route.id,
            fixture.route.source,
            fixture.route.session_id,
        )
        .unwrap();
    fixture.send_offer(&listener, StreamId::new());
    let reason = fixture.terminal();
    assert_eq!(
        reason.to_string(),
        "original stereo Source claim changed or absent"
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    fixture.assert_terminal(None, "source_revoked");
}

#[test]
fn wire_receiver_assigned_pending_handshake_keeps_deadline() {
    let mut fixture = WireSource::new(None);
    let listener = wire_listener();
    let stream = StreamId::new();
    let sent_at = Instant::now();
    fixture.send_offer(&listener, stream);
    fixture.consume_first(stream);
    let _pending = accept_pending(&listener);
    let claims = (
        fixture.owner.source_lease_expires_at_ms(),
        fixture.owner.sink_lease_expires_at_ms(),
    );
    thread::sleep(Duration::from_millis(150));
    fixture
        .sink
        .send_heartbeat_until(91, false, Instant::now() + Duration::from_millis(50))
        .unwrap();
    // Wait for actual authenticated heartbeat consumption, not one nonblocking poll.
    let heartbeat_end = Instant::now() + Duration::from_millis(70);
    loop {
        assert!(
            Instant::now() < heartbeat_end,
            "pending-handshake heartbeat was not acknowledged"
        );
        fixture.owner.poll().unwrap();
        if let Some(ReceivedControlMessage::Heartbeat(heartbeat)) = fixture
            .sink
            .poll_control_message_until(heartbeat_end)
            .unwrap()
        {
            assert!(heartbeat.acknowledgement);
            assert_eq!(heartbeat.sequence, 91);
            break;
        }
        thread::sleep(TICK);
    }
    let reason = fixture.terminal();
    assert!(matches!(
        reason.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::Other
    ));
    assert!(
        sent_at.elapsed() < Duration::from_millis(380),
        "heartbeat refreshed original 250 ms operation budget"
    );
    assert_eq!(
        (
            fixture.owner.source_lease_expires_at_ms(),
            fixture.owner.sink_lease_expires_at_ms()
        ),
        claims
    );
    fixture.assert_terminal(Some(stream), "media_timeout");
}
