use super::*;
use capyio_protocol::ValidatedPortLeaseResponse;

// Allow one bounded inbox worth of asynchronous status traffic before a reply.
// Four concurrently starting capabilities can exceed the old eight-message cap.
pub(super) const MAX_EXCHANGE_MESSAGES: usize = 32;

/// Bounded demultiplexing on the sole control owner. A peer can publish or
/// heartbeat while processing a lease; neither is the lease's response.
pub(super) fn receive_lease(
    link: &mut CatalogControlLink,
    node: &Arc<Mutex<DesktopNodeState>>,
    request: MessageId,
    route: RouteId,
    port: PortRef,
) -> Result<ValidatedPortLeaseResponse, String> {
    for _ in 0..MAX_EXCHANGE_MESSAGES {
        match link.receive_control_message().map_err(|e| e.to_string())? {
            ReceivedControlMessage::PortLeaseResponse(response) => {
                return correlate(response, request, route, port);
            }
            ReceivedControlMessage::Catalog(catalog) => {
                let mut state = node.lock().map_err(|_| "Node state lock poisoned")?;
                link.apply_catalog(&mut state.runtime, catalog)
                    .map_err(|e| e.to_string())?;
            }
            ReceivedControlMessage::Heartbeat(heartbeat) if !heartbeat.acknowledgement => {
                link.send_heartbeat(heartbeat.sequence, true)
                    .map_err(|e| e.to_string())?;
            }
            ReceivedControlMessage::ControllerSettingsState(value) => {
                controller_settings::received(
                    &link.peer_node_id().to_string(),
                    &link.session_id().to_string(),
                    value,
                );
            }
            ReceivedControlMessage::RouteStatus(status) => {
                apply_intervening_route(link, node, &status)?;
            }
            ReceivedControlMessage::DisplaySessionControl(value) => {
                display_workflow::defer_message(link.session_id(), value)?;
            }
            _ => return Err("peer returned an unexpected lease control message".into()),
        }
    }
    Err("peer lease response limit exceeded".into())
}

pub(super) fn apply_intervening_route(
    link: &mut CatalogControlLink,
    node: &Arc<Mutex<DesktopNodeState>>,
    status: &capyio_protocol::ValidatedRouteStatus,
) -> Result<(), String> {
    // The audio owner must preserve endpoint offers and handle new reverse
    // proposals itself after this bounded exchange, including unknown Routes.
    if native_audio_route::defer(status) {
        return Ok(());
    }
    if camera_route::defer(status) {
        return Ok(());
    }
    if display_workflow::defer_status(status) {
        return Ok(());
    }
    let mut state = node.lock().map_err(|_| "Node lock poisoned")?;
    let existing = state
        .runtime
        .route(status.route.id)
        .map_err(|e| e.to_string())?;
    if existing.session_id != link.session_id() {
        return Err("unrelated Route belongs to another Session".into());
    }
    link.apply_route_status(&mut state.runtime, status, false)
        .map_err(|e| e.to_string())?;
    if matches!(
        status.route.state,
        RouteState::Stopped | RouteState::Failed | RouteState::Offline
    ) || status.route.authorization == AuthorizationState::Denied
    {
        let route = state
            .runtime
            .route(status.route.id)
            .map_err(|e| e.to_string())?;
        if route.state == RouteState::Draft && route.authorization == AuthorizationState::Pending {
            state
                .runtime
                .deny_pending_route(status.route.id)
                .map_err(|e| e.to_string())?;
        } else if !matches!(
            route.state,
            RouteState::Stopped | RouteState::Failed | RouteState::Offline
        ) {
            state
                .runtime
                .set_route_active(status.route.id, false, unix_now_ms()?)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn correlate(
    response: ValidatedPortLeaseResponse,
    request: MessageId,
    route: RouteId,
    port: PortRef,
) -> Result<ValidatedPortLeaseResponse, String> {
    if response.request_message_id != request || response.route_id != route || response.port != port
    {
        return Err("peer returned a mismatched lease response".into());
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_control_tls::{
        AuthenticatedControlClientChannel, AuthenticatedControlServer, CertificateFingerprint,
        ControlTlsBinding, ControlTlsClientIdentity, ControlTlsEndpoint, ControlTlsServerIdentity,
    };
    use capyio_core::{CapabilityId, NodeDescriptor, NodeId, PortId};
    use capyio_protocol::{
        PortLeaseOutcome, decode_control_heartbeat, encode_catalog_snapshot,
        encode_control_heartbeat, encode_port_lease_response,
    };
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
        KeyUsagePurpose,
    };
    use std::net::Ipv4Addr;

    struct Loopback {
        link: CatalogControlLink,
        peer: AuthenticatedControlClientChannel,
        node: Arc<Mutex<DesktopNodeState>>,
        catalog: NodeDescriptor,
        session: SessionId,
        route: RouteId,
        port: PortRef,
    }

    fn identities() -> (ControlTlsServerIdentity, ControlTlsClientIdentity) {
        let mut params = CertificateParams::new(Vec::<String>::new()).expect("CA parameters");
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let key = KeyPair::generate().expect("CA key");
        let ca = params
            .self_signed(&key)
            .expect("CA certificate")
            .der()
            .to_vec();
        let issuer = Issuer::new(params, key);
        let leaf = |name: &str, usage| {
            let mut params =
                CertificateParams::new(vec![name.to_owned()]).expect("leaf parameters");
            params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
            params.extended_key_usages = vec![usage];
            let key = KeyPair::generate().expect("leaf key");
            (
                params
                    .signed_by(&key, &issuer)
                    .expect("leaf certificate")
                    .der()
                    .to_vec(),
                key.serialize_der(),
            )
        };
        let (server, server_key) = leaf("localhost", ExtendedKeyUsagePurpose::ServerAuth);
        let (client, client_key) = leaf("control-test-peer", ExtendedKeyUsagePurpose::ClientAuth);
        let server_pin = CertificateFingerprint::from_certificate_der(&server).expect("server pin");
        let client_pin = CertificateFingerprint::from_certificate_der(&client).expect("client pin");
        (
            ControlTlsServerIdentity {
                expected_peer_certificate: client_pin,
                certificate_chain_der: vec![server, ca.clone()],
                private_key_der: server_key,
                client_ca_der: ca.clone(),
            },
            ControlTlsClientIdentity {
                expected_peer_certificate: server_pin,
                certificate_chain_der: vec![client, ca.clone()],
                private_key_der: client_key,
                server_ca_der: ca,
            },
        )
    }

    fn connected() -> Loopback {
        let node = DesktopNodeState::demo().expect("desktop fixture");
        let snapshot = node.runtime.snapshot();
        let route = node
            .runtime
            .route(node.demo_routes.phone_microphone_to_windows)
            .expect("fixture Route");
        let catalog = snapshot
            .peers
            .iter()
            .find(|peer| peer.id == route.source.node_id)
            .expect("peer catalog")
            .clone();
        let (session, route_id, port) = (node.bootstrap_session_id, route.id, route.source);
        let binding =
            ControlTlsBinding::new(session, snapshot.local_node.id, catalog.id).expect("binding");
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("loopback listener");
        let address = listener.local_addr().expect("listener address");
        let (server_identity, client_identity) = identities();
        let server = AuthenticatedControlServer::from_listener(
            listener,
            server_identity,
            binding,
            Duration::from_secs(2),
        )
        .expect("TLS server");
        let accepted = thread::spawn(move || {
            CatalogControlLink::from_accepted(server.accept().expect("accept TLS"))
        });
        let peer = AuthenticatedControlClientChannel::connect(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(2),
            },
            "localhost",
            client_identity,
            binding,
        )
        .expect("connect TLS");
        Loopback {
            link: accepted.join().expect("accept worker"),
            peer,
            node: Arc::new(Mutex::new(node)),
            catalog,
            session,
            route: route_id,
            port,
        }
    }

    fn heartbeat(pair: &mut Loopback, sequence: u64, acknowledgement: bool) {
        pair.peer
            .send_envelope(
                &encode_control_heartbeat(pair.session, sequence, acknowledgement)
                    .expect("heartbeat encoding"),
            )
            .expect("send heartbeat");
    }

    fn response(
        pair: &mut Loopback,
        request: MessageId,
        route: RouteId,
        port: PortRef,
        outcome: PortLeaseOutcome,
    ) {
        let expiry = matches!(
            outcome,
            PortLeaseOutcome::Acquired | PortLeaseOutcome::Renewed
        )
        .then_some(5_000);
        pair.peer
            .send_envelope(
                &encode_port_lease_response(pair.session, request, route, port, outcome, expiry)
                    .expect("lease encoding"),
            )
            .expect("send lease reply");
    }

    fn assert_heartbeat_ack(pair: &mut Loopback, sequence: u64) {
        let acknowledgement = decode_control_heartbeat(
            &pair
                .peer
                .receive_envelope()
                .expect("receive heartbeat acknowledgement"),
        )
        .expect("heartbeat decoding");
        assert_eq!(acknowledgement.sequence, sequence);
        assert!(acknowledgement.acknowledgement);
    }

    #[test]
    fn controller_settings_do_not_interrupt_lease_acquisition() {
        let mut pair = connected();
        let peer_id = pair.catalog.id.to_string();
        let session_id = pair.session.to_string();
        controller_settings::connected(&peer_id, &session_id);
        let settings = capyio_protocol::new_envelope(
            Some(pair.session),
            capyio_protocol::v1::envelope::Payload::ControllerSettingsState(
                capyio_protocol::v1::ControllerSettingsState {
                    revision: 17,
                    motion_enabled: true,
                    haptics_strength: 65,
                    request_id: 0,
                    error: String::new(),
                },
            ),
        );
        pair.peer
            .send_envelope(&capyio_protocol::encode_envelope_checked(&settings).unwrap())
            .unwrap();
        let request = MessageId::new();
        let (route, port) = (pair.route, pair.port);
        response(&mut pair, request, route, port, PortLeaseOutcome::Acquired);
        assert_eq!(
            receive_lease(&mut pair.link, &pair.node, request, route, port)
                .unwrap()
                .outcome,
            PortLeaseOutcome::Acquired
        );
        let value = serde_json::to_value(
            controller_settings::get_remote_controller_settings(peer_id.clone()).unwrap(),
        )
        .unwrap();
        assert_eq!(value["revision"], 17);
        controller_settings::disconnected(&peer_id, &session_id);
    }

    #[test]
    fn lease_reply_applies_intervening_catalog_and_acknowledges_heartbeat_over_tls() {
        let mut pair = connected();
        for (index, outcome) in [
            PortLeaseOutcome::Acquired,
            PortLeaseOutcome::Renewed,
            PortLeaseOutcome::Released,
        ]
        .into_iter()
        .enumerate()
        {
            let request = MessageId::new();
            let sequence = index as u64 + 1;
            heartbeat(&mut pair, sequence, false);
            pair.catalog.display_name = format!("Catalog update {sequence}");
            pair.peer
                .send_envelope(
                    &encode_catalog_snapshot(pair.session, &pair.catalog)
                        .expect("catalog encoding"),
                )
                .expect("send catalog");
            let (route, port) = (pair.route, pair.port);
            response(&mut pair, request, route, port, outcome);
            let received = receive_lease(&mut pair.link, &pair.node, request, route, port)
                .expect("correlated reply after asynchronous messages");
            assert_eq!(received.outcome, outcome);
            assert_eq!(received.request_message_id, request);
            let snapshot = pair.node.lock().expect("node").runtime.snapshot();
            assert_eq!(
                snapshot
                    .peers
                    .iter()
                    .find(|peer| peer.id == pair.catalog.id)
                    .expect("updated peer")
                    .display_name,
                pair.catalog.display_name
            );
            assert_heartbeat_ack(&mut pair, sequence);
        }
    }

    #[test]
    fn lease_reply_rejects_mismatched_request_route_and_every_port_identity_over_tls() {
        let mut pair = connected();
        let request = MessageId::new();
        let (route, port) = (pair.route, pair.port);
        for (returned_request, returned_route, returned_port) in [
            (MessageId::new(), route, port),
            (request, RouteId::new(), port),
            (
                request,
                route,
                PortRef {
                    node_id: NodeId::new(),
                    ..port
                },
            ),
            (
                request,
                route,
                PortRef {
                    capability_id: CapabilityId::new(),
                    ..port
                },
            ),
            (
                request,
                route,
                PortRef {
                    port_id: PortId::new(),
                    ..port
                },
            ),
        ] {
            response(
                &mut pair,
                returned_request,
                returned_route,
                returned_port,
                PortLeaseOutcome::Acquired,
            );
            assert_eq!(
                receive_lease(&mut pair.link, &pair.node, request, route, port)
                    .expect_err("different correlation identity"),
                "peer returned a mismatched lease response"
            );
        }
    }

    #[test]
    fn lease_reply_after_a_full_status_burst_is_accepted() {
        let mut pair = connected();
        let request = MessageId::new();
        let (route, port) = (pair.route, pair.port);
        for sequence in 1..MAX_EXCHANGE_MESSAGES as u64 {
            heartbeat(&mut pair, sequence, false);
        }
        response(&mut pair, request, route, port, PortLeaseOutcome::Renewed);
        assert_eq!(
            receive_lease(&mut pair.link, &pair.node, request, route, port)
                .expect("reply at exchange bound")
                .outcome,
            PortLeaseOutcome::Renewed
        );
        for sequence in 1..MAX_EXCHANGE_MESSAGES as u64 {
            assert_heartbeat_ack(&mut pair, sequence);
        }
    }

    #[test]
    fn lease_wait_stops_at_the_exchange_bound_without_draining_more() {
        let mut pair = connected();
        let request = MessageId::new();
        let (route, port) = (pair.route, pair.port);
        for sequence in 1..=MAX_EXCHANGE_MESSAGES as u64 + 1 {
            heartbeat(&mut pair, sequence, false);
        }
        response(&mut pair, request, route, port, PortLeaseOutcome::Released);
        assert_eq!(
            receive_lease(&mut pair.link, &pair.node, request, route, port)
                .expect_err("bounded wait"),
            "peer lease response limit exceeded"
        );
        for sequence in 1..=MAX_EXCHANGE_MESSAGES as u64 {
            assert_heartbeat_ack(&mut pair, sequence);
        }
        assert!(
            matches!(pair.link.receive_control_message().expect("next message retained"), ReceivedControlMessage::Heartbeat(heartbeat) if heartbeat.sequence == MAX_EXCHANGE_MESSAGES as u64 + 1)
        );
        assert!(
            matches!(pair.link.receive_control_message().expect("later reply retained"), ReceivedControlMessage::PortLeaseResponse(reply) if reply.request_message_id == request)
        );
    }

    #[test]
    fn unsolicited_heartbeat_acknowledgement_is_not_a_lease_reply() {
        let mut pair = connected();
        heartbeat(&mut pair, 1, true);
        assert_eq!(
            receive_lease(
                &mut pair.link,
                &pair.node,
                MessageId::new(),
                pair.route,
                pair.port
            )
            .expect_err("unexpected acknowledgement"),
            "peer returned an unexpected lease control message"
        );
    }
}
