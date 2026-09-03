#![cfg_attr(not(windows), forbid(unsafe_code))]

//! Private, Adapter-managed camera integration boundary.
//!
//! The library owns bounded AVC record framing and stream reading. Its optional
//! lab executable accepts one loopback-only ADB-reverse connection; it does not
//! run a codec or claim StandardPort/production-network interoperability.

mod avc_stream;
mod avc_wire;
#[cfg(windows)]
mod windows_decoder;

pub use avc_stream::{AvcRecordStreamError, read_avc_record};

pub use avc_wire::{
    AVC_WIRE_HEADER_BYTES, AVC_WIRE_MAJOR, AVC_WIRE_MAX_ACCESS_UNIT_BYTES,
    AVC_WIRE_MAX_CODEC_SPECIFIC_BYTES, AVC_WIRE_MINOR, AvcAccessUnit, AvcConfig, AvcLayout,
    AvcRecord, AvcRecordGuard, AvcStreamKey, AvcWireError, decode_record, encode_access_unit,
    encode_config,
};
#[cfg(windows)]
pub use windows_decoder::{DecodedNv12Frame, MfAvcDecoder, MfAvcDecoderError, StageLatencyStats};

/// Makes the intentionally empty foundation state machine-readable to tests and inventories.
pub const IMPLEMENTATION_STATUS: &str = "private-avc-record-plus-loopback-mf-decoder-lab";
