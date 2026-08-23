#![forbid(unsafe_code)]

//! CapyIO v1 Protobuf control-plane types and explicit Core conversions.

mod codec;
mod conversion;
mod error;

/// Generated Protobuf v1 types.
pub mod v1 {
    include!(concat!(env!("OUT_DIR"), "/capyio.v1.rs"));
}

pub use codec::{
    MAX_CONTROL_ENVELOPE_BYTES, decode_envelope, encode_envelope, encode_envelope_checked,
    new_envelope, validate_envelope, validate_envelope_version,
};
pub use error::ProtocolError;

pub const PROTOCOL_MAJOR: u32 = 1;
pub const PROTOCOL_MINOR: u32 = 0;
