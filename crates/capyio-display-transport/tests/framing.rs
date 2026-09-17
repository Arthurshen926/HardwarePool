use capyio_core::StreamId;
use capyio_display_transport::{
    AvcDecoderConfig, EncodedAccessUnitDescriptor, EncodedAccessUnitFlags, EncodedDisplayError,
    EncodedDisplayStreamSpec, OwnedEncodedAccessUnit, PrivateDisplayRecord,
    decode_private_display_record, encode_access_unit_record, encode_decoder_config_record,
};
use uuid::Uuid;

fn stream_id() -> StreamId {
    StreamId::from_uuid(Uuid::parse_str("00000000-0000-4000-8000-00000000d201").unwrap())
}

fn selected() -> EncodedDisplayStreamSpec {
    EncodedDisplayStreamSpec::fixture_720p30_avc()
}

fn config() -> AvcDecoderConfig {
    AvcDecoderConfig {
        stream_id: stream_id(),
        stream_epoch: 1,
        selected: selected(),
        sequence_parameter_set: vec![0, 0, 0, 1, 0x67, 0x64, 0, 31],
        picture_parameter_set: vec![0, 0, 0, 1, 0x68, 0xeb, 0xec, 0xb2],
    }
}

fn key_frame() -> OwnedEncodedAccessUnit {
    OwnedEncodedAccessUnit {
        descriptor: EncodedAccessUnitDescriptor {
            stream_id: stream_id(),
            stream_epoch: 1,
            sequence: 0,
            presentation_time_micros: 1_000_000,
            duration_nanos: 33_333_333,
            payload_bytes: 7,
            flags: EncodedAccessUnitFlags {
                key_frame: true,
                discontinuity: false,
                end_of_stream: false,
            },
        },
        payload: vec![0, 0, 0, 1, 0x65, 0x88, 0x84],
    }
}

fn end_of_stream() -> OwnedEncodedAccessUnit {
    OwnedEncodedAccessUnit {
        descriptor: EncodedAccessUnitDescriptor {
            stream_id: stream_id(),
            stream_epoch: 1,
            sequence: 1,
            presentation_time_micros: 1_033_333,
            duration_nanos: 33_333_333,
            payload_bytes: 0,
            flags: EncodedAccessUnitFlags {
                end_of_stream: true,
                ..EncodedAccessUnitFlags::default()
            },
        },
        payload: Vec::new(),
    }
}

#[test]
fn decoder_configuration_and_access_units_round_trip_exactly() {
    let selected = selected();
    let configuration = config();
    let configuration_bytes = encode_decoder_config_record(&configuration).unwrap();
    assert_eq!(
        decode_private_display_record(&configuration_bytes, &selected).unwrap(),
        PrivateDisplayRecord::DecoderConfig(configuration)
    );

    for unit in [key_frame(), end_of_stream()] {
        let bytes = encode_access_unit_record(&unit, &selected).unwrap();
        assert_eq!(
            decode_private_display_record(&bytes, &selected).unwrap(),
            PrivateDisplayRecord::AccessUnit(unit)
        );
    }
}

#[test]
fn record_envelope_rejects_truncation_trailing_bytes_and_unknown_values() {
    let selected = selected();
    let valid = encode_access_unit_record(&key_frame(), &selected).unwrap();

    for invalid in [
        valid[..11].to_vec(),
        valid[..valid.len() - 1].to_vec(),
        {
            let mut bytes = valid.clone();
            bytes.push(0);
            bytes
        },
        mutate(&valid, 0, b'X'),
        mutate(&valid, 4, 2),
        mutate(&valid, 5, 99),
        mutate(&valid, 6, 0x80),
        mutate(&valid, 7, 59),
    ] {
        assert!(matches!(
            decode_private_display_record(&invalid, &selected),
            Err(EncodedDisplayError::InvalidRecord(_))
        ));
    }
}

#[test]
fn configuration_rejects_reserved_bytes_bad_lengths_and_selection_mismatch() {
    let selected = selected();
    let valid = encode_decoder_config_record(&config()).unwrap();

    for invalid in [
        mutate(&valid, 6, 1),
        mutate(&valid, 58, 1),
        mutate(&valid, 63, 0),
        mutate(&valid, 36, 1),
    ] {
        assert!(decode_private_display_record(&invalid, &selected).is_err());
    }

    let mut other_selection = selected;
    other_selection.maximum_access_unit_bytes /= 2;
    assert!(matches!(
        decode_private_display_record(&valid, &other_selection),
        Err(EncodedDisplayError::InvalidRecord(_))
    ));
}

#[test]
fn framing_rejects_values_outside_the_shared_java_numeric_range() {
    let selected = selected();
    let mut unit = key_frame();
    unit.descriptor.sequence = i64::MAX as u64 + 1;
    assert!(matches!(
        encode_access_unit_record(&unit, &selected),
        Err(EncodedDisplayError::InvalidRecord(_))
    ));

    let mut configuration = config();
    configuration.stream_epoch = i64::MAX as u64 + 1;
    assert!(matches!(
        encode_decoder_config_record(&configuration),
        Err(EncodedDisplayError::InvalidRecord(_))
    ));
}

#[test]
fn deterministic_fixture_hex_is_stable() {
    let selected = selected();
    assert_eq!(
        hex(&encode_decoder_config_record(&config()).unwrap()),
        include_str!("../../../fixtures/display/private_v1_decoder_config.hex").trim()
    );
    assert_eq!(
        hex(&encode_access_unit_record(&key_frame(), &selected).unwrap()),
        include_str!("../../../fixtures/display/private_v1_key_access_unit.hex").trim()
    );
}

#[test]
fn real_avc_fixture_decodes_as_one_bounded_key_frame() {
    let selected = selected();
    let configuration_bytes = decode_hex(include_str!(
        "../../../fixtures/display/real_v1_decoder_config.hex"
    ));
    let access_unit_bytes = decode_hex(include_str!(
        "../../../fixtures/display/real_v1_key_access_unit.hex"
    ));

    let PrivateDisplayRecord::DecoderConfig(configuration) =
        decode_private_display_record(&configuration_bytes, &selected).unwrap()
    else {
        panic!("fixture kind changed");
    };
    assert_eq!(configuration.stream_id, stream_id());
    assert_eq!(configuration.stream_epoch, 2);
    assert_eq!(configuration.sequence_parameter_set.len(), 28);
    assert_eq!(configuration.picture_parameter_set.len(), 9);
    assert_eq!(configuration.sequence_parameter_set[4] & 0x1f, 7);
    assert_eq!(configuration.picture_parameter_set[4] & 0x1f, 8);

    let PrivateDisplayRecord::AccessUnit(unit) =
        decode_private_display_record(&access_unit_bytes, &selected).unwrap()
    else {
        panic!("fixture kind changed");
    };
    assert_eq!(unit.descriptor.stream_id, stream_id());
    assert_eq!(unit.descriptor.stream_epoch, 2);
    assert_eq!(unit.descriptor.sequence, 0);
    assert!(unit.descriptor.flags.key_frame);
    assert_eq!(unit.payload.len(), 32_447);
    assert_eq!(unit.payload[4] & 0x1f, 5);
}

fn mutate(source: &[u8], offset: usize, value: u8) -> Vec<u8> {
    let mut bytes = source.to_vec();
    bytes[offset] = value;
    bytes
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

fn decode_hex(text: &str) -> Vec<u8> {
    let digits: Vec<u8> = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    assert_eq!(digits.len() % 2, 0);
    digits
        .chunks_exact(2)
        .map(|pair| (hex_digit(pair[0]) << 4) | hex_digit(pair[1]))
        .collect()
}

fn hex_digit(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        _ => panic!("fixture is not lowercase hexadecimal"),
    }
}
