use capyio_android_host::{android_adapter_id, android_capability_id, android_port_id};
use capyio_core::NodeId;

const NODE_ID: &str = "00000000-0000-4000-8000-00000000a001";

#[test]
fn matches_the_android_uuid_v5_catalog_vectors() {
    let node_id: NodeId = NODE_ID.parse().expect("node ID");

    assert_eq!(
        android_adapter_id(node_id, "android.imu")
            .expect("adapter ID")
            .to_string(),
        "b6cfd6b8-f99c-5fe4-8b55-cba830f956d0"
    );
    assert_eq!(
        android_capability_id(node_id, "android.imu")
            .expect("Capability ID")
            .to_string(),
        "ecaefa6d-6e4e-577e-9ef3-73760663dba7"
    );
    assert_eq!(
        android_port_id(node_id, "android.imu", "android.imu.source")
            .expect("Port ID")
            .to_string(),
        "44a9ac57-5493-5083-9bae-6cb997420e4a"
    );
}

#[test]
fn identity_domains_and_nodes_do_not_collide() {
    let node_id: NodeId = NODE_ID.parse().expect("node ID");
    let other_node = NodeId::new();

    assert_ne!(
        android_adapter_id(node_id, "android.imu")
            .expect("adapter ID")
            .to_string(),
        android_capability_id(node_id, "android.imu")
            .expect("Capability ID")
            .to_string()
    );
    assert_ne!(
        android_capability_id(node_id, "android.imu").expect("Capability ID"),
        android_capability_id(other_node, "android.imu").expect("other Capability ID")
    );
}

#[test]
fn malformed_platform_keys_fail_closed() {
    let node_id: NodeId = NODE_ID.parse().expect("node ID");
    assert!(android_capability_id(node_id, "Android IMU").is_err());
    assert!(android_port_id(node_id, "android.imu", "").is_err());
    assert!(android_adapter_id(node_id, &"x".repeat(97)).is_err());
}
