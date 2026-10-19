//! Non-secret resource consent, scoped to the current pairing generation.
use super::*;
use std::{fs, io::Read};

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
struct Permission {
    peer: NodeId,
    capability: CapabilityId,
    paired_at_ms: u64,
    certificate: [u8; 32],
}
fn current(dial: &ResolvedControlDial, capability: CapabilityId) -> Result<Permission, String> {
    let directory = dial.store.load_directory().map_err(|e| e.to_string())?;
    let peer = directory
        .peers()
        .find(|peer| {
            peer.node_id == dial.candidate.node_id
                && peer.state == PairingState::Active
                && peer.peer_certificate_sha256 == dial.candidate.peer_certificate_sha256
                && peer.credential_handle == dial.candidate.credential_handle
        })
        .ok_or("设备配对已更新或撤销，请重新连接")?;
    Ok(Permission {
        peer: peer.node_id,
        capability,
        paired_at_ms: peer.paired_at_ms,
        certificate: peer.peer_certificate_sha256.as_bytes(),
    })
}
fn path(dial: &ResolvedControlDial, capability: CapabilityId) -> std::path::PathBuf {
    dial.store
        .directory_path()
        .join("controller-permissions-v1")
        .join(format!("{}-{capability}.json", dial.candidate.node_id))
}
pub(super) fn allows(dial: &ResolvedControlDial, capability: CapabilityId) -> bool {
    let Ok(expected) = current(dial, capability) else {
        return false;
    };
    let Ok(file) = fs::File::open(path(dial, capability)) else {
        return false;
    };
    let mut bytes = Vec::new();
    if file.take(4097).read_to_end(&mut bytes).is_err() || bytes.len() > 4096 {
        return false;
    }
    serde_json::from_slice::<Permission>(&bytes).is_ok_and(|stored| stored == expected)
}
pub(super) fn remember(dial: &ResolvedControlDial, capability: CapabilityId) -> Result<(), String> {
    let permission = current(dial, capability)?;
    let path = path(dial, capability);
    fs::create_dir_all(path.parent().ok_or("permission directory unavailable")?)
        .map_err(|e| e.to_string())?;
    // This is a non-secret allow preference, never a certificate/private key.
    // An incomplete write is ignored by allows() and requires fresh consent.
    fs::write(
        path,
        serde_json::to_vec(&permission).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remembered_permission_survives_reload_but_not_pair_rotation_or_revocation() {
        use capyio_pairing::{CertificateFingerprint, CredentialHandle, PairedPeerRecord};
        let local = NodeId::new();
        let peer = NodeId::new();
        let base = std::env::temp_dir().join(format!("capyio-controller-consent-{}", local));
        let store = WindowsPairingStore::from_local_app_data(&base, local).unwrap();
        let certificate = CertificateFingerprint::new([7; 32]).unwrap();
        let handle = CredentialHandle::new("windows-dpapi:controller-consent-test").unwrap();
        let record =
            PairedPeerRecord::new(peer, "test", "peer.test", handle.clone(), certificate, 1)
                .unwrap();
        let write = |record: PairedPeerRecord| {
            let mut directory = PairedPeerDirectory::new(local).unwrap();
            directory.add(record).unwrap();
            store.commit_directory(&directory).unwrap();
        };
        write(record.clone());
        let dial = ResolvedControlDial {
            candidate: PairedControlCandidate {
                node_id: peer,
                endpoint: "127.0.0.1:43193".parse().unwrap(),
                server_name: "peer.test".into(),
                credential_handle: handle,
                peer_certificate_sha256: certificate,
                instance_nonce: 1,
            },
            display_name: "test".into(),
            store: store.clone(),
        };
        let capability = CapabilityId::new();
        assert!(!allows(&dial, capability));
        remember(&dial, capability).unwrap();
        assert!(allows(&dial, capability));
        assert!(!allows(&dial, CapabilityId::new()));
        let mut rotated = record.clone();
        rotated.paired_at_ms = 2;
        write(rotated);
        assert!(!allows(&dial, capability));
        let mut revoked = record;
        revoked.state = PairingState::Revoked { revoked_at_ms: 3 };
        write(revoked);
        assert!(!allows(&dial, capability));
        assert!(remember(&dial, capability).is_err());
        std::fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn permissions_are_per_resource_and_pair_generation() {
        let p = Permission {
            peer: NodeId::new(),
            capability: CapabilityId::new(),
            paired_at_ms: 1,
            certificate: [7; 32],
        };
        let mut other: Permission =
            serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
        assert_eq!(p, other);
        other.paired_at_ms = 2;
        assert_ne!(p, other);
        other.paired_at_ms = 1;
        other.certificate = [8; 32];
        assert_ne!(p, other);
        other.certificate = [7; 32];
        other.capability = CapabilityId::new();
        assert_ne!(p, other);
    }
}
