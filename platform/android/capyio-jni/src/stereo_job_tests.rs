//! No AudioTrack or physical playback assertion: read copies, and only an exact
//! platform-acceptance ACK followed by current authority increments telemetry.
use super::*;
use crate::stereo_negotiation::tests::fixture::{self, CONTROL_GENERATION, Fixture};
use std::sync::{atomic::AtomicUsize, mpsc};

const JOB_GENERATION: u64 = 23;
fn pcm() -> Vec<u8> {
    (0..480).flat_map(|_| [0x12, 0x34, 0x56, 0x78]).collect()
}
fn wait_until(mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(2);
    while !condition() {
        assert!(
            Instant::now() < end,
            "stereo lifecycle exceeded bounded two-second wait"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
fn bare_job(state: i32) -> Arc<StereoReceiveJob> {
    let now = fixture::wall_now();
    let (_, snapshot) = fixture::pure_snapshot(now);
    let instant = Instant::now();
    Arc::new(StereoReceiveJob {
        runtime: 7,
        generation: JOB_GENERATION,
        snapshot,
        gate: StereoAdmissionGate::new(),
        authority: Arc::new(|_| panic!("PCM callback must not invoke Runtime authority")),
        state: AtomicI32::new(state),
        offer_committed: AtomicBool::new(false),
        address: "127.0.0.1:1".parse().unwrap(),
        accept_deadline: instant + MAX_STEREO_OPERATION,
        listener_stop: None,
        channel_stop: OnceLock::new(),
        slot: Mutex::new(PcmSlot::default()),
        worker: Mutex::new(None),
        finished: AtomicBool::new(false),
        last_wall: AtomicU64::new(now),
        records_admitted: AtomicU64::new(0),
        records_accepted: AtomicU64::new(0),
        bytes_accepted: AtomicU64::new(0),
    })
}
fn put_record(job: &StereoReceiveJob) -> Instant {
    let pending = PendingPcm::new(pcm(), Instant::now(), job.snapshot.end).unwrap();
    let deadline = pending.deadline;
    let mut slot = job.slot.lock().unwrap();
    assert!(slot.pending.is_none());
    slot.pending = Some(pending);
    slot.authorized = true;
    deadline
}
fn read(job: &StereoReceiveJob) -> PcmReadPermit {
    job.try_read(JOB_GENERATION, &mut [0; SECURE_STEREO_PCM_PAYLOAD_BYTES])
        .unwrap()
        .unwrap()
}
fn allow_pure_continuation(job: &StereoReceiveJob) -> (usize, bool) {
    // Deterministic accounting seam only. Hosted tests below use the real worker
    // to revalidate authority and issue every subsequent permit.
    let mut slot = job.slot.lock().unwrap();
    let ack = slot.ack.take().unwrap();
    assert!(!ack.1);
    slot.authorized = true;
    ack
}
mod pure {
    use super::*;
    #[test]
    fn copying_exact_native_pcm_never_counts_acceptance_and_holds_one_permit() {
        let job = bare_job(ENABLED);
        let deadline = put_record(&job);
        let mut buffer = [0; SECURE_STEREO_PCM_PAYLOAD_BYTES];
        let permit = job.try_read(JOB_GENERATION, &mut buffer).unwrap().unwrap();
        assert_eq!(buffer.as_slice(), pcm());
        assert_eq!(
            (permit.generation, permit.offset, permit.length),
            (JOB_GENERATION, 0, 1920)
        );
        assert!(permit.token > 0 && permit.token <= i64::MAX as u64);
        assert!(job.try_read(JOB_GENERATION, &mut buffer).unwrap().is_none());
        assert_eq!(
            (
                job.metrics().records_accepted,
                job.metrics().pcm_bytes_accepted
            ),
            (0, 0)
        );
        assert!(job.acknowledge(JOB_GENERATION, permit.token, 0, 4).unwrap());
        assert!(
            job.try_read(JOB_GENERATION, &mut buffer).unwrap().is_none(),
            "ACK must await worker-side authority validation"
        );
        let slot = job.slot.lock().unwrap();
        assert_eq!(slot.pending.as_ref().unwrap().deadline, deadline);
        assert_eq!(slot.pending.as_ref().unwrap().offset, 4);
        assert_eq!(slot.ack, Some((4, false)));
        assert_eq!(
            (
                job.metrics().records_accepted,
                job.metrics().pcm_bytes_accepted
            ),
            (0, 0)
        );
    }
    #[test]
    fn zero_and_partial_acks_consume_tokens_without_resetting_frame_deadline() {
        let job = bare_job(ENABLED);
        let deadline = put_record(&job);
        let first = read(&job);
        assert!(job.acknowledge(JOB_GENERATION, first.token, 0, 0).unwrap());
        assert_eq!(allow_pure_continuation(&job), (0, false));
        let second = read(&job);
        assert_ne!(first.token, second.token);
        assert_eq!((second.offset, second.length), (0, 1920));
        assert!(
            job.acknowledge(JOB_GENERATION, second.token, 0, 956)
                .unwrap()
        );
        assert_eq!(allow_pure_continuation(&job), (956, false));
        let mut buffer = [0xff; 1920];
        let third = job.try_read(JOB_GENERATION, &mut buffer).unwrap().unwrap();
        assert_eq!((third.offset, third.length), (956, 964));
        assert_eq!(&buffer[..964], &pcm()[956..]);
        assert_eq!(&buffer[964..], &[0xff; 956]);
        assert_eq!(
            job.slot.lock().unwrap().pending.as_ref().unwrap().deadline,
            deadline
        );
        assert!(
            job.acknowledge(JOB_GENERATION, third.token, 956, 964)
                .unwrap()
        );
        assert_eq!(job.slot.lock().unwrap().ack, Some((964, true)));
        assert_eq!(job.metrics().records_accepted, 0);
    }
    #[test]
    fn stale_generation_wrong_token_offset_alignment_and_negative_equivalent_fail_closed() {
        for mutation in 0..7 {
            let job = bare_job(ENABLED);
            put_record(&job);
            let permit = read(&job);
            let (mut generation, mut token, mut offset, mut count) =
                (JOB_GENERATION, permit.token, permit.offset, permit.length);
            match mutation {
                0 => generation += 1,
                1 => token = 0,
                2 => token = token.saturating_add(1),
                3 => offset = 4,
                4 => count = 1924,
                5 => count = 1,
                _ => count = usize::MAX,
            }
            assert!(job.acknowledge(generation, token, offset, count).is_err());
            assert_eq!(job.state(), FAILED);
            assert!(!job.gate().is_open());
            assert_eq!(job.metrics().pcm_bytes_accepted, 0);
            assert_eq!(job.slot.lock().unwrap().pending.as_ref().unwrap().offset, 0);
        }
    }
    #[test]
    fn consumed_and_cross_job_permits_cannot_acknowledge_another_record() {
        let first = bare_job(ENABLED);
        let second = bare_job(ENABLED);
        put_record(&first);
        put_record(&second);
        let old = read(&first);
        let current = read(&second);
        assert_ne!(
            old.token, current.token,
            "even numerically equal generations require distinct job tokens"
        );
        assert!(
            second
                .acknowledge(JOB_GENERATION, old.token, old.offset, old.length)
                .is_err()
        );
        assert_eq!(second.metrics().pcm_bytes_accepted, 0);
        assert!(first.acknowledge(JOB_GENERATION, old.token, 0, 4).unwrap());
        assert!(first.acknowledge(JOB_GENERATION, old.token, 0, 4).is_err());
        assert_eq!(first.state(), FAILED);
    }
    #[test]
    fn callbacks_never_wait_for_slot_worker_or_runtime_locks() {
        let job = bare_job(ENABLED);
        put_record(&job);
        let _worker_guard = job.worker.lock().unwrap();
        let permit = read(&job);
        {
            let _slot_guard = job.slot.lock().unwrap();
            assert!(
                job.try_read(JOB_GENERATION, &mut [0; 1920])
                    .unwrap()
                    .is_none()
            );
            assert!(
                !job.acknowledge(JOB_GENERATION, permit.token, 0, 1920)
                    .unwrap()
            );
        }
        // false did not consume the token. Retry ACK, not the platform write.
        assert!(
            job.acknowledge(JOB_GENERATION, permit.token, 0, 1920)
                .unwrap()
        );
        assert_eq!(job.slot.lock().unwrap().ack, Some((1920, true)));
    }
    #[test]
    fn wrong_copy_generation_or_buffer_length_rejects_before_pcm_copy() {
        for (generation, length) in [
            (JOB_GENERATION + 1, 1920),
            (JOB_GENERATION, 0),
            (JOB_GENERATION, 1916),
            (JOB_GENERATION, 2128),
        ] {
            let job = bare_job(ENABLED);
            put_record(&job);
            let mut bytes = vec![0xaa; length];
            assert!(job.try_read(generation, &mut bytes).is_err());
            assert!(bytes.iter().all(|byte| *byte == 0xaa));
            assert_eq!(job.state(), FAILED);
            assert!(job.slot.lock().unwrap().permit.is_none());
        }
    }
    #[test]
    fn exact_250ms_budget_and_attempt_cap_do_not_restart_after_progress() {
        let job = bare_job(ENABLED);
        let deadline = put_record(&job);
        let permit = read(&job);
        assert!(job.acknowledge(JOB_GENERATION, permit.token, 0, 4).unwrap());
        allow_pure_continuation(&job);
        assert_eq!(
            job.slot.lock().unwrap().pending.as_ref().unwrap().deadline,
            deadline
        );
        let next = read(&job);
        job.slot.lock().unwrap().pending.as_mut().unwrap().deadline = Instant::now();
        assert!(
            job.acknowledge(JOB_GENERATION, next.token, next.offset, next.length)
                .is_err()
        );
        assert_eq!(job.slot.lock().unwrap().pending.as_ref().unwrap().offset, 4);
        assert_eq!(job.metrics().pcm_bytes_accepted, 0);
        let job = bare_job(ENABLED);
        put_record(&job);
        job.slot.lock().unwrap().pending.as_mut().unwrap().attempts =
            crate::stereo_admission::MAX_WRITE_ATTEMPTS;
        assert!(job.try_read(JOB_GENERATION, &mut [0; 1920]).is_err());
        assert_eq!(job.state(), FAILED);
    }
    #[test]
    fn no_read_before_bound_enable_and_enable_is_one_shot() {
        for state in [PENDING, BOUND] {
            let job = bare_job(state);
            put_record(&job);
            assert!(
                job.try_read(JOB_GENERATION, &mut [0; 1920])
                    .unwrap()
                    .is_none()
            );
            if state == PENDING {
                assert!(job.enable().is_err());
            } else {
                job.enable().unwrap();
                assert_eq!(job.state(), ENABLED);
                assert!(job.enable().is_err());
            }
            assert_eq!(job.state(), FAILED);
        }
    }
    #[test]
    fn stop_and_invalidation_reject_late_ack_without_acceptance_or_reopen() {
        for stop in [false, true] {
            let job = bare_job(ENABLED);
            put_record(&job);
            let permit = read(&job);
            if stop {
                job.stop();
            } else {
                job.gate().invalidate();
            }
            assert!(
                job.acknowledge(JOB_GENERATION, permit.token, permit.offset, 1920)
                    .is_err()
            );
            assert!(job.try_read(JOB_GENERATION, &mut [0; 1920]).is_err());
            assert_eq!(job.metrics().pcm_bytes_accepted, 0);
            assert_eq!(job.state(), if stop { STOPPED } else { FAILED });
            job.fail();
            assert_eq!(job.state(), if stop { STOPPED } else { FAILED });
        }
    }
    #[test]
    fn gate_clone_and_generation_tombstone_are_permanent() {
        let gate = StereoAdmissionGate::new();
        let clone = gate.clone();
        clone.invalidate();
        assert!(!gate.is_open());
        let mut claim = StereoGenerationClaim::default();
        let mut clone = claim.clone();
        claim.reserve().unwrap();
        assert!(clone.reserve().is_err());
        assert!(claim.reserve().is_err());
        assert!(claim.reserve().is_err());
    }
    #[test]
    fn finished_only_reaper_never_detaches_or_joins_a_running_worker() {
        let job = bare_job(BOUND);
        let (release, waiting) = mpsc::sync_channel(1);
        *job.worker.lock().unwrap() = Some(thread::spawn(move || {
            waiting.recv().unwrap();
        }));
        job.reap_finished();
        assert!(!job.worker_exited());
        assert!(job.worker.lock().unwrap().is_some());
        release.send(()).unwrap();
        wait_until(|| job.worker_exited());
        job.reap_finished();
        assert!(job.worker.lock().unwrap().is_none());
        assert!(job.finished());
        job.reap_finished();
    }
    #[test]
    fn reconcile_under_runtime_guard_invalidates_callbacks_without_authority_callback() {
        let now = fixture::wall_now();
        let (mut runtime, snapshot) = fixture::pure_snapshot(now);
        let mut job = Arc::try_unwrap(bare_job(ENABLED)).ok().unwrap();
        job.snapshot = snapshot;
        put_record(&job);
        let permit = read(&job);
        runtime
            .release_route_port_lease(
                job.snapshot.original.id,
                job.snapshot.original.sink,
                job.snapshot.original.session_id,
            )
            .unwrap();
        assert!(job.reconcile(&runtime, CONTROL_GENERATION, now).is_err());
        assert!(
            job.acknowledge(JOB_GENERATION, permit.token, 0, 1920)
                .is_err()
        );
        assert_eq!(job.metrics().pcm_bytes_accepted, 0);
    }
}

/// Real shared control and data TLS, production negotiation, authoritative
/// Runtime callbacks, and production worker. Required hosted execution only.
mod hosted {
    use super::*;
    use capyio_audio::AudioMediaPacket;
    use capyio_control_link::ReceivedControlMessage;
    use capyio_control_tls::{ControlTlsEndpoint, audio_stereo::StereoAudioTlsChannel};
    use capyio_core::RouteState;

    struct Running {
        fixture: Fixture,
        job: Arc<StereoReceiveJob>,
        failures: Arc<AtomicUsize>,
        control_generation: Arc<AtomicU64>,
        media: Option<StereoAudioTlsChannel>,
    }
    impl Running {
        fn new(accepts: bool) -> Self {
            Self::with_lifetime(accepts, Duration::from_secs(10))
        }
        fn with_lifetime(accepts: bool, lifetime: Duration) -> Self {
            let mut fixture = Fixture::with_lifetime(accepts, lifetime);
            let snapshot = fixture.complete(!accepts);
            let runtime = Arc::clone(&fixture.runtime);
            let control_generation = Arc::new(AtomicU64::new(CONTROL_GENERATION));
            let generation = Arc::clone(&control_generation);
            let authority: StereoAuthority = Arc::new(move |snapshot| {
                let guard = runtime.lock().unwrap();
                snapshot.validate(
                    &guard,
                    generation.load(Ordering::Acquire),
                    fixture::wall_now(),
                    Instant::now(),
                )
            });
            let failures = Arc::new(AtomicUsize::new(0));
            let failure_count = Arc::clone(&failures);
            fixture.assert_unlocked();
            let job = StereoReceiveJob::start(
                7,
                JOB_GENERATION,
                snapshot,
                fixture.identity.clone(),
                CONTROL_GENERATION,
                StereoAdmissionGate::new(),
                authority,
                Arc::new(move || {
                    failure_count.fetch_add(1, Ordering::AcqRel);
                }),
            )
            .unwrap();
            Self {
                fixture,
                job,
                failures,
                control_generation,
                media: None,
            }
        }
        fn publish(&mut self) {
            self.fixture.assert_unlocked();
            self.fixture
                .sink
                .send_secure_stereo_audio_data_offer_until(
                    self.job.snapshot().route(),
                    self.job.offer(),
                    fixture::wall_now(),
                    self.job.operation_deadline(),
                )
                .unwrap();
            self.job.offer_committed().unwrap();
            let ReceivedControlMessage::RouteStatus(status) = self
                .fixture
                .source
                .receive_control_message_until(self.job.operation_deadline())
                .unwrap()
            else {
                panic!("missing actual stereo media offer")
            };
            assert_eq!(status.route, *self.job.snapshot().route());
            assert_eq!(status.route.state, RouteState::Starting);
            assert_eq!(status.secure_audio_data, Some(self.job.offer()));
        }
        fn connect(&mut self, enable: bool) {
            self.publish();
            self.fixture.assert_unlocked();
            self.media = Some(
                StereoAudioTlsChannel::connect_until(
                    ControlTlsEndpoint {
                        address: self.job.listener_address(),
                        io_timeout: MAX_STEREO_OPERATION,
                    },
                    "localhost",
                    self.fixture.source_identity.clone(),
                    self.job.snapshot().binding(fixture::wall_now()).unwrap(),
                    self.job.operation_deadline(),
                )
                .unwrap(),
            );
            wait_until(|| self.job.state() != PENDING);
            assert_eq!(self.job.state(), BOUND);
            assert_eq!(self.job.metrics().records_admitted, 0);
            if enable {
                self.job.enable().unwrap();
            }
        }
        fn record(&self, sequence: u64) -> Vec<u8> {
            let route = self.job.snapshot().route();
            let binding = AudioMediaStreamBinding {
                session_id: route.session_id,
                route_id: route.id,
                stream_id: self.job.offer().stream_id,
                stream_epoch: route.epoch as u32,
                selected_spec: AudioStreamSpec::media_balanced(),
            };
            let mut parser = SecureStereoPcmAdmission::new(binding.clone()).unwrap();
            let mut wire = Vec::new();
            for sequence in 0..=sequence {
                wire = parser
                    .encode_record(&AudioMediaPacket {
                        stream_id: binding.stream_id,
                        stream_epoch: binding.stream_epoch,
                        sequence,
                        source_timestamp_micros: 100 + sequence * 10_000,
                        first_sample_index: sequence * 480,
                        sample_count: 480,
                        discontinuity: false,
                        payload: pcm(),
                    })
                    .unwrap()
                    .to_vec();
            }
            wire
        }
        fn send(&mut self, sequence: u64) {
            let record = self.record(sequence);
            self.media.as_mut().unwrap().send_record(&record).unwrap();
        }
        fn permit(&self) -> (PcmReadPermit, [u8; 1920]) {
            let mut buffer = [0; 1920];
            let mut permit = None;
            wait_until(|| {
                permit = self.job.try_read(JOB_GENERATION, &mut buffer).unwrap();
                permit.is_some()
            });
            (permit.unwrap(), buffer)
        }
        fn ack(&self, permit: PcmReadPermit, count: usize) {
            wait_until(|| {
                self.job
                    .acknowledge(JOB_GENERATION, permit.token, permit.offset, count)
                    .unwrap()
            });
        }
        fn terminal(&self, expected: i32) {
            wait_until(|| self.job.worker_exited());
            assert_eq!(self.job.state(), expected);
            // Observe terminal state/counters before reaping, not after cleanup.
            assert!(!self.job.gate().is_open());
            self.job.reap_finished();
            assert!(self.job.finished());
            assert!(self.job.worker.lock().unwrap().is_none());
        }
    }
    impl Drop for Running {
        fn drop(&mut self) {
            self.job.stop();
            wait_until(|| self.job.worker_exited());
            self.job.reap_finished();
        }
    }
    #[test]
    fn ipc_stereo_job_real_tls_copy_is_not_acceptance_and_partial_ack_preserves_channels() {
        for accepts in [false, true] {
            let mut f = Running::new(accepts);
            f.connect(true);
            f.send(0);
            let (first, buffer) = f.permit();
            assert_eq!(buffer.as_slice(), pcm());
            assert_eq!(f.job.metrics().records_admitted, 1);
            assert_eq!(
                (
                    f.job.metrics().records_accepted,
                    f.job.metrics().pcm_bytes_accepted
                ),
                (0, 0)
            );
            f.send(1);
            thread::sleep(Duration::from_millis(5));
            assert_eq!(
                f.job.metrics().records_admitted,
                1,
                "no second TLS read while native record is pending"
            );
            f.ack(first, 4);
            let (second, _) = f.permit();
            assert_eq!((second.offset, second.length), (4, 1916));
            assert_eq!(f.job.metrics().pcm_bytes_accepted, 4);
            f.ack(second, 0);
            let (third, _) = f.permit();
            assert_eq!((third.offset, third.length), (4, 1916));
            assert_ne!(second.token, third.token);
            f.ack(third, third.length);
            let (next, bytes) = f.permit();
            assert_eq!((next.offset, next.length), (0, 1920));
            assert_eq!(bytes.as_slice(), pcm());
            assert_eq!(
                (
                    f.job.metrics().records_accepted,
                    f.job.metrics().pcm_bytes_accepted
                ),
                (1, 1920)
            );
            assert_eq!(
                f.fixture
                    .runtime
                    .lock()
                    .unwrap()
                    .route(f.fixture.route_id)
                    .unwrap()
                    .state,
                RouteState::Starting
            );
            f.job.stop();
            f.terminal(STOPPED);
            assert_eq!(f.failures.load(Ordering::Acquire), 0);
        }
    }
    #[test]
    fn ipc_stereo_job_offer_commit_and_bound_enable_gate_every_media_permit() {
        let mut f = Running::new(true);
        assert_eq!(f.job.state(), PENDING);
        assert!(
            f.job
                .try_read(JOB_GENERATION, &mut [0; 1920])
                .unwrap()
                .is_none()
        );
        assert_eq!(f.job.metrics().records_admitted, 0);
        f.connect(false);
        f.send(0);
        thread::sleep(Duration::from_millis(10));
        assert_eq!(f.job.state(), BOUND);
        assert_eq!(f.job.metrics().records_admitted, 0);
        assert!(
            f.job
                .try_read(JOB_GENERATION, &mut [0; 1920])
                .unwrap()
                .is_none()
        );
        f.job.enable().unwrap();
        let (permit, _) = f.permit();
        assert_eq!(f.job.metrics().records_accepted, 0);
        f.ack(permit, 1920);
        wait_until(|| f.job.metrics().records_accepted == 1);
        f.job.stop();
        f.terminal(STOPPED);
    }
    #[test]
    fn ipc_stereo_job_uncommitted_offer_and_repeated_commit_fail_without_media() {
        for repeated in [false, true] {
            let mut f = Running::new(false);
            let original_end = f.job.operation_deadline();
            if repeated {
                f.publish();
                assert!(f.job.offer_committed().is_err());
            }
            f.terminal(FAILED);
            assert_eq!(f.job.operation_deadline(), original_end);
            assert_eq!(f.job.metrics().records_admitted, 0);
            assert_eq!(f.job.metrics().pcm_bytes_accepted, 0);
            assert_eq!(f.failures.load(Ordering::Acquire), 1);
        }
    }
    #[test]
    fn ipc_stereo_job_zero_progress_keeps_one_original_250ms_budget() {
        let mut f = Running::new(true);
        f.connect(true);
        f.send(0);
        let (permit, _) = f.permit();
        let original_deadline = f
            .job
            .slot
            .lock()
            .unwrap()
            .pending
            .as_ref()
            .unwrap()
            .deadline;
        f.ack(permit, 0);
        let (permit, _) = f.permit();
        assert_eq!(
            f.job
                .slot
                .lock()
                .unwrap()
                .pending
                .as_ref()
                .unwrap()
                .deadline,
            original_deadline
        );
        f.ack(permit, 4);
        let (permit, _) = f.permit();
        assert_eq!(permit.offset, 4);
        assert_eq!(
            f.job
                .slot
                .lock()
                .unwrap()
                .pending
                .as_ref()
                .unwrap()
                .deadline,
            original_deadline
        );
        wait_until(|| Instant::now() >= original_deadline);
        assert!(
            f.job
                .acknowledge(JOB_GENERATION, permit.token, permit.offset, permit.length)
                .is_err()
        );
        f.terminal(FAILED);
        assert_eq!(f.job.metrics().records_accepted, 0);
        assert_eq!(f.job.metrics().pcm_bytes_accepted, 4);
    }
    #[test]
    fn ipc_stereo_job_ack_queued_before_stop_or_revoke_never_publishes_acceptance() {
        for stop in [false, true] {
            let mut f = Running::new(false);
            f.connect(true);
            f.send(0);
            let (permit, _) = f.permit();
            {
                // Worker must re-enter this same authoritative mutex after ACK.
                // The callback does not enter it, so this proves ordering rather
                // than relying on a timing race or a cloned Runtime snapshot.
                let mut runtime = f.fixture.runtime.lock().unwrap();
                f.ack(permit, 1920);
                if stop {
                    f.job.stop();
                    runtime.begin_route_stop(f.fixture.route_id).unwrap();
                    runtime.stop_route(f.fixture.route_id).unwrap();
                } else {
                    f.job.gate().invalidate();
                    let route = f.job.snapshot().route();
                    runtime
                        .release_route_port_lease(route.id, route.sink, route.session_id)
                        .unwrap();
                }
            }
            f.terminal(if stop { STOPPED } else { FAILED });
            assert_eq!(
                (
                    f.job.metrics().records_accepted,
                    f.job.metrics().pcm_bytes_accepted
                ),
                (0, 0)
            );
            assert!(
                f.job
                    .acknowledge(JOB_GENERATION, permit.token, permit.offset, 1920)
                    .is_err()
            );
        }
    }
    #[test]
    fn ipc_stereo_job_control_generation_loss_and_mutated_lease_close_admission() {
        for generation_loss in [false, true] {
            let mut f = Running::new(true);
            f.connect(false);
            if generation_loss {
                f.control_generation
                    .store(CONTROL_GENERATION + 1, Ordering::Release);
            } else {
                let mut runtime = f.fixture.runtime.lock().unwrap();
                let route = f.job.snapshot().route();
                f.job.gate().invalidate();
                runtime
                    .release_route_port_lease(route.id, route.sink, route.session_id)
                    .unwrap();
            }
            f.terminal(FAILED);
            assert_eq!(f.job.metrics().records_admitted, 0);
            assert!(f.job.enable().is_err());
        }
    }
    #[test]
    fn ipc_stereo_job_original_lease_expiry_cannot_be_renewed_by_bound_readiness() {
        let mut f = Running::with_lifetime(false, Duration::from_secs(2));
        f.connect(false);
        let original = f.job.snapshot().monotonic_deadline();
        let wall = f.job.snapshot().wall_deadline_ms();
        wait_until(|| Instant::now() >= original || fixture::wall_now() >= wall);
        assert_eq!(f.job.state(), FAILED);
        assert_eq!(f.job.snapshot().monotonic_deadline(), original);
        assert_eq!(f.job.snapshot().wall_deadline_ms(), wall);
        assert!(f.job.enable().is_err());
        assert_eq!(f.job.metrics().records_admitted, 0);
        f.terminal(FAILED);
    }
    #[test]
    fn ipc_stereo_job_wrong_data_pin_and_exact_binding_are_rejected_by_real_tls() {
        for wrong_pin in [false, true] {
            let mut f = Running::new(true);
            f.publish();
            let mut identity = f.fixture.source_identity.clone();
            let mut binding = f.job.snapshot().binding(fixture::wall_now()).unwrap();
            if wrong_pin {
                identity.expected_peer_certificate =
                    capyio_pairing::CertificateFingerprint::new([0x44; 32]).unwrap();
            } else {
                binding.epoch += 1;
            }
            let result = StereoAudioTlsChannel::connect_until(
                ControlTlsEndpoint {
                    address: f.job.listener_address(),
                    io_timeout: MAX_STEREO_OPERATION,
                },
                "localhost",
                identity,
                binding,
                f.job.operation_deadline(),
            );
            assert!(result.is_err());
            f.terminal(FAILED);
            assert_eq!(f.job.metrics().records_admitted, 0);
        }
    }
    #[test]
    fn ipc_stereo_job_wrong_current_identity_profile_and_claim_reject_before_listener() {
        for mutation in 0..4 {
            let mut fixture = Fixture::new(false);
            let mut snapshot = fixture.complete(true);
            let mut identity = fixture.identity.clone();
            let generation = if mutation == 0 {
                CONTROL_GENERATION + 1
            } else {
                CONTROL_GENERATION
            };
            if mutation == 1 {
                identity.expected_peer_certificate =
                    capyio_pairing::CertificateFingerprint::new([0x66; 32]).unwrap();
            }
            if mutation == 2 {
                snapshot.original.selected_format = Some(capyio_core::FormatDescriptor::new(
                    capyio_protocol::SECURE_AUDIO_MONO_FORMAT,
                ));
            }
            if mutation == 3 {
                let route = snapshot.route();
                fixture
                    .runtime
                    .lock()
                    .unwrap()
                    .release_route_port_lease(route.id, route.sink, route.session_id)
                    .unwrap();
            }
            let runtime = Arc::clone(&fixture.runtime);
            let gate = StereoAdmissionGate::new();
            let result = StereoReceiveJob::start(
                7,
                JOB_GENERATION,
                snapshot,
                identity,
                generation,
                gate.clone(),
                Arc::new(move |snapshot| {
                    snapshot.validate(
                        &runtime.lock().unwrap(),
                        CONTROL_GENERATION,
                        fixture::wall_now(),
                        Instant::now(),
                    )
                }),
                Arc::new(|| panic!("rejected preparation spawned worker")),
            );
            assert!(result.is_err());
            assert!(!gate.is_open());
        }
    }
    #[test]
    fn ipc_stereo_job_malformed_second_fragment_and_wire_replay_are_terminal() {
        for replay in [false, true] {
            let mut f = Running::new(false);
            f.connect(true);
            let mut wire = f.record(0);
            if replay {
                f.media.as_mut().unwrap().send_record(&wire).unwrap();
                let (permit, _) = f.permit();
                f.ack(permit, 1920);
                wait_until(|| f.job.metrics().records_accepted == 1);
            } else {
                wire[1200] ^= 1;
            }
            f.media.as_mut().unwrap().send_record(&wire).unwrap();
            f.terminal(FAILED);
            assert_eq!(f.job.metrics().records_accepted, u64::from(replay));
            assert_eq!(f.job.metrics().records_admitted, u64::from(replay));
        }
    }
    #[test]
    fn ipc_stereo_job_stop_interrupts_accept_bound_wait_and_actual_pending_read() {
        for phase in 0..3 {
            let mut f = Running::new(true);
            match phase {
                0 => f.publish(),
                1 => f.connect(false),
                _ => f.connect(true),
            }
            let started = Instant::now();
            f.job.stop();
            f.terminal(STOPPED);
            assert!(started.elapsed() < Duration::from_secs(1));
            assert_eq!(f.job.metrics().records_admitted, 0);
            assert_eq!(f.failures.load(Ordering::Acquire), 0);
        }
    }
    #[test]
    fn ipc_stereo_job_weak_owner_callbacks_release_owner_and_job_after_finished_reap() {
        struct Owner {
            runtime: Arc<Mutex<NodeRuntime>>,
            gate: StereoAdmissionGate,
            job: OnceLock<Arc<StereoReceiveJob>>,
            authority_checks: AtomicUsize,
            failures: AtomicUsize,
        }

        let mut fixture = Fixture::new(false);
        let snapshot = fixture.complete(true);
        let owner = Arc::new(Owner {
            runtime: Arc::clone(&fixture.runtime),
            gate: StereoAdmissionGate::new(),
            job: OnceLock::new(),
            authority_checks: AtomicUsize::new(0),
            failures: AtomicUsize::new(0),
        });
        let registry = Mutex::new(Some(Arc::clone(&owner)));
        let mut pending_link_owner = Some(Arc::clone(&owner));
        let weak_owner = Arc::downgrade(&owner);
        let authority_owner = Arc::downgrade(&owner);
        let failure_owner = Arc::downgrade(&owner);
        let authority: StereoAuthority = Arc::new(move |snapshot| {
            let owner = authority_owner.upgrade().ok_or("stereo owner released")?;
            if !owner.gate.is_open() {
                return Err("stereo owner stopped".into());
            }
            owner.authority_checks.fetch_add(1, Ordering::AcqRel);
            snapshot.validate(
                &owner.runtime.lock().unwrap(),
                CONTROL_GENERATION,
                fixture::wall_now(),
                Instant::now(),
            )?;
            if owner.gate.is_open() {
                Ok(())
            } else {
                Err("stereo owner stopped during validation".into())
            }
        });
        let failure: StereoFailure = Arc::new(move || {
            if let Some(owner) = failure_owner.upgrade() {
                owner.failures.fetch_add(1, Ordering::AcqRel);
                owner.gate.invalidate();
                if let Some(job) = owner.job.get() {
                    job.reject_callback();
                }
            }
        });
        fixture.assert_unlocked();
        let job = StereoReceiveJob::start(
            7,
            JOB_GENERATION,
            snapshot,
            fixture.identity.clone(),
            CONTROL_GENERATION,
            owner.gate.clone(),
            authority,
            failure,
        )
        .unwrap();
        let weak_job = Arc::downgrade(&job);
        assert!(owner.job.set(Arc::clone(&job)).is_ok());
        fixture
            .sink
            .send_secure_stereo_audio_data_offer_until(
                job.snapshot().route(),
                job.offer(),
                fixture::wall_now(),
                job.operation_deadline(),
            )
            .unwrap();
        job.offer_committed().unwrap();
        let ReceivedControlMessage::RouteStatus(status) = fixture
            .source
            .receive_control_message_until(job.operation_deadline())
            .unwrap()
        else {
            panic!("missing actual stereo media offer")
        };
        assert_eq!(status.secure_audio_data, Some(job.offer()));
        let media = StereoAudioTlsChannel::connect_until(
            ControlTlsEndpoint {
                address: job.listener_address(),
                io_timeout: MAX_STEREO_OPERATION,
            },
            "localhost",
            fixture.source_identity.clone(),
            job.snapshot().binding(fixture::wall_now()).unwrap(),
            job.operation_deadline(),
        )
        .unwrap();
        wait_until(|| job.state() != PENDING);
        assert_eq!(job.state(), BOUND);
        assert!(owner.authority_checks.load(Ordering::Acquire) > 1);

        job.stop();
        wait_until(|| job.worker_exited());
        assert_eq!(job.state(), STOPPED);
        assert_eq!(owner.failures.load(Ordering::Acquire), 0);
        // Only the active registry and per-link pending entry retain the owner.
        // Its OnceLock owns the job; both callbacks point back through Weak.
        drop(job);
        drop(owner);
        assert!(weak_owner.upgrade().is_some());
        assert!(weak_job.upgrade().is_some());
        let removed = {
            let mut registry = registry.lock().unwrap();
            assert!(
                registry
                    .as_ref()
                    .unwrap()
                    .job
                    .get()
                    .unwrap()
                    .worker_exited()
            );
            registry.take().unwrap()
        };
        // Match the production ordering: remove the finished entry, release
        // the registry guard, then reap. No strong callback cycle can survive.
        let job = removed.job.get().unwrap();
        job.reap_finished();
        assert!(job.finished());
        assert!(job.worker.lock().unwrap().is_none());
        assert_eq!(job.metrics().records_accepted, 0);
        drop(removed);
        assert!(registry.lock().unwrap().is_none());
        assert!(
            weak_owner.upgrade().is_some(),
            "pending control entry must still own its job"
        );
        assert!(weak_job.upgrade().is_some());
        // The dedicated control supervisor separately retires its terminal
        // pending entry, after which there must be no owner or job retention.
        drop(pending_link_owner.take());
        assert!(
            weak_owner.upgrade().is_none(),
            "owner leaked after registry removal"
        );
        assert!(
            weak_job.upgrade().is_none(),
            "job leaked through an owner callback cycle"
        );
        drop(media);
    }
}
