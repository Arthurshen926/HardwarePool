use capyio_control_credentials::{
    ControlCredentialBundle, ControlCredentialError, MAX_CONTROL_CREDENTIAL_BUNDLE_BYTES,
};

#[test]
fn credential_bundle_round_trips_exact_components() {
    let bundle = ControlCredentialBundle::new(
        vec![b"leaf certificate".to_vec(), b"issuer certificate".to_vec()],
        b"private key".to_vec(),
        b"peer trust root".to_vec(),
    )
    .unwrap();
    let encoded = bundle.encode().unwrap();
    let decoded = ControlCredentialBundle::decode(&encoded).unwrap();
    let (chain, key, root) = decoded.into_parts();
    assert_eq!(
        chain,
        vec![b"leaf certificate".to_vec(), b"issuer certificate".to_vec()]
    );
    assert_eq!(key, b"private key");
    assert_eq!(root, b"peer trust root");
}

#[test]
fn malformed_truncated_extended_and_oversized_bundles_fail_closed() {
    let valid =
        ControlCredentialBundle::new(vec![b"leaf".to_vec()], b"key".to_vec(), b"root".to_vec())
            .unwrap()
            .encode()
            .unwrap();
    for candidate in [
        &valid[..valid.len() - 1],
        &[valid.as_slice(), b"trailing"].concat(),
        b"wrong".as_slice(),
    ] {
        assert!(ControlCredentialBundle::decode(candidate).is_err());
    }
    assert_eq!(
        ControlCredentialBundle::decode(&vec![0; MAX_CONTROL_CREDENTIAL_BUNDLE_BYTES + 1]).err(),
        Some(ControlCredentialError::BundleTooLarge)
    );
}

#[test]
fn empty_components_and_excessive_chain_are_rejected() {
    assert!(ControlCredentialBundle::new(vec![], vec![1], vec![2]).is_err());
    assert!(ControlCredentialBundle::new(vec![vec![1]], vec![], vec![2]).is_err());
    assert!(ControlCredentialBundle::new(vec![vec![1]; 5], vec![2], vec![3]).is_err());
}
