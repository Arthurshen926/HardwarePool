use capyio_audio::{AudioMediaPacket, AudioMediaStreamBinding, AudioStreamSpec};
use capyio_core::*;
use capyio_native_audio_lan::*;
use std::{collections::BTreeSet, net::UdpSocket, time::Duration};

fn context() -> (AudioMediaStreamBinding, Route, Session) {
    let session = Session::new(NodeId::new(), NodeId::new());
    let endpoint = |node_id| PortRef {
        node_id,
        capability_id: CapabilityId::new(),
        port_id: PortId::new(),
    };
    let route = Route {
        id: RouteId::new(),
        session_id: session.id,
        source: endpoint(session.local_node_id),
        sink: endpoint(session.remote_node_id),
        profile: ProfileId::audio_frames_v1(),
        backend: RouteBackend::AdapterManaged,
        compatible_formats: vec![],
        compatible_qos_modes: BTreeSet::new(),
        selected_format: None,
        selected_qos_mode: None,
        state: RouteState::Active,
        authorization: AuthorizationState::Authorized {
            expires_at_ms: Some(2000),
        },
        epoch: 19,
        diagnostic_ids: vec![],
    };
    let media = AudioMediaStreamBinding {
        session_id: session.id,
        route_id: route.id,
        stream_id: StreamId::new(),
        stream_epoch: 19,
        selected_spec: AudioStreamSpec::voice_interactive(),
    };
    (media, route, session)
}

#[test]
fn speaker_dynamic_worker_uses_stereo_and_retains_finite_deadline() {
    let (mut media, route, session) = context();
    media.selected_spec = AudioStreamSpec::media_balanced();
    let binding =
        NativeLanRouteBinding::from_live_context(media.clone(), &route, &session, 1).unwrap();
    assert!(
        NativeMicrophoneLaunch::route_arguments(
            "127.0.0.1:1".parse().unwrap(),
            "127.0.0.1:2".parse().unwrap(),
            &binding
        )
        .is_err()
    );
    let args = NativeMicrophoneLaunch::speaker_route_arguments(
        "127.0.0.1:1".parse().unwrap(),
        "127.0.0.1:2".parse().unwrap(),
        &binding,
    )
    .unwrap();
    let launch = NativeMicrophoneLaunch::parse_speaker(&args, 1).unwrap();
    assert_eq!(launch.binding, media);
    assert_eq!(launch.expires_at_ms, Some(2000));
    assert!(NativeMicrophoneLaunch::parse_speaker(&args, 2000).is_err());
    let now = std::time::Instant::now();
    let deadline = launch.deadline(1, now).unwrap();
    assert!(!launch.is_live(deadline, 1, now + Duration::from_millis(2000)));
}

#[test]
fn handoff_rejects_changed_grants_epochs_and_unsupported_contracts() {
    let (media, mut route, session) = context();
    let handoff =
        NativeLanRouteBinding::from_live_context(media.clone(), &route, &session, 1).unwrap();
    handoff
        .require_live_context(&route, &session, 1999)
        .unwrap();
    assert_eq!(handoff.expires_at_ms(), 2000);
    assert!(
        handoff
            .require_live_context(&route, &session, 2000)
            .is_err()
    );
    route.authorization = AuthorizationState::Authorized {
        expires_at_ms: Some(3000),
    };
    assert!(handoff.require_live_context(&route, &session, 1).is_err());
    route.authorization = AuthorizationState::Authorized {
        expires_at_ms: Some(2000),
    };
    route.backend = RouteBackend::CapyDataPlane;
    assert!(NativeLanRouteBinding::from_live_context(media.clone(), &route, &session, 1).is_err());
    route.backend = RouteBackend::AdapterManaged;
    route.epoch = i32::MAX as u64 + 1;
    let mut overflow = media.clone();
    overflow.stream_epoch = route.epoch as u32;
    assert!(NativeLanRouteBinding::from_live_context(overflow, &route, &session, 1).is_err());
    route.epoch = 19;
    route.authorization = AuthorizationState::Authorized {
        expires_at_ms: Some(9_007_199_254_740_992),
    };
    assert!(NativeLanRouteBinding::from_live_context(media, &route, &session, 1).is_err());
}

#[test]
fn real_udp_roundtrip_uses_dynamic_ids_not_lab_ids() {
    let (media, route, session) = context();
    let handoff = NativeLanRouteBinding::from_live_context(media, &route, &session, 1).unwrap();
    let left = UdpSocket::bind("127.0.0.1:0").unwrap();
    let right = UdpSocket::bind("127.0.0.1:0").unwrap();
    let left_addr = left.local_addr().unwrap();
    let right_addr = right.local_addr().unwrap();
    let config = |peer| NativeLanEndpointConfig {
        peer,
        read_timeout: Duration::from_millis(100),
        inflight_packet_capacity: 2,
    };
    let mut sender =
        NativeLanUdpEndpoint::from_socket(left, config(right_addr), handoff.media().clone())
            .unwrap();
    let args = NativeMicrophoneLaunch::route_arguments(
        right_addr.to_string().parse().unwrap(),
        left_addr.to_string().parse().unwrap(),
        &handoff,
    )
    .unwrap();
    let launch = NativeMicrophoneLaunch::parse(&args, 1).unwrap();
    let mut receiver =
        NativeLanUdpEndpoint::from_socket(right, config(left_addr), launch.binding).unwrap();
    handoff.require_live_context(&route, &session, 2).unwrap();
    let packet = AudioMediaPacket {
        stream_id: handoff.media().stream_id,
        stream_epoch: 19,
        sequence: 0,
        source_timestamp_micros: 0,
        first_sample_index: 0,
        sample_count: 480,
        discontinuity: true,
        payload: vec![0; 960],
    };
    sender.send_packet(&packet).unwrap();
    assert_eq!(
        receiver.receive().unwrap(),
        NativeLanReceiveOutcome::Packet(packet)
    );
    let mut stopped = route;
    stopped.state = RouteState::Stopped;
    assert!(handoff.require_live_context(&stopped, &session, 3).is_err());
}

#[test]
fn only_exact_android_pcm_baselines_are_handed_off() {
    let (mut media, route, session) = context();
    media.selected_spec = AudioStreamSpec::media_balanced();
    let speaker =
        NativeLanRouteBinding::from_live_context(media.clone(), &route, &session, 1).unwrap();
    assert_eq!(
        speaker.media().selected_spec,
        AudioStreamSpec::media_balanced()
    );
    media.selected_spec.format.sample_rate_hz = 44_100;
    assert!(NativeLanRouteBinding::from_live_context(media, &route, &session, 1).is_err());
}

#[test]
fn microphone_process_arguments_preserve_dynamic_binding_and_bound_lifetime() {
    let (media, route, session) = context();
    let handoff =
        NativeLanRouteBinding::from_live_context(media.clone(), &route, &session, 1000).unwrap();
    let args = NativeMicrophoneLaunch::route_arguments(
        "127.0.0.1:40211".parse().unwrap(),
        "127.0.0.1:40210".parse().unwrap(),
        &handoff,
    )
    .unwrap();
    let launch = NativeMicrophoneLaunch::parse(&args, 1000).unwrap();
    assert_eq!(launch.binding, media);
    assert_eq!(launch.expires_at_ms, Some(2000));
    assert_eq!(launch.lab_duration, None);
    let started = std::time::Instant::now();
    let deadline = launch.deadline(1000, started).unwrap();
    assert!(launch.is_live(deadline, 1999, started + Duration::from_millis(999)));
    assert!(!launch.is_live(deadline, 2000, started));
    // A wall clock correction cannot extend the captured monotonic duration.
    assert!(!launch.is_live(deadline, 500, started + Duration::from_millis(1000)));
    assert!(launch.deadline(2000, started).is_err());
}

#[test]
fn microphone_process_rejects_partial_expired_or_changed_contracts_without_lab_fallback() {
    let (media, route, session) = context();
    let handoff = NativeLanRouteBinding::from_live_context(media, &route, &session, 1).unwrap();
    let args = NativeMicrophoneLaunch::route_arguments(
        "127.0.0.1:40211".parse().unwrap(),
        "127.0.0.1:40210".parse().unwrap(),
        &handoff,
    )
    .unwrap();
    for length in 3..8 {
        assert!(NativeMicrophoneLaunch::parse(&args[..length], 1).is_err());
    }
    for (index, bad) in [
        (0, "0.0.0.0:40211"),
        (1, "localhost:40210"),
        (1, "127.0.0.1:40211"),
        (2, "--route-v2"),
        (3, "00000000-0000-0000-0000-000000000000"),
        (4, "bad-id"),
        (5, "00000000-0000-0000-0000-000000000000"),
        (6, "0"),
        (6, "2147483648"),
        (7, "9007199254740992"),
        (7, "0"),
    ] {
        let mut invalid = args.clone();
        invalid[index] = bad.into();
        assert!(
            NativeMicrophoneLaunch::parse(&invalid, 1).is_err(),
            "index={index}, value={bad}"
        );
    }
    assert!(NativeMicrophoneLaunch::parse(&args, 2000).is_err());
    let legacy = NativeMicrophoneLaunch::parse(&args[..2], 1).unwrap();
    assert_eq!(legacy.binding, microphone_lab_binding());
    assert_eq!(legacy.expires_at_ms, None);
    let mut extra = args.clone();
    extra.push("extra".into());
    assert!(NativeMicrophoneLaunch::parse(&extra, 1).is_err());
}

#[test]
fn supervisor_rejects_stale_and_stereo_context_before_spawning() {
    let (mut media, mut route, session) = context();
    let handoff =
        NativeLanRouteBinding::from_live_context(media.clone(), &route, &session, 1).unwrap();
    let mut supervisor = NativeMicrophoneSupervisor::new(
        "nonexistent-test-helper",
        "127.0.0.1:40211".parse().unwrap(),
        "127.0.0.1:40210".parse().unwrap(),
        Default::default(),
    )
    .unwrap();
    route.state = RouteState::Stopped;
    assert!(matches!(
        supervisor.start_route(&handoff, &route, &session, 1),
        Err(NativeSpeakerSupervisorError::InvalidRouteBinding)
    ));
    route.state = RouteState::Active;
    media.selected_spec = AudioStreamSpec::media_balanced();
    let stereo = NativeLanRouteBinding::from_live_context(media, &route, &session, 1).unwrap();
    assert!(matches!(
        supervisor.start_route(&stereo, &route, &session, 1),
        Err(NativeSpeakerSupervisorError::InvalidRouteBinding)
    ));
    assert!(matches!(
        supervisor.start_route(&handoff, &route, &session, 2000),
        Err(NativeSpeakerSupervisorError::InvalidRouteBinding)
    ));
    assert_eq!(
        supervisor.status().unwrap(),
        NativeSpeakerSupervisorStatus::Stopped
    );
}

#[cfg(windows)]
#[test]
fn actual_microphone_executable_rejects_incomplete_dynamic_arguments_before_hardware() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_capyio-native-virtual-microphone"))
        .args(["127.0.0.1:40211", "127.0.0.1:40210", "--route-v1"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("incomplete or unknown microphone worker arguments")
    );
}
