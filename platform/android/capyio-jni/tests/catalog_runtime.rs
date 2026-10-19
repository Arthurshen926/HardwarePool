use capyio_android_jni::{AndroidCatalogRuntimeError, AndroidCatalogRuntimeV1};
use capyio_core::{CapabilityClass, NodeDescriptor, PortDirection};

const FIXTURE: &str = include_str!("../../fixtures/android-core-catalog-v1.json");

fn assert_catalog_contract(node: &NodeDescriptor) {
    for (class, direction) in [
        (CapabilityClass::Microphone, PortDirection::Source),
        (CapabilityClass::Speaker, PortDirection::Sink),
        (CapabilityClass::Haptics, PortDirection::Sink),
        (CapabilityClass::Display, PortDirection::Sink),
    ] {
        let capability = node
            .capabilities
            .values()
            .find(|cap| cap.class == class)
            .unwrap_or_else(|| panic!("missing {class:?}"));
        assert!(
            capability
                .ports
                .values()
                .any(|port| port.direction == direction)
        );
        let adapter = &node.adapter_instances[&capability.adapter_instance_id];
        assert!(adapter.owned_capabilities.contains(&capability.id));
    }
}

#[test]
fn owns_java_catalog_and_runtime_installed_display() {
    let runtime = AndroidCatalogRuntimeV1::new(FIXTURE).expect("catalog Runtime");
    let snapshot = runtime.snapshot();

    assert_eq!(
        snapshot.local_node.id.to_string(),
        "00000000-0000-4000-8000-00000000a001"
    );
    assert_catalog_contract(&snapshot.local_node);
    assert!(snapshot.peers.is_empty());
    assert!(snapshot.sessions.is_empty());
    assert!(snapshot.routes.is_empty());
}

#[test]
fn disconnected_replacement_is_transactional_and_identity_bound() {
    let mut runtime = AndroidCatalogRuntimeV1::new(FIXTURE).expect("catalog Runtime");
    runtime
        .replace_disconnected_catalog(FIXTURE)
        .expect("same-node replacement");
    let before = runtime.snapshot();
    assert_catalog_contract(&before.local_node);

    let different_node = FIXTURE.replace(
        "00000000-0000-4000-8000-00000000a001",
        "00000000-0000-4000-8000-00000000a002",
    );
    assert!(matches!(
        runtime.replace_disconnected_catalog(&different_node),
        Err(AndroidCatalogRuntimeError::NodeIdentityChanged)
    ));
    assert_eq!(
        runtime.snapshot().local_node.id.to_string(),
        "00000000-0000-4000-8000-00000000a001"
    );
    assert_eq!(runtime.snapshot(), before);
}

#[test]
fn malformed_catalog_never_creates_or_replaces_runtime_state() {
    assert!(AndroidCatalogRuntimeV1::new("{}").is_err());

    let mut runtime = AndroidCatalogRuntimeV1::new(FIXTURE).expect("catalog Runtime");
    let before = runtime.snapshot();
    assert!(runtime.replace_disconnected_catalog("{}").is_err());
    assert_eq!(runtime.snapshot(), before);
    assert_catalog_contract(&runtime.snapshot().local_node);
    assert_eq!(runtime.publication_revision(), 0);
    runtime.replace_catalog(FIXTURE).unwrap();
    assert_catalog_contract(&runtime.snapshot().local_node);
    assert_eq!(runtime.publication_revision(), 1);
    let before = runtime.snapshot();
    assert!(runtime.replace_catalog("{}").is_err());
    assert_eq!(runtime.snapshot(), before);
    assert_eq!(runtime.publication_revision(), 1);
}
