#![forbid(unsafe_code)]

//! Bounded encoded display data-plane contracts.
//!
//! This crate is an AdapterManaged seam between an encoder and a platform
//! decoder. Its private complete-record framing still requires an authenticated
//! transport binding and is not a public network protocol, codec implementation,
//! JSON-RPC payload, Android API binding or interoperability claim.

mod access_unit;
mod binding;
mod config;
mod error;
mod framing;

pub use access_unit::{
    EncodedAccessUnitDescriptor, EncodedAccessUnitFlags, OwnedEncodedAccessUnit,
};
pub use binding::{BoundPrivateDisplayReceiver, DisplayTransportBinding};
pub use config::{
    AvcDecoderConfig, AvcLevel, AvcProfile, EncodedDisplayCodec, EncodedDisplayStreamSpec,
    MAX_ACCESS_UNIT_BYTES,
};
pub use error::EncodedDisplayError;
pub use framing::{
    PRIVATE_DISPLAY_RECORD_VERSION, PrivateDisplayRecord, decode_private_display_record,
    encode_access_unit_record, encode_decoder_config_record,
};
