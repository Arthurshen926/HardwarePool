use capyio_core::StreamId;
use capyio_input::{
    InputContractError, InputFrameHeader, InputStreamDescriptor, NormalizedMagnitude, SequenceGap,
    TouchpadButtonState, TouchpadButtonType, TouchpadContact, TouchpadContactSize,
    TouchpadDescriptor, TouchpadFixture, TouchpadFrame, TouchpadFrameKind, TouchpadFrameOutcome,
    TouchpadFrameTracker, TouchpadMetrics, TouchpadPhysicalSize, TouchpadPosition,
};

const FIXTURE: &str = include_str!("../../../fixtures/input/touchpad_frames_v1.json");

fn stream(epoch: u64) -> InputStreamDescriptor {
    InputStreamDescriptor {
        stream_id: "00000000-0000-4000-8000-00000000c301"
            .parse()
            .expect("fixture StreamId"),
        stream_epoch: epoch,
        clock_domain_id: "fixture.touchpad.monotonic".to_owned(),
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
        reports_contact_size: true,
        reports_pressure: true,
    }
}

fn contact(contact_id: u32, x_himetric: u32, y_himetric: u32) -> TouchpadContact {
    TouchpadContact {
        contact_id,
        position: TouchpadPosition {
            x_himetric,
            y_himetric,
        },
        confidence: true,
        size: Some(TouchpadContactSize {
            width_himetric: 700,
            height_himetric: 800,
        }),
        pressure: Some(NormalizedMagnitude::new(12_000)),
    }
}

fn frame(
    epoch: u64,
    sequence: u64,
    timestamp: u64,
    kind: TouchpadFrameKind,
    contacts: Vec<TouchpadContact>,
) -> TouchpadFrame {
    TouchpadFrame {
        header: InputFrameHeader {
            stream_id: stream(epoch).stream_id,
            stream_epoch: epoch,
            sequence,
            source_timestamp_nanos: timestamp,
        },
        kind,
        button: TouchpadButtonState::Released,
        contacts,
    }
}

#[test]
fn profile_and_format_are_distinct_from_generic_touch() {
    let touch = capyio_input::touch_events_profile();
    let touchpad = capyio_input::touchpad_frames_profile();
    assert_ne!(touch, touchpad);
    assert_eq!(touchpad.name, "capyio.input.touchpad-frames");
    assert_eq!(touchpad.major, 1);
    assert_eq!(
        capyio_input::touchpad_frame_format().id,
        "touchpad-frame-v1"
    );
}

#[test]
fn descriptor_requires_physical_size_and_three_to_five_contacts() {
    descriptor().validate().expect("five-contact descriptor");
    for max_contacts in [0, 1, 2, 6, u8::MAX] {
        let invalid = TouchpadDescriptor {
            max_contacts,
            ..descriptor()
        };
        assert!(matches!(
            invalid.validate(),
            Err(InputContractError::InvalidTouchpadDescriptor(_))
        ));
    }
    let missing_size = TouchpadDescriptor {
        physical_size: TouchpadPhysicalSize {
            width_himetric: 0,
            height_himetric: 6_000,
        },
        ..descriptor()
    };
    assert!(matches!(
        missing_size.validate(),
        Err(InputContractError::InvalidTouchpadDescriptor(_))
    ));
}

#[test]
fn frame_validation_rejects_ambiguous_or_undeclared_state() {
    let descriptor = descriptor();
    let duplicate = frame(
        1,
        0,
        1,
        TouchpadFrameKind::Update,
        vec![contact(1, 100, 100), contact(1, 200, 200)],
    );
    assert!(matches!(
        duplicate.validate(&descriptor),
        Err(InputContractError::InvalidTouchpadFrame(_))
    ));

    let overflow = frame(
        1,
        0,
        1,
        TouchpadFrameKind::Update,
        (0..6).map(|id| contact(id, 100, 100)).collect(),
    );
    assert!(matches!(
        overflow.validate(&descriptor),
        Err(InputContractError::InvalidTouchpadFrame(_))
    ));

    let outside = frame(
        1,
        0,
        1,
        TouchpadFrameKind::Update,
        vec![contact(1, 10_001, 100)],
    );
    assert!(matches!(
        outside.validate(&descriptor),
        Err(InputContractError::InvalidTouchpadFrame(_))
    ));

    let no_size = TouchpadDescriptor {
        reports_contact_size: false,
        ..descriptor
    };
    assert!(matches!(
        frame(
            1,
            0,
            1,
            TouchpadFrameKind::Update,
            vec![contact(1, 100, 100)]
        )
        .validate(&no_size),
        Err(InputContractError::InvalidTouchpadFrame(_))
    ));

    let no_pressure = TouchpadDescriptor {
        reports_pressure: false,
        ..descriptor
    };
    let mut pressure_only = contact(1, 100, 100);
    pressure_only.size = None;
    assert!(matches!(
        frame(1, 0, 1, TouchpadFrameKind::Update, vec![pressure_only]).validate(&no_pressure),
        Err(InputContractError::InvalidTouchpadFrame(_))
    ));

    let mut malformed_cancel = frame(
        1,
        0,
        1,
        TouchpadFrameKind::CancelAll,
        vec![contact(1, 100, 100)],
    );
    malformed_cancel.button = TouchpadButtonState::Pressed;
    assert!(matches!(
        malformed_cancel.validate(&descriptor),
        Err(InputContractError::InvalidTouchpadFrame(_))
    ));

    let mut impossible_button = frame(1, 0, 1, TouchpadFrameKind::Update, Vec::new());
    impossible_button.button = TouchpadButtonState::Pressed;
    assert!(matches!(
        impossible_button.validate(&descriptor),
        Err(InputContractError::InvalidTouchpadFrame(_))
    ));
}

#[test]
fn gap_and_epoch_changes_suppress_updates_until_cancel_all() {
    let descriptor = descriptor();
    let mut tracker = TouchpadFrameTracker::new(&stream(1), 0).expect("tracker");

    assert_eq!(
        tracker
            .observe(
                &frame(
                    1,
                    0,
                    100,
                    TouchpadFrameKind::Update,
                    vec![contact(1, 100, 100)]
                ),
                &descriptor
            )
            .expect("initial suppressed frame"),
        TouchpadFrameOutcome::SuppressedUntilCancelAll
    );
    assert_eq!(
        tracker
            .observe(
                &frame(1, 1, 110, TouchpadFrameKind::CancelAll, Vec::new()),
                &descriptor
            )
            .expect("initial cancel"),
        TouchpadFrameOutcome::Cancelled
    );
    assert_eq!(
        tracker
            .observe(
                &frame(
                    1,
                    2,
                    120,
                    TouchpadFrameKind::Update,
                    vec![contact(1, 200, 200)]
                ),
                &descriptor
            )
            .expect("applied"),
        TouchpadFrameOutcome::Applied
    );
    assert_eq!(
        tracker
            .observe(
                &frame(
                    1,
                    4,
                    140,
                    TouchpadFrameKind::Update,
                    vec![contact(1, 300, 300)]
                ),
                &descriptor
            )
            .expect("gap"),
        TouchpadFrameOutcome::GapRequiresCancelAll(SequenceGap {
            first_missing: 3,
            last_missing: 3,
        })
    );
    assert_eq!(
        tracker
            .observe(
                &frame(
                    1,
                    5,
                    150,
                    TouchpadFrameKind::Update,
                    vec![contact(1, 400, 400)]
                ),
                &descriptor
            )
            .expect("suppressed"),
        TouchpadFrameOutcome::SuppressedUntilCancelAll
    );
    assert_eq!(
        tracker
            .observe(
                &frame(1, 6, 160, TouchpadFrameKind::CancelAll, Vec::new()),
                &descriptor
            )
            .expect("recovery cancel"),
        TouchpadFrameOutcome::Cancelled
    );

    tracker.advance_epoch(2, 100).expect("new epoch");
    assert_eq!(
        tracker
            .observe(
                &frame(
                    2,
                    100,
                    200,
                    TouchpadFrameKind::Update,
                    vec![contact(2, 500, 500)]
                ),
                &descriptor
            )
            .expect("new epoch suppressed"),
        TouchpadFrameOutcome::SuppressedUntilCancelAll
    );
    assert_eq!(
        tracker
            .observe(
                &frame(2, 101, 210, TouchpadFrameKind::CancelAll, Vec::new()),
                &descriptor
            )
            .expect("new epoch cancel"),
        TouchpadFrameOutcome::Cancelled
    );
}

#[test]
fn timestamp_failure_is_transactional() {
    let descriptor = descriptor();
    let mut tracker = TouchpadFrameTracker::new(&stream(1), 0).expect("tracker");
    tracker
        .observe(
            &frame(1, 0, 100, TouchpadFrameKind::CancelAll, Vec::new()),
            &descriptor,
        )
        .expect("cancel");
    assert!(matches!(
        tracker.observe(
            &frame(1, 1, 99, TouchpadFrameKind::Update, Vec::new()),
            &descriptor
        ),
        Err(InputContractError::TouchpadTimestampRegression { .. })
    ));
    assert_eq!(
        tracker
            .observe(
                &frame(1, 1, 101, TouchpadFrameKind::Update, Vec::new()),
                &descriptor
            )
            .expect("same sequence after transactional error"),
        TouchpadFrameOutcome::Applied
    );
}

#[test]
fn committed_fixture_is_bounded_deterministic_and_released() {
    let fixture: TouchpadFixture = serde_json::from_str(FIXTURE).expect("fixture JSON");
    let metrics = fixture.validate().expect("fixture contract");
    assert_eq!(
        metrics,
        TouchpadMetrics {
            frames_observed: 8,
            contact_samples_observed: 13,
            peak_contacts: 5,
            sequence_gaps: 0,
            cancel_all_frames: 2,
            suppressed_frames: 0,
        }
    );
    assert!(fixture.frames.last().expect("last frame").is_released());
    let encoded = serde_json::to_string(&fixture).expect("diagnostic JSON");
    let decoded: TouchpadFixture = serde_json::from_str(&encoded).expect("round trip");
    assert_eq!(fixture, decoded);
}

#[test]
fn fixture_rejects_unreleased_tail_and_unknown_fields() {
    let mut fixture: TouchpadFixture = serde_json::from_str(FIXTURE).expect("fixture JSON");
    let last = fixture.frames.last_mut().expect("last frame");
    last.kind = TouchpadFrameKind::Update;
    last.contacts.push(contact(99, 100, 100));
    assert!(matches!(
        fixture.validate(),
        Err(InputContractError::InvalidTouchpadFixture(_))
    ));

    let unknown = FIXTURE.replacen(
        "\"clock_domain_id\": \"fixture.touchpad.monotonic\"",
        "\"clock_domain_id\": \"fixture.touchpad.monotonic\", \"unknown\": true",
        1,
    );
    assert!(serde_json::from_str::<TouchpadFixture>(&unknown).is_err());
}

#[test]
fn frame_rejects_wrong_stream_through_tracker() {
    let descriptor = descriptor();
    let mut tracker = TouchpadFrameTracker::new(&stream(1), 0).expect("tracker");
    let mut wrong = frame(1, 0, 1, TouchpadFrameKind::CancelAll, Vec::new());
    wrong.header.stream_id = StreamId::new();
    assert!(matches!(
        tracker.observe(&wrong, &descriptor),
        Err(InputContractError::WrongStream { .. })
    ));
}
