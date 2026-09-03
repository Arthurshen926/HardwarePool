use capyio_input::{
    DpadState, GamepadButton, GamepadButtons, GamepadControls, SignedAxis, StickState, TriggerValue,
};
use capyio_viiper_adapter::{
    VIIPER_XBOX360_INPUT_STATE_BYTES, VIIPER_XBOX360_RUMBLE_FEEDBACK_BYTES, ViiperXbox360AxisSign,
    ViiperXbox360Error, ViiperXbox360Mapping, Xbox360RumbleFeedback, decode_xbox360_rumble,
    encode_xbox360_input_state,
};

const PRESERVE: ViiperXbox360Mapping = ViiperXbox360Mapping::preserve();

#[test]
fn neutral_report_has_exact_size_and_zeroes_reserved_tail() {
    let report = encode_xbox360_input_state(GamepadControls::neutral(), PRESERVE).unwrap();
    assert_eq!(report.len(), VIIPER_XBOX360_INPUT_STATE_BYTES);
    assert_eq!(report, [0; VIIPER_XBOX360_INPUT_STATE_BYTES]);
    assert_eq!(&report[14..], &[0; 6]);
}

#[test]
fn combined_state_matches_an_independently_constructed_wire_vector() {
    let controls = GamepadControls {
        buttons: GamepadButtons::empty()
            .with(GamepadButton::West)
            .with(GamepadButton::South)
            .with(GamepadButton::RightShoulder)
            .with(GamepadButton::Guide)
            .with(GamepadButton::Start)
            .with(GamepadButton::LeftStick),
        dpad: DpadState { x: 1, y: 1 },
        left_stick: StickState {
            x: SignedAxis::new(0x1234).unwrap(),
            y: SignedAxis::new(-0x1234).unwrap(),
        },
        right_stick: StickState {
            x: SignedAxis::new(i16::MAX).unwrap(),
            y: SignedAxis::new(-32_767).unwrap(),
        },
        left_trigger: TriggerValue::new(32_768),
        right_trigger: TriggerValue::new(u16::MAX),
    };
    assert_eq!(
        encode_xbox360_input_state(controls, PRESERVE).unwrap(),
        [
            0x59, 0x56, 0x00, 0x00, 0x80, 0xff, 0x34, 0x12, 0xcc, 0xed, 0xff, 0x7f, 0x00, 0x80,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]
    );
}

#[test]
fn every_representable_semantic_button_maps_to_the_pinned_mask() {
    for (button, expected) in [
        (GamepadButton::Start, 0x0010),
        (GamepadButton::Select, 0x0020),
        (GamepadButton::LeftStick, 0x0040),
        (GamepadButton::RightStick, 0x0080),
        (GamepadButton::LeftShoulder, 0x0100),
        (GamepadButton::RightShoulder, 0x0200),
        (GamepadButton::Guide, 0x0400),
        (GamepadButton::South, 0x1000),
        (GamepadButton::East, 0x2000),
        (GamepadButton::West, 0x4000),
        (GamepadButton::North, 0x8000),
    ] {
        let controls = GamepadControls {
            buttons: GamepadButtons::empty().with(button),
            ..GamepadControls::neutral()
        };
        let report = encode_xbox360_input_state(controls, PRESERVE).unwrap();
        assert_eq!(
            u32::from_le_bytes(report[0..4].try_into().unwrap()),
            expected
        );
        assert_eq!(&report[4..], &[0; 16]);
    }
}

#[test]
fn every_dpad_combination_and_axis_sign_is_explicit() {
    for x in -1..=1 {
        for y in -1..=1 {
            let controls = GamepadControls {
                dpad: DpadState { x, y },
                ..GamepadControls::neutral()
            };
            let report = encode_xbox360_input_state(controls, PRESERVE).unwrap();
            let expected = match y {
                1 => 0x0001,
                -1 => 0x0002,
                _ => 0,
            } | match x {
                -1 => 0x0004,
                1 => 0x0008,
                _ => 0,
            };
            assert_eq!(
                u32::from_le_bytes(report[0..4].try_into().unwrap()),
                expected
            );
        }
    }

    let invert_x = ViiperXbox360Mapping::new(
        ViiperXbox360AxisSign::Negative,
        ViiperXbox360AxisSign::Positive,
        ViiperXbox360AxisSign::Positive,
        ViiperXbox360AxisSign::Positive,
        ViiperXbox360AxisSign::Positive,
        ViiperXbox360AxisSign::Positive,
    );
    let controls = GamepadControls {
        dpad: DpadState { x: 1, y: 1 },
        ..GamepadControls::neutral()
    };
    let report = encode_xbox360_input_state(controls, invert_x).unwrap();
    assert_eq!(u32::from_le_bytes(report[0..4].try_into().unwrap()), 0x0005);

    let invert_y = ViiperXbox360Mapping::new(
        ViiperXbox360AxisSign::Positive,
        ViiperXbox360AxisSign::Negative,
        ViiperXbox360AxisSign::Positive,
        ViiperXbox360AxisSign::Positive,
        ViiperXbox360AxisSign::Positive,
        ViiperXbox360AxisSign::Positive,
    );
    let report = encode_xbox360_input_state(controls, invert_y).unwrap();
    assert_eq!(u32::from_le_bytes(report[0..4].try_into().unwrap()), 0x000a);
}

#[test]
fn sticks_and_triggers_cover_neutral_and_full_scale_endpoints() {
    let controls = GamepadControls {
        left_stick: StickState {
            x: SignedAxis::new(-32_767).unwrap(),
            y: SignedAxis::neutral(),
        },
        right_stick: StickState {
            x: SignedAxis::new(32_767).unwrap(),
            y: SignedAxis::new(-1).unwrap(),
        },
        left_trigger: TriggerValue::new(32_768),
        right_trigger: TriggerValue::new(u16::MAX),
        ..GamepadControls::neutral()
    };
    let report = encode_xbox360_input_state(controls, PRESERVE).unwrap();
    assert_eq!(report[4], 128);
    assert_eq!(report[5], 255);
    assert_eq!(read_axis(&report, 6), i16::MIN);
    assert_eq!(read_axis(&report, 8), 0);
    assert_eq!(read_axis(&report, 10), i16::MAX);
    assert_eq!(read_axis(&report, 12), -1);
    assert_eq!(&report[14..], &[0; 6]);

    let inverted = ViiperXbox360Mapping::new(
        ViiperXbox360AxisSign::Positive,
        ViiperXbox360AxisSign::Positive,
        ViiperXbox360AxisSign::Negative,
        ViiperXbox360AxisSign::Negative,
        ViiperXbox360AxisSign::Negative,
        ViiperXbox360AxisSign::Negative,
    );
    let report = encode_xbox360_input_state(controls, inverted).unwrap();
    assert_eq!(read_axis(&report, 6), i16::MAX);
    assert_eq!(read_axis(&report, 8), 0);
    assert_eq!(read_axis(&report, 10), i16::MIN);
    assert_eq!(read_axis(&report, 12), 1);
}

#[test]
fn each_stick_axis_sign_selector_changes_only_its_own_field() {
    let controls = GamepadControls {
        left_stick: StickState {
            x: SignedAxis::new(1_001).unwrap(),
            y: SignedAxis::new(2_002).unwrap(),
        },
        right_stick: StickState {
            x: SignedAxis::new(3_003).unwrap(),
            y: SignedAxis::new(4_004).unwrap(),
        },
        ..GamepadControls::neutral()
    };
    let expected_preserved: [i16; 4] = [1_001, 2_002, 3_003, 4_004];
    let signs = [
        [
            ViiperXbox360AxisSign::Negative,
            ViiperXbox360AxisSign::Positive,
            ViiperXbox360AxisSign::Positive,
            ViiperXbox360AxisSign::Positive,
        ],
        [
            ViiperXbox360AxisSign::Positive,
            ViiperXbox360AxisSign::Negative,
            ViiperXbox360AxisSign::Positive,
            ViiperXbox360AxisSign::Positive,
        ],
        [
            ViiperXbox360AxisSign::Positive,
            ViiperXbox360AxisSign::Positive,
            ViiperXbox360AxisSign::Negative,
            ViiperXbox360AxisSign::Positive,
        ],
        [
            ViiperXbox360AxisSign::Positive,
            ViiperXbox360AxisSign::Positive,
            ViiperXbox360AxisSign::Positive,
            ViiperXbox360AxisSign::Negative,
        ],
    ];
    for (inverted_index, axis_signs) in signs.into_iter().enumerate() {
        let mapping = ViiperXbox360Mapping::new(
            ViiperXbox360AxisSign::Positive,
            ViiperXbox360AxisSign::Positive,
            axis_signs[0],
            axis_signs[1],
            axis_signs[2],
            axis_signs[3],
        );
        let report = encode_xbox360_input_state(controls, mapping).unwrap();
        for (axis_index, expected) in expected_preserved.into_iter().enumerate() {
            let projected = read_axis(&report, 6 + axis_index * 2);
            assert_eq!(
                projected,
                if axis_index == inverted_index {
                    -expected
                } else {
                    expected
                }
            );
        }
    }
}

#[test]
fn trigger_scaling_has_fixed_rounding_at_boundaries() {
    for (value, expected) in [
        (0, 0),
        (1, 0),
        (32_767, 127),
        (32_768, 128),
        (65_534, 255),
        (u16::MAX, 255),
    ] {
        let left = GamepadControls {
            left_trigger: TriggerValue::new(value),
            ..GamepadControls::neutral()
        };
        let report = encode_xbox360_input_state(left, PRESERVE).unwrap();
        assert_eq!(report[4], expected);
        assert_eq!(report[5], 0);

        let right = GamepadControls {
            right_trigger: TriggerValue::new(value),
            ..GamepadControls::neutral()
        };
        let report = encode_xbox360_input_state(right, PRESERVE).unwrap();
        assert_eq!(report[4], 0);
        assert_eq!(report[5], expected);
    }
}

#[test]
fn unsupported_device_specific_buttons_fail_closed() {
    for button in [
        GamepadButton::Touchpad,
        GamepadButton::Paddle1,
        GamepadButton::Paddle2,
        GamepadButton::Paddle3,
        GamepadButton::Paddle4,
    ] {
        let controls = GamepadControls {
            buttons: GamepadButtons::empty().with(button),
            ..GamepadControls::neutral()
        };
        assert_eq!(
            encode_xbox360_input_state(controls, PRESERVE),
            Err(ViiperXbox360Error::UnsupportedButtons(1 << button as u8))
        );
    }

    let mixed = GamepadControls {
        buttons: GamepadButtons::empty()
            .with(GamepadButton::South)
            .with(GamepadButton::Paddle2)
            .with(GamepadButton::Paddle4),
        ..GamepadControls::neutral()
    };
    assert_eq!(
        encode_xbox360_input_state(mixed, PRESERVE),
        Err(ViiperXbox360Error::UnsupportedButtons(
            (1 << GamepadButton::Paddle2 as u8) | (1 << GamepadButton::Paddle4 as u8)
        ))
    );

    let invalid = GamepadControls {
        dpad: DpadState { x: 2, y: 0 },
        ..GamepadControls::neutral()
    };
    assert_eq!(
        encode_xbox360_input_state(invalid, PRESERVE),
        Err(ViiperXbox360Error::InvalidControls)
    );
}

#[test]
fn rumble_feedback_requires_exactly_two_bytes_and_preserves_intensity() {
    assert_eq!(VIIPER_XBOX360_RUMBLE_FEEDBACK_BYTES, 2);
    assert_eq!(
        decode_xbox360_rumble(&[0, 0]).unwrap(),
        Xbox360RumbleFeedback {
            left_motor: 0,
            right_motor: 0,
        }
    );
    assert!(decode_xbox360_rumble(&[0, 0]).unwrap().is_neutral());
    assert_eq!(
        decode_xbox360_rumble(&[17, 255]).unwrap(),
        Xbox360RumbleFeedback {
            left_motor: 17,
            right_motor: 255,
        }
    );
    assert_eq!(
        decode_xbox360_rumble(&[255, 0]).unwrap(),
        Xbox360RumbleFeedback {
            left_motor: 255,
            right_motor: 0,
        }
    );
    assert_eq!(
        decode_xbox360_rumble(&[0, 255]).unwrap(),
        Xbox360RumbleFeedback {
            left_motor: 0,
            right_motor: 255,
        }
    );
    for report in [
        &[][..],
        &[1][..],
        &[1, 2, 3][..],
        &[0; VIIPER_XBOX360_INPUT_STATE_BYTES][..],
    ] {
        assert_eq!(
            decode_xbox360_rumble(report),
            Err(ViiperXbox360Error::UnexpectedRumbleLength {
                actual: report.len(),
                expected: 2,
            })
        );
    }
}

fn read_axis(report: &[u8; VIIPER_XBOX360_INPUT_STATE_BYTES], offset: usize) -> i16 {
    i16::from_le_bytes(report[offset..offset + 2].try_into().unwrap())
}
