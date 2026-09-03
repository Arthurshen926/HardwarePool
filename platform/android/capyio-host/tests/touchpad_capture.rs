use capyio_android_host::{
    AndroidMotionAction, AndroidMotionSample, AndroidPointerSample, AndroidToolType,
    AndroidTouchSurface, AndroidTouchpadCaptureError, AndroidTouchpadCaptureSession,
    AndroidTouchpadCaptureState, AndroidTouchpadMotionPolicy,
};
use capyio_input::{
    InputStreamDescriptor, TouchpadButtonType, TouchpadDescriptor, TouchpadFrameKind,
    TouchpadPhysicalSize,
};

fn stream() -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: "00000000-0000-4000-8000-00000000c402"
            .parse()
            .expect("stream ID"),
        stream_epoch: 9,
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
            reports_pressure: false,
        },
    }
}

fn finger(pointer_id: u32, x_px: f32) -> AndroidPointerSample {
    AndroidPointerSample {
        pointer_id,
        tool_type: AndroidToolType::Finger,
        x_px,
        y_px: 100.0,
        pressure: None,
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
fn captures_reordered_multitouch_lifecycle_transactionally() {
    let mut capture =
        AndroidTouchpadCaptureSession::new(stream(), surface(), 40).expect("capture session");
    assert_eq!(capture.state(), AndroidTouchpadCaptureState::Stopped);
    assert_eq!(
        capture
            .map_motion(&event(
                90,
                AndroidMotionAction::Down,
                vec![finger(10, 100.0)]
            ))
            .expect_err("stopped capture"),
        AndroidTouchpadCaptureError::NotRunning
    );

    let start = capture.start(100).expect("start");
    assert_eq!(start.kind, TouchpadFrameKind::CancelAll);
    assert_eq!(start.header.sequence, 40);

    let down = capture
        .map_motion(&event(
            110,
            AndroidMotionAction::Down,
            vec![finger(10, 100.0)],
        ))
        .expect("down");
    assert_eq!(down.header.sequence, 41);
    assert_eq!(capture.active_contact_ids(), &[10]);

    capture
        .map_motion(&event(
            120,
            AndroidMotionAction::PointerDown { action_index: 1 },
            vec![finger(10, 200.0), finger(20, 800.0)],
        ))
        .expect("second pointer down");
    assert_eq!(capture.active_contact_ids(), &[10, 20]);

    capture
        .map_motion(&event(
            130,
            AndroidMotionAction::Move,
            vec![finger(20, 700.0), finger(10, 300.0)],
        ))
        .expect("reordered move");
    assert_eq!(capture.active_contact_ids(), &[20, 10]);

    let pointer_up = capture
        .map_motion(&event(
            140,
            AndroidMotionAction::PointerUp { action_index: 0 },
            vec![finger(20, 700.0), finger(10, 300.0)],
        ))
        .expect("pointer up");
    assert_eq!(pointer_up.contacts[0].contact_id, 10);
    assert_eq!(capture.active_contact_ids(), &[10]);

    let up = capture
        .map_motion(&event(
            150,
            AndroidMotionAction::Up { action_index: 0 },
            vec![finger(10, 300.0)],
        ))
        .expect("up");
    assert!(up.contacts.is_empty());
    assert!(capture.active_contact_ids().is_empty());
}

#[test]
fn rejects_pointer_identity_drift_without_consuming_sequence_or_state() {
    let mut capture =
        AndroidTouchpadCaptureSession::new(stream(), surface(), 5).expect("capture session");
    capture.start(100).expect("start");
    capture
        .map_motion(&event(
            110,
            AndroidMotionAction::Down,
            vec![finger(1, 100.0)],
        ))
        .expect("down");

    assert!(matches!(
        capture.map_motion(&event(
            120,
            AndroidMotionAction::Move,
            vec![finger(2, 200.0)]
        )),
        Err(AndroidTouchpadCaptureError::InvalidLifecycle(_))
    ));
    assert_eq!(capture.active_contact_ids(), &[1]);

    assert!(matches!(
        capture.map_motion(&event(
            121,
            AndroidMotionAction::PointerDown { action_index: 0 },
            vec![finger(1, 100.0), finger(2, 200.0)]
        )),
        Err(AndroidTouchpadCaptureError::InvalidLifecycle(_))
    ));
    assert_eq!(capture.active_contact_ids(), &[1]);

    let valid = capture
        .map_motion(&event(
            122,
            AndroidMotionAction::Move,
            vec![finger(1, 300.0)],
        ))
        .expect("valid after lifecycle errors");
    assert_eq!(valid.header.sequence, 7);
}

#[test]
fn stop_restart_and_close_emit_bounded_cancellation() {
    let mut capture =
        AndroidTouchpadCaptureSession::new(stream(), surface(), 0).expect("capture session");
    capture.start(10).expect("start");
    capture
        .map_motion(&event(
            20,
            AndroidMotionAction::Down,
            vec![finger(7, 100.0)],
        ))
        .expect("down");

    let stopped = capture.stop(30).expect("stop").expect("cancel frame");
    assert_eq!(stopped.kind, TouchpadFrameKind::CancelAll);
    assert_eq!(stopped.header.sequence, 2);
    assert_eq!(capture.state(), AndroidTouchpadCaptureState::Stopped);
    assert!(capture.active_contact_ids().is_empty());
    assert!(capture.stop(31).expect("idempotent stop").is_none());

    let restarted = capture.start(40).expect("restart");
    assert_eq!(restarted.kind, TouchpadFrameKind::CancelAll);
    assert_eq!(restarted.header.sequence, 3);
    assert!(matches!(
        capture.start(41),
        Err(AndroidTouchpadCaptureError::AlreadyRunning)
    ));

    let closed = capture.close(50).expect("close").expect("close cancel");
    assert_eq!(closed.kind, TouchpadFrameKind::CancelAll);
    assert_eq!(capture.state(), AndroidTouchpadCaptureState::Closed);
    assert!(capture.close(51).expect("idempotent close").is_none());
    assert!(matches!(
        capture.start(52),
        Err(AndroidTouchpadCaptureError::Closed)
    ));
}

#[test]
fn mapping_and_stop_timestamp_failures_leave_capture_recoverable() {
    let mut capture =
        AndroidTouchpadCaptureSession::new(stream(), surface(), 0).expect("capture session");
    capture.start(100).expect("start");

    let mut stylus = finger(1, 100.0);
    stylus.tool_type = AndroidToolType::Stylus;
    assert!(matches!(
        capture.map_motion(&event(110, AndroidMotionAction::Down, vec![stylus])),
        Err(AndroidTouchpadCaptureError::Mapping(_))
    ));
    assert!(capture.active_contact_ids().is_empty());

    capture
        .map_motion(&event(
            111,
            AndroidMotionAction::Down,
            vec![finger(1, 100.0)],
        ))
        .expect("valid down");
    assert!(matches!(
        capture.stop(110),
        Err(AndroidTouchpadCaptureError::Mapping(_))
    ));
    assert_eq!(capture.state(), AndroidTouchpadCaptureState::Running);
    assert_eq!(capture.active_contact_ids(), &[1]);
    capture.stop(112).expect("recovering stop").expect("cancel");
}

#[test]
fn three_finger_policy_rebases_without_jump_and_attenuates_until_release() {
    let mut capture =
        AndroidTouchpadCaptureSession::new(stream(), surface(), 0).expect("capture session");
    capture.start(100).expect("start");

    let down = capture
        .map_motion(&event(
            110,
            AndroidMotionAction::Down,
            vec![finger(1, 100.0)],
        ))
        .expect("first down");
    assert_eq!(down.contacts[0].position.x_himetric, 1_000);

    capture
        .map_motion(&event(
            120,
            AndroidMotionAction::PointerDown { action_index: 1 },
            vec![finger(1, 100.0), finger(2, 400.0)],
        ))
        .expect("second down");
    let two_finger_move = capture
        .map_motion(&event(
            130,
            AndroidMotionAction::Move,
            vec![finger(1, 200.0), finger(2, 500.0)],
        ))
        .expect("identity two-finger move");
    assert_eq!(
        two_finger_move
            .contacts
            .iter()
            .map(|contact| contact.position.x_himetric)
            .collect::<Vec<_>>(),
        vec![2_000, 5_000]
    );

    let third_down = capture
        .map_motion(&event(
            140,
            AndroidMotionAction::PointerDown { action_index: 2 },
            vec![finger(1, 200.0), finger(2, 500.0), finger(3, 800.0)],
        ))
        .expect("third down");
    assert_eq!(
        third_down
            .contacts
            .iter()
            .map(|contact| contact.position.x_himetric)
            .collect::<Vec<_>>(),
        vec![2_000, 5_000, 8_000],
        "arming must not move any existing contact"
    );

    let attenuated = capture
        .map_motion(&event(
            150,
            AndroidMotionAction::Move,
            vec![finger(1, 300.0), finger(2, 600.0), finger(3, 900.0)],
        ))
        .expect("attenuated move");
    assert_eq!(
        attenuated
            .contacts
            .iter()
            .map(|contact| contact.position.x_himetric)
            .collect::<Vec<_>>(),
        vec![2_700, 5_700, 8_700]
    );

    capture
        .map_motion(&event(
            160,
            AndroidMotionAction::PointerUp { action_index: 2 },
            vec![finger(1, 350.0), finger(2, 650.0), finger(3, 950.0)],
        ))
        .expect("third pointer up");
    let still_attenuated = capture
        .map_motion(&event(
            170,
            AndroidMotionAction::Move,
            vec![finger(1, 450.0), finger(2, 750.0)],
        ))
        .expect("remaining contacts stay attenuated");
    assert_eq!(
        still_attenuated
            .contacts
            .iter()
            .map(|contact| contact.position.x_himetric)
            .collect::<Vec<_>>(),
        vec![3_750, 6_750]
    );

    capture
        .map_motion(&event(
            180,
            AndroidMotionAction::PointerUp { action_index: 1 },
            vec![finger(1, 450.0), finger(2, 750.0)],
        ))
        .expect("second pointer up");
    capture
        .map_motion(&event(
            190,
            AndroidMotionAction::Up { action_index: 0 },
            vec![finger(1, 450.0)],
        ))
        .expect("last pointer up");

    capture
        .map_motion(&event(
            200,
            AndroidMotionAction::Down,
            vec![finger(4, 100.0)],
        ))
        .expect("new gesture down");
    let new_one_finger_move = capture
        .map_motion(&event(
            210,
            AndroidMotionAction::Move,
            vec![finger(4, 300.0)],
        ))
        .expect("new gesture identity move");
    assert_eq!(new_one_finger_move.contacts[0].position.x_himetric, 3_000);
}

#[test]
fn multi_finger_motion_scale_is_bounded() {
    assert_eq!(
        AndroidTouchpadMotionPolicy::attenuated(700)
            .expect("bounded policy")
            .multi_finger_scale_per_mille(),
        700
    );
    assert!(AndroidTouchpadMotionPolicy::attenuated(0).is_err());
    assert!(AndroidTouchpadMotionPolicy::attenuated(1_001).is_err());
}
