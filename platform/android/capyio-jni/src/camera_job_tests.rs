//! Hardware-free lifecycle races through the real consent, control and media stack.
use super::*;
use crate::camera_sender::CameraSourceConsent;
use capyio_control_credentials::ControlCredentialBundle;
use capyio_control_link::CatalogControlLink;
use capyio_control_tls::{
    AuthenticatedControlClientChannel, AuthenticatedControlServer, ControlTlsBinding,
    ControlTlsClientIdentity, ControlTlsEndpoint, ControlTlsServerIdentity,
    camera::{CameraTlsChannel, CameraTlsListener},
};
use capyio_core::*;
use capyio_pairing::CertificateFingerprint;
use capyio_protocol::{SecureCameraDataOffer, SecureCameraTransport};
use rcgen::{
    BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use std::{net::TcpListener, time::Instant};

struct Fixture {
    runtime: Arc<Mutex<NodeRuntime>>,
    prepared: PreparedCameraSender,
    identity: ControlTlsServerIdentity,
    _source_control: CatalogControlLink,
    _sink_control: CatalogControlLink,
}
impl Fixture {
    fn new(port: u16) -> Self {
        let (android, sink_node, source, sink) = nodes();
        let (identity, bundle, pin) = dual_identities();
        let binding = ControlTlsBinding::new(SessionId::new(), sink_node.id, android.id).unwrap();
        let mut runtime = NodeRuntime::new(android.clone()).unwrap();
        runtime.register_peer(sink_node.clone(), true).unwrap();
        let mut sink_runtime = NodeRuntime::new(sink_node).unwrap();
        sink_runtime.register_peer(android, true).unwrap();
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = socket.local_addr().unwrap();
        let server = AuthenticatedControlServer::from_listener(
            socket,
            identity.clone(),
            binding,
            Duration::from_secs(2),
        )
        .unwrap();
        let accept = thread::spawn(move || server.accept().unwrap());
        let client_identity = ControlTlsClientIdentity::from_bundle(pin, bundle);
        let client = AuthenticatedControlClientChannel::connect(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(2),
            },
            "localhost",
            client_identity.clone(),
            binding,
        )
        .unwrap();
        let source_control =
            CatalogControlLink::admit_dialed_peer(&mut runtime, client, "Sink fixture".into())
                .unwrap();
        let sink_control = CatalogControlLink::admit_accepted_peer(
            &mut sink_runtime,
            accept.join().unwrap(),
            "Android fixture".into(),
        )
        .unwrap();
        let id = runtime
            .create_route(
                binding.session_id,
                source,
                sink,
                RouteBackend::AdapterManaged,
            )
            .unwrap();
        let timestamp = now().unwrap();
        let mut consent = CameraSourceConsent::default();
        consent.enqueue(&mut runtime, id, timestamp).unwrap();
        assert!(
            runtime
                .acquire_route_port_lease(id, source, timestamp + 10_000, timestamp)
                .is_err()
        );
        consent
            .resolve(&mut runtime, id, true, timestamp, timestamp + 10_000)
            .unwrap();
        runtime
            .acquire_route_port_lease(id, source, timestamp + 10_000, timestamp)
            .unwrap();
        runtime
            .prepare_route(
                id,
                Some(FormatDescriptor::new("cavc-avc-access-unit-v1")),
                QosMode::Interactive,
                timestamp,
            )
            .unwrap();
        runtime.begin_route_start(id, timestamp).unwrap();
        let offer = SecureCameraDataOffer {
            port,
            stream_id: StreamId::new(),
            transport: SecureCameraTransport::Tls13MtlsV1,
        };
        let prepared = PreparedCameraSender::new(
            &runtime,
            &source_control,
            id,
            offer,
            client_identity,
            "localhost".into(),
            timestamp,
        )
        .unwrap();
        Self {
            runtime: Arc::new(Mutex::new(runtime)),
            prepared,
            identity,
            _source_control: source_control,
            _sink_control: sink_control,
        }
    }
    fn snapshot(&self) -> Snapshot {
        let runtime = self.runtime.clone();
        Arc::new(move || Ok(runtime.lock().unwrap().clone()))
    }
    fn start(&self, snapshot: Snapshot) -> Arc<CameraJob> {
        let (job, receiver) = CameraJob::pending(
            1,
            1,
            self.prepared.binding,
            self.prepared.profile(),
            self.prepared.lease_deadline(),
        );
        let prepared = self.prepared.clone();
        job.spawn(
            receiver,
            Box::new(move || Ok(prepared)),
            snapshot,
            Arc::new(|| {}),
        )
        .unwrap();
        job
    }
    fn revoke_source_lease(&self) {
        let mut runtime = self.runtime.lock().unwrap();
        let route = runtime
            .route(self.prepared.binding.route_id)
            .unwrap()
            .clone();
        assert!(
            runtime
                .release_route_port_lease(route.id, route.source, route.session_id)
                .unwrap()
        );
    }
}
fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "bounded lifecycle completion exceeded two seconds"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
fn accept_media(listener: CameraTlsListener) -> thread::JoinHandle<CameraTlsChannel> {
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(channel) = listener.try_accept().unwrap() {
                return channel;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
    })
}
#[test]
fn stop_during_actual_tls_handshake_never_publishes_ready_and_finishes_bounded() {
    let media = TcpListener::bind("127.0.0.1:0").unwrap();
    let fixture = Fixture::new(media.local_addr().unwrap().port());
    let job = fixture.start(fixture.snapshot());
    // Accept TCP but deliberately never answer the actual TLS ClientHello.
    media.set_nonblocking(true).unwrap();
    let mut held_socket = None;
    wait_until(|| match media.accept() {
        Ok((socket, _)) => {
            held_socket = Some(socket);
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => false,
        Err(e) => panic!("{e}"),
    });
    assert_eq!(job.state(), PENDING);
    assert!(
        fixture.runtime.try_lock().is_ok(),
        "connect held Runtime mutex"
    );
    let started = Instant::now();
    job.stop();
    while !job.finished() {
        assert_eq!(job.state(), STOPPED);
        assert!(started.elapsed() < Duration::from_secs(2));
        thread::sleep(Duration::from_millis(1));
    }
    assert!(job.reserve(56).is_err());
    job.reap();
    assert!(job.worker.lock().unwrap().is_none());
    drop(held_socket);
}
#[test]
fn stop_after_actual_tls_connect_before_ready_installation_cannot_resurrect_job() {
    let media = TcpListener::bind("127.0.0.1:0").unwrap();
    let fixture = Fixture::new(media.local_addr().unwrap().port());
    let listener = CameraTlsListener::from_listener(
        media,
        fixture.identity.clone(),
        fixture.prepared.binding,
        Duration::from_millis(500),
    )
    .unwrap();
    let accepted = accept_media(listener);
    let runtime = fixture.runtime.clone();
    let count = AtomicUsize::new(0);
    let (reached_tx, reached_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    let snapshot: Snapshot = Arc::new(move || {
        // Second snapshot is after real TLS connect, immediately before Ready.
        let current = runtime.lock().unwrap().clone();
        if count.fetch_add(1, Ordering::AcqRel) == 1 {
            reached_tx.send(()).unwrap();
            release_rx
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
        }
        Ok(current)
    });
    let job = fixture.start(snapshot);
    reached_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut peer = accepted.join().unwrap();
    assert_eq!(job.state(), PENDING);
    assert!(fixture.runtime.try_lock().is_ok());
    job.stop();
    release_tx.send(()).unwrap();
    wait_until(|| job.finished());
    assert_eq!(job.state(), STOPPED);
    assert!(job.status.lock().unwrap().stop.is_none());
    assert!(job.reserve(56).is_err());
    assert!(
        peer.receive_record().is_err(),
        "cancelled connect left media socket live"
    );
    job.reap();
}
#[test]
fn actual_source_lease_revoke_after_ready_closes_media_and_rejects_records() {
    let media = TcpListener::bind("127.0.0.1:0").unwrap();
    let fixture = Fixture::new(media.local_addr().unwrap().port());
    let listener = CameraTlsListener::from_listener(
        media,
        fixture.identity.clone(),
        fixture.prepared.binding,
        Duration::from_millis(500),
    )
    .unwrap();
    let accepted = accept_media(listener);
    let job = fixture.start(fixture.snapshot());
    let mut peer = accepted.join().unwrap();
    wait_until(|| job.state() == READY);
    assert!(job.status.lock().unwrap().stop.is_some());
    // Exchange a genuine fixture CAVC configuration before revoking, rather
    // than treating Ready alone as evidence of a usable native record path.
    let bytes = include_bytes!("../../../../fixtures/linux-avc/solid-720p30.cavc");
    let length = 56 + u32::from_be_bytes(bytes[52..56].try_into().unwrap()) as usize;
    let capyio_vcamdroid_adapter::AvcRecord::Config { config, .. } =
        capyio_vcamdroid_adapter::decode_record(&bytes[..length]).unwrap()
    else {
        panic!("configuration fixture missing")
    };
    let wire = capyio_vcamdroid_adapter::encode_config(
        capyio_vcamdroid_adapter::AvcStreamKey {
            stream_id: *fixture.prepared.binding.stream_id.as_uuid().as_bytes(),
            epoch: fixture.prepared.binding.epoch,
        },
        &config,
    )
    .unwrap();
    job.submit(job.reserve(wire.len()).unwrap(), wire.clone())
        .unwrap();
    assert_eq!(peer.receive_record().unwrap(), wire);
    fixture.revoke_source_lease();
    wait_until(|| job.finished());
    assert_eq!(job.state(), FAILED);
    assert!(job.status.lock().unwrap().stop.is_none());
    assert!(job.reserve(56).is_err());
    assert!(peer.receive_record().is_err());
    job.reap();
}
#[test]
fn actual_tls_valid_eos_finishes_job_stopped_not_failed() {
    use capyio_vcamdroid_adapter::{
        AvcAccessUnit, AvcRecord, AvcStreamKey, decode_record, encode_access_unit, encode_config,
    };
    let media = TcpListener::bind("127.0.0.1:0").unwrap();
    let fixture = Fixture::new(media.local_addr().unwrap().port());
    let listener = CameraTlsListener::from_listener(
        media,
        fixture.identity.clone(),
        fixture.prepared.binding,
        Duration::from_millis(500),
    )
    .unwrap();
    let accepted = accept_media(listener);
    let job = fixture.start(fixture.snapshot());
    let mut peer = accepted.join().unwrap();
    wait_until(|| job.state() == READY);
    let bytes = include_bytes!("../../../../fixtures/linux-avc/solid-720p30.cavc");
    let config_length = 56 + u32::from_be_bytes(bytes[52..56].try_into().unwrap()) as usize;
    let AvcRecord::Config { config, .. } = decode_record(&bytes[..config_length]).unwrap() else {
        panic!("configuration fixture missing")
    };
    let frame_bytes = &bytes[config_length..];
    let frame_length = 56 + u32::from_be_bytes(frame_bytes[52..56].try_into().unwrap()) as usize;
    let AvcRecord::AccessUnit(mut frame) = decode_record(&frame_bytes[..frame_length]).unwrap()
    else {
        panic!("key-frame fixture missing")
    };
    let stream = AvcStreamKey {
        stream_id: *fixture.prepared.binding.stream_id.as_uuid().as_bytes(),
        epoch: fixture.prepared.binding.epoch,
    };
    frame.stream = stream;
    assert!(frame.key_frame && !frame.end_of_stream);
    let eos = AvcAccessUnit {
        stream,
        sequence: frame.sequence + 1,
        presentation_time_us: frame.presentation_time_us,
        key_frame: false,
        end_of_stream: true,
        discontinuity: false,
        payload: Vec::new(),
    };
    // Admit and observe each record separately, preserving the two-slot bound.
    for wire in [
        encode_config(stream, &config).unwrap(),
        encode_access_unit(&frame).unwrap(),
        encode_access_unit(&eos).unwrap(),
    ] {
        job.submit(job.reserve(wire.len()).unwrap(), wire.clone())
            .unwrap();
        assert_eq!(peer.receive_record().unwrap(), wire);
    }
    wait_until(|| job.finished());
    assert_eq!(
        job.state(),
        STOPPED,
        "successful EOS must not report sender failure"
    );
    assert!(job.status.lock().unwrap().stop.is_none());
    assert!(job.reserve(56).is_err());
    assert!(peer.receive_record().is_err());
    job.reap();
}
fn nodes() -> (NodeDescriptor, NodeDescriptor, PortRef, PortRef) {
    let mut android = capyio_testkit::android_node();
    let mut sink_node = capyio_testkit::windows_node();
    for node in [&mut android, &mut sink_node] {
        for adapter in node.adapter_instances.values_mut() {
            adapter
                .supported_route_modes
                .insert(RouteBackend::AdapterManaged);
        }
        for cap in node.capabilities.values_mut() {
            cap.availability = Availability::Available;
            for port in cap.ports.values_mut() {
                port.availability = Availability::Available;
                port.interoperability_mode = InteroperabilityMode::AdapterManaged;
            }
        }
    }
    let cap = android
        .capabilities
        .values_mut()
        .find(|c| c.class == CapabilityClass::Camera)
        .unwrap();
    let port = cap.ports.values_mut().next().unwrap();
    port.formats = vec![FormatDescriptor::new("cavc-avc-access-unit-v1")];
    port.qos_modes.insert(QosMode::Interactive);
    let source = PortRef {
        node_id: android.id,
        capability_id: cap.id,
        port_id: port.id,
    };
    let sink_cap = sink_node.capabilities.values_mut().next().unwrap();
    sink_cap.class = CapabilityClass::Camera;
    let sink_port = sink_cap.ports.values_mut().next().unwrap();
    sink_port.direction = PortDirection::Sink;
    sink_port.profile = port.profile.clone();
    sink_port.formats = port.formats.clone();
    sink_port.qos_modes = port.qos_modes.clone();
    let sink = PortRef {
        node_id: sink_node.id,
        capability_id: sink_cap.id,
        port_id: sink_port.id,
    };
    (android, sink_node, source, sink)
}
fn dual_identities() -> (
    ControlTlsServerIdentity,
    ControlCredentialBundle,
    CertificateFingerprint,
) {
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
    ];
    let ca_key = KeyPair::generate().unwrap();
    let ca = ca_params.self_signed(&ca_key).unwrap().der().to_vec();
    let issuer = Issuer::new(ca_params, ca_key);
    let leaf = |name: &str| {
        let mut params = CertificateParams::new(vec![name.to_owned()]).unwrap();
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ClientAuth,
            ExtendedKeyUsagePurpose::ServerAuth,
        ];
        let key = KeyPair::generate().unwrap();
        (
            params.signed_by(&key, &issuer).unwrap().der().to_vec(),
            key.serialize_der(),
        )
    };
    let (windows_cert, windows_key) = leaf("localhost");
    let (android_cert, android_key) = leaf("android.capyio.test");
    let windows_fingerprint = CertificateFingerprint::from_certificate_der(&windows_cert).unwrap();
    let android_fingerprint = CertificateFingerprint::from_certificate_der(&android_cert).unwrap();
    let windows_bundle =
        ControlCredentialBundle::new(vec![windows_cert, ca.clone()], windows_key, ca.clone())
            .unwrap();
    let android_bundle =
        ControlCredentialBundle::new(vec![android_cert, ca.clone()], android_key, ca).unwrap();
    (
        ControlTlsServerIdentity::from_bundle(android_fingerprint, windows_bundle),
        android_bundle,
        windows_fingerprint,
    )
}
