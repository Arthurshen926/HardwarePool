#![forbid(unsafe_code)]

//! Transport-agnostic host admission for already-authenticated CapyIO control
//! messages. This crate opens no socket and owns no cryptographic identity.

use std::collections::{BTreeSet, VecDeque};

use capyio_core::{
    Availability, CapabilityId, CoreError, MessageId, NodeDescriptor, NodeId, PortDirection,
    SessionId,
};
use capyio_protocol::{ProtocolError, decode_catalog_snapshot, encode_catalog_snapshot};
use capyio_runtime::{NodeRuntime, RuntimeError};
use thiserror::Error;

/// Fixed duplicate-retention bound for one admitted control connection.
pub const MAX_RECENT_CONTROL_MESSAGE_IDS: usize = 256;

/// Produces the catalog visible to a peer from validated, host-owned
/// publication intent. Sink/Control-only Capabilities remain visible because
/// they are not controlled by the Source-sharing checkbox. Any Capability
/// containing a Source Port is published only when its Capability ID is
/// selected; mixed-direction Capabilities therefore fail closed as one unit.
pub fn project_publication_catalog(
    node: &NodeDescriptor,
    selected_source_capabilities: &BTreeSet<CapabilityId>,
) -> Result<NodeDescriptor, PublicationError> {
    node.validate()?;
    for capability_id in selected_source_capabilities {
        let capability = node
            .capabilities
            .get(capability_id)
            .ok_or(PublicationError::UnknownCapability(*capability_id))?;
        if capability.availability == Availability::Offline {
            return Err(PublicationError::OfflineCapability(*capability_id));
        }
        if !capability
            .ports
            .values()
            .any(|port| port.direction == PortDirection::Source)
        {
            return Err(PublicationError::NotSourceCapability(*capability_id));
        }
    }

    let mut published = node.clone();
    published.capabilities.retain(|capability_id, capability| {
        let has_source = capability
            .ports
            .values()
            .any(|port| port.direction == PortDirection::Source);
        !has_source || selected_source_capabilities.contains(capability_id)
    });
    for adapter in published.adapter_instances.values_mut() {
        adapter
            .owned_capabilities
            .retain(|capability_id| published.capabilities.contains_key(capability_id));
    }
    published.validate()?;
    Ok(published)
}

/// Encodes one publication-filtered local catalog for an already-authorized
/// Session. This function performs no transport I/O or authentication.
pub fn encode_publication_catalog(
    session_id: SessionId,
    node: &NodeDescriptor,
    selected_source_capabilities: &BTreeSet<CapabilityId>,
) -> Result<Vec<u8>, PublicationError> {
    let published = project_publication_catalog(node, selected_source_capabilities)?;
    Ok(encode_catalog_snapshot(session_id, &published)?)
}

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

    /// Projects and encodes the local catalog on this host's already-admitted
    /// Session. Keeping the Session ID inside the binding prevents a platform
    /// caller from publishing on a different Session than the inbound peer.
    pub fn encode_local_catalog(
        &self,
        local_node: &NodeDescriptor,
        selected_source_capabilities: &BTreeSet<CapabilityId>,
    ) -> Result<Vec<u8>, PublicationError> {
        encode_publication_catalog(
            self.binding.session_id,
            local_node,
            selected_source_capabilities,
        )
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

#[derive(Debug, Error)]
pub enum PublicationError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("publication Capability {0} is not in the local catalog")]
    UnknownCapability(CapabilityId),
    #[error("publication Capability {0} has no Source Port")]
    NotSourceCapability(CapabilityId),
    #[error("offline publication Capability {0} cannot be shared")]
    OfflineCapability(CapabilityId),
}

#[cfg(test)]
mod tests {
    use capyio_protocol::{decode_catalog_snapshot, encode_envelope, new_envelope, v1};
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
    fn publication_projection_filters_unselected_sources_and_preserves_sinks() {
        let node = android_node();
        let selected_source = node
            .capabilities
            .values()
            .find(|capability| {
                capability
                    .ports
                    .values()
                    .any(|port| port.direction == PortDirection::Source)
            })
            .expect("Source Capability")
            .id;
        let sink_only_ids: BTreeSet<_> = node
            .capabilities
            .values()
            .filter(|capability| {
                capability
                    .ports
                    .values()
                    .all(|port| port.direction != PortDirection::Source)
            })
            .map(|capability| capability.id)
            .collect();
        let selected = BTreeSet::from([selected_source]);

        let published = project_publication_catalog(&node, &selected).expect("project catalog");

        assert!(published.capabilities.contains_key(&selected_source));
        assert!(
            sink_only_ids
                .iter()
                .all(|id| published.capabilities.contains_key(id))
        );
        assert!(published.capabilities.values().all(|capability| {
            let has_source = capability
                .ports
                .values()
                .any(|port| port.direction == PortDirection::Source);
            !has_source || capability.id == selected_source
        }));
        published
            .validate()
            .expect("published catalog remains valid");
    }

    #[test]
    fn encoded_publication_catalog_round_trips_with_session_binding() {
        let lab = DemoLab::new().expect("demo lab");
        let node = windows_node();
        let selected_source = node
            .capabilities
            .values()
            .find(|capability| {
                capability
                    .ports
                    .values()
                    .any(|port| port.direction == PortDirection::Source)
            })
            .expect("Source Capability")
            .id;
        let selected = BTreeSet::from([selected_source]);
        let host = host_for(&lab);

        let bytes = host
            .encode_local_catalog(&node, &selected)
            .expect("encode publication catalog");
        let decoded = decode_catalog_snapshot(&bytes).expect("decode publication catalog");

        assert_eq!(decoded.session_id, lab.session_id);
        assert_eq!(
            decoded.node,
            project_publication_catalog(&node, &selected).expect("project expected catalog")
        );
    }

    #[test]
    fn publication_selection_rejects_unknown_sink_and_offline_source() {
        let mut node = android_node();
        let source = node
            .capabilities
            .values()
            .find(|capability| {
                capability
                    .ports
                    .values()
                    .any(|port| port.direction == PortDirection::Source)
            })
            .expect("Source Capability")
            .id;
        let sink = node
            .capabilities
            .values()
            .find(|capability| {
                capability
                    .ports
                    .values()
                    .all(|port| port.direction != PortDirection::Source)
            })
            .expect("Sink-only Capability")
            .id;

        assert!(matches!(
            project_publication_catalog(&node, &BTreeSet::from([CapabilityId::new()])),
            Err(PublicationError::UnknownCapability(_))
        ));
        assert!(matches!(
            project_publication_catalog(&node, &BTreeSet::from([sink])),
            Err(PublicationError::NotSourceCapability(id)) if id == sink
        ));
        node.capabilities
            .get_mut(&source)
            .expect("Source Capability")
            .availability = Availability::Offline;
        assert!(matches!(
            project_publication_catalog(&node, &BTreeSet::from([source])),
            Err(PublicationError::OfflineCapability(id)) if id == source
        ));
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
