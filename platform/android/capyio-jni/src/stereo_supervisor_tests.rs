use super::*;
use crate::stereo_negotiation::tests::fixture::{credentials, nodes};
use capyio_control_tls::{
    AuthenticatedControlClientChannel, AuthenticatedControlServer, ControlTlsBinding,
    ControlTlsEndpoint,
};
use capyio_core::*;
use capyio_protocol::SECURE_AUDIO_STEREO_FORMAT;
use std::{collections::BTreeSet, net::TcpListener, thread};
fn catalog(revision: u64) -> String {
    let mut value: serde_json::Value =
        serde_json::from_str(include_str!("../../fixtures/android-core-catalog-v1.json")).unwrap();
    let cap = value["capabilities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|cap| cap["class"] == "speaker")
        .unwrap();
    cap["speaker_selection"] =
        serde_json::json!({"profile":crate::secure_speaker_policy::PROFILE,"revision":revision});
    cap["ports"][0]["formats"] = serde_json::json!([SECURE_AUDIO_STEREO_FORMAT]);
    value.to_string()
}

struct Fixture {
    supervisor: StereoSinkSupervisor,
    source: capyio_runtime::NodeRuntime,
    source_link: CatalogControlLink,
    source_port: PortRef,
    sink_port: PortRef,
}
impl Fixture {
    fn new(android_accepts: bool) -> Self {
        let mut host = AndroidCatalogRuntimeV1::new(&catalog(1)).unwrap();
        let android = host.runtime.snapshot().local_node;
        let (_, linux, source, _) = nodes();
        let cap = android
            .capabilities
            .values()
            .find(|cap| cap.class == CapabilityClass::Speaker)
            .unwrap();
        let sink = PortRef {
            node_id: android.id,
            capability_id: cap.id,
            port_id: *cap.ports.keys().next().unwrap(),
        };
        let binding = if android_accepts {
            ControlTlsBinding::new(SessionId::new(), android.id, linux.id)
        } else {
            ControlTlsBinding::new(SessionId::new(), linux.id, android.id)
        }
        .unwrap();
        let mut source_runtime = capyio_runtime::NodeRuntime::new(linux.clone()).unwrap();
        source_runtime.register_peer(android, true).unwrap();
        host.runtime.register_peer(linux, true).unwrap();
        let (android_server, android_client, linux_server, linux_client) = credentials();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = AuthenticatedControlServer::from_listener(
            listener,
            if android_accepts {
                android_server.clone()
            } else {
                linux_server
            },
            binding,
            Duration::from_secs(2),
        )
        .unwrap();
        let accepting = thread::spawn(move || server.accept().unwrap());
        let dialed = AuthenticatedControlClientChannel::connect(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(2),
            },
            "localhost",
            if android_accepts {
                linux_client.clone()
            } else {
                android_client
            },
            binding,
        )
        .unwrap();
        let accepted = accepting.join().unwrap();
        let (mut sink_link, mut source_link) = if android_accepts {
            (
                CatalogControlLink::admit_accepted_peer(
                    &mut host.runtime,
                    accepted,
                    "Linux".into(),
                )
                .unwrap(),
                CatalogControlLink::admit_dialed_peer(
                    &mut source_runtime,
                    dialed,
                    "Android".into(),
                )
                .unwrap(),
            )
        } else {
            (
                CatalogControlLink::admit_dialed_peer(&mut host.runtime, dialed, "Linux".into())
                    .unwrap(),
                CatalogControlLink::admit_accepted_peer(
                    &mut source_runtime,
                    accepted,
                    "Android".into(),
                )
                .unwrap(),
            )
        };
        // Actual catalog wire exchange, not two pre-authorized fixture grants.
        source_link
            .send_catalog(
                source_link
                    .prepare_catalog(&source_runtime, &BTreeSet::from([source.capability_id]))
                    .unwrap(),
            )
            .unwrap();
        let incoming = sink_link.receive_catalog().unwrap();
        sink_link
            .apply_catalog(&mut host.runtime, incoming)
            .unwrap();
        sink_link
            .send_catalog(
                sink_link
                    .prepare_catalog(&host.runtime, &BTreeSet::new())
                    .unwrap(),
            )
            .unwrap();
        let incoming = source_link.receive_catalog().unwrap();
        source_link
            .apply_catalog(&mut source_runtime, incoming)
            .unwrap();

        Self {
            supervisor: StereoSinkSupervisor::new(host, sink_link, 17).unwrap(),
            source: source_runtime,
            source_link,
            source_port: source,
            sink_port: sink,
        }
    }
    fn pending(&mut self) -> RouteId {
        let id = self
            .source
            .create_route(
                self.source_link.session_id(),
                self.source_port,
                self.sink_port,
                RouteBackend::AdapterManaged,
            )
            .unwrap();
        self.source_link
            .send_route_status(self.source.route(id).unwrap())
            .unwrap();
        self.wait_event();
        id
    }
    fn wait_event(&mut self) {
        let end = Instant::now() + Duration::from_secs(2);
        while self.supervisor.poll().unwrap().is_none() {
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn source_approval(&mut self, id: RouteId) {
        self.source
            .authorize_route(id, Some(now_ms().unwrap() + 60_000))
            .unwrap();
        self.source_link
            .send_route_status(self.source.route(id).unwrap())
            .unwrap();
        self.wait_event();
    }
    fn admission(&self, id: RouteId, seconds: u64) -> SecureStereoSinkAdmission {
        SecureStereoSinkAdmission {
            expected: self.source_link.tls_binding(),
            route_id: id,
            stream_id: StreamId::new(),
            local_address: "127.0.0.1".parse().unwrap(),
            lifetime: Duration::from_secs(seconds),
        }
    }
    fn retired(&self, id: RouteId) {
        let host = self.supervisor.host.lock().unwrap();
        let route = host.runtime.route(id).unwrap();
        assert_eq!(route.state, RouteState::Failed);
        assert_eq!(route.authorization, AuthorizationState::Revoked);
        assert!(
            host.runtime
                .route_port_lease_claim(id, route.sink)
                .is_none()
        );
    }
}
#[test]
fn supervisor_clock_floor_failure_retires_original_after_receipt() {
    let mut f = Fixture::new(true);
    let id = f.pending();
    f.source_approval(id);
    *f.supervisor.last_wall.lock().unwrap() = now_ms().unwrap() + 1000;
    assert!(f.supervisor.approve(id, true, f.admission(id, 60)).is_err());
    f.retired(id);
}
#[test]
fn supervisor_downstream_stamp_failure_retires_real_runtime() {
    let mut f = Fixture::new(false);
    let id = f.pending();
    f.source_approval(id);
    f.supervisor.generation = 18;
    assert!(f.supervisor.approve(id, true, f.admission(id, 60)).is_err());
    f.retired(id);
}
#[test]
fn supervisor_original_wait_deadline_does_not_restart_on_receipt() {
    let mut f = Fixture::new(true);
    let id = f.pending();
    f.source_approval(id);
    f.supervisor.end = Instant::now();
    assert!(f.supervisor.approve(id, true, f.admission(id, 60)).is_err());
    f.retired(id);
}
#[test]
fn supervisor_clone_control_invalidation_survives_stamp_consumption() {
    let mut f = Fixture::new(false);
    let id = f.pending();
    f.source_approval(id);
    f.supervisor.approve(id, true, f.admission(id, 60)).unwrap();
    let mut clone = f.supervisor.host.lock().unwrap().clone();
    clone
        .invalidate_bilateral_stereo_session(f.source_link.session_id())
        .unwrap();
    assert!(!f.supervisor.gate.as_ref().unwrap().is_open());
    assert!(f.supervisor.poll().is_err());
    f.retired(id);
}
#[test]
fn supervisor_invalid_import_retires_created_runtime_target() {
    let mut f = Fixture::new(true);
    let id = f
        .source
        .create_route(
            f.source_link.session_id(),
            f.source_port,
            f.sink_port,
            RouteBackend::AdapterManaged,
        )
        .unwrap();
    let mut route = f.source.route(id).unwrap().clone();
    route.epoch = 1;
    f.source_link.send_route_status(&route).unwrap();
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        match f.supervisor.poll() {
            Err(_) => break,
            Ok(_) => assert!(Instant::now() < end),
        }
    }
    f.retired(id);
}
#[test]
fn supervisor_short_receiver_bound_precedes_prepared_and_is_not_reauthorized() {
    let mut f = Fixture::new(false);
    let id = f.pending();
    f.source_approval(id);
    let before = now_ms().unwrap();
    f.supervisor.approve(id, true, f.admission(id, 5)).unwrap();
    let route = f
        .supervisor
        .host
        .lock()
        .unwrap()
        .runtime
        .route(id)
        .unwrap()
        .clone();
    assert_eq!(route.state, RouteState::Prepared);
    assert_eq!(route.epoch, 0);
    assert!(
        matches!(route.authorization,AuthorizationState::Authorized{expires_at_ms:Some(e)} if e<=before+5100)
    );
    assert!(f.supervisor.approve(id, true, f.admission(id, 60)).is_err());
    f.retired(id);
}

#[test]
fn supervisor_shared_clock_samples_are_serialized_and_rollback_rejected() {
    let floor = Arc::new(Mutex::new(99));
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let first = floor.clone();
    let a = thread::spawn(move || {
        observe_wall_with(&first, || {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(100)
        })
    });
    entered_rx.recv().unwrap();
    let (sampled_tx, sampled_rx) = std::sync::mpsc::channel();
    let second = floor.clone();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let worker_barrier = barrier.clone();
    let b = thread::spawn(move || {
        worker_barrier.wait();
        observe_wall_with(&second, || {
            sampled_tx.send(()).unwrap();
            Ok(101)
        })
    });
    barrier.wait();
    assert!(
        sampled_rx.try_recv().is_err(),
        "second cannot sample while first holds clock guard"
    );
    release_tx.send(()).unwrap();
    assert_eq!(a.join().unwrap().unwrap(), 100);
    assert_eq!(b.join().unwrap().unwrap(), 101);
    assert_eq!(*floor.lock().unwrap(), 101);
    assert!(observe_wall_with(&floor, || Ok(100)).is_err());
    assert_eq!(*floor.lock().unwrap(), 101);
}

thread_local! { static FAILURE_PHASE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) }; }
pub(super) fn fail_at(phase: u8) -> Result<(), String> {
    if FAILURE_PHASE.with(|p| p.get() == phase) {
        Err(format!("injected bilateral phase {phase}"))
    } else {
        Ok(())
    }
}
fn injected<T>(phase: u8, action: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            FAILURE_PHASE.with(|p| p.set(0));
        }
    }
    FAILURE_PHASE.with(|p| p.set(phase));
    let _reset = Reset;
    action()
}
#[test]
fn supervisor_failure_after_prepared_send_retires_original() {
    let mut f = Fixture::new(true);
    let id = f.pending();
    f.source_approval(id);
    let result = injected(1, || f.supervisor.approve(id, true, f.admission(id, 60)));
    assert!(result.unwrap_err().contains("phase 1"));
    f.retired(id);
    let ReceivedControlMessage::RouteStatus(status) =
        f.source_link.receive_control_message().unwrap()
    else {
        panic!("Prepared was sent")
    };
    assert_eq!(status.route.state, RouteState::Prepared);
}
#[test]
fn supervisor_failure_after_acquire_send_before_commit_retires_original() {
    let mut f = Fixture::new(false);
    let id = f.pending();
    f.source_approval(id);
    let result = injected(2, || f.supervisor.approve(id, true, f.admission(id, 60)));
    assert!(result.unwrap_err().contains("phase 2"));
    f.retired(id);
    assert!(matches!(
        f.source_link.receive_control_message().unwrap(),
        ReceivedControlMessage::RouteStatus(_)
    ));
    assert!(matches!(
        f.source_link.receive_control_message().unwrap(),
        ReceivedControlMessage::PortLeaseRequest(_)
    ));
}
#[test]
fn supervisor_failure_after_finish_start_retires_original_and_lease() {
    use capyio_protocol::PortLeaseAction;
    use capyio_runtime::PortLeasePolicy;
    let mut f = Fixture::new(true);
    let id = f.pending();
    f.source_approval(id);
    f.supervisor.approve(id, true, f.admission(id, 60)).unwrap();
    let ReceivedControlMessage::RouteStatus(status) =
        f.source_link.receive_control_message().unwrap()
    else {
        panic!("Prepared")
    };
    let AuthorizationState::Authorized {
        expires_at_ms: Some(expiry),
    } = status.route.authorization
    else {
        panic!("finite")
    };
    f.source
        .restrict_draft_authorization(id, expiry, now_ms().unwrap())
        .unwrap();
    f.source
        .prepare_route(
            id,
            status.route.selected_format.clone(),
            QosMode::Interactive,
            now_ms().unwrap(),
        )
        .unwrap();
    f.source
        .set_local_port_lease_policy(f.source_port, PortLeasePolicy::Exclusive)
        .unwrap();
    let ReceivedControlMessage::PortLeaseRequest(request) =
        f.source_link.receive_control_message().unwrap()
    else {
        panic!("Source Acquire")
    };
    let response = f
        .source_link
        .prepare_port_lease_response(&mut f.source, request, now_ms().unwrap())
        .unwrap();
    f.source_link
        .send_prepared_port_lease_response(response)
        .unwrap();
    f.source_link
        .send_port_lease_request(id, f.sink_port, PortLeaseAction::Acquire, 1000)
        .unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    while !f.supervisor.negotiation.as_ref().unwrap().ready() {
        f.supervisor.poll().unwrap();
        if f.supervisor
            .host
            .lock()
            .unwrap()
            .runtime
            .route_port_lease_claim(id, f.sink_port)
            .is_some()
        {
            break;
        }
        assert!(Instant::now() < until);
    }
    assert!(
        f.supervisor
            .host
            .lock()
            .unwrap()
            .runtime
            .route_port_lease_claim(id, f.sink_port)
            .is_some()
    );
    assert!(matches!(
        f.source_link.receive_control_message().unwrap(),
        ReceivedControlMessage::PortLeaseResponse(_)
    ));
    f.source.begin_route_start(id, now_ms().unwrap()).unwrap();
    f.source_link
        .send_route_command(id, RouteCommandAction::Start)
        .unwrap();
    let result = injected(3, || {
        loop {
            match f.supervisor.poll() {
                Err(e) => break e,
                Ok(_) => assert!(Instant::now() < until),
            }
        }
    });
    assert!(result.contains("phase 3"), "{result}");
    f.retired(id);
    assert_eq!(
        f.supervisor
            .host
            .lock()
            .unwrap()
            .runtime
            .route(id)
            .unwrap()
            .epoch,
        2
    );
}

#[test]
fn supervisor_clone_legacy_stop_revokes_live_original_gate() {
    let mut f = Fixture::new(true);
    let id = f.pending();
    f.source_approval(id);
    f.supervisor.approve(id, true, f.admission(id, 60)).unwrap();
    let mut clone = f.supervisor.host.lock().unwrap().clone();
    clone.stop_audio_direction("speaker-secure").unwrap();
    assert!(!f.supervisor.gate.as_ref().unwrap().is_open());
    assert_eq!(
        clone.runtime.route(id).unwrap().authorization,
        AuthorizationState::Revoked
    );
    assert!(f.supervisor.poll().is_err());
    f.retired(id);
}
#[test]
fn supervisor_registered_legacy_dispatch_cannot_mutate_or_publish() {
    let mut f = Fixture::new(false);
    let id = f.pending();
    f.source_approval(id);
    f.supervisor.approve(id, true, f.admission(id, 60)).unwrap();
    // Drain the supervisor's two real messages before checking legacy silence.
    assert!(matches!(
        f.source_link.receive_control_message().unwrap(),
        ReceivedControlMessage::RouteStatus(_)
    ));
    assert!(matches!(
        f.source_link.receive_control_message().unwrap(),
        ReceivedControlMessage::PortLeaseRequest(_)
    ));
    let mut clone = f.supervisor.host.lock().unwrap().clone();
    let before = clone.snapshot();
    let route = clone.runtime.route(id).unwrap().clone();
    let status = capyio_protocol::ValidatedRouteStatus {
        message_id: MessageId::new(),
        session_id: route.session_id,
        route,
        touchpad_data: None,
        haptics_data: None,
        native_audio_data: None,
        camera_data: None,
        secure_camera_data: None,
        secure_audio_data: None,
    };
    assert!(
        clone
            .receive_audio_route_status(&f.supervisor.link, &status, "127.0.0.1", now_ms().unwrap())
            .is_err()
    );
    let command = capyio_protocol::ValidatedRouteCommand {
        message_id: MessageId::new(),
        session_id: status.session_id,
        route_id: id,
        action: RouteCommandAction::Stop,
    };
    assert!(
        clone
            .stage_audio_route_command(&f.supervisor.link, &command, now_ms().unwrap())
            .is_err()
    );
    assert!(clone.prepare_audio_endpoint(id, 1234).is_err());
    assert!(
        clone
            .enqueue_touchpad_request(id, now_ms().unwrap())
            .is_err()
    );
    assert!(
        clone
            .resolve_touchpad_request(id, true, now_ms().unwrap())
            .is_err()
    );
    assert_eq!(clone.snapshot(), before);
    assert!(clone.audio_status_pending.contains(&id));
    clone
        .publish_audio_control(&mut f.supervisor.link, now_ms().unwrap())
        .unwrap();
    assert!(clone.audio_status_pending.contains(&id));
    assert!(
        f.source_link
            .poll_control_message_until(Instant::now() + Duration::from_millis(50))
            .unwrap()
            .is_none()
    );
    f.source_link
        .send_port_lease_request(
            id,
            f.sink_port,
            capyio_protocol::PortLeaseAction::Acquire,
            1000,
        )
        .unwrap();
    let ReceivedControlMessage::PortLeaseRequest(request) =
        f.supervisor.link.receive_control_message().unwrap()
    else {
        panic!("Acquire")
    };
    assert!(
        clone
            .answer_audio_port_lease(&mut f.supervisor.link, request, now_ms().unwrap() + 100_000)
            .is_err()
    );
    assert_eq!(clone.snapshot(), before);
    assert!(f.supervisor.gate.as_ref().unwrap().is_open());
}
#[test]
fn supervisor_clone_legacy_request_expiry_closes_pending_gate() {
    for list in [false, true] {
        let mut f = Fixture::new(list);
        let id = f.pending();
        let mut clone = f.supervisor.host.lock().unwrap().clone();
        let expiry = clone.touchpad_consent.pending_deadline(id).unwrap();
        if list {
            clone.resource_requests_json(expiry).unwrap();
        } else {
            clone.expire_touchpad_requests(expiry);
        }
        assert!(!f.supervisor.gate.as_ref().unwrap().is_open());
        assert!(f.supervisor.poll().is_err());
        f.retired(id);
    }
}

#[test]
fn supervisor_catalog_exchange_rejects_switched_host_without_mutating_original() {
    let mut f = Fixture::new(true);
    let id = f.pending();
    f.source_approval(id);
    f.supervisor.approve(id, true, f.admission(id, 60)).unwrap();
    let original = f.supervisor.host_handle();
    let before = original.lock().unwrap().snapshot();
    let mut alternate = AndroidCatalogRuntimeV1::new(&catalog(1)).unwrap();
    alternate
        .runtime
        .register_peer(f.source.snapshot().local_node.clone(), true)
        .unwrap();
    let alternate = Arc::new(Mutex::new(alternate));
    let binding = f.source_link.tls_binding();
    let (android_server, _, _, linux_client) = credentials();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = AuthenticatedControlServer::from_listener(
        listener,
        android_server,
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
        linux_client,
        binding,
    )
    .unwrap();
    let mut source = capyio_runtime::NodeRuntime::new(f.source.snapshot().local_node).unwrap();
    source
        .register_peer(before.local_node.clone(), true)
        .unwrap();
    let mut link =
        CatalogControlLink::admit_dialed_peer(&mut source, channel, "Android".into()).unwrap();
    link.send_catalog(
        link.prepare_catalog(&source, &BTreeSet::from([f.source_port.capability_id]))
            .unwrap(),
    )
    .unwrap();
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let error =
        crate::exchange_accepted_android_catalog(accepting.join().unwrap(), "Linux".into(), || {
            if calls.fetch_add(1, std::sync::atomic::Ordering::AcqRel) == 0 {
                alternate.lock().map_err(|_| "alternate poisoned".into())
            } else {
                original.lock().map_err(|_| "original poisoned".into())
            }
        })
        .err()
        .expect("alternate host catalog must be rejected");
    assert!(error.contains("registered bilateral peer"), "{error}");
    assert_eq!(original.lock().unwrap().snapshot(), before);
    assert!(f.supervisor.gate.as_ref().unwrap().is_open());
    assert!(!f.supervisor.link.is_closed());
}

#[test]
fn supervisor_closed_tombstone_allows_new_unregistered_peer_catalog_session() {
    let mut f = Fixture::new(true);
    let id = f.pending();
    f.source_approval(id);
    f.supervisor.approve(id, true, f.admission(id, 60)).unwrap();
    f.supervisor.stop().unwrap();
    f.retired(id);
    let handle = f.supervisor.host_handle();
    let android = handle.lock().unwrap().snapshot().local_node;
    let linux = f.source.snapshot().local_node;
    let binding = ControlTlsBinding::new(SessionId::new(), android.id, linux.id).unwrap();
    let (android_server, _, _, linux_client) = credentials();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = AuthenticatedControlServer::from_listener(
        listener,
        android_server,
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
        linux_client,
        binding,
    )
    .unwrap();
    let mut source = capyio_runtime::NodeRuntime::new(linux).unwrap();
    source.register_peer(android, true).unwrap();
    let mut link =
        CatalogControlLink::admit_dialed_peer(&mut source, channel, "Android".into()).unwrap();
    link.send_catalog(
        link.prepare_catalog(&source, &BTreeSet::from([f.source_port.capability_id]))
            .unwrap(),
    )
    .unwrap();
    let mut host = handle.lock().unwrap();
    let mut fresh = host
        .admit_accepted_catalog_exchange(
            accepting.join().unwrap(),
            "Linux".into(),
            &BTreeSet::new(),
        )
        .unwrap();
    assert_eq!(fresh.session_id(), binding.session_id);
    assert_ne!(fresh.session_id(), f.source_link.session_id());
    assert!(!host.bilateral_stereo.owns_session(fresh.session_id()));
    assert_eq!(
        host.runtime.route(id).unwrap().authorization,
        AuthorizationState::Revoked
    );
    fresh.close_and_reconcile(&mut host.runtime).unwrap();
}
