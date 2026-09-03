use std::fmt;

const MAGIC: [u8; 4] = *b"CAVC";
const KIND_CONFIG: u8 = 1;
const KIND_ACCESS_UNIT: u8 = 2;
const FLAG_KEY_FRAME: u8 = 0x01;
const FLAG_END_OF_STREAM: u8 = 0x02;
const FLAG_DISCONTINUITY: u8 = 0x04;
const KNOWN_ACCESS_FLAGS: u8 = FLAG_KEY_FRAME | FLAG_END_OF_STREAM | FLAG_DISCONTINUITY;
const CONFIG_PAYLOAD_HEADER_BYTES: usize = 28;

pub const AVC_WIRE_MAJOR: u8 = 1;
pub const AVC_WIRE_MINOR: u8 = 1;
pub const AVC_WIRE_HEADER_BYTES: usize = 56;
pub const AVC_WIRE_MAX_ACCESS_UNIT_BYTES: usize = 4 * 1024 * 1024;
pub const AVC_WIRE_MAX_CODEC_SPECIFIC_BYTES: usize = 64 * 1024;

const MAX_RECORD_BYTES: usize = AVC_WIRE_HEADER_BYTES + AVC_WIRE_MAX_ACCESS_UNIT_BYTES;
const MAX_DIMENSION: u16 = 4096;
const MAX_FRAMES_PER_SECOND: u16 = 60;
const MIN_BITRATE_BITS_PER_SECOND: u32 = 64_000;
const MAX_BITRATE_BITS_PER_SECOND: u32 = 50_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AvcLayout {
    AnnexB = 1,
    LengthPrefixed4 = 2,
    AvcDecoderConfigurationRecord = 3,
}

impl TryFrom<u8> for AvcLayout {
    type Error = AvcWireError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::AnnexB),
            2 => Ok(Self::LengthPrefixed4),
            3 => Ok(Self::AvcDecoderConfigurationRecord),
            _ => Err(AvcWireError::Invalid("unknown AVC payload layout")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AvcStreamKey {
    pub stream_id: [u8; 16],
    pub epoch: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvcConfig {
    pub width: u16,
    pub height: u16,
    pub frames_per_second: u16,
    pub bitrate_bits_per_second: u32,
    pub clockwise_rotation_degrees: u16,
    pub access_unit_layout: AvcLayout,
    pub codec_specific_layout: AvcLayout,
    pub csd0: Vec<u8>,
    pub csd1: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvcAccessUnit {
    pub stream: AvcStreamKey,
    pub sequence: u64,
    pub presentation_time_us: u64,
    pub key_frame: bool,
    pub end_of_stream: bool,
    pub discontinuity: bool,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AvcRecord {
    Config {
        stream: AvcStreamKey,
        config: AvcConfig,
    },
    AccessUnit(AvcAccessUnit),
}

/// Transactional per-stream ordering/replay guard for decoded records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvcRecordGuard {
    expected: AvcStreamKey,
    configured: bool,
    last_sequence: Option<u64>,
    last_presentation_time_us: Option<u64>,
    ended: bool,
}

impl AvcRecordGuard {
    pub fn new(expected: AvcStreamKey) -> Result<Self, AvcWireError> {
        validate_stream(expected)?;
        Ok(Self {
            expected,
            configured: false,
            last_sequence: None,
            last_presentation_time_us: None,
            ended: false,
        })
    }

    pub fn accept(&mut self, record: &AvcRecord) -> Result<(), AvcWireError> {
        if self.ended {
            return Err(AvcWireError::Invalid("stream already ended"));
        }
        match record {
            AvcRecord::Config { stream, .. } => {
                self.require_expected_stream(*stream)?;
                if self.configured {
                    return Err(AvcWireError::Invalid(
                        "duplicate config requires a new stream epoch",
                    ));
                }
                self.configured = true;
                Ok(())
            }
            AvcRecord::AccessUnit(unit) => self.accept_access_unit(unit),
        }
    }

    pub fn configured(&self) -> bool {
        self.configured
    }

    pub fn last_sequence(&self) -> Option<u64> {
        self.last_sequence
    }

    pub fn ended(&self) -> bool {
        self.ended
    }

    fn accept_access_unit(&mut self, unit: &AvcAccessUnit) -> Result<(), AvcWireError> {
        self.require_expected_stream(unit.stream)?;
        if !self.configured {
            return Err(AvcWireError::Invalid(
                "access unit arrived before stream config",
            ));
        }

        let gap = match self.last_sequence {
            Some(previous) => {
                if unit.sequence <= previous {
                    return Err(AvcWireError::Invalid(
                        "access-unit sequence is duplicate or replayed",
                    ));
                }
                unit.sequence != previous + 1
            }
            None => unit.sequence != 1,
        };
        if gap && !unit.discontinuity {
            return Err(AvcWireError::Invalid(
                "access-unit gap is missing discontinuity",
            ));
        }
        if self.last_sequence.is_none() && !unit.key_frame {
            return Err(AvcWireError::Invalid(
                "first access unit after config must be a key frame",
            ));
        }
        if unit.discontinuity && !unit.key_frame {
            return Err(AvcWireError::Invalid(
                "discontinuity must restart on a key frame",
            ));
        }
        if let Some(previous) = self.last_presentation_time_us {
            let advances = if unit.end_of_stream {
                unit.presentation_time_us >= previous
            } else {
                unit.presentation_time_us > previous
            };
            if !advances {
                return Err(AvcWireError::Invalid(
                    "access-unit presentation time regressed",
                ));
            }
        }

        self.last_sequence = Some(unit.sequence);
        self.last_presentation_time_us = Some(unit.presentation_time_us);
        self.ended = unit.end_of_stream;
        Ok(())
    }

    fn require_expected_stream(&self, actual: AvcStreamKey) -> Result<(), AvcWireError> {
        if actual != self.expected {
            return Err(AvcWireError::Invalid(
                "record stream or epoch does not match",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AvcWireError {
    TooShort,
    TooLarge,
    Invalid(&'static str),
}

impl fmt::Display for AvcWireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => formatter.write_str("AVC record is shorter than its fixed header"),
            Self::TooLarge => formatter.write_str("AVC record exceeds its fixed bootstrap bound"),
            Self::Invalid(reason) => write!(formatter, "invalid AVC record: {reason}"),
        }
    }
}

impl std::error::Error for AvcWireError {}

pub fn encode_config(stream: AvcStreamKey, config: &AvcConfig) -> Result<Vec<u8>, AvcWireError> {
    validate_stream(stream)?;
    validate_config(config)?;

    let payload_len = CONFIG_PAYLOAD_HEADER_BYTES
        .checked_add(config.csd0.len())
        .and_then(|value| value.checked_add(config.csd1.len()))
        .ok_or(AvcWireError::TooLarge)?;
    let mut record = Vec::with_capacity(AVC_WIRE_HEADER_BYTES + payload_len);
    write_header(&mut record, KIND_CONFIG, 0, stream, 0, 0, payload_len)?;
    record.extend_from_slice(&config.width.to_be_bytes());
    record.extend_from_slice(&config.height.to_be_bytes());
    record.extend_from_slice(&config.frames_per_second.to_be_bytes());
    record.extend_from_slice(&0_u16.to_be_bytes());
    record.extend_from_slice(&config.bitrate_bits_per_second.to_be_bytes());
    record.push(config.access_unit_layout as u8);
    record.push(config.codec_specific_layout as u8);
    record.extend_from_slice(&[1, 1, 1]); // limited-range BT.709 SDR
    record.push(rotation_code(config.clockwise_rotation_degrees)?);
    record.extend_from_slice(&[0, 0]);
    record.extend_from_slice(&(config.csd0.len() as u32).to_be_bytes());
    record.extend_from_slice(&(config.csd1.len() as u32).to_be_bytes());
    record.extend_from_slice(&config.csd0);
    record.extend_from_slice(&config.csd1);
    Ok(record)
}

pub fn encode_access_unit(unit: &AvcAccessUnit) -> Result<Vec<u8>, AvcWireError> {
    validate_stream(unit.stream)?;
    validate_access_unit(unit)?;

    let mut flags = 0;
    if unit.key_frame {
        flags |= FLAG_KEY_FRAME;
    }
    if unit.end_of_stream {
        flags |= FLAG_END_OF_STREAM;
    }
    if unit.discontinuity {
        flags |= FLAG_DISCONTINUITY;
    }
    let mut record = Vec::with_capacity(AVC_WIRE_HEADER_BYTES + unit.payload.len());
    write_header(
        &mut record,
        KIND_ACCESS_UNIT,
        flags,
        unit.stream,
        unit.sequence,
        unit.presentation_time_us,
        unit.payload.len(),
    )?;
    record.extend_from_slice(&unit.payload);
    Ok(record)
}

pub fn decode_record(input: &[u8]) -> Result<AvcRecord, AvcWireError> {
    if input.len() < AVC_WIRE_HEADER_BYTES {
        return Err(AvcWireError::TooShort);
    }
    if input.len() > MAX_RECORD_BYTES {
        return Err(AvcWireError::TooLarge);
    }
    if input[0..4] != MAGIC {
        return Err(AvcWireError::Invalid("magic does not match CAVC"));
    }
    if input[4] != AVC_WIRE_MAJOR || input[5] > AVC_WIRE_MINOR {
        return Err(AvcWireError::Invalid("wire version is unsupported"));
    }
    let wire_minor = input[5];
    let kind = input[6];
    let flags = input[7];
    if read_u16(input, 8) as usize != AVC_WIRE_HEADER_BYTES {
        return Err(AvcWireError::Invalid("header length is not 56 bytes"));
    }
    if read_u16(input, 10) != 0 {
        return Err(AvcWireError::Invalid("reserved header bits are non-zero"));
    }

    let mut stream_id = [0; 16];
    stream_id.copy_from_slice(&input[12..28]);
    let stream = AvcStreamKey {
        stream_id,
        epoch: read_u64(input, 28),
    };
    validate_stream(stream)?;
    let sequence = read_u64(input, 36);
    let presentation_time_us = read_u64(input, 44);
    let payload_len = read_u32(input, 52) as usize;
    let expected_len = AVC_WIRE_HEADER_BYTES
        .checked_add(payload_len)
        .ok_or(AvcWireError::TooLarge)?;
    if expected_len != input.len() {
        return Err(AvcWireError::Invalid(
            "payload length does not match record length",
        ));
    }
    let payload = &input[AVC_WIRE_HEADER_BYTES..];

    match kind {
        KIND_CONFIG => decode_config(
            stream,
            flags,
            sequence,
            presentation_time_us,
            payload,
            wire_minor,
        ),
        KIND_ACCESS_UNIT => {
            decode_access_unit(stream, flags, sequence, presentation_time_us, payload)
        }
        _ => Err(AvcWireError::Invalid("record kind is unknown")),
    }
}

fn decode_config(
    stream: AvcStreamKey,
    flags: u8,
    sequence: u64,
    presentation_time_us: u64,
    payload: &[u8],
    wire_minor: u8,
) -> Result<AvcRecord, AvcWireError> {
    if flags != 0 || sequence != 0 || presentation_time_us != 0 {
        return Err(AvcWireError::Invalid(
            "config flags, sequence and timestamp must be zero",
        ));
    }
    if payload.len() < CONFIG_PAYLOAD_HEADER_BYTES {
        return Err(AvcWireError::Invalid("config payload header is truncated"));
    }
    if read_u16(payload, 6) != 0 || payload[18..20] != [0, 0] {
        return Err(AvcWireError::Invalid("config reserved bits are non-zero"));
    }
    if wire_minor == 0 && payload[17] != 0 {
        return Err(AvcWireError::Invalid(
            "legacy config display rotation is non-zero",
        ));
    }
    if payload[14..17] != [1, 1, 1] {
        return Err(AvcWireError::Invalid(
            "config is not limited-range BT.709 SDR",
        ));
    }
    let csd0_len = read_u32(payload, 20) as usize;
    let csd1_len = read_u32(payload, 24) as usize;
    let expected_len = CONFIG_PAYLOAD_HEADER_BYTES
        .checked_add(csd0_len)
        .and_then(|value| value.checked_add(csd1_len))
        .ok_or(AvcWireError::TooLarge)?;
    if expected_len != payload.len() {
        return Err(AvcWireError::Invalid(
            "codec-specific lengths do not match config payload",
        ));
    }
    let csd1_offset = CONFIG_PAYLOAD_HEADER_BYTES + csd0_len;
    let config = AvcConfig {
        width: read_u16(payload, 0),
        height: read_u16(payload, 2),
        frames_per_second: read_u16(payload, 4),
        bitrate_bits_per_second: read_u32(payload, 8),
        clockwise_rotation_degrees: if wire_minor == 0 {
            0
        } else {
            decode_rotation(payload[17])?
        },
        access_unit_layout: AvcLayout::try_from(payload[12])?,
        codec_specific_layout: AvcLayout::try_from(payload[13])?,
        csd0: payload[CONFIG_PAYLOAD_HEADER_BYTES..csd1_offset].to_vec(),
        csd1: payload[csd1_offset..].to_vec(),
    };
    validate_config(&config)?;
    Ok(AvcRecord::Config { stream, config })
}

fn rotation_code(clockwise_rotation_degrees: u16) -> Result<u8, AvcWireError> {
    match clockwise_rotation_degrees {
        0 => Ok(0),
        90 => Ok(1),
        180 => Ok(2),
        270 => Ok(3),
        _ => Err(AvcWireError::Invalid(
            "display rotation is not 0/90/180/270 degrees",
        )),
    }
}

fn decode_rotation(code: u8) -> Result<u16, AvcWireError> {
    match code {
        0 => Ok(0),
        1 => Ok(90),
        2 => Ok(180),
        3 => Ok(270),
        _ => Err(AvcWireError::Invalid("display rotation code is unknown")),
    }
}

fn decode_access_unit(
    stream: AvcStreamKey,
    flags: u8,
    sequence: u64,
    presentation_time_us: u64,
    payload: &[u8],
) -> Result<AvcRecord, AvcWireError> {
    if flags & !KNOWN_ACCESS_FLAGS != 0 {
        return Err(AvcWireError::Invalid(
            "access-unit flags contain unknown bits",
        ));
    }
    let unit = AvcAccessUnit {
        stream,
        sequence,
        presentation_time_us,
        key_frame: flags & FLAG_KEY_FRAME != 0,
        end_of_stream: flags & FLAG_END_OF_STREAM != 0,
        discontinuity: flags & FLAG_DISCONTINUITY != 0,
        payload: payload.to_vec(),
    };
    validate_access_unit(&unit)?;
    Ok(AvcRecord::AccessUnit(unit))
}

fn write_header(
    output: &mut Vec<u8>,
    kind: u8,
    flags: u8,
    stream: AvcStreamKey,
    sequence: u64,
    presentation_time_us: u64,
    payload_len: usize,
) -> Result<(), AvcWireError> {
    if payload_len > AVC_WIRE_MAX_ACCESS_UNIT_BYTES || payload_len > u32::MAX as usize {
        return Err(AvcWireError::TooLarge);
    }
    output.extend_from_slice(&MAGIC);
    output.extend_from_slice(&[AVC_WIRE_MAJOR, AVC_WIRE_MINOR, kind, flags]);
    output.extend_from_slice(&(AVC_WIRE_HEADER_BYTES as u16).to_be_bytes());
    output.extend_from_slice(&0_u16.to_be_bytes());
    output.extend_from_slice(&stream.stream_id);
    output.extend_from_slice(&stream.epoch.to_be_bytes());
    output.extend_from_slice(&sequence.to_be_bytes());
    output.extend_from_slice(&presentation_time_us.to_be_bytes());
    output.extend_from_slice(&(payload_len as u32).to_be_bytes());
    Ok(())
}

fn validate_stream(stream: AvcStreamKey) -> Result<(), AvcWireError> {
    if stream.stream_id.iter().all(|value| *value == 0) {
        return Err(AvcWireError::Invalid("stream ID must not be all zero"));
    }
    if stream.epoch == 0 {
        return Err(AvcWireError::Invalid("stream epoch must be positive"));
    }
    Ok(())
}

fn validate_config(config: &AvcConfig) -> Result<(), AvcWireError> {
    if config.width == 0
        || config.height == 0
        || config.width > MAX_DIMENSION
        || config.height > MAX_DIMENSION
        || !config.width.is_multiple_of(2)
        || !config.height.is_multiple_of(2)
    {
        return Err(AvcWireError::Invalid(
            "config dimensions must be positive bounded and even",
        ));
    }
    if config.frames_per_second == 0 || config.frames_per_second > MAX_FRAMES_PER_SECOND {
        return Err(AvcWireError::Invalid("config frame rate is out of range"));
    }
    if !(MIN_BITRATE_BITS_PER_SECOND..=MAX_BITRATE_BITS_PER_SECOND)
        .contains(&config.bitrate_bits_per_second)
    {
        return Err(AvcWireError::Invalid("config bitrate is out of range"));
    }
    if config.access_unit_layout == AvcLayout::AvcDecoderConfigurationRecord {
        return Err(AvcWireError::Invalid(
            "access units cannot use an AVC decoder configuration record",
        ));
    }
    if config.csd0.is_empty()
        || config.csd0.len() > AVC_WIRE_MAX_CODEC_SPECIFIC_BYTES
        || config.csd1.len() > AVC_WIRE_MAX_CODEC_SPECIFIC_BYTES
    {
        return Err(AvcWireError::Invalid(
            "codec-specific data is missing or oversized",
        ));
    }
    Ok(())
}

fn validate_access_unit(unit: &AvcAccessUnit) -> Result<(), AvcWireError> {
    if unit.sequence == 0 {
        return Err(AvcWireError::Invalid(
            "access-unit sequence must be positive",
        ));
    }
    if unit.payload.len() > AVC_WIRE_MAX_ACCESS_UNIT_BYTES {
        return Err(AvcWireError::TooLarge);
    }
    if unit.end_of_stream {
        if !unit.payload.is_empty() || unit.key_frame {
            return Err(AvcWireError::Invalid(
                "end-of-stream must be empty and cannot be a key frame",
            ));
        }
    } else if unit.payload.is_empty() {
        return Err(AvcWireError::Invalid(
            "non-terminal access unit must carry bytes",
        ));
    }
    Ok(())
}

fn read_u16(input: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([input[offset], input[offset + 1]])
}

fn read_u32(input: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        input[offset],
        input[offset + 1],
        input[offset + 2],
        input[offset + 3],
    ])
}

fn read_u64(input: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes([
        input[offset],
        input[offset + 1],
        input[offset + 2],
        input[offset + 3],
        input[offset + 4],
        input[offset + 5],
        input[offset + 6],
        input[offset + 7],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG_GOLDEN_HEX: &str = concat!(
        "434156430101010000380000000102030405060708090a0b0c0d0e0f",
        "0000000000000002000000000000000000000000000000000000002c",
        "050002d0001e0000003d090001010101010000000000000800000008",
        "000000016764001f0000000168ee3c80"
    );
    const ACCESS_GOLDEN_HEX: &str = concat!(
        "434156430101020500380000000102030405060708090a0b0c0d0e0f",
        "000000000000000200000000000000070000000000030d4000000007",
        "00000001658884"
    );

    #[test]
    fn golden_config_and_access_unit_are_stable() {
        let stream = test_stream();
        let config = test_config();
        let config_record = encode_config(stream, &config).expect("valid config");
        assert_eq!(CONFIG_GOLDEN_HEX, hex(&config_record));
        assert_eq!(
            AvcRecord::Config { stream, config },
            decode_record(&config_record).expect("decode config")
        );

        let unit = AvcAccessUnit {
            stream,
            sequence: 7,
            presentation_time_us: 200_000,
            key_frame: true,
            end_of_stream: false,
            discontinuity: true,
            payload: vec![0, 0, 0, 1, 0x65, 0x88, 0x84],
        };
        let unit_record = encode_access_unit(&unit).expect("valid access unit");
        assert_eq!(ACCESS_GOLDEN_HEX, hex(&unit_record));
        assert_eq!(
            AvcRecord::AccessUnit(unit),
            decode_record(&unit_record).expect("decode access unit")
        );
    }

    #[test]
    fn decoder_rejects_malformed_or_ambiguous_records() {
        let valid = encode_config(test_stream(), &test_config()).expect("valid config");
        for cut in 0..AVC_WIRE_HEADER_BYTES {
            assert_eq!(
                AvcWireError::TooShort,
                decode_record(&valid[..cut]).unwrap_err()
            );
        }

        let mut bad = valid.clone();
        bad[0] = b'X';
        assert!(matches!(decode_record(&bad), Err(AvcWireError::Invalid(_))));
        let mut bad = valid.clone();
        bad[4] = 2;
        assert!(matches!(decode_record(&bad), Err(AvcWireError::Invalid(_))));
        let mut bad = valid.clone();
        bad[10] = 1;
        assert!(matches!(decode_record(&bad), Err(AvcWireError::Invalid(_))));
        let mut bad = valid.clone();
        bad[55] = bad[55].saturating_add(1);
        assert!(matches!(decode_record(&bad), Err(AvcWireError::Invalid(_))));
        let mut bad = valid.clone();
        bad[12..28].fill(0);
        assert!(matches!(decode_record(&bad), Err(AvcWireError::Invalid(_))));
        let mut bad = valid.clone();
        bad[AVC_WIRE_HEADER_BYTES + 17] = 4;
        assert!(matches!(decode_record(&bad), Err(AvcWireError::Invalid(_))));
    }

    #[test]
    fn display_rotation_roundtrips_and_legacy_minor_defaults_to_zero() {
        let stream = test_stream();
        let mut rotated = test_config();
        rotated.clockwise_rotation_degrees = 90;
        let encoded = encode_config(stream, &rotated).unwrap();
        assert_eq!(encoded[5], 1);
        assert_eq!(encoded[AVC_WIRE_HEADER_BYTES + 17], 1);
        assert_eq!(
            decode_record(&encoded).unwrap(),
            AvcRecord::Config {
                stream,
                config: rotated
            }
        );

        let mut legacy = encode_config(stream, &test_config()).unwrap();
        legacy[5] = 0;
        assert_eq!(
            decode_record(&legacy).unwrap(),
            AvcRecord::Config {
                stream,
                config: test_config()
            }
        );
    }

    #[test]
    fn bounds_and_terminal_rules_fail_closed() {
        let stream = test_stream();
        let mut config = test_config();
        config.csd0 = vec![0; AVC_WIRE_MAX_CODEC_SPECIFIC_BYTES + 1];
        assert!(matches!(
            encode_config(stream, &config),
            Err(AvcWireError::Invalid(_))
        ));

        let terminal = AvcAccessUnit {
            stream,
            sequence: 8,
            presentation_time_us: 233_333,
            key_frame: false,
            end_of_stream: true,
            discontinuity: false,
            payload: Vec::new(),
        };
        let encoded = encode_access_unit(&terminal).expect("valid terminal record");
        assert_eq!(
            AvcRecord::AccessUnit(terminal),
            decode_record(&encoded).unwrap()
        );

        let invalid = AvcAccessUnit {
            stream,
            sequence: 0,
            presentation_time_us: 0,
            key_frame: false,
            end_of_stream: false,
            discontinuity: false,
            payload: vec![1],
        };
        assert!(matches!(
            encode_access_unit(&invalid),
            Err(AvcWireError::Invalid(_))
        ));
    }

    #[test]
    fn receiver_guard_rejects_replay_gaps_and_timestamp_regression_transactionally() {
        let stream = test_stream();
        let config_record = AvcRecord::Config {
            stream,
            config: test_config(),
        };
        let mut guard = AvcRecordGuard::new(stream).unwrap();
        guard.accept(&config_record).unwrap();
        assert!(guard.configured());

        let first = access_record(stream, 7, 200_000, true, true, false);
        guard.accept(&first).unwrap();
        assert_eq!(Some(7), guard.last_sequence());

        assert!(guard.accept(&first).is_err());
        assert_eq!(Some(7), guard.last_sequence());
        let missing_discontinuity = access_record(stream, 9, 266_666, true, false, false);
        assert!(guard.accept(&missing_discontinuity).is_err());
        assert_eq!(Some(7), guard.last_sequence());
        let discontinuous_non_key = access_record(stream, 9, 266_666, false, true, false);
        assert!(guard.accept(&discontinuous_non_key).is_err());
        assert_eq!(Some(7), guard.last_sequence());

        guard
            .accept(&access_record(stream, 9, 266_666, true, true, false))
            .unwrap();
        let regressed = access_record(stream, 10, 100_000, false, false, false);
        assert!(guard.accept(&regressed).is_err());
        assert_eq!(Some(9), guard.last_sequence());

        guard
            .accept(&AvcRecord::AccessUnit(AvcAccessUnit {
                stream,
                sequence: 10,
                presentation_time_us: 266_666,
                key_frame: false,
                end_of_stream: true,
                discontinuity: false,
                payload: Vec::new(),
            }))
            .unwrap();
        assert!(guard.ended());
        assert!(guard.accept(&config_record).is_err());
    }

    fn test_stream() -> AvcStreamKey {
        AvcStreamKey {
            stream_id: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            epoch: 2,
        }
    }

    fn test_config() -> AvcConfig {
        AvcConfig {
            width: 1280,
            height: 720,
            frames_per_second: 30,
            bitrate_bits_per_second: 4_000_000,
            clockwise_rotation_degrees: 0,
            access_unit_layout: AvcLayout::AnnexB,
            codec_specific_layout: AvcLayout::AnnexB,
            csd0: vec![0, 0, 0, 1, 0x67, 0x64, 0, 0x1f],
            csd1: vec![0, 0, 0, 1, 0x68, 0xee, 0x3c, 0x80],
        }
    }

    fn access_record(
        stream: AvcStreamKey,
        sequence: u64,
        presentation_time_us: u64,
        key_frame: bool,
        discontinuity: bool,
        end_of_stream: bool,
    ) -> AvcRecord {
        AvcRecord::AccessUnit(AvcAccessUnit {
            stream,
            sequence,
            presentation_time_us,
            key_frame,
            end_of_stream,
            discontinuity,
            payload: if end_of_stream { Vec::new() } else { vec![1] },
        })
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
