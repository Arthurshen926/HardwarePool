use capyio_android_host::{
    AndroidMotionAction, AndroidMotionSample, AndroidPointerSample, AndroidToolType,
    AndroidTouchSurface, AndroidTouchpadCaptureSession,
};
use capyio_input::{
    InputFrameHeader, InputStreamDescriptor, TouchpadButtonState, TouchpadButtonType,
    TouchpadDescriptor, TouchpadFrame, TouchpadFrameKind, TouchpadPhysicalSize,
};
use capyio_remote_touchpad_adapter::{
    PrivateTouchpadPacketCodecV1, PrivateTouchpadPacketSource, PrivateTouchpadPacketSourceError,
    PrivateTouchpadPacketSourceState,
};

fn stream() -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: "00000000-0000-4000-8000-00000000c403"
            .parse()
            .expect("stream ID"),
        stream_epoch: 11,
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

fn surface() -> AndroidTouchSurface {
    AndroidTouchSurface {
        width_px: 1_000,
        height_px: 600,
        descriptor: descriptor(),
    }
}

fn finger(pointer_id: u32, x_px: f32, y_px: f32) -> AndroidPointerSample {
    AndroidPointerSample {
        pointer_id,
        tool_type: AndroidToolType::Finger,
        x_px,
        y_px,
        pressure: None,
    }
}

fn event(
    event_time_nanos: u64,
    action: AndroidMotionAction,
    pointers: Vec<AndroidPointerSample>,
) -> AndroidMotionSample {
    AndroidMotionSample {
        event_time_nanos,
        action,
        pointers,
    }
}

#[test]
fn android_capture_lifecycle_encodes_to_closed_private_packet_source() {
    let mut capture = AndroidTouchpadCaptureSession::new(stream(), surface(), 20).expect("capture");
    let mut source = PrivateTouchpadPacketSource::new(stream(), descriptor(), 20).expect("source");
    let decoder = PrivateTouchpadPacketCodecV1::new(stream(), descriptor()).expect("decoder");
    let mut decoded = Vec::new();

    let frames = [
        capture.start(100).expect("start"),
        capture
            .map_motion(&event(
                110,
                AndroidMotionAction::Down,
                vec![finger(10, 100.0, 100.0)],
            ))
            .expect("down"),
        capture
            .map_motion(&event(
                120,
                AndroidMotionAction::PointerDown { action_index: 1 },
                vec![finger(10, 200.0, 100.0), finger(20, 800.0, 100.0)],
            ))
            .expect("pointer down"),
        capture
            .map_motion(&event(
                130,
                AndroidMotionAction::Move,
                vec![finger(20, 700.0, 300.0), finger(10, 300.0, 300.0)],
            ))
            .expect("move"),
        capture
            .map_motion(&event(
                140,
                AndroidMotionAction::PointerUp { action_index: 0 },
                vec![finger(20, 700.0, 300.0), finger(10, 300.0, 300.0)],
            ))
            .expect("pointer up"),
        capture
            .map_motion(&event(
                150,
                AndroidMotionAction::Up { action_index: 0 },
                vec![finger(10, 300.0, 300.0)],
            ))
            .expect("up"),
        capture.stop(160).expect("stop").expect("stop cancel"),
    ];

    for frame in frames {
        let packet = source.encode(&frame).expect("encode");
        decoded.push(decoder.decode(packet.as_bytes()).expect("decode"));
    }

    assert_eq!(source.packets_encoded(), 7);
    assert_eq!(
        decoded.first().expect("first").kind,
        TouchpadFrameKind::CancelAll
    );
    assert_eq!(decoded[2].contacts.len(), 2);
    assert_eq!(decoded[3].contacts[0].contact_id, 20);
    assert!(decoded[5].contacts.is_empty());
    assert_eq!(
        decoded.last().expect("last").kind,
        TouchpadFrameKind::CancelAll
    );
    source.close().expect("closed source");
    assert_eq!(source.state(), PrivateTouchpadPacketSourceState::Closed);
}

#[test]
fn source_requires_initial_cancel_and_contiguous_sequence_transactionally() {
    let mut source = PrivateTouchpadPacketSource::new(stream(), descriptor(), 5).expect("source");
    let frame = |sequence, kind, contacts| TouchpadFrame {
        header: InputFrameHeader {
            stream_id: stream().stream_id,
            stream_epoch: stream().stream_epoch,
            sequence,
            source_timestamp_nanos: 100 + sequence,
        },
        kind,
        button: TouchpadButtonState::Released,
        contacts,
    };

    assert_eq!(
        source
            .encode(&frame(5, TouchpadFrameKind::Update, Vec::new()))
            .expect_err("initial update"),
        PrivateTouchpadPacketSourceError::InitialCancellationRequired
    );
    source
        .encode(&frame(5, TouchpadFrameKind::CancelAll, Vec::new()))
        .expect("initial cancel");
    assert_eq!(source.packets_encoded(), 1);

    assert_eq!(
        source
            .encode(&frame(7, TouchpadFrameKind::Update, Vec::new()))
            .expect_err("gap"),
        PrivateTouchpadPacketSourceError::SequenceGap {
            expected: 6,
            actual: 7,
        }
    );
    assert_eq!(source.packets_encoded(), 1);
    source
        .encode(&frame(6, TouchpadFrameKind::Update, Vec::new()))
        .expect("recovered sequence");
    assert_eq!(source.packets_encoded(), 2);
}

#[test]
fn source_refuses_close_until_contacts_are_released_and_close_is_terminal() {
    let mut capture = AndroidTouchpadCaptureSession::new(stream(), surface(), 0).expect("capture");
    let mut source = PrivateTouchpadPacketSource::new(stream(), descriptor(), 0).expect("source");
    source
        .encode(&capture.start(10).expect("start"))
        .expect("cancel");
    source
        .encode(
            &capture
                .map_motion(&event(
                    20,
                    AndroidMotionAction::Down,
                    vec![finger(1, 100.0, 100.0)],
                ))
                .expect("down"),
        )
        .expect("active packet");
    assert_eq!(
        source.close().expect_err("active close"),
        PrivateTouchpadPacketSourceError::ActiveContactsAtClose
    );

    let stop = capture.stop(30).expect("stop").expect("cancel");
    source.encode(&stop).expect("stop packet");
    source.close().expect("close");
    source.close().expect("idempotent close");
    assert_eq!(
        source.encode(&stop).expect_err("encode after close"),
        PrivateTouchpadPacketSourceError::Closed
    );
}
