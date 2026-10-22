//! Pure admission tests plus hosted-only real TLS lifecycle tests. The latter
//! use synthetic PCM and fresh fixture credentials, never an audio device.
use super::*;
use capyio_control_tls::ControlTlsBinding;
use capyio_core::*;

fn bare_job() -> (Arc<AudioJob>, Receiver<Record>) {
    bare_profile_job(SecureAudioSessionProfile::Finite60sV1, 10_000)
}
fn bare_profile_job(
    profile: SecureAudioSessionProfile,
    remaining_ms: u64,
) -> (Arc<AudioJob>, Receiver<Record>) {
    let source = PortRef {
        node_id: NodeId::new(),
        capability_id: CapabilityId::new(),
        port_id: PortId::new(),
    };
    let sink = PortRef {
        node_id: NodeId::new(),
        capability_id: CapabilityId::new(),
        port_id: PortId::new(),
    };
    let deadline = now().unwrap() + remaining_ms;
    AudioJob::pending(
        7,
        11,
        AudioTlsBinding {
            session: ControlTlsBinding::new(SessionId::new(), sink.node_id, source.node_id)
                .unwrap(),
            route_id: RouteId::new(),
            source,
            sink,
            stream_id: StreamId::new(),
            epoch: 1,
            expires_at_ms: deadline,
        },
        profile,
        deadline,
        Instant::now() + profile.max_duration(),
    )
}
fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "bounded audio lifecycle exceeded two seconds"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

mod pure {
    use super::*;
    #[test]
    fn exact_profile_caps_pending_deadline_without_inference_or_queue_growth() {
        for (profile, cap) in [
            (SecureAudioSessionProfile::Finite60sV1, 60),
            (SecureAudioSessionProfile::Finite300sV1, 300),
        ] {
            let before = Instant::now();
            let (job, _) = bare_profile_job(profile, 600_000);
            let after = Instant::now();
            assert_eq!(job.profile, profile);
            assert!(job.lease_deadline >= before + Duration::from_secs(cap));
            assert!(job.lease_deadline <= after + Duration::from_secs(cap));
            assert_eq!(RECORD_SLOTS, 2);
            job.state.store(READY, Ordering::Release);
            let a = job.reserve(960).unwrap();
            let b = job.reserve(960).unwrap();
            assert!(job.reserve(960).is_err());
            drop((a, b));
            assert_eq!(job.reserved.load(Ordering::Acquire), 0);
            let (short, _) = bare_profile_job(profile, 1000);
            assert!(short.lease_deadline <= Instant::now() + Duration::from_secs(1));
            assert_eq!(short.profile, profile);
        }
    }

    #[test]
    fn immutable_preparation_deadline_survives_delayed_start_for_both_profiles() {
        for profile in [
            SecureAudioSessionProfile::Finite60sV1,
            SecureAudioSessionProfile::Finite300sV1,
        ] {
            let (template, _) = bare_profile_job(profile, 299_000);
            let original_end = Instant::now();
            let (job, receiver) = AudioJob::pending(
                7,
                12,
                template.binding,
                profile,
                template.lease_deadline_ms,
                original_end,
            );
            assert_eq!(job.preparation_deadline, original_end);
            assert_eq!(job.lease_deadline, original_end);
            job.spawn(
                receiver,
                Box::new(|| panic!("expired preparation executed")),
                Arc::new(|| panic!("expired snapshot executed")),
                Arc::new(|| {}),
            )
            .unwrap();
            wait_until(|| job.worker_exited());
            job.reap();
            assert_eq!(job.state(), FAILED);
            assert!(job.reserve(960).is_err());
        }
    }
    #[test]
    fn stop_before_preparation_is_one_shot_for_both_explicit_profiles() {
        for profile in [
            SecureAudioSessionProfile::Finite60sV1,
            SecureAudioSessionProfile::Finite300sV1,
        ] {
            let (job, receiver) = bare_profile_job(profile, 299_000);
            job.stop();
            job.spawn(
                receiver,
                Box::new(|| panic!("cancelled preparation")),
                Arc::new(|| panic!("cancelled snapshot")),
                Arc::new(|| panic!("Stop is not failure")),
            )
            .unwrap();
            wait_until(|| job.worker_exited());
            job.reap();
            assert_eq!(job.state(), STOPPED);
            assert!(job.reserve(960).is_err());
            assert!(job.worker.lock().unwrap().is_none());
        }
    }
    #[test]
    fn pending_and_terminal_generations_never_reserve_pcm() {
        let (job, _) = bare_job();
        assert_eq!((job.runtime, job.generation), (7, 11));
        assert_eq!(job.state(), PENDING);
        assert!(job.reserve(960).is_err());
        job.stop();
        job.fail();
        assert_eq!(job.state(), STOPPED);
        assert!(job.reserve(960).is_err());
        assert_eq!(job.reserved.load(Ordering::Acquire), 0);
    }
    #[test]
    fn capture_admission_does_not_acquire_worker_registry_lock() {
        let (job, receiver) = bare_job();
        job.state.store(READY, Ordering::Release);
        let _held = job.worker.lock().unwrap();
        job.submit(job.reserve(960).unwrap(), [0; 960]).unwrap();
        drop(receiver.try_recv().unwrap());
        assert_eq!(job.reserved.load(Ordering::Acquire), 0);
    }
    #[test]
    fn slots_include_copy_reservation_queued_pcm_and_worker_current_pcm() {
        let (job, receiver) = bare_job();
        job.state.store(READY, Ordering::Release);
        let first = job.reserve(960).unwrap();
        let pending_copy = job.reserve(960).unwrap();
        job.submit(first, [0; 960]).unwrap();
        let worker_current = receiver.try_recv().unwrap();
        assert_eq!(job.reserved.load(Ordering::Acquire), 2);
        assert!(job.reserve(960).is_err());
        assert_eq!(job.state(), FAILED);
        job.stop();
        assert_eq!(job.state(), FAILED);
        drop(pending_copy);
        drop(worker_current);
        assert_eq!(job.reserved.load(Ordering::Acquire), 0);
    }
    #[test]
    fn copy_failure_and_late_submission_release_capacity() {
        let (job, _) = bare_job();
        job.state.store(READY, Ordering::Release);
        drop(job.reserve(960).unwrap());
        assert_eq!(job.reserved.load(Ordering::Acquire), 0);
        let permit = job.reserve(960).unwrap();
        job.stop();
        assert!(job.submit(permit, [0; 960]).is_err());
        assert_eq!(job.reserved.load(Ordering::Acquire), 0);
    }
    #[test]
    fn permit_cannot_cross_owner_even_when_generation_numbers_match() {
        let (a, _) = bare_job();
        let (b, _) = bare_job();
        a.state.store(READY, Ordering::Release);
        b.state.store(READY, Ordering::Release);
        assert_eq!(a.generation, b.generation);
        assert!(b.submit(a.reserve(960).unwrap(), [0; 960]).is_err());
        assert_eq!(a.reserved.load(Ordering::Acquire), 0);
        assert_eq!(b.reserved.load(Ordering::Acquire), 0);
    }
    #[test]
    fn non_pcm_lengths_fail_terminally_before_copy_including_cpya_header() {
        for length in [0, 1, 959, 961, 1064, usize::MAX] {
            let (job, _) = bare_job();
            job.state.store(READY, Ordering::Release);
            assert!(job.reserve(length).is_err());
            assert_eq!(job.state(), FAILED);
            assert_eq!(job.reserved.load(Ordering::Acquire), 0);
        }
    }
    #[test]
    fn disconnected_queue_is_terminal_and_releases_its_permit() {
        let (job, receiver) = bare_job();
        job.state.store(READY, Ordering::Release);
        let permit = job.reserve(960).unwrap();
        drop(receiver);
        assert!(job.submit(permit, [0; 960]).is_err());
        assert_eq!(job.state(), FAILED);
        assert_eq!(job.reserved.load(Ordering::Acquire), 0);
    }
    #[test]
    fn streaming_state_continues_to_accept_exact_pcm() {
        let (job, receiver) = bare_job();
        job.state.store(STREAMING, Ordering::Release);
        job.submit(job.reserve(960).unwrap(), [0x5a; 960]).unwrap();
        assert_eq!(receiver.try_recv().unwrap().pcm, [0x5a; 960]);
        job.stop();
        assert_eq!(job.state(), STOPPED);
    }
    #[test]
    fn worker_owns_canonical_timeline_and_preserves_pcm_verbatim() {
        let (job, _) = bare_job();
        let mut timeline = PcmTimeline::new(job.binding).unwrap();
        let mut admission =
            SecureMonoPcmAdmission::new(timeline.admission.binding().clone()).unwrap();
        for sequence in 0..4 {
            let pcm = [sequence as u8; 960];
            let wire = timeline.encode(pcm).unwrap();
            assert_eq!(wire.len(), 1064);
            let packet = admission.admit_record(&wire).unwrap();
            assert_eq!(packet.sequence, sequence);
            assert_eq!(packet.first_sample_index, sequence * 480);
            assert_eq!(packet.source_timestamp_micros, sequence * 10_000);
            assert_eq!(packet.sample_count, 480);
            assert!(!packet.discontinuity);
            assert_eq!(packet.payload, pcm);
            assert_eq!(packet.stream_id, job.binding.stream_id);
            assert_eq!(packet.stream_epoch, 1);
        }
    }
    #[test]
    fn worker_timeline_overflow_rejects_without_advancing() {
        let (job, _) = bare_job();
        for sequence in [u64::MAX, u64::MAX / 10_000] {
            let mut timeline = PcmTimeline::new(job.binding).unwrap();
            timeline.sequence = sequence;
            assert!(timeline.encode([0; 960]).is_err());
            assert_eq!(timeline.sequence, sequence);
        }
    }
    #[test]
    fn stop_before_preparation_never_connects_and_reaps_without_failure() {
        let (job, receiver) = bare_job();
        job.stop();
        job.spawn(
            receiver,
            Box::new(|| panic!("cancelled preparation executed")),
            Arc::new(|| panic!("cancelled snapshot executed")),
            Arc::new(|| panic!("Stop reported as failure")),
        )
        .unwrap();
        wait_until(|| job.worker_exited());
        assert_eq!(job.state(), STOPPED);
        job.reap();
        assert!(job.worker.lock().unwrap().is_none());
    }
    #[test]
    fn completion_marker_cannot_retire_or_join_a_still_live_thread() {
        let (job, _) = bare_job();
        let (release_tx, release_rx) = mpsc::channel();
        *job.worker.lock().unwrap() = Some(thread::spawn(move || {
            let _ = release_rx.recv_timeout(Duration::from_secs(2));
        }));
        // Model the interval between the last application instruction and the
        // native thread actually finishing. The marker alone is insufficient.
        job.finished.store(true, Ordering::Release);
        assert!(job.finished());
        assert!(!job.worker_exited());
        job.reap();
        assert!(
            job.worker.lock().unwrap().is_some(),
            "live worker was retired"
        );
        assert!(!job.worker_exited());
        release_tx.send(()).unwrap();
        wait_until(|| job.worker_exited());
        job.reap();
        assert!(job.worker.lock().unwrap().is_none());
        assert!(job.worker_exited());
        job.reap();
    }
    #[test]
    fn preparation_failure_reports_once_and_claim_outlives_worker_reaping() {
        let mut claim = GenerationClaim::default();
        claim.reserve().unwrap();
        let (job, receiver) = bare_job();
        let count = Arc::new(AtomicUsize::new(0));
        let failed = count.clone();
        job.spawn(
            receiver,
            Box::new(|| Err("no authenticated receipt".into())),
            Arc::new(|| panic!("failed preparation executed snapshot")),
            Arc::new(move || {
                failed.fetch_add(1, Ordering::AcqRel);
            }),
        )
        .unwrap();
        wait_until(|| job.worker_exited());
        assert_eq!(job.state(), FAILED);
        job.reap();
        job.reap();
        assert!(job.worker.lock().unwrap().is_none());
        assert_eq!(count.load(Ordering::Acquire), 1);
        drop(job);
        assert!(claim.reserve().is_err());
        let invalid_other_route: Option<&mut GenerationClaim> = None;
        assert!(invalid_other_route.ok_or("unknown Route").is_err());
        assert!(claim.reserve().is_err());
        assert!(GenerationClaim::default().reserve().is_ok());
    }
    #[test]
    fn original_deadline_fails_without_runtime_snapshot_and_cannot_revive() {
        let (template, _) = bare_job();
        let (job, _) = AudioJob::pending(
            7,
            12,
            template.binding,
            template.profile,
            now().unwrap(),
            template.preparation_deadline,
        );
        job.state.store(READY, Ordering::Release);
        assert_eq!(job.state(), FAILED);
        assert!(job.reserve(960).is_err());
        job.stop();
        assert_eq!(job.state(), FAILED);
    }
}

mod hosted {
    use super::*;
    use crate::AndroidCatalogRuntimeV1;
    use capyio_control_credentials::ControlCredentialBundle;
    use capyio_control_link::{CatalogControlLink, ReceivedControlMessage};
    use capyio_control_tls::{
        AuthenticatedControlClientChannel, AuthenticatedControlServer, ControlTlsClientIdentity,
        ControlTlsEndpoint, ControlTlsServerIdentity,
        audio::{AudioTlsChannel, AudioTlsListener},
    };
    use capyio_pairing::CertificateFingerprint;
    use capyio_protocol::{SecureAudioDataOffer, SecureAudioTransport};
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
        KeyUsagePurpose,
    };
    use std::net::TcpListener;

    struct Fixture {
        runtime: Arc<Mutex<NodeRuntime>>,
        prepared: PreparedAudioSender,
        identity: ControlTlsServerIdentity,
        source_control: CatalogControlLink,
        _sink_control: CatalogControlLink,
    }
    impl Fixture {
        fn new(port: u16) -> Self {
            Self::with_lease(port, 10_000)
        }
        fn with_lease(port: u16, lease_ms: u64) -> Self {
            Self::with_profile(port, lease_ms, SecureAudioSessionProfile::Finite60sV1)
        }
        fn with_profile(port: u16, lease_ms: u64, profile: SecureAudioSessionProfile) -> Self {
            let (android, sink_node, source, sink) = nodes(profile);
            let (identity, bundle, pin) = dual_identities();
            let binding =
                ControlTlsBinding::new(SessionId::new(), sink_node.id, android.id).unwrap();
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
            let accepted = thread::spawn(move || server.accept().unwrap());
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
            let mut source_control =
                CatalogControlLink::admit_dialed_peer(&mut runtime, client, "Sink fixture".into())
                    .unwrap();
            let mut sink_control = CatalogControlLink::admit_accepted_peer(
                &mut sink_runtime,
                accepted.join().unwrap(),
                "Android fixture".into(),
            )
            .unwrap();
            // Drive the same production policy used by authenticated JNI
            // control, rather than manufacturing an already-Starting Source.
            let mut host = AndroidCatalogRuntimeV1::new(include_str!(
                "../../fixtures/android-core-catalog-v1.json"
            ))
            .unwrap();
            host.runtime = runtime;
            let id = sink_runtime
                .create_route(
                    binding.session_id,
                    source,
                    sink,
                    RouteBackend::AdapterManaged,
                )
                .unwrap();
            let timestamp = now().unwrap();
            sink_control
                .send_route_status(sink_runtime.route(id).unwrap())
                .unwrap();
            let draft = receive_status(&mut host, &mut source_control, timestamp);
            assert_eq!(draft.state, RouteState::Draft);
            assert_eq!(draft.authorization, AuthorizationState::Pending);
            assert!(
                host.runtime
                    .acquire_route_port_lease(id, source, timestamp + lease_ms, timestamp)
                    .is_err()
            );
            assert!(
                host.audio_uses.is_empty(),
                "secure route created a UDP endpoint"
            );
            // A genuine received Start still cannot turn peer intent into
            // microphone consent or a Source lease.
            sink_control
                .send_route_command(id, capyio_protocol::RouteCommandAction::Start)
                .unwrap();
            let ReceivedControlMessage::RouteCommand(premature) =
                source_control.receive_control_message().unwrap()
            else {
                panic!("Start command missing")
            };
            assert!(
                host.stage_audio_route_command(&source_control, &premature, timestamp)
                    .is_err()
            );
            host.resolve_touchpad_request(id, true, timestamp).unwrap();
            sink_runtime
                .authorize_route(
                    id,
                    Some(timestamp + lease_ms.saturating_add(10_000).clamp(20_000, 300_000)),
                )
                .unwrap();
            sink_control
                .send_route_status(sink_runtime.route(id).unwrap())
                .unwrap();
            receive_status(&mut host, &mut source_control, timestamp);
            let request_id = sink_control
                .send_port_lease_request(
                    id,
                    source,
                    capyio_protocol::PortLeaseAction::Acquire,
                    lease_ms,
                )
                .unwrap();
            let ReceivedControlMessage::PortLeaseRequest(request) =
                source_control.receive_control_message().unwrap()
            else {
                panic!("Source lease request missing")
            };
            host.answer_audio_port_lease(&mut source_control, request, timestamp)
                .unwrap();
            let ReceivedControlMessage::PortLeaseResponse(response) =
                sink_control.receive_control_message().unwrap()
            else {
                panic!("Source lease response missing")
            };
            assert_eq!(response.request_message_id, request_id);
            assert_eq!(
                response.outcome,
                capyio_protocol::PortLeaseOutcome::Acquired
            );
            sink_runtime
                .set_local_port_lease_policy(sink, capyio_runtime::PortLeasePolicy::Exclusive)
                .unwrap();
            sink_runtime
                .acquire_route_port_lease(id, sink, timestamp + lease_ms, timestamp)
                .unwrap();
            sink_runtime
                .prepare_route(
                    id,
                    Some(profile.descriptor()),
                    QosMode::Interactive,
                    timestamp,
                )
                .unwrap();
            sink_control
                .send_route_status(sink_runtime.route(id).unwrap())
                .unwrap();
            let prepared_route = receive_status(&mut host, &mut source_control, timestamp);
            assert_eq!(prepared_route.state, RouteState::Prepared);
            assert_eq!(prepared_route.selected_format, Some(profile.descriptor()));
            assert!(host.audio_uses.is_empty());
            sink_runtime.begin_route_start(id, timestamp).unwrap();
            sink_control
                .send_route_command(id, capyio_protocol::RouteCommandAction::Start)
                .unwrap();
            let ReceivedControlMessage::RouteCommand(command) =
                source_control.receive_control_message().unwrap()
            else {
                panic!("authenticated Start missing")
            };
            let starting = host
                .stage_audio_route_command(&source_control, &command, timestamp)
                .unwrap();
            assert_eq!(starting.state, RouteState::Starting);
            assert_eq!(starting.epoch, sink_runtime.route(id).unwrap().epoch);
            let offer = SecureAudioDataOffer {
                port,
                stream_id: StreamId::new(),
                transport: SecureAudioTransport::Tls13MtlsV1,
            };
            sink_control
                .send_secure_audio_data_offer_until(
                    sink_runtime.route(id).unwrap(),
                    offer,
                    timestamp,
                    Instant::now() + Duration::from_secs(2),
                )
                .unwrap();
            let offered = receive_status(&mut host, &mut source_control, timestamp);
            assert_eq!(offered.state, RouteState::Starting);
            assert!(host.audio_uses.is_empty());
            let runtime = host.runtime;
            let prepared = PreparedAudioSender::new(
                &runtime,
                &mut source_control,
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
                source_control,
                _sink_control: sink_control,
            }
        }
        fn snapshot(&self) -> Snapshot {
            let runtime = self.runtime.clone();
            Arc::new(move || Ok(runtime.lock().unwrap().clone()))
        }
        fn start(&self, snapshot: Snapshot) -> Arc<AudioJob> {
            let (job, receiver) = AudioJob::pending(
                7,
                11,
                self.prepared.binding(),
                self.prepared.profile(),
                self.prepared.lease_deadline(),
                self.prepared.monotonic_deadline(),
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
        fn listener(&self, socket: TcpListener) -> AudioTlsListener {
            AudioTlsListener::from_listener(
                socket,
                self.identity.clone(),
                self.prepared.binding(),
                Duration::from_millis(500),
            )
            .unwrap()
        }
        fn revoke(&self) {
            let mut runtime = self.runtime.lock().unwrap();
            let route = runtime
                .route(self.prepared.binding().route_id)
                .unwrap()
                .clone();
            assert!(
                runtime
                    .release_route_port_lease(route.id, route.source, route.session_id)
                    .unwrap()
            );
        }
    }
    fn receive_status(
        host: &mut AndroidCatalogRuntimeV1,
        source: &mut CatalogControlLink,
        timestamp: u64,
    ) -> Route {
        let ReceivedControlMessage::RouteStatus(status) = source.receive_control_message().unwrap()
        else {
            panic!("authenticated Route status missing")
        };
        host.receive_audio_route_status(source, &status, "127.0.0.1", timestamp)
            .unwrap()
    }
    #[test]
    fn production_control_negotiation_requires_consent_lease_selection_start_and_offer() {
        let (fixture, job, mut peer) = running();
        // Fixture traversed actual control TLS Draft, local consent, finite
        // grant restriction, correlated Source lease, Prepared, Start and offer.
        assert_eq!(
            fixture
                .runtime
                .lock()
                .unwrap()
                .route(job.binding.route_id)
                .unwrap()
                .state,
            RouteState::Starting
        );
        job.submit(job.reserve(960).unwrap(), [0x4c; 960]).unwrap();
        assert_eq!(peer.receive_record().unwrap().len(), 1064);
        wait_until(|| job.state() == STREAMING);
        // Native I/O receipt is not an audibility claim or Runtime activation.
        assert_eq!(
            fixture
                .runtime
                .lock()
                .unwrap()
                .route(job.binding.route_id)
                .unwrap()
                .state,
            RouteState::Starting
        );
        job.stop();
        assert_reaped(&job);
    }
    fn accept_media(listener: AudioTlsListener) -> JoinHandle<AudioTlsChannel> {
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
    fn running() -> (Fixture, Arc<AudioJob>, AudioTlsChannel) {
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let fixture = Fixture::new(media.local_addr().unwrap().port());
        let accepted = accept_media(fixture.listener(media));
        let job = fixture.start(fixture.snapshot());
        let peer = accepted.join().unwrap();
        wait_until(|| job.state() == READY);
        (fixture, job, peer)
    }
    fn assert_reaped(job: &AudioJob) {
        wait_until(|| job.worker_exited());
        job.reap();
        job.reap();
        assert!(job.worker.lock().unwrap().is_none());
        assert_eq!(job.reserved.load(Ordering::Acquire), 0);
    }

    #[test]
    fn stale_preparation_after_lease_loss_never_attempts_media_connect() {
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let fixture = Fixture::new(media.local_addr().unwrap().port());
        fixture.revoke();
        let job = fixture.start(fixture.snapshot());
        assert_reaped(&job);
        assert_eq!(job.state(), FAILED);
        media.set_nonblocking(true).unwrap();
        assert!(
            matches!(media.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
    #[test]
    fn stop_interrupts_pending_real_tls_handshake_without_ready_resurrection() {
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let fixture = Fixture::new(media.local_addr().unwrap().port());
        let job = fixture.start(fixture.snapshot());
        media.set_nonblocking(true).unwrap();
        let mut held = None;
        wait_until(|| match media.accept() {
            Ok((socket, _)) => {
                held = Some(socket);
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => false,
            Err(error) => panic!("{error}"),
        });
        wait_until(|| job.stop_handle.get().is_some());
        assert_eq!(job.state(), PENDING);
        assert!(
            fixture.runtime.try_lock().is_ok(),
            "TLS handshake held Runtime"
        );
        job.stop();
        wait_until(|| {
            assert_eq!(job.state(), STOPPED);
            job.worker_exited()
        });
        assert_reaped(&job);
        assert!(job.reserve(960).is_err());
        drop(held);
    }
    #[test]
    fn stop_after_tls_connect_before_ready_cannot_publish_stale_success() {
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let fixture = Fixture::new(media.local_addr().unwrap().port());
        let accepted = accept_media(fixture.listener(media));
        let (snapshot, reached, release) = gated_snapshot(fixture.runtime.clone(), 1);
        let job = fixture.start(snapshot);
        reached.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut peer = accepted.join().unwrap();
        assert_eq!(job.state(), PENDING);
        assert!(fixture.runtime.try_lock().is_ok());
        job.stop();
        release.send(()).unwrap();
        assert_reaped(&job);
        assert_eq!(job.state(), STOPPED);
        assert!(peer.receive_record().is_err());
    }
    #[test]
    fn fake_capture_delivers_canonical_pcm_then_stop_releases_transport() {
        let (fixture, job, mut peer) = running();
        let mut admission = SecureMonoPcmAdmission::new(
            PcmTimeline::new(job.binding)
                .unwrap()
                .admission
                .binding()
                .clone(),
        )
        .unwrap();
        for sequence in 0..3 {
            let pcm = [sequence as u8 + 1; 960];
            job.submit(job.reserve(960).unwrap(), pcm).unwrap();
            let wire = peer.receive_record().unwrap();
            let packet = admission.admit_record(&wire).unwrap();
            assert_eq!(packet.sequence, sequence);
            assert_eq!(packet.first_sample_index, sequence * 480);
            assert_eq!(packet.source_timestamp_micros, sequence * 10_000);
            assert_eq!(packet.payload, pcm);
            wait_until(|| job.reserved.load(Ordering::Acquire) == 0);
        }
        assert_eq!(job.state(), STREAMING);
        assert!(fixture.runtime.try_lock().is_ok());
        job.stop();
        assert_reaped(&job);
        assert_eq!(job.state(), STOPPED);
        assert!(peer.receive_record().is_err());
    }
    #[test]
    fn revoke_after_real_pcm_closes_media_and_prevents_next_capture() {
        let (fixture, job, mut peer) = running();
        job.submit(job.reserve(960).unwrap(), [0x31; 960]).unwrap();
        assert_eq!(peer.receive_record().unwrap().len(), 1064);
        fixture.revoke();
        assert_reaped(&job);
        assert_eq!(job.state(), FAILED);
        assert!(job.reserve(960).is_err());
        assert!(peer.receive_record().is_err());
    }
    #[test]
    fn authenticated_source_renew_request_is_denied_and_terminates_production_owner() {
        let (mut fixture, job, mut peer) = running();
        let mut host = AndroidCatalogRuntimeV1::new(include_str!(
            "../../fixtures/android-core-catalog-v1.json"
        ))
        .unwrap();
        host.runtime = fixture.runtime.lock().unwrap().clone();
        let request_id = fixture
            ._sink_control
            .send_port_lease_request(
                job.binding.route_id,
                job.binding.source,
                capyio_protocol::PortLeaseAction::Renew,
                10_000,
            )
            .unwrap();
        let ReceivedControlMessage::PortLeaseRequest(request) =
            fixture.source_control.receive_control_message().unwrap()
        else {
            panic!("authenticated renewal request missing")
        };
        host.answer_audio_port_lease(&mut fixture.source_control, request, now().unwrap())
            .unwrap();
        let ReceivedControlMessage::PortLeaseResponse(response) =
            fixture._sink_control.receive_control_message().unwrap()
        else {
            panic!("correlated renewal denial missing")
        };
        assert_eq!(response.request_message_id, request_id);
        assert_eq!(response.outcome, capyio_protocol::PortLeaseOutcome::Denied);
        assert_eq!(
            host.runtime.route(job.binding.route_id).unwrap().state,
            RouteState::Offline
        );
        assert!(
            host.runtime
                .route_port_lease_claim(job.binding.route_id, job.binding.source)
                .is_none()
        );
        *fixture.runtime.lock().unwrap() = host.runtime;
        assert_reaped(&job);
        assert_eq!(job.state(), FAILED);
        assert!(job.reserve(960).is_err());
        assert!(peer.receive_record().is_err());
    }
    #[test]
    fn replacement_lease_or_grant_is_terminal_not_a_renewal() {
        for change in 0..3 {
            let (fixture, job, mut peer) = running();
            {
                let mut runtime = fixture.runtime.lock().unwrap();
                let route = runtime.route(job.binding.route_id).unwrap().clone();
                if change == 2 {
                    runtime
                        .renew_route_authorization(
                            route.id,
                            job.binding.expires_at_ms + 1000,
                            now().unwrap(),
                        )
                        .unwrap();
                } else {
                    let expiry = fixture.prepared.lease_deadline();
                    let replacement = if change == 0 {
                        expiry - 1000
                    } else {
                        expiry + 1000
                    };
                    runtime
                        .acquire_route_port_lease(
                            route.id,
                            route.source,
                            replacement,
                            now().unwrap(),
                        )
                        .unwrap();
                }
            }
            assert_reaped(&job);
            assert_eq!(job.state(), FAILED);
            assert!(peer.receive_record().is_err());
        }
    }
    #[test]
    fn control_owner_session_loss_invalidates_ready_without_capture() {
        let (mut fixture, job, mut peer) = running();
        fixture.source_control.close();
        fixture
            .runtime
            .lock()
            .unwrap()
            .close_session(job.binding.session.session_id)
            .unwrap();
        assert_reaped(&job);
        assert_eq!(job.state(), FAILED);
        assert!(peer.receive_record().is_err());
    }
    #[test]
    fn original_lease_expiry_finishes_worker_without_capture_or_poll_renewal() {
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let fixture = Fixture::with_lease(media.local_addr().unwrap().port(), 1000);
        let accepted = accept_media(fixture.listener(media));
        let job = fixture.start(fixture.snapshot());
        let mut peer = accepted.join().unwrap();
        wait_until(|| job.state() == READY);
        assert_reaped(&job);
        assert_eq!(job.state(), FAILED);
        assert!(peer.receive_record().is_err());
    }
    #[test]
    fn queue_overflow_during_inflight_snapshot_closes_actual_tls_terminally() {
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let fixture = Fixture::new(media.local_addr().unwrap().port());
        let accepted = accept_media(fixture.listener(media));
        // Gate the sender's fresh pre-write snapshot, not the network write.
        let (snapshot, reached, release) = gated_snapshot(fixture.runtime.clone(), 3);
        let job = fixture.start(snapshot);
        let mut peer = accepted.join().unwrap();
        wait_until(|| job.state() == READY);
        job.submit(job.reserve(960).unwrap(), [1; 960]).unwrap();
        release.send(()).unwrap();
        reached.recv_timeout(Duration::from_secs(2)).unwrap();
        job.submit(job.reserve(960).unwrap(), [2; 960]).unwrap();
        assert_eq!(job.reserved.load(Ordering::Acquire), 2);
        assert!(job.reserve(960).is_err());
        assert_eq!(job.state(), FAILED);
        release.send(()).unwrap();
        assert_reaped(&job);
        assert!(peer.receive_record().is_err());
    }
    #[test]
    fn revoke_in_post_write_snapshot_never_publishes_streaming_success() {
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let fixture = Fixture::new(media.local_addr().unwrap().port());
        let accepted = accept_media(fixture.listener(media));
        let (snapshot, reached, release) = gated_snapshot(fixture.runtime.clone(), 4);
        let job = fixture.start(snapshot);
        let mut peer = accepted.join().unwrap();
        wait_until(|| job.state() == READY);
        job.submit(job.reserve(960).unwrap(), [0; 960]).unwrap();
        release.send(()).unwrap();
        assert_eq!(peer.receive_record().unwrap().len(), 1064);
        reached.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(job.state(), READY, "write alone must not publish Streaming");
        fixture.revoke();
        release.send(()).unwrap();
        assert_reaped(&job);
        assert_eq!(job.state(), FAILED);
        assert!(peer.receive_record().is_err());
    }
    #[test]
    fn stop_during_post_write_snapshot_cannot_resurrect_streaming() {
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let fixture = Fixture::new(media.local_addr().unwrap().port());
        let accepted = accept_media(fixture.listener(media));
        let (snapshot, reached, release) = gated_snapshot(fixture.runtime.clone(), 4);
        let job = fixture.start(snapshot);
        let mut peer = accepted.join().unwrap();
        wait_until(|| job.state() == READY);
        job.submit(job.reserve(960).unwrap(), [0; 960]).unwrap();
        release.send(()).unwrap();
        assert_eq!(peer.receive_record().unwrap().len(), 1064);
        reached.recv_timeout(Duration::from_secs(2)).unwrap();
        job.stop();
        release.send(()).unwrap();
        assert_reaped(&job);
        assert_eq!(job.state(), STOPPED);
        assert!(peer.receive_record().is_err());
    }
    #[test]
    fn finite_profiles_bind_actual_job_and_preserve_exact_deadline_caps() {
        for profile in [
            SecureAudioSessionProfile::Finite60sV1,
            SecureAudioSessionProfile::Finite300sV1,
        ] {
            let media = TcpListener::bind("127.0.0.1:0").unwrap();
            let fixture =
                Fixture::with_profile(media.local_addr().unwrap().port(), 299_000, profile);
            let accepted = accept_media(fixture.listener(media));
            let job = fixture.start(fixture.snapshot());
            let mut peer = accepted.join().unwrap();
            wait_until(|| job.state() == READY);
            assert_eq!(job.profile, profile);
            assert_eq!(fixture.prepared.profile(), profile);
            let remaining = job.lease_deadline.saturating_duration_since(Instant::now());
            match profile {
                SecureAudioSessionProfile::Finite60sV1 => {
                    assert!(remaining <= Duration::from_secs(60))
                }
                SecureAudioSessionProfile::Finite300sV1 => {
                    assert!(remaining > Duration::from_secs(290))
                }
            }
            assert_eq!(job.lease_deadline_ms, fixture.prepared.lease_deadline());
            let original = fixture
                .runtime
                .lock()
                .unwrap()
                .route(job.binding.route_id)
                .unwrap()
                .clone();
            assert_eq!(original.selected_format, Some(profile.descriptor()));
            job.submit(job.reserve(960).unwrap(), [0x6c; 960]).unwrap();
            let mut records = SecureMonoPcmAdmission::new(
                PcmTimeline::new(job.binding)
                    .unwrap()
                    .admission
                    .binding()
                    .clone(),
            )
            .unwrap();
            assert_eq!(
                records
                    .admit_record(&peer.receive_record().unwrap())
                    .unwrap()
                    .payload,
                [0x6c; 960]
            );
            wait_until(|| job.state() == STREAMING);
            assert_eq!(
                fixture.runtime.lock().unwrap().route(original.id).unwrap(),
                &original
            );
            let late = job.reserve(960).unwrap();
            job.stop();
            assert!(job.submit(late, [0; 960]).is_err());
            assert_reaped(&job);
            assert_eq!(job.state(), STOPPED);
            assert!(peer.receive_record().is_err());
        }
    }

    #[test]
    fn long_profile_preparation_rejects_wrong_job_profile_before_media_connect() {
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let fixture = Fixture::with_profile(
            media.local_addr().unwrap().port(),
            299_000,
            SecureAudioSessionProfile::Finite300sV1,
        );
        let (job, receiver) = AudioJob::pending(
            7,
            11,
            fixture.prepared.binding(),
            SecureAudioSessionProfile::Finite60sV1,
            fixture.prepared.lease_deadline(),
            fixture.prepared.monotonic_deadline(),
        );
        let prepared = fixture.prepared.clone();
        job.spawn(
            receiver,
            Box::new(move || Ok(prepared)),
            fixture.snapshot(),
            Arc::new(|| {}),
        )
        .unwrap();
        assert_reaped(&job);
        assert_eq!(job.state(), FAILED);
        media.set_nonblocking(true).unwrap();
        assert!(
            matches!(media.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }

    #[test]
    fn long_profile_original_lease_grant_and_catalog_changes_close_actual_tls() {
        for change in 0..5 {
            let media = TcpListener::bind("127.0.0.1:0").unwrap();
            let fixture = Fixture::with_profile(
                media.local_addr().unwrap().port(),
                290_000,
                SecureAudioSessionProfile::Finite300sV1,
            );
            let accepted = accept_media(fixture.listener(media));
            let job = fixture.start(fixture.snapshot());
            let mut peer = accepted.join().unwrap();
            wait_until(|| job.state() == READY);
            job.submit(job.reserve(960).unwrap(), [0x31; 960]).unwrap();
            assert_eq!(peer.receive_record().unwrap().len(), 1064);
            wait_until(|| job.state() == STREAMING);
            {
                let mut runtime = fixture.runtime.lock().unwrap();
                let route = runtime.route(job.binding.route_id).unwrap().clone();
                match change {
                    0 | 1 => {
                        let expiry = fixture.prepared.lease_deadline();
                        // Both narrowing and extending the original full lease are forbidden.
                        let replacement = if change == 0 {
                            expiry - 1000
                        } else {
                            expiry + 1000
                        };
                        runtime
                            .acquire_route_port_lease(
                                route.id,
                                route.source,
                                replacement,
                                now().unwrap() + 1,
                            )
                            .unwrap();
                    }
                    2 => runtime
                        .renew_route_authorization(
                            route.id,
                            job.binding.expires_at_ms + 1000,
                            now().unwrap(),
                        )
                        .unwrap(),
                    _ => {
                        let mut node = runtime.snapshot().local_node;
                        let port = node
                            .capabilities
                            .get_mut(&route.source.capability_id)
                            .unwrap()
                            .ports
                            .get_mut(&route.source.port_id)
                            .unwrap();
                        if change == 3 {
                            port.formats =
                                vec![SecureAudioSessionProfile::Finite60sV1.descriptor()];
                        } else {
                            port.formats[0]
                                .parameters
                                .insert("duration".into(), "300".into());
                        }
                        runtime.replace_local_catalog(node).unwrap();
                    }
                }
            }
            assert_reaped(&job);
            assert_eq!(job.state(), FAILED);
            assert!(job.reserve(960).is_err());
            assert!(peer.receive_record().is_err());
        }
    }

    #[test]
    fn long_profile_stop_interrupts_pending_tls_and_rejects_stale_callbacks() {
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let fixture = Fixture::with_profile(
            media.local_addr().unwrap().port(),
            299_000,
            SecureAudioSessionProfile::Finite300sV1,
        );
        let job = fixture.start(fixture.snapshot());
        media.set_nonblocking(true).unwrap();
        let mut held = None;
        wait_until(|| match media.accept() {
            Ok((socket, _)) => {
                held = Some(socket);
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => false,
            Err(error) => panic!("{error}"),
        });
        wait_until(|| job.stop_handle.get().is_some());
        assert_eq!(job.state(), PENDING);
        job.stop();
        assert_reaped(&job);
        assert_eq!(job.state(), STOPPED);
        assert!(job.reserve(960).is_err());
        drop(held);
    }

    #[test]
    fn long_profile_cancel_after_actual_tls_never_publishes_ready() {
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let fixture = Fixture::with_profile(
            media.local_addr().unwrap().port(),
            299_000,
            SecureAudioSessionProfile::Finite300sV1,
        );
        let accepted = accept_media(fixture.listener(media));
        let (snapshot, reached, release) = gated_snapshot(fixture.runtime.clone(), 1);
        let job = fixture.start(snapshot);
        reached.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut peer = accepted.join().unwrap();
        assert_eq!(job.state(), PENDING);
        job.stop();
        release.send(()).unwrap();
        assert_reaped(&job);
        assert_eq!(job.state(), STOPPED);
        assert!(job.reserve(960).is_err());
        assert!(peer.receive_record().is_err());
    }

    #[test]
    fn long_profile_short_original_lease_expires_without_renewal() {
        let media = TcpListener::bind("127.0.0.1:0").unwrap();
        let fixture = Fixture::with_profile(
            media.local_addr().unwrap().port(),
            1000,
            SecureAudioSessionProfile::Finite300sV1,
        );
        let accepted = accept_media(fixture.listener(media));
        let job = fixture.start(fixture.snapshot());
        let mut peer = accepted.join().unwrap();
        wait_until(|| job.state() == READY);
        assert_eq!(job.profile, SecureAudioSessionProfile::Finite300sV1);
        assert!(
            job.lease_deadline.saturating_duration_since(Instant::now()) <= Duration::from_secs(1)
        );
        assert_reaped(&job);
        assert_eq!(job.state(), FAILED);
        assert!(peer.receive_record().is_err());
    }

    fn gated_snapshot(
        runtime: Arc<Mutex<NodeRuntime>>,
        index: usize,
    ) -> (Snapshot, Receiver<()>, mpsc::Sender<()>) {
        let count = AtomicUsize::new(0);
        let (reached_tx, reached_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_rx = Mutex::new(release_rx);
        let snapshot = Arc::new(move || {
            let current = count.fetch_add(1, Ordering::AcqRel);
            if index >= 3 && current == 2 {
                // Hold the first idle-loop snapshot until the test has queued
                // PCM. This removes scheduling-dependent idle poll counts.
                release_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap();
            }
            if current == index {
                reached_tx.send(()).unwrap();
                release_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap();
            }
            // Clone after the gate so a revoke during a completed write is seen.
            Ok(runtime.lock().unwrap().clone())
        });
        (snapshot, reached_rx, release_tx)
    }
    fn nodes(
        profile: SecureAudioSessionProfile,
    ) -> (NodeDescriptor, NodeDescriptor, PortRef, PortRef) {
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
            .find(|c| c.class == CapabilityClass::Microphone)
            .unwrap();
        let port = cap.ports.values_mut().next().unwrap();
        port.formats = vec![profile.descriptor()];
        port.qos_modes.insert(QosMode::Interactive);
        let source = PortRef {
            node_id: android.id,
            capability_id: cap.id,
            port_id: port.id,
        };
        let sink_cap = sink_node
            .capabilities
            .values_mut()
            .find(|cap| cap.class == CapabilityClass::Speaker)
            .unwrap();
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
        let windows_fingerprint =
            CertificateFingerprint::from_certificate_der(&windows_cert).unwrap();
        let android_fingerprint =
            CertificateFingerprint::from_certificate_der(&android_cert).unwrap();
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
}
