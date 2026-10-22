use super::*;
use capyio_core::*;
use capyio_runtime::PortLeasePolicy;

fn nodes() -> (NodeDescriptor, NodeDescriptor, PortRef, PortRef) {
    let mut android = capyio_testkit::android_node();
    let mut linux = capyio_testkit::windows_node();
    linux.platform = Platform::Linux;
    for node in [&mut android, &mut linux] {
        for adapter in node.adapter_instances.values_mut() {
            adapter.state = AdapterState::Ready;
            adapter
                .supported_route_modes
                .insert(RouteBackend::AdapterManaged);
        }
        for cap in node.capabilities.values_mut() {
            cap.availability = Availability::Available;
            for port in cap.ports.values_mut() {
                port.availability = Availability::Available;
                port.interoperability_mode = InteroperabilityMode::AdapterManaged;
                if port.profile == ProfileId::audio_frames_v1() {
                    port.formats = vec![FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)];
                    port.qos_modes = std::collections::BTreeSet::from([QosMode::Interactive]);
                }
            }
        }
    }
    let source_cap = linux
        .capabilities
        .values()
        .find(|c| c.class == CapabilityClass::Custom("system_audio_capture".into()))
        .unwrap();
    let source = PortRef {
        node_id: linux.id,
        capability_id: source_cap.id,
        port_id: *source_cap.ports.keys().next().unwrap(),
    };
    let sink_cap = android
        .capabilities
        .values()
        .find(|c| c.class == CapabilityClass::Speaker)
        .unwrap();
    let sink = PortRef {
        node_id: android.id,
        capability_id: sink_cap.id,
        port_id: *sink_cap.ports.keys().next().unwrap(),
    };
    (android, linux, source, sink)
}
fn prepare(
    runtime: &mut NodeRuntime,
    session: SessionId,
    id: RouteId,
    source: PortRef,
    sink: PortRef,
    now: u64,
) {
    runtime
        .create_route_with_id(id, session, source, sink, RouteBackend::AdapterManaged)
        .unwrap();
    runtime.authorize_route(id, Some(now + 10_000)).unwrap();
    let local = if runtime.snapshot().local_node.id == source.node_id {
        source
    } else {
        sink
    };
    runtime
        .set_local_port_lease_policy(local, PortLeasePolicy::Exclusive)
        .unwrap();
    runtime
        .prepare_route(
            id,
            Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)),
            QosMode::Interactive,
            now,
        )
        .unwrap();
}
fn pure_fixture(now: u64) -> (NodeRuntime, Route, Session, ControlTlsBinding) {
    let (android, linux, source, sink) = nodes();
    let mut runtime = NodeRuntime::new(android).unwrap();
    runtime.register_peer(linux, true).unwrap();
    let session = runtime.open_session(source.node_id).unwrap();
    let id = RouteId::new();
    prepare(&mut runtime, session, id, source, sink, now);
    let route = runtime.route(id).unwrap().clone();
    let session = runtime.session(session).unwrap().clone();
    let binding = ControlTlsBinding::new(session.id, source.node_id, sink.node_id).unwrap();
    (runtime, route, session, binding)
}
fn source_claim(route: &Route, now: u64) -> ValidatedPortLeaseResponse {
    ValidatedPortLeaseResponse {
        message_id: MessageId::new(),
        request_message_id: MessageId::new(),
        session_id: route.session_id,
        route_id: route.id,
        port: route.source,
        outcome: PortLeaseOutcome::Acquired,
        expires_at_ms: Some(now + 5_000),
    }
}

mod pure {
    use super::*;
    #[test]
    fn pcm_enable_requires_bound_unused_gate_and_original_unexpired_cap() {
        let now = Instant::now();
        let end = now + Duration::from_secs(1);
        for (bound, enabled) in [(false, false), (false, true), (true, true)] {
            assert!(validate_pcm_enable(bound, enabled, now, end).is_err());
        }
        validate_pcm_enable(true, false, now, end).unwrap();
        assert!(validate_pcm_enable(true, false, end, end).is_err());
        assert!(validate_pcm_enable(true, false, end + Duration::from_secs(1), end).is_err());
    }
    #[test]
    fn original_route_session_profile_qos_and_both_control_roles_are_exact() {
        let (runtime, route, session, binding) = pure_fixture(1000);
        validate_route(&runtime, &route, &session, binding, 1000).unwrap();
        let reversed = ControlTlsBinding::new(
            binding.session_id,
            binding.client_node_id,
            binding.server_node_id,
        )
        .unwrap();
        validate_route(&runtime, &route, &session, reversed, 1000).unwrap();
        for mutate in [
            (|r: &mut Route| r.epoch += 1) as fn(&mut Route),
            |r| r.state = RouteState::Active,
            |r| {
                r.selected_format = Some(FormatDescriptor::new(
                    capyio_protocol::SECURE_AUDIO_MONO_FORMAT,
                ))
            },
            |r| {
                r.selected_format
                    .as_mut()
                    .unwrap()
                    .parameters
                    .insert("extra".into(), "1".into())
                    .map(|_| ())
                    .unwrap_or(())
            },
            |r| r.selected_qos_mode = Some(QosMode::Basic),
            |r| {
                r.authorization = AuthorizationState::Authorized {
                    expires_at_ms: Some(99_999),
                }
            },
            |r| r.sink.port_id = PortId::new(),
            |r| r.source.port_id = PortId::new(),
        ] {
            let mut changed = route.clone();
            mutate(&mut changed);
            assert!(validate_route(&runtime, &changed, &session, binding, 1000).is_err());
        }
        assert!(validate_route(&runtime, &route, &session, binding, 11_000).is_err());
    }
    #[test]
    fn original_sink_claim_cannot_be_replaced_released_or_extended() {
        for mutation in 0..4 {
            let (mut runtime, route, _, _) = pure_fixture(1000);
            runtime
                .acquire_route_port_lease(route.id, route.sink, 5000, 1000)
                .unwrap();
            let claim = *runtime
                .route_port_lease_claim(route.id, route.sink)
                .unwrap();
            validate_sink_claim(&runtime, &route, claim, 1001).unwrap();
            match mutation {
                0 => {
                    runtime
                        .release_route_port_lease(route.id, route.sink, route.session_id)
                        .unwrap();
                }
                1 => {
                    runtime
                        .acquire_route_port_lease(route.id, route.sink, 6000, 1001)
                        .unwrap();
                }
                2 => {
                    runtime.close_session(route.session_id).unwrap();
                }
                _ => {
                    assert!(validate_sink_claim(&runtime, &route, claim, 5000).is_err());
                    continue;
                }
            }
            assert!(validate_sink_claim(&runtime, &route, claim, 1001).is_err());
        }
    }
    #[test]
    fn source_response_matches_one_exact_request_claim_and_finite_expiry() {
        let (_, route, _, _) = pure_fixture(1000);
        let original = source_claim(&route, 1000);
        validate_source_response(&route, original.request_message_id, original, 1000).unwrap();
        for mutate in [
            (|c: &mut ValidatedPortLeaseResponse| c.request_message_id = MessageId::new())
                as fn(&mut ValidatedPortLeaseResponse),
            |c| c.session_id = SessionId::new(),
            |c| c.route_id = RouteId::new(),
            |c| c.port.port_id = PortId::new(),
            |c| c.outcome = PortLeaseOutcome::Renewed,
            |c| c.expires_at_ms = None,
            |c| c.expires_at_ms = Some(1000),
            |c| c.expires_at_ms = Some(12_000),
        ] {
            let mut changed = original;
            mutate(&mut changed);
            assert!(
                validate_source_response(&route, original.request_message_id, changed, 1000)
                    .is_err()
            );
        }
    }
    #[test]
    fn exact_capability_adapter_and_catalog_are_pinned() {
        let (mut runtime, route, _, _) = pure_fixture(1000);
        let pinned = EndpointAuthority::capture(&runtime, route.source).unwrap();
        pinned.check(&runtime, &route, false).unwrap();
        let mut peer = runtime.snapshot().peers[0].clone();
        peer.capabilities
            .get_mut(&route.source.capability_id)
            .unwrap()
            .metadata
            .insert("settingsRevision".into(), "2".into());
        runtime.accept_peer_catalog(route.session_id, peer).unwrap();
        assert!(pinned.check(&runtime, &route, true).is_err());
    }
    #[test]
    fn consent_time_is_not_refreshed_by_negotiation_or_long_duration() {
        let start = Instant::now();
        let original = start + Duration::from_secs(10);
        assert_eq!(
            consent_end(
                start + Duration::from_secs(7),
                Duration::from_secs(10),
                original,
                10_000
            )
            .unwrap(),
            original
        );
        assert!(consent_end(original, Duration::from_secs(10), original, 10_000).is_err());
        let (_, route, _, expected) = pure_fixture(1000);
        for lifetime in [
            Duration::ZERO,
            Duration::from_millis(1500),
            Duration::from_secs(61),
            Duration::MAX,
        ] {
            assert!(
                validate_admission(&SecureStereoSinkAdmission {
                    expected,
                    route_id: route.id,
                    stream_id: StreamId::new(),
                    local_address: "127.0.0.1".parse().unwrap(),
                    lifetime
                })
                .is_err()
            );
        }
        assert!(source_lease_ttl(Duration::from_millis(1249)).is_err());
        assert_eq!(source_lease_ttl(Duration::from_millis(1250)).unwrap(), 1000);
    }
    #[test]
    fn every_partial_write_is_frame_aligned_and_retains_absolute_budget() {
        let now = Instant::now();
        let mut pending =
            PendingPcm::new(vec![0; 1920], now, now + Duration::from_secs(1)).unwrap();
        assert_eq!(pending.before_write(now).unwrap().len(), 1920);
        assert_eq!(
            pending
                .after_write(Ok(4), now + Duration::from_millis(200))
                .unwrap(),
            (4, false)
        );
        assert_eq!(
            pending
                .before_write(now + Duration::from_millis(249))
                .unwrap()
                .len(),
            1916
        );
        assert!(
            pending
                .after_write(Ok(4), now + MAX_STEREO_OPERATION)
                .is_err()
        );
        for count in [1, 2, 3, 5, 1924, usize::MAX] {
            let mut pending =
                PendingPcm::new(vec![0; 1920], now, now + Duration::from_secs(1)).unwrap();
            pending.before_write(now).unwrap();
            assert!(pending.after_write(Ok(count), now).is_err());
        }
    }
    #[test]
    fn zero_backpressure_and_interrupted_writes_have_bounded_attempts() {
        let now = Instant::now();
        for kind in [
            None,
            Some(io::ErrorKind::WouldBlock),
            Some(io::ErrorKind::Interrupted),
        ] {
            let mut pending =
                PendingPcm::new(vec![0; 1920], now, now + Duration::from_secs(1)).unwrap();
            for _ in 0..MAX_WRITE_ATTEMPTS {
                pending.before_write(now).unwrap();
                let result = kind.map(|k| Err(io::Error::from(k))).unwrap_or(Ok(0));
                assert_eq!(pending.after_write(result, now).unwrap(), (0, false));
            }
            assert!(pending.before_write(now).is_err());
            assert_eq!(pending.offset, 0);
        }
    }
    #[test]
    fn slow_or_failed_completion_cannot_be_counted_as_accepted() {
        let now = Instant::now();
        let mut pending =
            PendingPcm::new(vec![0; 1920], now, now + Duration::from_secs(1)).unwrap();
        pending.before_write(now).unwrap();
        assert!(
            pending
                .after_write(Ok(1920), now + MAX_STEREO_OPERATION)
                .is_err()
        );
        assert_eq!(pending.offset, 0);
        assert!(
            pending
                .after_write(Err(io::Error::from(io::ErrorKind::BrokenPipe)), now)
                .is_err()
        );
    }
    #[test]
    fn stop_handle_is_terminal_before_transport_registration() {
        let stop = SecureStereoSinkStopHandle::default();
        let clone = stop.clone();
        clone.stop();
        stop.stop();
        assert!(stop.stopped.load(Ordering::Acquire));
    }
}

/// Mandatory hosted-only tests: real synthetic control/media mTLS, no AudioTrack
/// or device. Do not run this module on socket-restricted local executors.
mod hosted {
    use super::*;
    use capyio_audio::AudioMediaPacket;
    use capyio_control_credentials::ControlCredentialBundle;
    use capyio_control_tls::{
        AuthenticatedControlClientChannel, AuthenticatedControlServer, ControlTlsClientIdentity,
        ControlTlsEndpoint, audio_stereo::StereoAudioTlsChannel,
    };
    use capyio_pairing::CertificateFingerprint;
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
        KeyUsagePurpose,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone)]
    struct FakeSink {
        writes: Arc<AtomicUsize>,
        accepted: Arc<Mutex<Vec<u8>>>,
        limit: usize,
        stop_during_write: Option<SecureStereoSinkStopHandle>,
        stops: Arc<AtomicUsize>,
    }
    impl PcmSink for FakeSink {
        fn try_write_pcm(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes.fetch_add(1, Ordering::AcqRel);
            let n = bytes.len().min(self.limit);
            self.accepted.lock().unwrap().extend_from_slice(&bytes[..n]);
            if let Some(stop) = &self.stop_during_write {
                stop.stop();
            }
            Ok(n)
        }
        fn stop(&mut self) -> io::Result<()> {
            self.stops.fetch_add(1, Ordering::AcqRel);
            Ok(())
        }
    }
    struct Fixture {
        owner: SecureStereoSinkOwner<FakeSink>,
        source: CatalogControlLink,
        runtime: NodeRuntime,
        identity: ControlTlsClientIdentity,
        route: RouteId,
        sink_request: MessageId,
    }
    fn credentials() -> (
        ControlTlsServerIdentity,
        ControlTlsClientIdentity,
        ControlTlsServerIdentity,
        ControlTlsClientIdentity,
    ) {
        let mut ca = CertificateParams::new(Vec::<String>::new()).unwrap();
        ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
        ];
        let key = KeyPair::generate().unwrap();
        let root = ca.self_signed(&key).unwrap().der().to_vec();
        let issuer = Issuer::new(ca, key);
        let leaf = || {
            let mut params = CertificateParams::new(vec!["localhost".into()]).unwrap();
            params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
            params.extended_key_usages = vec![
                ExtendedKeyUsagePurpose::ClientAuth,
                ExtendedKeyUsagePurpose::ServerAuth,
            ];
            let key = KeyPair::generate().unwrap();
            let cert = params.signed_by(&key, &issuer).unwrap().der().to_vec();
            let pin = CertificateFingerprint::from_certificate_der(&cert).unwrap();
            (
                ControlCredentialBundle::new(
                    vec![cert, root.clone()],
                    key.serialize_der(),
                    root.clone(),
                )
                .unwrap(),
                pin,
            )
        };
        let (android, android_pin) = leaf();
        let (linux, linux_pin) = leaf();
        let android = ControlTlsServerIdentity::from_bundle(linux_pin, android);
        let linux = ControlTlsServerIdentity::from_bundle(android_pin, linux);
        let client = |server: &ControlTlsServerIdentity| ControlTlsClientIdentity {
            expected_peer_certificate: server.expected_peer_certificate,
            certificate_chain_der: server.certificate_chain_der.clone(),
            private_key_der: server.private_key_der.clone(),
            server_ca_der: server.client_ca_der.clone(),
        };
        let android_client = client(&android);
        let linux_client = client(&linux);
        (android, android_client, linux, linux_client)
    }
    impl Fixture {
        fn new(android_server: bool, limit: usize) -> Self {
            Self::with_consent(android_server, limit, Duration::from_secs(10))
        }
        fn with_consent(android_server: bool, limit: usize, lifetime: Duration) -> Self {
            let (android, linux, source_port, sink_port) = nodes();
            let (android_server_id, android_client_id, linux_server_id, linux_client_id) =
                credentials();
            let binding = if android_server {
                ControlTlsBinding::new(SessionId::new(), android.id, linux.id).unwrap()
            } else {
                ControlTlsBinding::new(SessionId::new(), linux.id, android.id).unwrap()
            };
            let mut sink_runtime = NodeRuntime::new(android.clone()).unwrap();
            sink_runtime.register_peer(linux.clone(), true).unwrap();
            let mut source_runtime = NodeRuntime::new(linux).unwrap();
            source_runtime.register_peer(android, true).unwrap();
            let socket = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = socket.local_addr().unwrap();
            let server = AuthenticatedControlServer::from_listener(
                socket,
                if android_server {
                    android_server_id.clone()
                } else {
                    linux_server_id
                },
                binding,
                Duration::from_secs(2),
            )
            .unwrap();
            let accept = thread::spawn(move || server.accept().unwrap());
            let dial = AuthenticatedControlClientChannel::connect(
                ControlTlsEndpoint {
                    address,
                    io_timeout: Duration::from_secs(2),
                },
                "localhost",
                if android_server {
                    linux_client_id.clone()
                } else {
                    android_client_id
                },
                binding,
            )
            .unwrap();
            let accepted = accept.join().unwrap();
            let (sink_link, source_link) = if android_server {
                (
                    CatalogControlLink::admit_accepted_peer(
                        &mut sink_runtime,
                        accepted,
                        "Linux".into(),
                    )
                    .unwrap(),
                    CatalogControlLink::admit_dialed_peer(
                        &mut source_runtime,
                        dial,
                        "Android".into(),
                    )
                    .unwrap(),
                )
            } else {
                (
                    CatalogControlLink::admit_dialed_peer(&mut sink_runtime, dial, "Linux".into())
                        .unwrap(),
                    CatalogControlLink::admit_accepted_peer(
                        &mut source_runtime,
                        accepted,
                        "Android".into(),
                    )
                    .unwrap(),
                )
            };
            let now = now_ms().unwrap();
            let route = RouteId::new();
            prepare(
                &mut sink_runtime,
                binding.session_id,
                route,
                source_port,
                sink_port,
                now,
            );
            prepare(
                &mut source_runtime,
                binding.session_id,
                route,
                source_port,
                sink_port,
                now,
            );
            let sink = FakeSink {
                writes: Default::default(),
                accepted: Default::default(),
                limit,
                stop_during_write: None,
                stops: Default::default(),
            };
            let owner = SecureStereoSinkOwner::new_until(
                sink_runtime,
                sink_link,
                android_server_id,
                SecureStereoSinkAdmission {
                    expected: binding,
                    route_id: route,
                    stream_id: StreamId::new(),
                    local_address: address.ip(),
                    lifetime,
                },
                Instant::now() + lifetime,
                sink,
            )
            .unwrap();
            Self {
                owner,
                source: source_link,
                runtime: source_runtime,
                identity: linux_client_id,
                route,
                sink_request: MessageId::new(),
            }
        }
        fn send_sink_request(&mut self) {
            let route = self.runtime.route(self.route).unwrap();
            let ttl = u64::try_from(
                self.owner
                    .local_consent_end
                    .saturating_duration_since(Instant::now())
                    .as_millis(),
            )
            .unwrap()
            .saturating_sub(250)
            .min(5000);
            self.sink_request = self
                .source
                .send_port_lease_request(route.id, route.sink, PortLeaseAction::Acquire, ttl)
                .unwrap();
        }
        fn answer_source(&mut self) {
            let ReceivedControlMessage::PortLeaseRequest(request) =
                self.source.receive_control_message().unwrap()
            else {
                panic!("Source Acquire absent")
            };
            self.source
                .answer_port_lease_request_until(
                    &mut self.runtime,
                    request,
                    now_ms().unwrap(),
                    Instant::now() + MAX_STEREO_OPERATION,
                )
                .unwrap();
        }
        fn start(&mut self) {
            self.runtime
                .begin_route_start(self.route, now_ms().unwrap())
                .unwrap();
            self.source
                .send_route_command(self.route, RouteCommandAction::Start)
                .unwrap();
        }
        fn listen(&mut self, response_first: bool) {
            if response_first {
                self.answer_source();
                self.owner.poll().unwrap();
                assert!(self.owner.local_offer().is_none());
                self.send_sink_request();
            } else {
                self.send_sink_request();
                self.owner.poll().unwrap();
                self.answer_source();
            }
            for _ in 0..3 {
                self.owner.poll().unwrap();
            }
            assert!(self.owner.local_offer().is_none());
            self.start();
            while self.owner.local_offer().is_none() {
                self.owner.poll().unwrap();
            }
            let ReceivedControlMessage::PortLeaseResponse(response) =
                self.source.receive_control_message().unwrap()
            else {
                panic!("Sink claim absent")
            };
            assert_eq!(response.request_message_id, self.sink_request);
            assert_eq!(response.outcome, PortLeaseOutcome::Acquired);
        }
        fn connect(&mut self) -> StereoAudioTlsChannel {
            let channel = self.connect_without_pcm();
            self.owner.enable_pcm().unwrap();
            channel
        }
        fn connect_without_pcm(&mut self) -> StereoAudioTlsChannel {
            let ReceivedControlMessage::RouteStatus(status) =
                self.source.receive_control_message().unwrap()
            else {
                panic!("offer absent")
            };
            let offer = status.secure_audio_data.unwrap();
            assert_eq!(status.route, *self.runtime.route(self.route).unwrap());
            let binding = StereoAudioTlsBinding::from_route(
                self.source.tls_binding(),
                &status.route,
                offer.stream_id,
                now_ms().unwrap(),
            )
            .unwrap();
            let channel = StereoAudioTlsChannel::connect_until(
                ControlTlsEndpoint {
                    address: self.owner.listener_address().unwrap(),
                    io_timeout: MAX_STEREO_OPERATION,
                },
                "localhost",
                self.identity.clone(),
                binding,
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
            while !self.owner.bound {
                self.owner.poll().unwrap();
            }
            channel
        }
        fn record(&self, sequence: u64) -> Vec<u8> {
            let binding = AudioMediaStreamBinding {
                session_id: self.owner.original.session_id,
                route_id: self.route,
                stream_id: self.owner.admission.stream_id,
                stream_epoch: self.owner.original.epoch as u32,
                selected_spec: AudioStreamSpec::media_balanced(),
            };
            let mut parser = SecureStereoPcmAdmission::new(binding.clone()).unwrap();
            let mut record = Vec::new();
            for n in 0..=sequence {
                let payload = (0..480).flat_map(|_| [0x12, 0x34, 0x56, 0x78]).collect();
                record = parser
                    .encode_record(&AudioMediaPacket {
                        stream_id: binding.stream_id,
                        stream_epoch: binding.stream_epoch,
                        sequence: n,
                        source_timestamp_micros: 100 + n * 10_000,
                        first_sample_index: n * 480,
                        sample_count: 480,
                        discontinuity: false,
                        payload,
                    })
                    .unwrap()
                    .to_vec();
            }
            record
        }
        fn until_record(&mut self) {
            let deadline = Instant::now() + MAX_STEREO_OPERATION;
            while self.owner.metrics().records_accepted == 0 {
                self.owner.poll().unwrap();
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
    #[test]
    fn actual_tls_crossed_claim_orders_and_both_control_roles_preserve_stereo() {
        for android_server in [false, true] {
            for response_first in [false, true] {
                let mut f = Fixture::new(android_server, 1920);
                f.listen(response_first);
                let mut media = f.connect();
                media.send_record(&f.record(0)).unwrap();
                f.until_record();
                assert_eq!(
                    f.owner.sink.accepted.lock().unwrap().as_slice(),
                    (0..480)
                        .flat_map(|_| [0x12, 0x34, 0x56, 0x78])
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    f.owner.runtime().route(f.route).unwrap().state,
                    RouteState::Starting
                );
                assert_eq!(f.owner.metrics().records_accepted, 1);
                f.owner.stop().unwrap();
                assert_eq!(f.owner.sink.stops.load(Ordering::Acquire), 1);
                assert!(
                    f.owner
                        .runtime()
                        .route_port_lease_claim(f.route, f.owner.original.sink)
                        .is_none()
                );
            }
        }
    }
    #[test]
    fn partial_write_rechecks_lease_before_every_continuation() {
        let mut f = Fixture::new(false, 4);
        f.listen(false);
        let mut media = f.connect();
        media.send_record(&f.record(0)).unwrap();
        while f.owner.sink.writes.load(Ordering::Acquire) == 0 {
            f.owner.poll().unwrap();
        }
        let writes = f.owner.sink.writes.load(Ordering::Acquire);
        let route = f.owner.original.clone();
        f.owner
            .runtime_mut()
            .release_route_port_lease(route.id, route.sink, route.session_id)
            .unwrap();
        assert!(f.owner.poll().is_err());
        assert_eq!(f.owner.sink.writes.load(Ordering::Acquire), writes);
        assert_eq!(f.owner.metrics().records_accepted, 0);
    }
    #[test]
    fn stop_during_write_is_rechecked_before_success_and_next_record() {
        let mut f = Fixture::new(true, 1920);
        f.listen(true);
        let mut media = f.connect();
        f.owner.sink.stop_during_write = Some(f.owner.stop_handle());
        media.send_record(&f.record(0)).unwrap();
        while !f.owner.is_closed() {
            let _ = f.owner.poll();
        }
        assert_eq!(f.owner.sink.writes.load(Ordering::Acquire), 1);
        assert_eq!(f.owner.metrics().records_accepted, 0);
    }
    #[test]
    fn duplicate_start_and_acquire_are_terminal_without_renewal() {
        for duplicate_start in [false, true] {
            let mut f = Fixture::new(true, 1920);
            f.listen(false);
            if duplicate_start {
                f.source
                    .send_route_command(f.route, RouteCommandAction::Start)
                    .unwrap();
            } else {
                f.send_sink_request();
            }
            assert!(f.owner.poll().is_err());
            assert!(f.owner.is_closed());
            assert!(
                f.owner
                    .runtime()
                    .route_port_lease_claim(f.route, f.owner.original.sink)
                    .is_none()
            );
        }
    }
    #[test]
    fn missing_start_no_listener_and_stop_releases_partial_claim() {
        let mut f = Fixture::new(false, 1920);
        f.send_sink_request();
        f.owner.poll().unwrap();
        assert!(f.owner.sink_claim.is_some());
        assert!(f.owner.local_offer().is_none());
        f.source
            .send_route_command(f.route, RouteCommandAction::Stop)
            .unwrap();
        assert_eq!(
            f.owner.poll().unwrap(),
            Some(SecureStereoSinkEvent::Stopped)
        );
        assert!(
            f.owner
                .runtime()
                .route_port_lease_claim(f.route, f.owner.original.sink)
                .is_none()
        );
    }
    #[test]
    fn malformed_second_fragment_and_replay_are_terminal() {
        for replay in [false, true] {
            let mut f = Fixture::new(true, 1920);
            f.listen(false);
            let mut media = f.connect();
            let mut record = f.record(0);
            if replay {
                media.send_record(&record).unwrap();
                f.until_record();
            } else {
                record[1200] ^= 1;
            }
            media.send_record(&record).unwrap();
            while !f.owner.is_closed() {
                let _ = f.owner.poll();
            }
            assert_eq!(f.owner.metrics().records_accepted, u64::from(replay));
        }
    }
    #[test]
    fn stop_interrupts_accept_and_pending_read_without_reconnect() {
        for connected in [false, true] {
            let mut f = Fixture::new(false, 1920);
            f.listen(false);
            let _channel = connected.then(|| f.connect());
            let start = Instant::now();
            f.owner.stop_handle().stop();
            assert!(f.owner.poll().is_err());
            assert!(start.elapsed() < Duration::from_secs(1));
            assert!(f.owner.worker.is_none());
            assert!(f.owner.poll().is_err());
        }
    }
    #[test]
    fn control_eof_and_zero_sink_backpressure_are_terminal() {
        for eof in [false, true] {
            let mut f = Fixture::new(true, 0);
            f.listen(true);
            let mut media = f.connect();
            if eof {
                f.source.close();
            } else {
                media.send_record(&f.record(0)).unwrap();
            }
            let start = Instant::now();
            while !f.owner.is_closed() {
                let _ = f.owner.poll();
                thread::sleep(Duration::from_millis(1));
                assert!(start.elapsed() < Duration::from_secs(2));
            }
            assert_eq!(f.owner.metrics().records_accepted, 0);
        }
    }
    #[test]
    fn authenticated_start_cannot_publish_before_both_original_claims() {
        let mut f = Fixture::new(false, 1920);
        f.source
            .send_route_command(f.route, RouteCommandAction::Start)
            .unwrap();
        f.owner.poll().unwrap();
        assert!(f.owner.authenticated_start);
        assert!(f.owner.local_offer().is_none());
        assert_eq!(f.owner.original.state, RouteState::Prepared);
        f.send_sink_request();
        f.owner.poll().unwrap();
        assert!(f.owner.local_offer().is_none());
        f.answer_source();
        while f.owner.local_offer().is_none() {
            f.owner.poll().unwrap();
        }
        assert_eq!(f.owner.original.state, RouteState::Starting);
        f.owner.stop().unwrap();
    }
    #[test]
    fn authenticated_response_replay_is_rejected_without_replacing_claim() {
        let mut f = Fixture::new(true, 1920);
        f.answer_source();
        f.owner.poll().unwrap();
        let original = f.owner.source_claim.unwrap();
        let deadline = f.owner.operation_deadline();
        // Replay the already authenticated decoded message at the owner boundary;
        // actual TLS response correlation is exercised by the crossed-order test.
        assert!(
            f.owner
                .handle_control(
                    ReceivedControlMessage::PortLeaseResponse(original),
                    deadline
                )
                .is_err()
        );
        assert_eq!(f.owner.source_claim, Some(original));
        f.owner.stop_after_failure().unwrap();
        assert!(f.owner.local_offer().is_none());
    }
    #[test]
    fn stop_interrupts_partial_tls_handshake() {
        use std::io::Write;
        let mut f = Fixture::new(false, 1920);
        f.listen(false);
        let mut raw = std::net::TcpStream::connect(f.owner.listener_address().unwrap()).unwrap();
        // A partial TLS record reaches the actual listener; no plaintext media.
        raw.write_all(&[22, 3, 3, 0, 100, 1]).unwrap();
        thread::sleep(Duration::from_millis(10));
        let start = Instant::now();
        f.owner.stop_handle().stop();
        assert!(f.owner.poll().is_err());
        assert!(f.owner.worker.is_none());
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_eq!(f.owner.sink.writes.load(Ordering::Acquire), 0);
    }
    #[test]
    fn queued_record_never_refreshes_its_original_permit_deadline() {
        let mut f = Fixture::new(true, 1920);
        f.listen(false);
        let mut media = f.connect();
        media.send_record(&f.record(0)).unwrap();
        // The successful network read does not grant a fresh write budget when
        // host polling resumes after the original permit has elapsed.
        thread::sleep(MAX_STEREO_OPERATION + Duration::from_millis(20));
        assert!(f.owner.poll().is_err());
        assert_eq!(f.owner.metrics().records_accepted, 0);
        assert_eq!(f.owner.sink.writes.load(Ordering::Acquire), 0);
    }
    #[test]
    fn no_enable_keeps_control_stop_and_original_finite_expiry_supervised() {
        for stop in [true, false] {
            let mut f = Fixture::with_consent(false, 1920, Duration::from_secs(2));
            f.listen(false);
            let _media = f.connect_without_pcm();
            let original_end = f.owner.end;
            thread::sleep(MAX_STEREO_OPERATION + Duration::from_millis(20));
            f.owner.poll().unwrap();
            assert_eq!(f.owner.end, original_end);
            assert!(!f.owner.read_pending);
            assert!(f.owner.pending.is_none());
            assert_eq!(f.owner.sink.writes.load(Ordering::Acquire), 0);
            if stop {
                f.source
                    .send_route_command(f.route, RouteCommandAction::Stop)
                    .unwrap();
                assert_eq!(
                    f.owner.poll().unwrap(),
                    Some(SecureStereoSinkEvent::Stopped)
                );
                assert_eq!(f.owner.terminal_cause(), Some("peer_stop"));
            } else {
                while Instant::now() < original_end {
                    if f.owner.poll().is_err() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(2));
                }
                if !f.owner.is_closed() {
                    assert!(f.owner.poll().is_err());
                }
                assert_eq!(f.owner.end, original_end);
                assert_eq!(f.owner.terminal_cause(), Some("authority_expired"));
            }
            assert!(f.owner.is_closed());
            assert!(
                f.owner
                    .runtime()
                    .route_port_lease_claim(f.route, f.owner.original.sink)
                    .is_none()
            );
            assert_eq!(f.owner.sink.writes.load(Ordering::Acquire), 0);
        }
    }
    #[test]
    fn enable_is_one_shot_rejects_before_bound_and_after_original_expiry() {
        let mut early = Fixture::new(true, 1920);
        assert!(early.owner.enable_pcm().is_err());
        assert!(early.owner.is_closed());
        assert_eq!(early.owner.terminal_cause(), Some("invalid_pcm_enable"));
        let mut expired = Fixture::with_consent(true, 1920, Duration::from_secs(2));
        expired.listen(true);
        let _media = expired.connect_without_pcm();
        let original_end = expired.owner.end;
        thread::sleep(
            original_end.saturating_duration_since(Instant::now()) + Duration::from_millis(2),
        );
        assert!(expired.owner.enable_pcm().is_err());
        assert_eq!(expired.owner.end, original_end);
        assert_eq!(expired.owner.terminal_cause(), Some("authority_expired"));
        assert!(!expired.owner.read_pending);
        assert_eq!(expired.owner.sink.writes.load(Ordering::Acquire), 0);
        let mut repeated = Fixture::new(false, 1920);
        repeated.listen(false);
        let _media = repeated.connect();
        assert!(repeated.owner.enable_pcm().is_err());
        assert_eq!(repeated.owner.terminal_cause(), Some("invalid_pcm_enable"));
    }
    #[test]
    fn delayed_first_enable_does_not_refresh_original_authority() {
        let mut f = Fixture::new(true, 1920);
        f.listen(false);
        let mut media = f.connect_without_pcm();
        let original_end = f.owner.end;
        thread::sleep(MAX_STEREO_OPERATION + Duration::from_millis(20));
        f.owner.poll().unwrap();
        f.owner.enable_pcm().unwrap();
        assert_eq!(f.owner.end, original_end);
        media.send_record(&f.record(0)).unwrap();
        f.until_record();
        assert_eq!(f.owner.metrics().records_accepted, 1);
    }
}
