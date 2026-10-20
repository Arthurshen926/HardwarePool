//! Runtime-driven, explicitly insecure lab playback. A Ready Session is not
//! cryptographic authentication. The caller must admit the peer, negotiate the
//! exact format, obtain local consent and acquire the local Sink lease first.
//! No control listener or peer-supplied JSON can construct a worker here.
use crate::{PcmPlaybackSink, PipeWirePlayback, PlaybackConfig, PlaybackMetrics, PlaybackPump};
use capyio_audio::{AudioMediaPacket, AudioMediaStreamBinding};
use capyio_core::{
    FormatDescriptor, Problem, ProblemCategory, ProblemId, ProblemSeverity, QosMode, RouteState,
};
use capyio_native_audio_lan::{
    NativeLanEndpointConfig, NativeLanError, NativeLanReceiveOutcome, NativeLanRouteBinding,
    NativeLanUdpEndpoint,
};
use capyio_runtime::NodeRuntime;
use std::{
    io,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn error(value: impl ToString) -> io::Error {
    io::Error::other(value.to_string())
}

struct Guard {
    binding: NativeLanRouteBinding,
    last_wall_ms: u64,
}
impl Guard {
    fn new(runtime: &NodeRuntime, media: AudioMediaStreamBinding, now_ms: u64) -> io::Result<Self> {
        if runtime.snapshot().local_node.platform != capyio_core::Platform::Linux {
            return Err(error("playback requires a Linux Runtime"));
        }
        let route = runtime.route(media.route_id).map_err(error)?;
        if route.state != RouteState::Starting {
            return Err(error("playback requires an explicitly Starting Route"));
        }
        let session = runtime.session(media.session_id).map_err(error)?;
        let binding = NativeLanRouteBinding::from_live_context(media, route, session, now_ms)
            .map_err(error)?;
        let mut guard = Self {
            binding,
            last_wall_ms: now_ms,
        };
        guard.check(runtime, now_ms)?;
        Ok(guard)
    }
    fn check(&mut self, runtime: &NodeRuntime, now_ms: u64) -> io::Result<()> {
        if now_ms < self.last_wall_ms {
            return Err(error("Route clock regressed"));
        }
        self.last_wall_ms = now_ms;
        let media = self.binding.media();
        let route = runtime.route(media.route_id).map_err(error)?;
        let session = runtime.session(media.session_id).map_err(error)?;
        self.binding
            .require_live_context(route, session, now_ms)
            .map_err(error)?;
        if route.sink.node_id != session.local_node_id
            || route.source.node_id != session.remote_node_id
            || media.selected_spec != capyio_audio::AudioStreamSpec::voice_interactive()
            || route.selected_format != Some(FormatDescriptor::new("pcm-s16le-48000-mono"))
            || route.selected_qos_mode != Some(QosMode::Interactive)
        {
            return Err(error(
                "playback direction or negotiated PCM format does not match",
            ));
        }
        let claim = runtime
            .route_port_lease_claim(route.id, route.sink)
            .ok_or_else(|| error("local playback Sink lease missing"))?;
        if claim.session_id != media.session_id
            || claim.expires_at_ms <= now_ms
            || claim.expires_at_ms > self.binding.expires_at_ms()
        {
            return Err(error("local playback Sink lease expired or mismatched"));
        }
        Ok(())
    }
    // A stale owner must never stop or release a newer epoch's lease.
    fn owns_current(&self, runtime: &NodeRuntime) -> bool {
        let media = self.binding.media();
        runtime.route(media.route_id).is_ok_and(|route| {
            route.session_id == media.session_id && route.epoch == u64::from(media.stream_epoch)
        })
    }
}

trait Resources: PcmPlaybackSink {
    fn receive(&mut self) -> io::Result<Option<AudioMediaPacket>>;
    fn stop(&mut self) -> io::Result<()>;
}
struct NativeResources {
    endpoint: Option<NativeLanUdpEndpoint>,
    sink: PipeWirePlayback,
}
impl PcmPlaybackSink for NativeResources {
    fn try_write_pcm(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.sink.try_write_pcm(bytes)
    }
    fn check_alive(&mut self) -> io::Result<()> {
        self.sink.check_alive()
    }
}
impl Resources for NativeResources {
    fn receive(&mut self) -> io::Result<Option<AudioMediaPacket>> {
        match self
            .endpoint
            .as_mut()
            .ok_or_else(|| error("playback is stopped"))?
            .receive()
        {
            Ok(NativeLanReceiveOutcome::Packet(packet)) => Ok(Some(packet)),
            Ok(_) | Err(NativeLanError::ReceiveTimeout) => Ok(None),
            Err(reason) => Err(error(reason)),
        }
    }
    fn stop(&mut self) -> io::Result<()> {
        // Close ingress even if child cleanup fails.
        self.endpoint.take();
        self.sink.stop()
    }
}

struct Worker<R: Resources> {
    guard: Guard,
    pump: PlaybackPump,
    resources: R,
    stopped: bool,
}
impl<R: Resources> Worker<R> {
    fn start(
        runtime: &mut NodeRuntime,
        media: AudioMediaStreamBinding,
        lifetime: Duration,
        mut clock: impl FnMut() -> io::Result<(u64, Duration)>,
        open: impl FnOnce() -> io::Result<R>,
    ) -> io::Result<Self> {
        let (now, _) = clock()?;
        let guard = Guard::new(runtime, media.clone(), now)?;
        let pump = PlaybackPump::with_binding(lifetime, media)?;
        let resources = match open() {
            Ok(resources) => resources,
            Err(reason) => {
                Self::mark_offline(&guard, runtime);
                return Err(reason);
            }
        };
        let mut worker = Self {
            guard,
            pump,
            resources,
            stopped: false,
        };
        // Resource opening can cross a grant/lease deadline. Check again before Active.
        let result = (|| {
            let (now, elapsed) = clock()?;
            worker.guard.check(runtime, now)?;
            worker.pump.flush(&mut worker.resources, elapsed)?;
            runtime
                .activate_route(worker.guard.binding.media().route_id)
                .map_err(error)
        })();
        if let Err(reason) = result {
            return worker.fail(runtime).and(Err(reason));
        }
        Ok(worker)
    }
    fn poll(
        &mut self,
        runtime: &mut NodeRuntime,
        mut clock: impl FnMut() -> io::Result<(u64, Duration)>,
    ) -> io::Result<()> {
        if self.stopped {
            return Err(error("playback is stopped"));
        }
        if self.guard.owns_current(runtime)
            && runtime
                .route(self.guard.binding.media().route_id)
                .is_ok_and(|route| {
                    matches!(route.state, RouteState::Stopping | RouteState::Stopped)
                })
        {
            return self.stop(runtime);
        }
        let outcome = (|| {
            let (now, elapsed) = clock()?;
            self.guard.check(runtime, now)?;
            self.pump.flush(&mut self.resources, elapsed)?;
            let packet = self.resources.receive()?;
            // Recheck after bounded receive; never write after grant/lease expiry.
            let (now, elapsed) = clock()?;
            self.guard.check(runtime, now)?;
            if let Some(packet) = packet {
                self.pump.accept(&packet, elapsed)?;
            }
            self.pump.flush(&mut self.resources, elapsed)
        })();
        if let Err(reason) = outcome {
            let cleanup = self.fail(runtime);
            return cleanup.and(Err(reason));
        }
        Ok(())
    }
    fn mark_offline(guard: &Guard, runtime: &mut NodeRuntime) {
        if !guard.owns_current(runtime) {
            return;
        }
        let id = guard.binding.media().route_id;
        if runtime
            .route(id)
            .is_ok_and(|r| matches!(r.state, RouteState::Starting | RouteState::Active))
        {
            let _ = runtime.report_route_offline(
                id,
                Problem {
                    id: ProblemId::new(),
                    code: "CAPY.LINUX.PLAYBACK_STOPPED".into(),
                    category: ProblemCategory::Adapter,
                    severity: ProblemSeverity::Error,
                    retryable: true,
                    related_node: None,
                    related_adapter: None,
                    related_route: Some(id),
                    human_message:
                        "Linux playback stopped; restart requires fresh admission and lease checks"
                            .into(),
                    technical_detail: None,
                },
            );
        }
    }
    fn fail(&mut self, runtime: &mut NodeRuntime) -> io::Result<()> {
        let result = self.resources.stop();
        self.stopped = true;
        Self::mark_offline(&self.guard, runtime);
        result
    }
    fn stop(&mut self, runtime: &mut NodeRuntime) -> io::Result<()> {
        // Mark Stopping before cleanup so even a cleanup error cannot leave an
        // apparently Active Route. Retry cleanup, but never resume media.
        self.stopped = true;
        let begin = (|| -> io::Result<()> {
            if self.guard.owns_current(runtime) {
                let id = self.guard.binding.media().route_id;
                if matches!(
                    runtime.route(id).map_err(error)?.state,
                    RouteState::Starting | RouteState::Active | RouteState::Offline
                ) {
                    runtime.begin_route_stop(id).map_err(error)?;
                }
            }
            Ok(())
        })();
        // Resource cleanup runs even if the Runtime transition unexpectedly fails.
        self.resources.stop()?;
        begin?;
        if self.guard.owns_current(runtime) {
            let id = self.guard.binding.media().route_id;
            if runtime.route(id).map_err(error)?.state == RouteState::Stopping {
                runtime.stop_route(id).map_err(error)?;
            }
        }
        Ok(())
    }
}
impl<R: Resources> Drop for Worker<R> {
    fn drop(&mut self) {
        let _ = self.resources.stop();
    }
}

/// One explicitly opted-in lab Route worker. The caller owns the Runtime and
/// must poll on its media worker (receive waits at most 10 ms), and invoke Stop
/// immediately on owner shutdown. Dropping closes resources, not Runtime state.
/// No thread, listener, consent, lease or Session is implicitly created.
/// `Active` means worker readiness, not audible delivery or authenticated UDP.
/// The inherited CLI and default Linux host remain unchanged.
pub struct RuntimePlayback {
    worker: Worker<NativeResources>,
    start: Instant,
}
impl RuntimePlayback {
    /// Takes only already admitted Starting Routes. The required explicit lab
    /// config acknowledges insecure UDP, not authority to access a device.
    pub fn open_trusted_lab(
        runtime: &mut NodeRuntime,
        media: AudioMediaStreamBinding,
        config: &PlaybackConfig,
    ) -> io::Result<Self> {
        // Revalidate public fields; struct construction cannot bypass lab limits.
        PlaybackConfig::parse(&[
            "--trusted-lab".into(),
            config.local.to_string(),
            config.peer.to_string(),
            config.duration.as_secs().to_string(),
            config.target.clone(),
        ])
        .map_err(error)?;
        if config.duration.subsec_nanos() != 0 {
            return Err(error("duration must use whole seconds"));
        }
        let start = Instant::now();
        let binding = media.clone();
        let worker = Worker::start(
            runtime,
            media,
            config.duration,
            || clock(start),
            || {
                let endpoint = NativeLanUdpEndpoint::bind(
                    config.local.into(),
                    NativeLanEndpointConfig {
                        peer: config.peer.into(),
                        read_timeout: Duration::from_millis(10),
                        inflight_packet_capacity: 2,
                    },
                    binding,
                )
                .map_err(error)?;
                let sink = PipeWirePlayback::open(&config.target)?;
                Ok(NativeResources {
                    endpoint: Some(endpoint),
                    sink,
                })
            },
        )?;
        Ok(Self { worker, start })
    }
    pub fn poll(&mut self, runtime: &mut NodeRuntime) -> io::Result<()> {
        self.worker.poll(runtime, || clock(self.start))
    }
    pub fn stop(&mut self, runtime: &mut NodeRuntime) -> io::Result<()> {
        self.worker.stop(runtime)
    }
    pub fn metrics(&self) -> PlaybackMetrics {
        self.worker.pump.metrics()
    }
}
fn clock(start: Instant) -> io::Result<(u64, Duration)> {
    let wall = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(error)?
        .as_millis();
    Ok((u64::try_from(wall).map_err(error)?, start.elapsed()))
}

/// Shared preflight for the authenticated control owner; opens no resources.
pub(crate) fn validate_start(
    runtime: &NodeRuntime,
    media: AudioMediaStreamBinding,
    now_ms: u64,
) -> io::Result<()> {
    Guard::new(runtime, media, now_ms).map(|_| ())
}

#[cfg(test)]
impl RuntimePlayback {
    pub(crate) fn open_synthetic(
        runtime: &mut NodeRuntime,
        media: AudioMediaStreamBinding,
        config: &PlaybackConfig,
    ) -> io::Result<Self> {
        let start = Instant::now();
        let binding = media.clone();
        let worker = Worker::start(
            runtime,
            media,
            config.duration,
            || clock(start),
            || {
                let endpoint = NativeLanUdpEndpoint::bind(
                    config.local.into(),
                    NativeLanEndpointConfig {
                        peer: config.peer.into(),
                        read_timeout: Duration::from_millis(10),
                        inflight_packet_capacity: 2,
                    },
                    binding,
                )
                .map_err(error)?;
                Ok(NativeResources {
                    endpoint: Some(endpoint),
                    sink: PipeWirePlayback::synthetic_cat()?,
                })
            },
        )?;
        Ok(Self { worker, start })
    }
}

pub(crate) fn validate_live(
    runtime: &NodeRuntime,
    media: AudioMediaStreamBinding,
    now_ms: u64,
) -> io::Result<()> {
    let route = runtime.route(media.route_id).map_err(error)?;
    let session = runtime.session(media.session_id).map_err(error)?;
    let binding =
        NativeLanRouteBinding::from_live_context(media, route, session, now_ms).map_err(error)?;
    Guard {
        binding,
        last_wall_ms: now_ms,
    }
    .check(runtime, now_ms)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use capyio_core::{InteroperabilityMode, Platform, RouteBackend, StreamId};
    use capyio_runtime::PortLeasePolicy;
    use std::{cell::RefCell, rc::Rc};

    // Synthetic catalogs only, never an authenticated peer or real device grant.
    pub(crate) fn fixture() -> (NodeRuntime, AudioMediaStreamBinding) {
        let lab = capyio_testkit::DemoLab::new().unwrap();
        let original = lab
            .runtime
            .route(lab.routes.phone_microphone_to_windows)
            .unwrap()
            .clone();
        let snapshot = lab.runtime.snapshot();
        let mut local = snapshot.local_node;
        local.platform = Platform::Linux;
        let mut peer = snapshot.peers[0].clone();
        for node in [&mut local, &mut peer] {
            for adapter in node.adapter_instances.values_mut() {
                adapter
                    .supported_route_modes
                    .insert(RouteBackend::AdapterManaged);
            }
            for cap in node.capabilities.values_mut() {
                for port in cap.ports.values_mut() {
                    port.interoperability_mode = InteroperabilityMode::AdapterManaged;
                }
            }
        }
        let peer_id = peer.id;
        let mut runtime = NodeRuntime::new(local).unwrap();
        runtime.register_peer(peer, true).unwrap();
        let session_id = runtime.open_session(peer_id).unwrap();
        let route_id = runtime
            .create_route(
                session_id,
                original.source,
                original.sink,
                RouteBackend::AdapterManaged,
            )
            .unwrap();
        runtime
            .set_local_port_lease_policy(original.sink, PortLeasePolicy::Exclusive)
            .unwrap();
        runtime.authorize_route(route_id, Some(10_000)).unwrap();
        runtime
            .acquire_route_port_lease(route_id, original.sink, 5_000, 1_000)
            .unwrap();
        runtime
            .prepare_route(
                route_id,
                Some(FormatDescriptor::new("pcm-s16le-48000-mono")),
                QosMode::Interactive,
                1_000,
            )
            .unwrap();
        runtime.begin_route_start(route_id, 1_000).unwrap();
        let media = AudioMediaStreamBinding {
            session_id,
            route_id,
            stream_id: StreamId::new(),
            stream_epoch: 1,
            selected_spec: capyio_audio::AudioStreamSpec::voice_interactive(),
        };
        (runtime, media)
    }
    #[derive(Default)]
    struct State {
        writes: usize,
        stops: usize,
        receives: usize,
        fail_write: bool,
        fail_stop: bool,
    }
    struct Fake {
        state: Rc<RefCell<State>>,
        packet: Option<AudioMediaPacket>,
    }
    impl PcmPlaybackSink for Fake {
        fn try_write_pcm(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let mut state = self.state.borrow_mut();
            if state.fail_write {
                return Err(error("synthetic Sink failure"));
            }
            state.writes += bytes.len();
            Ok(bytes.len())
        }
        fn check_alive(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Resources for Fake {
        fn receive(&mut self) -> io::Result<Option<AudioMediaPacket>> {
            self.state.borrow_mut().receives += 1;
            Ok(self.packet.take())
        }
        fn stop(&mut self) -> io::Result<()> {
            let mut state = self.state.borrow_mut();
            state.stops += 1;
            if state.fail_stop {
                Err(error("synthetic cleanup failure"))
            } else {
                Ok(())
            }
        }
    }
    fn fake(media: &AudioMediaStreamBinding) -> (Fake, Rc<RefCell<State>>) {
        let state = Rc::new(RefCell::new(State::default()));
        (
            Fake {
                state: state.clone(),
                packet: Some(AudioMediaPacket {
                    stream_id: media.stream_id,
                    stream_epoch: media.stream_epoch,
                    sequence: 1,
                    source_timestamp_micros: 10_000,
                    first_sample_index: 0,
                    sample_count: 480,
                    discontinuity: false,
                    payload: vec![7; 960],
                }),
            },
            state,
        )
    }
    fn at(ms: u64) -> io::Result<(u64, Duration)> {
        Ok((ms, Duration::from_millis(ms - 1_000)))
    }
    fn start(
        runtime: &mut NodeRuntime,
        media: AudioMediaStreamBinding,
    ) -> (Worker<Fake>, Rc<RefCell<State>>) {
        let (resources, state) = fake(&media);
        (
            Worker::start(
                runtime,
                media,
                Duration::from_secs(10),
                || at(1_000),
                || Ok(resources),
            )
            .unwrap(),
            state,
        )
    }
    #[test]
    fn route_preflight_rejects_missing_lease_before_open() {
        let (mut runtime, media) = fixture();
        let sink = runtime.route(media.route_id).unwrap().sink;
        runtime
            .release_route_port_lease(media.route_id, sink, media.session_id)
            .unwrap();
        let (resources, _) = fake(&media);
        let mut opened = false;
        assert!(
            Worker::start(
                &mut runtime,
                media,
                Duration::from_secs(10),
                || at(1_000),
                || {
                    opened = true;
                    Ok(resources)
                }
            )
            .is_err()
        );
        assert!(!opened);
    }
    #[test]
    fn route_dynamic_pcm_start_stop_and_drop_are_owned() {
        let (mut runtime, media) = fixture();
        let id = media.route_id;
        let sink = runtime.route(id).unwrap().sink;
        let (mut worker, state) = start(&mut runtime, media);
        assert_eq!(runtime.route(id).unwrap().state, RouteState::Active);
        worker.poll(&mut runtime, || at(1_001)).unwrap();
        assert_eq!(state.borrow().writes, 960);
        worker.stop(&mut runtime).unwrap();
        worker.stop(&mut runtime).unwrap();
        assert_eq!(runtime.route(id).unwrap().state, RouteState::Stopped);
        assert!(runtime.route_port_lease_claim(id, sink).is_none());
        assert!(worker.poll(&mut runtime, || at(1_002)).is_err());
        drop(worker);
        assert_eq!(state.borrow().writes, 960);
        assert_eq!(state.borrow().stops, 3);
    }
    #[test]
    fn route_expiry_during_receive_does_not_forward_pcm() {
        let (mut runtime, media) = fixture();
        let id = media.route_id;
        let (mut worker, state) = start(&mut runtime, media);
        let mut calls = 0;
        assert!(
            worker
                .poll(&mut runtime, || {
                    calls += 1;
                    at(if calls == 1 { 4_999 } else { 5_000 })
                })
                .is_err()
        );
        assert_eq!(state.borrow().receives, 1);
        assert_eq!(state.borrow().writes, 0);
        assert_eq!(state.borrow().stops, 1);
        assert_eq!(runtime.route(id).unwrap().state, RouteState::Offline);
    }
    #[test]
    fn route_open_failure_releases_lease_and_reports_offline() {
        let (mut runtime, media) = fixture();
        let id = media.route_id;
        let sink = runtime.route(id).unwrap().sink;
        assert!(
            Worker::<Fake>::start(
                &mut runtime,
                media,
                Duration::from_secs(10),
                || at(1_000),
                || Err(error("synthetic open failure"))
            )
            .is_err()
        );
        assert_eq!(runtime.route(id).unwrap().state, RouteState::Offline);
        assert!(runtime.route_port_lease_claim(id, sink).is_none());
        assert_eq!(runtime.snapshot().problems.len(), 1);
    }
    #[test]
    fn route_expiry_during_open_rolls_back_resources() {
        let (mut runtime, media) = fixture();
        let (resources, state) = fake(&media);
        let mut calls = 0;
        assert!(
            Worker::start(
                &mut runtime,
                media,
                Duration::from_secs(10),
                || {
                    calls += 1;
                    at(if calls == 1 { 1_000 } else { 5_000 })
                },
                || Ok(resources)
            )
            .is_err()
        );
        assert_eq!(state.borrow().writes, 0);
        assert!(state.borrow().stops >= 1);
    }
    #[test]
    fn route_grant_change_disconnect_and_clock_regression_fail_closed() {
        for case in 0..4 {
            let (mut runtime, media) = fixture();
            let id = media.route_id;
            let (mut worker, state) = start(&mut runtime, media.clone());
            match case {
                0 => runtime
                    .renew_route_authorization(id, 11_000, 1_001)
                    .unwrap(),
                1 => runtime.close_session(media.session_id).unwrap(),
                2 => {}
                _ => {
                    let sink = runtime.route(id).unwrap().sink;
                    runtime
                        .release_route_port_lease(id, sink, media.session_id)
                        .unwrap();
                }
            }
            assert!(
                worker
                    .poll(&mut runtime, || if case == 2 {
                        Ok((999, Duration::ZERO))
                    } else {
                        at(1_001)
                    })
                    .is_err()
            );
            assert_eq!(state.borrow().writes, 0);
            assert_eq!(state.borrow().receives, 0);
            assert!(state.borrow().stops >= 1);
        }
    }
    #[test]
    fn route_stale_owner_cannot_stop_restarted_epoch() {
        let (mut runtime, media) = fixture();
        let id = media.route_id;
        let sink = runtime.route(id).unwrap().sink;
        let (mut worker, _) = start(&mut runtime, media.clone());
        runtime.begin_route_stop(id).unwrap();
        runtime.stop_route(id).unwrap();
        runtime
            .acquire_route_port_lease(id, sink, 5_000, 1_001)
            .unwrap();
        runtime
            .prepare_route(
                id,
                Some(FormatDescriptor::new("pcm-s16le-48000-mono")),
                QosMode::Interactive,
                1_001,
            )
            .unwrap();
        runtime.begin_route_start(id, 1_001).unwrap();
        assert!(worker.poll(&mut runtime, || at(1_002)).is_err());
        worker.stop(&mut runtime).unwrap();
        assert_eq!(runtime.route(id).unwrap().epoch, 2);
        assert_eq!(runtime.route(id).unwrap().state, RouteState::Starting);
        assert!(runtime.route_port_lease_claim(id, sink).is_some());
    }
    #[test]
    fn route_media_and_cleanup_failures_are_not_success() {
        let (mut runtime, media) = fixture();
        let id = media.route_id;
        let (mut worker, state) = start(&mut runtime, media);
        state.borrow_mut().fail_write = true;
        state.borrow_mut().fail_stop = true;
        let failure = worker.poll(&mut runtime, || at(1_001)).unwrap_err();
        assert_eq!(failure.to_string(), "synthetic cleanup failure");
        assert_eq!(runtime.route(id).unwrap().state, RouteState::Offline);
        assert!(worker.stop(&mut runtime).is_err());
        state.borrow_mut().fail_stop = false;
        worker.stop(&mut runtime).unwrap();
        // Offline invalidates the epoch. Cleanup must not rewrite that newer state.
        assert_eq!(runtime.route(id).unwrap().state, RouteState::Offline);
    }

    #[test]
    fn ipc_dynamic_route_udp_pcm_stop_releases_socket_and_lease() {
        use std::net::UdpSocket;
        let (mut runtime, media) = fixture();
        let id = media.route_id;
        let sink_port = runtime.route(id).unwrap().sink;
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let receiver_addr = receiver.local_addr().unwrap();
        let sender_addr = sender.local_addr().unwrap();
        let endpoint_config = |peer| NativeLanEndpointConfig {
            peer,
            read_timeout: Duration::from_millis(10),
            inflight_packet_capacity: 2,
        };
        let mut sender = NativeLanUdpEndpoint::from_socket(
            sender,
            endpoint_config(receiver_addr),
            media.clone(),
        )
        .unwrap();
        let endpoint = NativeLanUdpEndpoint::from_socket(
            receiver,
            endpoint_config(sender_addr),
            media.clone(),
        )
        .unwrap();
        let resources = NativeResources {
            endpoint: Some(endpoint),
            sink: PipeWirePlayback::synthetic_cat().unwrap(),
        };
        let mut worker = Worker::start(
            &mut runtime,
            media.clone(),
            Duration::from_secs(10),
            || at(1_000),
            || Ok(resources),
        )
        .unwrap();
        let (mut fixture_packet, _) = fake(&media);
        sender
            .send_packet(&fixture_packet.packet.take().unwrap())
            .unwrap();
        for _ in 0..4 {
            worker.poll(&mut runtime, || at(1_001)).unwrap();
            if worker.pump.metrics().packets_written == 1 {
                break;
            }
        }
        assert_eq!(worker.pump.metrics().bytes_written, 960);
        assert_eq!(runtime.route(id).unwrap().state, RouteState::Active);
        worker.stop(&mut runtime).unwrap();
        assert!(worker.resources.sink.check_alive().is_err());
        assert!(worker.resources.endpoint.is_none());
        assert!(UdpSocket::bind(receiver_addr).is_ok());
        assert!(runtime.route_port_lease_claim(id, sink_port).is_none());
        assert_eq!(runtime.route(id).unwrap().state, RouteState::Stopped);
    }

    #[test]
    fn route_wrong_binding_and_finite_lifetime_fail_closed() {
        for case in 0..3 {
            let (mut runtime, mut media) = fixture();
            match case {
                0 => media.stream_epoch += 1,
                1 => media.selected_spec = capyio_audio::AudioStreamSpec::media_balanced(),
                _ => media.session_id = capyio_core::SessionId::new(),
            }
            let (resources, _) = fake(&media);
            let mut opened = false;
            assert!(
                Worker::start(
                    &mut runtime,
                    media,
                    Duration::from_secs(10),
                    || at(1_000),
                    || {
                        opened = true;
                        Ok(resources)
                    }
                )
                .is_err()
            );
            assert!(!opened);
        }
        let (mut runtime, media) = fixture();
        let (resources, state) = fake(&media);
        let mut worker = Worker::start(
            &mut runtime,
            media,
            Duration::from_secs(1),
            || at(1_000),
            || Ok(resources),
        )
        .unwrap();
        assert!(worker.poll(&mut runtime, || at(2_000)).is_err());
        assert_eq!(state.borrow().writes, 0);
        assert_eq!(state.borrow().receives, 0);
    }

    #[test]
    fn route_stop_cleanup_failure_is_stopping_not_active() {
        let (mut runtime, media) = fixture();
        let id = media.route_id;
        let sink = runtime.route(id).unwrap().sink;
        let (mut worker, state) = start(&mut runtime, media);
        state.borrow_mut().fail_stop = true;
        assert!(worker.stop(&mut runtime).is_err());
        assert_eq!(runtime.route(id).unwrap().state, RouteState::Stopping);
        assert!(worker.poll(&mut runtime, || at(1_001)).is_err());
        state.borrow_mut().fail_stop = false;
        worker.stop(&mut runtime).unwrap();
        assert_eq!(runtime.route(id).unwrap().state, RouteState::Stopped);
        assert!(runtime.route_port_lease_claim(id, sink).is_none());
    }

    #[test]
    fn route_runtime_stop_request_cleans_before_more_media() {
        let (mut runtime, media) = fixture();
        let id = media.route_id;
        let (mut worker, state) = start(&mut runtime, media);
        runtime.begin_route_stop(id).unwrap();
        worker.poll(&mut runtime, || at(1_001)).unwrap();
        assert_eq!(runtime.route(id).unwrap().state, RouteState::Stopped);
        assert_eq!(state.borrow().writes, 0);
        assert_eq!(state.borrow().receives, 0);
        assert_eq!(state.borrow().stops, 1);
    }
}
