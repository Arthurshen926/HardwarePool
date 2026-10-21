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
            || self.stream_id.as_uuid().is_nil()
            || route.epoch == 0
            || route.epoch > i32::MAX as u64
            || route.source.node_id == route.sink.node_id
            || route.profile != ProfileId::video_frames_v1()
            || route.backend != RouteBackend::AdapterManaged
            || route.selected_format != Some(FormatDescriptor::new("cavc-avc-access-unit-v1"))
            || !matches!(route.state, RouteState::Starting | RouteState::Active)
            || !matches!(route.authorization, AuthorizationState::Authorized { expires_at_ms: Some(expiry) } if (1..=9_007_199_254_740_991).contains(&expiry))
        {
            return Err(ProtocolError::InvalidRouteControl(
                "invalid controlled-lab camera offer",
            ));
        }
        Ok(())
    }
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
        // Share only the canonical Route constraints, not the legacy transport.
        CameraDataOffer {
            port: 38173,
            stream_id: self.stream_id,
        }
        .validate(route)
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
