use capyio_core::StreamId;
use capyio_input::{
    DpadState, GamepadButton, GamepadButtons, GamepadControls, GamepadState, HapticsCommand,
    HapticsEffect, InputContractError, InputFrameHeader, InputSequenceOutcome,
    InputSequenceTracker, InputStreamDescriptor, KeyEvent, KeyPhase, KeyboardFrame,
    NormalizedMagnitude, NormalizedPosition, PhysicalKey, PointerEvent, PointerFrame, SignedAxis,
    StickState, TouchContact, TouchFrame, TriggerValue,
};

#[test]
fn canonical_profile_and_format_helpers_are_valid() {
    for profile in [
        capyio_input::key_events_profile(),
        capyio_input::pointer_events_profile(),
        capyio_input::touch_events_profile(),
        capyio_input::gamepad_state_profile(),
        capyio_input::haptics_feedback_profile(),
    ] {
        profile.validate().expect("Profile ID");
    }
    for format in [
        capyio_input::key_events_format(),
        capyio_input::pointer_events_format(),
        capyio_input::touch_snapshot_format(),
        capyio_input::gamepad_state_format(),
        capyio_input::dual_rumble_format(),
    ] {
        format.validate().expect("format descriptor");
    }
}

fn header(stream_id: StreamId, epoch: u64, sequence: u64) -> InputFrameHeader {
    InputFrameHeader {
        stream_id,
        stream_epoch: epoch,
        sequence,
        source_timestamp_nanos: sequence,
    }
}

#[test]
fn stream_clock_is_validated_once_and_sequence_guard_fails_closed() {
    let stream_id = StreamId::new();
    InputStreamDescriptor {
        stream_id,
        stream_epoch: 1,
        clock_domain_id: "android.elapsed_realtime".to_owned(),
    }
    .validate()
    .expect("stream descriptor");

    let mut tracker = InputSequenceTracker::new(stream_id, 1, 10).expect("tracker");
    assert_eq!(
        tracker.observe(header(stream_id, 1, 10)).expect("first"),
        InputSequenceOutcome::InOrder
    );
    assert!(matches!(
        tracker.observe(header(stream_id, 1, 10)),
        Err(InputContractError::DuplicateOrLate { .. })
    ));
    assert_eq!(
        tracker.observe(header(stream_id, 1, 13)).expect("gap"),
        InputSequenceOutcome::Gap(capyio_input::SequenceGap {
            first_missing: 11,
            last_missing: 12,
        })
    );
    assert!(matches!(
        tracker.observe(header(StreamId::new(), 1, 14)),
        Err(InputContractError::WrongStream { .. })
    ));
    assert!(matches!(
        tracker.observe(header(stream_id, 0, 14)),
        Err(InputContractError::InvalidHeader(_))
    ));
    assert!(matches!(
        tracker.observe(header(stream_id, 2, 14)),
        Err(InputContractError::FutureEpoch { .. })
    ));
    tracker.advance_epoch(2, 0).expect("new epoch");
    assert!(matches!(
        tracker.observe(header(stream_id, 1, 14)),
        Err(InputContractError::StaleEpoch { .. })
    ));

    let mut exhausted = InputSequenceTracker::new(stream_id, 3, u64::MAX).expect("tracker");
    exhausted
        .observe(header(stream_id, 3, u64::MAX))
        .expect("last sequence");
    assert!(matches!(
        exhausted.observe(header(stream_id, 3, u64::MAX)),
        Err(InputContractError::SequenceExhausted)
    ));
}

#[test]
fn pointer_frames_are_bounded_and_reset_is_explicit() {
    let stream_id = StreamId::new();
    PointerFrame {
        header: header(stream_id, 1, 1),
        events: vec![PointerEvent::RelativeMotion {
            delta_x: 12,
            delta_y: -4,
        }],
    }
    .validate()
    .expect("relative pointer frame");

    let invalid_reset = PointerFrame {
        header: header(stream_id, 1, 2),
        events: vec![
            PointerEvent::Reset,
            PointerEvent::AbsoluteMotion {
                position: NormalizedPosition::new(0, 0),
            },
        ],
    };
    assert!(matches!(
        invalid_reset.validate(),
        Err(InputContractError::InvalidPointerFrame(_))
    ));

    let oversized = PointerFrame {
        header: header(stream_id, 1, 3),
        events: vec![PointerEvent::Reset; 65],
    };
    assert!(matches!(
        oversized.validate(),
        Err(InputContractError::InvalidPointerFrame(_))
    ));
}

#[test]
fn touch_frame_is_a_complete_snapshot_and_empty_releases_all_contacts() {
    let stream_id = StreamId::new();
    TouchFrame {
        header: header(stream_id, 1, 1),
        contacts: Vec::new(),
    }
    .validate()
    .expect("empty release snapshot");

    let contact = TouchContact {
        contact_id: 9,
        position: NormalizedPosition::new(1000, 2000),
        pressure: Some(NormalizedMagnitude::new(12_000)),
    };
    let duplicate = TouchFrame {
        header: header(stream_id, 1, 2),
        contacts: vec![contact, contact],
    };
    assert!(matches!(
        duplicate.validate(),
        Err(InputContractError::InvalidTouchFrame(_))
    ));

    let oversized = TouchFrame {
        header: header(stream_id, 1, 3),
        contacts: (0..33)
            .map(|contact_id| TouchContact {
                contact_id,
                position: NormalizedPosition::new(0, 0),
                pressure: None,
            })
            .collect(),
    };
    assert!(matches!(
        oversized.validate(),
        Err(InputContractError::InvalidTouchFrame(_))
    ));
}

#[test]
fn keyboard_uses_semantic_keys_and_reset_cannot_mix_with_transitions() {
    let stream_id = StreamId::new();
    KeyboardFrame {
        header: header(stream_id, 1, 1),
        events: vec![KeyEvent::Transition {
            key: PhysicalKey::KeyA,
            phase: KeyPhase::Pressed,
            repeat: false,
        }],
    }
    .validate()
    .expect("keyboard frame");

    let invalid = KeyboardFrame {
        header: header(stream_id, 1, 2),
        events: vec![KeyEvent::Transition {
            key: PhysicalKey::KeyA,
            phase: KeyPhase::Released,
            repeat: true,
        }],
    };
    assert!(matches!(
        invalid.validate(),
        Err(InputContractError::InvalidKeyboardFrame(_))
    ));
}

#[test]
fn gamepad_state_uses_fixed_buttons_and_neutral_is_explicit() {
    let stream_id = StreamId::new();
    let controls = GamepadControls {
        buttons: GamepadButtons::empty().with(GamepadButton::South),
        ..GamepadControls::neutral()
    };
    assert!(controls.buttons.contains(GamepadButton::South));
    GamepadState {
        header: header(stream_id, 1, 1),
        controls,
    }
    .validate()
    .expect("gamepad state");

    let invalid_axis_json = format!(
        "{{\"header\":{{\"stream_id\":\"{}\",\"stream_epoch\":1,\"sequence\":2,\"source_timestamp_nanos\":2}},\"controls\":{{\"buttons\":65536,\"dpad\":{{\"x\":0,\"y\":0}},\"left_stick\":{{\"x\":-32768,\"y\":0}},\"right_stick\":{{\"x\":0,\"y\":0}},\"left_trigger\":0,\"right_trigger\":0}}}}",
        StreamId::new()
    );
    let decoded: GamepadState = serde_json::from_str(&invalid_axis_json).expect("shape decodes");
    assert!(matches!(
        decoded.validate(),
        Err(InputContractError::InvalidGamepadState(_))
    ));

    let invalid_axis: SignedAxis = serde_json::from_str("-32768").expect("shape decodes");
    assert!(matches!(
        invalid_axis.validate(),
        Err(InputContractError::InvalidGamepadState(_))
    ));

    let invalid_dpad = GamepadState {
        header: header(stream_id, 1, 3),
        controls: GamepadControls {
            buttons: GamepadButtons::empty(),
            dpad: DpadState { x: 2, y: 0 },
            left_stick: StickState::default(),
            right_stick: StickState::default(),
            left_trigger: TriggerValue::idle(),
            right_trigger: TriggerValue::idle(),
        },
    };
    assert!(matches!(
        invalid_dpad.validate(),
        Err(InputContractError::InvalidGamepadState(_))
    ));

    assert_eq!(SignedAxis::new(-32_767).expect("edge").get(), -32_767);
}

#[test]
fn haptics_stop_is_distinct_from_bounded_nonzero_rumble() {
    let stream_id = StreamId::new();
    HapticsCommand {
        header: header(stream_id, 1, 1),
        effect: HapticsEffect::Stop,
    }
    .validate()
    .expect("stop");

    let invalid = HapticsCommand {
        header: header(stream_id, 1, 2),
        effect: HapticsEffect::Rumble {
            low_frequency: NormalizedMagnitude::idle(),
            high_frequency: NormalizedMagnitude::idle(),
            duration_millis: 10,
        },
    };
    assert!(matches!(
        invalid.validate(),
        Err(InputContractError::InvalidHapticsCommand(_))
    ));

    let too_long = HapticsCommand {
        header: header(stream_id, 1, 3),
        effect: HapticsEffect::Rumble {
            low_frequency: NormalizedMagnitude::new(1),
            high_frequency: NormalizedMagnitude::idle(),
            duration_millis: 10_001,
        },
    };
    assert!(matches!(
        too_long.validate(),
        Err(InputContractError::InvalidHapticsCommand(_))
    ));
}
