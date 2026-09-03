use capyio_core::StreamId;
use capyio_input::{
    DpadState, GamepadButton, GamepadButtons, GamepadControlUpdate, GamepadControls,
    GamepadStateComposer, GamepadStick, GamepadTrigger, InputContractError, SignedAxis, StickState,
    TriggerValue,
};

#[test]
fn semantic_updates_accumulate_as_complete_sequenced_snapshots() {
    let stream_id = StreamId::new();
    let mut composer = GamepadStateComposer::new(stream_id, 3, 10).unwrap();
    let anchor = composer.anchor(999).unwrap();
    assert_eq!(anchor.header.stream_id, composer.stream_id());
    assert_eq!(anchor.header.stream_epoch, composer.stream_epoch());
    assert_eq!(anchor.header.sequence, 10);
    assert_eq!(anchor.controls, GamepadControls::neutral());
    assert_eq!(composer.next_sequence(), Some(10));

    let pressed = composer
        .apply(
            GamepadControlUpdate::Button {
                button: GamepadButton::South,
                pressed: true,
            },
            1_000,
        )
        .unwrap();
    assert_eq!(pressed.header.stream_id, stream_id);
    assert_eq!(pressed.header.stream_epoch, 3);
    assert_eq!(pressed.header.sequence, 10);
    assert_eq!(pressed.header.source_timestamp_nanos, 1_000);
    assert!(pressed.controls.buttons.contains(GamepadButton::South));

    let dpad = composer
        .apply(GamepadControlUpdate::Dpad(DpadState { x: -1, y: 1 }), 1_001)
        .unwrap();
    assert_eq!(dpad.header.sequence, 11);
    assert!(dpad.controls.buttons.contains(GamepadButton::South));
    assert_eq!(dpad.controls.dpad, DpadState { x: -1, y: 1 });

    let left_stick = StickState {
        x: SignedAxis::new(12_345).unwrap(),
        y: SignedAxis::new(-23_456).unwrap(),
    };
    let stick = composer
        .apply(
            GamepadControlUpdate::Stick {
                stick: GamepadStick::Left,
                state: left_stick,
            },
            1_002,
        )
        .unwrap();
    assert_eq!(stick.header.sequence, 12);
    assert_eq!(stick.controls.left_stick, left_stick);
    assert_eq!(stick.controls.dpad, dpad.controls.dpad);

    let trigger = composer
        .apply(
            GamepadControlUpdate::Trigger {
                trigger: GamepadTrigger::Right,
                value: TriggerValue::new(40_000),
            },
            1_003,
        )
        .unwrap();
    assert_eq!(trigger.header.sequence, 13);
    assert_eq!(trigger.controls.right_trigger.get(), 40_000);
    assert_eq!(trigger.controls.left_stick, left_stick);
    assert_eq!(composer.controls(), trigger.controls);
    assert_eq!(composer.next_sequence(), Some(14));
}

#[test]
fn button_release_and_reset_preserve_complete_state_rules() {
    let mut composer = GamepadStateComposer::new(StreamId::new(), 1, 0).unwrap();
    for button in [GamepadButton::South, GamepadButton::Start] {
        composer
            .apply(
                GamepadControlUpdate::Button {
                    button,
                    pressed: true,
                },
                1,
            )
            .unwrap();
    }
    composer
        .apply(GamepadControlUpdate::Dpad(DpadState { x: 1, y: -1 }), 1)
        .unwrap();
    let right_stick = StickState {
        x: SignedAxis::new(-12_000).unwrap(),
        y: SignedAxis::new(8_000).unwrap(),
    };
    composer
        .apply(
            GamepadControlUpdate::Stick {
                stick: GamepadStick::Right,
                state: right_stick,
            },
            1,
        )
        .unwrap();
    composer
        .apply(
            GamepadControlUpdate::Trigger {
                trigger: GamepadTrigger::Left,
                value: TriggerValue::new(33_000),
            },
            1,
        )
        .unwrap();
    let expected_before_release = GamepadControls {
        buttons: GamepadButtons::empty()
            .with(GamepadButton::South)
            .with(GamepadButton::Start),
        dpad: DpadState { x: 1, y: -1 },
        right_stick,
        left_trigger: TriggerValue::new(33_000),
        ..GamepadControls::neutral()
    };
    assert_eq!(composer.controls(), expected_before_release);

    let released = composer
        .apply(
            GamepadControlUpdate::Button {
                button: GamepadButton::South,
                pressed: false,
            },
            2,
        )
        .unwrap();
    assert!(!released.controls.buttons.contains(GamepadButton::South));
    assert!(released.controls.buttons.contains(GamepadButton::Start));
    assert_eq!(released.controls.dpad, expected_before_release.dpad);
    assert_eq!(released.controls.right_stick, right_stick);
    assert_eq!(released.controls.left_trigger.get(), 33_000);

    let reset = composer.apply(GamepadControlUpdate::Reset, 3).unwrap();
    assert_eq!(reset.controls, GamepadControls::neutral());
    let repeated_reset = composer.apply(GamepadControlUpdate::Reset, 4).unwrap();
    assert_eq!(repeated_reset.controls, GamepadControls::neutral());
    assert_eq!(repeated_reset.header.sequence, reset.header.sequence + 1);
}

#[test]
fn invalid_update_is_transactional_and_does_not_consume_sequence() {
    let mut composer = GamepadStateComposer::new(StreamId::new(), 1, 7).unwrap();
    composer
        .apply(
            GamepadControlUpdate::Button {
                button: GamepadButton::East,
                pressed: true,
            },
            1,
        )
        .unwrap();
    let retained = composer.controls();
    let before_invalid = composer;

    assert!(matches!(
        composer.apply(GamepadControlUpdate::Dpad(DpadState { x: 2, y: 0 }), 2,),
        Err(InputContractError::InvalidGamepadState(_))
    ));
    assert_eq!(composer, before_invalid);
    assert_eq!(composer.controls(), retained);
    assert_eq!(composer.next_sequence(), Some(8));

    let recovered = composer
        .apply(GamepadControlUpdate::Dpad(DpadState { x: 1, y: 0 }), 3)
        .unwrap();
    assert_eq!(recovered.header.sequence, 8);
    assert!(recovered.controls.buttons.contains(GamepadButton::East));
}

#[test]
fn epoch_and_sequence_exhaustion_fail_without_mutating_controls() {
    assert!(matches!(
        GamepadStateComposer::new(StreamId::new(), 0, 0),
        Err(InputContractError::InvalidStream(_))
    ));

    let mut composer = GamepadStateComposer::new(StreamId::new(), 1, u64::MAX).unwrap();
    let terminal = composer
        .apply(
            GamepadControlUpdate::Button {
                button: GamepadButton::North,
                pressed: true,
            },
            1,
        )
        .unwrap();
    assert_eq!(terminal.header.sequence, u64::MAX);
    assert_eq!(composer.next_sequence(), None);
    assert_eq!(
        composer.anchor(2),
        Err(InputContractError::SequenceExhausted)
    );
    let retained = composer.controls();
    let exhausted = composer;
    assert_eq!(
        composer.apply(GamepadControlUpdate::Reset, 2),
        Err(InputContractError::SequenceExhausted)
    );
    assert_eq!(
        composer.apply(
            GamepadControlUpdate::Button {
                button: GamepadButton::South,
                pressed: true,
            },
            3,
        ),
        Err(InputContractError::SequenceExhausted)
    );
    assert_eq!(composer, exhausted);
    assert_eq!(composer.controls(), retained);
}
