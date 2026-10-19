#![forbid(unsafe_code)]

//! Bounded post-pairing records. This crate never stores private keys and does
//! not turn an unauthenticated discovery packet into authority.

use std::{collections::BTreeMap, str::FromStr};

use capyio_core::NodeId;
use capyio_discovery::{DISCOVERY_CANDIDATE_TTL_MS, DiscoveryCandidate};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MAX_PAIRED_PEERS: usize = 64;
pub const MAX_PEER_DISPLAY_NAME_BYTES: usize = 128;
pub const MAX_SERVER_NAME_BYTES: usize = 253;
pub const MAX_CREDENTIAL_HANDLE_BYTES: usize = 256;
pub const MAX_PAIRED_DIRECTORY_JSON_BYTES: usize = 65_536;
pub const PAIRED_DIRECTORY_SCHEMA_VERSION: u8 = 1;
pub const MAX_PAIRING_INVITATION_CREDENTIAL_BYTES: usize = 65_536;
pub const MAX_PAIRING_INVITATION_BYTES: usize =
    28 + MAX_PAIRED_DIRECTORY_JSON_BYTES + MAX_PAIRING_INVITATION_CREDENTIAL_BYTES;
const PAIRING_INVITATION_MAGIC: [u8; 4] = *b"CPI1";

/// A bounded, target-bound bootstrap package. The credential payload remains
/// opaque here so platform secure stores own its interpretation and lifetime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingInvitation {
    target_node_id: NodeId,
    directory_json: Vec<u8>,
    credential_bundle: Vec<u8>,
}

impl PairingInvitation {
    pub fn new(
        target_node_id: NodeId,
        directory_json: Vec<u8>,
        credential_bundle: Vec<u8>,
    ) -> Result<Self, PairingError> {
        if target_node_id.as_uuid().is_nil() {
            return Err(PairingError::InvalidNodeId);
        }
        if directory_json.is_empty() || directory_json.len() > MAX_PAIRED_DIRECTORY_JSON_BYTES {
            return Err(PairingError::DirectoryTooLarge);
        }
        if credential_bundle.is_empty()
            || credential_bundle.len() > MAX_PAIRING_INVITATION_CREDENTIAL_BYTES
        {
            return Err(PairingError::InvitationCredentialSize);
        }
        Ok(Self {
            target_node_id,
            directory_json,
            credential_bundle,
        })
    }

    #[must_use]
    pub fn target_node_id(&self) -> NodeId {
        self.target_node_id
    }

    #[must_use]
    pub fn directory_json(&self) -> &[u8] {
        &self.directory_json
    }

    #[must_use]
    pub fn credential_bundle(&self) -> &[u8] {
        &self.credential_bundle
    }

    pub fn encode(&self) -> Result<Vec<u8>, PairingError> {
        let directory_len = u32::try_from(self.directory_json.len())
            .map_err(|_| PairingError::InvitationTooLarge)?;
        let credential_len = u32::try_from(self.credential_bundle.len())
            .map_err(|_| PairingError::InvitationTooLarge)?;
        let mut encoded =
            Vec::with_capacity(28 + self.directory_json.len() + self.credential_bundle.len());
        encoded.extend_from_slice(&PAIRING_INVITATION_MAGIC);
        encoded.extend_from_slice(self.target_node_id.as_uuid().as_bytes());
        encoded.extend_from_slice(&directory_len.to_be_bytes());
        encoded.extend_from_slice(&credential_len.to_be_bytes());
        encoded.extend_from_slice(&self.directory_json);
        encoded.extend_from_slice(&self.credential_bundle);
        Ok(encoded)
    }

    pub fn decode_for(target_node_id: NodeId, encoded: &[u8]) -> Result<Self, PairingError> {
        if encoded.len() < 28 || encoded.len() > MAX_PAIRING_INVITATION_BYTES {
            return Err(PairingError::InvitationTooLarge);
        }
        if encoded[..4] != PAIRING_INVITATION_MAGIC {
            return Err(PairingError::InvalidInvitation);
        }
        let embedded_node_id = NodeId::from(uuid::Uuid::from_bytes(
            encoded[4..20]
                .try_into()
                .map_err(|_| PairingError::InvalidInvitation)?,
        ));
        if embedded_node_id != target_node_id {
            return Err(PairingError::InvitationTargetMismatch);
        }
        let directory_len = usize::try_from(u32::from_be_bytes(
            encoded[20..24]
                .try_into()
                .map_err(|_| PairingError::InvalidInvitation)?,
        ))
        .map_err(|_| PairingError::InvitationTooLarge)?;
        let credential_len = usize::try_from(u32::from_be_bytes(
            encoded[24..28]
                .try_into()
                .map_err(|_| PairingError::InvalidInvitation)?,
        ))
        .map_err(|_| PairingError::InvitationTooLarge)?;
        let expected = 28_usize
            .checked_add(directory_len)
            .and_then(|value| value.checked_add(credential_len))
            .ok_or(PairingError::InvitationTooLarge)?;
        if expected != encoded.len() {
            return Err(PairingError::InvalidInvitation);
        }
        Self::new(
            embedded_node_id,
            encoded[28..28 + directory_len].to_vec(),
            encoded[28 + directory_len..].to_vec(),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CertificateFingerprint([u8; 32]);

impl CertificateFingerprint {
    pub fn new(bytes: [u8; 32]) -> Result<Self, PairingError> {
        if bytes == [0; 32] {
            return Err(PairingError::InvalidCertificateFingerprint);
        }
        Ok(Self(bytes))
    }

    #[must_use]
    pub fn from_certificate_der(certificate_der: &[u8]) -> Option<Self> {
        if certificate_der.is_empty() {
            return None;
        }
        let bytes: [u8; 32] = Sha256::digest(certificate_der).into();
        Self::new(bytes).ok()
    }

    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialHandle(String);

impl CredentialHandle {
    pub fn new(value: impl Into<String>) -> Result<Self, PairingError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_CREDENTIAL_HANDLE_BYTES
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':')
            })
        {
            return Err(PairingError::InvalidCredentialHandle);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PairingState {
    Active,
    Revoked { revoked_at_ms: u64 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairedPeerRecord {
    pub node_id: NodeId,
    pub display_name: String,
    pub server_name: String,
    pub credential_handle: CredentialHandle,
    pub peer_certificate_sha256: CertificateFingerprint,
    pub paired_at_ms: u64,
    pub state: PairingState,
}

impl PairedPeerRecord {
    pub fn new(
        node_id: NodeId,
        display_name: impl Into<String>,
        server_name: impl Into<String>,
        credential_handle: CredentialHandle,
        peer_certificate_sha256: CertificateFingerprint,
        paired_at_ms: u64,
    ) -> Result<Self, PairingError> {
        if node_id.as_uuid().is_nil() {
            return Err(PairingError::InvalidNodeId);
        }
        let display_name = display_name.into();
        validate_display_name(&display_name)?;
        let server_name = server_name.into();
        validate_server_name(&server_name)?;
        Ok(Self {
            node_id,
            display_name,
            server_name,
            credential_handle,
            peer_certificate_sha256,
            paired_at_ms,
            state: PairingState::Active,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairedControlCandidate {
    pub node_id: NodeId,
    pub endpoint: std::net::SocketAddr,
    pub server_name: String,
    pub credential_handle: CredentialHandle,
    pub peer_certificate_sha256: CertificateFingerprint,
    pub instance_nonce: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CandidateTrust {
    Expired,
    Unpaired,
    Revoked,
    Paired(PairedControlCandidate),
}

#[derive(Clone, Debug)]
pub struct PairedPeerDirectory {
    local_node_id: NodeId,
    peers: BTreeMap<NodeId, PairedPeerRecord>,
}

impl PairedPeerDirectory {
    pub fn new(local_node_id: NodeId) -> Result<Self, PairingError> {
        if local_node_id.as_uuid().is_nil() {
            return Err(PairingError::InvalidNodeId);
        }
        Ok(Self {
            local_node_id,
            peers: BTreeMap::new(),
        })
    }

    pub fn add(&mut self, record: PairedPeerRecord) -> Result<(), PairingError> {
        if record.node_id == self.local_node_id {
            return Err(PairingError::LocalNodeCannotBePaired);
        }
        if self.peers.contains_key(&record.node_id) {
            return Err(PairingError::DuplicatePeer(record.node_id));
        }
        if self.peers.len() >= MAX_PAIRED_PEERS {
            return Err(PairingError::CapacityReached);
        }
        self.peers.insert(record.node_id, record);
        Ok(())
    }

    pub fn revoke(&mut self, node_id: NodeId, revoked_at_ms: u64) -> Result<(), PairingError> {
        let peer = self
            .peers
            .get_mut(&node_id)
            .ok_or(PairingError::UnknownPeer(node_id))?;
        if revoked_at_ms < peer.paired_at_ms {
            return Err(PairingError::InvalidRevocationTime);
        }
        peer.state = PairingState::Revoked { revoked_at_ms };
        Ok(())
    }

    #[must_use]
    pub fn classify(&self, candidate: DiscoveryCandidate, now_ms: u64) -> CandidateTrust {
        if now_ms.saturating_sub(candidate.last_seen_ms) > DISCOVERY_CANDIDATE_TTL_MS {
            return CandidateTrust::Expired;
        }
        let Some(peer) = self.peers.get(&candidate.node_id) else {
            return CandidateTrust::Unpaired;
        };
        if matches!(peer.state, PairingState::Revoked { .. }) {
            return CandidateTrust::Revoked;
        }
        CandidateTrust::Paired(PairedControlCandidate {
            node_id: candidate.node_id,
            endpoint: candidate.endpoint,
            server_name: peer.server_name.clone(),
            credential_handle: peer.credential_handle.clone(),
            peer_certificate_sha256: peer.peer_certificate_sha256,
            instance_nonce: candidate.instance_nonce,
        })
    }

    pub fn peers(&self) -> impl ExactSizeIterator<Item = &PairedPeerRecord> {
        self.peers.values()
    }

    /// Returns only peers which remain authorized for new authenticated
    /// control sessions. Revoked records stay in the directory as audit
    /// evidence, but must never cause their credential to be recovered.
    pub fn active_peers(&self) -> impl Iterator<Item = &PairedPeerRecord> {
        self.peers
            .values()
            .filter(|peer| matches!(peer.state, PairingState::Active))
    }

    pub fn encode_json(&self) -> Result<Vec<u8>, PairingError> {
        let document = PairedDirectoryDocument {
            schema_version: PAIRED_DIRECTORY_SCHEMA_VERSION,
            peers: self
                .peers
                .values()
                .map(PairedPeerDocument::from_record)
                .collect(),
        };
        let encoded =
            serde_json::to_vec(&document).map_err(|_| PairingError::InvalidDirectoryJson)?;
        if encoded.len() > MAX_PAIRED_DIRECTORY_JSON_BYTES {
            return Err(PairingError::DirectoryTooLarge);
        }
        Ok(encoded)
    }

    pub fn decode_json(local_node_id: NodeId, encoded: &[u8]) -> Result<Self, PairingError> {
        if encoded.len() > MAX_PAIRED_DIRECTORY_JSON_BYTES {
            return Err(PairingError::DirectoryTooLarge);
        }
        let document: PairedDirectoryDocument =
            serde_json::from_slice(encoded).map_err(|_| PairingError::InvalidDirectoryJson)?;
        if document.schema_version != PAIRED_DIRECTORY_SCHEMA_VERSION {
            return Err(PairingError::UnsupportedDirectorySchema);
        }
        if document.peers.len() > MAX_PAIRED_PEERS {
            return Err(PairingError::CapacityReached);
        }
        let mut directory = Self::new(local_node_id)?;
        for peer in document.peers {
            let node_id =
                NodeId::from_str(&peer.node_id).map_err(|_| PairingError::InvalidNodeId)?;
            let fingerprint = decode_fingerprint(&peer.peer_certificate_sha256)?;
            let mut record = PairedPeerRecord::new(
                node_id,
                peer.display_name,
                peer.server_name,
                CredentialHandle::new(peer.credential_handle)?,
                fingerprint,
                peer.paired_at_ms,
            )?;
            if let StoredPairingState::Revoked { revoked_at_ms } = peer.pairing {
                if revoked_at_ms < record.paired_at_ms {
                    return Err(PairingError::InvalidRevocationTime);
                }
                record.state = PairingState::Revoked { revoked_at_ms };
            }
            directory.add(record)?;
        }
        Ok(directory)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PairedDirectoryDocument {
    schema_version: u8,
    peers: Vec<PairedPeerDocument>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PairedPeerDocument {
    node_id: String,
    display_name: String,
    server_name: String,
    credential_handle: String,
    peer_certificate_sha256: String,
    paired_at_ms: u64,
    pairing: StoredPairingState,
}

impl PairedPeerDocument {
    fn from_record(record: &PairedPeerRecord) -> Self {
        Self {
            node_id: record.node_id.to_string(),
            display_name: record.display_name.clone(),
            server_name: record.server_name.clone(),
            credential_handle: record.credential_handle.as_str().to_owned(),
            peer_certificate_sha256: encode_fingerprint(record.peer_certificate_sha256),
            paired_at_ms: record.paired_at_ms,
            pairing: match record.state {
                PairingState::Active => StoredPairingState::Active,
                PairingState::Revoked { revoked_at_ms } => {
                    StoredPairingState::Revoked { revoked_at_ms }
                }
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
enum StoredPairingState {
    Active,
    Revoked { revoked_at_ms: u64 },
}

fn encode_fingerprint(fingerprint: CertificateFingerprint) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = fingerprint.as_bytes();
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn decode_fingerprint(value: &str) -> Result<CertificateFingerprint, PairingError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(PairingError::InvalidCertificateFingerprint);
    }
    let mut decoded = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        decoded[index] = (decode_hex(pair[0])? << 4) | decode_hex(pair[1])?;
    }
    CertificateFingerprint::new(decoded)
}

fn decode_hex(value: u8) -> Result<u8, PairingError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(PairingError::InvalidCertificateFingerprint),
    }
}

fn validate_display_name(value: &str) -> Result<(), PairingError> {
    if value.is_empty()
        || value.len() > MAX_PEER_DISPLAY_NAME_BYTES
        || value.chars().any(char::is_control)
    {
        Err(PairingError::InvalidDisplayName)
    } else {
        Ok(())
    }
}

fn validate_server_name(value: &str) -> Result<(), PairingError> {
    if value.is_empty()
        || value.len() > MAX_SERVER_NAME_BYTES
        || !value.is_ascii()
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':' | b'[' | b']')
        })
    {
        Err(PairingError::InvalidServerName)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PairingError {
    #[error("paired Node ID is invalid")]
    InvalidNodeId,
    #[error("local Node cannot be added as a paired peer")]
    LocalNodeCannotBePaired,
    #[error("paired peer display name is invalid")]
    InvalidDisplayName,
    #[error("paired peer TLS server name is invalid")]
    InvalidServerName,
    #[error("secure credential handle is invalid")]
    InvalidCredentialHandle,
    #[error("peer certificate fingerprint is invalid")]
    InvalidCertificateFingerprint,
    #[error("paired peer {0} already exists; credential replacement must be explicit")]
    DuplicatePeer(NodeId),
    #[error("paired peer capacity reached")]
    CapacityReached,
    #[error("paired peer {0} is unknown")]
    UnknownPeer(NodeId),
    #[error("revocation time predates pairing")]
    InvalidRevocationTime,
    #[error("paired-peer directory JSON exceeds the size limit")]
    DirectoryTooLarge,
    #[error("paired-peer directory JSON is invalid")]
    InvalidDirectoryJson,
    #[error("paired-peer directory schema version is unsupported")]
    UnsupportedDirectorySchema,
    #[error("pairing invitation is malformed")]
    InvalidInvitation,
    #[error("pairing invitation targets a different Node")]
    InvitationTargetMismatch,
    #[error("pairing invitation exceeds the size limit")]
    InvitationTooLarge,
    #[error("pairing invitation credential size is invalid")]
    InvitationCredentialSize,
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use super::*;

    #[test]
    fn exact_pairing_upgrades_only_matching_fresh_discovery_candidate() {
        let local = NodeId::new();
        let peer = NodeId::new();
        let mut directory = PairedPeerDirectory::new(local).expect("directory");
        directory.add(record(peer, 100)).expect("pair peer");
        let candidate = candidate(peer, 110);

        let CandidateTrust::Paired(resolved) = directory.classify(candidate, 120) else {
            panic!("expected paired candidate");
        };
        assert_eq!(resolved.node_id, peer);
        assert_eq!(resolved.endpoint, candidate.endpoint);
        assert_eq!(resolved.server_name, "peer.capyio.test");
        assert_eq!(resolved.credential_handle.as_str(), "os-keystore:peer-1");
        assert_eq!(resolved.instance_nonce, candidate.instance_nonce);
    }

    #[test]
    fn unpaired_expired_and_revoked_candidates_never_become_connectable() {
        let local = NodeId::new();
        let peer = NodeId::new();
        let mut directory = PairedPeerDirectory::new(local).expect("directory");
        assert_eq!(
            directory.classify(candidate(peer, 10), 20),
            CandidateTrust::Unpaired
        );
        directory.add(record(peer, 30)).expect("pair peer");
        assert_eq!(
            directory.classify(candidate(peer, 10), 10 + DISCOVERY_CANDIDATE_TTL_MS + 1),
            CandidateTrust::Expired
        );
        directory.revoke(peer, 40).expect("revoke");
        assert_eq!(
            directory.classify(candidate(peer, 50), 51),
            CandidateTrust::Revoked
        );
    }

    #[test]
    fn pairing_records_reject_secret_like_or_ambiguous_metadata() {
        assert!(CredentialHandle::new("../../private-key.pem").is_err());
        assert!(CredentialHandle::new("os-keystore:peer_1").is_ok());
        assert!(CertificateFingerprint::from_certificate_der(&[]).is_none());
        assert!(CertificateFingerprint::new([0; 32]).is_err());
        let handle = CredentialHandle::new("os-keystore:peer-1").expect("handle");
        let fingerprint =
            CertificateFingerprint::from_certificate_der(b"certificate").expect("fingerprint");
        assert!(matches!(
            PairedPeerRecord::new(
                NodeId::new(),
                "bad\nname",
                "peer.capyio.test",
                handle.clone(),
                fingerprint,
                1,
            ),
            Err(PairingError::InvalidDisplayName)
        ));
        assert!(matches!(
            PairedPeerRecord::new(
                NodeId::new(),
                "Peer",
                "https://peer/",
                handle,
                fingerprint,
                1,
            ),
            Err(PairingError::InvalidServerName)
        ));
    }

    #[test]
    fn directory_is_bounded_and_rejects_silent_repairing() {
        let local = NodeId::new();
        let mut directory = PairedPeerDirectory::new(local).expect("directory");
        let first = NodeId::new();
        directory.add(record(first, 1)).expect("first");
        assert!(matches!(
            directory.add(record(first, 2)),
            Err(PairingError::DuplicatePeer(id)) if id == first
        ));
        for index in 1..MAX_PAIRED_PEERS {
            directory
                .add(record(NodeId::new(), index as u64 + 2))
                .expect("bounded peer");
        }
        assert_eq!(directory.peers().len(), MAX_PAIRED_PEERS);
        assert!(matches!(
            directory.add(record(NodeId::new(), 1000)),
            Err(PairingError::CapacityReached)
        ));
        assert_eq!(
            directory.revoke(first, 0),
            Err(PairingError::InvalidRevocationTime)
        );
    }

    #[test]
    fn non_secret_directory_round_trips_deterministically() {
        let local = NodeId::new();
        let first = NodeId::new();
        let second = NodeId::new();
        let mut directory = PairedPeerDirectory::new(local).expect("directory");
        directory.add(record(first, 10)).expect("first peer");
        directory.add(record(second, 20)).expect("second peer");
        directory.revoke(second, 30).expect("revoke second");

        let encoded = directory.encode_json().expect("encode");
        let text = std::str::from_utf8(&encoded).expect("UTF-8");
        assert!(text.contains("\"schemaVersion\":1"));
        assert!(text.contains("\"credentialHandle\":\"os-keystore:peer-1\""));
        assert!(!text.contains("certificate"));
        assert!(!text.contains("private"));

        let decoded = PairedPeerDirectory::decode_json(local, &encoded).expect("decode");
        assert_eq!(decoded.peers().len(), 2);
        assert!(matches!(
            decoded
                .peers()
                .find(|peer| peer.node_id == second)
                .expect("second")
                .state,
            PairingState::Revoked { revoked_at_ms: 30 }
        ));
        assert_eq!(decoded.encode_json().expect("re-encode"), encoded);
    }

    #[test]
    fn directory_decode_fails_closed_on_schema_shape_size_and_duplicates() {
        let local = NodeId::new();
        assert_eq!(
            PairedPeerDirectory::decode_json(local, br#"{"schemaVersion":2,"peers":[]}"#)
                .expect_err("schema"),
            PairingError::UnsupportedDirectorySchema
        );
        assert_eq!(
            PairedPeerDirectory::decode_json(
                local,
                br#"{"schemaVersion":1,"peers":[],"privateKey":"bad"}"#,
            )
            .expect_err("unknown field"),
            PairingError::InvalidDirectoryJson
        );
        assert_eq!(
            PairedPeerDirectory::decode_json(
                local,
                &vec![b' '; MAX_PAIRED_DIRECTORY_JSON_BYTES + 1],
            )
            .expect_err("oversize"),
            PairingError::DirectoryTooLarge
        );

        let peer = NodeId::new();
        let mut directory = PairedPeerDirectory::new(local).expect("directory");
        directory.add(record(peer, 1)).expect("peer");
        let one = String::from_utf8(directory.encode_json().expect("encode")).expect("UTF-8");
        let peer_object = one
            .split_once("\"peers\":[")
            .expect("peers prefix")
            .1
            .strip_suffix("]}")
            .expect("peers suffix");
        let duplicate = format!("{{\"schemaVersion\":1,\"peers\":[{peer_object},{peer_object}]}}");
        assert!(matches!(
            PairedPeerDirectory::decode_json(local, duplicate.as_bytes()),
            Err(PairingError::DuplicatePeer(id)) if id == peer
        ));
    }

    #[test]
    fn pairing_invitation_round_trips_and_is_bound_to_its_target() {
        let target = NodeId::new();
        let invitation = PairingInvitation::new(
            target,
            br#"{"schemaVersion":1,"peers":[]}"#.to_vec(),
            b"opaque-credential".to_vec(),
        )
        .expect("invitation");
        let encoded = invitation.encode().expect("encode");
        assert_eq!(
            PairingInvitation::decode_for(target, &encoded).unwrap(),
            invitation
        );
        assert_eq!(
            PairingInvitation::decode_for(NodeId::new(), &encoded).unwrap_err(),
            PairingError::InvitationTargetMismatch
        );
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert_eq!(
            PairingInvitation::decode_for(target, &trailing).unwrap_err(),
            PairingError::InvalidInvitation
        );
        assert!(PairingInvitation::decode_for(target, &encoded[..27]).is_err());
    }

    fn record(node_id: NodeId, paired_at_ms: u64) -> PairedPeerRecord {
        PairedPeerRecord::new(
            node_id,
            "Test peer",
            "peer.capyio.test",
            CredentialHandle::new("os-keystore:peer-1").expect("handle"),
            CertificateFingerprint::from_certificate_der(b"certificate").expect("fingerprint"),
            paired_at_ms,
        )
        .expect("record")
    }

    fn candidate(node_id: NodeId, last_seen_ms: u64) -> DiscoveryCandidate {
        DiscoveryCandidate {
            node_id,
            endpoint: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 9)), 42420),
            instance_nonce: 9,
            last_seen_ms,
        }
    }
}
