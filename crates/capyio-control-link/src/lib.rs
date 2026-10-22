#![forbid(unsafe_code)]

//! Platform composition for already-authenticated, host-authorized Sessions.
//! Prepare/apply touch Runtime but never sockets. Send/receive touch sockets
//! but never Runtime, so a platform need not hold its Runtime lock during I/O.

use std::{collections::BTreeSet, time::Instant};

use capyio_control_host::{
    AdmittedPeerBinding, ControlHostError, PeerCatalogControlHost, PublicationError,
};
use capyio_control_tls::{
    AuthenticatedControlClientChannel, AuthenticatedControlServerChannel, ControlTlsBinding,
    ControlTlsError,
};
use capyio_core::{
    AuthorizationState, CapabilityId, MessageId, NodeDescriptor, NodeId, Platform, ProtocolVersion,
    Route, RouteId, RouteState, SessionState,
};
use capyio_protocol::{
    RouteCommandAction, ValidatedControlHeartbeat, ValidatedPortLeaseRequest,
    ValidatedPortLeaseResponse, ValidatedRouteCommand, ValidatedRouteStatus,
    decode_catalog_snapshot, decode_control_heartbeat, decode_envelope, decode_port_lease_request,
    decode_port_lease_response, decode_route_command, decode_route_status,
    encode_control_heartbeat, encode_route_command, encode_route_status, v1,
};
use capyio_runtime::{NodeRuntime, PortLeasePolicy, RuntimeError};
use thiserror::Error;

enum Channel {
    Dialed(AuthenticatedControlClientChannel),
    Accepted(AuthenticatedControlServerChannel),
}

/// No global Node role: dialed/accepted describe this connection only.
pub struct CatalogControlLink {
    instance_id: MessageId,
    channel: Option<Channel>,
    binding: ControlTlsBinding,
    local_node_id: NodeId,
    peer_node_id: NodeId,
    host: PeerCatalogControlHost,
    received_secure_audio_offer: Option<(Route, capyio_protocol::SecureAudioDataOffer)>,
    secure_audio_offer_seen: bool,
}

/// One bounded, publication-filtered snapshot. A host must serialize publication
/// changes with its send queue; preparing does not authorize hardware use.
pub struct PreparedCatalog {
    instance_id: MessageId,
    binding: ControlTlsBinding,
    local_node_id: NodeId,
    bytes: Vec<u8>,
}

/// Authenticated incoming bytes, not yet admitted to a Runtime.
pub struct ReceivedCatalog {
    instance_id: MessageId,
    binding: ControlTlsBinding,
    peer_node_id: NodeId,
    bytes: Vec<u8>,
}

pub enum ReceivedControlMessage {
    DisplaySessionControl(v1::DisplaySessionControl),
    ControllerSettingsRequest(v1::ControllerSettingsRequest),
    ControllerSettingsState(v1::ControllerSettingsState),
    ControllerReceiverOffer(v1::ControllerReceiverOffer),
    Catalog(ReceivedCatalog),
    Heartbeat(ValidatedControlHeartbeat),
    RouteCommand(ValidatedRouteCommand),
    RouteStatus(Box<ValidatedRouteStatus>),
    PortLeaseRequest(ReceivedPortLeaseRequest),
    PortLeaseResponse(ValidatedPortLeaseResponse),
}

fn valid_controller_offer(offer: &v1::ControllerReceiverOffer) -> bool {
    if offer.port == 0 {
        return offer
            .host
            .parse::<std::net::Ipv4Addr>()
            .is_ok_and(|ip| !ip.is_unspecified() && !ip.is_multicast())
            && offer.token.is_empty();
    }
    offer.port <= u16::MAX as u32
        && offer
            .host
            .parse::<std::net::Ipv4Addr>()
            .is_ok_and(|ip| !ip.is_unspecified() && !ip.is_multicast())
        && (8..=64).contains(&offer.token.len())
        && offer.token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

mod native_audio;
pub use native_audio::NativeAudioWorkerPreparation;
mod lease_actions;
pub use lease_actions::{PreparedPortLeaseRequest, PreparedPortLeaseResponse};

pub struct ReceivedPortLeaseRequest {
    instance_id: MessageId,
    binding: ControlTlsBinding,
    request: ValidatedPortLeaseRequest,
    bytes: Vec<u8>,
}

impl ReceivedPortLeaseRequest {
    #[must_use]
    pub const fn request(&self) -> ValidatedPortLeaseRequest {
        self.request
    }
}

impl CatalogControlLink {
    #[must_use]
    pub const fn tls_binding(&self) -> ControlTlsBinding {
        self.binding
    }
    #[must_use]
    pub const fn session_id(&self) -> capyio_core::SessionId {
        self.binding.session_id
    }

    #[must_use]
    pub const fn peer_node_id(&self) -> NodeId {
        self.peer_node_id
    }
    /// Socket address of the authenticated peer, independent of who dialed.
    pub fn authenticated_peer_addr(&self) -> Result<std::net::SocketAddr, CatalogLinkError> {
        match self.channel.as_ref().ok_or(CatalogLinkError::Closed)? {
            Channel::Accepted(channel) => channel.peer_addr().map_err(Into::into),
            Channel::Dialed(channel) => channel.peer_addr().map_err(Into::into),
        }
    }
    /// Native local address of this exact authenticated control connection.
    /// Unlike a wildcard listener address, this identifies the selected interface.
    pub fn authenticated_local_addr(&self) -> Result<std::net::SocketAddr, CatalogLinkError> {
        match self.channel.as_ref().ok_or(CatalogLinkError::Closed)? {
            Channel::Accepted(channel) => channel.local_addr().map_err(Into::into),
            Channel::Dialed(channel) => channel.local_addr().map_err(Into::into),
        }
    }
    /// Pin of the exact authenticated control peer, not a caller-supplied Node ID.
    pub fn authenticated_peer_certificate(
        &self,
    ) -> Result<capyio_control_tls::CertificateFingerprint, CatalogLinkError> {
        match self.channel.as_ref().ok_or(CatalogLinkError::Closed)? {
            Channel::Accepted(channel) => {
                channel.peer_certificate_fingerprint().map_err(Into::into)
            }
            Channel::Dialed(channel) => channel.peer_certificate_fingerprint().map_err(Into::into),
        }
    }
    /// Address of the authenticated accepted peer. Dialed links deliberately do
    /// not satisfy the server-side Linux playback owner contract.
    pub fn accepted_peer_addr(&self) -> Result<std::net::SocketAddr, CatalogLinkError> {
        match self.channel.as_ref().ok_or(CatalogLinkError::Closed)? {
            Channel::Accepted(channel) => channel.peer_addr().map_err(Into::into),
            Channel::Dialed(_) => Err(CatalogLinkError::BindingMismatch),
        }
    }
    /// Admits either the first connection or an explicit reconnect after every
    /// earlier Session for this peer is closed.
    pub fn admit_dialed_peer(
        runtime: &mut NodeRuntime,
        channel: AuthenticatedControlClientChannel,
        paired_display_name: String,
    ) -> Result<Self, CatalogLinkError> {
        Self::admit_peer(runtime, Channel::Dialed(channel), paired_display_name)
    }

    pub fn admit_accepted_peer(
        runtime: &mut NodeRuntime,
        channel: AuthenticatedControlServerChannel,
        paired_display_name: String,
    ) -> Result<Self, CatalogLinkError> {
        Self::admit_peer(runtime, Channel::Accepted(channel), paired_display_name)
    }

    /// Transactionally admits a first authenticated connection for a peer not
    /// yet present in this Runtime. Reconnect of an existing peer is a separate
    /// lifecycle operation and deliberately fails closed here.
    pub fn admit_dialed_new_peer(
        runtime: &mut NodeRuntime,
        channel: AuthenticatedControlClientChannel,
        paired_display_name: String,
    ) -> Result<Self, CatalogLinkError> {
        Self::admit_new_peer(runtime, Channel::Dialed(channel), paired_display_name)
    }

    /// Server-side counterpart to `admit_dialed_new_peer`.
    pub fn admit_accepted_new_peer(
        runtime: &mut NodeRuntime,
        channel: AuthenticatedControlServerChannel,
        paired_display_name: String,
    ) -> Result<Self, CatalogLinkError> {
        Self::admit_new_peer(runtime, Channel::Accepted(channel), paired_display_name)
    }

    pub fn from_dialed(channel: AuthenticatedControlClientChannel) -> Self {
        let binding = channel.binding();
        Self::new(
            Channel::Dialed(channel),
            binding,
            binding.client_node_id,
            binding.server_node_id,
        )
    }

    pub fn from_accepted(channel: AuthenticatedControlServerChannel) -> Self {
        let binding = channel.binding();
        Self::new(
            Channel::Accepted(channel),
            binding,
            binding.server_node_id,
            binding.client_node_id,
        )
    }

    fn new(channel: Channel, binding: ControlTlsBinding, local: NodeId, peer: NodeId) -> Self {
        Self {
            instance_id: MessageId::new(),
            channel: Some(channel),
            binding,
            local_node_id: local,
            peer_node_id: peer,
            host: PeerCatalogControlHost::new(AdmittedPeerBinding::new(binding.session_id, peer)),
            received_secure_audio_offer: None,
            secure_audio_offer_seen: false,
        }
    }

    fn admit_new_peer(
        runtime: &mut NodeRuntime,
        channel: Channel,
        paired_display_name: String,
    ) -> Result<Self, CatalogLinkError> {
        let binding = match &channel {
            Channel::Dialed(channel) => channel.binding(),
            Channel::Accepted(channel) => channel.binding(),
        };
        let (local_node_id, peer_node_id) = match &channel {
            Channel::Dialed(_) => (binding.client_node_id, binding.server_node_id),
            Channel::Accepted(_) => (binding.server_node_id, binding.client_node_id),
        };
        let snapshot = runtime.snapshot();
        if snapshot.local_node.id != local_node_id {
            return Err(CatalogLinkError::BindingMismatch);
        }
        if snapshot.peers.iter().any(|peer| peer.id == peer_node_id) {
            return Err(CatalogLinkError::PeerAlreadyPresent);
        }

        let mut candidate = runtime.clone();
        let provisional_peer = NodeDescriptor::new(
            peer_node_id,
            paired_display_name,
            Platform::Unknown("pending_authenticated_catalog".to_owned()),
            "pending",
            "pending",
            [ProtocolVersion::new(1, 1)],
        );
        candidate.register_peer(provisional_peer, true)?;
        candidate.open_session_with_id(binding.session_id, peer_node_id)?;
        let link = Self::new(channel, binding, local_node_id, peer_node_id);
        link.validate_runtime(&candidate)?;
        *runtime = candidate;
        Ok(link)
    }

    fn admit_peer(
        runtime: &mut NodeRuntime,
        channel: Channel,
        paired_display_name: String,
    ) -> Result<Self, CatalogLinkError> {
        let binding = match &channel {
            Channel::Dialed(channel) => channel.binding(),
            Channel::Accepted(channel) => channel.binding(),
        };
        let (local_node_id, peer_node_id) = match &channel {
            Channel::Dialed(_) => (binding.client_node_id, binding.server_node_id),
            Channel::Accepted(_) => (binding.server_node_id, binding.client_node_id),
        };
        let snapshot = runtime.snapshot();
        if snapshot.local_node.id != local_node_id {
            return Err(CatalogLinkError::BindingMismatch);
        }
        if !snapshot.peers.iter().any(|peer| peer.id == peer_node_id) {
            return Self::admit_new_peer(runtime, channel, paired_display_name);
        }
        if snapshot.sessions.iter().any(|session| {
            session.remote_node_id == peer_node_id && session.state != SessionState::Closed
        }) {
            return Err(CatalogLinkError::PeerAlreadyPresent);
        }

        let mut candidate = runtime.clone();
        candidate.set_peer_online(peer_node_id, true)?;
        candidate.open_session_with_id(binding.session_id, peer_node_id)?;
        let link = Self::new(channel, binding, local_node_id, peer_node_id);
        link.validate_runtime(&candidate)?;
        *runtime = candidate;
        Ok(link)
    }

    fn validate_runtime(&self, runtime: &NodeRuntime) -> Result<(), CatalogLinkError> {
        if self.channel.is_none() {
            return Err(CatalogLinkError::Closed);
        }
        let session = runtime.session(self.binding.session_id)?;
        if session.local_node_id != self.local_node_id
            || session.remote_node_id != self.peer_node_id
        {
            return Err(CatalogLinkError::BindingMismatch);
        }
        if session.state != SessionState::Ready {
            return Err(CatalogLinkError::SessionNotReady);
        }
        Ok(())
    }

    /// Call while observing Runtime, then release its lock before send_catalog.
    pub fn prepare_catalog(
        &self,
        runtime: &NodeRuntime,
        selected: &BTreeSet<CapabilityId>,
    ) -> Result<PreparedCatalog, CatalogLinkError> {
        self.validate_runtime(runtime)?;
        let local = runtime.snapshot().local_node;
        if local.id != self.local_node_id {
            return Err(CatalogLinkError::BindingMismatch);
        }
        Ok(PreparedCatalog {
            instance_id: self.instance_id,
            binding: self.binding,
            local_node_id: local.id,
            bytes: self.host.encode_local_catalog(&local, selected)?,
        })
    }

    /// Performs bounded framed I/O without borrowing Runtime. Dialer sends first;
    /// acceptor receives first, to avoid both sides blocking on large writes.
    pub fn send_catalog(&mut self, catalog: PreparedCatalog) -> Result<(), CatalogLinkError> {
        if catalog.instance_id != self.instance_id
            || catalog.binding != self.binding
            || catalog.local_node_id != self.local_node_id
        {
            return Err(CatalogLinkError::BindingMismatch);
        }
        let result = match self.channel.as_mut().ok_or(CatalogLinkError::Closed)? {
            Channel::Dialed(channel) => channel.send_envelope(&catalog.bytes),
            Channel::Accepted(channel) => channel.send_envelope(&catalog.bytes),
        };
        if result.is_err() {
            self.close();
        }
        result.map_err(Into::into)
    }

    /// Same catalog binding checks, with an absolute whole-write deadline.
    pub fn send_catalog_until(
        &mut self,
        catalog: PreparedCatalog,
        deadline: Instant,
    ) -> Result<(), CatalogLinkError> {
        if catalog.instance_id != self.instance_id
            || catalog.binding != self.binding
            || catalog.local_node_id != self.local_node_id
        {
            return Err(CatalogLinkError::BindingMismatch);
        }
        self.send_bytes_until(&catalog.bytes, deadline)
    }

    /// Receives exactly one CatalogSnapshot; unsupported payloads and wrong
    /// identities terminate this catalog-only connection before Runtime access.
    pub fn receive_catalog(&mut self) -> Result<ReceivedCatalog, CatalogLinkError> {
        match self.receive_control_message()? {
            ReceivedControlMessage::Catalog(catalog) => Ok(catalog),
            _ => {
                self.close();
                Err(CatalogLinkError::UnexpectedPeerControlMessage)
            }
        }
    }

    /// Receives one authenticated catalog or heartbeat frame and validates its
    /// exact Session/Node binding before exposing the typed message.
    pub fn receive_control_message(&mut self) -> Result<ReceivedControlMessage, CatalogLinkError> {
        self.receive_control_message_mode(None)
    }

    /// Whole-frame deadline; expiry is terminal even when a peer trickles bytes.
    pub fn receive_control_message_until(
        &mut self,
        deadline: Instant,
    ) -> Result<ReceivedControlMessage, CatalogLinkError> {
        self.receive_control_message_mode(Some(deadline))
    }

    fn receive_control_message_mode(
        &mut self,
        deadline: Option<Instant>,
    ) -> Result<ReceivedControlMessage, CatalogLinkError> {
        let result = match (
            self.channel.as_mut().ok_or(CatalogLinkError::Closed)?,
            deadline,
        ) {
            (Channel::Dialed(channel), Some(deadline)) => channel.receive_envelope_until(deadline),
            (Channel::Accepted(channel), Some(deadline)) => {
                channel.receive_envelope_until(deadline)
            }
            (Channel::Dialed(channel), None) => channel.receive_envelope(),
            (Channel::Accepted(channel), None) => channel.receive_envelope(),
        };
        let bytes = match result {
            Ok(bytes) => bytes,
            Err(error) => {
                self.close();
                return Err(error.into());
            }
        };
        self.decode_control_message(bytes)
    }

    /// One bounded nonblocking authenticated frame step. Partial frames stay in
    /// the TLS channel; idle is not disconnect. The caller must keep polling to
    /// enforce the absolute partial-frame deadline and local authority expiry.
    pub fn poll_control_message_until(
        &mut self,
        deadline: Instant,
    ) -> Result<Option<ReceivedControlMessage>, CatalogLinkError> {
        let result = match self.channel.as_mut().ok_or(CatalogLinkError::Closed)? {
            Channel::Dialed(channel) => channel.poll_envelope_until(deadline),
            Channel::Accepted(channel) => channel.poll_envelope_until(deadline),
        };
        match result {
            Ok(Some(bytes)) => {
                let decoded = self.decode_control_message(bytes);
                if decoded.is_err() {
                    self.close();
                }
                decoded.map(Some)
            }
            Ok(None) => Ok(None),
            Err(error) => {
                self.close();
                Err(error.into())
            }
        }
    }

    fn decode_control_message(
        &mut self,
        bytes: Vec<u8>,
    ) -> Result<ReceivedControlMessage, CatalogLinkError> {
        let envelope =
            decode_envelope(&bytes).map_err(|_| CatalogLinkError::InvalidPeerControlMessage)?;
        let message = match envelope.payload {
            Some(v1::envelope::Payload::ControllerSettingsRequest(value)) => {
                if envelope.session_id != self.binding.session_id.to_string()
                    || value.request_id == 0
                    || value.expected_revision == 0
                    || value.haptics_strength > 100
                {
                    return Err(CatalogLinkError::InvalidPeerControlMessage);
                }
                return Ok(ReceivedControlMessage::ControllerSettingsRequest(value));
            }
            Some(v1::envelope::Payload::ControllerSettingsState(value)) => {
                if envelope.session_id != self.binding.session_id.to_string()
                    || value.revision == 0
                    || value.haptics_strength > 100
                    || value.error.len() > 128
                {
                    return Err(CatalogLinkError::InvalidPeerControlMessage);
                }
                return Ok(ReceivedControlMessage::ControllerSettingsState(value));
            }
            Some(v1::envelope::Payload::DisplaySessionControl(value)) => {
                if envelope.session_id != self.binding.session_id.to_string()
                    || capyio_protocol::validate_display_session_control(&value).is_err()
                {
                    self.close();
                    return Err(CatalogLinkError::InvalidPeerControlMessage);
                }
                return Ok(ReceivedControlMessage::DisplaySessionControl(value));
            }
            Some(v1::envelope::Payload::ControllerReceiverOffer(offer)) => {
                if envelope.session_id != self.binding.session_id.to_string()
                    || !valid_controller_offer(&offer)
                {
                    self.close();
                    return Err(CatalogLinkError::InvalidPeerControlMessage);
                }
                return Ok(ReceivedControlMessage::ControllerReceiverOffer(offer));
            }
            Some(v1::envelope::Payload::ControlHeartbeat(_)) => {
                ReceivedControlMessage::Heartbeat(decode_control_heartbeat(&bytes)?)
            }
            Some(v1::envelope::Payload::CatalogSnapshot(_)) => {
                let catalog = decode_catalog_snapshot(&bytes)?;
                if catalog.session_id != self.binding.session_id
                    || catalog.node.id != self.peer_node_id
                {
                    self.close();
                    return Err(CatalogLinkError::InvalidPeerCatalog);
                }
                return Ok(ReceivedControlMessage::Catalog(ReceivedCatalog {
                    instance_id: self.instance_id,
                    binding: self.binding,
                    peer_node_id: self.peer_node_id,
                    bytes,
                }));
            }
            Some(v1::envelope::Payload::RouteCommand(_)) => {
                ReceivedControlMessage::RouteCommand(decode_route_command(&bytes)?)
            }
            Some(v1::envelope::Payload::RouteStatus(_)) => {
                let status = match decode_route_status(&bytes) {
                    Ok(status) => status,
                    Err(error) => {
                        self.close();
                        return Err(error.into());
                    }
                };
                if let Some(offer) = status.native_audio_data.as_deref().copied() {
                    let other = match offer.endpoint {
                        capyio_protocol::NativeAudioEndpoint::Source => status.route.sink.node_id,
                        capyio_protocol::NativeAudioEndpoint::Sink => status.route.source.node_id,
                    };
                    if offer.owner(&status.route) != self.peer_node_id
                        || other != self.local_node_id
                    {
                        self.close();
                        return Err(CatalogLinkError::InvalidPeerRoute);
                    }
                }
                if (status.camera_data.is_some() || status.secure_camera_data.is_some())
                    && (status.route.sink.node_id != self.peer_node_id
                        || status.route.source.node_id != self.local_node_id)
                {
                    return Err(CatalogLinkError::InvalidPeerRoute);
                }
                if let Some(offer) = status.secure_audio_data {
                    if status.session_id != self.binding.session_id
                        || status.route.sink.node_id != self.peer_node_id
                        || status.route.source.node_id != self.local_node_id
                        || self.secure_audio_offer_seen
                    {
                        self.close();
                        return Err(CatalogLinkError::InvalidPeerRoute);
                    }
                    // One bounded authenticated receipt per connection, never replenished
                    // by a replay after a failed or completed one-shot media generation.
                    self.secure_audio_offer_seen = true;
                    self.received_secure_audio_offer = Some((status.route.clone(), offer));
                } else if self
                    .received_secure_audio_offer
                    .as_ref()
                    .is_some_and(|(route, _)| route.id == status.route.id)
                {
                    // Any subsequent status invalidates unconsumed preparation.
                    self.received_secure_audio_offer = None;
                }
                ReceivedControlMessage::RouteStatus(Box::new(status))
            }
            Some(v1::envelope::Payload::PortLeaseRequest(_)) => {
                let request = decode_port_lease_request(&bytes)?;
                if request.session_id != self.binding.session_id {
                    self.close();
                    return Err(CatalogLinkError::BindingMismatch);
                }
                return Ok(ReceivedControlMessage::PortLeaseRequest(
                    ReceivedPortLeaseRequest {
                        instance_id: self.instance_id,
                        binding: self.binding,
                        request,
                        bytes,
                    },
                ));
            }
            Some(v1::envelope::Payload::PortLeaseResponse(_)) => {
                ReceivedControlMessage::PortLeaseResponse(decode_port_lease_response(&bytes)?)
            }
            _ => {
                self.close();
                return Err(CatalogLinkError::InvalidPeerControlMessage);
            }
        };
        let session_id = match &message {
            ReceivedControlMessage::Heartbeat(value) => value.session_id,
            ReceivedControlMessage::RouteCommand(value) => value.session_id,
            ReceivedControlMessage::RouteStatus(value) => value.session_id,
            ReceivedControlMessage::PortLeaseResponse(value) => value.session_id,
            ReceivedControlMessage::Catalog(_) | ReceivedControlMessage::PortLeaseRequest(_) => {
                unreachable!()
            }
            ReceivedControlMessage::DisplaySessionControl(_)
            | ReceivedControlMessage::ControllerReceiverOffer(_)
            | ReceivedControlMessage::ControllerSettingsRequest(_)
            | ReceivedControlMessage::ControllerSettingsState(_) => unreachable!(),
        };
        if session_id != self.binding.session_id {
            self.close();
            return Err(CatalogLinkError::BindingMismatch);
        }
        Ok(message)
    }

    fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), CatalogLinkError> {
        let result = match self.channel.as_mut().ok_or(CatalogLinkError::Closed)? {
            Channel::Dialed(channel) => channel.send_envelope(bytes),
            Channel::Accepted(channel) => channel.send_envelope(bytes),
        };
        if result.is_err() {
            self.close();
        }
        result.map_err(Into::into)
    }

    fn send_bytes_until(
        &mut self,
        bytes: &[u8],
        deadline: Instant,
    ) -> Result<(), CatalogLinkError> {
        let result = match self.channel.as_mut().ok_or(CatalogLinkError::Closed)? {
            Channel::Dialed(channel) => channel.send_envelope_until(bytes, deadline),
            Channel::Accepted(channel) => channel.send_envelope_until(bytes, deadline),
        };
        if result.is_err() {
            self.close();
        }
        result.map_err(Into::into)
    }

    pub fn send_route_status_until(
        &mut self,
        route: &Route,
        deadline: Instant,
    ) -> Result<(), CatalogLinkError> {
        self.send_bytes_until(
            &encode_route_status(self.binding.session_id, route)?,
            deadline,
        )
    }

    pub fn send_heartbeat_until(
        &mut self,
        sequence: u64,
        acknowledgement: bool,
        deadline: Instant,
    ) -> Result<(), CatalogLinkError> {
        self.send_bytes_until(
            &encode_control_heartbeat(self.binding.session_id, sequence, acknowledgement)?,
            deadline,
        )
    }

    pub fn send_route_status(&mut self, route: &Route) -> Result<(), CatalogLinkError> {
        self.send_bytes(&encode_route_status(self.binding.session_id, route)?)
    }

    pub fn send_touchpad_data_offer(
        &mut self,
        route: &Route,
        offer: capyio_protocol::TouchpadDataOffer,
    ) -> Result<(), CatalogLinkError> {
        self.send_bytes(&capyio_protocol::encode_route_status_with_touchpad_data(
            self.binding.session_id,
            route,
            Some(offer),
        )?)
    }

    pub fn send_haptics_data_offer(
        &mut self,
        route: &Route,
        offer: capyio_protocol::HapticsDataOffer,
    ) -> Result<(), CatalogLinkError> {
        self.send_bytes(&capyio_protocol::encode_route_status_with_haptics_data(
            self.binding.session_id,
            route,
            offer,
        )?)
    }

    /// Endpoint metadata for the explicitly insecure native audio lab backend.
    /// Caller must separately authorize that transport and hold current leases.
    pub fn send_native_audio_data_offer(
        &mut self,
        route: &Route,
        offer: capyio_protocol::NativeAudioDataOffer,
        now_ms: u64,
    ) -> Result<(), CatalogLinkError> {
        self.send_native_audio_data_offer_mode(route, offer, now_ms, None)
    }

    pub fn send_native_audio_data_offer_until(
        &mut self,
        route: &Route,
        offer: capyio_protocol::NativeAudioDataOffer,
        now_ms: u64,
        deadline: Instant,
    ) -> Result<(), CatalogLinkError> {
        self.send_native_audio_data_offer_mode(route, offer, now_ms, Some(deadline))
    }

    fn send_native_audio_data_offer_mode(
        &mut self,
        route: &Route,
        offer: capyio_protocol::NativeAudioDataOffer,
        now_ms: u64,
        deadline: Option<Instant>,
    ) -> Result<(), CatalogLinkError> {
        let other = match offer.endpoint {
            capyio_protocol::NativeAudioEndpoint::Source => route.sink.node_id,
            capyio_protocol::NativeAudioEndpoint::Sink => route.source.node_id,
        };
        if offer.owner(route) != self.local_node_id
            || other != self.peer_node_id
            || !matches!(route.authorization, AuthorizationState::Authorized {
                expires_at_ms: Some(expiry) } if expiry > now_ms)
        {
            return Err(CatalogLinkError::InvalidPeerRoute);
        }
        let bytes = capyio_protocol::encode_route_status_with_native_audio_data(
            self.binding.session_id,
            route,
            offer,
        )?;
        match deadline {
            Some(deadline) => self.send_bytes_until(&bytes, deadline),
            None => self.send_bytes(&bytes),
        }
    }

    pub fn send_camera_data_offer(
        &mut self,
        route: &Route,
        offer: capyio_protocol::CameraDataOffer,
        now_ms: u64,
    ) -> Result<(), CatalogLinkError> {
        if route.sink.node_id != self.local_node_id
            || route.source.node_id != self.peer_node_id
            || !matches!(route.authorization, AuthorizationState::Authorized { expires_at_ms: Some(expiry) } if expiry > now_ms)
        {
            return Err(CatalogLinkError::InvalidPeerRoute);
        }
        self.send_bytes(&capyio_protocol::encode_route_status_with_camera_data(
            self.binding.session_id,
            route,
            offer,
        )?)
    }

    /// Send the local Sink's versioned secure camera listener on this exact Session.
    /// No host is accepted: the recipient derives the IP from its authenticated socket.
    pub fn send_secure_camera_data_offer_until(
        &mut self,
        route: &Route,
        offer: capyio_protocol::SecureCameraDataOffer,
        now_ms: u64,
        deadline: Instant,
    ) -> Result<(), CatalogLinkError> {
        if route.session_id != self.binding.session_id
            || route.sink.node_id != self.local_node_id
            || route.source.node_id != self.peer_node_id
            || !matches!(route.authorization, AuthorizationState::Authorized { expires_at_ms: Some(expiry) } if expiry > now_ms)
        {
            return Err(CatalogLinkError::InvalidPeerRoute);
        }
        self.send_bytes_until(
            &capyio_protocol::encode_route_status_with_secure_camera_data(
                self.binding.session_id,
                route,
                offer,
            )?,
            deadline,
        )
    }

    /// Send only the local Sink's exact secure mono endpoint on this Session.
    pub fn send_secure_audio_data_offer_until(
        &mut self,
        route: &Route,
        offer: capyio_protocol::SecureAudioDataOffer,
        now_ms: u64,
        deadline: Instant,
    ) -> Result<(), CatalogLinkError> {
        if offer.transport != capyio_protocol::SecureAudioTransport::Tls13MtlsV1 {
            return Err(CatalogLinkError::InvalidPeerRoute);
        }
        self.send_secure_audio_offer_until(route, offer, now_ms, deadline)
    }

    /// Stereo-only counterpart. The old mono endpoint API cannot send this offer.
    pub fn send_secure_stereo_audio_data_offer_until(
        &mut self,
        route: &Route,
        offer: capyio_protocol::SecureAudioDataOffer,
        now_ms: u64,
        deadline: Instant,
    ) -> Result<(), CatalogLinkError> {
        if offer.transport != capyio_protocol::SecureAudioTransport::Tls13MtlsStereoV1 {
            return Err(CatalogLinkError::InvalidPeerRoute);
        }
        self.send_secure_audio_offer_until(route, offer, now_ms, deadline)
    }

    fn send_secure_audio_offer_until(
        &mut self,
        route: &Route,
        offer: capyio_protocol::SecureAudioDataOffer,
        now_ms: u64,
        deadline: Instant,
    ) -> Result<(), CatalogLinkError> {
        if route.session_id != self.binding.session_id
            || route.sink.node_id != self.local_node_id
            || route.source.node_id != self.peer_node_id
            || !matches!(route.authorization, AuthorizationState::Authorized { expires_at_ms: Some(expiry) } if expiry > now_ms)
        {
            return Err(CatalogLinkError::InvalidPeerRoute);
        }
        self.send_bytes_until(
            &capyio_protocol::encode_route_status_with_secure_audio_data(
                self.binding.session_id,
                route,
                offer,
            )?,
            deadline,
        )
    }

    /// Consume proof that this exact endpoint came from the authenticated Sink.
    /// This is not consent or a lease. A caller must independently freeze the
    /// selected AudioStreamSpec and recheck live Runtime authority before I/O.
    /// Even a failed preparation consumes the receipt; repeated wire offers are
    /// rejected, so a second secure generation requires a fresh control link.
    pub fn take_received_secure_audio_offer(
        &mut self,
        route: &Route,
        offer: capyio_protocol::SecureAudioDataOffer,
    ) -> Result<(), CatalogLinkError> {
        self.take_received_audio_offer_for_transport(
            route,
            offer,
            capyio_protocol::SecureAudioTransport::Tls13MtlsV1,
        )
    }

    /// Consume only the separate stereo receipt; mono callers cannot take it.
    pub fn take_received_secure_stereo_audio_offer(
        &mut self,
        route: &Route,
        offer: capyio_protocol::SecureAudioDataOffer,
    ) -> Result<(), CatalogLinkError> {
        self.take_received_audio_offer_for_transport(
            route,
            offer,
            capyio_protocol::SecureAudioTransport::Tls13MtlsStereoV1,
        )
    }

    fn take_received_audio_offer_for_transport(
        &mut self,
        route: &Route,
        offer: capyio_protocol::SecureAudioDataOffer,
        transport: capyio_protocol::SecureAudioTransport,
    ) -> Result<(), CatalogLinkError> {
        // Reject the wrong API before taking even a mismatched receipt. After
        // this discriminator check, all same-transport attempts stay one-shot.
        if offer.transport != transport
            || self
                .received_secure_audio_offer
                .as_ref()
                .is_some_and(|(_, original)| original.transport != transport)
        {
            return Err(CatalogLinkError::InvalidPeerRoute);
        }
        let received = self.received_secure_audio_offer.take();
        if self.channel.is_none() {
            return Err(CatalogLinkError::Closed);
        }
        offer.validate(route)?;
        let Some((original, received_offer)) = received else {
            return Err(CatalogLinkError::InvalidPeerRoute);
        };
        if received_offer != offer
            || route.id != original.id
            || route.session_id != original.session_id
            || route.session_id != self.binding.session_id
            || route.source != original.source
            || route.sink != original.sink
            || route.profile != original.profile
            || route.backend != original.backend
            || route.epoch != original.epoch
            || route.authorization != original.authorization
            || route.selected_format != original.selected_format
            || route.selected_qos_mode != original.selected_qos_mode
            || route.compatible_formats != original.compatible_formats
            || route.compatible_qos_modes != original.compatible_qos_modes
        {
            return Err(CatalogLinkError::InvalidPeerRoute);
        }
        Ok(())
    }

    pub fn send_route_command(
        &mut self,
        route_id: RouteId,
        action: RouteCommandAction,
    ) -> Result<(), CatalogLinkError> {
        self.send_bytes(&encode_route_command(
            self.binding.session_id,
            route_id,
            action,
        )?)
    }

    /// Whole-operation deadline, shared with the caller's finite negotiation budget.
    pub fn send_route_command_until(
        &mut self,
        route_id: RouteId,
        action: RouteCommandAction,
        deadline: Instant,
    ) -> Result<(), CatalogLinkError> {
        self.send_bytes_until(
            &encode_route_command(self.binding.session_id, route_id, action)?,
            deadline,
        )
    }

    /// Mirrors a peer Route proposal/status into this Runtime. A new proposal
    /// must be a Draft, unauthorized Route for this exact authenticated
    /// Session. `authorize_local_use` is a platform policy decision (for
    /// example an explicitly published Capability), never a transport default.
    pub fn apply_route_status(
        &self,
        runtime: &mut NodeRuntime,
        status: &ValidatedRouteStatus,
        authorize_local_use: bool,
    ) -> Result<Route, CatalogLinkError> {
        self.validate_runtime(runtime)?;
        if status.session_id != self.binding.session_id
            || status.route.session_id != self.binding.session_id
        {
            return Err(CatalogLinkError::BindingMismatch);
        }
        let incoming = &status.route;
        let local_endpoint = if incoming.source.node_id == self.local_node_id {
            incoming.source
        } else if incoming.sink.node_id == self.local_node_id {
            incoming.sink
        } else {
            return Err(CatalogLinkError::InvalidPeerRoute);
        };
        if let Ok(existing) = runtime.route(incoming.id) {
            if existing.session_id != incoming.session_id
                || existing.source != incoming.source
                || existing.sink != incoming.sink
                || existing.backend != incoming.backend
            {
                return Err(CatalogLinkError::InvalidPeerRoute);
            }
        } else {
            if incoming.state != RouteState::Draft
                || incoming.authorization != AuthorizationState::Pending
            {
                return Err(CatalogLinkError::InvalidPeerRoute);
            }
            runtime.create_route_with_id(
                incoming.id,
                incoming.session_id,
                incoming.source,
                incoming.sink,
                incoming.backend,
            )?;
        }
        if authorize_local_use {
            if runtime.route(incoming.id)?.authorization == AuthorizationState::Pending {
                runtime.authorize_route(incoming.id, None)?;
            }
            if runtime
                .route_port_lease_claim(incoming.id, local_endpoint)
                .is_none()
            {
                match runtime
                    .set_local_port_lease_policy(local_endpoint, PortLeasePolicy::Exclusive)
                {
                    Ok(())
                    | Err(RuntimeError::Lease(capyio_runtime::PortLeaseError::PolicyInUse(_))) => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Ok(runtime.route(incoming.id)?.clone())
    }

    pub fn apply_route_command(
        &self,
        runtime: &mut NodeRuntime,
        command: &ValidatedRouteCommand,
        now_ms: u64,
    ) -> Result<Route, CatalogLinkError> {
        self.apply_route_command_mode(runtime, command, now_ms, false)
    }

    /// Starts only the staged Runtime lifecycle. A real Adapter must report
    /// readiness separately; receiving Start alone never proves data delivery.
    pub fn stage_route_command(
        &self,
        runtime: &mut NodeRuntime,
        command: &ValidatedRouteCommand,
        now_ms: u64,
    ) -> Result<Route, CatalogLinkError> {
        self.apply_route_command_mode(runtime, command, now_ms, true)
    }

    fn apply_route_command_mode(
        &self,
        runtime: &mut NodeRuntime,
        command: &ValidatedRouteCommand,
        now_ms: u64,
        staged: bool,
    ) -> Result<Route, CatalogLinkError> {
        self.validate_runtime(runtime)?;
        if command.session_id != self.binding.session_id
            || runtime.route(command.route_id)?.session_id != self.binding.session_id
        {
            return Err(CatalogLinkError::BindingMismatch);
        }
        if matches!(command.action, RouteCommandAction::Start) {
            let route = runtime.route(command.route_id)?;
            let local_endpoint = if route.source.node_id == self.local_node_id {
                route.source
            } else if route.sink.node_id == self.local_node_id {
                route.sink
            } else {
                return Err(CatalogLinkError::InvalidPeerRoute);
            };
            let claim = runtime
                .route_port_lease_claim(command.route_id, local_endpoint)
                .ok_or(CatalogLinkError::RouteLeaseRequired)?;
            if claim.expires_at_ms <= now_ms {
                return Err(CatalogLinkError::RouteLeaseRequired);
            }
        }
        if staged && matches!(command.action, RouteCommandAction::Start) {
            let route = runtime.route(command.route_id)?.clone();
            if route.state == RouteState::Draft {
                let qos = route
                    .compatible_qos_modes
                    .iter()
                    .next()
                    .cloned()
                    .ok_or(CatalogLinkError::InvalidPeerRoute)?;
                runtime.prepare_route(
                    route.id,
                    route.compatible_formats.first().cloned(),
                    qos,
                    now_ms,
                )?;
            }
            if runtime.route(route.id)?.state == RouteState::Prepared {
                runtime.begin_route_start(route.id, now_ms)?;
            }
            if !matches!(
                runtime.route(route.id)?.state,
                RouteState::Starting | RouteState::Active
            ) {
                return Err(CatalogLinkError::InvalidPeerRoute);
            }
            return Ok(runtime.route(route.id)?.clone());
        }
        if matches!(command.action, RouteCommandAction::Stop)
            && runtime.route(command.route_id)?.state == RouteState::Draft
            && runtime.route(command.route_id)?.authorization == AuthorizationState::Pending
        {
            runtime.deny_pending_route(command.route_id)?;
            return Ok(runtime.route(command.route_id)?.clone());
        }
        runtime.set_route_active(
            command.route_id,
            matches!(command.action, RouteCommandAction::Start),
            now_ms,
        )?;
        Ok(runtime.route(command.route_id)?.clone())
    }

    pub fn send_heartbeat(
        &mut self,
        sequence: u64,
        acknowledgement: bool,
    ) -> Result<(), CatalogLinkError> {
        let bytes = encode_control_heartbeat(self.binding.session_id, sequence, acknowledgement)?;
        let result = match self.channel.as_mut().ok_or(CatalogLinkError::Closed)? {
            Channel::Dialed(channel) => channel.send_envelope(&bytes),
            Channel::Accepted(channel) => channel.send_envelope(&bytes),
        };
        if result.is_err() {
            self.close();
        }
        result.map_err(Into::into)
    }

    pub fn send_controller_receiver_offer(
        &mut self,
        offer: v1::ControllerReceiverOffer,
    ) -> Result<(), CatalogLinkError> {
        if !valid_controller_offer(&offer) {
            return Err(CatalogLinkError::InvalidPeerControlMessage);
        }
        let envelope = capyio_protocol::new_envelope(
            Some(self.binding.session_id),
            v1::envelope::Payload::ControllerReceiverOffer(offer),
        );
        let bytes = capyio_protocol::encode_envelope_checked(&envelope)?;
        self.send_bytes(&bytes)
    }

    /// Bounded launch/ack metadata over the already pinned node connection.
    /// Callers must never log the secret-bearing launch record or forward it
    /// to the UI. Receiving it does not itself grant hardware authorization.
    pub fn send_display_session_control(
        &mut self,
        value: v1::DisplaySessionControl,
    ) -> Result<(), CatalogLinkError> {
        capyio_protocol::validate_display_session_control(&value)?;
        let envelope = capyio_protocol::new_envelope(
            Some(self.binding.session_id),
            v1::envelope::Payload::DisplaySessionControl(value),
        );
        self.send_bytes(&capyio_protocol::encode_envelope_checked(&envelope)?)
    }

    pub fn send_controller_settings_request(
        &mut self,
        value: v1::ControllerSettingsRequest,
    ) -> Result<(), CatalogLinkError> {
        let envelope = capyio_protocol::new_envelope(
            Some(self.binding.session_id),
            v1::envelope::Payload::ControllerSettingsRequest(value),
        );
        self.send_bytes(&capyio_protocol::encode_envelope_checked(&envelope)?)
    }

    pub fn send_controller_settings_state(
        &mut self,
        value: v1::ControllerSettingsState,
    ) -> Result<(), CatalogLinkError> {
        let envelope = capyio_protocol::new_envelope(
            Some(self.binding.session_id),
            v1::envelope::Payload::ControllerSettingsState(value),
        );
        self.send_bytes(&capyio_protocol::encode_envelope_checked(&envelope)?)
    }

    pub fn receive_heartbeat(&mut self) -> Result<ValidatedControlHeartbeat, CatalogLinkError> {
        match self.receive_control_message()? {
            ReceivedControlMessage::Heartbeat(heartbeat) => Ok(heartbeat),
            _ => {
                self.close();
                Err(CatalogLinkError::UnexpectedPeerControlMessage)
            }
        }
    }

    /// Rechecks the current Runtime Session, then atomically admits the catalog.
    /// No socket I/O occurs, and no Route or hardware is started or authorized.
    pub fn apply_catalog(
        &mut self,
        runtime: &mut NodeRuntime,
        catalog: ReceivedCatalog,
    ) -> Result<MessageId, CatalogLinkError> {
        if catalog.instance_id != self.instance_id
            || catalog.binding != self.binding
            || catalog.peer_node_id != self.peer_node_id
        {
            return Err(CatalogLinkError::BindingMismatch);
        }
        if let Err(error) = self.validate_runtime(runtime) {
            self.close();
            return Err(error);
        }
        match self.host.admit_catalog(runtime, &catalog.bytes) {
            Ok(id) => Ok(id),
            Err(error) => {
                self.close();
                Err(error.into())
            }
        }
    }

    pub fn close(&mut self) {
        self.received_secure_audio_offer = None;
        self.channel.take();
    }

    /// Drops transport ownership first, then atomically closes the exact bound
    /// Runtime Session. Repeated cleanup is safe.
    pub fn close_and_reconcile(
        &mut self,
        runtime: &mut NodeRuntime,
    ) -> Result<(), CatalogLinkError> {
        self.close();
        let session = runtime.session(self.binding.session_id)?;
        if session.local_node_id != self.local_node_id
            || session.remote_node_id != self.peer_node_id
        {
            return Err(CatalogLinkError::BindingMismatch);
        }
        runtime.close_session(self.binding.session_id)?;
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        self.channel.is_none()
    }
}

#[derive(Debug, Error)]
pub enum CatalogLinkError {
    #[error("native audio worker is not ready: {0}")]
    NativeAudioNotReady(&'static str),
    #[error("catalog control connection is closed")]
    Closed,
    #[error("catalog control Runtime/Session/Node binding does not match")]
    BindingMismatch,
    #[error("catalog control Session is not ready")]
    SessionNotReady,
    #[error("authenticated peer already exists; reconnect requires explicit reconciliation")]
    PeerAlreadyPresent,
    #[error("peer catalog is malformed, unsupported or belongs to another Session/Node")]
    InvalidPeerCatalog,
    #[error("peer Route is not a valid proposal/status for this authenticated Session")]
    InvalidPeerRoute,
    #[error("peer Route start requires an active owner-authoritative local Port lease")]
    RouteLeaseRequired,
    #[error("peer control message is malformed or unsupported")]
    InvalidPeerControlMessage,
    #[error("peer sent a valid control message that is not allowed in this exchange step")]
    UnexpectedPeerControlMessage,
    #[error(transparent)]
    Transport(#[from] ControlTlsError),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    Publication(#[from] PublicationError),
    #[error(transparent)]
    Protocol(#[from] capyio_protocol::ProtocolError),
    #[error(transparent)]
    Admission(#[from] ControlHostError),
}
