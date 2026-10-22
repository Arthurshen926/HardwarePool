#![cfg(target_os = "linux")]
//! Hosted synthetic acceptance of reusable Android Rust sender and Linux owner
//! APIs. The original nine-case gate is NOT CLI, stored-pairing or keyring
//! evidence. The additive secure_console module separately drives the actual
//! paired production console. Neither gate proves Java AudioRecord or physical
//! devices. Their Python harnesses own private PipeWire servers, independently
//! observe consent, link exact owned pw-cat children, and verify PCM/cleanup.
use capyio_android_jni::audio_sender::{AudioSourceConsent, SecureAudioSender};
use capyio_audio::{AudioMediaPacket, AudioMediaStreamBinding, AudioStreamSpec};
use capyio_control_credentials::ControlCredentialBundle;
use capyio_control_link::{CatalogControlLink, ReceivedControlMessage};
use capyio_control_tls::{
    AuthenticatedControlClientChannel, AuthenticatedControlServer, ControlTlsClientIdentity,
    ControlTlsEndpoint, ControlTlsNodeBinding, ControlTlsServerIdentity,
};
use capyio_core::*;
use capyio_linux_audio::{
    PlaybackMetrics, SecurePlaybackAdmission, SecurePlaybackEvent, SecurePlaybackOwner,
};
use capyio_native_audio_lan::SecureMonoPcmAdmission;
use capyio_pairing::{
    CertificateFingerprint, CredentialHandle, PairedPeerDirectory, PairedPeerRecord,
};
use capyio_protocol::{RouteCommandAction, SECURE_AUDIO_MONO_FORMAT, SecureAudioDataOffer};
use capyio_runtime::{NodeRuntime, PortLeasePolicy};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const BOUND: Duration = Duration::from_secs(10);
const TICK: Duration = Duration::from_millis(2);
const PREFIX: &str = ".capyio-secure-audio-";
const SCENARIOS: [&str; 9] = [
    "waveform",
    "stop",
    "source_revoke",
    "sink_revoke",
    "expiry",
    "control_eof",
    "media_eof",
    "no_authority",
    "peer_denied",
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
        "format":SECURE_AUDIO_MONO_FORMAT, "surface":"reusable_rust_apis",
        "owner_pid":std::process::id()});
    value
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let path = marker(root, &format!("{stage}.json"));
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, serde_json::to_vec(&value).unwrap()).unwrap();
    fs::rename(temporary, path).unwrap();
}
fn wait_marker(root: &Path, name: &str) {
    let deadline = Instant::now() + BOUND;
    while !marker(root, name).is_file() {
        assert!(
            Instant::now() < deadline,
            "hosted observation checkpoint {name} absent"
        );
        thread::sleep(TICK);
    }
}
fn no_children() {
    let pid = std::process::id();
    for entry in fs::read_dir("/proc").unwrap().flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|b| b.is_ascii_digit())
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
            Some(pid),
            "helper absent before both consent/leases and after cleanup"
        );
    }
}
fn pending(root: &Path, scenario: &str, stage: &str, runtime: &NodeRuntime) {
    no_children();
    assert!(runtime.snapshot().port_leases.is_empty());
    publish(
        root,
        scenario,
        stage,
        json!({"helper_absent":true,"lease_count":0,"bytes_written":0}),
    );
}
fn identities() -> (ControlTlsServerIdentity, ControlTlsClientIdentity) {
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
    let (client, client_key) = leaf(
        "synthetic-audio-client",
        ExtendedKeyUsagePurpose::ClientAuth,
    );
    let server_pin = CertificateFingerprint::from_certificate_der(&server).unwrap();
    let client_pin = CertificateFingerprint::from_certificate_der(&client).unwrap();
    (
        ControlTlsServerIdentity::from_bundle(
            client_pin,
            ControlCredentialBundle::new(vec![server, ca.clone()], server_key, ca.clone()).unwrap(),
        ),
        ControlTlsClientIdentity {
            expected_peer_certificate: server_pin,
            certificate_chain_der: vec![client, ca.clone()],
            private_key_der: client_key,
            server_ca_der: ca,
        },
    )
}
fn source_runtime() -> (NodeRuntime, PortRef) {
    let mut node = capyio_testkit::android_node();
    for adapter in node.adapter_instances.values_mut() {
        adapter
            .supported_route_modes
            .insert(RouteBackend::AdapterManaged);
    }
    let capability = node
        .capabilities
        .values_mut()
        .find(|capability| capability.class == CapabilityClass::Microphone)
        .unwrap();
    capability.availability = Availability::Available;
    let port = capability
        .ports
        .values_mut()
        .find(|port| port.direction == PortDirection::Source)
        .unwrap();
    port.availability = Availability::Available;
    port.interoperability_mode = InteroperabilityMode::AdapterManaged;
    port.formats = vec![FormatDescriptor::new(SECURE_AUDIO_MONO_FORMAT)];
    port.qos_modes = BTreeSet::from([QosMode::Interactive]);
    let source = PortRef {
        node_id: node.id,
        capability_id: capability.id,
        port_id: port.id,
    };
    (NodeRuntime::new(node).unwrap(), source)
}
fn sink_runtime() -> (NodeRuntime, PortRef) {
    let mut node = NodeDescriptor::new(
        NodeId::new(),
        "Synthetic Linux secure audio Sink",
        Platform::Linux,
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
        adapter_type: "capyio.linux.secure-audio".into(),
        display_name: "Synthetic owned playback".into(),
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
        display_name: "Synthetic Linux Speaker".into(),
        class: CapabilityClass::Speaker,
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::None,
        metadata: BTreeMap::new(),
        ports: BTreeMap::from([(
            port,
            PortDescriptor {
                id: port,
                capability_id: capability,
                display_name: "Secure mono Sink".into(),
                direction: PortDirection::Sink,
                profile: ProfileId::audio_frames_v1(),
                schema_id: None,
                formats: vec![FormatDescriptor::new(SECURE_AUDIO_MONO_FORMAT)],
                qos_modes: BTreeSet::from([QosMode::Interactive]),
                clock_domain: None,
                availability: Availability::Available,
                permission_requirement: PermissionRequirement::None,
                interoperability_mode: InteroperabilityMode::AdapterManaged,
            },
        )]),
    })
    .unwrap();
    let sink = PortRef {
        node_id: node.id,
        capability_id: capability,
        port_id: port,
    };
    (NodeRuntime::new(node).unwrap(), sink)
}

enum LocalAction {
    Stop,
    RevokeSink,
}
struct Outcome {
    helper: Option<u32>,
    expiry: Option<u64>,
    observed: u64,
    metrics: PlaybackMetrics,
}
fn assert_closed(runtime: &NodeRuntime, session: SessionId) {
    assert_eq!(
        runtime.session(session).unwrap().state,
        SessionState::Closed
    );
    assert!(runtime.snapshot().port_leases.is_empty());
}
fn negative_close(mut runtime: NodeRuntime, mut link: CatalogControlLink) -> Outcome {
    assert!(runtime.snapshot().port_leases.is_empty());
    let session = link.session_id();
    link.close_and_reconcile(&mut runtime).unwrap();
    assert_closed(&runtime, session);
    no_children();
    Outcome {
        helper: None,
        expiry: None,
        observed: now_ms(),
        metrics: PlaybackMetrics::default(),
    }
}

#[allow(clippy::too_many_arguments)]
fn receiver(
    root: PathBuf,
    scenario: String,
    mut runtime: NodeRuntime,
    sink: PortRef,
    source: PortRef,
    server: AuthenticatedControlServer,
    identity: ControlTlsServerIdentity,
    actions: mpsc::Receiver<LocalAction>,
    written: Arc<AtomicU64>,
    terminal: Arc<AtomicBool>,
    injected: Arc<AtomicBool>,
) -> Outcome {
    let accept_deadline = Instant::now() + BOUND;
    let channel = loop {
        assert!(
            Instant::now() < accept_deadline,
            "bounded synthetic accepted control"
        );
        if let Some(channel) = server.try_accept_until(accept_deadline).unwrap() {
            break channel;
        }
        thread::sleep(TICK);
    };
    drop(server);
    let mut link = CatalogControlLink::admit_accepted_new_peer(
        &mut runtime,
        channel,
        "Synthetic Android".into(),
    )
    .unwrap();
    let catalog = link.receive_catalog().unwrap();
    link.apply_catalog(&mut runtime, catalog).unwrap();
    let catalog = link.prepare_catalog(&runtime, &BTreeSet::new()).unwrap();
    link.send_catalog(catalog).unwrap();
    let route_id = runtime
        .create_route(
            link.session_id(),
            source,
            sink,
            RouteBackend::AdapterManaged,
        )
        .unwrap();
    assert_eq!(
        runtime.route(route_id).unwrap().authorization,
        AuthorizationState::Pending
    );
    pending(&root, &scenario, "before_local", &runtime);
    wait_marker(&root, "local_checked");
    if scenario == "no_authority" {
        return negative_close(runtime, link);
    }
    // Explicit synthetic local approval, independent of pairing/TLS/peer consent.
    let local_expiry = now_ms() + if scenario == "expiry" { 8000 } else { 12000 };
    link.send_route_status(runtime.route(route_id).unwrap())
        .unwrap();
    let peer_expiry = loop {
        let ReceivedControlMessage::RouteStatus(status) = link
            .receive_control_message_until(Instant::now() + BOUND)
            .unwrap()
        else {
            panic!("only exact consent status during negotiation")
        };
        assert_eq!(status.route.id, route_id);
        assert_eq!(status.route.source, source);
        assert_eq!(status.route.sink, sink);
        assert_eq!(status.route.state, RouteState::Draft);
        assert!(status.secure_audio_data.is_none() && status.native_audio_data.is_none());
        assert!(runtime.snapshot().port_leases.is_empty());
        no_children();
        match status.route.authorization {
            AuthorizationState::Pending => {}
            AuthorizationState::Denied if scenario == "peer_denied" => {
                return negative_close(runtime, link);
            }
            AuthorizationState::Authorized {
                expires_at_ms: Some(expiry),
            } if scenario != "peer_denied" => break expiry,
            _ => panic!("independent Source consent required"),
        }
    };
    let expiry = local_expiry.min(peer_expiry);
    runtime.authorize_route(route_id, Some(expiry)).unwrap();
    link.send_route_status(runtime.route(route_id).unwrap())
        .unwrap();
    let ReceivedControlMessage::RouteStatus(status) = link
        .receive_control_message_until(Instant::now() + BOUND)
        .unwrap()
    else {
        panic!("exact finite Source narrowing acknowledgement required")
    };
    assert_eq!(status.route, *runtime.route(route_id).unwrap());
    assert!(status.secure_audio_data.is_none());
    let now = now_ms();
    runtime
        .set_local_port_lease_policy(sink, PortLeasePolicy::Exclusive)
        .unwrap();
    runtime
        .acquire_route_port_lease(route_id, sink, expiry, now)
        .unwrap();
    runtime
        .prepare_route(
            route_id,
            Some(FormatDescriptor::new(SECURE_AUDIO_MONO_FORMAT)),
            QosMode::Interactive,
            now,
        )
        .unwrap();
    runtime.begin_route_start(route_id, now).unwrap();
    let session = link.session_id();
    let admission = SecurePlaybackAdmission {
        expected: link.tls_binding(),
        route_id,
        stream_id: StreamId::new(),
        local_address: "127.0.0.1".parse().unwrap(),
        lifetime: Duration::from_secs(12),
        pipewire_target: "capyio.test.null".into(),
    };
    let mut owner = SecurePlaybackOwner::new(runtime, link, identity, admission).unwrap();
    assert!(owner.helper_pid().is_none() && owner.listener_address().is_none());
    assert_eq!(owner.metrics().bytes_written, 0);
    let mut bound_helper = None;
    let mut pinned_expiry = None;
    let until = Instant::now() + Duration::from_secs(15);
    let observed = loop {
        assert!(
            Instant::now() < until,
            "bounded secure playback owner lifecycle"
        );
        if let Ok(action) = actions.try_recv() {
            match action {
                LocalAction::Stop => {
                    assert_eq!(scenario, "waveform");
                    break now_ms();
                }
                LocalAction::RevokeSink => {
                    assert_eq!(scenario, "sink_revoke");
                    owner
                        .runtime_mut()
                        .release_route_port_lease(route_id, sink, session)
                        .unwrap();
                }
            }
        }
        let event = owner.poll();
        written.store(owner.metrics().packets_written, Ordering::Release);
        match event {
            Ok(Some(SecurePlaybackEvent::Listening)) => {
                assert!(owner.helper_pid().is_none());
                assert_eq!(owner.metrics().bytes_written, 0);
                no_children();
                pinned_expiry = Some(owner.source_lease_expires_at_ms().unwrap());
                assert!(
                    pinned_expiry.unwrap() < expiry,
                    "Source lease is not the longer consent grant"
                );
            }
            Ok(Some(SecurePlaybackEvent::Bound)) => {
                let helper = owner.helper_pid().unwrap();
                assert!(bound_helper.replace(helper).is_none());
                assert_eq!(owner.source_lease_expires_at_ms(), pinned_expiry);
                publish(
                    &root,
                    &scenario,
                    "bound",
                    json!({"helper_pid":helper,"source_lease_expires_at_ms":pinned_expiry}),
                );
            }
            Ok(Some(SecurePlaybackEvent::Stopped)) => {
                assert_eq!(scenario, "stop");
                assert!(injected.load(Ordering::Acquire));
                break owner
                    .terminal_observed_at_ms()
                    .expect("peer Stop observed before cleanup");
            }
            Ok(Some(SecurePlaybackEvent::Record)) | Ok(None) => {}
            Err(error) => {
                let observed = owner
                    .terminal_observed_at_ms()
                    .expect("failure observed before cleanup");
                assert!(
                    injected.load(Ordering::Acquire),
                    "terminal failure preceded requested injection: {error}"
                );
                assert!(
                    !matches!(scenario.as_str(), "waveform" | "stop"),
                    "unexpected receiver failure: {error}"
                );
                assert!(
                    bound_helper.is_some(),
                    "failure must follow genuine media activation: {error}"
                );
                if scenario == "expiry" {
                    assert!(
                        expiry_reached(observed, pinned_expiry.unwrap()),
                        "receiver terminated before exact Source lease: {error}"
                    );
                } else {
                    assert!(
                        observed < pinned_expiry.unwrap(),
                        "incidental expiry cannot prove {scenario}: {error}"
                    );
                }
                if scenario == "sink_revoke" {
                    assert!(error.to_string().contains("Sink lease"));
                }
                if matches!(
                    scenario.as_str(),
                    "source_revoke" | "control_eof" | "media_eof"
                ) {
                    let expected = if scenario == "control_eof" {
                        "control_closed"
                    } else {
                        "media_closed"
                    };
                    // The owner classifies the preserved I/O kind, not rustls's
                    // display text or a wrapper prefix. Timeouts and unrelated
                    // failures must never satisfy these injected-close cases.
                    assert_eq!(
                        owner.terminal_cause(),
                        Some(expected),
                        "expected typed authenticated transport closure: {error}"
                    );
                }
                break observed;
            }
        }
        thread::sleep(TICK);
    };
    terminal.store(true, Ordering::Release);
    let metrics = owner.metrics();
    owner.stop().unwrap();
    assert_closed(owner.runtime(), session);
    assert_eq!(owner.metrics().bytes_written, metrics.bytes_written);
    assert_eq!(metrics.packets_dropped_busy, 0);
    if let Some(helper) = bound_helper {
        assert!(!Path::new(&format!("/proc/{helper}")).exists());
    }
    no_children();
    Outcome {
        helper: bound_helper,
        expiry: pinned_expiry,
        observed,
        metrics,
    }
}

fn expiry_reached(observed: u64, expiry: u64) -> bool {
    observed >= expiry
}
#[test]
fn pure_secure_audio_expiry_requires_exact_negotiated_deadline() {
    for (observed, expires, expected) in [
        (0, 1, false),
        (8999, 9000, false),
        (9000, 9000, true),
        (9001, 9000, true),
    ] {
        assert_eq!(expiry_reached(observed, expires), expected);
    }
}

#[test]
fn pure_secure_audio_control_endpoint_requires_actual_bound_port() {
    let mut endpoint = ControlTlsEndpoint {
        address: "127.0.0.1:0".parse().unwrap(),
        io_timeout: Duration::from_secs(2),
    };
    assert!(matches!(
        endpoint.validate(),
        Err(capyio_control_tls::ControlTlsError::InvalidEndpoint)
    ));
    endpoint.address.set_port(49152);
    endpoint.validate().unwrap();
}

fn receive_offer(
    runtime: &mut NodeRuntime,
    link: &mut CatalogControlLink,
    route_id: RouteId,
) -> SecureAudioDataOffer {
    let mut narrowed = false;
    let mut leased = false;
    loop {
        match link
            .receive_control_message_until(Instant::now() + BOUND)
            .unwrap()
        {
            ReceivedControlMessage::RouteStatus(status) => {
                assert_eq!(status.route.id, route_id);
                assert!(status.native_audio_data.is_none());
                if let Some(offer) = status.secure_audio_data {
                    assert!(narrowed && leased);
                    runtime.begin_route_start(route_id, now_ms()).unwrap();
                    assert_eq!(status.route, *runtime.route(route_id).unwrap());
                    return offer;
                }
                let AuthorizationState::Authorized {
                    expires_at_ms: Some(expiry),
                } = status.route.authorization
                else {
                    panic!("finite exact narrowing required")
                };
                runtime
                    .restrict_draft_authorization(route_id, expiry, now_ms())
                    .unwrap();
                link.send_route_status(runtime.route(route_id).unwrap())
                    .unwrap();
                narrowed = true;
            }
            ReceivedControlMessage::PortLeaseRequest(request) => {
                assert!(narrowed && !leased);
                link.answer_port_lease_request(runtime, request, now_ms())
                    .unwrap();
                runtime
                    .prepare_route(
                        route_id,
                        Some(FormatDescriptor::new(SECURE_AUDIO_MONO_FORMAT)),
                        QosMode::Interactive,
                        now_ms(),
                    )
                    .unwrap();
                leased = true;
            }
            _ => panic!("unexpected secure audio negotiation message"),
        }
    }
}
struct Samples {
    encoder: SecureMonoPcmAdmission,
    sequence: u64,
    attempted: u64,
    last_send: Option<Instant>,
}
impl Samples {
    fn new(sender: &SecureAudioSender) -> Self {
        let binding = sender.binding();
        Self {
            encoder: SecureMonoPcmAdmission::new(AudioMediaStreamBinding {
                session_id: binding.session.session_id,
                route_id: binding.route_id,
                stream_id: binding.stream_id,
                stream_epoch: binding.epoch.try_into().unwrap(),
                selected_spec: AudioStreamSpec::voice_interactive(),
            })
            .unwrap(),
            sequence: 0,
            attempted: 0,
            last_send: None,
        }
    }
    fn record(&mut self, hz: u32) -> [u8; 1064] {
        let binding = self.encoder.binding().clone();
        let payload = (0..480)
            .flat_map(|sample| {
                let phase =
                    (self.sequence * 480 + sample) as f64 * f64::from(hz) * std::f64::consts::TAU
                        / 48000.0;
                ((8192.0 * phase.sin()).round() as i16).to_le_bytes()
            })
            .collect();
        self.encoder
            .encode_record(&AudioMediaPacket {
                stream_id: binding.stream_id,
                stream_epoch: binding.stream_epoch,
                sequence: self.sequence,
                source_timestamp_micros: self.sequence * 10_000,
                first_sample_index: self.sequence * 480,
                sample_count: 480,
                discontinuity: false,
                payload,
            })
            .unwrap()
    }
    fn send(
        &mut self,
        sender: &mut SecureAudioSender,
        runtime: &NodeRuntime,
        hz: u32,
    ) -> Result<(), String> {
        if let Some(last) = self.last_send
            && let Some(wait) =
                (last + Duration::from_millis(10)).checked_duration_since(Instant::now())
        {
            thread::sleep(wait);
        }
        let bytes = self.record(hz);
        self.last_send = Some(Instant::now());
        self.attempted += 1;
        sender.send_record(runtime, &bytes, now_ms())?;
        self.sequence += 1;
        Ok(())
    }
}

#[test]
#[ignore = "requires explicit hosted-private-secure-audio harness, synthetic graph and certificates"]
fn hosted_secure_audio_android_sender_graph() {
    assert_eq!(
        std::env::var("CAPYIO_SECURE_AUDIO_HOSTED").as_deref(),
        Ok("1")
    );
    assert_eq!(std::env::var("CAPYIO_PIPEWIRE_TEST").as_deref(), Ok("1"));
    assert_eq!(
        std::env::var("PIPEWIRE_REMOTE").as_deref(),
        Ok("capyio-test")
    );
    let root = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap());
    assert_eq!(
        fs::read_to_string(root.join(".capyio-pipewire-loopback")).unwrap(),
        "capyio-hosted-pipewire-loopback-v1\n"
    );
    let scenario = std::env::var("CAPYIO_SECURE_AUDIO_SCENARIO").unwrap();
    assert!(SCENARIOS.contains(&scenario.as_str()));
    let (mut runtime, source) = source_runtime();
    let (linux, sink) = sink_runtime();
    let (server_identity, client_identity) = identities();
    let nodes = ControlTlsNodeBinding::new(sink.node_id, source.node_id).unwrap();
    // Port zero requests an ephemeral listener, not a valid peer endpoint.
    // Retain the bound socket rather than reserving and reopening its port.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = AuthenticatedControlServer::from_listener_fresh_session(
        listener,
        server_identity.clone(),
        nodes,
        Duration::from_secs(2),
    )
    .unwrap();
    let address = server.local_addr().unwrap();
    assert!(address.ip().is_loopback());
    assert_ne!(address.port(), 0);
    let mut directory = PairedPeerDirectory::new(source.node_id).unwrap();
    directory
        .add(
            PairedPeerRecord::new(
                sink.node_id,
                "Synthetic Linux",
                "localhost",
                CredentialHandle::new("ephemeral.synthetic.audio").unwrap(),
                client_identity.expected_peer_certificate,
                1,
            )
            .unwrap(),
        )
        .unwrap();
    let mut credentials = ControlCredentialBundle::new(
        client_identity.certificate_chain_der.clone(),
        client_identity.private_key_der.clone(),
        client_identity.server_ca_der.clone(),
    )
    .unwrap()
    .encode()
    .unwrap();
    let (actions, receive_actions) = mpsc::sync_channel(1);
    let written = Arc::new(AtomicU64::new(0));
    let terminal = Arc::new(AtomicBool::new(false));
    let injected = Arc::new(AtomicBool::new(false));
    let owner = {
        let (root, scenario, written, terminal, injected) = (
            root.clone(),
            scenario.clone(),
            written.clone(),
            terminal.clone(),
            injected.clone(),
        );
        thread::spawn(move || {
            receiver(
                root,
                scenario,
                linux,
                sink,
                source,
                server,
                server_identity,
                receive_actions,
                written,
                terminal,
                injected,
            )
        })
    };
    let channel = AuthenticatedControlClientChannel::connect_fresh_session(
        ControlTlsEndpoint {
            address,
            io_timeout: Duration::from_secs(2),
        },
        "localhost",
        client_identity,
        nodes,
    )
    .unwrap();
    let mut link =
        CatalogControlLink::admit_dialed_new_peer(&mut runtime, channel, "Synthetic Linux".into())
            .unwrap();
    let session = link.session_id();
    let catalog = link
        .prepare_catalog(&runtime, &BTreeSet::from([source.capability_id]))
        .unwrap();
    link.send_catalog(catalog).unwrap();
    let catalog = link.receive_catalog().unwrap();
    link.apply_catalog(&mut runtime, catalog).unwrap();
    let mut sent = 0;
    let mut attempted = 0;
    if scenario != "no_authority" {
        let ReceivedControlMessage::RouteStatus(status) = link
            .receive_control_message_until(Instant::now() + BOUND)
            .unwrap()
        else {
            panic!("pending exact Draft required")
        };
        let route_id = status.route.id;
        assert_eq!(status.route.authorization, AuthorizationState::Pending);
        assert_eq!(status.route.state, RouteState::Draft);
        assert!(status.secure_audio_data.is_none() && status.native_audio_data.is_none());
        link.apply_route_status(&mut runtime, &status, false)
            .unwrap();
        let mut consent = AudioSourceConsent::default();
        consent.enqueue(&mut runtime, route_id, now_ms()).unwrap();
        assert!(
            runtime
                .acquire_route_port_lease(route_id, source, now_ms() + 12000, now_ms())
                .is_err()
        );
        link.send_route_status(runtime.route(route_id).unwrap())
            .unwrap();
        pending(&root, &scenario, "before_source", &runtime);
        wait_marker(&root, "source_checked");
        let now = now_ms();
        consent
            .resolve(
                &mut runtime,
                route_id,
                scenario != "peer_denied",
                now,
                now + 12000,
            )
            .unwrap();
        link.send_route_status(runtime.route(route_id).unwrap())
            .unwrap();
        if scenario != "peer_denied" {
            let offer = receive_offer(&mut runtime, &mut link, route_id);
            let lease_expiry = runtime
                .route_port_lease_claim(route_id, source)
                .unwrap()
                .expires_at_ms;
            let mut sender = SecureAudioSender::connect(
                &runtime,
                &mut link,
                route_id,
                offer,
                &directory.encode_json().unwrap(),
                &credentials,
                now_ms(),
            )
            .unwrap();
            credentials.fill(0);
            let mut samples = Samples::new(&sender);
            // Keep the bounded media read alive while the independent graph
            // oracle resolves the actual pw-cat PID, ports and explicit links.
            let ready = Instant::now() + Duration::from_secs(5);
            while !marker(&root, "links_ready").is_file() {
                assert!(Instant::now() < ready && !terminal.load(Ordering::Acquire));
                samples.send(&mut sender, &runtime, 0).unwrap();
            }
            for index in 0..300 {
                assert!(
                    !terminal.load(Ordering::Acquire),
                    "owner cannot fail before requested injection"
                );
                let hz = if index < 120 {
                    1000
                } else if index < 180 {
                    0
                } else {
                    2000
                };
                samples.send(&mut sender, &runtime, hz).unwrap();
            }
            // Require the full waveform copied to the real owned helper before
            // injecting lifecycle failure; independent PCM remains the oracle.
            let drain = Instant::now() + Duration::from_millis(150);
            while written.load(Ordering::Acquire) < samples.sequence {
                assert!(Instant::now() < drain && !terminal.load(Ordering::Acquire));
                thread::sleep(TICK);
            }
            assert!(
                now_ms() < lease_expiry,
                "requested failure under live authority"
            );
            injected.store(true, Ordering::Release);
            match scenario.as_str() {
                "waveform" => actions.send(LocalAction::Stop).unwrap(),
                "stop" => link
                    .send_route_command_until(
                        route_id,
                        RouteCommandAction::Stop,
                        Instant::now() + Duration::from_millis(250),
                    )
                    .unwrap(),
                "source_revoke" => {
                    runtime
                        .release_route_port_lease(route_id, source, session)
                        .unwrap();
                    assert_eq!(
                        samples.send(&mut sender, &runtime, 0).unwrap_err(),
                        "audio Source authority revoked"
                    );
                }
                "sink_revoke" => actions.send(LocalAction::RevokeSink).unwrap(),
                "control_eof" => link.close(),
                "media_eof" => sender.stop(),
                "expiry" => loop {
                    match samples.send(&mut sender, &runtime, 0) {
                        Ok(()) => {}
                        Err(_) => {
                            assert!(
                                expiry_reached(now_ms(), lease_expiry),
                                "sender ended before exact negotiated Source lease"
                            );
                            break;
                        }
                    }
                },
                _ => unreachable!(),
            }
            sent = samples.sequence;
            attempted = samples.attempted;
            // Do not create an earlier media EOF for Stop/revoke/control EOF:
            // let the owning receiver observe the requested cause first.
            let until = Instant::now() + Duration::from_secs(2);
            while !terminal.load(Ordering::Acquire) {
                assert!(
                    Instant::now() < until,
                    "requested terminal cause was not observed"
                );
                thread::sleep(TICK);
            }
            sender.stop();
        }
    }
    let outcome = owner.join().unwrap();
    credentials.fill(0);
    link.close_and_reconcile(&mut runtime).unwrap();
    assert_closed(&runtime, session);
    no_children();
    publish(
        &root,
        &scenario,
        "stopped",
        json!({"helper_pid":outcome.helper,
        "source_lease_expires_at_ms":outcome.expiry,"terminal_observed_at_ms":outcome.observed,
        "bytes_written":outcome.metrics.bytes_written,"packets_written":outcome.metrics.packets_written,
        "packets_dropped_busy":outcome.metrics.packets_dropped_busy,"source_records_sent":sent,
        "source_records_attempted":attempted,
        "injected":scenario,"verified":scenario,"session_closed":true,"leases_released":true,"helper_reaped":true}),
    );
    wait_marker(&root, "stop_checked");
}

// Production console acceptance is additive: the nine reusable-API scenarios
// above retain their exact protocol, deadlines, names and graph contract.
#[cfg(feature = "paired-store")]
#[path = "support/paired_store.rs"]
mod paired_store;
#[cfg(feature = "paired-store")]
#[path = "support/playback_console.rs"]
mod playback_console;
#[cfg(feature = "paired-store")]
#[path = "support/secure_console.rs"]
mod secure_console;
#[cfg(feature = "paired-store")]
fn nonblocking(fd: impl std::os::fd::AsFd) {
    let flags = rustix::fs::fcntl_getfl(&fd).unwrap();
    rustix::fs::fcntl_setfl(fd, flags | rustix::fs::OFlags::NONBLOCK).unwrap();
}
#[cfg(feature = "paired-store")]
fn drain(reader: &mut impl std::io::Read, bytes: &mut Vec<u8>) {
    let mut buffer = [0; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => panic!("bounded console output: {e}"),
        }
        assert!(bytes.len() <= 1_048_576);
    }
}
