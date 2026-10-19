//! Current-account desktop identity. This metadata never contains credentials.
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    str::FromStr,
};

use capyio_core::NodeId;
use capyio_pairing::{MAX_PAIRED_DIRECTORY_JSON_BYTES, PairedPeerDirectory};
use capyio_windows_pairing_store::WindowsPairingStore;
use serde::{Deserialize, Serialize};

const IDENTITY_FILE: &str = "node-identity-v1.json";
const MAX_IDENTITY_BYTES: usize = 1024;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IdentityDocument {
    schema_version: u8,
    node_id: String,
}

pub(super) fn for_current_user() -> Result<NodeId, String> {
    let legacy = legacy_id();
    let store = WindowsPairingStore::default_for_user(legacy)
        .map_err(|error| format!("无法定位当前 Windows 账号的节点身份目录：{error}"))?;
    load_or_create(store.directory_path(), legacy).map_err(|error| {
        format!("无法加载或保存本机节点身份，已停止启动；请检查当前账号的 CapyIO 配对目录：{error}")
    })
}

fn legacy_id() -> NodeId {
    NodeId::from_str(capyio_testkit::WINDOWS_NODE_ID).expect("historical desktop identity")
}

fn read_optional(path: &Path, maximum: usize) -> Result<Option<Vec<u8>>, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("identity metadata read failed: {error}")),
    };
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("identity metadata is not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("identity metadata read failed: {error}"))?;
    if bytes.len() > maximum {
        return Err("identity metadata exceeds its size limit".into());
    }
    Ok(Some(bytes))
}

fn read_identity(path: &Path) -> Result<Option<NodeId>, String> {
    let Some(bytes) = read_optional(path, MAX_IDENTITY_BYTES)? else {
        return Ok(None);
    };
    let document: IdentityDocument = serde_json::from_slice(&bytes)
        .map_err(|_| "stored node identity has invalid JSON or fields".to_owned())?;
    let node_id = NodeId::from_str(&document.node_id)
        .map_err(|_| "stored node identity is not a UUID".to_owned())?;
    if document.schema_version != 1
        || node_id.as_uuid().is_nil()
        || node_id.to_string() != document.node_id
    {
        return Err("stored node identity has an unsupported schema or invalid ID".into());
    }
    Ok(Some(node_id))
}

fn legacy_installation(root: &Path, legacy: NodeId) -> Result<bool, String> {
    if let Some(bytes) =
        read_optional(&root.join("peers-v1.json"), MAX_PAIRED_DIRECTORY_JSON_BYTES)?
    {
        // An empty/revoked old directory still belongs to the historical ID.
        // Corrupt metadata never becomes an excuse to generate another ID.
        PairedPeerDirectory::decode_json(legacy, &bytes)
            .map_err(|_| "existing pairing directory is invalid; identity was not changed")?;
        return Ok(true);
    }
    match fs::read_dir(root.join("credentials-v1")) {
        Ok(mut entries) => entries
            .next()
            .transpose()
            .map(|entry| entry.is_some())
            .map_err(|error| format!("existing credential directory is unreadable: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "existing credential directory is unreadable: {error}"
        )),
    }
}

fn load_or_create(root: &Path, legacy: NodeId) -> Result<NodeId, String> {
    let path = root.join(IDENTITY_FILE);
    if let Some(id) = read_identity(&path)? {
        return Ok(id);
    }
    let candidate = if legacy_installation(root, legacy)? {
        legacy
    } else {
        NodeId::new()
    };
    let bytes = serde_json::to_vec(&IdentityDocument {
        schema_version: 1,
        node_id: candidate.to_string(),
    })
    .map_err(|error| error.to_string())?;
    fs::create_dir_all(root)
        .map_err(|error| format!("identity directory creation failed: {error}"))?;
    let temporary = root.join(format!(".node-identity-{}.tmp", NodeId::new()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("identity staging failed: {error}"))?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("identity staging failed: {error}"))?;
        drop(file);
        // A complete, synced file is published without replacing a winner
        // from another process. Unsupported filesystems fail explicitly.
        match fs::hard_link(&temporary, &path) {
            Ok(()) => Ok(candidate),
            Err(error) => read_identity(&path)?
                .ok_or_else(|| format!("atomic node identity publication failed: {error}")),
        }
    })();
    let _ = fs::remove_file(&temporary);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_pairing::{CertificateFingerprint, CredentialHandle, PairedPeerRecord};
    use std::{
        path::PathBuf,
        sync::{Arc, Barrier},
        thread,
    };

    struct TestRoot(PathBuf);
    impl TestRoot {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("capyio-node-identity-test-{}", NodeId::new()));
            fs::create_dir(&path).expect("unique test directory");
            Self(path)
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn fresh_accounts_get_different_ids_and_restarts_keep_the_same_id() {
        let first = TestRoot::new();
        let second = TestRoot::new();
        let id = load_or_create(&first.0, legacy_id()).unwrap();
        assert_ne!(id, legacy_id());
        assert_ne!(id, load_or_create(&second.0, legacy_id()).unwrap());
        assert_eq!(id, load_or_create(&first.0, legacy_id()).unwrap());
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(first.0.join(IDENTITY_FILE)).unwrap()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"schemaVersion": 1, "nodeId": id.to_string()})
        );
    }

    #[test]
    fn concurrent_first_launches_observe_one_complete_winning_identity() {
        let root = TestRoot::new();
        let barrier = Arc::new(Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let path = root.0.clone();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    load_or_create(&path, legacy_id()).unwrap()
                })
            })
            .collect();
        let ids: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert!(ids.iter().all(|id| *id == ids[0]));
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
    }

    #[test]
    fn active_revoked_and_empty_existing_directories_preserve_legacy_identity_and_bytes() {
        for state in ["active", "revoked", "empty"] {
            let root = TestRoot::new();
            let mut directory = PairedPeerDirectory::new(legacy_id()).unwrap();
            if state != "empty" {
                let peer = NodeId::new();
                directory
                    .add(
                        PairedPeerRecord::new(
                            peer,
                            "Existing tablet",
                            "tablet.test",
                            CredentialHandle::new(format!("windows-dpapi:{}", "a".repeat(64)))
                                .unwrap(),
                            CertificateFingerprint::new([1; 32]).unwrap(),
                            100,
                        )
                        .unwrap(),
                    )
                    .unwrap();
                if state == "revoked" {
                    directory.revoke(peer, 200).unwrap();
                }
            }
            let original = directory.encode_json().unwrap();
            let path = root.0.join("peers-v1.json");
            fs::write(&path, &original).unwrap();
            assert_eq!(
                load_or_create(&root.0, legacy_id()).unwrap().to_string(),
                capyio_testkit::WINDOWS_NODE_ID
            );
            assert_eq!(fs::read(path).unwrap(), original);
            assert_eq!(load_or_create(&root.0, legacy_id()).unwrap(), legacy_id());
        }
    }

    #[test]
    fn orphaned_credentials_preserve_legacy_without_reading_or_replacing_them() {
        let root = TestRoot::new();
        fs::create_dir(root.0.join("credentials-v1")).unwrap();
        let path = root.0.join("credentials-v1/opaque.dpapi");
        fs::write(&path, b"opaque protected fixture").unwrap();
        assert_eq!(load_or_create(&root.0, legacy_id()).unwrap(), legacy_id());
        assert_eq!(fs::read(path).unwrap(), b"opaque protected fixture");
    }

    #[test]
    fn corrupt_identity_is_not_replaced_or_downgraded_to_legacy() {
        let root = TestRoot::new();
        for bytes in [
            b"{".to_vec(),
            vec![b'x'; MAX_IDENTITY_BYTES + 1],
            br#"{"schemaVersion":2,"nodeId":"00000000-0000-4000-8000-000000000001"}"#.to_vec(),
            br#"{"schemaVersion":1,"nodeId":"00000000-0000-0000-0000-000000000000"}"#.to_vec(),
            br#"{"schemaVersion":1,"nodeId":"00000000-0000-4000-8000-000000000001","extra":1}"#
                .to_vec(),
        ] {
            let path = root.0.join(IDENTITY_FILE);
            fs::write(&path, &bytes).unwrap();
            assert!(load_or_create(&root.0, legacy_id()).is_err());
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }

    #[test]
    fn corrupt_pairing_or_unwritable_target_never_creates_a_replacement_identity() {
        let root = TestRoot::new();
        fs::write(root.0.join("peers-v1.json"), b"broken metadata").unwrap();
        assert!(load_or_create(&root.0, legacy_id()).is_err());
        assert!(!root.0.join(IDENTITY_FILE).exists());
        let blocked_root = root.0.join("not-a-directory");
        fs::write(&blocked_root, b"not a directory").unwrap();
        assert!(load_or_create(&blocked_root, legacy_id()).is_err());
        assert_eq!(fs::read(blocked_root).unwrap(), b"not a directory");
    }

    #[test]
    fn production_bootstrap_rebinds_local_routes_before_real_adapter_catalogs() {
        use super::super::{DesktopNodeState, touchpad_data};
        let id = NodeId::new();
        let mut node = DesktopNodeState::with_local_identity(id).unwrap();
        let snapshot = node.runtime.snapshot();
        assert_eq!(snapshot.local_node.id, id);
        assert_eq!(snapshot.routes.len(), 4);
        assert!(
            snapshot
                .sessions
                .iter()
                .all(|session| session.local_node_id == id)
        );
        for route in &snapshot.routes {
            assert!(route.source.node_id == id || route.sink.node_id == id);
            assert_ne!(route.source.node_id, legacy_id());
            assert_ne!(route.sink.node_id, legacy_id());
            assert_eq!(route.state, capyio_core::RouteState::Draft);
            assert_eq!(
                route.authorization,
                capyio_core::AuthorizationState::Pending
            );
        }
        // This only registers the host contract and enumerates availability;
        // it never opens a touchpad Sink or injects input.
        touchpad_data::install_catalog(&mut node.runtime).unwrap();
        let local = node.runtime.snapshot().local_node;
        assert_eq!(local.id, id);
        let adapter = local
            .adapter_instances
            .values()
            .find(|adapter| adapter.adapter_type == "dev.capyio.windows.touchpad-tls")
            .unwrap();
        assert!(
            local
                .capabilities
                .values()
                .any(|capability| capability.adapter_instance_id == adapter.id
                    && capability.class == capyio_core::CapabilityClass::Touchpad)
        );
        assert_eq!(
            DesktopNodeState::demo()
                .unwrap()
                .runtime
                .snapshot()
                .local_node
                .id,
            legacy_id()
        );
    }

    #[test]
    fn legacy_bootstrap_retains_all_existing_node_and_port_references() {
        use super::super::DesktopNodeState;
        let before = DesktopNodeState::demo().unwrap().runtime.snapshot();
        let after = DesktopNodeState::with_local_identity(legacy_id())
            .unwrap()
            .runtime
            .snapshot();
        assert_eq!(before.local_node, after.local_node);
        assert_eq!(before.sessions, after.sessions);
        assert_eq!(before.routes, after.routes);
    }
}
