use std::collections::{BTreeMap, BTreeSet};

use capyio_core::{
    AdapterDeploymentMode, AdapterHealth, AdapterInstanceDescriptor, AdapterInstanceId,
    AdapterState, Availability, CapabilityClass, CapabilityDescriptor, CapabilityId, CoreError,
    FormatDescriptor, InteroperabilityMode, NodeDescriptor, NodeId, PermissionRequirement,
    Platform, PortDescriptor, PortDirection, PortId, PortRef, ProblemId, ProfileId,
    ProtocolVersion, QosMode, Route, RouteBackend, RouteId, RouteState, SessionId,
};

fn port(direction: PortDirection, profile: ProfileId) -> (CapabilityDescriptor, PortDescriptor) {
    let adapter_id = AdapterInstanceId::new();
    let capability_id = CapabilityId::new();
    let port = PortDescriptor {
        id: PortId::new(),
        capability_id,
        display_name: "frames".to_owned(),
        direction,
        profile,
        schema_id: None,
        formats: vec![FormatDescriptor::new("mock/1")],
        qos_modes: BTreeSet::from([QosMode::Basic]),
        clock_domain: Some("mock-clock".to_owned()),
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::None,
        interoperability_mode: InteroperabilityMode::StandardPort,
    };
    let capability = CapabilityDescriptor {
        id: capability_id,
        adapter_instance_id: adapter_id,
        display_name: "Mock capability".to_owned(),
        class: CapabilityClass::Bridge,
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::None,
        metadata: BTreeMap::new(),
        ports: BTreeMap::from([(port.id, port.clone())]),
    };
    (capability, port)
}

fn route(source_port: &PortDescriptor, sink_port: &PortDescriptor) -> Result<Route, CoreError> {
    Route::new(
        RouteId::new(),
        SessionId::new(),
        PortRef {
            node_id: NodeId::new(),
            capability_id: source_port.capability_id,
            port_id: source_port.id,
        },
        source_port,
        PortRef {
            node_id: NodeId::new(),
            capability_id: sink_port.capability_id,
            port_id: sink_port.id,
        },
        sink_port,
        RouteBackend::CapyDataPlane,
    )
}

#[test]
fn source_to_sink_route_completes_lifecycle() {
    let (_, source) = port(PortDirection::Source, ProfileId::audio_frames_v1());
    let (_, sink) = port(PortDirection::Sink, ProfileId::audio_frames_v1());
    let mut route = route(&source, &sink).expect("compatible Route");
    route.authorize(None).expect("authorize");
    route
        .prepare(Some(FormatDescriptor::new("mock/1")), QosMode::Basic, 0)
        .expect("prepare");
    route.begin_start(0).expect("start");
    route.mark_active().expect("active");
    route.begin_stop().expect("stop");
    route.mark_stopped().expect("stopped");
    assert_eq!(route.state, RouteState::Stopped);
    assert_eq!(route.epoch, 1);
}

#[test]
fn same_direction_endpoints_are_rejected() {
    let (_, first) = port(PortDirection::Source, ProfileId::audio_frames_v1());
    let (_, second) = port(PortDirection::Source, ProfileId::audio_frames_v1());
    assert!(matches!(
        route(&first, &second),
        Err(CoreError::InvalidRouteEndpoint {
            expected: PortDirection::Sink,
            actual: PortDirection::Source,
            ..
        })
    ));
}

#[test]
fn incompatible_profiles_are_rejected() {
    let (_, source) = port(PortDirection::Source, ProfileId::audio_frames_v1());
    let (_, sink) = port(PortDirection::Sink, ProfileId::video_frames_v1());
    assert!(matches!(
        route(&source, &sink),
        Err(CoreError::IncompatibleProfiles { .. })
    ));
}

#[test]
fn invalid_route_transition_is_typed() {
    let (_, source) = port(PortDirection::Source, ProfileId::audio_frames_v1());
    let (_, sink) = port(PortDirection::Sink, ProfileId::audio_frames_v1());
    let mut route = route(&source, &sink).expect("Route");
    assert!(matches!(
        route.mark_active(),
        Err(CoreError::InvalidRouteTransition {
            from: RouteState::Draft,
            action: "mark_active"
        })
    ));
}

#[test]
fn offline_route_recovers_with_a_new_epoch() {
    let (_, source) = port(PortDirection::Source, ProfileId::audio_frames_v1());
    let (_, sink) = port(PortDirection::Sink, ProfileId::audio_frames_v1());
    let mut route = route(&source, &sink).expect("Route");
    route.authorize(None).expect("authorize");
    route
        .prepare(Some(FormatDescriptor::new("mock/1")), QosMode::Basic, 0)
        .expect("prepare");
    route.begin_start(0).expect("start");
    route.mark_active().expect("active");
    route.mark_offline().expect("offline");
    route.recover(1).expect("recover");
    route.begin_start(1).expect("restart");
    route.mark_active().expect("active again");
    assert_eq!(route.state, RouteState::Active);
    assert_eq!(route.epoch, 2);
}

#[test]
fn route_failure_retains_structured_problem_reference() {
    let (_, source) = port(PortDirection::Source, ProfileId::audio_frames_v1());
    let (_, sink) = port(PortDirection::Sink, ProfileId::audio_frames_v1());
    let mut route = route(&source, &sink).expect("Route");
    let problem_id = ProblemId::new();
    route
        .mark_failed(problem_id)
        .expect("failure is valid from Draft");
    assert_eq!(route.state, RouteState::Failed);
    assert_eq!(route.diagnostic_ids, vec![problem_id]);
}

#[test]
fn one_node_can_own_source_and_sink_ports_without_a_role() {
    let adapter_id = AdapterInstanceId::new();
    let mut node = NodeDescriptor::new(
        NodeId::new(),
        "Symmetric Node",
        Platform::Windows,
        "test",
        "0.1.0",
        [ProtocolVersion::new(1, 0)],
    );
    node.add_adapter(AdapterInstanceDescriptor {
        id: adapter_id,
        adapter_type: "mock.symmetric".to_owned(),
        display_name: "Mock symmetric".to_owned(),
        deployment_mode: AdapterDeploymentMode::InProcess,
        version: "1.0.0".to_owned(),
        state: AdapterState::Ready,
        health: AdapterHealth::Healthy,
        owned_capabilities: BTreeSet::new(),
        supported_route_modes: BTreeSet::from([RouteBackend::LocalPipeline]),
    })
    .expect("Adapter");

    for direction in [PortDirection::Source, PortDirection::Sink] {
        let (mut capability, mut descriptor) = port(direction, ProfileId::audio_frames_v1());
        capability.adapter_instance_id = adapter_id;
        descriptor.capability_id = capability.id;
        capability.ports = BTreeMap::from([(descriptor.id, descriptor)]);
        node.add_capability(capability).expect("Capability");
    }

    node.validate().expect("symmetric Node");
    assert_eq!(node.capabilities.len(), 2);
}
