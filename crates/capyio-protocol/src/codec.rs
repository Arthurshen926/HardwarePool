use std::str::FromStr;

use capyio_core::{MessageId, NodeDescriptor, PortRef, RouteId, SessionId};
use prost::Message;

use crate::{PROTOCOL_MAJOR, PROTOCOL_MINOR, ProtocolError, v1};

/// Defensive bootstrap limit for one control-plane envelope.
pub const MAX_CONTROL_ENVELOPE_BYTES: usize = 1024 * 1024;
/// Maximum Adapter descriptors carried by one Node catalog.
pub const MAX_CATALOG_ADAPTERS: usize = 64;
/// Maximum Capability descriptors carried by one Node catalog.
pub const MAX_CATALOG_CAPABILITIES: usize = 256;
/// Maximum Ports carried by one Capability descriptor.
pub const MAX_CATALOG_PORTS_PER_CAPABILITY: usize = 32;
/// Maximum format alternatives carried by one Port descriptor.
pub const MAX_CATALOG_FORMATS_PER_PORT: usize = 32;
/// Maximum metadata or format-parameter entries in one descriptor.
pub const MAX_CATALOG_METADATA_ENTRIES: usize = 64;
/// Maximum protocol versions advertised by one Node.
pub const MAX_CATALOG_PROTOCOL_VERSIONS: usize = 16;
/// Maximum UTF-8 byte length of one catalog string field.
pub const MAX_CATALOG_STRING_BYTES: usize = 2 * 1024;
const MAX_CATALOG_ENUM_LIST_ENTRIES: usize = 16;
pub const MIN_PORT_LEASE_TTL_MS: u64 = 1_000;
pub const MAX_PORT_LEASE_TTL_MS: u64 = 5 * 60 * 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortLeaseAction {
    Acquire,
    Renew,
    Release,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortLeaseOutcome {
    Acquired,
    Renewed,
    Released,
    Conflict,
    Denied,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteCommandAction {
    Start,
    Stop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRouteCommand {
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub route_id: RouteId,
    pub action: RouteCommandAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRouteStatus {
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub route: capyio_core::Route,
    pub touchpad_data: Option<TouchpadDataOffer>,
    pub haptics_data: Option<HapticsDataOffer>,
    pub native_audio_data: Option<Box<crate::NativeAudioDataOffer>>,
    pub camera_data: Option<crate::CameraDataOffer>,
    pub secure_camera_data: Option<crate::SecureCameraDataOffer>,
    pub secure_audio_data: Option<crate::SecureAudioDataOffer>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TouchpadDataOffer {
    pub port: u16,
    pub stream_id: capyio_core::StreamId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HapticsDataOffer {
    pub port: u16,
    pub stream_id: capyio_core::StreamId,
}

impl HapticsDataOffer {
    fn validate(self, route: &capyio_core::Route) -> Result<(), ProtocolError> {
        use capyio_core::{AuthorizationState, RouteBackend, RouteState};
        if self.port == 0
            || self.stream_id.as_uuid().is_nil()
            || route.epoch == 0
            || route.profile != capyio_core::ProfileId::haptics_feedback_v1()
            || route.backend != RouteBackend::AdapterManaged
            || !matches!(route.state, RouteState::Starting | RouteState::Active)
            || !matches!(route.authorization, AuthorizationState::Authorized { expires_at_ms: Some(expiry) } if expiry > 0)
        {
            return Err(ProtocolError::InvalidRouteControl(
                "invalid haptics data offer",
            ));
        }
        Ok(())
    }
}

impl TouchpadDataOffer {
    fn validate(self, route: &capyio_core::Route) -> Result<(), ProtocolError> {
        use capyio_core::{AuthorizationState, RouteBackend, RouteState};
        if self.port == 0
            || self.stream_id.as_uuid().is_nil()
            || route.epoch == 0
            || route.profile != capyio_core::ProfileId::touchpad_frames_v1()
            || route.backend != RouteBackend::AdapterManaged
            || !matches!(route.state, RouteState::Starting | RouteState::Active)
            || !matches!(route.authorization, AuthorizationState::Authorized { expires_at_ms: Some(expiry) } if expiry > 0)
        {
            return Err(ProtocolError::InvalidRouteControl(
                "invalid touchpad data offer",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedPortLeaseRequest {
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub route_id: RouteId,
    pub port: PortRef,
    pub action: PortLeaseAction,
    pub requested_ttl_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedPortLeaseResponse {
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub request_message_id: MessageId,
    pub route_id: RouteId,
    pub port: PortRef,
    pub outcome: PortLeaseOutcome,
    pub expires_at_ms: Option<u64>,
}

/// A structurally and semantically validated catalog control message. Peer
/// authentication and replay admission remain transport/host responsibilities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedCatalogSnapshot {
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub node: NodeDescriptor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedControlHeartbeat {
    pub message_id: MessageId,
    pub session_id: SessionId,
    pub sequence: u64,
    pub acknowledgement: bool,
}

/// Encodes an already validated envelope.
#[must_use]
pub fn encode_envelope(envelope: &v1::Envelope) -> Vec<u8> {
    envelope.encode_to_vec()
}

/// Validates and encodes a control envelope.
pub fn encode_envelope_checked(envelope: &v1::Envelope) -> Result<Vec<u8>, ProtocolError> {
    validate_envelope(envelope)?;
    let bytes = encode_envelope(envelope);
    if bytes.len() > MAX_CONTROL_ENVELOPE_BYTES {
        return Err(ProtocolError::MessageTooLarge {
            limit: MAX_CONTROL_ENVELOPE_BYTES,
            actual: bytes.len(),
        });
    }
    Ok(bytes)
}

/// Converts one complete Core Node catalog into a bounded, Session-bound
/// CatalogSnapshot envelope. The caller still owns Session authorization,
/// transport authentication and delivery.
pub fn encode_catalog_snapshot(
    session_id: SessionId,
    node: &NodeDescriptor,
) -> Result<Vec<u8>, ProtocolError> {
    let node = v1::NodeDescriptor::try_from(node)?;
    validate_catalog_bounds(&node)?;
    let envelope = new_envelope(
        Some(session_id),
        v1::envelope::Payload::CatalogSnapshot(v1::CatalogSnapshot { node: Some(node) }),
    );
    encode_envelope_checked(&envelope)
}

/// Decodes one bounded control message and validates required semantic fields.
pub fn decode_envelope(bytes: &[u8]) -> Result<v1::Envelope, ProtocolError> {
    if bytes.len() > MAX_CONTROL_ENVELOPE_BYTES {
        return Err(ProtocolError::MessageTooLarge {
            limit: MAX_CONTROL_ENVELOPE_BYTES,
            actual: bytes.len(),
        });
    }
    let envelope = v1::Envelope::decode(bytes)?;
    validate_envelope(&envelope)?;
    Ok(envelope)
}

/// Decodes one complete CatalogSnapshot envelope and converts its public wire
/// descriptor into a validated Core value ready for host-side Session admission.
pub fn decode_catalog_snapshot(bytes: &[u8]) -> Result<ValidatedCatalogSnapshot, ProtocolError> {
    let envelope = decode_envelope(bytes)?;
    let message_id =
        MessageId::from_str(&envelope.message_id).map_err(|_| ProtocolError::InvalidId {
            field: "envelope.message_id",
            value: envelope.message_id.clone(),
        })?;
    if envelope.session_id.is_empty() {
        return Err(ProtocolError::MissingField("envelope.session_id"));
    }
    let session_id =
        SessionId::from_str(&envelope.session_id).map_err(|_| ProtocolError::InvalidId {
            field: "envelope.session_id",
            value: envelope.session_id.clone(),
        })?;
    let snapshot = match envelope.payload {
        Some(v1::envelope::Payload::CatalogSnapshot(snapshot)) => snapshot,
        _ => {
            return Err(ProtocolError::UnexpectedPayload {
                expected: "catalog_snapshot",
            });
        }
    };
    let node = snapshot
        .node
        .ok_or(ProtocolError::MissingField("catalog_snapshot.node"))?;
    validate_catalog_bounds(&node)?;
    let node = node.try_into()?;
    Ok(ValidatedCatalogSnapshot {
        message_id,
        session_id,
        node,
    })
}

pub fn encode_control_heartbeat(
    session_id: SessionId,
    sequence: u64,
    acknowledgement: bool,
) -> Result<Vec<u8>, ProtocolError> {
    if sequence == 0 {
        return Err(ProtocolError::InvalidControlHeartbeat);
    }
    let envelope = new_envelope(
        Some(session_id),
        v1::envelope::Payload::ControlHeartbeat(v1::ControlHeartbeat {
            sequence,
            acknowledgement,
        }),
    );
    encode_envelope_checked(&envelope)
}

pub fn decode_control_heartbeat(bytes: &[u8]) -> Result<ValidatedControlHeartbeat, ProtocolError> {
    let envelope = decode_envelope(bytes)?;
    let message_id =
        MessageId::from_str(&envelope.message_id).map_err(|_| ProtocolError::InvalidId {
            field: "envelope.message_id",
            value: envelope.message_id.clone(),
        })?;
    let session_id =
        SessionId::from_str(&envelope.session_id).map_err(|_| ProtocolError::InvalidId {
            field: "envelope.session_id",
            value: envelope.session_id.clone(),
        })?;
    let heartbeat = match envelope.payload {
        Some(v1::envelope::Payload::ControlHeartbeat(heartbeat)) => heartbeat,
        _ => {
            return Err(ProtocolError::UnexpectedPayload {
                expected: "control_heartbeat",
            });
        }
    };
    if heartbeat.sequence == 0 {
        return Err(ProtocolError::InvalidControlHeartbeat);
    }
    Ok(ValidatedControlHeartbeat {
        message_id,
        session_id,
        sequence: heartbeat.sequence,
        acknowledgement: heartbeat.acknowledgement,
    })
}

pub fn encode_port_lease_request(
    session_id: SessionId,
    route_id: RouteId,
    port: PortRef,
    action: PortLeaseAction,
    requested_ttl_ms: u64,
) -> Result<Vec<u8>, ProtocolError> {
    validate_lease_ttl(action, requested_ttl_ms)?;
    let envelope = new_envelope(
        Some(session_id),
        v1::envelope::Payload::PortLeaseRequest(v1::PortLeaseRequest {
            route_id: route_id.to_string(),
            port: Some((&port).into()),
            action: lease_action_to_wire(action),
            requested_ttl_ms,
        }),
    );
    encode_envelope_checked(&envelope)
}

pub fn encode_route_command(
    session_id: SessionId,
    route_id: RouteId,
    action: RouteCommandAction,
) -> Result<Vec<u8>, ProtocolError> {
    let action = match action {
        RouteCommandAction::Start => v1::RouteCommandAction::Start as i32,
        RouteCommandAction::Stop => v1::RouteCommandAction::Stop as i32,
    };
    let envelope = new_envelope(
        Some(session_id),
        v1::envelope::Payload::RouteCommand(v1::RouteCommand {
            route_id: route_id.to_string(),
            action,
        }),
    );
    encode_envelope_checked(&envelope)
}

pub fn decode_route_command(bytes: &[u8]) -> Result<ValidatedRouteCommand, ProtocolError> {
    let envelope = decode_envelope(bytes)?;
    let (message_id, session_id) = parse_bound_envelope_ids(&envelope)?;
    let command = match envelope.payload {
        Some(v1::envelope::Payload::RouteCommand(command)) => command,
        _ => {
            return Err(ProtocolError::UnexpectedPayload {
                expected: "route_command",
            });
        }
    };
    let route_id = parse_id("route_command.route_id", &command.route_id)?;
    let action = match v1::RouteCommandAction::try_from(command.action) {
        Ok(v1::RouteCommandAction::Start) => RouteCommandAction::Start,
        Ok(v1::RouteCommandAction::Stop) => RouteCommandAction::Stop,
        _ => {
            return Err(ProtocolError::InvalidEnum {
                field: "route_command.action",
                value: command.action,
            });
        }
    };
    Ok(ValidatedRouteCommand {
        message_id,
        session_id,
        route_id,
        action,
    })
}

pub fn encode_route_status(
    session_id: SessionId,
    route: &capyio_core::Route,
) -> Result<Vec<u8>, ProtocolError> {
    encode_route_status_with_touchpad_data(session_id, route, None)
}

pub fn encode_route_status_with_touchpad_data(
    session_id: SessionId,
    route: &capyio_core::Route,
    touchpad_data: Option<TouchpadDataOffer>,
) -> Result<Vec<u8>, ProtocolError> {
    encode_route_status_data(
        session_id,
        route,
        touchpad_data.map(RouteDataOffer::Touchpad),
    )
}

pub fn encode_route_status_with_haptics_data(
    session_id: SessionId,
    route: &capyio_core::Route,
    offer: HapticsDataOffer,
) -> Result<Vec<u8>, ProtocolError> {
    encode_route_status_data(session_id, route, Some(RouteDataOffer::Haptics(offer)))
}

pub fn encode_route_status_with_native_audio_data(
    session_id: SessionId,
    route: &capyio_core::Route,
    offer: crate::NativeAudioDataOffer,
) -> Result<Vec<u8>, ProtocolError> {
    encode_route_status_data(session_id, route, Some(RouteDataOffer::NativeAudio(offer)))
}

pub fn encode_route_status_with_camera_data(
    session_id: SessionId,
    route: &capyio_core::Route,
    offer: crate::CameraDataOffer,
) -> Result<Vec<u8>, ProtocolError> {
    encode_route_status_data(session_id, route, Some(RouteDataOffer::Camera(offer)))
}

pub fn encode_route_status_with_secure_camera_data(
    session_id: SessionId,
    route: &capyio_core::Route,
    offer: crate::SecureCameraDataOffer,
) -> Result<Vec<u8>, ProtocolError> {
    encode_route_status_data(session_id, route, Some(RouteDataOffer::SecureCamera(offer)))
}

pub fn encode_route_status_with_secure_audio_data(
    session_id: SessionId,
    route: &capyio_core::Route,
    offer: crate::SecureAudioDataOffer,
) -> Result<Vec<u8>, ProtocolError> {
    encode_route_status_data(session_id, route, Some(RouteDataOffer::SecureAudio(offer)))
}

// Exclusive by construction: the encoder cannot produce mixed data offers.
enum RouteDataOffer {
    Touchpad(TouchpadDataOffer),
    NativeAudio(crate::NativeAudioDataOffer),
    Haptics(HapticsDataOffer),
    Camera(crate::CameraDataOffer),
    SecureCamera(crate::SecureCameraDataOffer),
    SecureAudio(crate::SecureAudioDataOffer),
}

fn encode_route_status_data(
    session_id: SessionId,
    route: &capyio_core::Route,
    offer: Option<RouteDataOffer>,
) -> Result<Vec<u8>, ProtocolError> {
    if route.session_id != session_id {
        return Err(ProtocolError::InvalidRouteControl(
            "Route belongs to another Session",
        ));
    }
    let mut status = v1::RouteStatus {
        route: Some(route.try_into()?),
        ..Default::default()
    };
    match offer {
        Some(RouteDataOffer::Touchpad(offer)) => {
            offer.validate(route)?;
            status.touchpad_data = Some(v1::TouchpadDataOffer {
                port: offer.port.into(),
                stream_id: offer.stream_id.to_string(),
            });
        }
        Some(RouteDataOffer::NativeAudio(offer)) => {
            offer.validate(route)?;
            status.native_audio_data = Some(offer.to_wire());
        }
        Some(RouteDataOffer::Haptics(offer)) => {
            offer.validate(route)?;
            status.haptics_data = Some(v1::HapticsDataOffer {
                port: offer.port.into(),
                stream_id: offer.stream_id.to_string(),
            });
        }
        Some(RouteDataOffer::Camera(offer)) => {
            offer.validate(route)?;
            status.camera_data = Some(v1::CameraDataOffer {
                port: offer.port.into(),
                stream_id: offer.stream_id.to_string(),
            });
        }
        Some(RouteDataOffer::SecureCamera(offer)) => {
            offer.validate(route)?;
            status.secure_camera_data = Some(offer.to_wire());
        }
        Some(RouteDataOffer::SecureAudio(offer)) => {
            offer.validate(route)?;
            status.secure_audio_data = Some(offer.to_wire());
        }
        None => {}
    }
    encode_envelope_checked(&new_envelope(
        Some(session_id),
        v1::envelope::Payload::RouteStatus(Box::new(status)),
    ))
}

pub fn decode_route_status(bytes: &[u8]) -> Result<ValidatedRouteStatus, ProtocolError> {
    let envelope = decode_envelope(bytes)?;
    let (message_id, session_id) = parse_bound_envelope_ids(&envelope)?;
    let status = match envelope.payload {
        Some(v1::envelope::Payload::RouteStatus(status)) => status,
        _ => {
            return Err(ProtocolError::UnexpectedPayload {
                expected: "route_status",
            });
        }
    };
    let route: capyio_core::Route = status
        .route
        .ok_or(ProtocolError::MissingField("route_status.route"))?
        .try_into()?;
    if route.session_id != session_id {
        return Err(ProtocolError::InvalidRouteControl(
            "Route belongs to another Session",
        ));
    }
    if [
        status.touchpad_data.is_some(),
        status.native_audio_data.is_some(),
        status.haptics_data.is_some(),
        status.camera_data.is_some(),
        status.secure_camera_data.is_some(),
        status.secure_audio_data.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count()
        > 1
    {
        return Err(ProtocolError::InvalidRouteControl(
            "conflicting data offers",
        ));
    }
    let native_audio_data = status
        .native_audio_data
        .map(|offer| crate::NativeAudioDataOffer::from_wire(offer, &route))
        .transpose()?;
    let secure_audio_data = status
        .secure_audio_data
        .map(|wire| crate::SecureAudioDataOffer::from_wire(wire, &route))
        .transpose()?;
    let secure_camera_data = status
        .secure_camera_data
        .map(|wire| crate::SecureCameraDataOffer::from_wire(wire, &route))
        .transpose()?;
    let camera_data = status
        .camera_data
        .map(|wire| {
            let offer = crate::CameraDataOffer {
                port: wire
                    .port
                    .try_into()
                    .map_err(|_| ProtocolError::InvalidRouteControl("invalid camera port"))?,
                stream_id: wire
                    .stream_id
                    .parse()
                    .map_err(|_| ProtocolError::InvalidRouteControl("invalid camera Stream ID"))?,
            };
            offer.validate(&route)?;
            Ok::<_, ProtocolError>(offer)
        })
        .transpose()?;
    let haptics_data = status
        .haptics_data
        .map(|offer| {
            let offer = HapticsDataOffer {
                port: u16::try_from(offer.port)
                    .map_err(|_| ProtocolError::InvalidRouteControl("invalid haptics port"))?,
                stream_id: capyio_core::StreamId::from_str(&offer.stream_id)
                    .map_err(|_| ProtocolError::InvalidRouteControl("invalid haptics Stream ID"))?,
            };
            offer.validate(&route)?;
            Ok::<_, ProtocolError>(offer)
        })
        .transpose()?;
    let touchpad_data = status
        .touchpad_data
        .map(|offer| {
            let offer = TouchpadDataOffer {
                port: u16::try_from(offer.port).map_err(|_| {
                    ProtocolError::InvalidRouteControl("touchpad data port exceeds 65535")
                })?,
                stream_id: capyio_core::StreamId::from_str(&offer.stream_id).map_err(|_| {
                    ProtocolError::InvalidRouteControl("invalid touchpad Stream ID")
                })?,
            };
            offer.validate(&route)?;
            Ok::<_, ProtocolError>(offer)
        })
        .transpose()?;
    Ok(ValidatedRouteStatus {
        message_id,
        session_id,
        route,
        touchpad_data,
        haptics_data,
        native_audio_data: native_audio_data.map(Box::new),
        camera_data,
        secure_camera_data,
        secure_audio_data,
    })
}

pub fn decode_port_lease_request(bytes: &[u8]) -> Result<ValidatedPortLeaseRequest, ProtocolError> {
    let envelope = decode_envelope(bytes)?;
    let (message_id, session_id) = parse_bound_envelope_ids(&envelope)?;
    let request = match envelope.payload {
        Some(v1::envelope::Payload::PortLeaseRequest(request)) => request,
        _ => {
            return Err(ProtocolError::UnexpectedPayload {
                expected: "port_lease_request",
            });
        }
    };
    let route_id = parse_id("port_lease_request.route_id", &request.route_id)?;
    let port = request
        .port
        .ok_or(ProtocolError::MissingField("port_lease_request.port"))?
        .try_into()?;
    let action = lease_action_from_wire(request.action)?;
    validate_lease_ttl(action, request.requested_ttl_ms)?;
    Ok(ValidatedPortLeaseRequest {
        message_id,
        session_id,
        route_id,
        port,
        action,
        requested_ttl_ms: request.requested_ttl_ms,
    })
}

pub fn encode_port_lease_response(
    session_id: SessionId,
    request_message_id: MessageId,
    route_id: RouteId,
    port: PortRef,
    outcome: PortLeaseOutcome,
    expires_at_ms: Option<u64>,
) -> Result<Vec<u8>, ProtocolError> {
    validate_lease_response(outcome, expires_at_ms)?;
    let envelope = new_envelope(
        Some(session_id),
        v1::envelope::Payload::PortLeaseResponse(v1::PortLeaseResponse {
            request_message_id: request_message_id.to_string(),
            route_id: route_id.to_string(),
            port: Some((&port).into()),
            outcome: lease_outcome_to_wire(outcome),
            expires_at_ms,
        }),
    );
    encode_envelope_checked(&envelope)
}

pub fn decode_port_lease_response(
    bytes: &[u8],
) -> Result<ValidatedPortLeaseResponse, ProtocolError> {
    let envelope = decode_envelope(bytes)?;
    let (message_id, session_id) = parse_bound_envelope_ids(&envelope)?;
    let response = match envelope.payload {
        Some(v1::envelope::Payload::PortLeaseResponse(response)) => response,
        _ => {
            return Err(ProtocolError::UnexpectedPayload {
                expected: "port_lease_response",
            });
        }
    };
    let request_message_id = parse_id(
        "port_lease_response.request_message_id",
        &response.request_message_id,
    )?;
    let route_id = parse_id("port_lease_response.route_id", &response.route_id)?;
    let port = response
        .port
        .ok_or(ProtocolError::MissingField("port_lease_response.port"))?
        .try_into()?;
    let outcome = lease_outcome_from_wire(response.outcome)?;
    validate_lease_response(outcome, response.expires_at_ms)?;
    Ok(ValidatedPortLeaseResponse {
        message_id,
        session_id,
        request_message_id,
        route_id,
        port,
        outcome,
        expires_at_ms: response.expires_at_ms,
    })
}

fn parse_bound_envelope_ids(
    envelope: &v1::Envelope,
) -> Result<(MessageId, SessionId), ProtocolError> {
    let message_id = parse_id("envelope.message_id", &envelope.message_id)?;
    if envelope.session_id.is_empty() {
        return Err(ProtocolError::MissingField("envelope.session_id"));
    }
    let session_id = parse_id("envelope.session_id", &envelope.session_id)?;
    Ok((message_id, session_id))
}

fn parse_id<T: FromStr>(field: &'static str, value: &str) -> Result<T, ProtocolError> {
    T::from_str(value).map_err(|_| ProtocolError::InvalidId {
        field,
        value: value.to_owned(),
    })
}

fn validate_lease_ttl(action: PortLeaseAction, ttl_ms: u64) -> Result<(), ProtocolError> {
    match action {
        PortLeaseAction::Release if ttl_ms == 0 => Ok(()),
        PortLeaseAction::Release => {
            Err(ProtocolError::InvalidPortLease("release TTL must be zero"))
        }
        PortLeaseAction::Acquire | PortLeaseAction::Renew
            if (MIN_PORT_LEASE_TTL_MS..=MAX_PORT_LEASE_TTL_MS).contains(&ttl_ms) =>
        {
            Ok(())
        }
        PortLeaseAction::Acquire | PortLeaseAction::Renew => Err(ProtocolError::InvalidPortLease(
            "acquire/renew TTL is outside the supported range",
        )),
    }
}

fn validate_lease_response(
    outcome: PortLeaseOutcome,
    expires_at_ms: Option<u64>,
) -> Result<(), ProtocolError> {
    let requires_expiry = matches!(
        outcome,
        PortLeaseOutcome::Acquired | PortLeaseOutcome::Renewed
    );
    if requires_expiry == expires_at_ms.is_some() {
        Ok(())
    } else {
        Err(ProtocolError::InvalidPortLease(
            "response expiry does not match outcome",
        ))
    }
}

fn lease_action_to_wire(action: PortLeaseAction) -> i32 {
    match action {
        PortLeaseAction::Acquire => v1::PortLeaseAction::Acquire as i32,
        PortLeaseAction::Renew => v1::PortLeaseAction::Renew as i32,
        PortLeaseAction::Release => v1::PortLeaseAction::Release as i32,
    }
}

fn lease_action_from_wire(value: i32) -> Result<PortLeaseAction, ProtocolError> {
    match v1::PortLeaseAction::try_from(value) {
        Ok(v1::PortLeaseAction::Acquire) => Ok(PortLeaseAction::Acquire),
        Ok(v1::PortLeaseAction::Renew) => Ok(PortLeaseAction::Renew),
        Ok(v1::PortLeaseAction::Release) => Ok(PortLeaseAction::Release),
        _ => Err(ProtocolError::InvalidEnum {
            field: "port_lease_request.action",
            value,
        }),
    }
}

fn lease_outcome_to_wire(outcome: PortLeaseOutcome) -> i32 {
    match outcome {
        PortLeaseOutcome::Acquired => v1::PortLeaseOutcome::Acquired as i32,
        PortLeaseOutcome::Renewed => v1::PortLeaseOutcome::Renewed as i32,
        PortLeaseOutcome::Released => v1::PortLeaseOutcome::Released as i32,
        PortLeaseOutcome::Conflict => v1::PortLeaseOutcome::Conflict as i32,
        PortLeaseOutcome::Denied => v1::PortLeaseOutcome::Denied as i32,
    }
}

fn lease_outcome_from_wire(value: i32) -> Result<PortLeaseOutcome, ProtocolError> {
    match v1::PortLeaseOutcome::try_from(value) {
        Ok(v1::PortLeaseOutcome::Acquired) => Ok(PortLeaseOutcome::Acquired),
        Ok(v1::PortLeaseOutcome::Renewed) => Ok(PortLeaseOutcome::Renewed),
        Ok(v1::PortLeaseOutcome::Released) => Ok(PortLeaseOutcome::Released),
        Ok(v1::PortLeaseOutcome::Conflict) => Ok(PortLeaseOutcome::Conflict),
        Ok(v1::PortLeaseOutcome::Denied) => Ok(PortLeaseOutcome::Denied),
        _ => Err(ProtocolError::InvalidEnum {
            field: "port_lease_response.outcome",
            value,
        }),
    }
}

fn validate_catalog_bounds(node: &v1::NodeDescriptor) -> Result<(), ProtocolError> {
    check_count(
        "node.protocol_versions",
        node.protocol_versions.len(),
        MAX_CATALOG_PROTOCOL_VERSIONS,
    )?;
    check_count(
        "node.adapter_instances",
        node.adapter_instances.len(),
        MAX_CATALOG_ADAPTERS,
    )?;
    check_count(
        "node.capabilities",
        node.capabilities.len(),
        MAX_CATALOG_CAPABILITIES,
    )?;
    check_string("node.id", &node.id)?;
    check_string("node.display_name", &node.display_name)?;
    check_string("node.platform_detail", &node.platform_detail)?;
    check_string("node.platform_version", &node.platform_version)?;
    check_string("node.runtime_version", &node.runtime_version)?;

    for adapter in &node.adapter_instances {
        check_count(
            "adapter.owned_capability_ids",
            adapter.owned_capability_ids.len(),
            MAX_CATALOG_CAPABILITIES,
        )?;
        check_count(
            "adapter.supported_route_modes",
            adapter.supported_route_modes.len(),
            MAX_CATALOG_ENUM_LIST_ENTRIES,
        )?;
        check_string("adapter.id", &adapter.id)?;
        check_string("adapter.adapter_type", &adapter.adapter_type)?;
        check_string("adapter.display_name", &adapter.display_name)?;
        check_string("adapter.version", &adapter.version)?;
        for capability_id in &adapter.owned_capability_ids {
            check_string("adapter.owned_capability_ids", capability_id)?;
        }
    }

    for capability in &node.capabilities {
        check_count(
            "capability.metadata",
            capability.metadata.len(),
            MAX_CATALOG_METADATA_ENTRIES,
        )?;
        check_count(
            "capability.ports",
            capability.ports.len(),
            MAX_CATALOG_PORTS_PER_CAPABILITY,
        )?;
        check_string("capability.id", &capability.id)?;
        check_string("capability.display_name", &capability.display_name)?;
        check_string(
            "capability.adapter_instance_id",
            &capability.adapter_instance_id,
        )?;
        check_string("capability.custom_class", &capability.custom_class)?;
        for (key, value) in &capability.metadata {
            check_string("capability.metadata key", key)?;
            check_string("capability.metadata value", value)?;
        }
        for port in &capability.ports {
            check_count(
                "port.formats",
                port.formats.len(),
                MAX_CATALOG_FORMATS_PER_PORT,
            )?;
            check_count(
                "port.qos_modes",
                port.qos_modes.len(),
                MAX_CATALOG_ENUM_LIST_ENTRIES,
            )?;
            check_string("port.id", &port.id)?;
            check_string("port.capability_id", &port.capability_id)?;
            check_string("port.display_name", &port.display_name)?;
            if let Some(profile) = &port.profile {
                check_string("port.profile.name", &profile.name)?;
            }
            if let Some(value) = &port.schema_id {
                check_string("port.schema_id", value)?;
            }
            if let Some(value) = &port.clock_domain {
                check_string("port.clock_domain", value)?;
            }
            for format in &port.formats {
                check_string("format.id", &format.id)?;
                check_count(
                    "format.parameters",
                    format.parameters.len(),
                    MAX_CATALOG_METADATA_ENTRIES,
                )?;
                for (key, value) in &format.parameters {
                    check_string("format.parameters key", key)?;
                    check_string("format.parameters value", value)?;
                }
            }
        }
    }
    Ok(())
}

fn check_count(field: &'static str, actual: usize, limit: usize) -> Result<(), ProtocolError> {
    if actual > limit {
        return Err(ProtocolError::CatalogCountLimit {
            field,
            limit,
            actual,
        });
    }
    Ok(())
}

fn check_string(field: &'static str, value: &str) -> Result<(), ProtocolError> {
    let actual = value.len();
    if actual > MAX_CATALOG_STRING_BYTES {
        return Err(ProtocolError::CatalogStringLimit {
            field,
            limit: MAX_CATALOG_STRING_BYTES,
            actual,
        });
    }
    Ok(())
}

/// Validates version, typed identifiers and the required payload.
pub fn validate_envelope(envelope: &v1::Envelope) -> Result<(), ProtocolError> {
    validate_envelope_version(envelope)?;
    MessageId::from_str(&envelope.message_id).map_err(|_| ProtocolError::InvalidId {
        field: "envelope.message_id",
        value: envelope.message_id.clone(),
    })?;
    if !envelope.session_id.is_empty() {
        SessionId::from_str(&envelope.session_id).map_err(|_| ProtocolError::InvalidId {
            field: "envelope.session_id",
            value: envelope.session_id.clone(),
        })?;
    }
    if envelope.payload.is_none() {
        return Err(ProtocolError::MissingField("envelope.payload"));
    }
    if let Some(v1::envelope::Payload::DisplaySessionControl(value)) = &envelope.payload {
        if envelope.session_id.is_empty() {
            return Err(ProtocolError::MissingField("envelope.session_id"));
        }
        crate::validate_display_session_control(value)?;
    }
    if let Some(v1::envelope::Payload::CatalogSnapshot(snapshot)) = &envelope.payload
        && let Some(node) = &snapshot.node
    {
        validate_catalog_bounds(node)?;
    }
    Ok(())
}

/// Rejects unsupported major protocol versions. Newer minor versions remain append-compatible.
pub fn validate_envelope_version(envelope: &v1::Envelope) -> Result<(), ProtocolError> {
    if envelope.protocol_major != PROTOCOL_MAJOR {
        return Err(ProtocolError::UnsupportedProtocolMajor {
            expected: PROTOCOL_MAJOR,
            actual: envelope.protocol_major,
        });
    }
    Ok(())
}

/// Creates a v1 envelope with a fresh typed message identifier.
#[must_use]
pub fn new_envelope(session_id: Option<SessionId>, payload: v1::envelope::Payload) -> v1::Envelope {
    v1::Envelope {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
        message_id: MessageId::new().to_string(),
        session_id: session_id.map_or_else(String::new, |id| id.to_string()),
        payload: Some(payload),
    }
}

#[cfg(test)]
mod tests {
    use capyio_core::{CapabilityId, NodeId, PortId, RouteId};
    use capyio_testkit::DemoLab;

    use super::*;

    fn error_payload() -> v1::envelope::Payload {
        v1::envelope::Payload::Error(v1::ErrorMessage {
            code: "test".to_owned(),
            category: "protocol".to_owned(),
            retryable: false,
            detail: String::new(),
            related_id: String::new(),
        })
    }

    #[test]
    fn missing_payload_is_rejected() {
        let mut envelope = new_envelope(None, error_payload());
        envelope.payload = None;
        assert!(matches!(
            validate_envelope(&envelope),
            Err(ProtocolError::MissingField("envelope.payload"))
        ));
    }

    #[test]
    fn invalid_message_id_is_rejected() {
        let mut envelope = new_envelope(None, error_payload());
        envelope.message_id = "not-a-uuid".to_owned();
        assert!(matches!(
            validate_envelope(&envelope),
            Err(ProtocolError::InvalidId {
                field: "envelope.message_id",
                ..
            })
        ));
    }

    #[test]
    fn oversized_input_is_rejected_before_decode() {
        let bytes = vec![0; MAX_CONTROL_ENVELOPE_BYTES + 1];
        assert!(matches!(
            decode_envelope(&bytes),
            Err(ProtocolError::MessageTooLarge { .. })
        ));
    }

    #[test]
    fn lease_request_and_response_round_trip_with_exact_correlation() {
        let session_id = SessionId::new();
        let route_id = RouteId::new();
        let port = PortRef {
            node_id: NodeId::new(),
            capability_id: CapabilityId::new(),
            port_id: PortId::new(),
        };
        let request_bytes = encode_port_lease_request(
            session_id,
            route_id,
            port,
            PortLeaseAction::Acquire,
            MIN_PORT_LEASE_TTL_MS,
        )
        .expect("encode request");
        let request = decode_port_lease_request(&request_bytes).expect("decode request");
        assert_eq!(request.session_id, session_id);
        assert_eq!(request.route_id, route_id);
        assert_eq!(request.port, port);

        let response_bytes = encode_port_lease_response(
            session_id,
            request.message_id,
            route_id,
            port,
            PortLeaseOutcome::Acquired,
            Some(10_000),
        )
        .expect("encode response");
        let response = decode_port_lease_response(&response_bytes).expect("decode response");
        assert_eq!(response.request_message_id, request.message_id);
        assert_eq!(response.outcome, PortLeaseOutcome::Acquired);
        assert_eq!(response.expires_at_ms, Some(10_000));
    }

    #[test]
    fn route_status_and_command_round_trip_with_session_binding() {
        let lab = DemoLab::new().expect("demo Runtime");
        let route_id = lab.routes.phone_microphone_to_windows;
        let route = lab.runtime.route(route_id).expect("demo Route");
        let status = decode_route_status(
            &encode_route_status(lab.session_id, route).expect("encode Route status"),
        )
        .expect("decode Route status");
        assert_eq!(status.session_id, lab.session_id);
        assert_eq!(status.route, route.clone());

        let command = decode_route_command(
            &encode_route_command(lab.session_id, route_id, RouteCommandAction::Start)
                .expect("encode Route command"),
        )
        .expect("decode Route command");
        assert_eq!(command.session_id, lab.session_id);
        assert_eq!(command.route_id, route_id);
        assert_eq!(command.action, RouteCommandAction::Start);
    }

    #[test]
    fn touchpad_offer_is_only_endpoint_metadata_for_a_finitely_authorized_route() {
        use capyio_core::{AuthorizationState, ProfileId, RouteBackend, RouteState, StreamId};
        let lab = DemoLab::new().unwrap();
        let mut route = lab
            .runtime
            .route(lab.routes.phone_microphone_to_windows)
            .unwrap()
            .clone();
        route.profile = ProfileId::touchpad_frames_v1();
        route.backend = RouteBackend::AdapterManaged;
        route.state = RouteState::Starting;
        route.epoch = 1;
        route.authorization = AuthorizationState::Authorized {
            expires_at_ms: Some(60_000),
        };
        let offer = TouchpadDataOffer {
            port: 49152,
            stream_id: StreamId::new(),
        };
        let bytes =
            encode_route_status_with_touchpad_data(lab.session_id, &route, Some(offer)).unwrap();
        assert_eq!(
            decode_route_status(&bytes).unwrap().touchpad_data,
            Some(offer)
        );
        let mut wire = decode_envelope(&bytes).unwrap();
        if let Some(v1::envelope::Payload::RouteStatus(status)) = &mut wire.payload {
            status.touchpad_data.as_mut().unwrap().port = 65_536;
        }
        assert!(decode_route_status(&encode_envelope(&wire)).is_err());
        route.authorization = AuthorizationState::Pending;
        assert!(
            encode_route_status_with_touchpad_data(lab.session_id, &route, Some(offer)).is_err()
        );
        route.authorization = AuthorizationState::Authorized {
            expires_at_ms: None,
        };
        assert!(
            encode_route_status_with_touchpad_data(lab.session_id, &route, Some(offer)).is_err()
        );
        assert_eq!(
            decode_route_status(&encode_route_status(lab.session_id, &route).unwrap())
                .unwrap()
                .touchpad_data,
            None
        );
    }

    #[test]
    fn lease_ttl_and_response_shape_fail_closed() {
        let session_id = SessionId::new();
        let route_id = RouteId::new();
        let port = PortRef {
            node_id: NodeId::new(),
            capability_id: CapabilityId::new(),
            port_id: PortId::new(),
        };
        assert!(matches!(
            encode_port_lease_request(
                session_id,
                route_id,
                port,
                PortLeaseAction::Acquire,
                MAX_PORT_LEASE_TTL_MS + 1,
            ),
            Err(ProtocolError::InvalidPortLease(_))
        ));
        assert!(matches!(
            encode_port_lease_request(
                session_id,
                route_id,
                port,
                PortLeaseAction::Release,
                MIN_PORT_LEASE_TTL_MS,
            ),
            Err(ProtocolError::InvalidPortLease(_))
        ));
        assert!(matches!(
            encode_port_lease_response(
                session_id,
                MessageId::new(),
                route_id,
                port,
                PortLeaseOutcome::Denied,
                Some(10_000),
            ),
            Err(ProtocolError::InvalidPortLease(_))
        ));
    }
}
