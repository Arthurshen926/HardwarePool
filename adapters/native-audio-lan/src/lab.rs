use capyio_audio::{AudioMediaStreamBinding, AudioStreamSpec};
use capyio_core::{RouteId, SessionId, StreamId};
use uuid::Uuid;

pub const NATIVE_SPEAKER_LAB_SESSION_ID: Uuid =
    Uuid::from_u128(0xa100_0000_0000_4000_8000_0000_0000_0001);
pub const NATIVE_SPEAKER_LAB_ROUTE_ID: Uuid =
    Uuid::from_u128(0xa100_0000_0000_4000_8000_0000_0000_0002);
pub const NATIVE_SPEAKER_LAB_STREAM_ID: Uuid =
    Uuid::from_u128(0xa100_0000_0000_4000_8000_0000_0000_0003);
pub const NATIVE_SPEAKER_LAB_STREAM_EPOCH: u32 = 1;
pub const NATIVE_MICROPHONE_LAB_SESSION_ID: Uuid =
    Uuid::from_u128(0xa200_0000_0000_4000_8000_0000_0000_0001);
pub const NATIVE_MICROPHONE_LAB_ROUTE_ID: Uuid =
    Uuid::from_u128(0xa200_0000_0000_4000_8000_0000_0000_0002);
pub const NATIVE_MICROPHONE_LAB_STREAM_ID: Uuid =
    Uuid::from_u128(0xa200_0000_0000_4000_8000_0000_0000_0003);
pub const NATIVE_MICROPHONE_LAB_STREAM_EPOCH: u32 = 1;

/// Fixed identity for the controlled 001E speaker lab, not production pairing.
#[must_use]
pub const fn speaker_lab_binding() -> AudioMediaStreamBinding {
    AudioMediaStreamBinding {
        session_id: SessionId::from_uuid(NATIVE_SPEAKER_LAB_SESSION_ID),
        route_id: RouteId::from_uuid(NATIVE_SPEAKER_LAB_ROUTE_ID),
        stream_id: StreamId::from_uuid(NATIVE_SPEAKER_LAB_STREAM_ID),
        stream_epoch: NATIVE_SPEAKER_LAB_STREAM_EPOCH,
        selected_spec: AudioStreamSpec::media_balanced(),
    }
}

/// Fixed identity for the controlled 001F microphone lab, not production pairing.
#[must_use]
pub const fn microphone_lab_binding() -> AudioMediaStreamBinding {
    AudioMediaStreamBinding {
        session_id: SessionId::from_uuid(NATIVE_MICROPHONE_LAB_SESSION_ID),
        route_id: RouteId::from_uuid(NATIVE_MICROPHONE_LAB_ROUTE_ID),
        stream_id: StreamId::from_uuid(NATIVE_MICROPHONE_LAB_STREAM_ID),
        stream_epoch: NATIVE_MICROPHONE_LAB_STREAM_EPOCH,
        selected_spec: AudioStreamSpec::voice_interactive(),
    }
}
