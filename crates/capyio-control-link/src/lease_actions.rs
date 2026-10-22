use std::time::Instant;

use capyio_control_tls::{ControlTlsBinding, ControlTlsError};
use capyio_core::{MessageId, PortRef, RouteId};
use capyio_protocol::{PortLeaseAction, decode_port_lease_request, encode_port_lease_request};
use capyio_runtime::NodeRuntime;

use crate::{CatalogControlLink, CatalogLinkError, ReceivedPortLeaseRequest};

/// One immutable lease request, prepared without network I/O for one exact link.
/// This is not resource authority: the owner must validate its current Runtime
/// and policy before preparation, then release its guards before sending.
/// Sending consumes the value, including on failure; requests cannot be cloned
/// or moved to a replacement link, even if it uses the same Session binding.
///
/// ```compile_fail
/// use capyio_control_link::{CatalogControlLink, PreparedPortLeaseRequest};
/// fn replay(link: &mut CatalogControlLink, request: PreparedPortLeaseRequest) {
///     link.send_prepared_port_lease_request(request).unwrap();
///     link.send_prepared_port_lease_request(request).unwrap();
/// }
/// ```
///
/// ```compile_fail
/// use capyio_control_link::PreparedPortLeaseRequest;
/// fn duplicate(request: PreparedPortLeaseRequest) { let _ = request.clone(); }
/// ```
pub struct PreparedPortLeaseRequest {
    provenance: LeaseActionProvenance,
    message_id: MessageId,
    bytes: Vec<u8>,
}

impl PreparedPortLeaseRequest {
    fn encode(
        instance_id: MessageId,
        binding: ControlTlsBinding,
        route_id: RouteId,
        port: PortRef,
        action: PortLeaseAction,
        ttl_ms: u64,
    ) -> Result<Self, CatalogLinkError> {
        let bytes = encode_port_lease_request(binding.session_id, route_id, port, action, ttl_ms)?;
        let message_id = decode_port_lease_request(&bytes)?.message_id;
        Ok(Self {
            provenance: LeaseActionProvenance {
                instance_id,
                binding,
            },
            message_id,
            bytes,
        })
    }

    /// The exact wire request ID, available before sending for correlation.
    #[must_use]
    pub const fn message_id(&self) -> MessageId {
        self.message_id
    }
}

/// One immutable owner-authoritative response, prepared without network I/O.
/// Preparation has already applied the request to Runtime and recorded its ID
/// for duplicate suppression. Dropping this value or failing to send it does
/// not roll back that mutation. The owner must reconcile/release any resulting
/// claim on failure, not retry preparation or reacquire to replace the response.
/// This value owns no Runtime borrow and is consumed by its exact live link.
///
/// ```compile_fail
/// use capyio_control_link::{CatalogControlLink, PreparedPortLeaseResponse};
/// fn replay(link: &mut CatalogControlLink, response: PreparedPortLeaseResponse) {
///     link.send_prepared_port_lease_response(response).unwrap();
///     link.send_prepared_port_lease_response(response).unwrap();
/// }
/// ```
///
/// ```compile_fail
/// use capyio_control_link::PreparedPortLeaseResponse;
/// fn duplicate(response: PreparedPortLeaseResponse) { let _ = response.clone(); }
/// ```
pub struct PreparedPortLeaseResponse {
    provenance: LeaseActionProvenance,
    bytes: Vec<u8>,
}

struct LeaseActionProvenance {
    instance_id: MessageId,
    binding: ControlTlsBinding,
}

impl LeaseActionProvenance {
    fn validate(
        &self,
        instance_id: MessageId,
        binding: ControlTlsBinding,
    ) -> Result<(), CatalogLinkError> {
        if self.instance_id != instance_id || self.binding != binding {
            return Err(CatalogLinkError::BindingMismatch);
        }
        Ok(())
    }
}

impl CatalogControlLink {
    /// Encode one bounded request without network I/O or a retained Runtime
    /// borrow. Runtime/resource policy remains the caller's responsibility,
    /// exactly as for `send_port_lease_request`. This does not acquire a claim.
    pub fn prepare_port_lease_request(
        &self,
        route_id: RouteId,
        port: PortRef,
        action: PortLeaseAction,
        ttl_ms: u64,
    ) -> Result<PreparedPortLeaseRequest, CatalogLinkError> {
        if self.channel.is_none() {
            return Err(CatalogLinkError::Closed);
        }
        PreparedPortLeaseRequest::encode(
            self.instance_id,
            self.binding,
            route_id,
            port,
            action,
            ttl_ms,
        )
    }

    /// Send the exact prepared request once, without observing Runtime.
    pub fn send_prepared_port_lease_request(
        &mut self,
        prepared: PreparedPortLeaseRequest,
    ) -> Result<MessageId, CatalogLinkError> {
        prepared
            .provenance
            .validate(self.instance_id, self.binding)?;
        self.send_bytes(&prepared.bytes)?;
        Ok(prepared.message_id)
    }

    /// Send once under the caller's original absolute negotiation deadline.
    pub fn send_prepared_port_lease_request_until(
        &mut self,
        prepared: PreparedPortLeaseRequest,
        deadline: Instant,
    ) -> Result<MessageId, CatalogLinkError> {
        prepared
            .provenance
            .validate(self.instance_id, self.binding)?;
        self.send_bytes_until(&prepared.bytes, deadline)?;
        Ok(prepared.message_id)
    }

    /// Validate the current Runtime/Session and apply one authenticated request
    /// without network I/O. Callers must first enforce their action policy and
    /// finite deadline; this does not broaden Acquire/Renew/Release authority.
    /// Release Runtime/global guards before sending the returned response.
    /// A send failure does not undo a prepared claim: reconcile it explicitly.
    pub fn prepare_port_lease_response(
        &mut self,
        runtime: &mut NodeRuntime,
        received: ReceivedPortLeaseRequest,
        now_ms: u64,
    ) -> Result<PreparedPortLeaseResponse, CatalogLinkError> {
        let provenance = LeaseActionProvenance {
            instance_id: received.instance_id,
            binding: received.binding,
        };
        provenance.validate(self.instance_id, self.binding)?;
        self.validate_runtime(runtime)?;
        let bytes = self
            .host
            .handle_port_lease_request(runtime, &received.bytes, now_ms)?;
        Ok(PreparedPortLeaseResponse { provenance, bytes })
    }

    /// Send the exact response once after all Runtime/global guards are released.
    pub fn send_prepared_port_lease_response(
        &mut self,
        prepared: PreparedPortLeaseResponse,
    ) -> Result<(), CatalogLinkError> {
        prepared
            .provenance
            .validate(self.instance_id, self.binding)?;
        self.send_bytes(&prepared.bytes)
    }

    /// Send once with the original absolute deadline, never a renewed budget.
    pub fn send_prepared_port_lease_response_until(
        &mut self,
        prepared: PreparedPortLeaseResponse,
        deadline: Instant,
    ) -> Result<(), CatalogLinkError> {
        prepared
            .provenance
            .validate(self.instance_id, self.binding)?;
        self.send_bytes_until(&prepared.bytes, deadline)
    }

    /// Convenience wrapper. Use prepare/send separately to release host guards.
    pub fn send_port_lease_request(
        &mut self,
        route_id: RouteId,
        port: PortRef,
        action: PortLeaseAction,
        ttl_ms: u64,
    ) -> Result<MessageId, CatalogLinkError> {
        let prepared = self.prepare_port_lease_request(route_id, port, action, ttl_ms)?;
        self.send_prepared_port_lease_request(prepared)
    }

    /// Sends an exact correlated lease request under an absolute operation deadline.
    pub fn send_port_lease_request_until(
        &mut self,
        route_id: RouteId,
        port: PortRef,
        action: PortLeaseAction,
        ttl_ms: u64,
        deadline: Instant,
    ) -> Result<MessageId, CatalogLinkError> {
        let prepared = self.prepare_port_lease_request(route_id, port, action, ttl_ms)?;
        self.send_prepared_port_lease_request_until(prepared, deadline)
    }

    /// Compatibility wrapper retaining a Runtime borrow across the send.
    /// Guard-owning callers should use the separate prepare/send APIs instead.
    pub fn answer_port_lease_request(
        &mut self,
        runtime: &mut NodeRuntime,
        received: ReceivedPortLeaseRequest,
        now_ms: u64,
    ) -> Result<(), CatalogLinkError> {
        let prepared = self.prepare_port_lease_response(runtime, received, now_ms)?;
        self.send_prepared_port_lease_response(prepared)
    }

    /// Answer one authenticated lease request under the original operation deadline.
    /// An already-expired dispatch never mutates Runtime or consumes its host ID.
    /// Callers still decide whether the request action is allowed before dispatch.
    pub fn answer_port_lease_request_until(
        &mut self,
        runtime: &mut NodeRuntime,
        received: ReceivedPortLeaseRequest,
        now_ms: u64,
        deadline: Instant,
    ) -> Result<(), CatalogLinkError> {
        if received.instance_id != self.instance_id || received.binding != self.binding {
            return Err(CatalogLinkError::BindingMismatch);
        }
        if Instant::now() >= deadline {
            return Err(ControlTlsError::ChannelFailed.into());
        }
        let prepared = self.prepare_port_lease_response(runtime, received, now_ms)?;
        self.send_prepared_port_lease_response_until(prepared, deadline)
    }
}

#[cfg(test)]
mod tests {
    use capyio_control_host::{AdmittedPeerBinding, PeerCatalogControlHost};
    use capyio_core::{NodeId, SessionId};
    use capyio_protocol::{MAX_PORT_LEASE_TTL_MS, MIN_PORT_LEASE_TTL_MS};
    use capyio_runtime::PortLeasePolicy;
    use capyio_testkit::DemoLab;

    use super::*;

    #[test]
    fn prepared_lease_provenance_requires_exact_instance_and_complete_tls_binding() {
        let instance_id = MessageId::new();
        let binding =
            ControlTlsBinding::new(SessionId::new(), NodeId::new(), NodeId::new()).unwrap();
        let provenance = LeaseActionProvenance {
            instance_id,
            binding,
        };
        assert!(provenance.validate(instance_id, binding).is_ok());
        assert!(matches!(
            provenance.validate(MessageId::new(), binding),
            Err(CatalogLinkError::BindingMismatch)
        ));
        for changed in [
            ControlTlsBinding {
                session_id: SessionId::new(),
                ..binding
            },
            ControlTlsBinding {
                server_node_id: NodeId::new(),
                ..binding
            },
            ControlTlsBinding {
                client_node_id: NodeId::new(),
                ..binding
            },
        ] {
            assert!(matches!(
                provenance.validate(instance_id, changed),
                Err(CatalogLinkError::BindingMismatch)
            ));
        }
    }

    #[test]
    fn prepared_lease_request_freezes_exact_message_and_preserves_codec_bounds() {
        let lab = DemoLab::new().unwrap();
        let route = lab
            .runtime
            .route(lab.routes.phone_microphone_to_windows)
            .unwrap();
        let binding =
            ControlTlsBinding::new(lab.session_id, route.sink.node_id, route.source.node_id)
                .unwrap();
        for (action, ttl_ms) in [
            (PortLeaseAction::Acquire, MIN_PORT_LEASE_TTL_MS),
            (PortLeaseAction::Renew, MAX_PORT_LEASE_TTL_MS),
            (PortLeaseAction::Release, 0),
        ] {
            let prepared = PreparedPortLeaseRequest::encode(
                MessageId::new(),
                binding,
                route.id,
                route.source,
                action,
                ttl_ms,
            )
            .unwrap();
            let decoded = decode_port_lease_request(&prepared.bytes).unwrap();
            assert_eq!(decoded.message_id, prepared.message_id());
            assert_eq!(decoded.session_id, binding.session_id);
            assert_eq!(decoded.route_id, route.id);
            assert_eq!(decoded.port, route.source);
            assert_eq!(decoded.action, action);
            assert_eq!(decoded.requested_ttl_ms, ttl_ms);
        }
        for (action, ttl_ms) in [
            (PortLeaseAction::Acquire, 0),
            (PortLeaseAction::Renew, MIN_PORT_LEASE_TTL_MS - 1),
            (PortLeaseAction::Acquire, MAX_PORT_LEASE_TTL_MS + 1),
            (PortLeaseAction::Release, MIN_PORT_LEASE_TTL_MS),
        ] {
            assert!(
                PreparedPortLeaseRequest::encode(
                    MessageId::new(),
                    binding,
                    route.id,
                    route.source,
                    action,
                    ttl_ms,
                )
                .is_err()
            );
        }
    }

    #[test]
    fn prepared_lease_closed_link_rejects_before_owner_mutation_or_send() {
        let mut lab = DemoLab::new().unwrap();
        let route_id = lab.routes.phone_microphone_to_windows;
        let route = lab.runtime.route(route_id).unwrap().clone();
        lab.runtime.authorize_route(route_id, Some(5000)).unwrap();
        lab.runtime
            .set_local_port_lease_policy(route.sink, PortLeasePolicy::Exclusive)
            .unwrap();
        let binding =
            ControlTlsBinding::new(lab.session_id, route.sink.node_id, route.source.node_id)
                .unwrap();
        let instance_id = MessageId::new();
        let mut closed = CatalogControlLink {
            instance_id,
            channel: None,
            binding,
            local_node_id: route.sink.node_id,
            peer_node_id: route.source.node_id,
            host: PeerCatalogControlHost::new(AdmittedPeerBinding::new(
                lab.session_id,
                route.source.node_id,
            )),
            received_secure_audio_offer: None,
            secure_audio_offer_seen: false,
        };
        assert!(matches!(
            closed.authenticated_local_addr(),
            Err(CatalogLinkError::Closed)
        ));
        let request = PreparedPortLeaseRequest::encode(
            instance_id,
            binding,
            route_id,
            route.sink,
            PortLeaseAction::Acquire,
            MIN_PORT_LEASE_TTL_MS,
        )
        .unwrap();
        let received = ReceivedPortLeaseRequest {
            instance_id,
            binding,
            request: decode_port_lease_request(&request.bytes).unwrap(),
            bytes: request.bytes.clone(),
        };
        assert!(matches!(
            closed.prepare_port_lease_response(&mut lab.runtime, received, 1000),
            Err(CatalogLinkError::Closed)
        ));
        assert!(
            lab.runtime
                .route_port_lease_claim(route_id, route.sink)
                .is_none()
        );
        assert_eq!(closed.host.retained_message_count(), 0);
        assert!(matches!(
            closed.prepare_port_lease_request(
                route_id,
                route.source,
                PortLeaseAction::Acquire,
                MIN_PORT_LEASE_TTL_MS,
            ),
            Err(CatalogLinkError::Closed)
        ));
        assert!(matches!(
            closed.send_prepared_port_lease_request(request),
            Err(CatalogLinkError::Closed)
        ));
    }
}
