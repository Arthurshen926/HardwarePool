use capyio_core::{RouteId, SessionId, StreamId};
use capyio_display_transport::{
    AvcDecoderConfig, BoundPrivateDisplayReceiver, DisplayTransportBinding,
    EncodedAccessUnitDescriptor, EncodedAccessUnitFlags, EncodedDisplayError,
    EncodedDisplayStreamSpec, OwnedEncodedAccessUnit, PrivateDisplayRecord,
    encode_access_unit_record, encode_decoder_config_record,
};
use uuid::Uuid;

fn id<T>(value: &str, convert: impl FnOnce(Uuid) -> T) -> T {
    convert(Uuid::parse_str(value).unwrap())
}

fn stream_id() -> StreamId {
    id("00000000-0000-4000-8000-00000000d201", StreamId::from_uuid)
}

fn selected() -> EncodedDisplayStreamSpec {
    EncodedDisplayStreamSpec::fixture_720p30_avc()
}

fn binding(stream_id: StreamId, epoch: u64) -> DisplayTransportBinding {
    DisplayTransportBinding::new(
        id("00000000-0000-4000-8000-00000000a001", SessionId::from_uuid),
        id("00000000-0000-4000-8000-00000000a002", RouteId::from_uuid),
        stream_id,
        epoch,
        selected(),
    )
    .unwrap()
}

fn config(stream_id: StreamId, epoch: u64) -> AvcDecoderConfig {
    AvcDecoderConfig {
        stream_id,
        stream_epoch: epoch,
        selected: selected(),
        sequence_parameter_set: vec![0, 0, 0, 1, 0x67, 0x4d, 0, 31],
        picture_parameter_set: vec![0, 0, 0, 1, 0x68, 0xeb, 0xec, 0xb2],
    }
}

fn unit(stream_id: StreamId, epoch: u64, sequence: u64, eos: bool) -> OwnedEncodedAccessUnit {
    let payload = if eos {
        Vec::new()
    } else {
        vec![0, 0, 0, 1, 0x65, 0x88, 0x84]
    };
    OwnedEncodedAccessUnit {
        descriptor: EncodedAccessUnitDescriptor {
            stream_id,
            stream_epoch: epoch,
            sequence,
            presentation_time_micros: 1_000_000 + sequence * 33_333,
            duration_nanos: 33_333_333,
            payload_bytes: u32::try_from(payload.len()).unwrap(),
            flags: EncodedAccessUnitFlags {
                key_frame: !eos,
                end_of_stream: eos,
                ..EncodedAccessUnitFlags::default()
            },
        },
        payload,
    }
}

#[test]
fn bound_receiver_accepts_config_then_monotonic_units_and_eos() {
    let stream_id = stream_id();
    let selected = selected();
    let mut receiver = BoundPrivateDisplayReceiver::new(binding(stream_id, 7));
    assert!(matches!(
        receiver
            .accept(&encode_decoder_config_record(&config(stream_id, 7)).unwrap())
            .unwrap(),
        PrivateDisplayRecord::DecoderConfig(_)
    ));
    for item in [unit(stream_id, 7, 2, false), unit(stream_id, 7, 9, false)] {
        assert!(matches!(
            receiver
                .accept(&encode_access_unit_record(&item, &selected).unwrap())
                .unwrap(),
            PrivateDisplayRecord::AccessUnit(_)
        ));
    }
    receiver
        .accept(&encode_access_unit_record(&unit(stream_id, 7, 10, true), &selected).unwrap())
        .unwrap();
    assert!(
        receiver
            .accept(&encode_decoder_config_record(&config(stream_id, 7)).unwrap())
            .is_err()
    );
}

#[test]
fn bound_receiver_rejects_unconfigured_cross_stream_cross_epoch_and_replay() {
    let stream_id = stream_id();
    let selected = selected();
    let first = unit(stream_id, 7, 3, false);
    let first_bytes = encode_access_unit_record(&first, &selected).unwrap();
    let mut receiver = BoundPrivateDisplayReceiver::new(binding(stream_id, 7));
    assert!(receiver.accept(&first_bytes).is_err());

    let other_stream = StreamId::new();
    assert!(
        receiver
            .accept(&encode_decoder_config_record(&config(other_stream, 7)).unwrap())
            .is_err()
    );
    assert!(
        receiver
            .accept(&encode_decoder_config_record(&config(stream_id, 8)).unwrap())
            .is_err()
    );

    receiver
        .accept(&encode_decoder_config_record(&config(stream_id, 7)).unwrap())
        .unwrap();
    receiver.accept(&first_bytes).unwrap();
    for stale in [unit(stream_id, 7, 3, false), unit(stream_id, 7, 2, false)] {
        assert!(matches!(
            receiver.accept(&encode_access_unit_record(&stale, &selected).unwrap()),
            Err(EncodedDisplayError::InvalidRecord(_))
        ));
    }
}

#[test]
fn binding_retains_outer_session_and_route_without_serializing_them() {
    let binding = binding(stream_id(), 7);
    assert_eq!(
        binding.session_id().to_string(),
        "00000000-0000-4000-8000-00000000a001"
    );
    assert_eq!(
        binding.route_id().to_string(),
        "00000000-0000-4000-8000-00000000a002"
    );
    assert_eq!(binding.stream_epoch(), 7);
    assert_eq!(binding.selected(), &selected());
}
