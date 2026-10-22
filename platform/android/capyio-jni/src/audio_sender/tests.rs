use super::*;
use capyio_control_tls::ControlTlsBinding;

fn pending() -> (NodeRuntime, RouteId) {
    let mut android = capyio_testkit::android_node();
    let mut sink = capyio_testkit::windows_node();
    for node in [&mut android, &mut sink] {
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
    port.formats = vec![FormatDescriptor::new(SECURE_AUDIO_MONO_FORMAT)];
    port.qos_modes.insert(QosMode::Interactive);
    let source = PortRef {
        node_id: android.id,
        capability_id: cap.id,
        port_id: port.id,
    };
    let sink_cap = sink
        .capabilities
        .values_mut()
        .find(|c| c.class == CapabilityClass::Speaker)
        .unwrap();
    let sink_port = sink_cap.ports.values_mut().next().unwrap();
    sink_port.direction = PortDirection::Sink;
    sink_port.profile = port.profile.clone();
    sink_port.formats = port.formats.clone();
    sink_port.qos_modes = port.qos_modes.clone();
    let destination = PortRef {
        node_id: sink.id,
        capability_id: sink_cap.id,
        port_id: sink_port.id,
    };
    let mut runtime = NodeRuntime::new(android).unwrap();
    runtime.register_peer(sink, true).unwrap();
    let session = runtime.open_session(destination.node_id).unwrap();
    let id = runtime
        .create_route(session, source, destination, RouteBackend::AdapterManaged)
        .unwrap();
    (runtime, id)
}

fn ready() -> (NodeRuntime, RouteId, PinnedSourceAuthority) {
    let (mut runtime, id) = pending();
    let mut consent = AudioSourceConsent::default();
    consent.enqueue(&mut runtime, id, 100).unwrap();
    consent.resolve(&mut runtime, id, true, 101, 3000).unwrap();
    let source = runtime.route(id).unwrap().source;
    runtime
        .acquire_route_port_lease(id, source, 2000, 102)
        .unwrap();
    runtime
        .prepare_route(
            id,
            Some(FormatDescriptor::new(SECURE_AUDIO_MONO_FORMAT)),
            QosMode::Interactive,
            103,
        )
        .unwrap();
    runtime.begin_route_start(id, 104).unwrap();
    let route = runtime.route(id).unwrap();
    let binding = AudioTlsBinding::from_route(
        ControlTlsBinding::new(route.session_id, route.sink.node_id, route.source.node_id).unwrap(),
        route,
        StreamId::new(),
        105,
    )
    .unwrap();
    let authority = PinnedSourceAuthority::new(&runtime, binding, 105).unwrap();
    (runtime, id, authority)
}

#[test]
fn actual_microphone_consent_never_mints_a_source_lease() {
    let (mut runtime, id) = pending();
    let source = runtime.route(id).unwrap().source;
    let mut consent = AudioSourceConsent::default();
    assert!(source_authority(&runtime, runtime.route(id).unwrap(), 100).is_none());
    consent.enqueue(&mut runtime, id, 100).unwrap();
    assert!(
        runtime
            .acquire_route_port_lease(id, source, 1000, 101)
            .is_err()
    );
    consent.resolve(&mut runtime, id, true, 101, 2000).unwrap();
    assert!(source_authority(&runtime, runtime.route(id).unwrap(), 102).is_none());
    runtime
        .acquire_route_port_lease(id, source, 1000, 102)
        .unwrap();
    assert_eq!(
        source_authority(&runtime, runtime.route(id).unwrap(), 103),
        Some(2000)
    );
    assert!(source_authority(&runtime, runtime.route(id).unwrap(), 1000).is_none());
}

#[test]
fn explicit_denial_and_camera_substitution_cannot_authorize_audio() {
    let (mut runtime, id) = pending();
    let mut consent = AudioSourceConsent::default();
    consent.enqueue(&mut runtime, id, 100).unwrap();
    consent.resolve(&mut runtime, id, false, 101, 2000).unwrap();
    assert!(source_authority(&runtime, runtime.route(id).unwrap(), 102).is_none());
    let (mut runtime, id, authority) = ready();
    let source = runtime.route(id).unwrap().source;
    let mut node = runtime.snapshot().local_node;
    node.capabilities
        .get_mut(&source.capability_id)
        .unwrap()
        .class = CapabilityClass::Camera;
    runtime.replace_local_catalog(node).unwrap();
    assert!(source_authority(&runtime, runtime.route(id).unwrap(), 106).is_none());
    assert!(authority.check(&runtime, 106).is_err());
    assert!(consent.enqueue(&mut runtime, id, 106).is_err());
}

#[test]
fn original_grant_and_source_lease_cannot_be_renewed_or_narrowed() {
    for expiry in [1500, 2500] {
        let (mut runtime, id, authority) = ready();
        let source = runtime.route(id).unwrap().source;
        runtime
            .acquire_route_port_lease(id, source, expiry, 106)
            .unwrap();
        assert!(source_authority(&runtime, runtime.route(id).unwrap(), 107).is_some());
        assert!(authority.check(&runtime, 107).is_err());
    }
    let (mut runtime, id, authority) = ready();
    runtime.renew_route_authorization(id, 4000, 106).unwrap();
    assert!(authority.check(&runtime, 107).is_err());
}

#[test]
fn all_immutable_route_fields_are_pinned() {
    let (runtime, id, authority) = ready();
    let original = runtime.route(id).unwrap().clone();
    let changes: [fn(&mut Route); 13] = [
        |r| r.id = RouteId::new(),
        |r| r.session_id = SessionId::new(),
        |r| r.source.port_id = PortId::new(),
        |r| r.sink.port_id = PortId::new(),
        |r| r.epoch += 1,
        |r| r.profile = ProfileId::video_frames_v1(),
        |r| r.backend = RouteBackend::CapyDataPlane,
        |r| r.selected_format = Some(FormatDescriptor::new("pcm-s16le-48000-mono")),
        |r| {
            r.selected_format
                .as_mut()
                .unwrap()
                .parameters
                .insert("channels".into(), "1".into());
        },
        |r| r.selected_qos_mode = Some(QosMode::Basic),
        |r| {
            r.authorization = AuthorizationState::Authorized {
                expires_at_ms: Some(2000),
            }
        },
        |r| r.authorization = AuthorizationState::Revoked,
        |r| r.state = RouteState::Stopping,
    ];
    for change in changes {
        let mut changed = original.clone();
        change(&mut changed);
        assert!(authority.check_route(&changed, 106).is_err());
    }
    let mut active = original;
    active.state = RouteState::Active;
    authority.check_route(&active, 106).unwrap();
}

#[test]
fn stop_session_loss_and_catalog_unavailability_fail_closed() {
    let (mut runtime, id, authority) = ready();
    runtime.activate_route(id).unwrap();
    authority.check(&runtime, 106).unwrap();
    runtime.begin_route_stop(id).unwrap();
    runtime.stop_route(id).unwrap();
    assert!(authority.check(&runtime, 107).is_err());
    let (mut runtime, id, authority) = ready();
    runtime
        .close_session(runtime.route(id).unwrap().session_id)
        .unwrap();
    assert!(authority.check(&runtime, 106).is_err());
    for availability in [Availability::Offline, Availability::PermissionRequired] {
        let (mut runtime, id, authority) = ready();
        let source = runtime.route(id).unwrap().source;
        let mut node = runtime.snapshot().local_node;
        node.capabilities
            .get_mut(&source.capability_id)
            .unwrap()
            .availability = availability;
        runtime.replace_local_catalog(node).unwrap();
        assert!(authority.check(&runtime, 106).is_err());
    }
}

#[test]
fn original_clock_includes_handshake_and_never_ceil_extends_or_refreshes() {
    let origin = Instant::now();
    let mut short = AuthorityClock::new(1000, 1001, origin).unwrap();
    short
        .check(1000, origin + Duration::from_micros(999))
        .unwrap();
    assert!(
        short
            .check(1000, origin + Duration::from_millis(1))
            .is_err()
    );
    assert!(AuthorityClock::new(1000, 1000, origin).is_err());
    assert!(
        AuthorityClock::new(1000, 2000, origin)
            .unwrap()
            .check(999, origin)
            .is_err()
    );
    assert!(
        AuthorityClock::new(1000, 2000, origin)
            .unwrap()
            .check(1000, origin - Duration::from_nanos(1))
            .is_err()
    );
    let mut capped = AuthorityClock::new(1000, 500_000, origin).unwrap();
    assert_eq!(capped.deadline, origin + Duration::from_secs(60));
    capped
        .check(1000, origin + Duration::from_secs(59))
        .unwrap();
    assert!(
        capped
            .check(1000, origin + Duration::from_secs(60))
            .is_err()
    );
    let mut wall_expiry = AuthorityClock::new(1000, 2000, origin).unwrap();
    assert!(wall_expiry.check(2000, origin).is_err());
}

#[test]
fn owned_busy_is_not_new_authority_and_catalog_selection_cannot_change() {
    let (mut runtime, id, authority) = ready();
    let source = runtime.route(id).unwrap().source;
    let mut node = runtime.snapshot().local_node;
    let cap = node.capabilities.get_mut(&source.capability_id).unwrap();
    cap.availability = Availability::Busy;
    cap.ports.get_mut(&source.port_id).unwrap().availability = Availability::Busy;
    runtime.replace_local_catalog(node).unwrap();
    authority.check(&runtime, 106).unwrap();
    runtime
        .release_route_port_lease(id, source, runtime.route(id).unwrap().session_id)
        .unwrap();
    assert!(authority.check(&runtime, 107).is_err());
    for change in [
        (|port: &mut PortDescriptor| {
            port.formats = vec![FormatDescriptor::new("pcm-s16le-48000-mono")]
        }) as fn(&mut PortDescriptor),
        |port| {
            port.qos_modes.clear();
            port.qos_modes.insert(QosMode::Basic);
        },
        |port| port.interoperability_mode = InteroperabilityMode::StandardPort,
    ] {
        let (mut runtime, id, authority) = ready();
        let source = runtime.route(id).unwrap().source;
        let mut node = runtime.snapshot().local_node;
        change(
            node.capabilities
                .get_mut(&source.capability_id)
                .unwrap()
                .ports
                .get_mut(&source.port_id)
                .unwrap(),
        );
        runtime.replace_local_catalog(node).unwrap();
        assert!(authority.check(&runtime, 106).is_err());
    }
}
