//! Runtime-owned stereo Source. No capture opens without exact live authority.
use crate::route::{Guard, clock};
use crate::{
    CaptureEndpoint, CapturePump, EndpointMode, PcmCaptureSource, PipeWireCapture, PlaybackConfig,
    PlaybackMetrics,
};
use capyio_audio::{AudioMediaStreamBinding, AudioStreamSpec};
use capyio_core::{Problem, ProblemCategory, ProblemId, ProblemSeverity, RouteState};
use capyio_native_audio_lan::{NativeLanEndpointConfig, NativeLanError, NativeLanUdpEndpoint};
use capyio_runtime::NodeRuntime;
use std::{
    io,
    time::{Duration, Instant},
};

fn error(value: impl ToString) -> io::Error {
    io::Error::other(value.to_string())
}

pub(crate) struct RuntimeCapture {
    guard: Guard,
    pump: CapturePump,
    input: PipeWireCapture,
    endpoint: Option<NativeLanUdpEndpoint>,
    started: Instant,
    stopped: bool,
    packets_sent: u64,
}
impl RuntimeCapture {
    pub(crate) fn open(
        runtime: &mut NodeRuntime,
        media: AudioMediaStreamBinding,
        config: &PlaybackConfig,
    ) -> io::Result<Self> {
        Self::open_with(runtime, media, config, PipeWireCapture::open_checked)
    }
    fn open_with(
        runtime: &mut NodeRuntime,
        media: AudioMediaStreamBinding,
        config: &PlaybackConfig,
        open: impl FnOnce(
            &CaptureEndpoint,
            &mut dyn FnMut() -> io::Result<()>,
        ) -> io::Result<PipeWireCapture>,
    ) -> io::Result<Self> {
        if !config.endpoint_mode.capture()
            || media.selected_spec != AudioStreamSpec::media_balanced()
            || config.duration.subsec_nanos() != 0
        {
            return Err(error("exact stereo capture mode required"));
        }
        PlaybackConfig::parse(&[
            "--trusted-lab".into(),
            config.local.to_string(),
            config.peer.to_string(),
            config.duration.as_secs().to_string(),
            config.target.clone(),
        ])
        .map_err(error)?;
        crate::validate_endpoint_target(config.endpoint_mode, &config.target).map_err(error)?;
        let capture = if config.endpoint_mode == EndpointMode::VirtualSpeaker {
            let route = runtime.route(media.route_id).map_err(error)?;
            if config.target != crate::virtual_speaker_name(route.source.port_id) {
                return Err(error(
                    "virtual speaker identity differs from admitted Source",
                ));
            }
            CaptureEndpoint::VirtualSpeaker(route.source.port_id)
        } else {
            CaptureEndpoint::SelectedSource(config.target.clone())
        };
        let started = Instant::now();
        let mut guard = Guard::new(runtime, media.clone(), clock(started)?.0)?;
        let pump = CapturePump::with_binding(config.duration, media.clone())?;
        // A packet has two fragments, each with a configured 1ms send timeout.
        let opened = (|| {
            let endpoint = NativeLanUdpEndpoint::bind(
                config.local.into(),
                NativeLanEndpointConfig {
                    peer: config.peer.into(),
                    read_timeout: Duration::from_millis(1),
                    inflight_packet_capacity: 2,
                },
                media,
            )
            .map_err(error)?;
            let input = open(&capture, &mut || {
                let (now, elapsed) = clock(started)?;
                guard.check(runtime, now)?;
                if elapsed >= config.duration {
                    return Err(error("capture lifetime elapsed during compatibility probe"));
                }
                Ok(())
            })?;
            Ok::<_, io::Error>((endpoint, input))
        })();
        let (endpoint, input) = match opened {
            Ok(resources) => resources,
            Err(reason) => {
                Self::mark_offline(&guard, runtime);
                return Err(reason);
            }
        };
        let mut worker = Self {
            guard,
            pump,
            input,
            endpoint: Some(endpoint),
            started,
            stopped: false,
            packets_sent: 0,
        };
        let activation = (|| {
            let (now, elapsed) = clock(started)?;
            worker.guard.check(runtime, now)?;
            if elapsed >= config.duration {
                return Err(error("capture lifetime elapsed while opening"));
            }
            worker.input.check_alive()?;
            runtime
                .activate_route(worker.guard.binding.media().route_id)
                .map_err(error)
        })();
        if let Err(reason) = activation {
            Self::mark_offline(&worker.guard, runtime);
            let _ = worker.stop(runtime);
            return Err(reason);
        }
        Ok(worker)
    }
    pub(crate) fn poll(&mut self, runtime: &mut NodeRuntime) -> io::Result<()> {
        if self.stopped {
            return Err(error("capture stopped"));
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
        let result = (|| {
            let (now, elapsed) = clock(self.started)?;
            self.guard.check(runtime, now)?;
            let packet = self.pump.poll(&mut self.input, elapsed)?;
            self.guard.check(runtime, clock(self.started)?.0)?;
            if let Some(packet) = packet {
                let guard = &mut self.guard;
                let started = self.started;
                self.endpoint
                    .as_mut()
                    .ok_or_else(|| error("capture ingress closed"))?
                    .send_packet_checked(&packet, || {
                        let now = clock(started).map_err(NativeLanError::Io)?.0;
                        guard.check(runtime, now).map_err(NativeLanError::Io)?;
                        let route = runtime
                            .route(guard.binding.media().route_id)
                            .map_err(|e| NativeLanError::Io(error(e)))?;
                        let lease = runtime
                            .route_port_lease_claim(route.id, route.source)
                            .ok_or_else(|| NativeLanError::Io(error("capture lease absent")))?;
                        // Reserve finite syscall headroom; never renew the grant.
                        if guard
                            .binding
                            .expires_at_ms()
                            .min(lease.expires_at_ms)
                            .saturating_sub(now)
                            <= 10
                        {
                            return Err(NativeLanError::Io(error(
                                "capture send authority near expiry",
                            )));
                        }
                        Ok(())
                    })
                    .map_err(error)?;
                self.guard.check(runtime, clock(self.started)?.0)?;
                self.packets_sent = self.packets_sent.saturating_add(1);
            }
            Ok(())
        })();
        if let Err(reason) = result {
            Self::mark_offline(&self.guard, runtime);
            let _ = self.stop(runtime);
            return Err(reason);
        }
        Ok(())
    }
    pub(crate) fn stop(&mut self, runtime: &mut NodeRuntime) -> io::Result<()> {
        self.stopped = true;
        self.pump.discard();
        self.endpoint.take();
        let transition: io::Result<()> = (|| {
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
        let cleanup = self.input.stop();
        transition?;
        cleanup?;
        if self.guard.owns_current(runtime) {
            let id = self.guard.binding.media().route_id;
            if runtime.route(id).map_err(error)?.state == RouteState::Stopping {
                runtime.stop_route(id).map_err(error)?;
            }
        }
        Ok(())
    }
    fn mark_offline(guard: &Guard, runtime: &mut NodeRuntime) {
        if guard.owns_current(runtime) {
            let id = guard.binding.media().route_id;
            let _ = runtime.report_route_offline(
                id,
                Problem {
                    id: ProblemId::new(),
                    code: "CAPY.LINUX.CAPTURE_STOPPED".into(),
                    category: ProblemCategory::Adapter,
                    severity: ProblemSeverity::Error,
                    retryable: true,
                    related_node: None,
                    related_adapter: None,
                    related_route: Some(id),
                    human_message: "Linux capture stopped; fresh admission required".into(),
                    technical_detail: None,
                },
            );
        }
    }
    pub(crate) fn metrics(&self) -> PlaybackMetrics {
        PlaybackMetrics {
            packets_sent: self.packets_sent,
            ..PlaybackMetrics::default()
        }
    }
}
impl Drop for RuntimeCapture {
    fn drop(&mut self) {
        self.pump.discard();
        self.endpoint.take();
        let _ = self.input.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket;
    fn admitted() -> (
        NodeRuntime,
        AudioMediaStreamBinding,
        PlaybackConfig,
        UdpSocket,
    ) {
        let (mut runtime, media) = crate::route::tests::fixture_for(true);
        let now = clock(Instant::now()).unwrap().0;
        runtime
            .renew_route_authorization(media.route_id, now + 60_000, 1_001)
            .unwrap();
        let source = runtime.route(media.route_id).unwrap().source;
        runtime
            .acquire_route_port_lease(media.route_id, source, now + 30_000, now)
            .unwrap();
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let reservation = UdpSocket::bind("127.0.0.1:0").unwrap();
        let local = reservation
            .local_addr()
            .unwrap()
            .to_string()
            .parse()
            .unwrap();
        drop(reservation);
        let config = PlaybackConfig {
            local,
            peer: receiver.local_addr().unwrap().to_string().parse().unwrap(),
            duration: Duration::from_secs(10),
            target: "synthetic.selected.capture".into(),
            endpoint_mode: EndpointMode::StereoCapture,
        };
        (runtime, media, config, receiver)
    }
    #[test]
    fn ipc_capture_runtime_stop_discards_resources_without_offline_failure() {
        let (mut runtime, media, config, _receiver) = admitted();
        let mut worker = RuntimeCapture::open_with(&mut runtime, media.clone(), &config, |_, _| {
            PipeWireCapture::synthetic_pcm()
        })
        .unwrap();
        runtime.begin_route_stop(media.route_id).unwrap();
        worker.poll(&mut runtime).unwrap();
        assert_eq!(
            runtime.route(media.route_id).unwrap().state,
            RouteState::Stopped
        );
        assert!(worker.endpoint.is_none());
        assert_eq!(worker.packets_sent, 0);
        assert!(worker.poll(&mut runtime).is_err());
        worker.stop(&mut runtime).unwrap();
        assert!(UdpSocket::bind(config.local).is_ok());
    }
    #[test]
    fn ipc_capture_lost_lease_prevents_read_send_and_releases_udp() {
        let (mut runtime, media, config, _receiver) = admitted();
        let mut worker = RuntimeCapture::open_with(&mut runtime, media.clone(), &config, |_, _| {
            PipeWireCapture::synthetic_pcm()
        })
        .unwrap();
        let source = runtime.route(media.route_id).unwrap().source;
        runtime
            .release_route_port_lease(media.route_id, source, media.session_id)
            .unwrap();
        assert!(worker.poll(&mut runtime).is_err());
        assert_eq!(worker.pump.metrics().bytes_read, 0);
        assert_eq!(worker.packets_sent, 0);
        assert!(worker.endpoint.is_none());
        assert!(UdpSocket::bind(config.local).is_ok());
    }

    fn assert_failed_open_released(
        runtime: &NodeRuntime,
        media: &AudioMediaStreamBinding,
        config: &PlaybackConfig,
    ) {
        let route = runtime.route(media.route_id).unwrap();
        assert_eq!(route.state, RouteState::Offline);
        assert!(
            runtime
                .route_port_lease_claim(route.id, route.source)
                .is_none()
        );
        assert!(runtime.snapshot().problems.iter().any(|problem| {
            problem.code == "CAPY.LINUX.CAPTURE_STOPPED" && problem.related_route == Some(route.id)
        }));
        assert!(UdpSocket::bind(config.local).is_ok());
    }

    #[test]
    fn ipc_capture_source_open_failure_releases_udp_and_source_lease() {
        let (mut runtime, media, config, _receiver) = admitted();
        let mut opened = false;
        let result = RuntimeCapture::open_with(&mut runtime, media.clone(), &config, |_, _| {
            opened = true;
            Err(error("synthetic source-open failure"))
        });
        let failure = result.err().expect("source open unexpectedly succeeded");
        assert_eq!(failure.to_string(), "synthetic source-open failure");
        assert!(opened);
        assert_failed_open_released(&runtime, &media, &config);
    }

    #[test]
    fn ipc_capture_exited_source_cannot_activate_and_releases_resources() {
        let (mut runtime, media, config, _receiver) = admitted();
        let result = RuntimeCapture::open_with(&mut runtime, media.clone(), &config, |_, _| {
            let mut source = PipeWireCapture::synthetic_pcm()?;
            source.stop()?;
            Ok(source)
        });
        let failure = result.err().expect("stopped source unexpectedly activated");
        assert_eq!(failure.kind(), io::ErrorKind::BrokenPipe);
        assert_failed_open_released(&runtime, &media, &config);
    }

    #[test]
    fn ipc_capture_lifetime_elapsed_during_open_cannot_activate() {
        let (mut runtime, media, mut config, _receiver) = admitted();
        config.duration = Duration::from_secs(1);
        let result = RuntimeCapture::open_with(&mut runtime, media.clone(), &config, |_, _| {
            // Exercise the actual nonrenewable opening deadline, not a fake
            // Runtime, clock or permission check. Hosted execution only.
            std::thread::sleep(Duration::from_millis(1_100));
            PipeWireCapture::synthetic_pcm()
        });
        let failure = result
            .err()
            .expect("expired opening unexpectedly activated");
        assert_eq!(
            failure.to_string(),
            "capture lifetime elapsed while opening"
        );
        assert_failed_open_released(&runtime, &media, &config);
    }

    #[test]
    fn ipc_capture_lifetime_session_or_grant_change_stops_before_read_or_send() {
        for case in 0..3 {
            let (mut runtime, media, config, _receiver) = admitted();
            let mut worker =
                RuntimeCapture::open_with(&mut runtime, media.clone(), &config, |_, _| {
                    PipeWireCapture::synthetic_pcm()
                })
                .unwrap();
            match case {
                0 => {
                    // Only the worker's monotonic deadline is moved; real domain
                    // grant/lease checks still run against the original Runtime.
                    worker.started = Instant::now() - config.duration;
                }
                1 => runtime.close_session(media.session_id).unwrap(),
                _ => {
                    let now = clock(Instant::now()).unwrap().0;
                    runtime
                        .renew_route_authorization(media.route_id, now + 90_000, now)
                        .unwrap();
                }
            }
            assert!(worker.poll(&mut runtime).is_err());
            assert_eq!(worker.pump.metrics().bytes_read, 0);
            assert_eq!(worker.packets_sent, 0);
            assert!(worker.endpoint.is_none());
            assert!(worker.input.check_alive().is_err());
            let route = runtime.route(media.route_id).unwrap();
            assert_eq!(route.state, RouteState::Offline);
            assert!(
                runtime
                    .route_port_lease_claim(route.id, route.source)
                    .is_none()
            );
            assert!(UdpSocket::bind(config.local).is_ok());
            worker.stop(&mut runtime).unwrap();
        }
    }

    #[test]
    fn ipc_capture_probe_delay_rechecks_authority_before_opening_source() {
        let (mut runtime, media, mut config, _receiver) = admitted();
        config.duration = Duration::from_secs(1);
        let mut opened = false;
        let result =
            RuntimeCapture::open_with(&mut runtime, media.clone(), &config, |_, revalidate| {
                std::thread::sleep(Duration::from_millis(1100));
                revalidate()?;
                opened = true;
                PipeWireCapture::synthetic_pcm()
            });
        assert!(result.is_err());
        assert!(!opened, "expired probe must not open capture");
        assert_failed_open_released(&runtime, &media, &config);
    }

    #[test]
    fn ipc_capture_drop_releases_udp_without_claiming_runtime_transition() {
        let (mut runtime, media, config, _receiver) = admitted();
        let worker = RuntimeCapture::open_with(&mut runtime, media.clone(), &config, |_, _| {
            PipeWireCapture::synthetic_pcm()
        })
        .unwrap();
        assert_eq!(
            runtime.route(media.route_id).unwrap().state,
            RouteState::Active
        );
        drop(worker);
        assert!(UdpSocket::bind(config.local).is_ok());
        // Drop owns resources, not this separately-owned Runtime. The host must
        // still perform explicit lifecycle cleanup; no implicit lease mutation.
        let route = runtime.route(media.route_id).unwrap();
        assert_eq!(route.state, RouteState::Active);
        assert!(
            runtime
                .route_port_lease_claim(route.id, route.source)
                .is_some()
        );
        runtime.begin_route_stop(media.route_id).unwrap();
        runtime.stop_route(media.route_id).unwrap();
    }
}
