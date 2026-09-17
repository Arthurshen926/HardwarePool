use std::convert::TryInto;

use capyio_core::StreamId;
use uuid::Uuid;

use crate::{
    AvcDecoderConfig, AvcLevel, AvcProfile, EncodedAccessUnitDescriptor, EncodedAccessUnitFlags,
    EncodedDisplayError, EncodedDisplayStreamSpec, OwnedEncodedAccessUnit,
};

const MAGIC: [u8; 4] = *b"CPD1";
pub const PRIVATE_DISPLAY_RECORD_VERSION: u8 = 1;
const KIND_DECODER_CONFIG: u8 = 1;
const KIND_ACCESS_UNIT: u8 = 2;
const COMMON_HEADER_BYTES: usize = 12;
const DECODER_CONFIG_HEADER_BYTES: usize = 68;
const ACCESS_UNIT_HEADER_BYTES: usize = 60;
const FLAG_KEY_FRAME: u8 = 1 << 0;
const FLAG_DISCONTINUITY: u8 = 1 << 1;
const FLAG_END_OF_STREAM: u8 = 1 << 2;
const KNOWN_ACCESS_UNIT_FLAGS: u8 = FLAG_KEY_FRAME | FLAG_DISCONTINUITY | FLAG_END_OF_STREAM;
const MAX_CODEC_CONFIG_BODY_BYTES: usize = 2 * 64 * 1024;
const MAX_RECORD_BODY_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Eq, PartialEq)]
pub enum PrivateDisplayRecord {
    DecoderConfig(AvcDecoderConfig),
    AccessUnit(OwnedEncodedAccessUnit),
}

pub fn encode_decoder_config_record(
    config: &AvcDecoderConfig,
) -> Result<Vec<u8>, EncodedDisplayError> {
    config.validate()?;
    validate_java_u64("stream epoch", config.stream_epoch)?;
    validate_java_u32(
        "frame-rate numerator",
        config.selected.decoded.frame_rate.numerator(),
    )?;
    validate_java_u32(
        "frame-rate denominator",
        config.selected.decoded.frame_rate.denominator(),
    )?;

    let body_bytes = config
        .sequence_parameter_set
        .len()
        .checked_add(config.picture_parameter_set.len())
        .ok_or_else(|| invalid_record("codec configuration length overflowed"))?;
    if body_bytes > MAX_CODEC_CONFIG_BODY_BYTES {
        return Err(invalid_record(
            "codec configuration exceeds its record bound",
        ));
    }
    let body_bytes_u32 = u32::try_from(body_bytes)
        .map_err(|_| invalid_record("codec configuration length does not fit u32"))?;
    let total_bytes = DECODER_CONFIG_HEADER_BYTES
        .checked_add(body_bytes)
        .ok_or_else(|| invalid_record("decoder configuration record length overflowed"))?;
    let mut encoded = Vec::with_capacity(total_bytes);
    write_common_header(
        &mut encoded,
        KIND_DECODER_CONFIG,
        0,
        DECODER_CONFIG_HEADER_BYTES,
        body_bytes_u32,
    )?;
    write_stream_id(&mut encoded, config.stream_id);
    write_u64(&mut encoded, config.stream_epoch);
    write_u32(&mut encoded, config.selected.decoded.width);
    write_u32(&mut encoded, config.selected.decoded.height);
    write_u32(&mut encoded, config.selected.decoded.frame_rate.numerator());
    write_u32(
        &mut encoded,
        config.selected.decoded.frame_rate.denominator(),
    );
    write_u32(&mut encoded, config.selected.maximum_access_unit_bytes);
    encoded.push(encode_profile(config.selected.avc_profile));
    encoded.push(encode_level(config.selected.avc_level));
    encoded.extend_from_slice(&[0, 0]);
    write_u32(
        &mut encoded,
        u32::try_from(config.sequence_parameter_set.len())
            .map_err(|_| invalid_record("SPS length does not fit u32"))?,
    );
    write_u32(
        &mut encoded,
        u32::try_from(config.picture_parameter_set.len())
            .map_err(|_| invalid_record("PPS length does not fit u32"))?,
    );
    encoded.extend_from_slice(&config.sequence_parameter_set);
    encoded.extend_from_slice(&config.picture_parameter_set);
    debug_assert_eq!(encoded.len(), total_bytes);
    Ok(encoded)
}

pub fn encode_access_unit_record(
    unit: &OwnedEncodedAccessUnit,
    selected: &EncodedDisplayStreamSpec,
) -> Result<Vec<u8>, EncodedDisplayError> {
    unit.validate(selected)?;
    validate_java_u64("stream epoch", unit.descriptor.stream_epoch)?;
    validate_java_u64("sequence", unit.descriptor.sequence)?;
    validate_java_u64(
        "presentation timestamp",
        unit.descriptor.presentation_time_micros,
    )?;
    validate_java_u64("duration", unit.descriptor.duration_nanos)?;

    let body_bytes = unit.payload.len();
    if body_bytes > MAX_RECORD_BODY_BYTES {
        return Err(invalid_record(
            "access-unit body exceeds the absolute record bound",
        ));
    }
    let body_bytes_u32 = u32::try_from(body_bytes)
        .map_err(|_| invalid_record("access-unit body length does not fit u32"))?;
    let total_bytes = ACCESS_UNIT_HEADER_BYTES
        .checked_add(body_bytes)
        .ok_or_else(|| invalid_record("access-unit record length overflowed"))?;
    let mut encoded = Vec::with_capacity(total_bytes);
    write_common_header(
        &mut encoded,
        KIND_ACCESS_UNIT,
        encode_flags(unit.descriptor.flags),
        ACCESS_UNIT_HEADER_BYTES,
        body_bytes_u32,
    )?;
    write_stream_id(&mut encoded, unit.descriptor.stream_id);
    write_u64(&mut encoded, unit.descriptor.stream_epoch);
    write_u64(&mut encoded, unit.descriptor.sequence);
    write_u64(&mut encoded, unit.descriptor.presentation_time_micros);
    write_u64(&mut encoded, unit.descriptor.duration_nanos);
    encoded.extend_from_slice(&unit.payload);
    debug_assert_eq!(encoded.len(), total_bytes);
    Ok(encoded)
}

pub fn decode_private_display_record(
    bytes: &[u8],
    selected: &EncodedDisplayStreamSpec,
) -> Result<PrivateDisplayRecord, EncodedDisplayError> {
    selected.validate()?;
    let prelude = parse_common_header(bytes)?;
    match prelude.kind {
        KIND_DECODER_CONFIG => decode_decoder_config(bytes, prelude, selected),
        KIND_ACCESS_UNIT => decode_access_unit(bytes, prelude, selected),
        _ => Err(invalid_record("record kind is unknown")),
    }
}

#[derive(Clone, Copy)]
struct CommonHeader {
    kind: u8,
    flags: u8,
    header_bytes: usize,
    body_bytes: usize,
}

fn parse_common_header(bytes: &[u8]) -> Result<CommonHeader, EncodedDisplayError> {
    if bytes.len() < COMMON_HEADER_BYTES {
        return Err(invalid_record("record is shorter than the common header"));
    }
    if bytes[..4] != MAGIC {
        return Err(invalid_record("record magic is invalid"));
    }
    if bytes[4] != PRIVATE_DISPLAY_RECORD_VERSION {
        return Err(invalid_record("record version is unsupported"));
    }
    let header_bytes = usize::from(bytes[7]);
    if header_bytes < COMMON_HEADER_BYTES {
        return Err(invalid_record("record header length is invalid"));
    }
    let body_bytes = usize::try_from(read_u32(bytes, 8)?)
        .map_err(|_| invalid_record("record body length does not fit usize"))?;
    if body_bytes > MAX_RECORD_BODY_BYTES {
        return Err(invalid_record("record body exceeds the absolute bound"));
    }
    let expected = header_bytes
        .checked_add(body_bytes)
        .ok_or_else(|| invalid_record("record length overflowed"))?;
    if bytes.len() != expected {
        return Err(invalid_record(
            "record length does not match header plus body",
        ));
    }
    Ok(CommonHeader {
        kind: bytes[5],
        flags: bytes[6],
        header_bytes,
        body_bytes,
    })
}

fn decode_decoder_config(
    bytes: &[u8],
    header: CommonHeader,
    selected: &EncodedDisplayStreamSpec,
) -> Result<PrivateDisplayRecord, EncodedDisplayError> {
    if header.header_bytes != DECODER_CONFIG_HEADER_BYTES {
        return Err(invalid_record(
            "decoder configuration header length is invalid",
        ));
    }
    if header.flags != 0 {
        return Err(invalid_record("decoder configuration flags must be zero"));
    }
    if header.body_bytes > MAX_CODEC_CONFIG_BODY_BYTES {
        return Err(invalid_record(
            "decoder configuration body exceeds its bound",
        ));
    }
    if bytes[58] != 0 || bytes[59] != 0 {
        return Err(invalid_record(
            "decoder configuration reserved bytes are nonzero",
        ));
    }

    let width = read_u32(bytes, 36)?;
    let height = read_u32(bytes, 40)?;
    let frame_rate_numerator = read_u32(bytes, 44)?;
    let frame_rate_denominator = read_u32(bytes, 48)?;
    let maximum_access_unit_bytes = read_u32(bytes, 52)?;
    let profile = decode_profile(bytes[56])?;
    let level = decode_level(bytes[57])?;
    if width != selected.decoded.width
        || height != selected.decoded.height
        || frame_rate_numerator != selected.decoded.frame_rate.numerator()
        || frame_rate_denominator != selected.decoded.frame_rate.denominator()
        || maximum_access_unit_bytes != selected.maximum_access_unit_bytes
        || profile != selected.avc_profile
        || level != selected.avc_level
    {
        return Err(invalid_record(
            "decoder configuration does not match the authenticated selection",
        ));
    }

    let sps_bytes = usize::try_from(read_u32(bytes, 60)?)
        .map_err(|_| invalid_record("SPS length does not fit usize"))?;
    let pps_bytes = usize::try_from(read_u32(bytes, 64)?)
        .map_err(|_| invalid_record("PPS length does not fit usize"))?;
    if sps_bytes
        .checked_add(pps_bytes)
        .ok_or_else(|| invalid_record("codec configuration lengths overflowed"))?
        != header.body_bytes
    {
        return Err(invalid_record(
            "SPS and PPS lengths do not match the record body",
        ));
    }
    let sps_end = header
        .header_bytes
        .checked_add(sps_bytes)
        .ok_or_else(|| invalid_record("SPS end offset overflowed"))?;
    let config = AvcDecoderConfig {
        stream_id: read_stream_id(bytes, 12)?,
        stream_epoch: read_java_u64(bytes, 28, "stream epoch")?,
        selected: selected.clone(),
        sequence_parameter_set: bytes[header.header_bytes..sps_end].to_vec(),
        picture_parameter_set: bytes[sps_end..].to_vec(),
    };
    config.validate()?;
    Ok(PrivateDisplayRecord::DecoderConfig(config))
}

fn decode_access_unit(
    bytes: &[u8],
    header: CommonHeader,
    selected: &EncodedDisplayStreamSpec,
) -> Result<PrivateDisplayRecord, EncodedDisplayError> {
    if header.header_bytes != ACCESS_UNIT_HEADER_BYTES {
        return Err(invalid_record("access-unit header length is invalid"));
    }
    if header.flags & !KNOWN_ACCESS_UNIT_FLAGS != 0 {
        return Err(invalid_record("access-unit flags contain reserved bits"));
    }
    let flags = EncodedAccessUnitFlags {
        key_frame: header.flags & FLAG_KEY_FRAME != 0,
        discontinuity: header.flags & FLAG_DISCONTINUITY != 0,
        end_of_stream: header.flags & FLAG_END_OF_STREAM != 0,
    };
    let payload_bytes = u32::try_from(header.body_bytes)
        .map_err(|_| invalid_record("access-unit body length does not fit u32"))?;
    let unit = OwnedEncodedAccessUnit {
        descriptor: EncodedAccessUnitDescriptor {
            stream_id: read_stream_id(bytes, 12)?,
            stream_epoch: read_java_u64(bytes, 28, "stream epoch")?,
            sequence: read_java_u64(bytes, 36, "sequence")?,
            presentation_time_micros: read_java_u64(bytes, 44, "presentation timestamp")?,
            duration_nanos: read_java_u64(bytes, 52, "duration")?,
            payload_bytes,
            flags,
        },
        payload: bytes[header.header_bytes..].to_vec(),
    };
    unit.validate(selected)?;
    Ok(PrivateDisplayRecord::AccessUnit(unit))
}

fn write_common_header(
    output: &mut Vec<u8>,
    kind: u8,
    flags: u8,
    header_bytes: usize,
    body_bytes: u32,
) -> Result<(), EncodedDisplayError> {
    let header_bytes = u8::try_from(header_bytes)
        .map_err(|_| invalid_record("record header length does not fit u8"))?;
    output.extend_from_slice(&MAGIC);
    output.push(PRIVATE_DISPLAY_RECORD_VERSION);
    output.push(kind);
    output.push(flags);
    output.push(header_bytes);
    write_u32(output, body_bytes);
    Ok(())
}

fn write_stream_id(output: &mut Vec<u8>, stream_id: StreamId) {
    output.extend_from_slice(stream_id.as_uuid().as_bytes());
}

fn write_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn write_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, EncodedDisplayError> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| invalid_record("u32 offset overflowed"))?;
    let raw: [u8; 4] = bytes
        .get(offset..end)
        .ok_or_else(|| invalid_record("record ended inside a u32 field"))?
        .try_into()
        .map_err(|_| invalid_record("u32 field has the wrong width"))?;
    Ok(u32::from_be_bytes(raw))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, EncodedDisplayError> {
    let end = offset
        .checked_add(8)
        .ok_or_else(|| invalid_record("u64 offset overflowed"))?;
    let raw: [u8; 8] = bytes
        .get(offset..end)
        .ok_or_else(|| invalid_record("record ended inside a u64 field"))?
        .try_into()
        .map_err(|_| invalid_record("u64 field has the wrong width"))?;
    Ok(u64::from_be_bytes(raw))
}

fn read_java_u64(bytes: &[u8], offset: usize, label: &str) -> Result<u64, EncodedDisplayError> {
    let value = read_u64(bytes, offset)?;
    validate_java_u64(label, value)?;
    Ok(value)
}

fn read_stream_id(bytes: &[u8], offset: usize) -> Result<StreamId, EncodedDisplayError> {
    let end = offset
        .checked_add(16)
        .ok_or_else(|| invalid_record("stream ID offset overflowed"))?;
    let raw: [u8; 16] = bytes
        .get(offset..end)
        .ok_or_else(|| invalid_record("record ended inside the stream ID"))?
        .try_into()
        .map_err(|_| invalid_record("stream ID has the wrong width"))?;
    Ok(StreamId::from_uuid(Uuid::from_bytes(raw)))
}

fn validate_java_u64(label: &str, value: u64) -> Result<(), EncodedDisplayError> {
    if value > i64::MAX as u64 {
        return Err(invalid_record(&format!(
            "{label} exceeds the shared Java signed-long range"
        )));
    }
    Ok(())
}

fn validate_java_u32(label: &str, value: u32) -> Result<(), EncodedDisplayError> {
    if value > i32::MAX as u32 {
        return Err(invalid_record(&format!(
            "{label} exceeds the shared Java signed-int range"
        )));
    }
    Ok(())
}

const fn encode_profile(profile: AvcProfile) -> u8 {
    match profile {
        AvcProfile::Baseline => 1,
        AvcProfile::Main => 2,
        AvcProfile::High => 3,
    }
}

fn decode_profile(value: u8) -> Result<AvcProfile, EncodedDisplayError> {
    match value {
        1 => Ok(AvcProfile::Baseline),
        2 => Ok(AvcProfile::Main),
        3 => Ok(AvcProfile::High),
        _ => Err(invalid_record("AVC profile value is unknown")),
    }
}

const fn encode_level(level: AvcLevel) -> u8 {
    match level {
        AvcLevel::L3_1 => 1,
        AvcLevel::L4 => 2,
        AvcLevel::L4_1 => 3,
        AvcLevel::L4_2 => 4,
        AvcLevel::L5 => 5,
        AvcLevel::L5_1 => 6,
        AvcLevel::L5_2 => 7,
    }
}

fn decode_level(value: u8) -> Result<AvcLevel, EncodedDisplayError> {
    match value {
        1 => Ok(AvcLevel::L3_1),
        2 => Ok(AvcLevel::L4),
        3 => Ok(AvcLevel::L4_1),
        4 => Ok(AvcLevel::L4_2),
        5 => Ok(AvcLevel::L5),
        6 => Ok(AvcLevel::L5_1),
        7 => Ok(AvcLevel::L5_2),
        _ => Err(invalid_record("AVC level value is unknown")),
    }
}

fn encode_flags(flags: EncodedAccessUnitFlags) -> u8 {
    (u8::from(flags.key_frame) * FLAG_KEY_FRAME)
        | (u8::from(flags.discontinuity) * FLAG_DISCONTINUITY)
        | (u8::from(flags.end_of_stream) * FLAG_END_OF_STREAM)
}

fn invalid_record(message: &str) -> EncodedDisplayError {
    EncodedDisplayError::InvalidRecord(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_header_sizes_remain_cross_language_safe() {
        assert_eq!(COMMON_HEADER_BYTES, 12);
        assert_eq!(DECODER_CONFIG_HEADER_BYTES, 68);
        assert_eq!(ACCESS_UNIT_HEADER_BYTES, 60);
        assert!(DECODER_CONFIG_HEADER_BYTES <= usize::from(u8::MAX));
        assert!(ACCESS_UNIT_HEADER_BYTES <= usize::from(u8::MAX));
    }
}
