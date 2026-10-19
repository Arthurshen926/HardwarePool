use std::collections::BTreeSet;

use capyio_audio::{AudioMediaStreamBinding, AudioStreamSpec};
use capyio_core::*;

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
        state: RouteState::Starting,
        authorization: AuthorizationState::Authorized {
            expires_at_ms: Some(1000),
        },
        epoch: 7,
        diagnostic_ids: vec![],
    };
    let binding = AudioMediaStreamBinding {
        session_id: session.id,
        route_id: route.id,
        stream_id: StreamId::new(),
        stream_epoch: 7,
        selected_spec: AudioStreamSpec::media_balanced(),
    };
    (binding, route, session)
}

#[test]
fn starting_and_active_audio_contexts_support_both_directions() {
    let (binding, mut route, session) = context();
    binding
        .validate_route_context(&route, &session, 999)
        .unwrap();
    std::mem::swap(&mut route.source, &mut route.sink);
    route.state = RouteState::Active;
    binding
        .validate_route_context(&route, &session, 999)
        .unwrap();
    assert!(
        binding
            .validate_route_context(&route, &session, 1000)
            .is_err()
    );
}

#[test]
fn stopped_revoked_or_restarted_routes_cannot_reuse_binding() {
    let (binding, route, session) = context();
    for state in [
        RouteState::Draft,
        RouteState::Prepared,
        RouteState::Stopping,
        RouteState::Stopped,
        RouteState::Failed,
        RouteState::Offline,
    ] {
        let mut changed = route.clone();
        changed.state = state;
        assert!(
            binding
                .validate_route_context(&changed, &session, 1)
                .is_err()
        );
    }
    for authorization in [
        AuthorizationState::Pending,
        AuthorizationState::Denied,
        AuthorizationState::Revoked,
        AuthorizationState::Authorized {
            expires_at_ms: None,
        },
    ] {
        let mut changed = route.clone();
        changed.authorization = authorization;
        assert!(
            binding
                .validate_route_context(&changed, &session, 1)
                .is_err()
        );
    }
    for epoch in [0, 8, u64::from(u32::MAX) + 1] {
        let mut changed = route.clone();
        changed.epoch = epoch;
        assert!(
            binding
                .validate_route_context(&changed, &session, 1)
                .is_err()
        );
    }
}

#[test]
fn foreign_route_session_profile_or_node_is_rejected() {
    let (binding, route, session) = context();
    let mut wrong = route.clone();
    wrong.id = RouteId::new();
    assert!(binding.validate_route_context(&wrong, &session, 1).is_err());
    wrong = route.clone();
    wrong.profile = ProfileId::touchpad_frames_v1();
    assert!(binding.validate_route_context(&wrong, &session, 1).is_err());
    wrong = route.clone();
    wrong.source.node_id = NodeId::new();
    assert!(binding.validate_route_context(&wrong, &session, 1).is_err());
    wrong = route.clone();
    wrong.session_id = SessionId::new();
    assert!(binding.validate_route_context(&wrong, &session, 1).is_err());
    let mut closed = session.clone();
    closed.state = SessionState::Closed;
    assert!(binding.validate_route_context(&route, &closed, 1).is_err());
    let other = Session::new(session.local_node_id, session.remote_node_id);
    assert!(binding.validate_route_context(&route, &other, 1).is_err());
}
