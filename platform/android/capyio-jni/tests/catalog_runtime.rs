use capyio_android_jni::{AndroidCatalogRuntimeError, AndroidCatalogRuntimeV1};

const FIXTURE: &str = include_str!("../../fixtures/android-core-catalog-v1.json");

#[test]
fn owns_the_exact_java_catalog_as_the_local_runtime_node() {
    let runtime = AndroidCatalogRuntimeV1::new(FIXTURE).expect("catalog Runtime");
    let snapshot = runtime.snapshot();

    assert_eq!(
        snapshot.local_node.id.to_string(),
        "00000000-0000-4000-8000-00000000a001"
    );
    assert_eq!(snapshot.local_node.capabilities.len(), 3);
    assert_eq!(snapshot.local_node.adapter_instances.len(), 3);
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
}

#[test]
fn malformed_catalog_never_creates_or_replaces_runtime_state() {
    assert!(AndroidCatalogRuntimeV1::new("{}").is_err());

    let mut runtime = AndroidCatalogRuntimeV1::new(FIXTURE).expect("catalog Runtime");
    assert!(runtime.replace_disconnected_catalog("{}").is_err());
    assert_eq!(runtime.snapshot().local_node.capabilities.len(), 3);
}
