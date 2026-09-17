use capyio_core::StreamId;
use capyio_display_transport::{
    AvcDecoderConfig, AvcLevel, EncodedAccessUnitDescriptor, EncodedAccessUnitFlags,
    EncodedDisplayError, EncodedDisplayStreamSpec, OwnedEncodedAccessUnit,
};

fn stream_id() -> StreamId {
    StreamId::new()
}

fn config(stream_id: StreamId, epoch: u64) -> AvcDecoderConfig {
    AvcDecoderConfig {
        stream_id,
        stream_epoch: epoch,
        selected: EncodedDisplayStreamSpec::fixture_720p30_avc(),
        sequence_parameter_set: vec![0, 0, 0, 1, 0x67, 0x64, 0, 31],
        picture_parameter_set: vec![0, 0, 0, 1, 0x68, 0xeb, 0xec, 0xb2],
    }
}

fn unit(stream_id: StreamId, epoch: u64, sequence: u64, key_frame: bool) -> OwnedEncodedAccessUnit {
    OwnedEncodedAccessUnit {
        descriptor: EncodedAccessUnitDescriptor {
            stream_id,
            stream_epoch: epoch,
            sequence,
            presentation_time_micros: 1_000_000 + sequence * 33_333,
            duration_nanos: 33_333_333,
            payload_bytes: 5,
            flags: EncodedAccessUnitFlags {
                key_frame,
                ..EncodedAccessUnitFlags::default()
            },
        },
        payload: vec![0, 0, 0, 1, 0x65],
    }
}

#[test]
fn avc_config_requires_bounded_annex_b_sps_and_pps() {
    let stream = stream_id();
    let valid = config(stream, 1);
    valid.validate().unwrap();

    let mut missing_start_code = valid.clone();
    missing_start_code.picture_parameter_set = vec![0x68, 1, 2, 3, 4];
    assert!(matches!(
        missing_start_code.validate(),
        Err(EncodedDisplayError::InvalidDecoderConfig(_))
    ));

    let mut wrong_nal_type = valid.clone();
    wrong_nal_type.sequence_parameter_set = vec![0, 0, 0, 1, 0x68, 1];
    assert!(wrong_nal_type.validate().is_err());

    let mut zero_epoch = valid;
    zero_epoch.stream_epoch = 0;
    assert!(zero_epoch.validate().is_err());
}

#[test]
fn avc_level_covers_exact_dimensions_and_frame_rate() {
    let mut phone_wide = EncodedDisplayStreamSpec::fixture_720p30_avc();
    phone_wide.decoded.width = 1_980;
    phone_wide.decoded.height = 912;

    assert_eq!(
        AvcLevel::minimum_for(&phone_wide.decoded),
        Some(AvcLevel::L4)
    );
    assert!(!AvcLevel::L3_1.supports(&phone_wide.decoded));
    assert!(AvcLevel::L4.supports(&phone_wide.decoded));
    assert!(phone_wide.validate().is_err());

    phone_wide.avc_level = AvcLevel::L4;
    phone_wide.validate().unwrap();

    let fixture = EncodedDisplayStreamSpec::fixture_720p30_avc();
    assert_eq!(
        AvcLevel::minimum_for(&fixture.decoded),
        Some(AvcLevel::L3_1)
    );
    fixture.validate().unwrap();
}

#[test]
fn access_units_are_complete_bounded_and_match_owned_bytes() {
    let stream = stream_id();
    let selected = EncodedDisplayStreamSpec::fixture_720p30_avc();
    unit(stream, 1, 0, true).validate(&selected).unwrap();

    let mut mismatched = unit(stream, 1, 1, false);
    mismatched.descriptor.payload_bytes = 6;
    assert!(matches!(
        mismatched.validate(&selected),
        Err(EncodedDisplayError::InvalidAccessUnit(_))
    ));

    let mut oversized = unit(stream, 1, 2, false);
    oversized.descriptor.payload_bytes = selected.maximum_access_unit_bytes + 1;
    oversized.payload.clear();
    assert!(oversized.validate(&selected).is_err());
}

#[test]
fn end_of_stream_is_an_empty_distinct_terminal_unit() {
    let stream = stream_id();
    let selected = EncodedDisplayStreamSpec::fixture_720p30_avc();
    let terminal = OwnedEncodedAccessUnit {
        descriptor: EncodedAccessUnitDescriptor {
            stream_id: stream,
            stream_epoch: 1,
            sequence: 9,
            presentation_time_micros: 2_000_000,
            duration_nanos: 33_333_333,
            payload_bytes: 0,
            flags: EncodedAccessUnitFlags {
                end_of_stream: true,
                ..EncodedAccessUnitFlags::default()
            },
        },
        payload: Vec::new(),
    };
    terminal.validate(&selected).unwrap();

    let mut invalid = terminal;
    invalid.descriptor.flags.key_frame = true;
    assert!(invalid.validate(&selected).is_err());
}
