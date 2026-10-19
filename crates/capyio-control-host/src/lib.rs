#![forbid(unsafe_code)]

//! Transport-agnostic host admission for already-authenticated CapyIO control
//! messages. This crate opens no socket and owns no cryptographic identity.

use std::collections::{BTreeSet, VecDeque};

use capyio_core::{
    Availability, CapabilityId, CoreError, MessageId, NodeDescriptor, NodeId, PortDirection,
    SessionId, SessionState,
};
use capyio_protocol::{
    PortLeaseAction, PortLeaseOutcome, ProtocolError, decode_catalog_snapshot, decode_envelope,
    decode_port_lease_request, encode_catalog_snapshot, encode_port_lease_response, v1,
};
use capyio_runtime::{LeaseClaimOutcome, NodeRuntime, PortLeaseError, RuntimeError};
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

/// Applies decoded peer control messages to Runtime while retaining a bounded
/// set of recent IDs for accidental duplicate suppression. A production
/// transport still requires cryptographic replay protection outside this object.
#[derive(Clone, Debug)]
pub struct PeerCatalogControlHost {
    binding: AdmittedPeerBinding,
    recent_message_ids: VecDeque<MessageId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ControlMessageOutcome {
    CatalogAdmitted {
        message_id: MessageId,
    },
    Response {
        request_message_id: MessageId,
        bytes: Vec<u8>,
    },
}

impl PeerCatalogControlHost {
    #[must_use]
    pub const fn new(binding: AdmittedPeerBinding) -> Self {
        Self {
            binding,
            recent_message_ids: VecDeque::new(),
        }
    }

    /// Dispatches one complete control envelope after transport framing and
    /// authentication. Continuous data and unsupported control payloads are
    /// never accepted through this boundary.
    pub fn handle_control_message(
        &mut self,
        runtime: &mut NodeRuntime,
        bytes: &[u8],
        now_ms: u64,
    ) -> Result<ControlMessageOutcome, ControlHostError> {
        let envelope = decode_envelope(bytes)?;
        match envelope.payload {
            Some(v1::envelope::Payload::CatalogSnapshot(_)) => {
                let message_id = self.admit_catalog(runtime, bytes)?;
                Ok(ControlMessageOutcome::CatalogAdmitted { message_id })
            }
            Some(v1::envelope::Payload::PortLeaseRequest(_)) => {
                let request = decode_port_lease_request(bytes)?;
                let response = self.handle_port_lease_request(runtime, bytes, now_ms)?;
                Ok(ControlMessageOutcome::Response {
                    request_message_id: request.message_id,
                    bytes: response,
                })
            }
            _ => Err(ControlHostError::UnsupportedInboundPayload),
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
        self.remember_message(catalog.message_id);
        Ok(catalog.message_id)
    }

    /// Handles one owner-authoritative Port lease request. The caller must
    /// supply monotonically non-decreasing host time and an authenticated
    /// connection represented by this host's fixed binding.
    pub fn handle_port_lease_request(
        &mut self,
        runtime: &mut NodeRuntime,
        bytes: &[u8],
        now_ms: u64,
    ) -> Result<Vec<u8>, ControlHostError> {
        let request = decode_port_lease_request(bytes)?;
        if request.session_id != self.binding.session_id {
            return Err(ControlHostError::SessionMismatch {
                expected: self.binding.session_id,
                actual: request.session_id,
            });
        }
        if self.recent_message_ids.contains(&request.message_id) {
            return Err(ControlHostError::DuplicateMessage(request.message_id));
        }

        let session = runtime.session(self.binding.session_id)?;
        if session.remote_node_id != self.binding.peer_node_id {
            return Err(ControlHostError::PeerMismatch {
                expected: self.binding.peer_node_id,
                actual: session.remote_node_id,
            });
        }
        if session.state != SessionState::Ready {
            return Err(ControlHostError::SessionNotReady(self.binding.session_id));
        }
        let route = runtime.route(request.route_id)?;
        if route.session_id != self.binding.session_id {
            return Err(ControlHostError::RouteSessionMismatch {
                route_id: request.route_id,
                expected: self.binding.session_id,
                actual: route.session_id,
            });
        }

        let expires_at_ms = match request.action {
            PortLeaseAction::Acquire | PortLeaseAction::Renew => {
                now_ms.checked_add(request.requested_ttl_ms)
            }
            PortLeaseAction::Release => None,
        };
        let (outcome, response_expiry) = if request.port.node_id != session.local_node_id
            || expires_at_ms.is_none() && !matches!(request.action, PortLeaseAction::Release)
        {
            (PortLeaseOutcome::Denied, None)
        } else {
            match request.action {
                PortLeaseAction::Acquire => {
                    let expiry = expires_at_ms.expect("checked non-release expiry");
                    match runtime.acquire_route_port_lease(
                        request.route_id,
                        request.port,
                        expiry,
                        now_ms,
                    ) {
                        Ok(LeaseClaimOutcome::Acquired) => {
                            (PortLeaseOutcome::Acquired, Some(expiry))
                        }
                        Ok(LeaseClaimOutcome::Refreshed) => {
                            (PortLeaseOutcome::Renewed, Some(expiry))
                        }
                        Err(RuntimeError::Lease(PortLeaseError::Conflict(_))) => {
                            (PortLeaseOutcome::Conflict, None)
                        }
                        Err(_) => (PortLeaseOutcome::Denied, None),
                    }
                }
                PortLeaseAction::Renew => {
                    let expiry = expires_at_ms.expect("checked non-release expiry");
                    let has_current_claim = runtime
                        .route_port_lease_claim(request.route_id, request.port)
                        .is_some_and(|claim| {
                            claim.session_id == self.binding.session_id
                                && claim.expires_at_ms > now_ms
                        });
                    if !has_current_claim {
                        (PortLeaseOutcome::Denied, None)
                    } else {
                        match runtime.acquire_route_port_lease(
                            request.route_id,
                            request.port,
                            expiry,
                            now_ms,
                        ) {
                            Ok(LeaseClaimOutcome::Refreshed) => {
                                (PortLeaseOutcome::Renewed, Some(expiry))
                            }
                            Ok(LeaseClaimOutcome::Acquired) | Err(_) => {
                                (PortLeaseOutcome::Denied, None)
                            }
                        }
                    }
                }
                PortLeaseAction::Release => {
                    match runtime.release_route_port_lease(
                        request.route_id,
                        request.port,
                        self.binding.session_id,
                    ) {
                        Ok(true) => (PortLeaseOutcome::Released, None),
                        Ok(false) | Err(_) => (PortLeaseOutcome::Denied, None),
                    }
                }
            }
        };
        let response = encode_port_lease_response(
            self.binding.session_id,
            request.message_id,
            request.route_id,
            request.port,
            outcome,
            response_expiry,
        )?;
        self.remember_message(request.message_id);
        Ok(response)
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

    fn remember_message(&mut self, message_id: MessageId) {
        self.recent_message_ids.push_back(message_id);
        while self.recent_message_ids.len() > MAX_RECENT_CONTROL_MESSAGE_IDS {
            self.recent_message_ids.pop_front();
        }
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
    #[error("control Session {0} is not ready")]
    SessionNotReady(SessionId),
    #[error("Route {route_id} belongs to Session {actual}, not admitted Session {expected}")]
    RouteSessionMismatch {
        route_id: capyio_core::RouteId,
        expected: SessionId,
        actual: SessionId,
    },
    #[error("control payload is not accepted by the admitted peer host")]
    UnsupportedInboundPayload,
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
    use capyio_protocol::{
        MIN_PORT_LEASE_TTL_MS, decode_catalog_snapshot, decode_port_lease_response,
        encode_envelope, encode_port_lease_request, new_envelope, v1,
    };
    use capyio_runtime::PortLeasePolicy;
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

    #[test]
    fn admitted_peer_can_acquire_renew_and_release_only_its_bound_route_lease() {
        let mut lab = DemoLab::new().expect("demo lab");
        let route_id = lab.routes.phone_microphone_to_windows;
        let local_port = lab.runtime.route(route_id).expect("Route").sink;
        lab.runtime
            .set_local_port_lease_policy(local_port, PortLeasePolicy::Exclusive)
            .expect("policy");
        lab.runtime
            .authorize_route(route_id, None)
            .expect("authorize Route");
        let mut host = host_for(&lab);

        let acquire = encode_port_lease_request(
            lab.session_id,
            route_id,
            local_port,
            PortLeaseAction::Acquire,
            MIN_PORT_LEASE_TTL_MS,
        )
        .expect("acquire request");
        let acquired = decode_port_lease_response(
            &host
                .handle_port_lease_request(&mut lab.runtime, &acquire, 10)
                .expect("acquire response"),
        )
        .expect("decode acquired");
        assert_eq!(acquired.outcome, PortLeaseOutcome::Acquired);
        assert_eq!(acquired.expires_at_ms, Some(10 + MIN_PORT_LEASE_TTL_MS));

        let renew = encode_port_lease_request(
            lab.session_id,
            route_id,
            local_port,
            PortLeaseAction::Renew,
            MIN_PORT_LEASE_TTL_MS,
        )
        .expect("renew request");
        let renewed = decode_port_lease_response(
            &host
                .handle_port_lease_request(&mut lab.runtime, &renew, 20)
                .expect("renew response"),
        )
        .expect("decode renewed");
        assert_eq!(renewed.outcome, PortLeaseOutcome::Renewed);
        assert_eq!(renewed.expires_at_ms, Some(20 + MIN_PORT_LEASE_TTL_MS));

        let release = encode_port_lease_request(
            lab.session_id,
            route_id,
            local_port,
            PortLeaseAction::Release,
            0,
        )
        .expect("release request");
        let released = decode_port_lease_response(
            &host
                .handle_port_lease_request(&mut lab.runtime, &release, 30)
                .expect("release response"),
        )
        .expect("decode released");
        assert_eq!(released.outcome, PortLeaseOutcome::Released);
        assert!(
            lab.runtime
                .route_port_lease_claim(route_id, local_port)
                .is_none()
        );
        assert_eq!(host.retained_message_count(), 3);
    }

    #[test]
    fn renewal_rejects_expired_claim_before_and_after_expiry_sweep() {
        for swept in [false, true] {
            for after_expiry_ms in [0, 1] {
                let mut lab = DemoLab::new().expect("demo lab");
                let route_id = lab.routes.phone_microphone_to_windows;
                let port = lab.runtime.route(route_id).expect("Route").sink;
                lab.runtime
                    .set_local_port_lease_policy(port, PortLeasePolicy::Exclusive)
                    .expect("policy");
                lab.runtime
                    .authorize_route(route_id, Some(10_000))
                    .expect("finite authorization");
                let expiry = 10 + MIN_PORT_LEASE_TTL_MS;
                lab.runtime
                    .acquire_route_port_lease(route_id, port, expiry, 10)
                    .expect("initial claim");
                lab.runtime
                    .set_route_active(route_id, true, 10)
                    .expect("active Route");
                let now = expiry + after_expiry_ms;
                if swept {
                    assert_eq!(lab.runtime.expire_port_leases(now), 1);
                }
                let before = lab.runtime.snapshot();
                let mut host = host_for(&lab);
                let request = encode_port_lease_request(
                    lab.session_id,
                    route_id,
                    port,
                    PortLeaseAction::Renew,
                    MIN_PORT_LEASE_TTL_MS,
                )
                .expect("renew request");
                let response = decode_port_lease_response(
                    &host
                        .handle_port_lease_request(&mut lab.runtime, &request, now)
                        .expect("bounded renewal denial"),
                )
                .expect("decode denial");
                assert_eq!(response.outcome, PortLeaseOutcome::Denied);
                assert_eq!(response.expires_at_ms, None);
                // Without a sweep the old, expired claim can still be present;
                // Renew must neither replace it nor resume an Offline Route.
                assert_eq!(lab.runtime.snapshot(), before);
            }
        }
    }

    #[test]
    fn renewal_rejects_a_released_claim_without_reacquiring_it() {
        let mut lab = DemoLab::new().expect("demo lab");
        let route_id = lab.routes.phone_microphone_to_windows;
        let port = lab.runtime.route(route_id).expect("Route").sink;
        lab.runtime
            .set_local_port_lease_policy(port, PortLeasePolicy::Exclusive)
            .expect("policy");
        lab.runtime
            .authorize_route(route_id, Some(10_000))
            .expect("finite authorization");
        lab.runtime
            .acquire_route_port_lease(route_id, port, 2_000, 10)
            .expect("initial claim");
        let mut host = host_for(&lab);
        let release =
            encode_port_lease_request(lab.session_id, route_id, port, PortLeaseAction::Release, 0)
                .expect("release request");
        let released = decode_port_lease_response(
            &host
                .handle_port_lease_request(&mut lab.runtime, &release, 20)
                .expect("release response"),
        )
        .expect("decode release");
        assert_eq!(released.outcome, PortLeaseOutcome::Released);
        assert!(lab.runtime.route_port_lease_claim(route_id, port).is_none());
        let before = lab.runtime.snapshot();
        let renew = encode_port_lease_request(
            lab.session_id,
            route_id,
            port,
            PortLeaseAction::Renew,
            MIN_PORT_LEASE_TTL_MS,
        )
        .expect("renew request");
        let denied = decode_port_lease_response(
            &host
                .handle_port_lease_request(&mut lab.runtime, &renew, 21)
                .expect("bounded renewal denial"),
        )
        .expect("decode denial");
        assert_eq!(denied.outcome, PortLeaseOutcome::Denied);
        assert_eq!(denied.expires_at_ms, None);
        assert_eq!(lab.runtime.snapshot(), before);
    }

    #[test]
    fn renewal_rejects_a_stopped_route_without_resuming_it() {
        let mut lab = DemoLab::new().expect("demo lab");
        let route_id = lab.routes.phone_microphone_to_windows;
        let port = lab.runtime.route(route_id).expect("Route").sink;
        lab.runtime
            .set_local_port_lease_policy(port, PortLeasePolicy::Exclusive)
            .expect("policy");
        lab.runtime
            .authorize_route(route_id, Some(10_000))
            .expect("finite authorization");
        lab.runtime
            .acquire_route_port_lease(route_id, port, 2_000, 10)
            .expect("initial claim");
        lab.runtime
            .set_route_active(route_id, true, 10)
            .expect("active Route");
        lab.runtime
            .set_route_active(route_id, false, 20)
            .expect("stop releases its claim");
        assert!(lab.runtime.route_port_lease_claim(route_id, port).is_none());
        assert_eq!(
            lab.runtime.route(route_id).expect("Route").state,
            capyio_core::RouteState::Stopped
        );
        let before = lab.runtime.snapshot();
        let mut host = host_for(&lab);
        let renew = encode_port_lease_request(
            lab.session_id,
            route_id,
            port,
            PortLeaseAction::Renew,
            MIN_PORT_LEASE_TTL_MS,
        )
        .expect("renew request");
        let denied = decode_port_lease_response(
            &host
                .handle_port_lease_request(&mut lab.runtime, &renew, 21)
                .expect("bounded renewal denial"),
        )
        .expect("decode denial");
        assert_eq!(denied.outcome, PortLeaseOutcome::Denied);
        assert_eq!(denied.expires_at_ms, None);
        assert_eq!(lab.runtime.snapshot(), before);
    }

    #[test]
    fn lease_request_replay_and_non_local_port_fail_closed() {
        let mut lab = DemoLab::new().expect("demo lab");
        let route_id = lab.routes.phone_microphone_to_windows;
        let route = lab.runtime.route(route_id).expect("Route").clone();
        lab.runtime
            .authorize_route(route_id, None)
            .expect("authorize Route");
        let mut host = host_for(&lab);
        let request = encode_port_lease_request(
            lab.session_id,
            route_id,
            route.source,
            PortLeaseAction::Acquire,
            MIN_PORT_LEASE_TTL_MS,
        )
        .expect("remote Port request");

        let denied = decode_port_lease_response(
            &host
                .handle_port_lease_request(&mut lab.runtime, &request, 10)
                .expect("bounded denial"),
        )
        .expect("decode denial");
        assert_eq!(denied.outcome, PortLeaseOutcome::Denied);
        assert!(matches!(
            host.handle_port_lease_request(&mut lab.runtime, &request, 11),
            Err(ControlHostError::DuplicateMessage(_))
        ));
        assert!(lab.runtime.snapshot().port_leases.is_empty());
    }

    #[test]
    fn unified_dispatch_accepts_only_catalog_and_lease_requests() {
        let mut lab = DemoLab::new().expect("demo lab");
        let mut host = host_for(&lab);
        let catalog = catalog_bytes(lab.session_id, &android_node());
        assert!(matches!(
            host.handle_control_message(&mut lab.runtime, &catalog, 0),
            Ok(ControlMessageOutcome::CatalogAdmitted { .. })
        ));

        let route_id = lab.routes.phone_microphone_to_windows;
        let local_port = lab.runtime.route(route_id).expect("Route").sink;
        lab.runtime
            .set_local_port_lease_policy(local_port, PortLeasePolicy::Exclusive)
            .expect("policy");
        lab.runtime
            .authorize_route(route_id, None)
            .expect("authorize Route");
        let lease = encode_port_lease_request(
            lab.session_id,
            route_id,
            local_port,
            PortLeaseAction::Acquire,
            MIN_PORT_LEASE_TTL_MS,
        )
        .expect("lease request");
        let outcome = host
            .handle_control_message(&mut lab.runtime, &lease, 10)
            .expect("lease dispatch");
        let ControlMessageOutcome::Response {
            request_message_id,
            bytes,
        } = outcome
        else {
            panic!("expected response");
        };
        let response = decode_port_lease_response(&bytes).expect("lease response");
        assert_eq!(response.request_message_id, request_message_id);
        assert_eq!(response.outcome, PortLeaseOutcome::Acquired);

        let unsupported = new_envelope(
            Some(lab.session_id),
            v1::envelope::Payload::RouteCommand(v1::RouteCommand {
                route_id: route_id.to_string(),
                action: v1::RouteCommandAction::Start as i32,
            }),
        );
        assert!(matches!(
            host.handle_control_message(&mut lab.runtime, &encode_envelope(&unsupported), 20),
            Err(ControlHostError::UnsupportedInboundPayload)
        ));
    }
}
