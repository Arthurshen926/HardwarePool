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
    MAX_CATALOG_ADAPTERS, MAX_CATALOG_CAPABILITIES, MAX_CATALOG_FORMATS_PER_PORT,
    MAX_CATALOG_METADATA_ENTRIES, MAX_CATALOG_PORTS_PER_CAPABILITY, MAX_CATALOG_PROTOCOL_VERSIONS,
    MAX_CATALOG_STRING_BYTES, MAX_CONTROL_ENVELOPE_BYTES, ValidatedCatalogSnapshot,
    decode_catalog_snapshot, decode_envelope, encode_catalog_snapshot, encode_envelope,
    encode_envelope_checked, new_envelope, validate_envelope, validate_envelope_version,
};
pub use error::ProtocolError;

pub const PROTOCOL_MAJOR: u32 = 1;
pub const PROTOCOL_MINOR: u32 = 0;
