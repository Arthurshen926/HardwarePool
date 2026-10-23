use super::*;
#[path = "../tests/support/bilateral_fixture.rs"]
mod fixture;
use capyio_control_tls::{
    AuthenticatedControlClientChannel, AuthenticatedControlServer, ControlTlsBinding,
    ControlTlsEndpoint,
};
use capyio_runtime::PortLeasePolicy;
use std::{net::TcpListener, thread};

struct Peers {
    source: SecureStereoSourceHandshake,
    sink: NodeRuntime,
    link: CatalogControlLink,
    identity: ControlTlsClientIdentity,
}
impl Peers {
    fn new(lifetime: Duration) -> Self {
        let (mut android, linux, source, sink) = fixture::nodes();
        let cap = android.capabilities.get_mut(&sink.capability_id).unwrap();
        cap.metadata
            .insert("speaker.profile".into(), "secure_stereo_tls13_v1".into());
        cap.metadata
            .insert("speaker.selectionRevision".into(), "1".into());
        let binding = ControlTlsBinding::new(SessionId::new(), android.id, linux.id).unwrap();
        let (server_identity, _, _, identity) = fixture::credentials();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = AuthenticatedControlServer::from_listener(
            listener,
            server_identity,
            binding,
            Duration::from_secs(2),
        )
        .unwrap();
        let accepting = thread::spawn(move || server.accept().unwrap());
        let channel = AuthenticatedControlClientChannel::connect(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(2),
            },
            "localhost",
            identity.clone(),
            binding,
        )
        .unwrap();
        let mut source_runtime = NodeRuntime::new(linux.clone()).unwrap();
        let mut sink_runtime = NodeRuntime::new(android.clone()).unwrap();
        let source_link =
            CatalogControlLink::admit_dialed_peer(&mut source_runtime, channel, "Android".into())
                .unwrap();
        let sink_link = CatalogControlLink::admit_accepted_peer(
            &mut sink_runtime,
            accepting.join().unwrap(),
            "Linux".into(),
        )
        .unwrap();
        // Unit fixture catalog injection only; cross-library tests separately
        // exercise the real catalog exchange and portable Android approval API.
        source_runtime
            .accept_peer_catalog(binding.session_id, android)
            .unwrap();
        sink_runtime
            .accept_peer_catalog(binding.session_id, linux)
            .unwrap();
        let source = SecureStereoSourceHandshake::propose(
            source_runtime,
            source_link,
            SecureStereoSourceIntent {
                source,
                sink,
                pipewire_target: "selected.synthetic.source".into(),
                lifetime,
            },
        )
        .unwrap();
        Self {
            source,
            sink: sink_runtime,
            link: sink_link,
            identity,
        }
    }
    fn ready(&mut self) {
        let ReceivedControlMessage::RouteStatus(pending) =
            self.link.receive_control_message().unwrap()
        else {
            panic!("Pending")
        };
        self.link
            .apply_route_status(&mut self.sink, &pending, false)
            .unwrap();
        self.source.approve(true).unwrap();
        let ReceivedControlMessage::RouteStatus(authorized) =
            self.link.receive_control_message().unwrap()
        else {
            panic!("Authorized Draft")
        };
        let AuthorizationState::Authorized {
            expires_at_ms: Some(end),
        } = authorized.route.authorization
        else {
            panic!("finite")
        };
        let id = authorized.route.id;
        self.sink.authorize_route(id, Some(end)).unwrap();
        self.sink
            .prepare_route(
                id,
                Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)),
                QosMode::Interactive,
                clock().unwrap().0,
            )
            .unwrap();
        self.link
            .send_route_status(self.sink.route(id).unwrap())
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.source.poll().unwrap() != SecureStereoSourceHandshakeEvent::Ready {
            assert!(Instant::now() < deadline);
        }
    }
}
fn retired(runtime: &NodeRuntime, id: RouteId) {
    let route = runtime.route(id).unwrap();
    assert_eq!(route.state, RouteState::Failed);
    assert_eq!(route.authorization, AuthorizationState::Revoked);
}

#[test]
fn bilateral_source_constructor_rejects_inherited_wall_floor_before_acquire() {
    let mut peers = Peers::new(Duration::from_secs(60));
    peers.ready();
    let id = peers.source.route_id().unwrap();
    let admission = SecureStereoReceiverAssignedSourceAdmission {
        expected: peers.source.link.as_ref().unwrap().tls_binding(),
        route_id: id,
        lifetime: Duration::from_secs(60),
        pipewire_target: "selected.synthetic.source".into(),
    };
    let result = SecureStereoSourceOwner::new_bilateral_until(
        peers.source.runtime.take().unwrap(),
        peers.source.link.take().unwrap(),
        peers.identity,
        "localhost".into(),
        admission,
        peers.source.approved_end.unwrap(),
        clock().unwrap().0 + 1000,
    );
    let failure = result.err().expect("rollback must fail before Acquire");
    assert!(failure.error.to_string().contains("clock regressed"));
    retired(&failure.runtime, id);
    assert!(
        failure
            .runtime
            .route_port_lease_claim(id, peers.source.intent.source)
            .is_none()
    );
    assert!(
        peers.link.receive_control_message().is_err(),
        "no Acquire may be sent before high-water validation"
    );
}

#[test]
fn bilateral_source_constructor_cannot_restart_expired_monotonic_cap() {
    let mut peers = Peers::new(Duration::from_secs(60));
    peers.ready();
    let id = peers.source.route_id().unwrap();
    let admission = SecureStereoReceiverAssignedSourceAdmission {
        expected: peers.source.link.as_ref().unwrap().tls_binding(),
        route_id: id,
        lifetime: Duration::from_secs(60),
        pipewire_target: "selected.synthetic.source".into(),
    };
    let result = SecureStereoSourceOwner::new_bilateral_until(
        peers.source.runtime.take().unwrap(),
        peers.source.link.take().unwrap(),
        peers.identity,
        "localhost".into(),
        admission,
        Instant::now() - Duration::from_millis(1),
        peers.source.last_wall,
    );
    let failure = result.err().expect("elapsed cap");
    assert!(failure.error.to_string().contains("consent elapsed"));
    retired(&failure.runtime, id);
    assert!(peers.link.receive_control_message().is_err());
}

#[test]
fn bilateral_source_repeated_approval_retires_only_target_session() {
    let mut peers = Peers::new(Duration::from_secs(60));
    let target = peers.source.route_id().unwrap();
    let mut other = peers.sink.snapshot().local_node;
    other.id = NodeId::new();
    let other_sink = PortRef {
        node_id: other.id,
        ..peers.source.intent.sink
    };
    let source = peers.source.intent.source;
    let runtime = peers.source.runtime.as_mut().unwrap();
    runtime.register_peer(other, true).unwrap();
    let other_session = runtime.open_session(other_sink.node_id).unwrap();
    let other_route = runtime
        .create_route(
            other_session,
            source,
            other_sink,
            RouteBackend::AdapterManaged,
        )
        .unwrap();
    let now = clock().unwrap().0;
    runtime
        .authorize_route(other_route, Some(now + 60_000))
        .unwrap();
    runtime
        .set_local_port_lease_policy(source, PortLeasePolicy::Exclusive)
        .unwrap();
    runtime
        .acquire_route_port_lease(other_route, source, now + 30_000, now)
        .unwrap();
    let original = runtime.route(other_route).unwrap().clone();
    let claim = runtime.route_port_lease_claim(other_route, source).copied();
    peers.source.approve(true).unwrap();
    assert!(peers.source.approve(true).is_err());
    assert_eq!(
        peers.source.state(),
        SecureStereoSourceHandshakeEvent::Terminal
    );
    retired(peers.source.runtime(), target);
    assert_eq!(
        peers.source.runtime().route(other_route).unwrap(),
        &original
    );
    assert_eq!(
        peers
            .source
            .runtime()
            .route_port_lease_claim(other_route, source)
            .copied(),
        claim
    );
    assert_eq!(
        peers.source.runtime().session(other_session).unwrap().state,
        SessionState::Ready
    );
}

#[test]
fn bilateral_source_wall_and_monotonic_high_water_never_regress() {
    let mut peers = Peers::new(Duration::from_secs(60));
    peers.source.approve(true).unwrap();
    let wall = peers.source.last_wall;
    let instant = peers.source.last_instant;
    assert!(peers.source.check_at(wall - 1, instant).is_err());
    assert!(
        peers
            .source
            .check_at(wall, instant - Duration::from_nanos(1))
            .is_err()
    );
    let future_wall = wall + 10;
    peers
        .source
        .check_at(future_wall, instant + Duration::from_millis(10))
        .unwrap();
    assert!(
        peers
            .source
            .check_at(wall, instant + Duration::from_millis(11))
            .is_err()
    );
}

#[test]
fn bilateral_source_short_consent_is_anchored_at_approval() {
    let mut peers = Peers::new(Duration::from_secs(2));
    let before = Instant::now();
    peers.source.approve(true).unwrap();
    assert!(peers.source.approved_end.unwrap() >= before + Duration::from_secs(2));
    assert!(peers.source.approved_end.unwrap() <= Instant::now() + Duration::from_secs(2));
    let AuthorizationState::Authorized {
        expires_at_ms: Some(end),
    } = peers.source.original.as_ref().unwrap().authorization
    else {
        panic!("finite")
    };
    assert_eq!(end, peers.source.last_wall + 2000);
    assert!(
        peers
            .source
            .check_at(
                peers.source.last_wall + 2000,
                peers.source.approved_end.unwrap()
            )
            .is_err()
    );
}

#[test]
fn bilateral_source_injected_first_acquire_send_failure_retains_retired_runtime() {
    let mut peers = Peers::new(Duration::from_secs(60));
    peers.ready();
    let id = peers.source.route_id().unwrap();
    crate::secure_capture::fail_next_bilateral_acquire();
    let failure = peers
        .source
        .into_owner(peers.identity, "localhost".into())
        .err()
        .expect("injected send failure");
    retired(&failure.runtime, id);
    assert!(failure.cleanup_errors.is_empty());
    assert!(
        failure
            .runtime
            .route_port_lease_claim(id, failure.runtime.route(id).unwrap().source)
            .is_none()
    );
    assert!(
        peers.link.receive_control_message().is_err(),
        "injected first Acquire send must not publish bytes"
    );
}

fn peer_prepared(peers: &mut Peers, approve: bool) -> Route {
    let ReceivedControlMessage::RouteStatus(pending) =
        peers.link.receive_control_message().unwrap()
    else {
        panic!("Pending")
    };
    peers
        .link
        .apply_route_status(&mut peers.sink, &pending, false)
        .unwrap();
    let end = if approve {
        peers.source.approve(true).unwrap();
        let ReceivedControlMessage::RouteStatus(status) =
            peers.link.receive_control_message().unwrap()
        else {
            panic!("Authorized Draft")
        };
        let AuthorizationState::Authorized {
            expires_at_ms: Some(end),
        } = status.route.authorization
        else {
            panic!("finite")
        };
        end
    } else {
        clock().unwrap().0 + 60_000
    };
    let id = pending.route.id;
    peers.sink.authorize_route(id, Some(end)).unwrap();
    peers
        .sink
        .prepare_route(
            id,
            Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)),
            QosMode::Interactive,
            clock().unwrap().0,
        )
        .unwrap();
    peers.sink.route(id).unwrap().clone()
}
fn poll_failure(peers: &mut Peers) -> String {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match peers.source.poll() {
            Err(reason) => return reason.to_string(),
            Ok(_) => assert!(Instant::now() < deadline),
        }
    }
}
#[test]
fn bilateral_source_authenticated_prepared_cannot_widen_original_grant() {
    let mut peers = Peers::new(Duration::from_secs(2));
    let mut route = peer_prepared(&mut peers, true);
    let id = route.id;
    let AuthorizationState::Authorized {
        expires_at_ms: Some(end),
    } = route.authorization
    else {
        unreachable!()
    };
    route.authorization = AuthorizationState::Authorized {
        expires_at_ms: Some(end + 1000),
    };
    peers.link.send_route_status(&route).unwrap();
    assert!(poll_failure(&mut peers).contains("changed exact bilateral authority"));
    retired(peers.source.runtime(), id);
    assert!(
        peers
            .source
            .runtime()
            .route_port_lease_claim(id, route.source)
            .is_none()
    );
}
#[test]
fn bilateral_source_authenticated_prepared_rejects_changed_epoch_and_selected_qos() {
    for epoch_change in [false, true] {
        let mut peers = Peers::new(Duration::from_secs(60));
        let mut route = peer_prepared(&mut peers, true);
        let id = route.id;
        if epoch_change {
            route.epoch = 1;
        } else {
            route.selected_qos_mode = Some(QosMode::Measurement);
            route.compatible_qos_modes.insert(QosMode::Measurement);
        }
        peers.link.send_route_status(&route).unwrap();
        let reason = poll_failure(&mut peers);
        assert!(
            reason.contains("invalid") || reason.contains("Invalid") || reason.contains("peer"),
            "{reason}"
        );
        retired(peers.source.runtime(), id);
        assert!(
            peers
                .source
                .runtime()
                .route_port_lease_claim(id, route.source)
                .is_none()
        );
    }
}
#[test]
fn bilateral_source_authenticated_prepared_before_local_approval_cannot_authorize() {
    let mut peers = Peers::new(Duration::from_secs(60));
    let route = peer_prepared(&mut peers, false);
    let id = route.id;
    peers.link.send_route_status(&route).unwrap();
    assert!(poll_failure(&mut peers).contains("preceded local source consent"));
    assert_eq!(
        peers.source.runtime().route(id).unwrap().authorization,
        AuthorizationState::Denied
    );
    assert!(
        peers
            .source
            .runtime()
            .route_port_lease_claim(id, route.source)
            .is_none()
    );
}

#[test]
fn explicit_cancel_reports_retirement_and_preserves_cleanup_failure() {
    let mut peers = Peers::new(Duration::from_secs(60));
    peers.ready();
    let id = peers.source.route_id().unwrap();
    peers.source.cancel().unwrap();
    retired(peers.source.runtime(), id);
    assert!(peers.source.is_terminal());
    assert!(peers.source.cleanup_errors().is_empty());
    peers.source.cancel().unwrap();

    let mut peers = Peers::new(Duration::from_secs(60));
    peers.ready();
    // Deterministic actual Runtime failure: the private fixture corrupts only
    // the owned target identity, so retirement returns RouteNotFound. No mock
    // success, injected sleeps or swallowed errors can satisfy this oracle.
    peers.source.original.as_mut().unwrap().id = RouteId::new();
    assert!(peers.source.cancel().is_err());
    assert!(peers.source.is_terminal());
    assert!(!peers.source.cleanup_errors().is_empty());
    let retained = peers.source.cleanup_errors().to_vec();
    assert!(peers.source.cancel().is_err());
    assert_eq!(peers.source.cleanup_errors(), retained);
}
