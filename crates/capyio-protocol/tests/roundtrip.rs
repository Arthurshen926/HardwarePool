use capyio_core::{
    AdapterInstanceId, CapabilityClass, NodeDescriptor, Problem, ProblemCategory, ProblemId,
    ProblemSeverity, Route,
};
use capyio_protocol::{
    PROTOCOL_MAJOR, ProtocolError, decode_catalog_snapshot, decode_envelope,
    encode_catalog_snapshot, encode_envelope, new_envelope, v1,
};
use capyio_testkit::{DemoLab, android_node};

#[test]
fn generic_node_catalog_round_trips_through_protobuf() {
    let original = android_node();
    let wire = v1::NodeDescriptor::try_from(&original).expect("to wire");
    let decoded = NodeDescriptor::try_from(wire).expect("to Core");
    assert_eq!(original, decoded);
}

#[test]
fn touchpad_capability_uses_appended_wire_value_16() {
    let mut original = android_node();
    let capability = original
        .capabilities
        .values_mut()
        .next()
        .expect("fixture capability");
    capability.class = CapabilityClass::Touchpad;

    let wire = v1::NodeDescriptor::try_from(&original).expect("to wire");
    assert!(
        wire.capabilities
            .iter()
            .any(|capability| capability.capability_class == 16)
    );
    let decoded = NodeDescriptor::try_from(wire).expect("to Core");
    assert_eq!(original, decoded);
}

#[test]
fn all_demo_routes_round_trip() {
    let lab = DemoLab::new().expect("demo lab");
    for route_id in lab.routes.all() {
        let original = lab.runtime.route(route_id).expect("Route");
        let wire = v1::RouteDescriptor::try_from(original).expect("to wire");
        let decoded = Route::try_from(wire).expect("to Core");
        assert_eq!(*original, decoded);
    }
}

#[test]
fn structured_problem_round_trips() {
    let original = Problem {
        id: ProblemId::new(),
        code: "adapter_process_exit_23".to_owned(),
        category: ProblemCategory::Adapter,
        severity: ProblemSeverity::Error,
        retryable: true,
        related_node: Some(android_node().id),
        related_adapter: Some(AdapterInstanceId::new()),
        related_route: None,
        human_message: "Adapter stopped unexpectedly".to_owned(),
        technical_detail: Some("finite smoke-test crash".to_owned()),
    };
    let wire = v1::ProblemDescriptor::try_from(&original).expect("to wire");
    let decoded = Problem::try_from(wire).expect("to Core");
    assert_eq!(original, decoded);
}

#[test]
fn hello_envelope_round_trips_as_binary() {
    let node = android_node();
    let hello = v1::Hello {
        node: Some(v1::NodeDescriptor::try_from(&node).expect("node conversion")),
        supported_protocol_majors: vec![PROTOCOL_MAJOR],
    };
    let envelope = new_envelope(None, v1::envelope::Payload::Hello(hello));
    let bytes = encode_envelope(&envelope);
    let decoded = decode_envelope(&bytes).expect("decode envelope");
    assert!(matches!(
        decoded.payload,
        Some(v1::envelope::Payload::Hello(_))
    ));
}

#[test]
fn catalog_envelope_decodes_to_session_bound_core_value() {
    let mut node = android_node();
    node.display_name = "Decoded Android catalog".to_owned();
    let mut lab = DemoLab::new().expect("demo lab");
    let envelope = new_envelope(
        Some(lab.session_id),
        v1::envelope::Payload::CatalogSnapshot(v1::CatalogSnapshot {
            node: Some(v1::NodeDescriptor::try_from(&node).expect("node conversion")),
        }),
    );

    let decoded = decode_catalog_snapshot(&encode_envelope(&envelope)).expect("catalog decode");

    assert_eq!(decoded.session_id, lab.session_id);
    assert_eq!(decoded.node, node);
    assert_eq!(decoded.message_id.to_string(), envelope.message_id);
    lab.runtime
        .accept_peer_catalog(decoded.session_id, decoded.node)
        .expect("Runtime catalog admission");
    assert_eq!(
        lab.runtime.snapshot().peers[0].display_name,
        "Decoded Android catalog"
    );
}

#[test]
fn catalog_encoder_produces_a_bounded_session_bound_round_trip() {
    let lab = DemoLab::new().expect("demo lab");
    let node = android_node();

    let bytes = encode_catalog_snapshot(lab.session_id, &node).expect("catalog encode");
    let decoded = decode_catalog_snapshot(&bytes).expect("catalog decode");

    assert!(bytes.len() <= capyio_protocol::MAX_CONTROL_ENVELOPE_BYTES);
    assert_eq!(decoded.session_id, lab.session_id);
    assert_eq!(decoded.node, node);
    assert!(!decoded.message_id.to_string().is_empty());
}

#[test]
fn catalog_decoder_requires_session_payload_and_node() {
    let node = android_node();
    let without_session = new_envelope(
        None,
        v1::envelope::Payload::CatalogSnapshot(v1::CatalogSnapshot {
            node: Some(v1::NodeDescriptor::try_from(&node).expect("node conversion")),
        }),
    );
    assert!(matches!(
        decode_catalog_snapshot(&encode_envelope(&without_session)),
        Err(ProtocolError::MissingField("envelope.session_id"))
    ));

    let lab = DemoLab::new().expect("demo lab");
    let wrong_payload = new_envelope(
        Some(lab.session_id),
        v1::envelope::Payload::Hello(v1::Hello {
            node: Some(v1::NodeDescriptor::try_from(&node).expect("node conversion")),
            supported_protocol_majors: vec![PROTOCOL_MAJOR],
        }),
    );
    assert!(matches!(
        decode_catalog_snapshot(&encode_envelope(&wrong_payload)),
        Err(ProtocolError::UnexpectedPayload {
            expected: "catalog_snapshot"
        })
    ));

    let without_node = new_envelope(
        Some(lab.session_id),
        v1::envelope::Payload::CatalogSnapshot(v1::CatalogSnapshot { node: None }),
    );
    assert!(matches!(
        decode_catalog_snapshot(&encode_envelope(&without_node)),
        Err(ProtocolError::MissingField("catalog_snapshot.node"))
    ));
}

#[test]
fn zero_or_unknown_enum_is_rejected() {
    let mut wire = v1::NodeDescriptor::try_from(&android_node()).expect("to wire");
    wire.online_state = 0;
    assert!(matches!(
        NodeDescriptor::try_from(wire),
        Err(ProtocolError::InvalidEnum {
            field: "node.online_state",
            value: 0
        })
    ));
}

#[test]
fn catalog_ownership_mismatch_is_rejected() {
    let mut wire = v1::NodeDescriptor::try_from(&android_node()).expect("to wire");
    wire.adapter_instances[0].owned_capability_ids.clear();
    assert!(matches!(
        NodeDescriptor::try_from(wire),
        Err(ProtocolError::CatalogOwnershipMismatch { .. })
    ));
}

#[test]
fn unsupported_major_is_rejected() {
    let mut envelope = new_envelope(
        None,
        v1::envelope::Payload::Error(v1::ErrorMessage {
            code: "test".to_owned(),
            category: "protocol".to_owned(),
            retryable: false,
            detail: String::new(),
            related_id: String::new(),
        }),
    );
    envelope.protocol_major = PROTOCOL_MAJOR + 1;
    let bytes = encode_envelope(&envelope);
    assert!(matches!(
        decode_envelope(&bytes),
        Err(ProtocolError::UnsupportedProtocolMajor { .. })
    ));
}
