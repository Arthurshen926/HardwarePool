//! Deterministic snapshot/commit tests and mandatory real pinned-control tests.
//! The fixture owns the authoritative Runtime in a mutex. All TLS operations
//! occur outside its guard; no Runtime clone authorizes negotiation or media.
use super::*;
use capyio_core::*;
use capyio_protocol::{PortLeaseOutcome, SECURE_AUDIO_STEREO_FORMAT};
use capyio_runtime::PortLeasePolicy;
use std::sync::{Arc, Mutex};

pub(crate) mod fixture {
    use super::*;
    use capyio_control_credentials::ControlCredentialBundle;
    use capyio_control_link::ReceivedPortLeaseRequest;
    use capyio_control_tls::{
        AuthenticatedControlClientChannel, AuthenticatedControlServer, ControlTlsClientIdentity,
        ControlTlsEndpoint, ControlTlsServerIdentity,
    };
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
        KeyUsagePurpose,
    };
    use std::{
        net::TcpListener,
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    pub(crate) const CONTROL_GENERATION: u64 = 17;
    pub(crate) fn wall_now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .try_into()
            .unwrap()
    }
    pub(crate) fn nodes() -> (NodeDescriptor, NodeDescriptor, PortRef, PortRef) {
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
            for capability in node.capabilities.values_mut() {
                capability.availability = Availability::Available;
                for port in capability.ports.values_mut() {
                    port.availability = Availability::Available;
                    port.interoperability_mode = InteroperabilityMode::AdapterManaged;
                    if port.profile == ProfileId::audio_frames_v1() {
                        port.formats = vec![FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)];
                        port.qos_modes = std::collections::BTreeSet::from([QosMode::Interactive]);
                    }
                }
            }
        }
        let capability = linux
            .capabilities
            .values()
            .find(|capability| {
                capability.class == CapabilityClass::Custom("system_audio_capture".into())
            })
            .unwrap();
        let source = PortRef {
            node_id: linux.id,
            capability_id: capability.id,
            port_id: *capability.ports.keys().next().unwrap(),
        };
        let capability = android
            .capabilities
            .values()
            .find(|capability| capability.class == CapabilityClass::Speaker)
            .unwrap();
        let sink = PortRef {
            node_id: android.id,
            capability_id: capability.id,
            port_id: *capability.ports.keys().next().unwrap(),
        };
        (android, linux, source, sink)
    }
    fn prepare_route(
        runtime: &mut NodeRuntime,
        session: SessionId,
        route: RouteId,
        source: PortRef,
        sink: PortRef,
        now: u64,
    ) {
        runtime
            .create_route_with_id(route, session, source, sink, RouteBackend::AdapterManaged)
            .unwrap();
        runtime.authorize_route(route, Some(now + 10_000)).unwrap();
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
                route,
                Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)),
                QosMode::Interactive,
                now,
            )
            .unwrap();
    }
    pub(crate) fn pure_prepared(now: u64) -> (NodeRuntime, RouteId) {
        let (android, linux, source, sink) = nodes();
        let mut runtime = NodeRuntime::new(android).unwrap();
        runtime.register_peer(linux, true).unwrap();
        let session_id = runtime.open_session(source.node_id).unwrap();
        let route_id = RouteId::new();
        prepare_route(&mut runtime, session_id, route_id, source, sink, now);
        (runtime, route_id)
    }
    pub(crate) fn pure_snapshot(now: u64) -> (NodeRuntime, StereoSinkSnapshot) {
        let (mut runtime, route_id) = pure_prepared(now);
        let route = runtime.route(route_id).unwrap();
        let (session_id, source, sink) = (route.session_id, route.source, route.sink);
        let endpoints = [
            EndpointAuthority::capture(&runtime, source).unwrap(),
            EndpointAuthority::capture(&runtime, sink).unwrap(),
        ];
        runtime
            .acquire_route_port_lease(route_id, sink, now + 5_000, now)
            .unwrap();
        let sink_claim = *runtime.route_port_lease_claim(route_id, sink).unwrap();
        runtime.begin_route_start(route_id, now).unwrap();
        let original = runtime.route(route_id).unwrap().clone();
        let session = runtime.session(session_id).unwrap().clone();
        let expected = ControlTlsBinding::new(session_id, sink.node_id, source.node_id).unwrap();
        let request = MessageId::new();
        let instant = Instant::now();
        let snapshot = StereoSinkSnapshot {
            admission: SecureStereoSinkAdmission {
                expected,
                route_id,
                stream_id: StreamId::new(),
                local_address: "127.0.0.1".parse().unwrap(),
                lifetime: Duration::from_secs(10),
            },
            original,
            session,
            endpoints,
            sink_claim,
            source_claim: ValidatedPortLeaseResponse {
                message_id: MessageId::new(),
                request_message_id: request,
                session_id,
                route_id,
                port: source,
                outcome: PortLeaseOutcome::Acquired,
                expires_at_ms: Some(now + 4_000),
            },
            source_request: request,
            control_generation: CONTROL_GENERATION,
            peer_certificate: CertificateFingerprint::new([1; 32]).unwrap(),
            started: instant,
            end: instant + Duration::from_secs(4),
            wall_floor: now,
        };
        (runtime, snapshot)
    }
    pub(crate) fn credentials() -> (
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
    // The initial request may be received before either crossed claim is
    // answered. Preserve the authenticated object itself, not reconstructed
    // bytes or metadata, and consume it once when Source response is due.
    pub(crate) fn take_source_acquire<T>(
        deferred: &mut Option<T>,
        receive: impl FnOnce() -> T,
    ) -> T {
        deferred.take().unwrap_or_else(receive)
    }
    pub(crate) struct Fixture {
        pub(crate) runtime: Arc<Mutex<NodeRuntime>>,
        pub(crate) source_runtime: NodeRuntime,
        pub(crate) sink: CatalogControlLink,
        pub(crate) source: CatalogControlLink,
        pub(crate) negotiation: StereoSinkNegotiation,
        pub(crate) pending_request: Option<PreparedPortLeaseRequest>,
        deferred_source_request: Option<ReceivedPortLeaseRequest>,
        pub(crate) identity: ControlTlsServerIdentity,
        pub(crate) source_identity: ControlTlsClientIdentity,
        pub(crate) original_end: Instant,
        pub(crate) route_id: RouteId,
    }
    impl Fixture {
        pub(crate) fn new(android_accepts_control: bool) -> Self {
            Self::with_lifetime(android_accepts_control, Duration::from_secs(10))
        }
        pub(crate) fn with_lifetime(android_accepts_control: bool, lifetime: Duration) -> Self {
            let (android, linux, source_port, sink_port) = nodes();
            let (android_identity, android_client, linux_identity, linux_client) = credentials();
            let binding = if android_accepts_control {
                ControlTlsBinding::new(SessionId::new(), android.id, linux.id).unwrap()
            } else {
                ControlTlsBinding::new(SessionId::new(), linux.id, android.id).unwrap()
            };
            let mut sink_runtime = NodeRuntime::new(android.clone()).unwrap();
            sink_runtime.register_peer(linux.clone(), true).unwrap();
            let mut source_runtime = NodeRuntime::new(linux).unwrap();
            source_runtime.register_peer(android, true).unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = AuthenticatedControlServer::from_listener(
                listener,
                if android_accepts_control {
                    android_identity.clone()
                } else {
                    linux_identity
                },
                binding,
                Duration::from_secs(2),
            )
            .unwrap();
            let accepting = thread::spawn(move || server.accept().unwrap());
            let dialed = AuthenticatedControlClientChannel::connect(
                ControlTlsEndpoint {
                    address,
                    io_timeout: Duration::from_secs(2),
                },
                "localhost",
                if android_accepts_control {
                    linux_client.clone()
                } else {
                    android_client
                },
                binding,
            )
            .unwrap();
            let accepted = accepting.join().unwrap();
            let (sink, source) = if android_accepts_control {
                (
                    CatalogControlLink::admit_accepted_peer(
                        &mut sink_runtime,
                        accepted,
                        "Linux fixture".into(),
                    )
                    .unwrap(),
                    CatalogControlLink::admit_dialed_peer(
                        &mut source_runtime,
                        dialed,
                        "Android fixture".into(),
                    )
                    .unwrap(),
                )
            } else {
                (
                    CatalogControlLink::admit_dialed_peer(
                        &mut sink_runtime,
                        dialed,
                        "Linux fixture".into(),
                    )
                    .unwrap(),
                    CatalogControlLink::admit_accepted_peer(
                        &mut source_runtime,
                        accepted,
                        "Android fixture".into(),
                    )
                    .unwrap(),
                )
            };
            let now = wall_now();
            let route_id = RouteId::new();
            prepare_route(
                &mut sink_runtime,
                binding.session_id,
                route_id,
                source_port,
                sink_port,
                now,
            );
            prepare_route(
                &mut source_runtime,
                binding.session_id,
                route_id,
                source_port,
                sink_port,
                now,
            );
            let runtime = Arc::new(Mutex::new(sink_runtime));
            let original_end = Instant::now() + lifetime;
            let (negotiation, request) = {
                let guard = runtime.lock().unwrap();
                StereoSinkNegotiation::prepare(
                    &guard,
                    &sink,
                    SecureStereoSinkAdmission {
                        expected: binding,
                        route_id,
                        stream_id: StreamId::new(),
                        local_address: address.ip(),
                        lifetime,
                    },
                    original_end,
                    CONTROL_GENERATION,
                    now,
                    Instant::now(),
                )
                .unwrap()
            };
            Self {
                runtime,
                source_runtime,
                sink,
                source,
                negotiation,
                pending_request: Some(request),
                deferred_source_request: None,
                identity: android_identity,
                source_identity: linux_client,
                original_end,
                route_id,
            }
        }
        pub(crate) fn assert_unlocked(&self) {
            assert!(
                self.runtime.try_lock().is_ok(),
                "network action retained authoritative Runtime guard"
            );
        }
        pub(crate) fn send_request(&mut self, commit: bool) -> MessageId {
            self.assert_unlocked();
            let sent = self
                .sink
                .send_prepared_port_lease_request_until(
                    self.pending_request.take().unwrap(),
                    self.negotiation.operation_deadline(),
                )
                .unwrap();
            if commit {
                self.negotiation.commit_source_request(sent).unwrap();
            }
            // This only receives the request. In the Sink-first ordering it
            // must stay unanswered until the crossed Sink response commits.
            // Otherwise commit_response would read this earlier FIFO frame
            // instead of the response it is checking.
            assert!(self.deferred_source_request.is_none());
            self.assert_unlocked();
            let ReceivedControlMessage::PortLeaseRequest(request) = self
                .source
                .receive_control_message_until(self.negotiation.operation_deadline())
                .unwrap()
            else {
                panic!("missing exact initial Source Acquire")
            };
            let initial = request.request();
            assert_eq!(initial.message_id, sent);
            assert_eq!(initial.session_id, self.negotiation.route().session_id);
            assert_eq!(initial.route_id, self.route_id);
            assert_eq!(initial.action, PortLeaseAction::Acquire);
            assert_eq!(initial.port, self.negotiation.route().source);
            assert!(
                self.source_runtime
                    .route_port_lease_claim(self.route_id, initial.port)
                    .is_none()
            );
            self.deferred_source_request = Some(request);
            sent
        }
        pub(crate) fn source_response(&mut self) {
            self.assert_unlocked();
            let request = take_source_acquire(&mut self.deferred_source_request, || {
                let ReceivedControlMessage::PortLeaseRequest(request) = self
                    .source
                    .receive_control_message_until(self.negotiation.operation_deadline())
                    .unwrap()
                else {
                    panic!("missing exact Source Acquire")
                };
                request
            });
            assert_eq!(
                request.request().session_id,
                self.negotiation.route().session_id
            );
            assert_eq!(request.request().route_id, self.route_id);
            assert_eq!(request.request().action, PortLeaseAction::Acquire);
            assert_eq!(request.request().port, self.negotiation.route().source);
            let response = self
                .source
                .prepare_port_lease_response(&mut self.source_runtime, request, wall_now())
                .unwrap();
            self.assert_unlocked();
            self.source
                .send_prepared_port_lease_response_until(
                    response,
                    self.negotiation.operation_deadline(),
                )
                .unwrap();
        }
        pub(crate) fn sink_request(&mut self, action: PortLeaseAction) -> MessageId {
            self.assert_unlocked();
            let remaining = self
                .original_end
                .saturating_duration_since(Instant::now())
                .as_millis() as u64;
            self.source
                .send_port_lease_request_until(
                    self.route_id,
                    self.negotiation.route().sink,
                    action,
                    remaining.saturating_sub(300).min(5_000),
                    self.negotiation.operation_deadline(),
                )
                .unwrap()
        }
        pub(crate) fn receive(&mut self) -> ReceivedControlMessage {
            self.assert_unlocked();
            self.sink
                .receive_control_message_until(self.negotiation.operation_deadline())
                .unwrap()
        }
        pub(crate) fn accept(
            &mut self,
            message: ReceivedControlMessage,
        ) -> Result<StereoNegotiationAction, String> {
            let mut guard = self.runtime.lock().unwrap();
            self.negotiation.accept_control(
                &mut guard,
                &mut self.sink,
                message,
                CONTROL_GENERATION,
                wall_now(),
                Instant::now(),
            )
        }
        pub(crate) fn receive_accept(&mut self) -> Result<StereoNegotiationAction, String> {
            let message = self.receive();
            self.accept(message)
        }
        pub(crate) fn commit_response(
            &mut self,
            action: StereoNegotiationAction,
            request: MessageId,
        ) {
            let StereoNegotiationAction::SendLeaseResponse(response) = action else {
                panic!("exact Sink response action absent")
            };
            assert!(!self.negotiation.ready());
            self.assert_unlocked();
            self.sink
                .send_prepared_port_lease_response_until(
                    response,
                    self.negotiation.operation_deadline(),
                )
                .unwrap();
            self.negotiation.commit_sink_response().unwrap();
            let ReceivedControlMessage::PortLeaseResponse(response) = self
                .source
                .receive_control_message_until(self.negotiation.operation_deadline())
                .unwrap()
            else {
                panic!("missing committed Sink response")
            };
            assert_eq!(response.request_message_id, request);
            assert_eq!(response.port, self.negotiation.route().sink);
            assert_eq!(response.outcome, PortLeaseOutcome::Acquired);
        }
        pub(crate) fn complete(&mut self, response_first: bool) -> StereoSinkSnapshot {
            self.send_request(true);
            if response_first {
                self.source_response();
                assert!(matches!(
                    self.receive_accept().unwrap(),
                    StereoNegotiationAction::None
                ));
                assert!(self.negotiation.source_claim.is_some());
                assert!(self.negotiation.sink_claim.is_none());
                let request = self.sink_request(PortLeaseAction::Acquire);
                let action = self.receive_accept().unwrap();
                self.commit_response(action, request);
            } else {
                let request = self.sink_request(PortLeaseAction::Acquire);
                let action = self.receive_accept().unwrap();
                self.commit_response(action, request);
                assert!(self.negotiation.sink_claim.is_some());
                assert!(self.negotiation.source_claim.is_none());
                assert!(
                    self.source_runtime
                        .route_port_lease_claim(self.route_id, self.negotiation.route().source)
                        .is_none()
                );
                self.source_response();
                assert!(matches!(
                    self.receive_accept().unwrap(),
                    StereoNegotiationAction::None
                ));
            }
            assert!(
                !self.negotiation.ready(),
                "claims alone must not create a media job"
            );
            self.source_runtime
                .begin_route_start(self.route_id, wall_now())
                .unwrap();
            self.assert_unlocked();
            self.source
                .send_route_command(self.route_id, RouteCommandAction::Start)
                .unwrap();
            assert!(matches!(
                self.receive_accept().unwrap(),
                StereoNegotiationAction::None
            ));
            assert!(self.negotiation.ready());
            let mut guard = self.runtime.lock().unwrap();
            self.negotiation
                .finish(
                    &mut guard,
                    &self.sink,
                    CONTROL_GENERATION,
                    wall_now(),
                    Instant::now(),
                )
                .unwrap()
        }
    }
}

mod pure {
    use super::*;
    use fixture::*;
    #[test]
    fn deferred_initial_source_acquire_preserves_both_claim_orders_and_exact_sink_reply() {
        use capyio_protocol::{
            decode_port_lease_request, decode_port_lease_response, encode_port_lease_request,
            encode_port_lease_response,
        };
        use std::collections::VecDeque;

        let (runtime, route_id) = pure_prepared(1_000);
        let route = runtime.route(route_id).unwrap();
        for source_response_first in [false, true] {
            let source_wire = encode_port_lease_request(
                route.session_id,
                route_id,
                route.source,
                PortLeaseAction::Acquire,
                5_000,
            )
            .unwrap();
            let expected_source = decode_port_lease_request(&source_wire).unwrap();
            let sink_request = MessageId::new();
            let sink_wire = encode_port_lease_response(
                route.session_id,
                sink_request,
                route_id,
                route.sink,
                PortLeaseOutcome::Acquired,
                Some(6_000),
            )
            .unwrap();
            // Existing protocol frames in their actual Source-side FIFO order.
            // Receiving the first request must not answer it or consume the
            // response that commit_response expects next.
            let mut inbound = VecDeque::from([source_wire, sink_wire]);
            let mut deferred =
                Some(decode_port_lease_request(&inbound.pop_front().unwrap()).unwrap());
            let mut completed = Vec::new();
            if source_response_first {
                let source =
                    take_source_acquire(&mut deferred, || panic!("deferred Acquire was lost"));
                assert_eq!(source, expected_source);
                completed.push("Source");
            }
            let sink = decode_port_lease_response(&inbound.pop_front().unwrap()).unwrap();
            assert_eq!(sink.request_message_id, sink_request);
            assert_eq!(sink.session_id, route.session_id);
            assert_eq!(sink.route_id, route_id);
            assert_eq!(sink.port, route.sink);
            assert_eq!(sink.outcome, PortLeaseOutcome::Acquired);
            assert_eq!(sink.expires_at_ms, Some(6_000));
            completed.push("Sink");
            if !source_response_first {
                let source =
                    take_source_acquire(&mut deferred, || panic!("deferred Acquire was lost"));
                assert_eq!(source, expected_source);
                completed.push("Source");
            }
            assert_eq!(
                completed,
                if source_response_first {
                    ["Source", "Sink"]
                } else {
                    ["Sink", "Source"]
                }
            );
            assert!(deferred.is_none());
            assert!(inbound.is_empty());
            // The retained request is consume-once. A later intentional foreign
            // request must perform a fresh read, not replay the retained object.
            let next_wire = encode_port_lease_request(
                route.session_id,
                route_id,
                route.source,
                PortLeaseAction::Acquire,
                1_000,
            )
            .unwrap();
            let next = decode_port_lease_request(&next_wire).unwrap();
            let mut reads = 0;
            let received = take_source_acquire(&mut deferred, || {
                reads += 1;
                next
            });
            assert_eq!(reads, 1);
            assert_eq!(received, next);
            assert_ne!(received.message_id, expected_source.message_id);
        }
    }
    #[test]
    fn consent_stamp_captures_original_deadline_and_rejects_changed_or_broad_grants() {
        let (runtime, route) = pure_prepared(1_000);
        let instant = Instant::now();
        assert!(StereoConsentStamp::is_exact_stereo_sink(&runtime, route));
        let stamp = StereoConsentStamp::capture(&runtime, route, 1_000, instant).unwrap();
        assert_eq!(stamp.route_id(), route);
        assert_eq!(
            stamp.monotonic_deadline(),
            instant + Duration::from_secs(10)
        );
        assert!(StereoConsentStamp::capture(&runtime, route, 11_000, instant).is_err());
        let (broad, broad_route) = pure_prepared(100_000);
        assert!(StereoConsentStamp::capture(&broad, broad_route, 1_000, instant).is_err());
    }
    #[test]
    fn snapshot_freezes_exact_generation_route_and_original_deadlines() {
        let (runtime, snapshot) = pure_snapshot(1_000);
        snapshot
            .validate(&runtime, CONTROL_GENERATION, 1_001, snapshot.started)
            .unwrap();
        assert_eq!(snapshot.wall_deadline_ms(), 5_000);
        assert_eq!(snapshot.monotonic_deadline(), snapshot.end);
        assert!(
            snapshot
                .validate(&runtime, CONTROL_GENERATION + 1, 1_001, snapshot.started)
                .is_err()
        );
        assert!(
            snapshot
                .validate(&runtime, CONTROL_GENERATION, 999, snapshot.started)
                .is_err()
        );
        assert!(
            snapshot
                .validate(&runtime, CONTROL_GENERATION, 5_000, snapshot.started)
                .is_err()
        );
        assert!(
            snapshot
                .validate(&runtime, CONTROL_GENERATION, 1_001, snapshot.end)
                .is_err()
        );
        assert_eq!(snapshot.route().state, RouteState::Starting);
    }
    #[test]
    fn snapshot_rejects_revoked_replaced_and_expired_original_sink_lease() {
        for mutation in 0..4 {
            let (mut runtime, snapshot) = pure_snapshot(1_000);
            let route = snapshot.route();
            match mutation {
                0 => {
                    runtime
                        .release_route_port_lease(route.id, route.sink, route.session_id)
                        .unwrap();
                }
                1 => {
                    runtime
                        .acquire_route_port_lease(route.id, route.sink, 7_000, 1_001)
                        .unwrap();
                }
                2 => {
                    runtime.close_session(route.session_id).unwrap();
                }
                _ => {
                    runtime.begin_route_stop(route.id).unwrap();
                }
            }
            assert!(
                snapshot
                    .validate(&runtime, CONTROL_GENERATION, 1_001, snapshot.started)
                    .is_err()
            );
        }
    }
    #[test]
    fn snapshot_rejects_profile_catalog_and_correlated_source_claim_mutation() {
        for mutation in 0..7 {
            let (mut runtime, mut snapshot) = pure_snapshot(1_000);
            match mutation {
                0 => {
                    snapshot.original.selected_format = Some(FormatDescriptor::new(
                        capyio_protocol::SECURE_AUDIO_MONO_FORMAT,
                    ))
                }
                1 => snapshot.source_claim.request_message_id = MessageId::new(),
                2 => snapshot.source_claim.outcome = PortLeaseOutcome::Renewed,
                3 => snapshot.source_claim.expires_at_ms = Some(20_000),
                4 => snapshot.source_claim.port = snapshot.original.sink,
                5 => snapshot.original.epoch += 1,
                _ => {
                    let mut peer = runtime.snapshot().peers[0].clone();
                    peer.capabilities
                        .get_mut(&snapshot.original.source.capability_id)
                        .unwrap()
                        .metadata
                        .insert("settingsRevision".into(), "2".into());
                    runtime
                        .accept_peer_catalog(snapshot.original.session_id, peer)
                        .unwrap();
                }
            }
            assert!(
                snapshot
                    .validate(&runtime, CONTROL_GENERATION, 1_001, snapshot.started)
                    .is_err()
            );
        }
    }
    #[test]
    fn original_consent_intersection_and_request_ttl_cannot_renew_authority() {
        let start = Instant::now();
        let original = start + Duration::from_secs(4);
        let delayed = start + Duration::from_secs(2);
        assert_eq!(
            consent_end(delayed, Duration::from_secs(60), original, 60_000).unwrap(),
            original
        );
        assert_eq!(source_lease_ttl(original - delayed).unwrap(), 1_750);
        assert!(source_lease_ttl(MAX_STEREO_OPERATION).is_err());
        assert!(consent_end(original, Duration::from_secs(60), original, 60_000).is_err());
    }
}

/// Mandatory disposable hosted execution. Socket restrictions must fail these
/// tests, never turn them into a skip or substitute a mocked control protocol.
mod hosted {
    use super::*;
    use fixture::*;
    #[test]
    fn ipc_stereo_supervisor_crossed_claims_both_control_roles_use_current_runtime() {
        for accepts in [false, true] {
            for response_first in [false, true] {
                let mut f = Fixture::new(accepts);
                let original_end = f.original_end;
                let snapshot = f.complete(response_first);
                assert_eq!(
                    snapshot.peer_certificate,
                    f.identity.expected_peer_certificate
                );
                assert_eq!(snapshot.control_generation(), CONTROL_GENERATION);
                assert!(snapshot.monotonic_deadline() <= original_end);
                let mut runtime = f.runtime.lock().unwrap();
                snapshot
                    .validate(&runtime, CONTROL_GENERATION, wall_now(), Instant::now())
                    .unwrap();
                assert_eq!(runtime.route(f.route_id).unwrap(), snapshot.route());
                assert!(
                    f.negotiation
                        .finish(
                            &mut runtime,
                            &f.sink,
                            CONTROL_GENERATION,
                            wall_now(),
                            Instant::now()
                        )
                        .is_err(),
                    "snapshot handoff must be consume-once"
                );
            }
        }
    }
    #[test]
    fn ipc_stereo_supervisor_early_or_replayed_source_response_is_terminal() {
        for before_commit in [false, true] {
            let mut f = Fixture::new(true);
            f.send_request(!before_commit);
            f.source_response();
            let ReceivedControlMessage::PortLeaseResponse(response) = f.receive() else {
                panic!("Source response absent")
            };
            if !before_commit {
                assert!(matches!(
                    f.accept(ReceivedControlMessage::PortLeaseResponse(response))
                        .unwrap(),
                    StereoNegotiationAction::None
                ));
            }
            assert!(
                f.accept(ReceivedControlMessage::PortLeaseResponse(response))
                    .is_err()
            );
            assert!(f.negotiation.is_terminal());
            assert!(!f.negotiation.ready());
            assert!(
                f.runtime
                    .lock()
                    .unwrap()
                    .route_port_lease_claim(f.route_id, f.negotiation.route().sink)
                    .is_none()
            );
        }
    }
    #[test]
    fn ipc_stereo_supervisor_foreign_correlated_response_never_completes_claim() {
        let mut f = Fixture::new(false);
        f.send_request(true);
        // The first real response is deliberately left queued. A second real
        // request gets a different ID, then its response is dispatched first.
        f.source_response();
        let original = f.receive();
        let other = f
            .sink
            .prepare_port_lease_request(
                f.route_id,
                f.negotiation.route().source,
                PortLeaseAction::Acquire,
                1_000,
            )
            .unwrap();
        f.sink
            .send_prepared_port_lease_request_until(other, f.negotiation.operation_deadline())
            .unwrap();
        f.source_response();
        let foreign = f.receive();
        assert!(f.accept(foreign).is_err());
        assert!(f.accept(original).is_err());
        assert!(f.negotiation.is_terminal());
    }
    #[test]
    fn ipc_stereo_supervisor_queued_response_loses_to_authoritative_stop() {
        let mut f = Fixture::new(true);
        f.send_request(true);
        f.source_response();
        let queued = f.receive();
        {
            let mut guard = f.runtime.lock().unwrap();
            guard.begin_route_stop(f.route_id).unwrap();
            guard.stop_route(f.route_id).unwrap();
        }
        assert!(f.accept(queued).is_err());
        assert!(!f.negotiation.ready());
        assert_eq!(
            f.runtime.lock().unwrap().route(f.route_id).unwrap().state,
            RouteState::Stopped
        );
    }
    #[test]
    fn ipc_stereo_supervisor_pending_sink_action_cannot_commit_after_stop() {
        let mut f = Fixture::new(false);
        f.send_request(true);
        f.sink_request(PortLeaseAction::Acquire);
        let action = f.receive_accept().unwrap();
        assert!(matches!(
            action,
            StereoNegotiationAction::SendLeaseResponse(_)
        ));
        f.source
            .send_route_command(f.route_id, RouteCommandAction::Stop)
            .unwrap();
        let stop = f.receive();
        // A pending response is not authority to ignore Stop. Whether rejected
        // as uncommitted action or recognized as PeerStop, it is terminal.
        let result = f.accept(stop);
        assert!(result.is_err() || matches!(result, Ok(StereoNegotiationAction::PeerStop)));
        assert!(f.negotiation.is_terminal());
        assert!(f.negotiation.commit_sink_response().is_err());
        drop(action);
    }
    #[test]
    fn ipc_stereo_supervisor_duplicate_sink_acquire_and_renew_are_terminal() {
        for action in [PortLeaseAction::Acquire, PortLeaseAction::Renew] {
            let mut f = Fixture::new(true);
            f.send_request(true);
            let id = f.sink_request(PortLeaseAction::Acquire);
            let response = f.receive_accept().unwrap();
            f.commit_response(response, id);
            let original = *f
                .runtime
                .lock()
                .unwrap()
                .route_port_lease_claim(f.route_id, f.negotiation.route().sink)
                .unwrap();
            f.sink_request(action);
            assert!(f.receive_accept().is_err());
            assert_eq!(
                f.runtime
                    .lock()
                    .unwrap()
                    .route_port_lease_claim(f.route_id, f.negotiation.route().sink),
                Some(&original)
            );
            assert!(f.negotiation.is_terminal());
        }
    }
    #[test]
    fn ipc_stereo_supervisor_current_generation_and_original_budget_are_not_refreshed() {
        for stale_generation in [false, true] {
            let mut f = Fixture::new(false);
            f.send_request(true);
            let end = f.negotiation.operation_deadline();
            let runtime = f.runtime.lock().unwrap();
            let generation = CONTROL_GENERATION + u64::from(stale_generation);
            let instant = if stale_generation {
                Instant::now()
            } else {
                end
            };
            assert!(
                f.negotiation
                    .validate(&runtime, &f.sink, generation, wall_now(), instant)
                    .is_err()
            );
            assert_eq!(f.negotiation.operation_deadline(), end);
            assert!(f.negotiation.is_terminal());
        }
    }
    #[test]
    fn ipc_stereo_supervisor_clock_rollback_after_successful_validation_is_terminal() {
        let mut f = Fixture::new(true);
        let forward = f.negotiation.last_wall + 10;
        let runtime = f.runtime.lock().unwrap();
        f.negotiation
            .validate(
                &runtime,
                &f.sink,
                CONTROL_GENERATION,
                forward,
                Instant::now(),
            )
            .unwrap();
        assert!(
            f.negotiation
                .validate(
                    &runtime,
                    &f.sink,
                    CONTROL_GENERATION,
                    forward - 1,
                    Instant::now()
                )
                .is_err()
        );
        assert!(f.negotiation.is_terminal());
    }
    #[test]
    fn ipc_stereo_consent_stamp_clones_share_consumption_even_after_failed_prepare() {
        for first_valid in [false, true] {
            let f = Fixture::new(false);
            let runtime = f.runtime.lock().unwrap();
            let now = wall_now();
            let instant = Instant::now();
            let stamp = StereoConsentStamp::capture(&runtime, f.route_id, now, instant).unwrap();
            let clone = stamp.clone();
            let original_end = stamp.monotonic_deadline();
            let prepared = stamp.prepare(
                &runtime,
                &f.sink,
                f.negotiation.admission.clone(),
                if first_valid { CONTROL_GENERATION } else { 0 },
                now + 5,
                instant + Duration::from_millis(5),
            );
            if first_valid {
                let (negotiation, _) = prepared.unwrap();
                assert!(negotiation.end <= original_end);
                assert_eq!(negotiation.local_consent_end, original_end);
            } else {
                assert!(prepared.is_err());
            }
            assert!(
                clone
                    .prepare(
                        &runtime,
                        &f.sink,
                        f.negotiation.admission.clone(),
                        CONTROL_GENERATION,
                        now + 5,
                        instant + Duration::from_millis(5)
                    )
                    .is_err()
            );
        }
    }

    #[test]
    fn ipc_stereo_supervisor_commits_are_exact_one_shot_actions() {
        for wrong_request in [false, true] {
            let mut f = Fixture::new(true);
            let sent = f.send_request(!wrong_request);
            assert!(
                f.negotiation
                    .commit_source_request(if wrong_request {
                        MessageId::new()
                    } else {
                        sent
                    })
                    .is_err()
            );
            assert!(f.negotiation.is_terminal());
        }
        let mut f = Fixture::new(false);
        assert!(f.negotiation.commit_sink_response().is_err());
        assert!(f.negotiation.is_terminal());
    }
}
