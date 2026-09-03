use capyio_data_plane::parse_imu_fixture_jsonl;
use capyio_input::{
    GamepadButton, GamepadButtons, GamepadControls, SignedAxis, StickState, TriggerValue,
};
use capyio_viiper_adapter::{
    VIIPER_DS4_FEEDBACK_BYTES, VIIPER_DS4_INPUT_STATE_BYTES, ViiperDs4AxisPermutation,
    ViiperDs4ControlsMapping, ViiperDs4Error, ViiperDs4MotionMapping, ViiperDs4MotionState,
    ViiperDs4SignedSourceAxis, ViiperDs4SourceAxis, decode_dualshock4_feedback,
    encode_dualshock4_input_state, project_dualshock4_motion,
};

const FIXTURE: &str = include_str!("../../../fixtures/imu/imu_samples_v1.jsonl");

fn read_i16(bytes: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

#[test]
fn neutral_controller_uses_stationary_motion_and_inactive_touch() {
    let report = encode_dualshock4_input_state(
        GamepadControls::neutral(),
        ViiperDs4MotionState::stationary(),
        ViiperDs4ControlsMapping::preserve(),
    )
    .unwrap();
    assert_eq!(report.len(), VIIPER_DS4_INPUT_STATE_BYTES);
    assert_eq!(&report[..9], &[0; 9]);
    assert_eq!(&report[9..19], &[0; 10]);
    assert_eq!(&report[19..25], &[0; 6]);
    assert_eq!(read_i16(&report, 25), 0);
    assert_eq!(read_i16(&report, 27), 0);
    assert_eq!(read_i16(&report, 29), -5023);
}

#[test]
fn controls_map_to_dualshock_physical_fields_and_axis_endpoints() {
    let controls = GamepadControls {
        buttons: GamepadButtons::empty()
            .with(GamepadButton::South)
            .with(GamepadButton::East)
            .with(GamepadButton::West)
            .with(GamepadButton::North)
            .with(GamepadButton::LeftShoulder)
            .with(GamepadButton::RightShoulder)
            .with(GamepadButton::Select)
            .with(GamepadButton::Start)
            .with(GamepadButton::LeftStick)
            .with(GamepadButton::RightStick)
            .with(GamepadButton::Guide)
            .with(GamepadButton::Touchpad),
        dpad: capyio_input::DpadState { x: -1, y: 1 },
        left_stick: StickState {
            x: SignedAxis::new(-32_767).unwrap(),
            y: SignedAxis::new(32_767).unwrap(),
        },
        right_stick: StickState {
            x: SignedAxis::new(1).unwrap(),
            y: SignedAxis::new(-1).unwrap(),
        },
        left_trigger: TriggerValue::new(1),
        right_trigger: TriggerValue::new(u16::MAX),
    };
    let report = encode_dualshock4_input_state(
        controls,
        ViiperDs4MotionState::stationary(),
        ViiperDs4ControlsMapping::preserve(),
    )
    .unwrap();
    assert_eq!(&report[..4], &[128, 127, 0, 0]);
    assert_eq!(read_u16(&report, 4), 0xfff3);
    assert_eq!(report[6], 0x05);
    assert_eq!(report[7], 0);
    assert_eq!(report[8], 255);
}

#[test]
fn semantic_positive_y_up_is_inverted_for_viiper_ds4_sticks_only() {
    let controls = GamepadControls {
        left_stick: StickState {
            x: SignedAxis::new(12_000).unwrap(),
            y: SignedAxis::new(20_000).unwrap(),
        },
        right_stick: StickState {
            x: SignedAxis::new(-12_000).unwrap(),
            y: SignedAxis::new(-20_000).unwrap(),
        },
        ..GamepadControls::neutral()
    };
    let preserved = encode_dualshock4_input_state(
        controls,
        ViiperDs4MotionState::stationary(),
        ViiperDs4ControlsMapping::preserve(),
    )
    .unwrap();
    let mapped = encode_dualshock4_input_state(
        controls,
        ViiperDs4MotionState::stationary(),
        ViiperDs4ControlsMapping::gamepad_y_up(),
    )
    .unwrap();
    assert_eq!(mapped[0], preserved[0], "left X must remain unchanged");
    assert_eq!(mapped[2], preserved[2], "right X must remain unchanged");
    assert_eq!(mapped[1] as i8, -(preserved[1] as i8));
    assert_eq!(mapped[3] as i8, -(preserved[3] as i8));
}

#[test]
fn canonical_si_motion_projects_to_pinned_fixed_point_layout() {
    let envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
    let motion = project_dualshock4_motion(&envelope, ViiperDs4MotionMapping::identity()).unwrap();
    assert_eq!(motion.gyroscope(), [1, 2, -1]);
    assert_eq!(motion.acceleration(), [5, -10, 5018]);
    let report = encode_dualshock4_input_state(
        GamepadControls::neutral(),
        motion,
        ViiperDs4ControlsMapping::preserve(),
    )
    .unwrap();
    assert_eq!(read_i16(&report, 19), 1);
    assert_eq!(read_i16(&report, 21), 2);
    assert_eq!(read_i16(&report, 23), -1);
    assert_eq!(read_i16(&report, 25), 5);
    assert_eq!(read_i16(&report, 27), -10);
    assert_eq!(read_i16(&report, 29), 5018);
}

#[test]
fn motion_axis_mapping_is_explicit_and_rejects_duplicates() {
    let permutation = ViiperDs4AxisPermutation::new(
        ViiperDs4SignedSourceAxis::positive(ViiperDs4SourceAxis::Z),
        ViiperDs4SignedSourceAxis::negative(ViiperDs4SourceAxis::X),
        ViiperDs4SignedSourceAxis::positive(ViiperDs4SourceAxis::Y),
    )
    .unwrap();
    let envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
    let motion = project_dualshock4_motion(
        &envelope,
        ViiperDs4MotionMapping::new(permutation, permutation),
    )
    .unwrap();
    assert_eq!(motion.acceleration(), [5018, -5, -10]);
    assert_eq!(motion.gyroscope(), [-1, -1, 2]);

    assert_eq!(
        ViiperDs4AxisPermutation::new(
            ViiperDs4SignedSourceAxis::positive(ViiperDs4SourceAxis::X),
            ViiperDs4SignedSourceAxis::negative(ViiperDs4SourceAxis::X),
            ViiperDs4SignedSourceAxis::positive(ViiperDs4SourceAxis::Z),
        ),
        Err(ViiperDs4Error::DuplicateSourceAxis(ViiperDs4SourceAxis::X))
    );
}

#[test]
fn android_landscape_mount_maps_pitch_yaw_roll_into_ds4_body_axes() {
    let envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
    let identity =
        project_dualshock4_motion(&envelope, ViiperDs4MotionMapping::identity()).unwrap();
    let mounted = project_dualshock4_motion(
        &envelope,
        ViiperDs4MotionMapping::android_landscape_to_ds4(),
    )
    .unwrap();

    assert_eq!(
        mounted.gyroscope(),
        [
            identity.gyroscope()[1],
            identity.gyroscope()[2],
            identity.gyroscope()[0],
        ]
    );
    assert_eq!(
        mounted.acceleration(),
        [
            identity.acceleration()[1],
            identity.acceleration()[2],
            identity.acceleration()[0],
        ]
    );
}

#[test]
fn paddles_and_motion_overflow_fail_closed() {
    let controls = GamepadControls {
        buttons: GamepadButtons::empty().with(GamepadButton::Paddle1),
        ..GamepadControls::neutral()
    };
    assert_eq!(
        encode_dualshock4_input_state(
            controls,
            ViiperDs4MotionState::stationary(),
            ViiperDs4ControlsMapping::preserve(),
        ),
        Err(ViiperDs4Error::UnsupportedButtons(
            1 << GamepadButton::Paddle1 as u8
        ))
    );

    let mut envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
    envelope.payload.angular_velocity[0] = f64::MAX;
    assert!(matches!(
        project_dualshock4_motion(&envelope, ViiperDs4MotionMapping::identity()),
        Err(ViiperDs4Error::MotionOutOfRange("gyroscope"))
    ));
}

#[test]
fn feedback_is_exactly_seven_bytes() {
    assert_eq!(VIIPER_DS4_FEEDBACK_BYTES, 7);
    let feedback = decode_dualshock4_feedback(&[1, 2, 3, 4, 5, 6, 7]).unwrap();
    assert_eq!(feedback.small_motor, 1);
    assert_eq!(feedback.large_motor, 2);
    assert_eq!(
        [feedback.led_red, feedback.led_green, feedback.led_blue],
        [3, 4, 5]
    );
    assert_eq!([feedback.flash_on, feedback.flash_off], [6, 7]);
    assert!(matches!(
        decode_dualshock4_feedback(&[0; 6]),
        Err(ViiperDs4Error::UnexpectedFeedbackLength {
            actual: 6,
            expected: 7
        })
    ));
}
