use capyio_control_host::project_publication_catalog;
use capyio_control_link::{CatalogControlLink, CatalogLinkError, ReceivedControlMessage};
use capyio_control_tls::{
    AuthenticatedControlClientChannel, AuthenticatedControlServer,
    AuthenticatedControlServerChannel, ControlTlsBinding, ControlTlsClientIdentity,
    ControlTlsEndpoint, ControlTlsNodeBinding, ControlTlsServerIdentity,
};
use capyio_core::{NodeDescriptor, PortDirection, PortRef, RouteBackend, RouteState, SessionId};
use capyio_protocol::{
    MIN_PORT_LEASE_TTL_MS, PortLeaseAction, PortLeaseOutcome, RouteCommandAction,
};
use capyio_runtime::NodeRuntime;
use capyio_testkit::{android_node, windows_node};
use rcgen::{
    BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use std::{
    collections::BTreeSet,
    net::{Ipv4Addr, TcpListener},
    thread,
    time::Duration,
};

fn runtime(local: NodeDescriptor, peer: NodeDescriptor, session: SessionId) -> NodeRuntime {
    let mut runtime = NodeRuntime::new(local).unwrap();
    let peer_id = peer.id;
    runtime.register_peer(peer, true).unwrap();
    runtime.open_session_with_id(session, peer_id).unwrap();
    runtime
}

fn connected_pair() -> (
    CatalogControlLink,
    CatalogControlLink,
    NodeRuntime,
    NodeRuntime,
) {
    let (server, client, windows, android) = connected_raw_pair();
    (
        server,
        CatalogControlLink::from_dialed(client),
        windows,
        android,
    )
}

#[test]
fn native_audio_worker_preparation_requires_current_matching_offers_and_local_lease() {
    for preset in [
        capyio_protocol::NativeAudioPreset::PcmS16le48000Mono,
        capyio_protocol::NativeAudioPreset::PcmS16le48000Stereo,
    ] {
        check_native_audio_worker_preparation(preset);
    }
}

fn check_native_audio_worker_preparation(preset: capyio_protocol::NativeAudioPreset) {
    use capyio_core::{FormatDescriptor, InteroperabilityMode, StreamId};
    use capyio_protocol::{NativeAudioDataOffer, NativeAudioEndpoint};
    use capyio_runtime::PortLeasePolicy;
    let (mut windows_link, mut android_link, _, _) = connected_pair();
    let convert = |mut node: NodeDescriptor| {
        for adapter in node.adapter_instances.values_mut() {
            adapter
                .supported_route_modes
                .insert(RouteBackend::AdapterManaged);
        }
        for capability in node.capabilities.values_mut() {
            for port in capability.ports.values_mut() {
                if port.profile == capyio_core::ProfileId::audio_frames_v1() {
                    port.interoperability_mode = InteroperabilityMode::AdapterManaged;
                    port.formats = vec![FormatDescriptor::new(preset.format_name())];
                }
            }
        }
        node
    };
    let windows = convert(windows_node());
    let android = convert(android_node());
    let mono = FormatDescriptor::new(preset.format_name());
    let find = |node: &NodeDescriptor, direction| {
        node.capabilities
            .values()
            .find_map(|cap| {
                cap.ports.values().find_map(|port| {
                    (port.direction == direction && port.formats.contains(&mono)).then_some(
                        PortRef {
                            node_id: node.id,
                            capability_id: cap.id,
                            port_id: port.id,
                        },
                    )
                })
            })
            .unwrap()
    };
    let source = find(&android, PortDirection::Source);
    let sink = find(&windows, PortDirection::Sink);
    let mut runtime = runtime(windows, android, windows_link.session_id());
    let id = runtime
        .create_route(
            windows_link.session_id(),
            source,
            sink,
            RouteBackend::AdapterManaged,
        )
        .unwrap();
    runtime.authorize_route(id, Some(5000)).unwrap();
    runtime
        .set_local_port_lease_policy(sink, PortLeasePolicy::Exclusive)
        .unwrap();
    runtime.acquire_route_port_lease(id, sink, 3000, 1).unwrap();
    let qos = runtime
        .route(id)
        .unwrap()
        .compatible_qos_modes
        .iter()
        .next()
        .unwrap()
        .clone();
    runtime.prepare_route(id, Some(mono), qos, 1).unwrap();
    runtime.begin_route_start(id, 1).unwrap();
    let local = NativeAudioDataOffer {
        port: 40211,
        stream_id: StreamId::new(),
        endpoint: NativeAudioEndpoint::Sink,
        preset,
    };
    let peer = NativeAudioDataOffer {
        port: 40210,
        stream_id: local.stream_id,
        endpoint: NativeAudioEndpoint::Source,
        preset,
    };
    android_link
        .send_native_audio_data_offer(runtime.route(id).unwrap(), peer, 1)
        .unwrap();
    let ReceivedControlMessage::RouteStatus(received) =
        windows_link.receive_control_message().unwrap()
    else {
        panic!("expected audio offer")
    };
    let prepare = |runtime: &NodeRuntime, offer: &capyio_protocol::ValidatedRouteStatus, now| {
        windows_link.prepare_native_audio_worker(runtime, id, local, offer, true, now)
    };
    let result = prepare(&runtime, &received, 1).unwrap();
    assert_eq!(result.stream_id, local.stream_id);
    assert_eq!(result.preset, preset);
    assert_eq!((result.local_port, result.peer_port), (40211, 40210));
    assert_eq!(result.authorization_expires_at_ms, 5000);
    assert_eq!(result.local_lease_expires_at_ms, 3000);
    assert_eq!(runtime.route(id).unwrap().state, RouteState::Starting);
    assert!(
        windows_link
            .prepare_native_audio_worker(&runtime, id, local, &received, false, 1)
            .is_err()
    );
    for case in 0..8 {
        let mut bad = received.clone();
        match case {
            0 => bad.native_audio_data.as_mut().unwrap().stream_id = StreamId::new(),
            1 => bad.route.epoch += 1,
            2 => bad.route.selected_format = None,
            3 => bad.native_audio_data.as_mut().unwrap().endpoint = NativeAudioEndpoint::Sink,
            4 => bad.native_audio_data = None,
            5 => {
                bad.route.authorization = capyio_core::AuthorizationState::Authorized {
                    expires_at_ms: Some(4000),
                }
            }
            6 => bad.route.id = capyio_core::RouteId::new(),
            _ => {
                bad.native_audio_data.as_mut().unwrap().preset = match preset {
                    capyio_protocol::NativeAudioPreset::PcmS16le48000Mono => {
                        capyio_protocol::NativeAudioPreset::PcmS16le48000Stereo
                    }
                    capyio_protocol::NativeAudioPreset::PcmS16le48000Stereo => {
                        capyio_protocol::NativeAudioPreset::PcmS16le48000Mono
                    }
                }
            }
        }
        assert!(prepare(&runtime, &bad, 1).is_err(), "case {case}");
    }
    assert!(prepare(&runtime, &received, 3000).is_err());
    assert!(prepare(&runtime, &received, 5000).is_err());
    runtime
        .release_route_port_lease(id, sink, windows_link.session_id())
        .unwrap();
    assert!(prepare(&runtime, &received, 1).is_err());
    windows_link.close_and_reconcile(&mut runtime).unwrap();
    assert!(
        windows_link
            .prepare_native_audio_worker(&runtime, id, local, &received, true, 1)
            .is_err()
    );
}

#[test]
fn native_audio_endpoints_exchange_on_tls_only_for_their_own_route_side() {
    use capyio_core::{AuthorizationState, StreamId};
    use capyio_protocol::{NativeAudioDataOffer, NativeAudioEndpoint};
    let (mut windows_link, mut android_link, _, _) = connected_pair();
    let lab = capyio_testkit::DemoLab::new().unwrap();
    let mut route = lab
        .runtime
        .route(lab.routes.phone_microphone_to_windows)
        .unwrap()
        .clone();
    route.session_id = windows_link.session_id();
    route.source.node_id = windows_link.peer_node_id();
    route.sink.node_id = android_link.peer_node_id();
    route.backend = RouteBackend::AdapterManaged;
    route.state = RouteState::Starting;
    route.epoch = 3;
    route.authorization = AuthorizationState::Authorized {
        expires_at_ms: Some(5000),
    };
    let mut offer = NativeAudioDataOffer {
        port: 40211,
        stream_id: StreamId::new(),
        endpoint: NativeAudioEndpoint::Sink,
        preset: capyio_protocol::NativeAudioPreset::PcmS16le48000Mono,
    };
    windows_link
        .send_native_audio_data_offer(&route, offer, 1)
        .unwrap();
    let ReceivedControlMessage::RouteStatus(received) =
        android_link.receive_control_message().unwrap()
    else {
        panic!("expected offer")
    };
    assert_eq!(received.native_audio_data.as_deref(), Some(&offer));
    assert!(
        android_link
            .send_native_audio_data_offer(&route, offer, 1)
            .is_err()
    );
    offer.endpoint = NativeAudioEndpoint::Source;
    offer.port = 40210;
    android_link
        .send_native_audio_data_offer(&route, offer, 1)
        .unwrap();
    let ReceivedControlMessage::RouteStatus(received) =
        windows_link.receive_control_message().unwrap()
    else {
        panic!("expected reply")
    };
    assert_eq!(received.native_audio_data.as_deref(), Some(&offer));
    assert!(
        android_link
            .send_native_audio_data_offer(&route, offer, 5000)
            .is_err()
    );
}

#[test]
fn peer_cannot_advertise_the_other_nodes_audio_endpoint() {
    use capyio_core::{AuthorizationState, StreamId};
    use capyio_protocol::{
        NativeAudioDataOffer, NativeAudioEndpoint, encode_route_status_with_native_audio_data,
    };
    let (mut receiver, mut sender, _, _) = connected_raw_pair();
    let lab = capyio_testkit::DemoLab::new().unwrap();
    let mut route = lab
        .runtime
        .route(lab.routes.phone_microphone_to_windows)
        .unwrap()
        .clone();
    route.session_id = sender.binding().session_id;
    route.source.node_id = receiver.peer_node_id();
    route.backend = RouteBackend::AdapterManaged;
    route.state = RouteState::Starting;
    route.epoch = 1;
    route.authorization = AuthorizationState::Authorized {
        expires_at_ms: Some(5000),
    };
    let offer = NativeAudioDataOffer {
        port: 40211,
        stream_id: StreamId::new(),
        endpoint: NativeAudioEndpoint::Sink,
        preset: capyio_protocol::NativeAudioPreset::PcmS16le48000Mono,
    };
    sender
        .send_envelope(
            &encode_route_status_with_native_audio_data(route.session_id, &route, offer).unwrap(),
        )
        .unwrap();
    assert!(matches!(
        receiver.receive_control_message(),
        Err(CatalogLinkError::InvalidPeerRoute)
    ));
    assert!(receiver.is_closed());
}

#[test]
fn heartbeat_request_and_acknowledgement_share_the_authenticated_session() {
    let (mut server, mut client, _, _) = connected_pair();
    client.send_heartbeat(1, false).unwrap();
    let request = server.receive_heartbeat().unwrap();
    assert_eq!(request.sequence, 1);
    assert!(!request.acknowledgement);
    server.send_heartbeat(request.sequence, true).unwrap();
    let acknowledgement = client.receive_heartbeat().unwrap();
    assert_eq!(acknowledgement.sequence, 1);
    assert!(acknowledgement.acknowledgement);
}

#[test]
fn display_start_and_stop_use_the_existing_paired_node_connection() {
    use capyio_protocol::v1::{
        DisplaySessionAction, DisplaySessionControl, DisplayStreamMode, DisplayStreamSelection,
    };
    let (mut server, mut client, _, _) = connected_pair();
    let request = DisplaySessionControl {
        operation_id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc".into(),
        action: DisplaySessionAction::Start as i32,
        selection: Some(DisplayStreamSelection {
            mode: Some(DisplayStreamMode {
                width: 1920,
                height: 1080,
                refresh_millihertz: 60000,
            }),
            bitrate_kbps: 15000,
        }),
        ..Default::default()
    };
    client
        .send_display_session_control(request.clone())
        .unwrap();
    match server.receive_control_message().unwrap() {
        ReceivedControlMessage::DisplaySessionControl(received) => assert_eq!(received, request),
        _ => panic!("expected display request"),
    }
    let stopped = DisplaySessionControl {
        action: DisplaySessionAction::Stop as i32,
        ..request
    };
    server
        .send_display_session_control(stopped.clone())
        .unwrap();
    match client.receive_control_message().unwrap() {
        ReceivedControlMessage::DisplaySessionControl(received) => assert_eq!(received, stopped),
        _ => panic!("expected display stop"),
    }
    client.send_heartbeat(1, false).unwrap();
    assert_eq!(server.receive_heartbeat().unwrap().sequence, 1);
}

#[test]
fn catalog_update_can_be_dispatched_while_a_heartbeat_acknowledgement_is_pending() {
    let (mut server, mut client, windows, mut android) = connected_pair();
    let selected = windows
        .snapshot()
        .local_node
        .capabilities
        .keys()
        .copied()
        .take(1)
        .collect();
    client.send_heartbeat(9, false).unwrap();
    let request = server.receive_heartbeat().unwrap();
    server
        .send_catalog(server.prepare_catalog(&windows, &selected).unwrap())
        .unwrap();
    server.send_heartbeat(request.sequence, true).unwrap();

    let catalog = match client.receive_control_message().unwrap() {
        ReceivedControlMessage::Catalog(catalog) => catalog,
        _ => panic!("catalog must retain wire order"),
    };
    client.apply_catalog(&mut android, catalog).unwrap();
    let acknowledgement = match client.receive_control_message().unwrap() {
        ReceivedControlMessage::Heartbeat(heartbeat) => heartbeat,
        _ => panic!("heartbeat acknowledgement must follow"),
    };
    assert_eq!(acknowledgement.sequence, 9);
    assert!(acknowledgement.acknowledgement);
}

#[test]
fn authenticated_route_authorization_lease_and_start_round_trip() {
    let (mut windows_link, mut android_link, mut windows, mut android) = connected_pair();
    let android_node = android.snapshot().local_node;
    let windows_node = windows.snapshot().local_node;
    let (source_capability, source_port) = android_node
        .capabilities
        .values()
        .flat_map(|capability| {
            capability
                .ports
                .values()
                .map(move |port| (capability.id, port))
        })
        .find(|(_, port)| {
            port.direction == PortDirection::Source && port.profile.name == "capyio.audio.frames"
        })
        .unwrap();
    let (sink_capability, sink_port) = windows_node
        .capabilities
        .values()
        .flat_map(|capability| {
            capability
                .ports
                .values()
                .map(move |port| (capability.id, port))
        })
        .find(|(_, port)| {
            port.direction == PortDirection::Sink
                && port.profile == source_port.profile
                && port
                    .formats
                    .iter()
                    .any(|format| source_port.formats.contains(format))
        })
        .unwrap();
    let source = PortRef {
        node_id: android_node.id,
        capability_id: source_capability,
        port_id: source_port.id,
    };
    let sink = PortRef {
        node_id: windows_node.id,
        capability_id: sink_capability,
        port_id: sink_port.id,
    };
    let route_id = windows
        .create_route(
            windows_link.session_id(),
            source,
            sink,
            RouteBackend::CapyDataPlane,
        )
        .unwrap();
    windows_link
        .send_route_status(windows.route(route_id).unwrap())
        .unwrap();

    let proposal = match android_link.receive_control_message().unwrap() {
        ReceivedControlMessage::RouteStatus(status) => status,
        _ => panic!("expected Route proposal"),
    };
    let accepted = android_link
        .apply_route_status(&mut android, &proposal, true)
        .unwrap();
    android_link.send_route_status(&accepted).unwrap();
    let accepted = match windows_link.receive_control_message().unwrap() {
        ReceivedControlMessage::RouteStatus(status) => status,
        _ => panic!("expected Route authorization"),
    };
    windows_link
        .apply_route_status(&mut windows, &accepted, true)
        .unwrap();

    let request_id = windows_link
        .send_port_lease_request(
            route_id,
            source,
            PortLeaseAction::Acquire,
            MIN_PORT_LEASE_TTL_MS,
        )
        .unwrap();
    let request = match android_link.receive_control_message().unwrap() {
        ReceivedControlMessage::PortLeaseRequest(request) => request,
        _ => panic!("expected lease request"),
    };
    android_link
        .answer_port_lease_request(&mut android, request, 1_000)
        .unwrap();
    let response = match windows_link.receive_control_message().unwrap() {
        ReceivedControlMessage::PortLeaseResponse(response) => response,
        _ => panic!("expected lease response"),
    };
    assert_eq!(response.request_message_id, request_id);
    assert_eq!(response.outcome, PortLeaseOutcome::Acquired);

    windows_link
        .send_route_command(route_id, RouteCommandAction::Start)
        .unwrap();
    let command = match android_link.receive_control_message().unwrap() {
        ReceivedControlMessage::RouteCommand(command) => command,
        _ => panic!("expected Route start"),
    };
    let staged = android_link
        .stage_route_command(&mut android, &command, 1_001)
        .unwrap();
    assert_eq!(staged.state, RouteState::Starting);
    let repeated = android_link
        .stage_route_command(&mut android, &command, 1_001)
        .unwrap();
    assert_eq!(repeated.epoch, staged.epoch);
    android.activate_route(route_id).unwrap();
    let active = android_link
        .apply_route_command(&mut android, &command, 1_001)
        .unwrap();
    assert_eq!(active.state, RouteState::Active);

    let pending_id = android
        .create_route(
            android_link.session_id(),
            source,
            sink,
            RouteBackend::CapyDataPlane,
        )
        .unwrap();
    windows_link
        .send_route_command(pending_id, RouteCommandAction::Stop)
        .unwrap();
    let cancel = match android_link.receive_control_message().unwrap() {
        ReceivedControlMessage::RouteCommand(command) => command,
        _ => panic!("expected cancellation"),
    };
    let cancelled = android_link
        .stage_route_command(&mut android, &cancel, 1_002)
        .unwrap();
    assert_eq!(
        cancelled.authorization,
        capyio_core::AuthorizationState::Denied
    );
    assert!(android.authorize_route(pending_id, Some(10_000)).is_err());
    assert!(
        android_link
            .stage_route_command(&mut android, &cancel, 1_003)
            .is_ok()
    );
}

fn fresh_raw_pair() -> (
    AuthenticatedControlServerChannel,
    AuthenticatedControlClientChannel,
    NodeDescriptor,
    NodeDescriptor,
) {
    let windows = windows_node();
    let android = android_node();
    let nodes = ControlTlsNodeBinding::new(windows.id, android.id).unwrap();
    let (server_identity, client_identity) = identities();
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = AuthenticatedControlServer::from_listener_fresh_session(
        listener,
        server_identity,
        nodes,
        Duration::from_secs(2),
    )
    .unwrap();
    let worker = thread::spawn(move || server.accept().unwrap());
    let client = AuthenticatedControlClientChannel::connect_fresh_session(
        ControlTlsEndpoint {
            address,
            io_timeout: Duration::from_secs(2),
        },
        "localhost",
        client_identity,
        nodes,
    )
    .unwrap();
    (worker.join().unwrap(), client, windows, android)
}

fn connected_raw_pair() -> (
    CatalogControlLink,
    AuthenticatedControlClientChannel,
    NodeRuntime,
    NodeRuntime,
) {
    let windows = windows_node();
    let android = android_node();
    let session = SessionId::new();
    let binding = ControlTlsBinding::new(session, windows.id, android.id).unwrap();
    let (server_identity, client_identity) = identities();
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = AuthenticatedControlServer::from_listener(
        listener,
        server_identity,
        binding,
        Duration::from_secs(2),
    )
    .unwrap();
    let worker = thread::spawn(move || CatalogControlLink::from_accepted(server.accept().unwrap()));
    let client = AuthenticatedControlClientChannel::connect(
        ControlTlsEndpoint {
            address,
            io_timeout: Duration::from_secs(2),
        },
        "localhost",
        client_identity,
        binding,
    )
    .unwrap();
    (
        worker.join().unwrap(),
        client,
        runtime(windows.clone(), android.clone(), session),
        runtime(android, windows, session),
    )
}

#[test]
fn fresh_authenticated_pair_is_transactionally_admitted_into_both_runtimes() {
    let (server, client, windows, android) = fresh_raw_pair();
    let binding = client.binding();
    assert_eq!(server.binding(), binding);
    let mut windows_runtime = NodeRuntime::new(windows.clone()).unwrap();
    let mut android_runtime = NodeRuntime::new(android.clone()).unwrap();

    let mut windows_link = CatalogControlLink::admit_accepted_new_peer(
        &mut windows_runtime,
        server,
        android.display_name.clone(),
    )
    .unwrap();
    let mut android_link = CatalogControlLink::admit_dialed_new_peer(
        &mut android_runtime,
        client,
        windows.display_name.clone(),
    )
    .unwrap();

    assert!(!windows_link.is_closed());
    assert!(!android_link.is_closed());
    for (runtime, local, peer) in [
        (&windows_runtime, windows.id, android.id),
        (&android_runtime, android.id, windows.id),
    ] {
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.local_node.id, local);
        assert_eq!(snapshot.peers.len(), 1);
        assert_eq!(snapshot.peers[0].id, peer);
        assert!(snapshot.peers[0].adapter_instances.is_empty());
        assert!(snapshot.peers[0].capabilities.is_empty());
        assert!(matches!(
            snapshot.peers[0].platform,
            capyio_core::Platform::Unknown(ref value) if value == "pending_authenticated_catalog"
        ));
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].id, binding.session_id);
        assert_eq!(snapshot.sessions[0].remote_node_id, peer);
    }

    windows_link
        .close_and_reconcile(&mut windows_runtime)
        .unwrap();
    android_link
        .close_and_reconcile(&mut android_runtime)
        .unwrap();
    assert!(windows_link.is_closed());
    assert!(android_link.is_closed());
    assert_eq!(
        windows_runtime.snapshot().sessions[0].state,
        capyio_core::SessionState::Closed
    );
    assert_eq!(
        android_runtime.snapshot().sessions[0].state,
        capyio_core::SessionState::Closed
    );
}

#[test]
fn closed_peer_sessions_can_reconnect_but_live_duplicates_cannot() {
    let (server, client, windows, android) = fresh_raw_pair();
    let mut windows_runtime = NodeRuntime::new(windows.clone()).unwrap();
    let mut android_runtime = NodeRuntime::new(android.clone()).unwrap();
    let mut windows_link = CatalogControlLink::admit_accepted_peer(
        &mut windows_runtime,
        server,
        android.display_name.clone(),
    )
    .unwrap();
    let mut android_link = CatalogControlLink::admit_dialed_peer(
        &mut android_runtime,
        client,
        windows.display_name.clone(),
    )
    .unwrap();
    windows_link
        .close_and_reconcile(&mut windows_runtime)
        .unwrap();
    android_link
        .close_and_reconcile(&mut android_runtime)
        .unwrap();

    let nodes = ControlTlsNodeBinding::new(windows.id, android.id).unwrap();
    let (server_identity, client_identity) = identities();
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = AuthenticatedControlServer::from_listener_fresh_session(
        listener,
        server_identity,
        nodes,
        Duration::from_secs(2),
    )
    .unwrap();
    let worker = thread::spawn(move || server.accept().unwrap());
    let client = AuthenticatedControlClientChannel::connect_fresh_session(
        ControlTlsEndpoint {
            address,
            io_timeout: Duration::from_secs(2),
        },
        "localhost",
        client_identity,
        nodes,
    )
    .unwrap();
    let server = worker.join().unwrap();
    let reconnected_windows =
        CatalogControlLink::admit_accepted_peer(&mut windows_runtime, server, android.display_name)
            .unwrap();
    let reconnected_android =
        CatalogControlLink::admit_dialed_peer(&mut android_runtime, client, windows.display_name)
            .unwrap();
    assert!(!reconnected_windows.is_closed());
    assert!(!reconnected_android.is_closed());
    assert_eq!(
        windows_runtime
            .snapshot()
            .sessions
            .iter()
            .filter(|session| session.state == capyio_core::SessionState::Ready)
            .count(),
        1
    );
}

#[test]
fn fresh_admission_rejects_wrong_or_existing_peer_without_runtime_mutation() {
    let (server, client, windows, android) = fresh_raw_pair();
    let mut wrong_runtime = NodeRuntime::new(android.clone()).unwrap();
    let before = wrong_runtime.snapshot();
    assert!(matches!(
        CatalogControlLink::admit_accepted_new_peer(
            &mut wrong_runtime,
            server,
            android.display_name.clone()
        ),
        Err(CatalogLinkError::BindingMismatch)
    ));
    assert_eq!(wrong_runtime.snapshot(), before);

    let mut android_runtime = NodeRuntime::new(android).unwrap();
    android_runtime
        .register_peer(windows.clone(), true)
        .unwrap();
    let before = android_runtime.snapshot();
    assert!(matches!(
        CatalogControlLink::admit_dialed_new_peer(
            &mut android_runtime,
            client,
            windows.display_name
        ),
        Err(CatalogLinkError::PeerAlreadyPresent)
    ));
    assert_eq!(android_runtime.snapshot(), before);
}

#[test]
fn authenticated_wrong_session_or_node_catalog_closes_without_runtime_mutation() {
    for wrong_session in [false, true] {
        let (mut receiver, mut sender, windows, android) = connected_raw_pair();
        let before = windows.snapshot();
        let session = if wrong_session {
            SessionId::new()
        } else {
            sender.binding().session_id
        };
        let descriptor = if wrong_session {
            android.snapshot().local_node
        } else {
            windows.snapshot().local_node
        };
        let bytes = capyio_protocol::encode_catalog_snapshot(session, &descriptor).unwrap();
        sender.send_envelope(&bytes).unwrap();
        assert!(matches!(
            receiver.receive_catalog(),
            Err(CatalogLinkError::InvalidPeerCatalog)
        ));
        assert!(receiver.is_closed());
        assert_eq!(
            windows.snapshot().peers[0].capabilities,
            before.peers[0].capabilities
        );
    }
}

#[test]
fn duplicate_catalog_is_rejected_without_reapplying_it() {
    let (mut receiver, mut sender, mut windows, android) = connected_raw_pair();
    let bytes = capyio_protocol::encode_catalog_snapshot(
        sender.binding().session_id,
        &android.snapshot().local_node,
    )
    .unwrap();
    sender.send_envelope(&bytes).unwrap();
    let incoming = receiver.receive_catalog().unwrap();
    receiver.apply_catalog(&mut windows, incoming).unwrap();
    let before = windows.snapshot();
    sender.send_envelope(&bytes).unwrap();
    let repeated = receiver.receive_catalog().unwrap();
    assert!(matches!(
        receiver.apply_catalog(&mut windows, repeated),
        Err(CatalogLinkError::Admission(_))
    ));
    assert_eq!(windows.snapshot().events.len(), before.events.len());
    assert!(receiver.is_closed());
}

#[test]
fn both_nodes_exchange_selected_catalogs_over_real_mtls_without_starting_routes() {
    let (mut windows_link, mut android_link, mut windows, mut android) = connected_pair();
    let selected = BTreeSet::new();
    let windows_expected =
        project_publication_catalog(&windows.snapshot().local_node, &selected).unwrap();
    let android_expected =
        project_publication_catalog(&android.snapshot().local_node, &selected).unwrap();
    let outbound = android_link.prepare_catalog(&android, &selected).unwrap();
    let worker = thread::spawn(move || {
        let incoming = windows_link.receive_catalog().unwrap();
        windows_link.apply_catalog(&mut windows, incoming).unwrap();
        let outbound = windows_link.prepare_catalog(&windows, &selected).unwrap();
        windows_link.send_catalog(outbound).unwrap();
        windows
    });
    android_link.send_catalog(outbound).unwrap();
    let incoming = android_link.receive_catalog().unwrap();
    android_link.apply_catalog(&mut android, incoming).unwrap();
    let windows = worker.join().unwrap();
    assert_eq!(
        windows.snapshot().peers[0].capabilities,
        android_expected.capabilities
    );
    assert_eq!(
        android.snapshot().peers[0].capabilities,
        windows_expected.capabilities
    );
    assert!(windows.snapshot().routes.is_empty());
    assert!(android.snapshot().routes.is_empty());
    assert!(windows.snapshot().port_leases.is_empty());
    assert!(android.snapshot().port_leases.is_empty());
}

#[test]
fn peer_loss_between_receive_and_apply_preserves_catalog_and_closes_link() {
    let (mut windows_link, mut android_link, mut windows, android) = connected_pair();
    let before = windows.snapshot().peers[0].capabilities.clone();
    android_link
        .send_catalog(
            android_link
                .prepare_catalog(&android, &BTreeSet::new())
                .unwrap(),
        )
        .unwrap();
    let incoming = windows_link.receive_catalog().unwrap();
    windows
        .set_peer_online(android.snapshot().local_node.id, false)
        .unwrap();
    assert!(matches!(
        windows_link.apply_catalog(&mut windows, incoming),
        Err(CatalogLinkError::SessionNotReady)
    ));
    assert_eq!(windows.snapshot().peers[0].capabilities, before);
    assert!(windows_link.is_closed());
}

#[test]
fn wrong_runtime_and_unknown_selection_are_rejected_before_network_io() {
    let (windows_link, _android_link, windows, android) = connected_pair();
    assert!(matches!(
        windows_link.prepare_catalog(&android, &BTreeSet::new()),
        Err(CatalogLinkError::BindingMismatch)
    ));
    assert!(matches!(
        windows_link.prepare_catalog(
            &windows,
            &BTreeSet::from([capyio_core::CapabilityId::new()])
        ),
        Err(CatalogLinkError::Publication(_))
    ));
    assert!(!windows_link.is_closed());
}

#[test]
fn explicit_close_rejects_pending_catalog_and_further_receives() {
    let (mut windows_link, _android_link, windows, _) = connected_pair();
    let pending = windows_link
        .prepare_catalog(&windows, &BTreeSet::new())
        .unwrap();
    windows_link.close();
    windows_link.close();
    assert!(matches!(
        windows_link.send_catalog(pending),
        Err(CatalogLinkError::Closed)
    ));
    assert!(matches!(
        windows_link.receive_catalog(),
        Err(CatalogLinkError::Closed)
    ));
}

#[test]
fn publication_changes_replace_the_remote_catalog_on_the_same_authenticated_link() {
    let (mut windows_link, mut android_link, mut windows, android) = connected_pair();
    let local = android.snapshot().local_node;
    let source = local
        .capabilities
        .values()
        .find(|capability| {
            capability
                .ports
                .values()
                .any(|port| port.direction == capyio_core::PortDirection::Source)
        })
        .unwrap()
        .id;
    for selected in [BTreeSet::from([source]), BTreeSet::new()] {
        let expected = project_publication_catalog(&local, &selected).unwrap();
        let outbound = android_link.prepare_catalog(&android, &selected).unwrap();
        android_link.send_catalog(outbound).unwrap();
        let incoming = windows_link.receive_catalog().unwrap();
        windows_link.apply_catalog(&mut windows, incoming).unwrap();
        assert_eq!(
            windows.snapshot().peers[0].capabilities,
            expected.capabilities
        );
        assert_eq!(
            windows.snapshot().peers[0]
                .capabilities
                .contains_key(&source),
            selected.contains(&source)
        );
        assert!(windows.snapshot().routes.is_empty());
    }
}

#[test]
fn prepared_and_received_records_cannot_cross_connections() {
    let (mut first, mut sender, mut runtime, sender_runtime) = connected_pair();
    let (mut second, _other_sender, other_runtime, _) = connected_pair();
    let outbound = first.prepare_catalog(&runtime, &BTreeSet::new()).unwrap();
    assert!(matches!(
        second.send_catalog(outbound),
        Err(CatalogLinkError::BindingMismatch)
    ));
    sender
        .send_catalog(
            sender
                .prepare_catalog(&sender_runtime, &BTreeSet::new())
                .unwrap(),
        )
        .unwrap();
    let incoming = first.receive_catalog().unwrap();
    assert!(matches!(
        second.apply_catalog(&mut runtime, incoming),
        Err(CatalogLinkError::BindingMismatch)
    ));
    assert!(
        second
            .prepare_catalog(&other_runtime, &BTreeSet::new())
            .is_ok()
    );
}

fn identities() -> (ControlTlsServerIdentity, ControlTlsClientIdentity) {
    let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];
    let key = KeyPair::generate().unwrap();
    let ca = params.self_signed(&key).unwrap().der().to_vec();
    let issuer = Issuer::new(params, key);
    let leaf = |name: &str, usage| {
        let mut params = CertificateParams::new(vec![name.into()]).unwrap();
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![usage];
        let key = KeyPair::generate().unwrap();
        (
            params.signed_by(&key, &issuer).unwrap().der().to_vec(),
            key.serialize_der(),
        )
    };
    let (server, server_key) = leaf("localhost", ExtendedKeyUsagePurpose::ServerAuth);
    let (client, client_key) = leaf("capyio-test-client", ExtendedKeyUsagePurpose::ClientAuth);
    let server_fingerprint =
        capyio_control_tls::CertificateFingerprint::from_certificate_der(&server).unwrap();
    let client_fingerprint =
        capyio_control_tls::CertificateFingerprint::from_certificate_der(&client).unwrap();
    (
        ControlTlsServerIdentity {
            expected_peer_certificate: client_fingerprint,
            certificate_chain_der: vec![server, ca.clone()],
            private_key_der: server_key,
            client_ca_der: ca.clone(),
        },
        ControlTlsClientIdentity {
            expected_peer_certificate: server_fingerprint,
            certificate_chain_der: vec![client, ca.clone()],
            private_key_der: client_key,
            server_ca_der: ca,
        },
    )
}

#[test]
fn secure_camera_offer_requires_exact_session_receiver_and_live_grant() {
    use capyio_core::{AuthorizationState, FormatDescriptor, ProfileId, StreamId};
    use capyio_protocol::{SecureCameraDataOffer, SecureCameraTransport};
    use std::time::Instant;
    let (mut sink, mut source, _, _) = connected_pair();
    let lab = capyio_testkit::DemoLab::new().unwrap();
    let mut route = lab
        .runtime
        .route(lab.routes.phone_microphone_to_windows)
        .unwrap()
        .clone();
    route.session_id = sink.session_id();
    route.source.node_id = sink.peer_node_id();
    route.sink.node_id = source.peer_node_id();
    route.profile = ProfileId::video_frames_v1();
    route.backend = RouteBackend::AdapterManaged;
    route.selected_format = Some(FormatDescriptor::new("cavc-avc-access-unit-v1"));
    route.compatible_formats = vec![route.selected_format.clone().unwrap()];
    route.state = RouteState::Starting;
    route.epoch = 3;
    route.authorization = AuthorizationState::Authorized {
        expires_at_ms: Some(5000),
    };
    let offer = SecureCameraDataOffer {
        port: 40211,
        stream_id: StreamId::new(),
        transport: SecureCameraTransport::Tls13MtlsV1,
    };
    let deadline = || Instant::now() + Duration::from_secs(2);
    for case in 0..5 {
        let mut bad = route.clone();
        match case {
            0 => bad.session_id = SessionId::new(),
            1 => bad.sink.node_id = capyio_core::NodeId::new(),
            2 => bad.source.node_id = capyio_core::NodeId::new(),
            3 => {
                bad.authorization = AuthorizationState::Authorized {
                    expires_at_ms: Some(1),
                }
            }
            4 => {
                bad.authorization = AuthorizationState::Authorized {
                    expires_at_ms: None,
                }
            }
            _ => unreachable!(),
        }
        assert!(
            sink.send_secure_camera_data_offer_until(&bad, offer, 1, deadline())
                .is_err(),
            "case {case}"
        );
    }
    assert!(
        source
            .send_secure_camera_data_offer_until(&route, offer, 1, deadline())
            .is_err()
    );
    sink.send_secure_camera_data_offer_until(&route, offer, 1, deadline())
        .unwrap();
    let ReceivedControlMessage::RouteStatus(received) =
        source.receive_control_message_until(deadline()).unwrap()
    else {
        panic!("expected secure camera offer")
    };
    assert_eq!(received.secure_camera_data, Some(offer));
    assert!(received.camera_data.is_none());
}

#[test]
fn authenticated_address_supports_both_roles_without_weakening_accepted_contract() {
    let (accepted, dialed, _, _) = connected_pair();
    assert!(
        accepted
            .authenticated_peer_addr()
            .unwrap()
            .ip()
            .is_loopback()
    );
    assert!(dialed.authenticated_peer_addr().unwrap().ip().is_loopback());
    assert_eq!(
        accepted.authenticated_peer_addr().unwrap(),
        accepted.accepted_peer_addr().unwrap()
    );
    assert!(matches!(
        dialed.accepted_peer_addr(),
        Err(CatalogLinkError::BindingMismatch)
    ));
}

// Hosted-only: these tests open actual synthetic loopback pinned TLS sockets.
fn secure_audio_fixture(
    sink: &CatalogControlLink,
    source: &CatalogControlLink,
) -> (capyio_core::Route, capyio_protocol::SecureAudioDataOffer) {
    use capyio_core::{AuthorizationState, FormatDescriptor, ProfileId, QosMode, StreamId};
    let lab = capyio_testkit::DemoLab::new().unwrap();
    let mut route = lab
        .runtime
        .route(lab.routes.phone_microphone_to_windows)
        .unwrap()
        .clone();
    route.session_id = sink.session_id();
    route.source.node_id = sink.peer_node_id();
    route.sink.node_id = source.peer_node_id();
    route.profile = ProfileId::audio_frames_v1();
    route.backend = RouteBackend::AdapterManaged;
    route.selected_format = Some(FormatDescriptor::new(
        capyio_protocol::SECURE_AUDIO_MONO_FORMAT,
    ));
    route.compatible_formats = vec![route.selected_format.clone().unwrap()];
    route.compatible_qos_modes = [QosMode::Interactive].into_iter().collect();
    route.selected_qos_mode = Some(QosMode::Interactive);
    route.state = RouteState::Starting;
    route.epoch = 3;
    route.authorization = AuthorizationState::Authorized {
        expires_at_ms: Some(5000),
    };
    (
        route,
        capyio_protocol::SecureAudioDataOffer {
            port: 40211,
            stream_id: StreamId::new(),
            transport: capyio_protocol::SecureAudioTransport::Tls13MtlsV1,
        },
    )
}

#[test]
fn ipc_secure_audio_offer_requires_exact_session_receiver_and_live_grant() {
    use capyio_core::{AuthorizationState, FormatDescriptor, QosMode};
    use std::time::Instant;
    let (mut sink, mut source, _, _) = connected_pair();
    let (route, offer) = secure_audio_fixture(&sink, &source);
    let deadline = || Instant::now() + Duration::from_secs(2);
    assert!(
        source
            .take_received_secure_audio_offer(&route, offer)
            .is_err()
    );
    for case in 0..8 {
        let mut bad = route.clone();
        match case {
            0 => bad.session_id = SessionId::new(),
            1 => bad.sink.node_id = capyio_core::NodeId::new(),
            2 => bad.source.node_id = capyio_core::NodeId::new(),
            3 => {
                bad.authorization = AuthorizationState::Authorized {
                    expires_at_ms: Some(1),
                }
            }
            4 => {
                bad.authorization = AuthorizationState::Authorized {
                    expires_at_ms: None,
                }
            }
            5 => bad.selected_format = Some(FormatDescriptor::new("pcm-s16le-48000-mono")),
            6 => bad.selected_qos_mode = Some(QosMode::Basic),
            _ => bad
                .selected_format
                .as_mut()
                .unwrap()
                .parameters
                .insert("extra".into(), "1".into())
                .map(|_| ())
                .unwrap_or(()),
        }
        assert!(
            sink.send_secure_audio_data_offer_until(&bad, offer, 1, deadline())
                .is_err(),
            "case {case}"
        );
    }
    assert!(
        source
            .send_secure_audio_data_offer_until(&route, offer, 1, deadline())
            .is_err()
    );
    sink.send_secure_audio_data_offer_until(&route, offer, 1, deadline())
        .unwrap();
    let ReceivedControlMessage::RouteStatus(received) =
        source.receive_control_message_until(deadline()).unwrap()
    else {
        panic!()
    };
    assert_eq!(received.secure_audio_data, Some(offer));
    assert!(received.native_audio_data.is_none());
    source
        .take_received_secure_audio_offer(&route, offer)
        .unwrap();
    assert!(
        source
            .take_received_secure_audio_offer(&route, offer)
            .is_err()
    );
    // A repeated authenticated offer cannot replenish consumed receipt authority.
    sink.send_secure_audio_data_offer_until(&route, offer, 1, deadline())
        .unwrap();
    assert!(source.receive_control_message_until(deadline()).is_err());
    assert!(
        source
            .take_received_secure_audio_offer(&route, offer)
            .is_err()
    );
}

#[test]
fn ipc_secure_audio_receipt_rejects_changed_binding_and_consumes_failed_attempt() {
    use std::time::Instant;
    for case in 0..7 {
        let (mut sink, mut source, _, _) = connected_pair();
        let (route, offer) = secure_audio_fixture(&sink, &source);
        let deadline = || Instant::now() + Duration::from_secs(2);
        sink.send_secure_audio_data_offer_until(&route, offer, 1, deadline())
            .unwrap();
        source.receive_control_message_until(deadline()).unwrap();
        let mut bad = route.clone();
        let mut bad_offer = offer;
        match case {
            0 => bad.epoch += 1,
            1 => bad.source.port_id = capyio_core::PortId::new(),
            2 => bad.sink.capability_id = capyio_core::CapabilityId::new(),
            3 => {
                bad.authorization = capyio_core::AuthorizationState::Authorized {
                    expires_at_ms: Some(4999),
                }
            }
            4 => bad_offer.port += 1,
            5 => bad_offer.stream_id = capyio_core::StreamId::new(),
            _ => bad
                .compatible_formats
                .push(capyio_core::FormatDescriptor::new("other")),
        }
        assert!(
            source
                .take_received_secure_audio_offer(&bad, bad_offer)
                .is_err()
        );
        assert!(
            source
                .take_received_secure_audio_offer(&route, offer)
                .is_err()
        );
    }
}

#[test]
fn ipc_secure_audio_received_source_owned_offer_closes_control_link() {
    let (mut server, mut client, _, _) = connected_raw_pair();
    let lab = capyio_testkit::DemoLab::new().unwrap();
    let mut route = lab
        .runtime
        .route(lab.routes.phone_microphone_to_windows)
        .unwrap()
        .clone();
    route.session_id = server.session_id();
    // The raw authenticated client is Source, so it cannot advertise the Sink.
    route.source.node_id = server.peer_node_id();
    route.sink.node_id = server.tls_binding().server_node_id;
    route.backend = RouteBackend::AdapterManaged;
    route.state = RouteState::Starting;
    route.epoch = 1;
    route.authorization = capyio_core::AuthorizationState::Authorized {
        expires_at_ms: Some(5000),
    };
    route.selected_format = Some(capyio_core::FormatDescriptor::new(
        capyio_protocol::SECURE_AUDIO_MONO_FORMAT,
    ));
    route.compatible_formats = vec![route.selected_format.clone().unwrap()];
    route.selected_qos_mode = Some(capyio_core::QosMode::Interactive);
    route.compatible_qos_modes = [capyio_core::QosMode::Interactive].into_iter().collect();
    let offer = capyio_protocol::SecureAudioDataOffer {
        port: 40211,
        stream_id: capyio_core::StreamId::new(),
        transport: capyio_protocol::SecureAudioTransport::Tls13MtlsV1,
    };
    client
        .send_envelope(
            &capyio_protocol::encode_route_status_with_secure_audio_data(
                route.session_id,
                &route,
                offer,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(server.receive_control_message().is_err());
    assert!(
        server
            .take_received_secure_audio_offer(&route, offer)
            .is_err()
    );
}
