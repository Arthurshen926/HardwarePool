use super::*;
use capyio_core::{InteroperabilityMode, Platform, QosMode, RouteBackend};
use capyio_runtime::PortLeasePolicy;
use capyio_video::VideoFrameFlags;
use std::sync::{Arc, Mutex};

fn fixture() -> (NodeRuntime, CameraBinding) {
    fixture_format(false)
}
fn fixture_format(encoded_avc: bool) -> (NodeRuntime, CameraBinding) {
    let format = if encoded_avc {
        capyio_core::FormatDescriptor::new("cavc-avc-access-unit-v1")
    } else {
        VideoStreamSpec::camera_720p30_nv12().format_descriptor()
    };
    fixture_descriptor(format)
}
fn fixture_descriptor(format: capyio_core::FormatDescriptor) -> (NodeRuntime, CameraBinding) {
    let lab = capyio_testkit::DemoLab::new().unwrap();
    let original = lab
        .runtime
        .route(lab.routes.phone_camera_to_panel)
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
            cap.availability = capyio_core::Availability::Available;
            for port in cap.ports.values_mut() {
                port.availability = capyio_core::Availability::Available;
                port.interoperability_mode = InteroperabilityMode::AdapterManaged;
                if port.profile == capyio_video::video_frames_profile() {
                    port.formats = vec![format.clone()];
                    port.qos_modes.insert(QosMode::Interactive);
                }
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
        .prepare_route(route_id, Some(format), QosMode::Interactive, 1_000)
        .unwrap();
    runtime.begin_route_start(route_id, 1_000).unwrap();
    (
        runtime,
        CameraBinding {
            session_id,
            route_id,
            stream_id: StreamId::new(),
            stream_epoch: 1,
            local_sink: original.sink,
        },
    )
}
#[derive(Default)]
struct State {
    frames: Vec<VideoFrameDescriptor>,
    stop: usize,
    full: bool,
    failed: bool,
}
struct Fake(Arc<Mutex<State>>);
impl Projection for Fake {
    fn submit(&mut self, frame: Frame) -> io::Result<SubmitOutcome> {
        assert_eq!(frame.payload.len(), FRAME_BYTES);
        let mut state = self.0.lock().unwrap();
        if state.full {
            return Ok(SubmitOutcome::DroppedBackpressure);
        }
        state.frames.push(frame.descriptor);
        Ok(SubmitOutcome::Queued)
    }
    fn check(&mut self) -> io::Result<()> {
        if self.0.lock().unwrap().failed {
            Err(error("crash"))
        } else {
            Ok(())
        }
    }
    fn stop(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().stop += 1;
        Ok(())
    }
}
fn owner() -> (RuntimeCamera, Arc<Mutex<State>>, CameraBinding) {
    let (runtime, binding) = fixture();
    let state = Arc::new(Mutex::new(State::default()));
    let handle = state.clone();
    (
        RuntimeCamera::open(
            runtime,
            binding.clone(),
            Duration::from_secs(4),
            1_000,
            |_, _| Ok(Box::new(Fake(handle))),
        )
        .unwrap(),
        state,
        binding,
    )
}
fn descriptor(binding: &CameraBinding, sequence: u64) -> VideoFrameDescriptor {
    VideoFrameDescriptor {
        stream_id: binding.stream_id,
        stream_epoch: binding.stream_epoch,
        sequence,
        source_timestamp_nanos: sequence * FRAME_DURATION_NS,
        duration_nanos: FRAME_DURATION_NS,
        payload_bytes: FRAME_BYTES as u64,
        flags: VideoFrameFlags::default(),
    }
}
#[test]
fn requires_live_exact_runtime_before_resource_open() {
    for change in 0..5 {
        let (mut runtime, mut binding) = fixture();
        match change {
            0 => binding.stream_epoch += 1,
            1 => binding.stream_id = "00000000-0000-0000-0000-000000000000".parse().unwrap(),
            2 => {
                runtime
                    .release_route_port_lease(
                        binding.route_id,
                        binding.local_sink,
                        binding.session_id,
                    )
                    .unwrap();
            }
            3 => {
                runtime.close_session(binding.session_id).unwrap();
            }
            _ => binding.local_sink = runtime.route(binding.route_id).unwrap().source,
        }
        let result =
            RuntimeCamera::open(runtime, binding, Duration::from_secs(1), 1_000, |_, _| {
                panic!("must validate before spawn")
            });
        assert!(result.is_err());
    }
}
#[test]
fn accepted_frame_is_not_delivery_and_backpressure_marks_discontinuity() {
    let (mut owner, state, binding) = owner();
    assert_eq!(
        owner
            .submit_at(descriptor(&binding, 1), vec![0; FRAME_BYTES], 1000)
            .unwrap(),
        SubmitOutcome::Queued
    );
    assert_eq!(
        owner.runtime().route(binding.route_id).unwrap().state,
        RouteState::Starting
    );
    state.lock().unwrap().full = true;
    assert_eq!(
        owner
            .submit_at(descriptor(&binding, 2), vec![0; FRAME_BYTES], 1000)
            .unwrap(),
        SubmitOutcome::DroppedBackpressure
    );
    state.lock().unwrap().full = false;
    owner
        .submit_at(descriptor(&binding, 3), vec![0; FRAME_BYTES], 1000)
        .unwrap();
    assert!(state.lock().unwrap().frames[1].flags.discontinuity);
    assert_eq!(
        owner.metrics(),
        CameraMetrics {
            queued: 2,
            dropped: 1
        }
    );
    owner.stop().unwrap();
    assert_eq!(
        owner.runtime().route(binding.route_id).unwrap().state,
        RouteState::Stopped
    );
}
#[test]
fn malformed_identity_timing_and_payload_are_terminal() {
    for change in 0..8 {
        let (mut owner, state, binding) = owner();
        owner
            .submit_at(descriptor(&binding, 1), vec![0; FRAME_BYTES], 1000)
            .unwrap();
        let mut frame = descriptor(&binding, 2);
        let mut payload = vec![0; FRAME_BYTES];
        match change {
            0 => frame.stream_id = StreamId::new(),
            1 => frame.stream_epoch += 1,
            2 => frame.sequence = 1,
            3 => frame.source_timestamp_nanos = 1,
            4 => frame.payload_bytes -= 1,
            5 => {
                payload.pop();
            }
            6 => frame.flags.end_of_stream = true,
            _ => frame.duration_nanos += 1,
        }
        assert!(owner.submit_at(frame, payload, 1000).is_err());
        assert_eq!(state.lock().unwrap().frames.len(), 1);
        assert!(owner.stopped);
    }
}
#[test]
fn lease_expiry_regression_revocation_and_child_crash_stop() {
    for change in 0..4 {
        let (mut owner, state, binding) = owner();
        let now = match change {
            0 => 5000,
            1 => 999,
            2 => {
                owner
                    .runtime_mut()
                    .release_route_port_lease(
                        binding.route_id,
                        binding.local_sink,
                        binding.session_id,
                    )
                    .unwrap();
                1000
            }
            _ => {
                state.lock().unwrap().failed = true;
                1000
            }
        };
        assert!(
            owner
                .submit_at(descriptor(&binding, 1), vec![0; FRAME_BYTES], now)
                .is_err()
        );
        assert!(state.lock().unwrap().frames.is_empty());
        assert!(owner.stopped);
    }
}

#[test]
fn stale_owner_does_not_stop_restarted_route_epoch() {
    let (mut owner, _, binding) = owner();
    owner
        .runtime_mut()
        .begin_route_stop(binding.route_id)
        .unwrap();
    owner.runtime_mut().stop_route(binding.route_id).unwrap();
    owner
        .runtime_mut()
        .acquire_route_port_lease(binding.route_id, binding.local_sink, 5000, 1000)
        .unwrap();
    owner
        .runtime_mut()
        .prepare_route(
            binding.route_id,
            Some(VideoStreamSpec::camera_720p30_nv12().format_descriptor()),
            QosMode::Interactive,
            1000,
        )
        .unwrap();
    owner
        .runtime_mut()
        .begin_route_start(binding.route_id, 1000)
        .unwrap();
    assert!(
        owner
            .submit_at(descriptor(&binding, 1), vec![0; FRAME_BYTES], 1000)
            .is_err()
    );
    assert_eq!(owner.runtime().route(binding.route_id).unwrap().epoch, 2);
    assert_eq!(
        owner.runtime().route(binding.route_id).unwrap().state,
        RouteState::Starting
    );
    assert!(
        owner
            .runtime()
            .route_port_lease_claim(binding.route_id, binding.local_sink)
            .is_some()
    );
}

#[test]
fn changed_grant_is_not_silent_extension_and_expiry_during_open_closes() {
    let (mut owner, _, binding) = owner();
    owner
        .runtime_mut()
        .renew_route_authorization(binding.route_id, 20000, 1000)
        .unwrap();
    assert!(
        owner
            .submit_at(descriptor(&binding, 1), vec![0; FRAME_BYTES], 1000)
            .is_err()
    );
    let (runtime, binding) = fixture();
    let state = Arc::new(Mutex::new(State::default()));
    let handle = state.clone();
    assert!(
        RuntimeCamera::open(runtime, binding, Duration::from_millis(1), 1000, |_, _| {
            std::thread::sleep(Duration::from_millis(5));
            Ok(Box::new(Fake(handle)))
        })
        .is_err()
    );
    assert!(state.lock().unwrap().stop > 0);
}

#[test]
fn encoded_projection_requires_explicit_boundary_and_preserves_route_format() {
    let (runtime, binding) = fixture_format(true);
    let state = Arc::new(Mutex::new(State::default()));
    assert!(
        RuntimeCamera::open(
            runtime,
            binding,
            Duration::from_secs(1),
            1000,
            |_, _| panic!("must not open CAVC via NV12 entry")
        )
        .is_err()
    );
    let (runtime, binding) = fixture_format(true);
    assert!(validate_decoded_avc_authority(&runtime, binding.clone(), 1000).is_ok());
    let handle = state.clone();
    let mut owner = RuntimeCamera::open_format(
        runtime,
        binding.clone(),
        Duration::from_secs(1),
        1000,
        guard::CameraFormat::LegacyAvc,
        |_, _| Ok(Box::new(Fake(handle))),
    )
    .unwrap();
    assert_eq!(
        owner
            .runtime()
            .route(binding.route_id)
            .unwrap()
            .selected_format,
        Some(capyio_core::FormatDescriptor::new(
            "cavc-avc-access-unit-v1"
        ))
    );
    assert_eq!(
        owner
            .submit_at(descriptor(&binding, 1), vec![0; FRAME_BYTES], 1000)
            .unwrap(),
        SubmitOutcome::Queued
    );
    let (runtime, binding) = fixture();
    assert!(validate_decoded_avc_authority(&runtime, binding, 1000).is_err());
}

#[test]
fn encoded_owned_source_may_publish_busy_but_not_unavailable() {
    let (mut runtime, binding) = fixture_format(true);
    let source = runtime.route(binding.route_id).unwrap().source;
    let mut peer = runtime
        .snapshot()
        .peers
        .into_iter()
        .find(|n| n.id == source.node_id)
        .unwrap();
    let cap = peer.capabilities.get_mut(&source.capability_id).unwrap();
    cap.availability = capyio_core::Availability::Busy;
    cap.ports.get_mut(&source.port_id).unwrap().availability = capyio_core::Availability::Busy;
    runtime
        .accept_peer_catalog(binding.session_id, peer.clone())
        .unwrap();
    assert!(validate_decoded_avc_authority(&runtime, binding.clone(), 1000).is_ok());
    peer.capabilities
        .get_mut(&source.capability_id)
        .unwrap()
        .availability = capyio_core::Availability::Offline;
    runtime
        .accept_peer_catalog(binding.session_id, peer)
        .unwrap();
    assert!(validate_decoded_avc_authority(&runtime, binding, 1000).is_err());
}

#[test]
fn finite_projection_is_explicit_and_pins_its_exact_format() {
    let finite = capyio_core::FormatDescriptor::new("cavc-avc-finite-720p30-v1");
    let (runtime, binding) = fixture_descriptor(finite.clone());
    assert!(validate_decoded_avc_authority(&runtime, binding.clone(), 1000).is_err());
    assert!(validate_decoded_avc_finite_authority(&runtime, binding.clone(), 1000).is_ok());
    assert!(
        RuntimeCamera::open_format(
            runtime,
            binding,
            Duration::from_secs(1),
            1000,
            guard::CameraFormat::LegacyAvc,
            |_, _| panic!("finite format must not open via the legacy boundary"),
        )
        .is_err()
    );
    let (runtime, binding) = fixture_format(true);
    assert!(validate_decoded_avc_finite_authority(&runtime, binding, 1000).is_err());

    let (runtime, binding) = fixture_descriptor(finite.clone());
    let state = Arc::new(Mutex::new(State::default()));
    let handle = state.clone();
    let mut owner = RuntimeCamera::open_format(
        runtime,
        binding.clone(),
        Duration::from_secs(1),
        1000,
        guard::CameraFormat::FiniteAvc,
        |_, _| Ok(Box::new(Fake(handle))),
    )
    .unwrap();
    owner
        .submit_at(descriptor(&binding, 1), vec![0; FRAME_BYTES], 1000)
        .unwrap();
    assert_eq!(
        owner
            .runtime()
            .route(binding.route_id)
            .unwrap()
            .selected_format,
        Some(finite),
    );
    // Replacing the advertised Source with an old legacy-only catalog cannot
    // silently downgrade an already admitted finite owner.
    let source = owner.runtime().route(binding.route_id).unwrap().source;
    let mut peer = owner
        .runtime()
        .snapshot()
        .peers
        .into_iter()
        .find(|node| node.id == source.node_id)
        .unwrap();
    peer.capabilities
        .get_mut(&source.capability_id)
        .unwrap()
        .ports
        .get_mut(&source.port_id)
        .unwrap()
        .formats = vec![capyio_core::FormatDescriptor::new(
        "cavc-avc-access-unit-v1",
    )];
    owner
        .runtime_mut()
        .accept_peer_catalog(binding.session_id, peer)
        .unwrap();
    assert!(owner.check_at(1000).is_err());
    assert!(owner.stopped);
    assert_eq!(state.lock().unwrap().frames.len(), 1);
}

#[test]
fn finite_projection_rejects_format_parameters_before_open() {
    for key in ["max_frames", "unknown"] {
        let mut format = capyio_core::FormatDescriptor::new("cavc-avc-finite-720p30-v1");
        format.parameters.insert(key.into(), "1".into());
        let (runtime, binding) = fixture_descriptor(format);
        assert!(
            RuntimeCamera::open_format(
                runtime,
                binding,
                Duration::from_secs(1),
                1000,
                guard::CameraFormat::FiniteAvc,
                |_, _| panic!("unknown parameters must not open a helper"),
            )
            .is_err()
        );
    }
}

#[test]
fn finite_projection_keeps_exact_duration_and_rejects_elapsed_authority() {
    let (runtime, binding) = fixture_descriptor(capyio_core::FormatDescriptor::new(
        "cavc-avc-finite-720p30-v1",
    ));
    let duration = Duration::from_micros(34_500);
    let state = Arc::new(Mutex::new(State::default()));
    let handle = state.clone();
    let mut owner = RuntimeCamera::open_format(
        runtime,
        binding,
        duration,
        1000,
        guard::CameraFormat::FiniteAvc,
        |lifetime, deadline| {
            assert_eq!(lifetime, duration);
            assert_eq!(deadline, 1034); // no upward rounding to a second frame
            Ok(Box::new(Fake(handle)))
        },
    )
    .unwrap();
    assert_eq!(owner.lifetime, duration);
    owner.started = Instant::now() - duration;
    assert!(owner.check_at(1000).is_err());
    assert!(owner.stopped);
    assert!(state.lock().unwrap().frames.is_empty());
}

#[test]
fn finite_projection_rejects_out_of_profile_duration_before_open() {
    for duration in [
        Duration::ZERO,
        Duration::from_micros(999),
        Duration::from_secs(61),
    ] {
        let (runtime, binding) = fixture_descriptor(capyio_core::FormatDescriptor::new(
            "cavc-avc-finite-720p30-v1",
        ));
        assert!(
            RuntimeCamera::open_format(
                runtime,
                binding,
                duration,
                1000,
                guard::CameraFormat::FiniteAvc,
                |_, _| panic!("invalid finite lifetime must not open a helper"),
            )
            .is_err()
        );
    }
}
