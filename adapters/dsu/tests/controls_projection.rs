use capyio_data_plane::parse_imu_fixture_jsonl;
use capyio_dsu_adapter::{
    AxisSign, DsuControlsMapping, DsuFaceButtonLayout, DsuMotionMapping, DsuPacketError,
    crc32_ieee, encode_neutral_pad_data, encode_pad_data, project_imu_envelope,
};
use capyio_input::{
    DpadState, GamepadButton, GamepadButtons, GamepadControls, SignedAxis, StickState, TriggerValue,
};

const FIXTURE: &str = include_str!("../../../fixtures/imu/imu_samples_v1.jsonl");

fn motion() -> capyio_dsu_adapter::DsuMotionSample {
    let envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
    project_imu_envelope(&envelope, DsuMotionMapping::identity()).unwrap()
}

fn packet(controls: GamepadControls) -> [u8; 100] {
    encode_pad_data(
        0x0102_0304,
        0,
        9,
        motion(),
        controls,
        DsuControlsMapping::dualshock_physical(),
    )
    .unwrap()
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

#[test]
fn every_supported_semantic_button_maps_to_its_dsu_field() {
    for (button, digital_offset, mask, analog_offset) in [
        (GamepadButton::South, 37, 1 << 6, Some(49)),
        (GamepadButton::East, 37, 1 << 5, Some(50)),
        (GamepadButton::West, 37, 1 << 7, Some(48)),
        (GamepadButton::North, 37, 1 << 4, Some(51)),
        (GamepadButton::LeftShoulder, 37, 1 << 2, Some(53)),
        (GamepadButton::RightShoulder, 37, 1 << 3, Some(52)),
        (GamepadButton::LeftStick, 36, 1 << 1, None),
        (GamepadButton::RightStick, 36, 1 << 2, None),
        (GamepadButton::Select, 36, 1, None),
        (GamepadButton::Start, 36, 1 << 3, None),
        (GamepadButton::Guide, 38, 1, None),
        (GamepadButton::Touchpad, 39, 1, None),
    ] {
        let controls = GamepadControls {
            buttons: GamepadButtons::empty().with(button),
            ..GamepadControls::neutral()
        };
        let packet = packet(controls);
        assert_eq!(packet[digital_offset], mask, "button {button:?}");
        if let Some(offset) = analog_offset {
            assert_eq!(packet[offset], u8::MAX, "button {button:?}");
        }
    }
}

#[test]
fn protocol_named_face_layout_is_separate_from_dualshock_physical_layout() {
    for (button, mask, analog_offset) in [
        (GamepadButton::North, 1 << 7, 48),
        (GamepadButton::East, 1 << 6, 49),
        (GamepadButton::South, 1 << 5, 50),
        (GamepadButton::West, 1 << 4, 51),
    ] {
        let controls = GamepadControls {
            buttons: GamepadButtons::empty().with(button),
            ..GamepadControls::neutral()
        };
        let packet =
            encode_pad_data(7, 0, 0, motion(), controls, DsuControlsMapping::identity()).unwrap();
        assert_eq!(packet[37], mask, "button {button:?}");
        assert_eq!(packet[analog_offset], u8::MAX, "button {button:?}");
    }
}

#[test]
fn dpad_sticks_and_triggers_use_explicit_full_range_scaling() {
    let controls = GamepadControls {
        dpad: DpadState { x: -1, y: 1 },
        left_stick: StickState {
            x: SignedAxis::new(-32_767).unwrap(),
            y: SignedAxis::new(32_767).unwrap(),
        },
        right_stick: StickState {
            x: SignedAxis::neutral(),
            y: SignedAxis::new(-16_384).unwrap(),
        },
        left_trigger: TriggerValue::new(32_768),
        right_trigger: TriggerValue::new(u16::MAX),
        ..GamepadControls::neutral()
    };
    let neutral_packet = packet(GamepadControls::neutral());
    let packet = packet(controls);

    assert_eq!(packet[36], 0b1001_0000);
    assert_eq!(packet[37], 0b0000_0011);
    assert_eq!(&packet[40..44], &[0, 255, 128, 64]);
    assert_eq!(&packet[44..48], &[255, 0, 0, 255]);
    assert_eq!(packet[54], 255);
    assert_eq!(packet[55], 128);
    assert!(packet[56..68].iter().all(|byte| *byte == 0));
    assert_eq!(packet[68..], neutral_packet[68..]);
}

#[test]
fn every_valid_dpad_combination_sets_matching_digital_and_analog_fields() {
    for x in -1_i8..=1 {
        for y in -1_i8..=1 {
            let packet = packet(GamepadControls {
                dpad: DpadState { x, y },
                ..GamepadControls::neutral()
            });
            let expected_digital = if x < 0 { 1 << 7 } else { 0 }
                | if y < 0 { 1 << 6 } else { 0 }
                | if x > 0 { 1 << 5 } else { 0 }
                | if y > 0 { 1 << 4 } else { 0 };
            assert_eq!(packet[36], expected_digital, "dpad ({x}, {y})");
            assert_eq!(packet[44], if x < 0 { 255 } else { 0 });
            assert_eq!(packet[45], if y < 0 { 255 } else { 0 });
            assert_eq!(packet[46], if x > 0 { 255 } else { 0 });
            assert_eq!(packet[47], if y > 0 { 255 } else { 0 });
        }
    }
}

#[test]
fn trigger_rounding_and_digital_nonzero_threshold_are_fixed() {
    for (value, expected_analog, expected_digital) in [
        (0, 0, 0),
        (1, 0, 1),
        (32_767, 127, 1),
        (32_768, 128, 1),
        (u16::MAX, 255, 1),
    ] {
        let right = packet(GamepadControls {
            right_trigger: TriggerValue::new(value),
            ..GamepadControls::neutral()
        });
        assert_eq!(right[37] & (1 << 1), expected_digital << 1);
        assert_eq!(right[54], expected_analog);

        let left = packet(GamepadControls {
            left_trigger: TriggerValue::new(value),
            ..GamepadControls::neutral()
        });
        assert_eq!(left[37] & 1, expected_digital);
        assert_eq!(left[55], expected_analog);
    }
}

#[test]
fn each_stick_axis_uses_the_same_minimum_neutral_maximum_mapping() {
    for (value, expected) in [(-32_767, 0), (0, 128), (32_767, 255)] {
        let axis = SignedAxis::new(value).unwrap();
        let packets = [
            packet(GamepadControls {
                left_stick: StickState {
                    x: axis,
                    y: SignedAxis::neutral(),
                },
                ..GamepadControls::neutral()
            }),
            packet(GamepadControls {
                left_stick: StickState {
                    x: SignedAxis::neutral(),
                    y: axis,
                },
                ..GamepadControls::neutral()
            }),
            packet(GamepadControls {
                right_stick: StickState {
                    x: axis,
                    y: SignedAxis::neutral(),
                },
                ..GamepadControls::neutral()
            }),
            packet(GamepadControls {
                right_stick: StickState {
                    x: SignedAxis::neutral(),
                    y: axis,
                },
                ..GamepadControls::neutral()
            }),
        ];
        for (packet, offset) in packets.iter().zip(40..44) {
            assert_eq!(packet[offset], expected, "axis offset {offset}");
        }
    }
}

#[test]
fn neutral_compatibility_crc_and_fail_closed_buttons_are_preserved() {
    let motion = motion();
    let neutral = GamepadControls::neutral();
    let packet = encode_pad_data(7, 0, 4, motion, neutral, DsuControlsMapping::identity()).unwrap();
    assert_eq!(
        packet,
        encode_neutral_pad_data(7, 0, 4, motion, neutral).unwrap()
    );
    assert_eq!(&packet[36..40], &[0; 4]);
    assert_eq!(&packet[40..44], &[128; 4]);
    assert!(packet[44..68].iter().all(|byte| *byte == 0));
    let stored_crc = read_u32(&packet, 8);
    let mut checksum_input = packet;
    checksum_input[8..12].fill(0);
    assert_eq!(stored_crc, crc32_ieee(&checksum_input));

    for button in [
        GamepadButton::Paddle1,
        GamepadButton::Paddle2,
        GamepadButton::Paddle3,
        GamepadButton::Paddle4,
    ] {
        let controls = GamepadControls {
            buttons: GamepadButtons::empty().with(button),
            ..neutral
        };
        assert_eq!(
            encode_pad_data(7, 0, 4, motion, controls, DsuControlsMapping::identity(),),
            Err(DsuPacketError::UnsupportedGamepadButtons(1 << button as u8))
        );
    }

    let invalid = GamepadControls {
        dpad: DpadState { x: 0, y: 2 },
        ..neutral
    };
    assert_eq!(
        encode_pad_data(7, 0, 4, motion, invalid, DsuControlsMapping::identity(),),
        Err(DsuPacketError::InvalidGamepadControls)
    );
}

#[test]
fn source_y_orientation_is_selected_by_explicit_sign_mapping() {
    let controls = GamepadControls {
        dpad: DpadState { x: 0, y: 1 },
        left_stick: StickState {
            x: SignedAxis::neutral(),
            y: SignedAxis::new(32_767).unwrap(),
        },
        ..GamepadControls::neutral()
    };
    let identity = packet(controls);
    assert_eq!(identity[36], 1 << 4);
    assert_eq!(identity[41], 255);
    assert_eq!(identity[47], 255);

    let source_y_down = DsuControlsMapping::new(
        DsuFaceButtonLayout::DualShockPhysical,
        AxisSign::Positive,
        AxisSign::Negative,
        AxisSign::Positive,
        AxisSign::Negative,
        AxisSign::Positive,
        AxisSign::Negative,
    );
    let inverted = encode_pad_data(7, 0, 0, motion(), controls, source_y_down).unwrap();
    assert_eq!(inverted[36], 1 << 6);
    assert_eq!(inverted[41], 0);
    assert_eq!(inverted[45], 255);
}
