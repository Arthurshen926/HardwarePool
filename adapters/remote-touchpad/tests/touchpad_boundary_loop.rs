use capyio_android_host::{
    AndroidMotionAction, AndroidMotionSample, AndroidPointerSample, AndroidToolType,
    AndroidTouchSurface, AndroidTouchpadMapper,
};
use capyio_input::{
    InputStreamDescriptor, TouchpadButtonType, TouchpadDescriptor, TouchpadFrame,
    TouchpadPhysicalSize,
};
use capyio_remote_touchpad_adapter::PrivateTouchpadPacketCodecV1;
use capyio_windows_input::{
    WindowsTouchpadContactPhase, WindowsTouchpadProjection, WindowsTouchpadProjector,
};

fn stream() -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: "00000000-0000-4000-8000-00000000c702"
            .parse()
            .expect("stream ID"),
        stream_epoch: 9,
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
        reports_pressure: true,
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
        pressure: Some(0.5),
    }
}

fn event(
    timestamp: u64,
    action: AndroidMotionAction,
    pointers: Vec<AndroidPointerSample>,
) -> AndroidMotionSample {
    AndroidMotionSample {
        event_time_nanos: timestamp,
        action,
        pointers,
    }
}

fn transmit(
    codec: &PrivateTouchpadPacketCodecV1,
    projector: &mut WindowsTouchpadProjector,
    frame: &TouchpadFrame,
) -> WindowsTouchpadProjection {
    let packet = codec.encode(frame).expect("encode Android frame");
    let decoded = codec.decode(packet.as_bytes()).expect("decode packet");
    assert_eq!(&decoded, frame);
    projector.project(&decoded).expect("Windows projection")
}

#[test]
fn android_multicontact_lifecycle_survives_private_packet_and_windows_projection() {
    let mut mapper = AndroidTouchpadMapper::new(stream(), surface(), 40).expect("mapper");
    let codec = PrivateTouchpadPacketCodecV1::new(stream(), descriptor()).expect("codec");
    let mut projector =
        WindowsTouchpadProjector::new(&stream(), descriptor(), 40).expect("projector");

    let cancel = mapper.cancel_all(1_000).expect("initial cancel");
    assert!(
        transmit(&codec, &mut projector, &cancel)
            .batches()
            .is_empty()
    );

    let down = mapper
        .map_event(&event(
            2_000,
            AndroidMotionAction::Down,
            vec![finger(10, 100.0, 60.0)],
        ))
        .expect("down");
    let projected = transmit(&codec, &mut projector, &down);
    assert_eq!(projected.batches()[0].contacts().len(), 1);
    assert_eq!(projected.batches()[0].contacts()[0].contact_id, 10);
    assert_eq!(projected.batches()[0].contacts()[0].x_himetric, 1_000);
    assert_eq!(projected.batches()[0].contacts()[0].y_himetric, 600);
    assert_eq!(projected.batches()[0].contacts()[0].pressure, Some(32_768));

    let two = mapper
        .map_event(&event(
            3_000,
            AndroidMotionAction::PointerDown { action_index: 1 },
            vec![finger(10, 200.0, 120.0), finger(20, 800.0, 480.0)],
        ))
        .expect("two contacts");
    let projected = transmit(&codec, &mut projector, &two);
    assert_eq!(
        projected.batches()[0]
            .contacts()
            .iter()
            .map(|contact| (contact.contact_id, contact.phase))
            .collect::<Vec<_>>(),
        vec![
            (10, WindowsTouchpadContactPhase::Updated),
            (20, WindowsTouchpadContactPhase::Pressed),
        ]
    );

    let reordered = mapper
        .map_event(&event(
            4_000,
            AndroidMotionAction::Move,
            vec![finger(20, 700.0, 420.0), finger(10, 300.0, 180.0)],
        ))
        .expect("reordered move");
    let projected = transmit(&codec, &mut projector, &reordered);
    assert_eq!(
        projected.batches()[0]
            .contacts()
            .iter()
            .map(|contact| contact.contact_id)
            .collect::<Vec<_>>(),
        vec![10, 20]
    );

    let pointer_up = mapper
        .map_event(&event(
            5_000,
            AndroidMotionAction::PointerUp { action_index: 1 },
            vec![finger(10, 300.0, 180.0), finger(20, 700.0, 420.0)],
        ))
        .expect("pointer up");
    let projected = transmit(&codec, &mut projector, &pointer_up);
    assert_eq!(
        projected.batches()[0]
            .contacts()
            .iter()
            .map(|contact| (contact.contact_id, contact.phase))
            .collect::<Vec<_>>(),
        vec![
            (10, WindowsTouchpadContactPhase::Updated),
            (20, WindowsTouchpadContactPhase::Released),
        ]
    );

    let up = mapper
        .map_event(&event(
            6_000,
            AndroidMotionAction::Up { action_index: 0 },
            vec![finger(10, 300.0, 180.0)],
        ))
        .expect("up");
    let projected = transmit(&codec, &mut projector, &up);
    assert_eq!(projected.batches()[0].contacts().len(), 1);
    assert_eq!(
        projected.batches()[0].contacts()[0].phase,
        WindowsTouchpadContactPhase::Released
    );
}
