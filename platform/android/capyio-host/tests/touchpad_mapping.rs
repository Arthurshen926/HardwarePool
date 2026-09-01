use capyio_android_host::{
    AndroidMotionAction, AndroidMotionSample, AndroidPointerSample, AndroidToolType,
    AndroidTouchSurface, AndroidTouchpadMapper, AndroidTouchpadMappingError,
};
use capyio_input::{
    InputStreamDescriptor, TouchpadButtonType, TouchpadDescriptor, TouchpadFrameKind,
    TouchpadPhysicalSize,
};

fn stream() -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: "00000000-0000-4000-8000-00000000c401"
            .parse()
            .expect("stream ID"),
        stream_epoch: 7,
        clock_domain_id: "android.uptime_nanos".to_owned(),
    }
}

fn surface() -> AndroidTouchSurface {
    AndroidTouchSurface {
        width_px: 1_000,
        height_px: 600,
        descriptor: TouchpadDescriptor {
            physical_size: TouchpadPhysicalSize {
                width_himetric: 10_000,
                height_himetric: 6_000,
            },
            max_contacts: 5,
            button_type: TouchpadButtonType::NonClickable,
            reports_contact_size: false,
            reports_pressure: true,
        },
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

#[test]
fn maps_android_lifecycle_to_post_event_complete_snapshots() {
    let mut mapper = AndroidTouchpadMapper::new(stream(), surface(), 40).expect("mapper");
    assert_eq!(
        mapper
            .map_event(&event(
                90,
                AndroidMotionAction::Down,
                vec![finger(10, 100.0, 60.0)]
            ))
            .expect_err("initial barrier"),
        AndroidTouchpadMappingError::CancelAllRequired
    );

    let cancel = mapper.cancel_all(100).expect("initial cancel");
    assert_eq!(cancel.kind, TouchpadFrameKind::CancelAll);
    assert_eq!(cancel.header.sequence, 40);

    let down = mapper
        .map_event(&event(
            110,
            AndroidMotionAction::Down,
            vec![finger(10, 100.0, 60.0)],
        ))
        .expect("down");
    assert_eq!(down.header.sequence, 41);
    assert_eq!(down.contacts[0].contact_id, 10);
    assert_eq!(down.contacts[0].position.x_himetric, 1_000);
    assert_eq!(down.contacts[0].position.y_himetric, 600);
    assert_eq!(down.contacts[0].pressure.expect("pressure").get(), 32_768);

    let two = mapper
        .map_event(&event(
            120,
            AndroidMotionAction::PointerDown { action_index: 1 },
            vec![finger(10, 200.0, 120.0), finger(20, 800.0, 480.0)],
        ))
        .expect("pointer down");
    assert_eq!(
        two.contacts
            .iter()
            .map(|contact| contact.contact_id)
            .collect::<Vec<_>>(),
        vec![10, 20]
    );

    let reordered = mapper
        .map_event(&event(
            130,
            AndroidMotionAction::Move,
            vec![finger(20, 700.0, 420.0), finger(10, 300.0, 180.0)],
        ))
        .expect("move");
    assert_eq!(
        reordered
            .contacts
            .iter()
            .map(|contact| contact.contact_id)
            .collect::<Vec<_>>(),
        vec![20, 10]
    );

    let pointer_up = mapper
        .map_event(&event(
            140,
            AndroidMotionAction::PointerUp { action_index: 0 },
            vec![finger(20, 700.0, 420.0), finger(10, 300.0, 180.0)],
        ))
        .expect("pointer up");
    assert_eq!(pointer_up.contacts.len(), 1);
    assert_eq!(pointer_up.contacts[0].contact_id, 10);

    let up = mapper
        .map_event(&event(
            150,
            AndroidMotionAction::Up { action_index: 0 },
            vec![finger(10, 300.0, 180.0)],
        ))
        .expect("up");
    assert!(up.contacts.is_empty());
}

#[test]
fn cancel_ignores_pointer_payload_and_pressure_clamps_at_full_scale() {
    let mut mapper = AndroidTouchpadMapper::new(stream(), surface(), 0).expect("mapper");
    let cancel = mapper
        .map_event(&event(
            1,
            AndroidMotionAction::Cancel,
            vec![AndroidPointerSample {
                pointer_id: 1,
                tool_type: AndroidToolType::Unknown,
                x_px: f32::NAN,
                y_px: f32::NAN,
                pressure: Some(f32::NAN),
            }],
        ))
        .expect("fail-safe cancel");
    assert_eq!(cancel.kind, TouchpadFrameKind::CancelAll);
    assert!(cancel.contacts.is_empty());

    let mut calibrated = finger(1, 1_000.0, 600.0);
    calibrated.pressure = Some(1.7);
    let frame = mapper
        .map_event(&event(2, AndroidMotionAction::Down, vec![calibrated]))
        .expect("calibrated pressure");
    assert_eq!(
        frame.contacts[0].pressure.expect("pressure").get(),
        u16::MAX
    );
    assert_eq!(frame.contacts[0].position.x_himetric, 10_000);
    assert_eq!(frame.contacts[0].position.y_himetric, 6_000);
}

#[test]
fn malformed_motion_is_rejected_without_consuming_sequence_or_timestamp() {
    let mut mapper = AndroidTouchpadMapper::new(stream(), surface(), 5).expect("mapper");
    mapper.cancel_all(100).expect("cancel");

    let bad_index = event(
        110,
        AndroidMotionAction::PointerUp { action_index: 2 },
        vec![finger(1, 10.0, 10.0), finger(2, 20.0, 20.0)],
    );
    assert!(matches!(
        mapper.map_event(&bad_index),
        Err(AndroidTouchpadMappingError::InvalidMotion(_))
    ));

    let mut stylus = finger(1, 10.0, 10.0);
    stylus.tool_type = AndroidToolType::Stylus;
    assert!(matches!(
        mapper.map_event(&event(111, AndroidMotionAction::Down, vec![stylus])),
        Err(AndroidTouchpadMappingError::InvalidMotion(_))
    ));

    assert!(matches!(
        mapper.map_event(&event(
            112,
            AndroidMotionAction::Move,
            vec![finger(1, 10.0, 10.0), finger(1, 20.0, 20.0)]
        )),
        Err(AndroidTouchpadMappingError::InvalidMotion(_))
    ));

    assert!(matches!(
        mapper.map_event(&event(
            113,
            AndroidMotionAction::Down,
            vec![finger(1, 1_001.0, 10.0)]
        )),
        Err(AndroidTouchpadMappingError::InvalidMotion(_))
    ));

    let valid = mapper
        .map_event(&event(
            114,
            AndroidMotionAction::Down,
            vec![finger(1, 10.0, 10.0)],
        ))
        .expect("valid after failures");
    assert_eq!(valid.header.sequence, 6);

    assert_eq!(
        mapper
            .map_event(&event(
                113,
                AndroidMotionAction::Move,
                vec![finger(1, 20.0, 20.0)]
            ))
            .expect_err("timestamp regression"),
        AndroidTouchpadMappingError::TimestampRegression {
            previous: 114,
            actual: 113,
        }
    );
    let next = mapper
        .map_event(&event(
            115,
            AndroidMotionAction::Move,
            vec![finger(1, 20.0, 20.0)],
        ))
        .expect("transactional timestamp failure");
    assert_eq!(next.header.sequence, 7);
}

#[test]
fn surface_and_pointer_bounds_fail_closed() {
    let invalid_surface = AndroidTouchSurface {
        width_px: 0,
        ..surface()
    };
    assert!(matches!(
        AndroidTouchpadMapper::new(stream(), invalid_surface, 0),
        Err(AndroidTouchpadMappingError::InvalidSurface(_))
    ));

    let mut mapper = AndroidTouchpadMapper::new(stream(), surface(), 0).expect("mapper");
    mapper.cancel_all(1).expect("cancel");
    assert!(matches!(
        mapper.map_event(&event(
            2,
            AndroidMotionAction::Move,
            (0..=5)
                .map(|id| finger(id, 100.0 + id as f32, 100.0))
                .collect()
        )),
        Err(AndroidTouchpadMappingError::InvalidMotion(_))
    ));
}
