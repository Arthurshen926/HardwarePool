//! Disposable synthetic graph only: real Android consent/sender, pinned control/data,
//! production Linux owner/decoder/PipeWire. Never capture hardware or enroll peers.
use capyio_android_jni::camera_sender::{CameraSourceConsent, SecureCameraSender};
use capyio_control_credentials::ControlCredentialBundle;
use capyio_control_link::{CatalogControlLink, ReceivedControlMessage};
use capyio_control_tls::{
    AuthenticatedControlClientChannel, AuthenticatedControlServer, CertificateFingerprint,
    ControlTlsBinding, ControlTlsClientIdentity, ControlTlsEndpoint, ControlTlsServerIdentity,
};
use capyio_core::*;
use capyio_linux_avc::secure::{SecureCameraAdmission, SecureCameraEvent, SecureCameraOwner};
use capyio_pairing::{CredentialHandle, PairedPeerDirectory, PairedPeerRecord};
use capyio_protocol::RouteCommandAction;
use capyio_runtime::{NodeRuntime, PortLeasePolicy};
use capyio_vcamdroid_adapter::{
    AvcAccessUnit, AvcRecord, AvcStreamKey, decode_record, encode_access_unit, encode_config,
};
use rcgen::{
    BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use std::{
    io::{self, BufRead, Write},
    net::{Ipv4Addr, TcpListener},
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
fn error(e: impl ToString) -> io::Error {
    io::Error::other(e.to_string())
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
fn identities() -> (ControlTlsServerIdentity, Vec<u8>, CertificateFingerprint) {
    let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
    ];
    let key = KeyPair::generate().unwrap();
    let ca = params.self_signed(&key).unwrap().der().to_vec();
    let issuer = Issuer::new(params, key);
    let leaf = |name: &str| {
        let mut params = CertificateParams::new(vec![name.into()]).unwrap();
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
    let (server, server_key) = leaf("localhost");
    let (client, client_key) = leaf("android.capyio.test");
    let pin = CertificateFingerprint::from_certificate_der(&server).unwrap();
    let server_identity = ControlTlsServerIdentity {
        expected_peer_certificate: CertificateFingerprint::from_certificate_der(&client).unwrap(),
        certificate_chain_der: vec![server, ca.clone()],
        private_key_der: server_key,
        client_ca_der: ca.clone(),
    };
    let bundle = ControlCredentialBundle::new(vec![client, ca.clone()], client_key, ca)
        .unwrap()
        .encode()
        .unwrap();
    (server_identity, bundle, pin)
}
fn fixture(now: u64, ttl: u64) -> (NodeRuntime, NodeRuntime, RouteId, ControlTlsBinding) {
    let lab = capyio_testkit::DemoLab::new().unwrap();
    let original = lab
        .runtime
        .route(lab.routes.phone_camera_to_panel)
        .unwrap()
        .clone();
    let snapshot = lab.runtime.snapshot();
    let mut linux = snapshot.local_node;
    linux.platform = Platform::Linux;
    let mut android = snapshot
        .peers
        .into_iter()
        .find(|p| p.id == original.source.node_id)
        .unwrap();
    for node in [&mut linux, &mut android] {
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
                if port.profile == ProfileId::video_frames_v1() {
                    port.formats = vec![FormatDescriptor::new("cavc-avc-access-unit-v1")];
                    port.qos_modes.insert(QosMode::Interactive);
                }
            }
        }
    }
    let session = SessionId::new();
    let binding = ControlTlsBinding::new(session, linux.id, android.id).unwrap();
    let id = RouteId::new();
    let make = |local: NodeDescriptor, peer: NodeDescriptor| {
        let peer_id = peer.id;
        let mut runtime = NodeRuntime::new(local).unwrap();
        runtime.register_peer(peer, true).unwrap();
        runtime.open_session_with_id(session, peer_id).unwrap();
        runtime
            .create_route_with_id(
                id,
                session,
                original.source,
                original.sink,
                RouteBackend::AdapterManaged,
            )
            .unwrap();
        runtime
    };
    let mut source = make(android.clone(), linux.clone());
    let mut sink = make(linux, android);
    let mut consent = CameraSourceConsent::default();
    consent.enqueue(&mut source, id, now).unwrap();
    assert!(
        source
            .acquire_route_port_lease(id, original.source, now + ttl, now)
            .is_err()
    );
    consent
        .resolve(&mut source, id, true, now, now + ttl)
        .unwrap();
    sink.authorize_route(id, Some(now + ttl)).unwrap();
    sink.set_local_port_lease_policy(original.sink, PortLeasePolicy::Exclusive)
        .unwrap();
    sink.acquire_route_port_lease(id, original.sink, now + ttl, now)
        .unwrap();
    for runtime in [&mut sink, &mut source] {
        runtime
            .prepare_route(
                id,
                Some(FormatDescriptor::new("cavc-avc-access-unit-v1")),
                QosMode::Interactive,
                now,
            )
            .unwrap();
    }
    sink.begin_route_start(id, now).unwrap();
    // Source starts only AFTER the real authenticated Acquire request is granted.
    (sink, source, id, binding)
}
fn records(stream: AvcStreamKey) -> io::Result<(Vec<u8>, Vec<AvcAccessUnit>)> {
    let bytes = include_bytes!("../../../../fixtures/linux-avc/solid-720p30.cavc");
    let mut pos = 0;
    let mut config = None;
    let mut units = Vec::new();
    while pos < bytes.len() {
        let n = 56 + u32::from_be_bytes(bytes[pos + 52..pos + 56].try_into().unwrap()) as usize;
        match decode_record(&bytes[pos..pos + n]).map_err(error)? {
            AvcRecord::Config { config: c, .. } => {
                config = Some(encode_config(stream, &c).map_err(error)?)
            }
            AvcRecord::AccessUnit(u) if !u.end_of_stream => units.push(u),
            _ => {}
        }
        pos += n;
    }
    Ok((config.ok_or_else(|| error("fixture config absent"))?, units))
}
struct Source {
    runtime: NodeRuntime,
    link: CatalogControlLink,
    id: RouteId,
    directory: Vec<u8>,
    credential: Vec<u8>,
    scenario: String,
    start: mpsc::Receiver<()>,
    done: mpsc::Receiver<()>,
}
impl Source {
    fn run(mut self) -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let offer = loop {
            match self
                .link
                .receive_control_message_until(deadline)
                .map_err(error)?
            {
                ReceivedControlMessage::PortLeaseRequest(request) => {
                    self.link
                        .answer_port_lease_request(&mut self.runtime, request, now())
                        .map_err(error)?;
                    self.runtime
                        .begin_route_start(self.id, now())
                        .map_err(error)?;
                }
                ReceivedControlMessage::RouteStatus(status) => {
                    break status
                        .secure_camera_data
                        .ok_or_else(|| error("secure offer absent"))?;
                }
                _ => return Err(error("unexpected control message before secure offer")),
            }
        };
        let mut sender = SecureCameraSender::connect(
            &self.runtime,
            &self.link,
            self.id,
            offer,
            &self.directory,
            &self.credential,
            now(),
        )
        .map_err(error)?;
        let binding = sender.binding();
        let stream = AvcStreamKey {
            stream_id: *binding.stream_id.as_uuid().as_bytes(),
            epoch: binding.epoch,
        };
        self.start
            .recv_timeout(Duration::from_secs(8))
            .map_err(error)?;
        let (config, units) = records(stream)?;
        sender
            .send_record(&self.runtime, &config, now())
            .map_err(error)?;
        let start = Instant::now();
        let count = if self.scenario == "expiry" { 120 } else { 90 };
        for sequence in 1..=count {
            if sequence == 4 {
                match self.scenario.as_str() {
                    "control_stop" => {
                        self.link
                            .send_route_command_until(
                                self.id,
                                RouteCommandAction::Stop,
                                Instant::now() + Duration::from_secs(1),
                            )
                            .map_err(error)?;
                        break;
                    }
                    "source_revoke" => {
                        let route = self.runtime.route(self.id).map_err(error)?.clone();
                        self.runtime
                            .release_route_port_lease(self.id, route.source, route.session_id)
                            .map_err(error)?;
                        let unit = AvcAccessUnit {
                            stream,
                            sequence,
                            presentation_time_us: sequence * 33333,
                            ..units[0].clone()
                        };
                        let rejected = sender.send_record(
                            &self.runtime,
                            &encode_access_unit(&unit).map_err(error)?,
                            now(),
                        );
                        if rejected.is_ok() {
                            return Err(error("Source revoke failed to close sender"));
                        }
                        println!("INJECTED source_revoke");
                        break;
                    }
                    "control_eof" => {
                        println!("INJECTED control_eof");
                        self.link.close();
                        break;
                    }
                    "partial_cavc" => {
                        assert!(sender.send_record(&self.runtime, b"CAVC", now()).is_err());
                        println!("INJECTED partial_cavc");
                        break;
                    }
                    _ => {}
                }
            }
            let unit = AvcAccessUnit {
                stream,
                sequence,
                presentation_time_us: sequence * 33333,
                ..units[(sequence as usize - 1) % units.len()].clone()
            };
            sender
                .send_record(
                    &self.runtime,
                    &encode_access_unit(&unit).map_err(error)?,
                    now(),
                )
                .map_err(error)?;
            if self.scenario != "no_consumer" {
                let due =
                    start + Duration::from_nanos(sequence * capyio_linux_camera::FRAME_DURATION_NS);
                if let Some(wait) = due.checked_duration_since(Instant::now()) {
                    thread::sleep(wait);
                }
            }
        }
        if matches!(self.scenario.as_str(), "pixels" | "no_consumer") {
            let eos = AvcAccessUnit {
                stream,
                sequence: 91,
                presentation_time_us: 91 * 33333,
                key_frame: false,
                end_of_stream: true,
                discontinuity: false,
                payload: vec![],
            };
            sender
                .send_record(
                    &self.runtime,
                    &encode_access_unit(&eos).map_err(error)?,
                    now(),
                )
                .map_err(error)?;
        }
        let _ = self.done.recv_timeout(Duration::from_secs(12));
        sender.stop();
        Ok(())
    }
}
fn main() -> io::Result<()> {
    if std::env::args().nth(1).as_deref() != Some("--synthetic-private-graph") {
        return Err(error("explicit synthetic graph opt-in required"));
    }
    let scenario = std::env::args().nth(2).unwrap_or_else(|| "pixels".into());
    if ![
        "pixels",
        "no_consumer",
        "expiry",
        "control_stop",
        "control_eof",
        "partial_cavc",
        "source_revoke",
        "sink_revoke",
        "child_exit",
        "decoder_exit",
        "server_disconnect",
    ]
    .contains(&scenario.as_str())
    {
        return Err(error("unknown synthetic scenario"));
    }
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    if line.trim() != "approve synthetic camera" {
        return Err(error("fixture consent absent"));
    }
    let ttl = if scenario == "expiry" { 4000 } else { 12000 };
    let started_ms = now();
    let (sink, source, id, binding) = fixture(started_ms, ttl);
    let (identity, credential, pin) = identities();
    let mut directory = PairedPeerDirectory::new(source.snapshot().local_node.id).map_err(error)?;
    directory
        .add(
            PairedPeerRecord::new(
                sink.snapshot().local_node.id,
                "Synthetic Linux",
                "localhost",
                CredentialHandle::new(format!("android-keystore:{}", "a".repeat(64)))
                    .map_err(error)?,
                pin,
                1,
            )
            .map_err(error)?,
        )
        .map_err(error)?;
    let directory = directory.encode_json().map_err(error)?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let address = listener.local_addr()?;
    let server = AuthenticatedControlServer::from_listener(
        listener,
        identity.clone(),
        binding,
        Duration::from_secs(2),
    )
    .map_err(error)?;
    let accept = thread::spawn(move || server.accept());
    let client = AuthenticatedControlClientChannel::connect(
        ControlTlsEndpoint {
            address,
            io_timeout: Duration::from_secs(2),
        },
        "localhost",
        ControlTlsClientIdentity::from_bundle(
            pin,
            ControlCredentialBundle::decode(&credential).map_err(error)?,
        ),
        binding,
    )
    .map_err(error)?;
    let link = CatalogControlLink::from_accepted(
        accept
            .join()
            .map_err(|_| error("accept panicked"))?
            .map_err(error)?,
    );
    let (start_tx, start) = mpsc::channel();
    let (done_tx, done) = mpsc::channel();
    let peer = Source {
        runtime: source,
        link: CatalogControlLink::from_dialed(client),
        id,
        directory,
        credential,
        scenario: scenario.clone(),
        start,
        done,
    };
    let source_worker = thread::spawn(move || peer.run());
    let mut owner = SecureCameraOwner::new(
        sink,
        link,
        identity,
        SecureCameraAdmission {
            expected: binding,
            route_id: id,
            stream_id: StreamId::new(),
            local_address: Ipv4Addr::LOCALHOST.into(),
            lifetime: Duration::from_millis(ttl),
        },
    )?;
    let deadline = Instant::now() + Duration::from_secs(14);
    let mut announced = false;
    let mut record_count = 0;
    let result = loop {
        match owner.poll() {
            Ok(Some(SecureCameraEvent::Listening)) => {
                assert!(owner.helper_pid().is_none());
                assert!(owner.decoder_pid().is_none());
                assert!(owner.node_name().is_none());
                println!("SECURE_LISTENING no_media_children=true");
            }
            Ok(Some(SecureCameraEvent::Record)) => {
                record_count += 1;
                if scenario == "sink_revoke" && record_count == 3 {
                    let route = owner.runtime().route(id).map_err(error)?.clone();
                    owner
                        .runtime_mut()
                        .release_route_port_lease(id, route.sink, route.session_id)
                        .map_err(error)?;
                    println!("INJECTED sink_revoke");
                }
            }
            Ok(Some(SecureCameraEvent::Bound)) => {
                println!(
                    "CAMERA_READY {} {}",
                    owner.helper_pid().unwrap(),
                    owner.node_name().unwrap()
                );
                println!("DECODER_READY {}", owner.decoder_pid().unwrap());
                io::stdout().flush()?;
                line.clear();
                io::stdin().lock().read_line(&mut line)?;
                if line.trim() != "start frames" {
                    break Err(error("frame approval absent"));
                }
                announced = true;
                start_tx.send(()).map_err(error)?;
            }
            Ok(Some(SecureCameraEvent::Stopped)) => break Ok(()),
            Ok(_) => {}
            Err(e) => break Err(e),
        }
        if Instant::now() >= deadline {
            break Err(error("secure graph deadline"));
        }
        thread::sleep(Duration::from_millis(1));
    };
    let source_deadline = owner.source_lease_expires_at_ms();
    let metrics = owner.metrics().unwrap_or_default();
    owner.stop()?;
    let _ = done_tx.send(());
    let peer_result = source_worker.join().map_err(|_| error("sender panicked"))?;
    if !announced {
        return Err(error(format!(
            "never bound: {result:?}; sender: {peer_result:?}"
        )));
    }
    if scenario == "no_consumer" {
        if result.is_ok() || metrics.dropped == 0 {
            return Err(error("expected bounded backpressure rejection absent"));
        }
        println!(
            "CAMERA_STOPPED expected_backpressure=true dropped={}",
            metrics.dropped
        );
        return Ok(());
    }
    if scenario == "control_stop"
        && !result
            .as_ref()
            .is_err_and(|e| e.to_string().contains("peer stopped camera"))
    {
        return Err(error("explicit peer Stop was not the terminal reason"));
    }
    if scenario == "control_stop" {
        println!("VERIFIED control_stop");
    }
    if scenario == "sink_revoke" {
        if !result.as_ref().is_err_and(|e| {
            e.to_string().contains("lease") || e.to_string().contains("binding changed")
        }) {
            return Err(error("Sink revoke was not the terminal reason"));
        }
        println!("VERIFIED sink_revoke");
    }
    if scenario == "expiry" {
        let source_deadline =
            source_deadline.ok_or_else(|| error("Source lease never acquired"))?;
        if now() < source_deadline || source_deadline > started_ms + ttl || result.is_ok() {
            return Err(error(
                "camera failed before actual finite Source lease expiry",
            ));
        }
        println!(
            "FINITE_SOURCE_LEASE_EXPIRED elapsed_ms={} lease_deadline_ms={}",
            now() - started_ms,
            source_deadline
        );
    }
    result?;
    peer_result?;
    println!(
        "CAMERA_STOPPED queued={} dropped={}",
        metrics.queued, metrics.dropped
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_consent_prepares_source_without_faking_its_peer_lease() {
        let (sink, source, id, binding) = fixture(now(), 12000);
        assert_eq!(sink.route(id).unwrap().state, RouteState::Starting);
        assert_eq!(source.route(id).unwrap().state, RouteState::Prepared);
        assert_eq!(source.route(id).unwrap().session_id, binding.session_id);
        assert!(
            source
                .route_port_lease_claim(id, source.route(id).unwrap().source)
                .is_none()
        );
        assert!(
            sink.route_port_lease_claim(id, sink.route(id).unwrap().sink)
                .is_some()
        );
    }
    #[test]
    fn authenticated_source_lease_precedes_offer_and_media_children() {
        delayed_lease_negotiation(Duration::from_millis(100), true);
    }
    #[test]
    fn over_budget_lease_receipt_fails_without_opening_media() {
        delayed_lease_negotiation(Duration::from_millis(1100), false);
    }
    fn delayed_lease_negotiation(delay: Duration, accepted: bool) {
        let (sink, mut source, id, binding) = fixture(now(), 4000);
        let (identity, credential, pin) = identities();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = AuthenticatedControlServer::from_listener(
            listener,
            identity.clone(),
            binding,
            Duration::from_secs(2),
        )
        .unwrap();
        let accept = thread::spawn(move || server.accept().unwrap());
        let client = AuthenticatedControlClientChannel::connect(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(2),
            },
            "localhost",
            ControlTlsClientIdentity::from_bundle(
                pin,
                ControlCredentialBundle::decode(&credential).unwrap(),
            ),
            binding,
        )
        .unwrap();
        let link = CatalogControlLink::from_accepted(accept.join().unwrap());
        let mut peer = CatalogControlLink::from_dialed(client);
        let mut owner = SecureCameraOwner::new(
            sink,
            link,
            identity,
            SecureCameraAdmission {
                expected: binding,
                route_id: id,
                stream_id: StreamId::new(),
                local_address: Ipv4Addr::LOCALHOST.into(),
                lifetime: Duration::from_secs(10),
            },
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let ReceivedControlMessage::PortLeaseRequest(request) =
            peer.receive_control_message_until(deadline).unwrap()
        else {
            panic!("missing real Source lease request")
        };
        assert!(
            source
                .route_port_lease_claim(id, source.route(id).unwrap().source)
                .is_none()
        );
        // Reproduce receipt-clock drift that formerly exceeded the exact grant.
        thread::sleep(delay);
        peer.answer_port_lease_request(&mut source, request, now())
            .unwrap();
        if !accepted {
            assert!(
                source
                    .route_port_lease_claim(id, source.route(id).unwrap().source)
                    .is_none()
            );
            assert!(
                owner
                    .poll()
                    .unwrap_err()
                    .to_string()
                    .contains("negotiation deadline")
            );
            assert!(owner.helper_pid().is_none());
            assert!(owner.decoder_pid().is_none());
            owner.stop().unwrap();
            return;
        }
        source.begin_route_start(id, now()).unwrap();
        loop {
            if owner.poll().unwrap() == Some(SecureCameraEvent::Listening) {
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        let ReceivedControlMessage::RouteStatus(status) =
            peer.receive_control_message_until(deadline).unwrap()
        else {
            panic!("missing secure offer")
        };
        let offer = status.secure_camera_data.unwrap();
        offer.validate(source.route(id).unwrap()).unwrap();
        assert!(
            source
                .route_port_lease_claim(id, source.route(id).unwrap().source)
                .is_some()
        );
        assert!(owner.helper_pid().is_none());
        assert!(owner.decoder_pid().is_none());
        assert!(owner.node_name().is_none());
        owner.stop().unwrap();
    }
}
