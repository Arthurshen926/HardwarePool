use capyio_input::{
    InputContractError, InputFrameHeader, InputStreamDescriptor, NormalizedMagnitude,
    TouchpadButtonState, TouchpadButtonType, TouchpadContact, TouchpadContactSize,
    TouchpadDescriptor, TouchpadFrame, TouchpadFrameKind, TouchpadPhysicalSize, TouchpadPosition,
};
use capyio_remote_touchpad_adapter::{
    PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES, PRIVATE_TOUCHPAD_PACKET_MAGIC,
    PRIVATE_TOUCHPAD_PACKET_MAX_BYTES, PRIVATE_TOUCHPAD_PACKET_RECORD_BYTES,
    PRIVATE_TOUCHPAD_PACKET_VERSION, PrivateTouchpadPacketCodecV1, PrivateTouchpadPacketError,
};

fn stream(epoch: u64) -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: "00000000-0000-4000-8000-00000000c701"
            .parse()
            .expect("stream ID"),
        stream_epoch: epoch,
        clock_domain_id: "android.uptime_nanos".to_owned(),
    }
}

fn descriptor() -> TouchpadDescriptor {
    TouchpadDescriptor {
        physical_size: TouchpadPhysicalSize {
            width_himetric: 10_000,
            height_himetric: 6_000,
        },
        max_contacts: 5,
        button_type: TouchpadButtonType::ClickPad,
        reports_contact_size: true,
        reports_pressure: true,
    }
}

fn contact(index: u32) -> TouchpadContact {
    TouchpadContact {
        contact_id: 100 + index,
        position: TouchpadPosition {
            x_himetric: 1_000 + index * 1_000,
            y_himetric: 500 + index * 500,
        },
        confidence: index.is_multiple_of(2),
        size: Some(TouchpadContactSize {
            width_himetric: 100 + index,
            height_himetric: 200 + index,
        }),
        pressure: Some(NormalizedMagnitude::new(10_000 + index as u16)),
    }
}

fn update(epoch: u64, contacts: Vec<TouchpadContact>) -> TouchpadFrame {
    TouchpadFrame {
        header: InputFrameHeader {
            stream_id: stream(epoch).stream_id,
            stream_epoch: epoch,
            sequence: 42,
            source_timestamp_nanos: 1_234_567_890,
        },
        kind: TouchpadFrameKind::Update,
        button: TouchpadButtonState::Pressed,
        contacts,
    }
}

#[test]
fn five_contact_packet_round_trips_at_exact_maximum() {
    let codec = PrivateTouchpadPacketCodecV1::new(stream(7), descriptor()).expect("codec");
    let frame = update(7, (0..5).map(contact).collect());
    let packet = codec.encode(&frame).expect("encode");

    assert_eq!(packet.as_bytes()[..4], PRIVATE_TOUCHPAD_PACKET_MAGIC);
    assert_eq!(packet.as_bytes()[4], PRIVATE_TOUCHPAD_PACKET_VERSION);
    assert_eq!(packet.as_bytes()[7], 5);
    assert_eq!(
        packet.len(),
        PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES + 5 * PRIVATE_TOUCHPAD_PACKET_RECORD_BYTES
    );
    assert_eq!(packet.len(), PRIVATE_TOUCHPAD_PACKET_MAX_BYTES);
    assert!(!packet.is_empty());
    assert_eq!(codec.decode(packet.as_bytes()).expect("decode"), frame);
}

#[test]
fn cancel_packet_is_header_only_and_deterministic() {
    let codec = PrivateTouchpadPacketCodecV1::new(stream(7), descriptor()).expect("codec");
    let frame = TouchpadFrame {
        header: InputFrameHeader {
            stream_id: stream(7).stream_id,
            stream_epoch: 7,
            sequence: 0x0807_0605_0403_0201,
            source_timestamp_nanos: 9,
        },
        kind: TouchpadFrameKind::CancelAll,
        button: TouchpadButtonState::Released,
        contacts: Vec::new(),
    };
    let packet = codec.encode(&frame).expect("encode");
    assert_eq!(packet.len(), PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES);
    assert_eq!(&packet.as_bytes()[8..16], &7_u64.to_le_bytes());
    assert_eq!(
        &packet.as_bytes()[16..24],
        &0x0807_0605_0403_0201_u64.to_le_bytes()
    );
    assert_eq!(codec.decode(packet.as_bytes()).expect("decode"), frame);
}

#[test]
fn structural_corruption_is_rejected_before_semantic_projection() {
    let codec = PrivateTouchpadPacketCodecV1::new(stream(7), descriptor()).expect("codec");
    let packet = codec.encode(&update(7, vec![contact(0)])).expect("packet");

    assert!(matches!(
        codec.decode(&packet.as_bytes()[..31]),
        Err(PrivateTouchpadPacketError::PacketTooShort { .. })
    ));
    assert!(matches!(
        codec.decode(&[0; PRIVATE_TOUCHPAD_PACKET_MAX_BYTES + 1]),
        Err(PrivateTouchpadPacketError::PacketTooLong { .. })
    ));

    let mutate = |offset: usize, value: u8| {
        let mut bytes = packet.as_bytes().to_vec();
        bytes[offset] = value;
        bytes
    };
    assert!(matches!(
        codec.decode(&mutate(0, b'X')),
        Err(PrivateTouchpadPacketError::InvalidMagic { .. })
    ));
    assert_eq!(
        codec.decode(&mutate(4, 2)).expect_err("version"),
        PrivateTouchpadPacketError::UnsupportedVersion(2)
    );
    assert_eq!(
        codec.decode(&mutate(5, 2)).expect_err("kind"),
        PrivateTouchpadPacketError::InvalidFrameKind(2)
    );
    assert_eq!(
        codec.decode(&mutate(6, 2)).expect_err("button"),
        PrivateTouchpadPacketError::InvalidButtonState(2)
    );
    assert_eq!(
        codec.decode(&mutate(7, 6)).expect_err("count"),
        PrivateTouchpadPacketError::ContactCountExceedsDescriptor {
            actual: 6,
            maximum: 5,
        }
    );
    assert!(matches!(
        codec.decode(&mutate(7, 0)),
        Err(PrivateTouchpadPacketError::InvalidPacketLength { .. })
    ));
}

#[test]
fn unknown_and_noncanonical_contact_fields_are_rejected() {
    let codec = PrivateTouchpadPacketCodecV1::new(stream(7), descriptor()).expect("codec");
    let packet = codec.encode(&update(7, vec![contact(0)])).expect("packet");
    let flags_offset = PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES + 12;

    let mut unknown_flags = packet.as_bytes().to_vec();
    unknown_flags[flags_offset] |= 0x80;
    assert!(matches!(
        codec.decode(&unknown_flags),
        Err(PrivateTouchpadPacketError::UnknownContactFlags { .. })
    ));

    let mut reserved = packet.as_bytes().to_vec();
    reserved[flags_offset + 1] = 1;
    assert!(matches!(
        codec.decode(&reserved),
        Err(PrivateTouchpadPacketError::NonZeroReservedContactByte { .. })
    ));

    let mut absent_pressure = packet.as_bytes().to_vec();
    absent_pressure[flags_offset] &= !(1 << 2);
    assert_eq!(
        codec
            .decode(&absent_pressure)
            .expect_err("non-canonical pressure"),
        PrivateTouchpadPacketError::NonCanonicalContact {
            index: 0,
            field: "pressure",
        }
    );

    let mut absent_size = packet.as_bytes().to_vec();
    absent_size[flags_offset] &= !(1 << 1);
    assert_eq!(
        codec.decode(&absent_size).expect_err("non-canonical size"),
        PrivateTouchpadPacketError::NonCanonicalContact {
            index: 0,
            field: "contact-size",
        }
    );
}

#[test]
fn semantic_binding_and_epoch_advance_fail_closed() {
    let mut codec = PrivateTouchpadPacketCodecV1::new(stream(7), descriptor()).expect("codec");
    let old = codec
        .encode(&update(7, vec![contact(0)]))
        .expect("old packet");

    let mut wrong_stream = update(7, vec![contact(0)]);
    wrong_stream.header.stream_id = "00000000-0000-4000-8000-00000000ffff"
        .parse()
        .expect("different stream");
    assert!(matches!(
        codec.encode(&wrong_stream),
        Err(PrivateTouchpadPacketError::Contract(
            InputContractError::WrongStream { .. }
        ))
    ));
    assert!(matches!(
        codec.encode(&update(6, vec![contact(0)])),
        Err(PrivateTouchpadPacketError::Contract(
            InputContractError::StaleEpoch { .. }
        ))
    ));
    assert!(matches!(
        codec.encode(&update(8, vec![contact(0)])),
        Err(PrivateTouchpadPacketError::Contract(
            InputContractError::FutureEpoch { .. }
        ))
    ));

    codec.advance_epoch(8).expect("advance");
    assert_eq!(codec.epoch(), 8);
    assert!(matches!(
        codec.decode(old.as_bytes()),
        Err(PrivateTouchpadPacketError::Contract(
            InputContractError::StaleEpoch { .. }
        ))
    ));
    assert!(matches!(
        codec.advance_epoch(8),
        Err(PrivateTouchpadPacketError::Contract(
            InputContractError::NonAdvancingEpoch { .. }
        ))
    ));
    let fresh = codec.encode(&update(8, vec![contact(1)])).expect("fresh");
    assert_eq!(
        codec
            .decode(fresh.as_bytes())
            .expect("decode")
            .header
            .stream_epoch,
        8
    );
}

#[test]
fn decoded_duplicate_ids_still_pass_through_semantic_validation() {
    let codec = PrivateTouchpadPacketCodecV1::new(stream(7), descriptor()).expect("codec");
    let packet = codec
        .encode(&update(7, vec![contact(0), contact(1)]))
        .expect("packet");
    let mut duplicate = packet.as_bytes().to_vec();
    let first_id = duplicate
        [PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES..PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES + 4]
        .to_vec();
    let second = PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES + PRIVATE_TOUCHPAD_PACKET_RECORD_BYTES;
    duplicate[second..second + 4].copy_from_slice(&first_id);
    assert!(matches!(
        codec.decode(&duplicate),
        Err(PrivateTouchpadPacketError::Contract(
            InputContractError::InvalidTouchpadFrame(_)
        ))
    ));
}
