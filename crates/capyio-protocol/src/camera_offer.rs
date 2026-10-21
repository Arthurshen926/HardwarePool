use crate::ProtocolError;
use capyio_core::{
    AuthorizationState, FormatDescriptor, ProfileId, Route, RouteBackend, RouteState, StreamId,
};

/// Plain CAVC controlled-lab endpoint, not an authenticated media transport.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CameraDataOffer {
    pub port: u16,
    pub stream_id: StreamId,
}
impl CameraDataOffer {
    pub fn validate(self, route: &Route) -> Result<(), ProtocolError> {
        if self.port != 38173
            || route.selected_format != Some(FormatDescriptor::new("cavc-avc-access-unit-v1"))
            || !valid_camera_route(route, self.stream_id)
        {
            return Err(ProtocolError::InvalidRouteControl(
                "invalid controlled-lab camera offer",
            ));
        }
        Ok(())
    }
}

// Shared identity/authority constraints do not widen either transport's exact
// format allowlist. In particular, finite streams never use the plaintext offer.
fn valid_camera_route(route: &Route, stream_id: StreamId) -> bool {
    !stream_id.as_uuid().is_nil()
        && route.epoch > 0
        && route.epoch <= i32::MAX as u64
        && route.source.node_id != route.sink.node_id
        && route.profile == ProfileId::video_frames_v1()
        && route.backend == RouteBackend::AdapterManaged
        && matches!(route.state, RouteState::Starting | RouteState::Active)
        && matches!(route.authorization, AuthorizationState::Authorized { expires_at_ms: Some(expiry) } if (1..=9_007_199_254_740_991).contains(&expiry))
}

/// Explicit versioned pinned mutual-TLS camera transport. Never plain CAVC.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecureCameraTransport {
    Tls13MtlsV1,
}

/// Receiver's actual listening port; its IP comes from the authenticated control socket.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SecureCameraDataOffer {
    pub port: u16,
    pub stream_id: StreamId,
    pub transport: SecureCameraTransport,
}

impl SecureCameraDataOffer {
    pub fn validate(self, route: &Route) -> Result<(), ProtocolError> {
        if self.port == 0 {
            return Err(ProtocolError::InvalidRouteControl(
                "invalid secure camera port",
            ));
        }
        // Each format is an exact, empty-parameter opt-in. A secure offer may
        // select either contract, but it never silently upgrades a legacy Route.
        let legacy = FormatDescriptor::new("cavc-avc-access-unit-v1");
        let finite = FormatDescriptor::new("cavc-avc-finite-720p30-v1");
        if !valid_camera_route(route, self.stream_id)
            || !matches!(route.selected_format.as_ref(), Some(format) if format == &legacy || format == &finite)
        {
            return Err(ProtocolError::InvalidRouteControl(
                "invalid secure camera Route or exact format",
            ));
        }
        Ok(())
    }

    pub(crate) fn to_wire(self) -> crate::v1::SecureCameraDataOffer {
        crate::v1::SecureCameraDataOffer {
            port: self.port.into(),
            stream_id: self.stream_id.to_string(),
            transport: match self.transport {
                SecureCameraTransport::Tls13MtlsV1 => 1,
            },
        }
    }

    pub(crate) fn from_wire(
        wire: crate::v1::SecureCameraDataOffer,
        route: &Route,
    ) -> Result<Self, ProtocolError> {
        let transport = match wire.transport {
            1 => SecureCameraTransport::Tls13MtlsV1,
            _ => {
                return Err(ProtocolError::InvalidRouteControl(
                    "unsupported secure camera transport version",
                ));
            }
        };
        let offer = Self {
            port: wire
                .port
                .try_into()
                .map_err(|_| ProtocolError::InvalidRouteControl("invalid secure camera port"))?,
            stream_id: wire.stream_id.parse().map_err(|_| {
                ProtocolError::InvalidRouteControl("invalid secure camera Stream ID")
            })?,
            transport,
        };
        offer.validate(route)?;
        Ok(offer)
    }
}
