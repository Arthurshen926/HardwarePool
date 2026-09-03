use capyio_core::{PortRef, RouteId, SessionId};
use capyio_input::{
    InputFrameHeader, InputStreamDescriptor, TouchpadButtonState, TouchpadButtonType,
    TouchpadContact, TouchpadDescriptor, TouchpadFrame, TouchpadFrameKind, TouchpadPhysicalSize,
    TouchpadPosition,
};
use capyio_remote_touchpad_adapter::{
    PRIVATE_TOUCHPAD_TRANSPORT_ACK_BYTES, PRIVATE_TOUCHPAD_TRANSPORT_CLOSE_BYTES,
    PRIVATE_TOUCHPAD_TRANSPORT_HELLO_BYTES, PRIVATE_TOUCHPAD_TRANSPORT_MAX_BYTES,
    PrivateTouchpadPacketCodecV1, PrivateTouchpadRouteBinding, PrivateTouchpadTransportCodecV1,
    PrivateTouchpadTransportRecordError,
};

fn id<T: std::str::FromStr>(value: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    value.parse().expect("fixture ID")
}

fn port(node: &str, capability: &str, port: &str) -> PortRef {
    PortRef {
        node_id: id(node),
        capability_id: id(capability),
        port_id: id(port),
    }
}

fn binding(epoch: u64, expiry: Option<u64>) -> PrivateTouchpadRouteBinding {
    PrivateTouchpadRouteBinding {
        route_id: id::<RouteId>("00000000-0000-4000-8000-00000000f101"),
        session_id: id::<SessionId>("00000000-0000-4000-8000-00000000f102"),
        source: port(
            "00000000-0000-4000-8000-00000000f103",
            "00000000-0000-4000-8000-00000000f104",
            "00000000-0000-4000-8000-00000000f105",
        ),
        sink: port(
            "00000000-0000-4000-8000-00000000f106",
            "00000000-0000-4000-8000-00000000f107",
            "00000000-0000-4000-8000-00000000f108",
        ),
        route_epoch: epoch,
        authorization_expires_at_ms: expiry,
    }
}

fn stream(epoch: u64) -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: id("00000000-0000-4000-8000-00000000f109"),
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
        button_type: TouchpadButtonType::NonClickable,
        reports_contact_size: false,
        reports_pressure: false,
    }
}

fn packet(
    epoch: u64,
    sequence: u64,
    contacts: usize,
) -> capyio_remote_touchpad_adapter::PrivateTouchpadPacketV1 {
    let contacts = (0..contacts)
        .map(|index| TouchpadContact {
            contact_id: index as u32 + 1,
            position: TouchpadPosition {
                x_himetric: 1_000 + index as u32 * 1_000,
                y_himetric: 2_000,
            },
            confidence: true,
            size: None,
            pressure: None,
        })
        .collect();
    PrivateTouchpadPacketCodecV1::new(stream(epoch), descriptor())
        .expect("packet codec")
        .encode(&TouchpadFrame {
            header: InputFrameHeader {
                stream_id: stream(epoch).stream_id,
                stream_epoch: epoch,
                sequence,
                source_timestamp_nanos: sequence + 1,
            },
            kind: TouchpadFrameKind::Update,
            button: TouchpadButtonState::Released,
            contacts,
        })
        .expect("packet")
}

#[test]
fn exact_hello_binds_route_session_endpoints_epoch_and_expiry() {
    let codec = PrivateTouchpadTransportCodecV1::new(binding(7, Some(9_000)));
    let hello = codec.encode_hello();
    assert_eq!(hello.len(), PRIVATE_TOUCHPAD_TRANSPORT_HELLO_BYTES);
    codec.validate_hello(hello.as_bytes()).expect("Hello");

    let mut changed = hello.as_bytes().to_vec();
    changed[152] ^= 1;
    assert_eq!(
        codec.validate_hello(&changed),
        Err(PrivateTouchpadTransportRecordError::BindingMismatch)
    );

    let no_expiry = PrivateTouchpadTransportCodecV1::new(binding(7, None));
    let hello = no_expiry.encode_hello();
    no_expiry
        .validate_hello(hello.as_bytes())
        .expect("no-expiry Hello");
    assert_eq!(&hello.as_bytes()[152..160], &[0; 8]);
}

#[test]
fn maximum_data_record_round_trips_and_ack_is_exact() {
    let codec = PrivateTouchpadTransportCodecV1::new(binding(1, Some(10_000)));
    let packet = packet(1, 42, 5);
    let record = codec.encode_data(&packet).expect("data record");
    assert_eq!(record.len(), PRIVATE_TOUCHPAD_TRANSPORT_MAX_BYTES);
    let decoded = codec.decode_data(record.as_bytes()).expect("decoded data");
    assert_eq!(decoded.as_bytes(), packet.as_bytes());

    let ack = codec.encode_ack(42);
    assert_eq!(ack.len(), PRIVATE_TOUCHPAD_TRANSPORT_ACK_BYTES);
    codec.validate_ack(ack.as_bytes(), 42).expect("ack");
    assert!(matches!(
        codec.validate_ack(ack.as_bytes(), 43),
        Err(PrivateTouchpadTransportRecordError::SequenceMismatch {
            expected: 43,
            actual: 42
        })
    ));

    let close = codec.encode_close();
    assert_eq!(close.len(), PRIVATE_TOUCHPAD_TRANSPORT_CLOSE_BYTES);
    codec.validate_close(close.as_bytes()).expect("close");
}

#[test]
fn malformed_outer_or_embedded_headers_fail_before_packet_delivery() {
    let codec = PrivateTouchpadTransportCodecV1::new(binding(1, None));
    let packet = packet(1, 4, 1);
    let record = codec.encode_data(&packet).expect("record");

    assert!(matches!(
        codec.decode_data(&record.as_bytes()[..20]),
        Err(PrivateTouchpadTransportRecordError::TooShort { .. })
    ));

    let mut wrong_outer_sequence = record.as_bytes().to_vec();
    wrong_outer_sequence[16..24].copy_from_slice(&5_u64.to_le_bytes());
    assert!(matches!(
        codec.decode_data(&wrong_outer_sequence),
        Err(PrivateTouchpadTransportRecordError::SequenceMismatch {
            expected: 5,
            actual: 4
        })
    ));

    let mut wrong_packet_length = record.as_bytes().to_vec();
    wrong_packet_length[24 + 7] = 2;
    assert!(matches!(
        codec.decode_data(&wrong_packet_length),
        Err(PrivateTouchpadTransportRecordError::InvalidPacketLength { .. })
    ));

    let mut unknown_flags = record.as_bytes().to_vec();
    unknown_flags[6] = 0x80;
    assert_eq!(
        codec.decode_data(&unknown_flags),
        Err(PrivateTouchpadTransportRecordError::UnknownFlags(0x80))
    );
}

#[test]
fn data_epoch_must_match_the_admitted_route_binding() {
    let codec = PrivateTouchpadTransportCodecV1::new(binding(1, None));
    assert_eq!(
        codec.encode_data(&packet(2, 0, 0)),
        Err(PrivateTouchpadTransportRecordError::EpochMismatch {
            expected: 1,
            actual: 2
        })
    );
}
