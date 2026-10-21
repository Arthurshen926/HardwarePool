#![forbid(unsafe_code)]
#![cfg(target_os = "linux")]
//! Linux persistence boundary. Callers explicitly supply protected storage.
//! `native-secret-service` offers an optional local OS keyring adapter; there is
//! never a plaintext fallback or automatic keyring discovery/unlock.
//! The exclusive owner holds its lock until drop. Recovery is explicit and local.

mod files;
#[cfg(feature = "native-secret-service")]
mod secret_service;
#[cfg(feature = "native-secret-service")]
pub use secret_service::SecretServiceBackend;

use capyio_control_credentials::{ControlCredentialBundle, MAX_CONTROL_CREDENTIAL_BUNDLE_BYTES};
use capyio_core::NodeId;
use capyio_enrollment::EnrollmentManager;
use capyio_pairing::{
    CredentialHandle, PairedPeerDirectory, PairedPeerRecord, PairingInvitation, PairingState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
type Bindings = BTreeMap<String, [u8; 32]>;
use thiserror::Error;

pub const HANDLE_PREFIX: &str = "linux-protected:";
const DIRECTORY: &str = "peers-v1.json";
const JOURNAL: &str = "pending-v1.json";
const MAX_DOCUMENT: usize = 131_072;

/// An exact protected-storage namespace. Implementations must scope every lookup
/// to BOTH owner and handle and reject duplicate matching items. Missing, locked,
/// unavailable and mismatched entries must never cause unlock, replacement or
/// collection creation. This trait deliberately has no delete operation.
pub trait CredentialBackend {
    fn load(&self, owner: NodeId, handle: &CredentialHandle) -> Result<SecretBytes, BackendError>;
    /// Create-only or byte-exact idempotent replay. Success requires protected
    /// service/storage acknowledgement and exact readback, not merely queuing a
    /// write. Refuse different existing values and duplicate matches. Standard
    /// keyring implementations may rely on cooperative namespace serialization
    /// rather than attribute CAS; their acknowledgement is not a power-loss or
    /// fsync guarantee. Implementations must disclose these trust assumptions.
    fn put_exact(
        &self,
        owner: NodeId,
        handle: &CredentialHandle,
        bytes: &[u8],
    ) -> Result<(), BackendError>;
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum BackendError {
    #[error("protected credential backend unavailable")]
    Unavailable,
    #[error("protected credential backend locked")]
    Locked,
    #[error("protected credential missing")]
    Missing,
    #[error("protected credential duplicate or mismatched")]
    Conflict,
    #[error("protected credential invalid")]
    Invalid,
}

/// Bounded secret buffer, intentionally without Debug/Serialize/Clone.
/// Clearing is best effort, not a swap/core-dump/compiler-erasure guarantee.
pub struct SecretBytes(Vec<u8>);
impl SecretBytes {
    pub fn new(mut bytes: Vec<u8>) -> Result<Self, BackendError> {
        if bytes.is_empty() || bytes.len() > MAX_CONTROL_CREDENTIAL_BUNDLE_BYTES {
            bytes.fill(0);
            return Err(BackendError::Invalid);
        }
        Ok(Self(bytes))
    }
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}
impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("pairing metadata I/O failed")]
    Io(#[from] std::io::Error),
    #[error("pairing store is already owned")]
    Busy,
    #[error("invalid or conflicting pairing metadata")]
    Metadata,
    #[error("selected peer is absent or revoked")]
    Inactive,
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error("invalid pairing package")]
    Pairing(#[from] capyio_pairing::PairingError),
    #[error("invalid control credential")]
    Credential(#[from] capyio_control_credentials::ControlCredentialError),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u8,
    owner: NodeId,
    directory: String,
    /// Integrity binding to the exact original high-entropy encoded credential.
    /// One digest per exact credential handle, never credential bytes.
    #[serde(deserialize_with = "unique_bindings")]
    credential_sha256: Bindings,
}

pub struct LinuxPairingStore<B: CredentialBackend> {
    files: files::Files,
    owner: NodeId,
    backend: B,
}

/// Typed input for the existing finite launcher's second and third frames.
/// No endpoint, duration, resource permission, lease or process is selected here.
pub struct BootstrapMaterial {
    directory: PairedPeerDirectory,
    credential: SecretBytes,
}
impl BootstrapMaterial {
    pub fn directory_json(&self) -> Result<Vec<u8>, StoreError> {
        Ok(self.directory.encode_json()?)
    }
    pub fn credential_bytes(&self) -> &[u8] {
        self.credential.as_slice()
    }
    pub fn into_credential(self) -> Result<ControlCredentialBundle, StoreError> {
        Ok(ControlCredentialBundle::decode(self.credential.as_slice())?)
    }
}

impl<B: CredentialBackend> LinuxPairingStore<B> {
    /// Opens an already-existing private (0700), current-user directory using
    /// no-follow directory-relative operations. Never changes permissions.
    pub fn open(root: &Path, owner: NodeId, backend: B) -> Result<Self, StoreError> {
        PairedPeerDirectory::new(owner)?;
        Ok(Self {
            files: files::Files::open(root)?,
            owner,
            backend,
        })
    }
    fn read(
        &self,
        name: &str,
        pending: bool,
    ) -> Result<Option<(PairedPeerDirectory, Bindings)>, StoreError> {
        let Some(bytes) = self.files.read(name, MAX_DOCUMENT)? else {
            return Ok(None);
        };
        let document: Document =
            serde_json::from_slice(&bytes).map_err(|_| StoreError::Metadata)?;
        if document.version != 1 || document.owner != self.owner {
            return Err(StoreError::Metadata);
        }
        let directory =
            PairedPeerDirectory::decode_json(self.owner, document.directory.as_bytes())?;
        validate_directory(&directory)?;
        if document.credential_sha256.len() != directory.peers().len()
            || directory.peers().any(|p| {
                !document
                    .credential_sha256
                    .contains_key(p.credential_handle.as_str())
            })
        {
            return Err(StoreError::Metadata);
        }
        if document
            .credential_sha256
            .iter()
            .any(|(handle, digest)| *handle != handle_for(*digest))
        {
            return Err(StoreError::Metadata);
        }
        if pending && (directory.peers().len() != 1 || directory.active_peers().count() != 1) {
            return Err(StoreError::Metadata);
        }
        Ok(Some((directory, document.credential_sha256)))
    }
    fn write(
        &self,
        name: &str,
        directory: &PairedPeerDirectory,
        digest: Bindings,
    ) -> Result<(), StoreError> {
        let document = Document {
            version: 1,
            owner: self.owner,
            directory: String::from_utf8(directory.encode_json()?)
                .map_err(|_| StoreError::Metadata)?,
            credential_sha256: digest,
        };
        let bytes = serde_json::to_vec(&document).map_err(|_| StoreError::Metadata)?;
        if bytes.len() > MAX_DOCUMENT {
            return Err(StoreError::Metadata);
        }
        self.files.write(name, &bytes)
    }
    pub fn load_directory(&self) -> Result<PairedPeerDirectory, StoreError> {
        Ok(self
            .read(DIRECTORY, false)?
            .map(|v| v.0)
            .unwrap_or(PairedPeerDirectory::new(self.owner)?))
    }
    /// Inspect only the bounded, validated nonsecret recovery journal.
    /// Presence is not proof of remote completion and never initiates recovery.
    pub fn has_pending_enrollment(&self) -> Result<bool, StoreError> {
        Ok(self.read(JOURNAL, true)?.is_some())
    }
    /// Only invoke via EnrollmentManager::stage_with after bilateral approval.
    pub fn stage_enrollment(&mut self, bytes: &[u8]) -> Result<(), StoreError> {
        self.files.check_owner()?;
        let invitation = PairingInvitation::decode_for(self.owner, bytes)?;
        let offered = PairedPeerDirectory::decode_json(self.owner, invitation.directory_json())?;
        validate_directory(&offered)?;
        if offered.peers().len() != 1 || offered.active_peers().count() != 1 {
            return Err(StoreError::Metadata);
        }
        let peer = offered.peers().next().ok_or(StoreError::Metadata)?;
        let digest: [u8; 32] = Sha256::digest(invitation.credential_bundle()).into();
        ControlCredentialBundle::decode(invitation.credential_bundle())?;
        if let Some((pending, existing_digest)) = self.read(JOURNAL, true)?
            && (pending.peers().ne(offered.peers())
                || existing_digest.get(peer.credential_handle.as_str()) != Some(&digest))
        {
            return Err(StoreError::Metadata);
        }
        if handle_for(digest) != peer.credential_handle.as_str() {
            return Err(StoreError::Metadata);
        }
        let (mut prospective, existing_bindings) = self
            .read(DIRECTORY, false)?
            .unwrap_or((PairedPeerDirectory::new(self.owner)?, Bindings::new()));
        check_existing(&prospective, peer)?;
        if let Some(existing_digest) = existing_bindings.get(peer.credential_handle.as_str()) {
            if *existing_digest != digest {
                return Err(StoreError::Metadata);
            }
            // An already-committed credential may never be recreated, even from an invitation.
            self.load_exact(peer, Some(*existing_digest))?;
        }
        if !prospective.peers().any(|p| p.node_id == peer.node_id) {
            prospective.add(peer.clone())?;
        }
        // Capacity/encoding failure must precede protected writes and journal staging.
        prospective.encode_json()?;
        self.backend.put_exact(
            self.owner,
            &peer.credential_handle,
            invitation.credential_bundle(),
        )?;
        // Read-back before acknowledging durable preparation; never delete on failure.
        self.load_exact(peer, Some(digest))?;
        self.write(
            JOURNAL,
            &offered,
            BTreeMap::from([(peer.credential_handle.as_str().to_owned(), digest)]),
        )
    }
    fn load_exact(
        &self,
        peer: &PairedPeerRecord,
        digest: Option<[u8; 32]>,
    ) -> Result<SecretBytes, StoreError> {
        if peer.state != PairingState::Active {
            return Err(StoreError::Inactive);
        }
        let bytes = self.backend.load(self.owner, &peer.credential_handle)?;
        if let Some(expected) = digest
            && <[u8; 32]>::from(Sha256::digest(bytes.as_slice())) != expected
        {
            return Err(StoreError::Backend(BackendError::Conflict));
        }
        ControlCredentialBundle::decode(bytes.as_slice())?;
        Ok(bytes)
    }
    /// Idempotent local replay of a durable journal; never creates an identity.
    /// Normal enrollment calls this only from persist_with (remote prepared).
    pub fn commit_pending_enrollment(&mut self) -> Result<Option<PairedPeerRecord>, StoreError> {
        let Some((pending, digest)) = self.read(JOURNAL, true)? else {
            return Ok(None);
        };
        let peer = pending.peers().next().ok_or(StoreError::Metadata)?.clone();
        let (mut directory, mut bindings) = self
            .read(DIRECTORY, false)?
            .unwrap_or((PairedPeerDirectory::new(self.owner)?, BTreeMap::new()));
        check_existing(&directory, &peer)?;
        self.load_exact(&peer, digest.get(peer.credential_handle.as_str()).copied())?;
        if !directory.peers().any(|p| p.node_id == peer.node_id) {
            directory.add(peer.clone())?;
        }
        bindings.extend(digest);
        self.write(DIRECTORY, &directory, bindings)?;
        self.files.remove(JOURNAL)?;
        Ok(Some(peer))
    }
    /// One poll, not a thread/service. Manager errors become terminal failure.
    pub fn poll_enrollment(&mut self, manager: &EnrollmentManager) -> Result<bool, StoreError> {
        self.files.check_owner()?;
        let result = (|| {
            manager
                .stage_with(|bytes| self.stage_enrollment(bytes).map_err(|e| e.to_string()))
                .map_err(|_| StoreError::Metadata)?;
            manager
                .persist_with(|bytes| {
                    // Revalidate the exact candidate before committing any journal.
                    self.stage_enrollment(bytes).map_err(|e| e.to_string())?;
                    self.commit_pending_enrollment()
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| "missing prepared enrollment".to_owned())?;
                    Ok(())
                })
                .map_err(|_| StoreError::Metadata)
        })();
        if let Err(error) = &result {
            manager.fail_persistence(error.to_string());
        }
        result
    }
    pub fn recover(&mut self, manager: &EnrollmentManager) -> Result<bool, StoreError> {
        self.files.check_owner()?;
        if !matches!(
            manager.status().phase.as_str(),
            "idle" | "failed" | "cancelled" | "recovered" | "complete"
        ) {
            return Err(StoreError::Busy);
        }
        let Some(peer) = self.commit_pending_enrollment()? else {
            return Ok(false);
        };
        manager.note_local_recovery(peer.node_id, peer.display_name);
        Ok(true)
    }
    /// Persist local revocation without removing a protected credential. A stale
    /// pending journal cannot reactivate this tombstone; remote propagation is separate.
    pub fn revoke(&mut self, peer_id: NodeId, at_ms: u64) -> Result<(), StoreError> {
        let (mut directory, bindings) = self.read(DIRECTORY, false)?.ok_or(StoreError::Inactive)?;
        directory.revoke(peer_id, at_ms)?;
        self.write(DIRECTORY, &directory, bindings)
    }
    pub fn bootstrap(&self, peer_id: NodeId) -> Result<BootstrapMaterial, StoreError> {
        // A corrupt/pending journal cannot silently be treated as completed pairing.
        if self.read(JOURNAL, true)?.is_some() {
            return Err(StoreError::Metadata);
        }
        let (directory, bindings) = self.read(DIRECTORY, false)?.ok_or(StoreError::Inactive)?;
        let peer = directory
            .active_peers()
            .find(|p| p.node_id == peer_id)
            .ok_or(StoreError::Inactive)?;
        let credential =
            self.load_exact(peer, bindings.get(peer.credential_handle.as_str()).copied())?;
        let mut selected = PairedPeerDirectory::new(self.owner)?;
        selected.add(peer.clone())?;
        Ok(BootstrapMaterial {
            directory: selected,
            credential,
        })
    }
}
fn validate_directory(directory: &PairedPeerDirectory) -> Result<(), StoreError> {
    let mut handles = std::collections::BTreeSet::new();
    for peer in directory.peers() {
        let suffix = peer
            .credential_handle
            .as_str()
            .strip_prefix(HANDLE_PREFIX)
            .ok_or(StoreError::Metadata)?;
        if suffix.len() != 64
            || !suffix
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !handles.insert(peer.credential_handle.as_str())
        {
            return Err(StoreError::Metadata);
        }
    }
    Ok(())
}
fn check_existing(
    directory: &PairedPeerDirectory,
    peer: &PairedPeerRecord,
) -> Result<(), StoreError> {
    if directory.peers().any(|p| {
        (p.node_id == peer.node_id && p != peer)
            || (p.node_id != peer.node_id && p.credential_handle == peer.credential_handle)
    }) {
        return Err(StoreError::Metadata);
    }
    Ok(())
}

#[cfg(test)]
mod tests;

fn handle_for(digest: [u8; 32]) -> String {
    use std::fmt::Write;
    let mut handle = HANDLE_PREFIX.to_owned();
    for byte in digest {
        write!(handle, "{byte:02x}").expect("String writing cannot fail");
    }
    handle
}
fn unique_bindings<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Bindings, D::Error> {
    struct Unique;
    impl<'de> serde::de::Visitor<'de> for Unique {
        type Value = Bindings;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("unique bounded credential bindings")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Bindings, A::Error> {
            let mut bindings = Bindings::new();
            while let Some((key, value)) = map.next_entry::<String, [u8; 32]>()? {
                if bindings.len() >= capyio_pairing::MAX_PAIRED_PEERS
                    || bindings.insert(key, value).is_some()
                {
                    return Err(serde::de::Error::custom(
                        "duplicate or excessive credential binding",
                    ));
                }
            }
            Ok(bindings)
        }
    }
    deserializer.deserialize_map(Unique)
}
