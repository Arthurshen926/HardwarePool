#![forbid(unsafe_code)]

//! Fixed-size LAN discovery candidate records. This crate performs no I/O and
//! grants no trust: a decoded record is only a hint for a later authenticated
//! connection attempt.

use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
};

use capyio_core::NodeId;
use thiserror::Error;
use uuid::Uuid;

pub const DISCOVERY_RECORD_BYTES: usize = 32;
pub const DISCOVERY_VERSION: u8 = 1;
const MAGIC: [u8; 4] = *b"CPYD";
pub const MAX_DISCOVERY_CANDIDATES: usize = 64;
pub const DISCOVERY_CANDIDATE_TTL_MS: u64 = 15_000;
pub const DISCOVERY_RATE_WINDOW_MS: u64 = 1_000;
pub const MAX_DISCOVERY_DATAGRAMS_PER_WINDOW: u16 = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscoveryAnnouncement {
    pub node_id: NodeId,
    pub control_port: u16,
    /// Random per-process value used only to distinguish restarts. It is not a
    /// credential, challenge, replay window or stable identity.
    pub instance_nonce: u64,
}

impl DiscoveryAnnouncement {
    pub fn new(
        node_id: NodeId,
        control_port: u16,
        instance_nonce: u64,
    ) -> Result<Self, DiscoveryError> {
        if node_id.as_uuid().is_nil() {
            return Err(DiscoveryError::NilNodeId);
        }
        if control_port == 0 {
            return Err(DiscoveryError::InvalidControlPort);
        }
        if instance_nonce == 0 {
            return Err(DiscoveryError::InvalidInstanceNonce);
        }
        Ok(Self {
            node_id,
            control_port,
            instance_nonce,
        })
    }

    #[must_use]
    pub fn encode(self) -> [u8; DISCOVERY_RECORD_BYTES] {
        let mut bytes = [0_u8; DISCOVERY_RECORD_BYTES];
        bytes[..4].copy_from_slice(&MAGIC);
        bytes[4] = DISCOVERY_VERSION;
        // Byte 5 is the flags field. Version 1 requires its zero value.
        bytes[6..8].copy_from_slice(&self.control_port.to_be_bytes());
        bytes[8..24].copy_from_slice(self.node_id.as_uuid().as_bytes());
        bytes[24..32].copy_from_slice(&self.instance_nonce.to_be_bytes());
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DiscoveryError> {
        if bytes.len() != DISCOVERY_RECORD_BYTES {
            return Err(DiscoveryError::InvalidLength(bytes.len()));
        }
        if bytes[..4] != MAGIC {
            return Err(DiscoveryError::InvalidMagic);
        }
        if bytes[4] != DISCOVERY_VERSION {
            return Err(DiscoveryError::UnsupportedVersion(bytes[4]));
        }
        if bytes[5] != 0 {
            return Err(DiscoveryError::UnknownFlags(bytes[5]));
        }
        let control_port = u16::from_be_bytes([bytes[6], bytes[7]]);
        let node_id = NodeId::from_uuid(
            Uuid::from_slice(&bytes[8..24]).map_err(|_| DiscoveryError::InvalidNodeId)?,
        );
        let instance_nonce = u64::from_be_bytes(
            bytes[24..32]
                .try_into()
                .map_err(|_| DiscoveryError::InvalidInstanceNonce)?,
        );
        Self::new(node_id, control_port, instance_nonce)
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum DiscoveryError {
    #[error("discovery record has invalid length {0}")]
    InvalidLength(usize),
    #[error("discovery record has unknown magic")]
    InvalidMagic,
    #[error("discovery record version {0} is unsupported")]
    UnsupportedVersion(u8),
    #[error("discovery record flags {0:#04x} are unknown")]
    UnknownFlags(u8),
    #[error("discovery record has an invalid Node ID")]
    InvalidNodeId,
    #[error("discovery record must not use the nil Node ID")]
    NilNodeId,
    #[error("discovery control port must be non-zero")]
    InvalidControlPort,
    #[error("discovery instance nonce must be non-zero")]
    InvalidInstanceNonce,
    #[error("local discovery announcements are ignored")]
    SelfAnnouncement,
    #[error("discovery packet source must be a non-loopback unicast address")]
    InvalidSourceAddress,
    #[error("discovery candidate capacity is reached")]
    CandidateCapacityReached,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum DiscoveryIngressError {
    #[error("discovery observation time moved backwards")]
    NonMonotonicTime,
    #[error("discovery datagram rate limit exceeded")]
    RateLimitExceeded,
    #[error(transparent)]
    Candidate(#[from] DiscoveryError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscoveryCandidate {
    pub node_id: NodeId,
    pub endpoint: SocketAddr,
    pub instance_nonce: u64,
    pub last_seen_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObserveOutcome {
    Added,
    Refreshed,
    Restarted,
    IgnoredStale,
}

/// Bounded candidate memory for a platform-owned discovery listener. Time is
/// caller supplied to keep this contract deterministic and runtime agnostic.
#[derive(Clone, Debug)]
pub struct DiscoveryCandidateSet {
    local_node_id: NodeId,
    candidates: BTreeMap<NodeId, DiscoveryCandidate>,
}

impl DiscoveryCandidateSet {
    pub fn new(local_node_id: NodeId) -> Result<Self, DiscoveryError> {
        if local_node_id.as_uuid().is_nil() {
            return Err(DiscoveryError::NilNodeId);
        }
        Ok(Self {
            local_node_id,
            candidates: BTreeMap::new(),
        })
    }

    pub fn observe(
        &mut self,
        source_ip: IpAddr,
        announcement: DiscoveryAnnouncement,
        observed_at_ms: u64,
    ) -> Result<ObserveOutcome, DiscoveryError> {
        if announcement.node_id == self.local_node_id {
            return Err(DiscoveryError::SelfAnnouncement);
        }
        if source_ip.is_unspecified() || source_ip.is_loopback() || source_ip.is_multicast() {
            return Err(DiscoveryError::InvalidSourceAddress);
        }
        let endpoint = SocketAddr::new(source_ip, announcement.control_port);
        if let Some(existing) = self.candidates.get_mut(&announcement.node_id) {
            if observed_at_ms < existing.last_seen_ms {
                return Ok(ObserveOutcome::IgnoredStale);
            }
            let outcome = if existing.instance_nonce == announcement.instance_nonce {
                ObserveOutcome::Refreshed
            } else {
                ObserveOutcome::Restarted
            };
            *existing = DiscoveryCandidate {
                node_id: announcement.node_id,
                endpoint,
                instance_nonce: announcement.instance_nonce,
                last_seen_ms: observed_at_ms,
            };
            return Ok(outcome);
        }
        if self.candidates.len() >= MAX_DISCOVERY_CANDIDATES {
            return Err(DiscoveryError::CandidateCapacityReached);
        }
        self.candidates.insert(
            announcement.node_id,
            DiscoveryCandidate {
                node_id: announcement.node_id,
                endpoint,
                instance_nonce: announcement.instance_nonce,
                last_seen_ms: observed_at_ms,
            },
        );
        Ok(ObserveOutcome::Added)
    }

    pub fn expire(&mut self, now_ms: u64) -> usize {
        let before = self.candidates.len();
        self.candidates.retain(|_, candidate| {
            now_ms.saturating_sub(candidate.last_seen_ms) <= DISCOVERY_CANDIDATE_TTL_MS
        });
        before - self.candidates.len()
    }

    pub fn candidates(&self) -> impl ExactSizeIterator<Item = &DiscoveryCandidate> {
        self.candidates.values()
    }
}

/// Bounded platform-host ingress placed directly behind a UDP receive loop.
/// It performs no I/O and counts malformed packets toward the fixed rate limit.
#[derive(Clone, Debug)]
pub struct DiscoveryIngress {
    candidates: DiscoveryCandidateSet,
    window_started_ms: Option<u64>,
    datagrams_in_window: u16,
}

impl DiscoveryIngress {
    pub fn new(local_node_id: NodeId) -> Result<Self, DiscoveryError> {
        Ok(Self {
            candidates: DiscoveryCandidateSet::new(local_node_id)?,
            window_started_ms: None,
            datagrams_in_window: 0,
        })
    }

    pub fn ingest(
        &mut self,
        source_ip: IpAddr,
        datagram: &[u8],
        observed_at_ms: u64,
    ) -> Result<ObserveOutcome, DiscoveryIngressError> {
        match self.window_started_ms {
            Some(started) if observed_at_ms < started => {
                return Err(DiscoveryIngressError::NonMonotonicTime);
            }
            Some(started) if observed_at_ms.saturating_sub(started) >= DISCOVERY_RATE_WINDOW_MS => {
                self.window_started_ms = Some(observed_at_ms);
                self.datagrams_in_window = 0;
            }
            None => self.window_started_ms = Some(observed_at_ms),
            Some(_) => {}
        }
        if self.datagrams_in_window >= MAX_DISCOVERY_DATAGRAMS_PER_WINDOW {
            return Err(DiscoveryIngressError::RateLimitExceeded);
        }
        self.datagrams_in_window += 1;
        let announcement = DiscoveryAnnouncement::decode(datagram)?;
        self.candidates
            .observe(source_ip, announcement, observed_at_ms)
            .map_err(Into::into)
    }

    pub fn expire(&mut self, now_ms: u64) -> usize {
        self.candidates.expire(now_ms)
    }

    pub fn candidates(&self) -> impl ExactSizeIterator<Item = &DiscoveryCandidate> {
        self.candidates.candidates()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn announcement() -> DiscoveryAnnouncement {
        DiscoveryAnnouncement::new(NodeId::new(), 42420, 0x0102_0304_0506_0708)
            .expect("valid announcement")
    }

    #[test]
    fn exact_record_round_trips_without_endpoint_or_secret() {
        let value = announcement();
        let encoded = value.encode();
        assert_eq!(encoded.len(), DISCOVERY_RECORD_BYTES);
        assert_eq!(&encoded[..8], b"CPYD\x01\x00\xa5\xb4");
        assert_eq!(DiscoveryAnnouncement::decode(&encoded), Ok(value));
    }

    #[test]
    fn framing_and_future_fields_fail_closed() {
        let encoded = announcement().encode();
        for length in [0, 1, DISCOVERY_RECORD_BYTES - 1] {
            assert!(matches!(
                DiscoveryAnnouncement::decode(&encoded[..length]),
                Err(DiscoveryError::InvalidLength(_))
            ));
        }
        let mut trailing = encoded.to_vec();
        trailing.push(0);
        assert!(matches!(
            DiscoveryAnnouncement::decode(&trailing),
            Err(DiscoveryError::InvalidLength(_))
        ));
        let mut invalid = encoded;
        invalid[0] = b'X';
        assert_eq!(
            DiscoveryAnnouncement::decode(&invalid),
            Err(DiscoveryError::InvalidMagic)
        );
        invalid = encoded;
        invalid[4] = 2;
        assert_eq!(
            DiscoveryAnnouncement::decode(&invalid),
            Err(DiscoveryError::UnsupportedVersion(2))
        );
        invalid = encoded;
        invalid[5] = 1;
        assert_eq!(
            DiscoveryAnnouncement::decode(&invalid),
            Err(DiscoveryError::UnknownFlags(1))
        );
    }

    #[test]
    fn zero_identity_port_and_nonce_are_rejected() {
        assert_eq!(
            DiscoveryAnnouncement::new(NodeId::from_uuid(Uuid::nil()), 1, 1),
            Err(DiscoveryError::NilNodeId)
        );
        assert_eq!(
            DiscoveryAnnouncement::new(NodeId::new(), 0, 1),
            Err(DiscoveryError::InvalidControlPort)
        );
        assert_eq!(
            DiscoveryAnnouncement::new(NodeId::new(), 1, 0),
            Err(DiscoveryError::InvalidInstanceNonce)
        );
    }

    #[test]
    fn decoded_zero_fields_are_rejected() {
        let encoded = announcement().encode();
        let mut invalid = encoded;
        invalid[6..8].fill(0);
        assert_eq!(
            DiscoveryAnnouncement::decode(&invalid),
            Err(DiscoveryError::InvalidControlPort)
        );
        invalid = encoded;
        invalid[8..24].fill(0);
        assert_eq!(
            DiscoveryAnnouncement::decode(&invalid),
            Err(DiscoveryError::NilNodeId)
        );
        invalid = encoded;
        invalid[24..32].fill(0);
        assert_eq!(
            DiscoveryAnnouncement::decode(&invalid),
            Err(DiscoveryError::InvalidInstanceNonce)
        );
    }

    #[test]
    fn candidate_set_uses_packet_source_and_tracks_refresh_or_restart() {
        let local = NodeId::new();
        let remote = NodeId::new();
        let mut set = DiscoveryCandidateSet::new(local).expect("candidate set");
        let first = DiscoveryAnnouncement::new(remote, 42420, 1).expect("announcement");
        assert_eq!(
            set.observe("192.168.1.4".parse().expect("IP"), first, 100),
            Ok(ObserveOutcome::Added)
        );
        assert_eq!(set.candidates().len(), 1);
        assert_eq!(
            set.candidates()
                .next()
                .expect("candidate")
                .endpoint
                .to_string(),
            "192.168.1.4:42420"
        );
        assert_eq!(
            set.observe("100.64.0.9".parse().expect("IP"), first, 101),
            Ok(ObserveOutcome::Refreshed)
        );
        let restarted = DiscoveryAnnouncement::new(remote, 42421, 2).expect("announcement");
        assert_eq!(
            set.observe("100.64.0.9".parse().expect("IP"), restarted, 102),
            Ok(ObserveOutcome::Restarted)
        );
        assert_eq!(
            set.candidates().next().expect("candidate").endpoint.port(),
            42421
        );
    }

    #[test]
    fn candidate_set_rejects_self_invalid_sources_and_stale_updates() {
        let local = NodeId::new();
        let remote = NodeId::new();
        let mut set = DiscoveryCandidateSet::new(local).expect("candidate set");
        let self_record = DiscoveryAnnouncement::new(local, 1, 1).expect("announcement");
        assert_eq!(
            set.observe("192.168.1.2".parse().expect("IP"), self_record, 1),
            Err(DiscoveryError::SelfAnnouncement)
        );
        let remote_record = DiscoveryAnnouncement::new(remote, 1, 1).expect("announcement");
        for source in ["0.0.0.0", "127.0.0.1", "224.0.0.1", "::", "::1", "ff02::1"] {
            assert_eq!(
                set.observe(source.parse().expect("IP"), remote_record, 1),
                Err(DiscoveryError::InvalidSourceAddress)
            );
        }
        assert_eq!(
            set.observe("10.0.0.2".parse().expect("IP"), remote_record, 10),
            Ok(ObserveOutcome::Added)
        );
        let stale = DiscoveryAnnouncement::new(remote, 2, 2).expect("announcement");
        assert_eq!(
            set.observe("10.0.0.3".parse().expect("IP"), stale, 9),
            Ok(ObserveOutcome::IgnoredStale)
        );
        assert_eq!(
            set.candidates()
                .next()
                .expect("candidate")
                .endpoint
                .to_string(),
            "10.0.0.2:1"
        );
    }

    #[test]
    fn candidate_set_is_bounded_and_explicitly_expires() {
        let mut set = DiscoveryCandidateSet::new(NodeId::new()).expect("candidate set");
        for index in 0..MAX_DISCOVERY_CANDIDATES {
            let record = DiscoveryAnnouncement::new(NodeId::new(), 1, index as u64 + 1)
                .expect("announcement");
            assert_eq!(
                set.observe("10.0.0.2".parse().expect("IP"), record, 100),
                Ok(ObserveOutcome::Added)
            );
        }
        let overflow = DiscoveryAnnouncement::new(NodeId::new(), 1, 99).expect("announcement");
        assert_eq!(
            set.observe("10.0.0.2".parse().expect("IP"), overflow, 100),
            Err(DiscoveryError::CandidateCapacityReached)
        );
        assert_eq!(set.expire(100 + DISCOVERY_CANDIDATE_TTL_MS), 0);
        assert_eq!(
            set.expire(101 + DISCOVERY_CANDIDATE_TTL_MS),
            MAX_DISCOVERY_CANDIDATES
        );
        assert_eq!(set.candidates().len(), 0);
    }

    #[test]
    fn ingress_decodes_and_rate_limits_all_datagrams_before_candidate_admission() {
        let local = NodeId::new();
        let remote = NodeId::new();
        let mut ingress = DiscoveryIngress::new(local).expect("ingress");
        let valid = DiscoveryAnnouncement::new(remote, 42_420, 1)
            .expect("announcement")
            .encode();
        assert_eq!(
            ingress.ingest("192.168.1.4".parse().expect("IP"), &valid, 100),
            Ok(ObserveOutcome::Added)
        );
        for _ in 1..MAX_DISCOVERY_DATAGRAMS_PER_WINDOW {
            assert!(matches!(
                ingress.ingest("192.168.1.4".parse().expect("IP"), b"bad", 100),
                Err(DiscoveryIngressError::Candidate(
                    DiscoveryError::InvalidLength(3)
                ))
            ));
        }
        assert_eq!(
            ingress.ingest("192.168.1.4".parse().expect("IP"), &valid, 100),
            Err(DiscoveryIngressError::RateLimitExceeded)
        );
        assert_eq!(
            ingress.ingest("192.168.1.4".parse().expect("IP"), &valid, 99),
            Err(DiscoveryIngressError::NonMonotonicTime)
        );
        assert_eq!(
            ingress.ingest(
                "192.168.1.4".parse().expect("IP"),
                &valid,
                100 + DISCOVERY_RATE_WINDOW_MS
            ),
            Ok(ObserveOutcome::Refreshed)
        );
        assert_eq!(ingress.candidates().len(), 1);
    }
}
