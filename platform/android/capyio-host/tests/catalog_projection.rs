use std::collections::BTreeSet;

use capyio_android_host::{
    AndroidAdapterRouteModeV1, AndroidCapabilityAvailabilityV1, AndroidCapabilityClassV1,
    AndroidCapabilityDtoV1, AndroidCatalogDtoV1, AndroidInteroperabilityModeV1,
    AndroidPermissionRequirementV1, AndroidPortDirectionV1, AndroidPortDtoV1, AndroidQosModeV1,
    MAX_ANDROID_CATALOG_JSON_BYTES, android_capability_id, decode_android_catalog_json,
    project_android_catalog,
};
use capyio_control_host::encode_publication_catalog;
use capyio_core::{NodeId, Platform, PortDirection, SessionId};
use capyio_protocol::decode_catalog_snapshot;

const NODE_ID: &str = "00000000-0000-4000-8000-00000000a001";

#[test]
fn projects_android_dto_into_a_valid_owned_core_catalog() {
    let node = project_android_catalog(catalog()).expect("Core catalog");

    assert_eq!(node.platform, Platform::Android);
    assert_eq!(node.capabilities.len(), 2);
    assert_eq!(node.adapter_instances.len(), 2);
    node.validate().expect("valid Node catalog");
    assert!(node.capabilities.values().all(|capability| {
        node.adapter_instances
            .get(&capability.adapter_instance_id)
            .is_some_and(|adapter| adapter.owned_capabilities.contains(&capability.id))
    }));
}

#[test]
fn android_catalog_projects_and_round_trips_through_session_bound_publication() {
    let node = project_android_catalog(catalog()).expect("Core catalog");
    let node_id: NodeId = NODE_ID.parse().expect("Node ID");
    let microphone =
        android_capability_id(node_id, "android.microphone").expect("microphone Capability ID");
    let bytes = encode_publication_catalog(SessionId::new(), &node, &BTreeSet::from([microphone]))
        .expect("publication envelope");
    let decoded = decode_catalog_snapshot(&bytes).expect("decoded publication");

    assert!(decoded.node.capabilities.contains_key(&microphone));
    assert!(decoded.node.capabilities.values().any(|capability| {
        capability
            .ports
            .values()
            .all(|port| port.direction != PortDirection::Source)
    }));
    assert_eq!(decoded.node.capabilities.len(), 2);
}

#[test]
fn malformed_android_catalogs_fail_before_core_publication() {
    let mut dto = catalog();
    dto.capabilities[0].ports[0].profile = "capyio.audio.frames/01".to_owned();
    assert!(project_android_catalog(dto).is_err());

    let mut dto = catalog();
    dto.capabilities[0].adapter_attached = false;
    assert!(project_android_catalog(dto).is_err());

    let mut dto = catalog();
    dto.capabilities.push(dto.capabilities[0].clone());
    assert!(project_android_catalog(dto).is_err());

    let json = serde_json::to_string(&catalog()).expect("catalog JSON");
    assert!(decode_android_catalog_json(&json).is_ok());
    assert!(
        decode_android_catalog_json(&json.replace("\"schema_version\":1", "\"schema_version\":2"))
            .is_err()
    );
    assert!(decode_android_catalog_json(&format!("{{\"unknown\":{},{}", 1, &json[1..])).is_err());
    assert!(decode_android_catalog_json(&"x".repeat(MAX_ANDROID_CATALOG_JSON_BYTES + 1)).is_err());
}

#[test]
fn decodes_the_exact_java_service_catalog_fixture() {
    let fixture = include_str!("../../fixtures/android-core-catalog-v1.json");
    let node = decode_android_catalog_json(fixture).expect("Java catalog fixture");

    assert_eq!(node.capabilities.len(), 3);
    assert_eq!(node.adapter_instances.len(), 3);
    node.validate()
        .expect("projected fixture must remain valid");
}

fn catalog() -> AndroidCatalogDtoV1 {
    AndroidCatalogDtoV1 {
        schema_version: 1,
        node_id: NODE_ID.to_owned(),
        display_name: "CapyIO Android".to_owned(),
        platform_version: "Android 16".to_owned(),
        runtime_version: "0.4.3-dev".to_owned(),
        capabilities: vec![
            capability(
                "android.microphone",
                "Phone Microphone",
                AndroidCapabilityClassV1::Microphone,
                AndroidPermissionRequirementV1::UserConfirmation,
                AndroidPortDirectionV1::Source,
                "android.microphone.source",
                "Microphone Source",
            ),
            capability(
                "android.speaker",
                "Phone Speaker",
                AndroidCapabilityClassV1::Speaker,
                AndroidPermissionRequirementV1::ForegroundService,
                AndroidPortDirectionV1::Sink,
                "android.speaker.sink",
                "Speaker Sink",
            ),
        ],
    }
}

fn capability(
    key: &str,
    display_name: &str,
    class: AndroidCapabilityClassV1,
    permission_requirement: AndroidPermissionRequirementV1,
    direction: AndroidPortDirectionV1,
    port_key: &str,
    port_name: &str,
) -> AndroidCapabilityDtoV1 {
    AndroidCapabilityDtoV1 {
        key: key.to_owned(),
        display_name: display_name.to_owned(),
        class,
        availability: AndroidCapabilityAvailabilityV1::Available,
        permission_requirement,
        adapter_attached: true,
        adapter_type: format!("dev.capyio.{key}"),
        adapter_version: "1".to_owned(),
        supported_route_modes: BTreeSet::from([AndroidAdapterRouteModeV1::CapyDataPlane]),
        ports: vec![AndroidPortDtoV1 {
            key: port_key.to_owned(),
            display_name: port_name.to_owned(),
            direction,
            profile: "capyio.audio.frames/1".to_owned(),
            formats: vec!["pcm-s16le-48000".to_owned()],
            qos_modes: vec![AndroidQosModeV1::Interactive],
            clock_domain: Some("android.audio.frame-position".to_owned()),
            interoperability: AndroidInteroperabilityModeV1::StandardPort,
        }],
    }
}
