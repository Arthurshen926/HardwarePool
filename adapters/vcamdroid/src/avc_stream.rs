use std::{error::Error, fmt, io};

use crate::{
    AVC_WIRE_HEADER_BYTES, AVC_WIRE_MAX_ACCESS_UNIT_BYTES, AvcRecord, AvcWireError, decode_record,
};

/// Reads one length-bounded CAVC record from a byte stream.
///
/// Clean EOF is accepted only between records. A short header or payload is a
/// fail-closed framing error and payload allocation is capped before it occurs.
pub fn read_avc_record<R: io::Read>(
    reader: &mut R,
) -> Result<Option<AvcRecord>, AvcRecordStreamError> {
    let mut header = [0_u8; AVC_WIRE_HEADER_BYTES];
    loop {
        match reader.read(&mut header[..1]) {
            Ok(0) => return Ok(None),
            Ok(1) => break,
            Ok(_) => unreachable!("one-byte read returned more than one byte"),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(AvcRecordStreamError::Io(error)),
        }
    }
    reader
        .read_exact(&mut header[1..])
        .map_err(|error| map_short_read(error, AvcRecordStreamError::TruncatedHeader))?;

    let payload_len = u32::from_be_bytes([header[52], header[53], header[54], header[55]]) as usize;
    if payload_len > AVC_WIRE_MAX_ACCESS_UNIT_BYTES {
        return Err(AvcRecordStreamError::Wire(AvcWireError::TooLarge));
    }
    let total_len = AVC_WIRE_HEADER_BYTES
        .checked_add(payload_len)
        .ok_or(AvcRecordStreamError::Wire(AvcWireError::TooLarge))?;
    let mut bytes = Vec::with_capacity(total_len);
    bytes.extend_from_slice(&header);
    bytes.resize(total_len, 0);
    reader
        .read_exact(&mut bytes[AVC_WIRE_HEADER_BYTES..])
        .map_err(|error| map_short_read(error, AvcRecordStreamError::TruncatedPayload))?;
    decode_record(&bytes)
        .map(Some)
        .map_err(AvcRecordStreamError::Wire)
}

fn map_short_read(error: io::Error, short: AvcRecordStreamError) -> AvcRecordStreamError {
    if error.kind() == io::ErrorKind::UnexpectedEof {
        short
    } else {
        AvcRecordStreamError::Io(error)
    }
}

#[derive(Debug)]
pub enum AvcRecordStreamError {
    TruncatedHeader,
    TruncatedPayload,
    Wire(AvcWireError),
    Io(io::Error),
}

impl fmt::Display for AvcRecordStreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TruncatedHeader => {
                formatter.write_str("CAVC stream ended inside a record header")
            }
            Self::TruncatedPayload => {
                formatter.write_str("CAVC stream ended inside a record payload")
            }
            Self::Wire(error) => error.fmt(formatter),
            Self::Io(error) => write!(formatter, "CAVC stream I/O failed: {error}"),
        }
    }
}

impl Error for AvcRecordStreamError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Wire(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::TruncatedHeader | Self::TruncatedPayload => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::{
        AvcAccessUnit, AvcConfig, AvcLayout, AvcStreamKey, encode_access_unit, encode_config,
    };

    use super::*;

    #[test]
    fn concatenated_records_and_clean_eof_are_read() {
        let stream = test_stream();
        let mut bytes = encode_config(stream, &test_config()).unwrap();
        bytes.extend_from_slice(
            &encode_access_unit(&AvcAccessUnit {
                stream,
                sequence: 1,
                presentation_time_us: 10,
                key_frame: true,
                end_of_stream: false,
                discontinuity: false,
                payload: vec![0, 0, 0, 1, 0x65],
            })
            .unwrap(),
        );
        let mut input = Cursor::new(bytes);
        assert!(matches!(
            read_avc_record(&mut input).unwrap(),
            Some(AvcRecord::Config { .. })
        ));
        assert!(matches!(
            read_avc_record(&mut input).unwrap(),
            Some(AvcRecord::AccessUnit(_))
        ));
        assert!(read_avc_record(&mut input).unwrap().is_none());
    }

    #[test]
    fn truncated_and_oversized_stream_frames_fail_before_decode() {
        let encoded = encode_config(test_stream(), &test_config()).unwrap();
        let mut short_header = Cursor::new(encoded[..20].to_vec());
        assert!(matches!(
            read_avc_record(&mut short_header),
            Err(AvcRecordStreamError::TruncatedHeader)
        ));

        let mut short_payload = Cursor::new(encoded[..encoded.len() - 1].to_vec());
        assert!(matches!(
            read_avc_record(&mut short_payload),
            Err(AvcRecordStreamError::TruncatedPayload)
        ));

        let mut huge_header = encoded[..AVC_WIRE_HEADER_BYTES].to_vec();
        huge_header[52..56].copy_from_slice(&u32::MAX.to_be_bytes());
        let mut huge = Cursor::new(huge_header);
        assert!(matches!(
            read_avc_record(&mut huge),
            Err(AvcRecordStreamError::Wire(AvcWireError::TooLarge))
        ));
    }

    fn test_stream() -> AvcStreamKey {
        AvcStreamKey {
            stream_id: [7; 16],
            epoch: 3,
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
            csd0: vec![0, 0, 0, 1, 0x67],
            csd1: vec![0, 0, 0, 1, 0x68],
        }
    }
}
