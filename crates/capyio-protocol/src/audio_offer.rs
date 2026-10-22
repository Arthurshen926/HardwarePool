/// Capability metadata advertises implemented, finite-consent incoming Draft
/// admission for the native mono PCM contract. It is not authorization, trust,
/// encryption, a reverse-audio promise, or generic Profile interoperability.
pub const NATIVE_AUDIO_INBOUND_DRAFT_METADATA_KEY: &str = "dev.capyio.native-audio.inbound-draft";
pub const NATIVE_AUDIO_INBOUND_DRAFT_MONO_V1: &str = "pcm-s16le-48000-mono-v1";

use capyio_core::{
    AuthorizationState, NodeId, ProfileId, Route, RouteBackend, RouteState, StreamId,
};

use crate::{ProtocolError, v1};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeAudioEndpoint {
    Source,
    Sink,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeAudioPreset {
    PcmS16le48000Mono,
    PcmS16le48000Stereo,
}

impl NativeAudioPreset {
    pub const fn format_name(self) -> &'static str {
        match self {
            Self::PcmS16le48000Mono => "pcm-s16le-48000-mono",
            Self::PcmS16le48000Stereo => "pcm-s16le-48000-stereo",
        }
    }

    pub const fn channels(self) -> u8 {
        match self {
            Self::PcmS16le48000Mono => 1,
            Self::PcmS16le48000Stereo => 2,
        }
    }
}

/// Only the current explicitly insecure laboratory PCM contract is supported.
/// Receiving this value grants neither permission nor transport security.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeAudioDataOffer {
    pub port: u16,
    pub stream_id: StreamId,
    pub endpoint: NativeAudioEndpoint,
    pub preset: NativeAudioPreset,
}

impl NativeAudioDataOffer {
    pub fn owner(self, route: &Route) -> NodeId {
        match self.endpoint {
            NativeAudioEndpoint::Source => route.source.node_id,
            NativeAudioEndpoint::Sink => route.sink.node_id,
        }
    }

    pub(crate) fn validate(self, route: &Route) -> Result<(), ProtocolError> {
        if route.selected_format.as_ref().is_some_and(|format| {
            matches!(
                format.id.as_str(),
                crate::SECURE_AUDIO_MONO_FORMAT
                    | crate::SECURE_AUDIO_MONO_300S_FORMAT
                    | crate::SECURE_AUDIO_STEREO_FORMAT
            )
        }) || self.port == 0
            || self.stream_id.as_uuid().is_nil()
            || route.epoch == 0
            || route.epoch > i32::MAX as u64
            || route.source.node_id == route.sink.node_id
            || route.profile != ProfileId::audio_frames_v1()
            || route.backend != RouteBackend::AdapterManaged
            || !matches!(route.state, RouteState::Starting | RouteState::Active)
            || !matches!(route.authorization, AuthorizationState::Authorized {
                expires_at_ms: Some(expiry) } if (1..=9_007_199_254_740_991).contains(&expiry))
        {
            return Err(ProtocolError::InvalidRouteControl(
                "invalid native audio data offer",
            ));
        }
        Ok(())
    }

    pub(crate) fn to_wire(self) -> v1::NativeAudioDataOffer {
        v1::NativeAudioDataOffer {
            port: self.port.into(),
            stream_id: self.stream_id.to_string(),
            endpoint: match self.endpoint {
                NativeAudioEndpoint::Source => 1,
                NativeAudioEndpoint::Sink => 2,
            },
            preset: match self.preset {
                NativeAudioPreset::PcmS16le48000Mono => 1,
                NativeAudioPreset::PcmS16le48000Stereo => 2,
            },
            transport: 1,
        }
    }

    pub(crate) fn from_wire(
        value: v1::NativeAudioDataOffer,
        route: &Route,
    ) -> Result<Self, ProtocolError> {
        if value.transport != 1 {
            return Err(ProtocolError::InvalidRouteControl(
                "unsupported native audio preset or transport",
            ));
        }
        let offer = Self {
            preset: match value.preset {
                1 => NativeAudioPreset::PcmS16le48000Mono,
                2 => NativeAudioPreset::PcmS16le48000Stereo,
                _ => {
                    return Err(ProtocolError::InvalidRouteControl(
                        "unsupported native audio preset",
                    ));
                }
            },
            port: value
                .port
                .try_into()
                .map_err(|_| ProtocolError::InvalidRouteControl("invalid audio port"))?,
            stream_id: value
                .stream_id
                .parse()
                .map_err(|_| ProtocolError::InvalidRouteControl("invalid audio Stream ID"))?,
            endpoint: match value.endpoint {
                1 => NativeAudioEndpoint::Source,
                2 => NativeAudioEndpoint::Sink,
                _ => {
                    return Err(ProtocolError::InvalidRouteControl(
                        "invalid audio endpoint role",
                    ));
                }
            },
        };
        offer.validate(route)?;
        Ok(offer)
    }
}
