use std::{fs, path::PathBuf};

use capyio_control_credentials::ControlCredentialBundle;
use capyio_core::NodeId;
use capyio_pairing::{
    CertificateFingerprint, CredentialHandle, PairedPeerDirectory, PairedPeerRecord,
};
use capyio_windows_pairing_store::{PairingStoreError, WindowsPairingStore};

struct TemporaryDirectory(PathBuf);
impl TemporaryDirectory {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "capyio-pairing-store-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn handle(byte: char) -> CredentialHandle {
    CredentialHandle::new(format!("windows-dpapi:{}", byte.to_string().repeat(64))).unwrap()
}

fn record(peer: NodeId, credential: CredentialHandle) -> PairedPeerRecord {
    PairedPeerRecord::new(
        peer,
        "Tablet",
        "tablet.capyio.test",
        credential,
        CertificateFingerprint::new([7; 32]).unwrap(),
        10,
    )
    .unwrap()
}

#[test]
fn absent_store_loads_as_empty_without_creating_files() {
    let temp = TemporaryDirectory::new("absent");
    let local = NodeId::new();
    let store = WindowsPairingStore::from_local_app_data(&temp.0, local).unwrap();
    assert_eq!(store.load_directory().unwrap().peers().len(), 0);
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 0);
}

#[test]
fn directory_replacement_is_complete_and_owner_validated() {
    let temp = TemporaryDirectory::new("directory");
    let local = NodeId::new();
    let peer = NodeId::new();
    let store = WindowsPairingStore::from_local_app_data(&temp.0, local).unwrap();
    let mut first = PairedPeerDirectory::new(local).unwrap();
    first.add(record(peer, handle('a'))).unwrap();
    store.commit_directory(&first).unwrap();
    assert_eq!(
        store
            .load_directory()
            .unwrap()
            .peers()
            .next()
            .unwrap()
            .node_id,
        peer
    );
    let second = PairedPeerDirectory::new(local).unwrap();
    store.commit_directory(&second).unwrap();
    assert_eq!(store.load_directory().unwrap().peers().len(), 0);
    let wrong_owner = PairedPeerDirectory::new(peer).unwrap();
    let mut wrong_owner = wrong_owner;
    wrong_owner.add(record(local, handle('b'))).unwrap();
    assert!(store.commit_directory(&wrong_owner).is_err());
    assert_eq!(store.load_directory().unwrap().peers().len(), 0);
}

#[test]
fn malformed_and_oversized_directory_fail_closed() {
    let temp = TemporaryDirectory::new("bad-directory");
    let store = WindowsPairingStore::from_local_app_data(&temp.0, NodeId::new()).unwrap();
    let root = temp.0.join("CapyIO/Control/Pairing");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("peers-v1.json"), b"{}").unwrap();
    assert!(store.load_directory().is_err());
    fs::write(root.join("peers-v1.json"), vec![b'x'; 65_537]).unwrap();
    assert!(matches!(
        store.load_directory(),
        Err(PairingStoreError::Size)
    ));
}

#[test]
fn credential_handles_are_closed_and_never_become_paths() {
    let temp = TemporaryDirectory::new("handles");
    let store = WindowsPairingStore::from_local_app_data(&temp.0, NodeId::new()).unwrap();
    for value in [
        "other:abc",
        "windows-dpapi:abc",
        &format!("windows-dpapi:{}", "A".repeat(64)),
        &format!("windows-dpapi:{}", "g".repeat(64)),
    ] {
        let handle = CredentialHandle::new(value).unwrap();
        assert!(matches!(
            store.load_credential(&handle),
            Err(PairingStoreError::UnsupportedCredentialHandle)
        ));
    }
}

#[cfg(windows)]
#[test]
fn dpapi_credential_roundtrip_replaces_atomically_and_is_user_scoped() {
    let temp = TemporaryDirectory::new("credential");
    let store = WindowsPairingStore::from_local_app_data(&temp.0, NodeId::new()).unwrap();
    let handle = handle('c');
    store
        .put_credential(&handle, b"credential generation one")
        .unwrap();
    assert_eq!(
        store.load_credential(&handle).unwrap(),
        b"credential generation one"
    );
    store
        .put_credential(&handle, b"credential generation two")
        .unwrap();
    assert_eq!(
        store.load_credential(&handle).unwrap(),
        b"credential generation two"
    );
    let credential_dir = temp.0.join("CapyIO/Control/Pairing/credentials-v1");
    assert_eq!(fs::read_dir(credential_dir).unwrap().count(), 1);
}

#[cfg(windows)]
#[test]
fn typed_control_credential_roundtrip_rejects_arbitrary_plaintext() {
    let temp = TemporaryDirectory::new("typed-credential");
    let store = WindowsPairingStore::from_local_app_data(&temp.0, NodeId::new()).unwrap();
    let credential_handle = handle('d');
    let bundle = ControlCredentialBundle::new(
        vec![b"leaf".to_vec(), b"issuer".to_vec()],
        b"private-key".to_vec(),
        b"peer-root".to_vec(),
    )
    .unwrap();
    store
        .put_control_credential(&credential_handle, &bundle)
        .unwrap();
    let recovered = store.load_control_credential(&credential_handle).unwrap();
    let (chain, key, root) = recovered.into_parts();
    assert_eq!(chain, vec![b"leaf".to_vec(), b"issuer".to_vec()]);
    assert_eq!(key, b"private-key");
    assert_eq!(root, b"peer-root");

    let invalid = handle('e');
    store
        .put_credential(&invalid, b"not a credential bundle")
        .unwrap();
    assert!(store.load_control_credential(&invalid).is_err());
}
