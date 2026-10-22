use capyio_core::{
    AuthorizationState, FormatDescriptor, ProfileId, QosMode, Route, RouteBackend, RouteState,
    StreamId,
};

use crate::ProtocolError;

/// Explicit opt-in to mono PCM over the dedicated pinned TLS 1.3 channel.
/// Empty parameters are part of this closed first-version contract.
pub const SECURE_AUDIO_MONO_FORMAT: &str = "pcm-s16le-48000-mono-tls13-v1";

/// Separately negotiated finite five-minute profile. It does not alter v1 limits.
pub const SECURE_AUDIO_MONO_300S_FORMAT: &str = "pcm-s16le-48000-mono-tls13-300s-v1";

/// Separate finite 60-second stereo contract. Never interpreted by the mono
/// session-profile enum; empty parameters and transport value 2 are mandatory.
pub const SECURE_AUDIO_STEREO_FORMAT: &str = "pcm-s16le-48000-stereo-tls13-v1";

/// Immutable authenticated mono Route selection, never inferred from lease duration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecureAudioSessionProfile {
    Finite60sV1,
    Finite300sV1,
}
impl SecureAudioSessionProfile {
    pub const fn format_name(self) -> &'static str {
        match self {
            Self::Finite60sV1 => SECURE_AUDIO_MONO_FORMAT,
            Self::Finite300sV1 => SECURE_AUDIO_MONO_300S_FORMAT,
        }
    }
    pub fn descriptor(self) -> FormatDescriptor {
        FormatDescriptor::new(self.format_name())
    }
    pub fn from_descriptor(format: &FormatDescriptor) -> Option<Self> {
        if !format.parameters.is_empty() {
            return None;
        }
        match format.id.as_str() {
            SECURE_AUDIO_MONO_FORMAT => Some(Self::Finite60sV1),
            SECURE_AUDIO_MONO_300S_FORMAT => Some(Self::Finite300sV1),
            _ => None,
        }
    }
    pub const fn max_duration(self) -> std::time::Duration {
        std::time::Duration::from_secs(match self {
            Self::Finite60sV1 => 60,
            Self::Finite300sV1 => 300,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecureAudioTransport {
    /// Exact mono framing, with either explicitly selected mono session profile.
    Tls13MtlsV1,
    /// Separate ALPN/binding and two-fragment stereo record; finite 60 seconds.
    Tls13MtlsStereoV1,
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
            || !route
                .selected_format
                .as_ref()
                .is_some_and(|format| match self.transport {
                    SecureAudioTransport::Tls13MtlsV1 => {
                        SecureAudioSessionProfile::from_descriptor(format).is_some()
                    }
                    SecureAudioTransport::Tls13MtlsStereoV1 => {
                        format.id == SECURE_AUDIO_STEREO_FORMAT && format.parameters.is_empty()
                    }
                })
            || route.selected_qos_mode != Some(QosMode::Interactive)
            || !matches!(route.state, RouteState::Starting | RouteState::Active)
            || !matches!(route.authorization, AuthorizationState::Authorized {
                expires_at_ms: Some(expiry) } if (1..=9_007_199_254_740_991).contains(&expiry))
        {
            return Err(ProtocolError::InvalidRouteControl(
                "invalid secure audio transport, Route, format, QoS or endpoint",
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
                SecureAudioTransport::Tls13MtlsStereoV1 => 2,
            },
        }
    }

    pub(crate) fn from_wire(
        wire: crate::v1::SecureAudioDataOffer,
        route: &Route,
    ) -> Result<Self, ProtocolError> {
        let transport = match wire.transport {
            1 => SecureAudioTransport::Tls13MtlsV1,
            2 => SecureAudioTransport::Tls13MtlsStereoV1,
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
