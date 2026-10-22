#![forbid(unsafe_code)]

//! CapyIO v1 Protobuf control-plane types and explicit Core conversions.

mod audio_offer;
mod camera_offer;
mod secure_audio_offer;
pub use codec::encode_route_status_with_secure_audio_data;
pub use secure_audio_offer::{
    SECURE_AUDIO_MONO_FORMAT, SecureAudioDataOffer, SecureAudioTransport,
};
mod display_settings;
pub use camera_offer::{CameraDataOffer, SecureCameraDataOffer, SecureCameraTransport};
pub use codec::{
    encode_route_status_with_camera_data, encode_route_status_with_secure_camera_data,
};
pub use display_settings::negotiate_display_settings;
pub use display_settings::validate_display_session_control;
mod codec;
mod conversion;
mod error;

/// Generated Protobuf v1 types.
pub mod v1 {
    include!(concat!(env!("OUT_DIR"), "/capyio.v1.rs"));
}

pub use audio_offer::{
    NATIVE_AUDIO_INBOUND_DRAFT_METADATA_KEY, NATIVE_AUDIO_INBOUND_DRAFT_MONO_V1,
    NativeAudioDataOffer, NativeAudioEndpoint, NativeAudioPreset,
};
pub use codec::encode_route_status_with_native_audio_data;
pub use codec::{HapticsDataOffer, encode_route_status_with_haptics_data};
pub use codec::{
    MAX_CATALOG_ADAPTERS, MAX_CATALOG_CAPABILITIES, MAX_CATALOG_FORMATS_PER_PORT,
    MAX_CATALOG_METADATA_ENTRIES, MAX_CATALOG_PORTS_PER_CAPABILITY, MAX_CATALOG_PROTOCOL_VERSIONS,
    MAX_CATALOG_STRING_BYTES, MAX_CONTROL_ENVELOPE_BYTES, MAX_PORT_LEASE_TTL_MS,
    MIN_PORT_LEASE_TTL_MS, PortLeaseAction, PortLeaseOutcome, RouteCommandAction,
    ValidatedCatalogSnapshot, ValidatedControlHeartbeat, ValidatedPortLeaseRequest,
    ValidatedPortLeaseResponse, ValidatedRouteCommand, ValidatedRouteStatus,
    decode_catalog_snapshot, decode_control_heartbeat, decode_envelope, decode_port_lease_request,
    decode_port_lease_response, decode_route_command, decode_route_status, encode_catalog_snapshot,
    encode_control_heartbeat, encode_envelope, encode_envelope_checked, encode_port_lease_request,
    encode_port_lease_response, encode_route_command, encode_route_status, new_envelope,
    validate_envelope, validate_envelope_version,
};
pub use codec::{TouchpadDataOffer, encode_route_status_with_touchpad_data};
pub use error::ProtocolError;

pub const PROTOCOL_MAJOR: u32 = 1;
pub const PROTOCOL_MINOR: u32 = 8;
