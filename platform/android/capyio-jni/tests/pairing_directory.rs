use capyio_android_jni::{
    AndroidPairingDirectoryError, android_pairing_directory_active_credential_handles,
    android_pairing_directory_peer_count, canonicalize_android_pairing_directory,
};
use capyio_core::NodeId;
use capyio_pairing::{
    CertificateFingerprint, CredentialHandle, PairedPeerDirectory, PairedPeerRecord,
};

#[test]
fn empty_directory_is_canonical_and_stable() {
    let local = NodeId::new();
    let canonical = canonicalize_android_pairing_directory(
        &local.to_string(),
        br#"{ "peers" : [], "schemaVersion" : 1 }"#,
    )
    .unwrap();
    assert_eq!(canonical, br#"{"schemaVersion":1,"peers":[]}"#);
    assert_eq!(
        canonicalize_android_pairing_directory(&local.to_string(), &canonical).unwrap(),
        canonical
    );
    assert_eq!(
        android_pairing_directory_peer_count(&local.to_string(), &canonical).unwrap(),
        0
    );
}

#[test]
fn active_credential_projection_excludes_revoked_peers() {
    let local = NodeId::new();
    let active = NodeId::new();
    let revoked = NodeId::new();
    let mut directory = PairedPeerDirectory::new(local).unwrap();
    let record = |node_id, name: &str, handle: String| {
        PairedPeerRecord::new(
            node_id,
            name,
            format!("{}.capyio.test", name.to_ascii_lowercase()),
            CredentialHandle::new(handle).unwrap(),
            CertificateFingerprint::new([1; 32]).unwrap(),
            1,
        )
        .unwrap()
    };
    directory
        .add(record(
            active,
            "Active",
            format!("android-keystore:{}", "a".repeat(64)),
        ))
        .unwrap();
    directory
        .add(record(
            revoked,
            "Revoked",
            format!("android-keystore:{}", "b".repeat(64)),
        ))
        .unwrap();
    directory.revoke(revoked, 2).unwrap();
    let encoded = directory.encode_json().unwrap();
    assert_eq!(
        android_pairing_directory_active_credential_handles(&local.to_string(), &encoded).unwrap(),
        vec![format!("android-keystore:{}", "a".repeat(64))]
    );
}

#[test]
fn invalid_identity_and_directory_fail_closed() {
    assert_eq!(
        canonicalize_android_pairing_directory("not-a-node", br#"{"schemaVersion":1,"peers":[]}"#),
        Err(AndroidPairingDirectoryError::InvalidLocalNodeId)
    );
    assert!(
        canonicalize_android_pairing_directory(
            &NodeId::new().to_string(),
            br#"{"schemaVersion":1,"peers":[],"privateKey":"bad"}"#
        )
        .is_err()
    );
}
