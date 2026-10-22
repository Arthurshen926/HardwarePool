use capyio_core::{
    AuthorizationState, FormatDescriptor, ProfileId, QosMode, Route, RouteBackend, RouteState,
    StreamId,
};

use crate::ProtocolError;

/// Explicit opt-in to mono PCM over the dedicated pinned TLS 1.3 channel.
/// Empty parameters are part of this closed first-version contract.
pub const SECURE_AUDIO_MONO_FORMAT: &str = "pcm-s16le-48000-mono-tls13-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecureAudioTransport {
    Tls13MtlsV1,
}

/// The Sink's actual listening port. Peer IP is derived only from the
/// authenticated control connection, never from caller-supplied offer metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SecureAudioDataOffer {
    pub port: u16,
    pub stream_id: StreamId,
    pub transport: SecureAudioTransport,
}

impl SecureAudioDataOffer {
    pub fn validate(self, route: &Route) -> Result<(), ProtocolError> {
        if self.port == 0
            || self.stream_id.as_uuid().is_nil()
            || route.epoch == 0
            || route.epoch > i32::MAX as u64
            || route.source.node_id == route.sink.node_id
            || route.profile != ProfileId::audio_frames_v1()
            || route.backend != RouteBackend::AdapterManaged
            || route.selected_format != Some(FormatDescriptor::new(SECURE_AUDIO_MONO_FORMAT))
            || route.selected_qos_mode != Some(QosMode::Interactive)
            || !matches!(route.state, RouteState::Starting | RouteState::Active)
            || !matches!(route.authorization, AuthorizationState::Authorized {
                expires_at_ms: Some(expiry) } if (1..=9_007_199_254_740_991).contains(&expiry))
        {
            return Err(ProtocolError::InvalidRouteControl(
                "invalid secure mono audio Route, format, QoS or endpoint",
            ));
        }
        Ok(())
    }

    pub(crate) fn to_wire(self) -> crate::v1::SecureAudioDataOffer {
        crate::v1::SecureAudioDataOffer {
            port: self.port.into(),
            stream_id: self.stream_id.to_string(),
            transport: match self.transport {
                SecureAudioTransport::Tls13MtlsV1 => 1,
            },
        }
    }

    pub(crate) fn from_wire(
        wire: crate::v1::SecureAudioDataOffer,
        route: &Route,
    ) -> Result<Self, ProtocolError> {
        let transport = match wire.transport {
            1 => SecureAudioTransport::Tls13MtlsV1,
            _ => {
                return Err(ProtocolError::InvalidRouteControl(
                    "unsupported secure audio transport",
                ));
            }
        };
        let offer = Self {
            port: wire
                .port
                .try_into()
                .map_err(|_| ProtocolError::InvalidRouteControl("invalid secure audio port"))?,
            stream_id: wire.stream_id.parse().map_err(|_| {
                ProtocolError::InvalidRouteControl("invalid secure audio Stream ID")
            })?,
            transport,
        };
        offer.validate(route)?;
        Ok(offer)
    }
}
