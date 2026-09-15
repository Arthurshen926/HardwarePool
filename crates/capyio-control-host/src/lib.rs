#![forbid(unsafe_code)]

//! Transport-agnostic host admission for already-authenticated CapyIO control
//! messages. This crate opens no socket and owns no cryptographic identity.

use std::collections::VecDeque;

use capyio_core::{MessageId, NodeId, SessionId};
use capyio_protocol::{ProtocolError, decode_catalog_snapshot};
use capyio_runtime::{NodeRuntime, RuntimeError};
use thiserror::Error;

/// Fixed duplicate-retention bound for one admitted control connection.
pub const MAX_RECENT_CONTROL_MESSAGE_IDS: usize = 256;

/// Binding supplied by a host only after it has authenticated the connection.
/// Constructing this value is an assertion, not an authentication operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdmittedPeerBinding {
    pub session_id: SessionId,
    pub peer_node_id: NodeId,
}

impl AdmittedPeerBinding {
    #[must_use]
    pub const fn new(session_id: SessionId, peer_node_id: NodeId) -> Self {
        Self {
            session_id,
            peer_node_id,
        }
    }
}

/// Applies decoded catalog messages to Runtime while retaining a bounded set
/// of recent IDs for accidental duplicate suppression. A production transport
/// still requires cryptographic replay protection outside this object.
#[derive(Clone, Debug)]
pub struct PeerCatalogControlHost {
    binding: AdmittedPeerBinding,
    recent_message_ids: VecDeque<MessageId>,
}

impl PeerCatalogControlHost {
    #[must_use]
    pub const fn new(binding: AdmittedPeerBinding) -> Self {
        Self {
            binding,
            recent_message_ids: VecDeque::new(),
        }
    }

    pub fn admit_catalog(
        &mut self,
        runtime: &mut NodeRuntime,
        bytes: &[u8],
    ) -> Result<MessageId, ControlHostError> {
        let catalog = decode_catalog_snapshot(bytes)?;
        if catalog.session_id != self.binding.session_id {
            return Err(ControlHostError::SessionMismatch {
                expected: self.binding.session_id,
                actual: catalog.session_id,
            });
        }
        if catalog.node.id != self.binding.peer_node_id {
            return Err(ControlHostError::PeerMismatch {
                expected: self.binding.peer_node_id,
                actual: catalog.node.id,
            });
        }
        if self.recent_message_ids.contains(&catalog.message_id) {
            return Err(ControlHostError::DuplicateMessage(catalog.message_id));
        }

        runtime.accept_peer_catalog(catalog.session_id, catalog.node)?;
        self.recent_message_ids.push_back(catalog.message_id);
        while self.recent_message_ids.len() > MAX_RECENT_CONTROL_MESSAGE_IDS {
            self.recent_message_ids.pop_front();
        }
        Ok(catalog.message_id)
    }

    #[must_use]
    pub fn retained_message_count(&self) -> usize {
        self.recent_message_ids.len()
    }
}

#[derive(Debug, Error)]
pub enum ControlHostError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error("control Session mismatch: expected {expected}, got {actual}")]
    SessionMismatch {
        expected: SessionId,
        actual: SessionId,
    },
    #[error("control peer mismatch: expected {expected}, got {actual}")]
    PeerMismatch { expected: NodeId, actual: NodeId },
    #[error("recent control message {0} is a duplicate")]
    DuplicateMessage(MessageId),
}

#[cfg(test)]
mod tests {
    use capyio_protocol::{encode_envelope, new_envelope, v1};
    use capyio_testkit::{ANDROID_NODE_ID, DemoLab, android_node, windows_node};

    use super::*;

    #[test]
    fn decoded_catalog_crosses_the_host_boundary_into_runtime() {
        let mut lab = DemoLab::new().expect("demo lab");
        let mut node = android_node();
        node.display_name = "Admitted Android peer".to_owned();
        let bytes = catalog_bytes(lab.session_id, &node);
        let mut host = host_for(&lab);

        let message_id = host
            .admit_catalog(&mut lab.runtime, &bytes)
            .expect("admit catalog");

        assert_eq!(host.retained_message_count(), 1);
        assert_eq!(
            lab.runtime.snapshot().peers[0].display_name,
            node.display_name
        );
        assert!(!message_id.to_string().is_empty());
    }

    #[test]
    fn duplicate_message_is_rejected_without_runtime_mutation() {
        let mut lab = DemoLab::new().expect("demo lab");
        let bytes = catalog_bytes(lab.session_id, &android_node());
        let mut host = host_for(&lab);
        host.admit_catalog(&mut lab.runtime, &bytes)
            .expect("first admission");
        let before = lab.runtime.snapshot();

        assert!(matches!(
            host.admit_catalog(&mut lab.runtime, &bytes),
            Err(ControlHostError::DuplicateMessage(_))
        ));
        assert_eq!(lab.runtime.snapshot(), before);
        assert_eq!(host.retained_message_count(), 1);
    }

    #[test]
    fn wrong_session_and_peer_are_rejected_before_runtime() {
        let mut lab = DemoLab::new().expect("demo lab");
        let before = lab.runtime.snapshot();
        let mut host = host_for(&lab);
        let wrong_session = SessionId::new();
        assert!(matches!(
            host.admit_catalog(
                &mut lab.runtime,
                &catalog_bytes(wrong_session, &android_node())
            ),
            Err(ControlHostError::SessionMismatch { .. })
        ));
        assert!(matches!(
            host.admit_catalog(
                &mut lab.runtime,
                &catalog_bytes(lab.session_id, &windows_node())
            ),
            Err(ControlHostError::PeerMismatch { .. })
        ));
        assert_eq!(lab.runtime.snapshot(), before);
        assert_eq!(host.retained_message_count(), 0);
    }

    #[test]
    fn runtime_rejection_does_not_consume_message_id() {
        let mut lab = DemoLab::new().expect("demo lab");
        let bytes = catalog_bytes(lab.session_id, &android_node());
        let mut host = host_for(&lab);
        lab.runtime
            .set_peer_online(ANDROID_NODE_ID.parse().expect("Android Node ID"), false)
            .expect("peer offline");
        assert!(matches!(
            host.admit_catalog(&mut lab.runtime, &bytes),
            Err(ControlHostError::Runtime(_))
        ));
        assert_eq!(host.retained_message_count(), 0);

        lab.runtime
            .set_peer_online(ANDROID_NODE_ID.parse().expect("Android Node ID"), true)
            .expect("peer online");
        host.admit_catalog(&mut lab.runtime, &bytes)
            .expect("retry same message after Runtime recovery");
        assert_eq!(host.retained_message_count(), 1);
    }

    #[test]
    fn recent_message_retention_is_bounded() {
        let mut lab = DemoLab::new().expect("demo lab");
        let mut host = host_for(&lab);
        for index in 0..=MAX_RECENT_CONTROL_MESSAGE_IDS {
            let mut node = android_node();
            node.display_name = format!("Android catalog {index}");
            host.admit_catalog(&mut lab.runtime, &catalog_bytes(lab.session_id, &node))
                .expect("bounded catalog admission");
        }
        assert_eq!(
            host.retained_message_count(),
            MAX_RECENT_CONTROL_MESSAGE_IDS
        );
    }

    fn host_for(lab: &DemoLab) -> PeerCatalogControlHost {
        PeerCatalogControlHost::new(AdmittedPeerBinding::new(
            lab.session_id,
            ANDROID_NODE_ID.parse().expect("Android Node ID"),
        ))
    }

    fn catalog_bytes(session_id: SessionId, node: &capyio_core::NodeDescriptor) -> Vec<u8> {
        let envelope = new_envelope(
            Some(session_id),
            v1::envelope::Payload::CatalogSnapshot(v1::CatalogSnapshot {
                node: Some(v1::NodeDescriptor::try_from(node).expect("wire catalog")),
            }),
        );
        encode_envelope(&envelope)
    }

    #[test]
    fn malformed_protocol_is_rejected_without_mutation() {
        let mut lab = DemoLab::new().expect("demo lab");
        let before = lab.runtime.snapshot();
        let mut host = host_for(&lab);

        assert!(matches!(
            host.admit_catalog(&mut lab.runtime, b"not a protobuf envelope"),
            Err(ControlHostError::Protocol(_))
        ));
        assert_eq!(lab.runtime.snapshot(), before);
        assert_eq!(host.retained_message_count(), 0);
    }
}
