//! Real authenticated TLS coverage for dormant reverse Prepared provenance.
use super::*;

fn stereo_prepared_route(
    local_source: &CatalogControlLink,
    peer_sink: &CatalogControlLink,
) -> capyio_core::Route {
    let mut route = stereo_consent_route(peer_sink, local_source);
    route.state = RouteState::Prepared;
    route.selected_format = Some(capyio_core::FormatDescriptor::new(
        capyio_protocol::SECURE_AUDIO_STEREO_FORMAT,
    ));
    route.selected_qos_mode = Some(capyio_core::QosMode::Interactive);
    route
}

#[test]
fn ipc_stereo_prepared_receipt_requires_real_wire_and_is_single_use() {
    let (mut sink, mut source, _, _) = connected_pair();
    let route = stereo_prepared_route(&sink, &source);
    let fabricated = capyio_protocol::decode_route_status(
        &capyio_protocol::encode_route_status(route.session_id, &route).unwrap(),
    )
    .unwrap();
    assert!(sink.take_received_stereo_prepared(&fabricated).is_err());
    let status = receive_stereo_status(&mut sink, &mut source, &route);
    let receipt = sink.take_received_stereo_prepared(&status).unwrap();
    assert_eq!(receipt.status().route, route);
    assert!(receipt.received_wall_ms() > 0);
    assert!(receipt.monotonic_deadline() > receipt.received_instant());
    assert!(receipt.is_live());
    sink.validate_stereo_prepared_receipt(&receipt).unwrap();
    assert!(sink.take_received_stereo_prepared(&status).is_err());
    let replay = receive_stereo_status(&mut sink, &mut source, &route);
    assert!(sink.take_received_stereo_prepared(&replay).is_err());
    assert!(receipt.is_live());
    sink.close();
    assert!(!receipt.is_live());
}

#[test]
fn ipc_stereo_prepared_receipt_wrong_instance_even_with_same_tls_ids_fails() {
    let session = SessionId::new();
    let (mut sink, source, _, _) = connected_raw_pair_with_session(session);
    let mut source = CatalogControlLink::from_dialed(source);
    let (other, _, _, _) = connected_raw_pair_with_session(session);
    assert_eq!(sink.tls_binding(), other.tls_binding());
    assert_ne!(
        sink.stereo_consent_control_identity().unwrap(),
        other.stereo_consent_control_identity().unwrap()
    );
    let route = stereo_prepared_route(&sink, &source);
    let status = receive_stereo_status(&mut sink, &mut source, &route);
    let receipt = sink.take_received_stereo_prepared(&status).unwrap();
    let identity = sink.stereo_consent_control_identity().unwrap();
    assert!(identity.is_live());
    assert!(
        other
            .validate_stereo_consent_control_identity(&identity)
            .is_err()
    );
    sink.validate_stereo_consent_control_identity(&identity)
        .unwrap();
    assert!(other.validate_stereo_prepared_receipt(&receipt).is_err());
    sink.validate_stereo_prepared_receipt(&receipt).unwrap();
    drop(sink);
    assert!(!identity.is_live());
    assert!(!receipt.is_live());
}

#[test]
fn ipc_stereo_prepared_duplicate_preserves_first_clock_and_conflict_retires() {
    let (mut sink, mut source, _, _) = connected_pair();
    let route = stereo_prepared_route(&sink, &source);
    let first = receive_stereo_status(&mut sink, &mut source, &route);
    let before_replay = std::time::Instant::now();
    thread::sleep(Duration::from_millis(5));
    let replay = receive_stereo_status(&mut sink, &mut source, &route);
    assert_ne!(first.message_id, replay.message_id);
    let receipt = sink.take_received_stereo_prepared(&replay).unwrap();
    assert!(receipt.received_instant() <= before_replay);
    let mut changed = route.clone();
    let capyio_core::AuthorizationState::Authorized {
        expires_at_ms: Some(expiry),
    } = changed.authorization
    else {
        panic!()
    };
    changed.authorization = capyio_core::AuthorizationState::Authorized {
        expires_at_ms: Some(expiry - 1),
    };
    receive_stereo_status(&mut sink, &mut source, &changed);
    assert!(!receipt.is_live());
    let replay = receive_stereo_status(&mut sink, &mut source, &route);
    assert!(sink.take_received_stereo_prepared(&replay).is_err());
}

#[test]
fn ipc_stereo_prepared_bad_consume_and_expired_first_receipt_cannot_rearm() {
    let (mut sink, mut source, _, _) = connected_pair();
    let mut route = stereo_prepared_route(&sink, &source);
    let status = receive_stereo_status(&mut sink, &mut source, &route);
    let mut bad = status.clone();
    bad.route.diagnostic_ids.push(capyio_core::ProblemId::new());
    assert!(sink.take_received_stereo_prepared(&bad).is_err());
    assert!(sink.take_received_stereo_prepared(&status).is_err());
    route.id = capyio_core::RouteId::new();
    route.authorization = capyio_core::AuthorizationState::Authorized {
        expires_at_ms: Some(1),
    };
    let status = receive_stereo_status(&mut sink, &mut source, &route);
    assert!(sink.take_received_stereo_prepared(&status).is_err());
    route.authorization = stereo_prepared_route(&sink, &source).authorization;
    let status = receive_stereo_status(&mut sink, &mut source, &route);
    assert!(sink.take_received_stereo_prepared(&status).is_err());
}

#[test]
fn ipc_stereo_prepared_storage_is_bounded_without_changing_generic_receive() {
    let (mut sink, mut source, _, _) = connected_pair();
    let mut route = stereo_prepared_route(&sink, &source);
    let status = receive_stereo_status(&mut sink, &mut source, &route);
    let receipt = sink.take_received_stereo_prepared(&status).unwrap();
    for _ in 0..16 {
        route.id = capyio_core::RouteId::new();
        // Even cache exhaustion does not change the existing generic wire API.
        receive_stereo_status(&mut sink, &mut source, &route);
    }
    assert!(!receipt.is_live());
    let status = receive_stereo_status(&mut sink, &mut source, &route);
    assert!(sink.take_received_stereo_prepared(&status).is_err());
    source.send_heartbeat(1, false).unwrap();
    assert!(matches!(
        sink.receive_control_message().unwrap(),
        ReceivedControlMessage::Heartbeat(_)
    ));
}

#[test]
fn ipc_stereo_prepared_decode_errors_revoke_receipt_and_identity_in_both_receive_modes() {
    for polling in [false, true] {
        for wrong_session in [false, true] {
            let (mut sink, mut raw_source, _, source_runtime) = connected_raw_pair();
            let lab = capyio_testkit::DemoLab::new().unwrap();
            let mut route = lab
                .runtime
                .route(lab.routes.phone_microphone_to_windows)
                .unwrap()
                .clone();
            route.session_id = sink.session_id();
            route.source.node_id = sink.tls_binding().server_node_id;
            route.sink.node_id = sink.peer_node_id();
            route.backend = RouteBackend::AdapterManaged;
            route.state = RouteState::Prepared;
            route.epoch = 0;
            route.selected_format = Some(capyio_core::FormatDescriptor::new(
                capyio_protocol::SECURE_AUDIO_STEREO_FORMAT,
            ));
            route.selected_qos_mode = Some(capyio_core::QosMode::Interactive);
            route.compatible_formats = vec![capyio_core::FormatDescriptor::new(
                capyio_protocol::SECURE_AUDIO_STEREO_FORMAT,
            )];
            route.compatible_qos_modes = [capyio_core::QosMode::Interactive].into_iter().collect();
            let now: u64 = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
                .try_into()
                .unwrap();
            route.authorization = capyio_core::AuthorizationState::Authorized {
                expires_at_ms: Some(now + 60_000),
            };
            raw_source
                .send_envelope(
                    &capyio_protocol::encode_route_status(route.session_id, &route).unwrap(),
                )
                .unwrap();
            let ReceivedControlMessage::RouteStatus(status) =
                sink.receive_control_message().unwrap()
            else {
                panic!("expected genuine receipt");
            };
            let receipt = sink.take_received_stereo_prepared(&status).unwrap();
            let identity = sink.stereo_consent_control_identity().unwrap();
            let bytes = if wrong_session {
                capyio_protocol::encode_catalog_snapshot(
                    SessionId::new(),
                    &source_runtime.snapshot().local_node,
                )
                .unwrap()
            } else {
                capyio_protocol::encode_envelope(&capyio_protocol::new_envelope(
                    Some(sink.session_id()),
                    capyio_protocol::v1::envelope::Payload::CatalogSnapshot(
                        capyio_protocol::v1::CatalogSnapshot { node: None },
                    ),
                ))
            };
            raw_source.send_envelope(&bytes).unwrap();
            let error = if polling {
                let deadline = std::time::Instant::now() + Duration::from_secs(2);
                loop {
                    assert!(std::time::Instant::now() < deadline);
                    match sink.poll_control_message_until(deadline) {
                        Err(error) => break error,
                        Ok(None) => thread::sleep(Duration::from_millis(1)),
                        Ok(Some(_)) => panic!("malformed/mismatched frame was accepted"),
                    }
                }
            } else {
                match sink.receive_control_message() {
                    Err(error) => error,
                    Ok(_) => panic!("invalid frame accepted"),
                }
            };
            if wrong_session {
                assert!(matches!(error, CatalogLinkError::InvalidPeerCatalog));
            } else {
                assert!(matches!(
                    error,
                    CatalogLinkError::Protocol(capyio_protocol::ProtocolError::MissingField(
                        "catalog_snapshot.node"
                    ))
                ));
            }
            assert!(!receipt.is_live());
            assert!(!identity.is_live());
            // Preserve existing behavior: malformed blocking decode reports an
            // error without closing; polling and wrong catalogs already close.
            assert_eq!(
                sink.authenticated_peer_certificate().is_ok(),
                !polling && !wrong_session
            );
        }
    }
}

#[test]
fn ipc_stereo_prepared_both_control_roles_and_lifecycle_offer_still_delivered() {
    use capyio_core::StreamId;
    use capyio_protocol::{SecureAudioDataOffer, SecureAudioTransport};
    use std::time::Instant;

    for source_dials in [false, true] {
        let (accepted, dialed, _, _) = connected_pair();
        let (mut source, mut sink) = if source_dials {
            (dialed, accepted)
        } else {
            (accepted, dialed)
        };
        let mut route = stereo_prepared_route(&source, &sink);
        let status = receive_stereo_status(&mut source, &mut sink, &route);
        let receipt = source.take_received_stereo_prepared(&status).unwrap();
        // A successful handoff consumes proof; subsequent real lifecycle status
        // is delivered normally, not treated as a malformed prelaunch message.
        route.state = RouteState::Starting;
        route.epoch = 1;
        let starting = receive_stereo_status(&mut source, &mut sink, &route);
        assert_eq!(starting.route, route);
        assert!(!receipt.is_live());
        assert!(source.take_received_stereo_prepared(&starting).is_err());
        let offer = SecureAudioDataOffer {
            port: 40212,
            stream_id: StreamId::new(),
            transport: SecureAudioTransport::Tls13MtlsStereoV1,
        };
        sink.send_secure_stereo_audio_data_offer_until(
            &route,
            offer,
            receipt.received_wall_ms(),
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
        let ReceivedControlMessage::RouteStatus(offered) =
            source.receive_control_message().unwrap()
        else {
            panic!("expected genuine lifecycle offer");
        };
        assert_eq!(offered.secure_audio_data, Some(offer));
        assert!(source.take_received_stereo_prepared(&offered).is_err());
        source
            .take_received_secure_stereo_audio_offer(&route, offer)
            .unwrap();
        assert!(!source.is_closed());
    }
}

#[test]
fn ipc_stereo_prepared_only_exact_reverse_prelaunch_shape_mints_receipt() {
    use capyio_core::{AuthorizationState, FormatDescriptor, QosMode};
    for case in 0..7 {
        let (mut source, mut sink, _, _) = connected_pair();
        let mut route = stereo_prepared_route(&source, &sink);
        match case {
            0 => route.epoch = 1,
            1 => route.state = RouteState::Starting,
            2 => {
                route.state = RouteState::Draft;
                route.selected_format = None;
                route.selected_qos_mode = None;
            }
            3 => {
                route.authorization = AuthorizationState::Authorized {
                    expires_at_ms: None,
                }
            }
            4 => {
                std::mem::swap(&mut route.source, &mut route.sink);
            }
            5 => {
                let mono = FormatDescriptor::new(capyio_protocol::SECURE_AUDIO_MONO_FORMAT);
                route.compatible_formats = vec![mono.clone()];
                route.selected_format = Some(mono);
            }
            6 => {
                route.compatible_qos_modes.insert(QosMode::Basic);
            }
            _ => unreachable!(),
        }
        let status = receive_stereo_status(&mut source, &mut sink, &route);
        assert!(
            source.take_received_stereo_prepared(&status).is_err(),
            "case {case}"
        );
        assert!(!source.is_closed());
    }
}
