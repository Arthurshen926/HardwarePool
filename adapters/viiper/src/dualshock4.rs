use std::error::Error;
use std::fmt::{self, Display, Formatter};

use capyio_data_plane::{DataEnvelope, DataPlaneError, ImuSampleV1};
use capyio_input::{GamepadButton, GamepadControls};

pub const VIIPER_DS4_INPUT_STATE_BYTES: usize = 31;
pub const VIIPER_DS4_FEEDBACK_BYTES: usize = 7;
pub const VIIPER_DS4_GYRO_COUNTS_PER_DEGREE_PER_SECOND: f64 = 16.0;
pub const VIIPER_DS4_ACCEL_COUNTS_PER_METER_PER_SECOND_SQUARED: f64 = 512.0;

const SUPPORTED_SOURCE_BUTTON_MASK: u32 = (1 << GamepadButton::South as u8)
    | (1 << GamepadButton::East as u8)
    | (1 << GamepadButton::West as u8)
    | (1 << GamepadButton::North as u8)
    | (1 << GamepadButton::LeftShoulder as u8)
    | (1 << GamepadButton::RightShoulder as u8)
    | (1 << GamepadButton::LeftStick as u8)
    | (1 << GamepadButton::RightStick as u8)
    | (1 << GamepadButton::Select as u8)
    | (1 << GamepadButton::Start as u8)
    | (1 << GamepadButton::Guide as u8)
    | (1 << GamepadButton::Touchpad as u8);

const BUTTON_SQUARE: u16 = 0x0010;
const BUTTON_CROSS: u16 = 0x0020;
const BUTTON_CIRCLE: u16 = 0x0040;
const BUTTON_TRIANGLE: u16 = 0x0080;
const BUTTON_L1: u16 = 0x0100;
const BUTTON_R1: u16 = 0x0200;
const BUTTON_L2: u16 = 0x0400;
const BUTTON_R2: u16 = 0x0800;
const BUTTON_SHARE: u16 = 0x1000;
const BUTTON_OPTIONS: u16 = 0x2000;
const BUTTON_L3: u16 = 0x4000;
const BUTTON_R3: u16 = 0x8000;
const BUTTON_PS: u16 = 0x0001;
const BUTTON_TOUCHPAD_CLICK: u16 = 0x0002;

const DPAD_UP: u8 = 0x01;
const DPAD_DOWN: u8 = 0x02;
const DPAD_LEFT: u8 = 0x04;
const DPAD_RIGHT: u8 = 0x08;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViiperDs4AxisSign {
    Positive,
    Negative,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViiperDs4SourceAxis {
    X,
    Y,
    Z,
}

impl ViiperDs4SourceAxis {
    const fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }

    const fn bit(self) -> u8 {
        1 << self.index()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViiperDs4SignedSourceAxis {
    axis: ViiperDs4SourceAxis,
    sign: ViiperDs4AxisSign,
}

impl ViiperDs4SignedSourceAxis {
    #[must_use]
    pub const fn positive(axis: ViiperDs4SourceAxis) -> Self {
        Self {
            axis,
            sign: ViiperDs4AxisSign::Positive,
        }
    }

    #[must_use]
    pub const fn negative(axis: ViiperDs4SourceAxis) -> Self {
        Self {
            axis,
            sign: ViiperDs4AxisSign::Negative,
        }
    }

    fn apply(self, values: [f64; 3]) -> f64 {
        let value = values[self.axis.index()];
        match self.sign {
            ViiperDs4AxisSign::Positive => value,
            ViiperDs4AxisSign::Negative => -value,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViiperDs4AxisPermutation {
    outputs: [ViiperDs4SignedSourceAxis; 3],
}

impl ViiperDs4AxisPermutation {
    pub fn new(
        x: ViiperDs4SignedSourceAxis,
        y: ViiperDs4SignedSourceAxis,
        z: ViiperDs4SignedSourceAxis,
    ) -> Result<Self, ViiperDs4Error> {
        let outputs = [x, y, z];
        let mut seen = 0_u8;
        for output in outputs {
            if seen & output.axis.bit() != 0 {
                return Err(ViiperDs4Error::DuplicateSourceAxis(output.axis));
            }
            seen |= output.axis.bit();
        }
        Ok(Self { outputs })
    }

    #[must_use]
    pub const fn identity() -> Self {
        Self {
            outputs: [
                ViiperDs4SignedSourceAxis::positive(ViiperDs4SourceAxis::X),
                ViiperDs4SignedSourceAxis::positive(ViiperDs4SourceAxis::Y),
                ViiperDs4SignedSourceAxis::positive(ViiperDs4SourceAxis::Z),
            ],
        }
    }

    fn apply(self, values: [f64; 3]) -> [f64; 3] {
        self.outputs.map(|output| output.apply(values))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViiperDs4ControlsMapping {
    dpad_x: ViiperDs4AxisSign,
    dpad_y: ViiperDs4AxisSign,
    left_stick_x: ViiperDs4AxisSign,
    left_stick_y: ViiperDs4AxisSign,
    right_stick_x: ViiperDs4AxisSign,
    right_stick_y: ViiperDs4AxisSign,
}

impl ViiperDs4ControlsMapping {
    #[must_use]
    pub const fn new(
        dpad_x: ViiperDs4AxisSign,
        dpad_y: ViiperDs4AxisSign,
        left_stick_x: ViiperDs4AxisSign,
        left_stick_y: ViiperDs4AxisSign,
        right_stick_x: ViiperDs4AxisSign,
        right_stick_y: ViiperDs4AxisSign,
    ) -> Self {
        Self {
            dpad_x,
            dpad_y,
            left_stick_x,
            left_stick_y,
            right_stick_x,
            right_stick_y,
        }
    }

    #[must_use]
    pub const fn preserve() -> Self {
        Self::new(
            ViiperDs4AxisSign::Positive,
            ViiperDs4AxisSign::Positive,
            ViiperDs4AxisSign::Positive,
            ViiperDs4AxisSign::Positive,
            ViiperDs4AxisSign::Positive,
            ViiperDs4AxisSign::Positive,
        )
    }

    /// Maps CapyIO's semantic stick convention (positive Y is up) to
    /// VIIPER's DS4 stick convention (positive Y is down).
    #[must_use]
    pub const fn gamepad_y_up() -> Self {
        Self::new(
            ViiperDs4AxisSign::Positive,
            ViiperDs4AxisSign::Positive,
            ViiperDs4AxisSign::Positive,
            ViiperDs4AxisSign::Negative,
            ViiperDs4AxisSign::Positive,
            ViiperDs4AxisSign::Negative,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViiperDs4MotionMapping {
    acceleration: ViiperDs4AxisPermutation,
    angular_velocity: ViiperDs4AxisPermutation,
}

impl ViiperDs4MotionMapping {
    #[must_use]
    pub const fn new(
        acceleration: ViiperDs4AxisPermutation,
        angular_velocity: ViiperDs4AxisPermutation,
    ) -> Self {
        Self {
            acceleration,
            angular_velocity,
        }
    }

    #[must_use]
    pub const fn identity() -> Self {
        Self::new(
            ViiperDs4AxisPermutation::identity(),
            ViiperDs4AxisPermutation::identity(),
        )
    }

    /// Maps the fixed Android device frame of a portrait-native phone held in
    /// the Controller Lab's landscape orientation into the DS4 body frame.
    ///
    /// Android source axes are X toward the portrait right edge, Y toward the
    /// portrait top edge and Z out of the screen. In Controller Lab landscape,
    /// DS4 pitch/right is +Y, DS4 yaw/up is +Z and DS4 roll/toward-player is +X.
    #[must_use]
    pub fn android_landscape_to_ds4() -> Self {
        let permutation = ViiperDs4AxisPermutation::new(
            ViiperDs4SignedSourceAxis::positive(ViiperDs4SourceAxis::Y),
            ViiperDs4SignedSourceAxis::positive(ViiperDs4SourceAxis::Z),
            ViiperDs4SignedSourceAxis::positive(ViiperDs4SourceAxis::X),
        )
        .expect("the fixed Android-to-DS4 axes are a permutation");
        Self::new(permutation, permutation)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViiperDs4MotionState {
    gyroscope: [i16; 3],
    acceleration: [i16; 3],
}

impl ViiperDs4MotionState {
    #[must_use]
    pub const fn stationary() -> Self {
        Self {
            gyroscope: [0, 0, 0],
            acceleration: [0, 0, -5023],
        }
    }

    #[must_use]
    pub const fn gyroscope(self) -> [i16; 3] {
        self.gyroscope
    }

    #[must_use]
    pub const fn acceleration(self) -> [i16; 3] {
        self.acceleration
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViiperDs4Feedback {
    pub small_motor: u8,
    pub large_motor: u8,
    pub led_red: u8,
    pub led_green: u8,
    pub led_blue: u8,
    pub flash_on: u8,
    pub flash_off: u8,
}

#[derive(Debug, Eq, PartialEq)]
pub enum ViiperDs4Error {
    InvalidControls,
    InvalidMotionEnvelope(DataPlaneError),
    UnsupportedButtons(u32),
    DuplicateSourceAxis(ViiperDs4SourceAxis),
    MotionOutOfRange(&'static str),
    UnexpectedFeedbackLength { actual: usize, expected: usize },
}

impl Display for ViiperDs4Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidControls => formatter.write_str("invalid normalized gamepad controls"),
            Self::InvalidMotionEnvelope(error) => {
                write!(formatter, "invalid IMU envelope: {error}")
            }
            Self::UnsupportedButtons(bits) => write!(
                formatter,
                "normalized gamepad buttons {bits:#010x} have no DualShock 4 field"
            ),
            Self::DuplicateSourceAxis(axis) => {
                write!(formatter, "source axis {axis:?} appears more than once")
            }
            Self::MotionOutOfRange(field) => {
                write!(
                    formatter,
                    "projected DualShock 4 {field} is outside i16 range"
                )
            }
            Self::UnexpectedFeedbackLength { actual, expected } => write!(
                formatter,
                "VIIPER DualShock 4 feedback is {actual} bytes; expected {expected}"
            ),
        }
    }
}

impl Error for ViiperDs4Error {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidMotionEnvelope(error) => Some(error),
            _ => None,
        }
    }
}

pub fn project_dualshock4_motion(
    envelope: &DataEnvelope<ImuSampleV1>,
    mapping: ViiperDs4MotionMapping,
) -> Result<ViiperDs4MotionState, ViiperDs4Error> {
    envelope
        .validate_for_profile(&ImuSampleV1::profile())
        .map_err(ViiperDs4Error::InvalidMotionEnvelope)?;
    let acceleration = mapping.acceleration.apply(envelope.payload.acceleration);
    let angular_velocity = mapping
        .angular_velocity
        .apply(envelope.payload.angular_velocity);
    Ok(ViiperDs4MotionState {
        gyroscope: project_motion_axes(
            angular_velocity.map(f64::to_degrees),
            VIIPER_DS4_GYRO_COUNTS_PER_DEGREE_PER_SECOND,
            "gyroscope",
        )?,
        acceleration: project_motion_axes(
            acceleration,
            VIIPER_DS4_ACCEL_COUNTS_PER_METER_PER_SECOND_SQUARED,
            "acceleration",
        )?,
    })
}

/// Encodes the pinned VIIPER v0.7.0 31-byte DualShock 4 device-stream state.
///
/// Touch contacts remain inactive because they are a separate CapyIO Port.
/// Touchpad click is representable. Any non-zero analog trigger also sets its
/// corresponding DS4 digital trigger bit.
pub fn encode_dualshock4_input_state(
    controls: GamepadControls,
    motion: ViiperDs4MotionState,
    mapping: ViiperDs4ControlsMapping,
) -> Result<[u8; VIIPER_DS4_INPUT_STATE_BYTES], ViiperDs4Error> {
    controls
        .validate()
        .map_err(|_| ViiperDs4Error::InvalidControls)?;
    let unsupported = controls.buttons.bits() & !SUPPORTED_SOURCE_BUTTON_MASK;
    if unsupported != 0 {
        return Err(ViiperDs4Error::UnsupportedButtons(unsupported));
    }

    let mut report = [0_u8; VIIPER_DS4_INPUT_STATE_BYTES];
    report[0] = scale_axis(apply_i16_sign(
        mapping.left_stick_x,
        controls.left_stick.x.get(),
    )) as u8;
    report[1] = scale_axis(apply_i16_sign(
        mapping.left_stick_y,
        controls.left_stick.y.get(),
    )) as u8;
    report[2] = scale_axis(apply_i16_sign(
        mapping.right_stick_x,
        controls.right_stick.x.get(),
    )) as u8;
    report[3] = scale_axis(apply_i16_sign(
        mapping.right_stick_y,
        controls.right_stick.y.get(),
    )) as u8;

    let mut buttons = 0_u16;
    for (button, mask) in [
        (GamepadButton::West, BUTTON_SQUARE),
        (GamepadButton::South, BUTTON_CROSS),
        (GamepadButton::East, BUTTON_CIRCLE),
        (GamepadButton::North, BUTTON_TRIANGLE),
        (GamepadButton::LeftShoulder, BUTTON_L1),
        (GamepadButton::RightShoulder, BUTTON_R1),
        (GamepadButton::Select, BUTTON_SHARE),
        (GamepadButton::Start, BUTTON_OPTIONS),
        (GamepadButton::LeftStick, BUTTON_L3),
        (GamepadButton::RightStick, BUTTON_R3),
        (GamepadButton::Guide, BUTTON_PS),
        (GamepadButton::Touchpad, BUTTON_TOUCHPAD_CLICK),
    ] {
        set_mask(&mut buttons, mask, controls.buttons.contains(button));
    }
    set_mask(&mut buttons, BUTTON_L2, controls.left_trigger.get() != 0);
    set_mask(&mut buttons, BUTTON_R2, controls.right_trigger.get() != 0);
    report[4..6].copy_from_slice(&buttons.to_le_bytes());

    let dpad_x = apply_i8_sign(mapping.dpad_x, controls.dpad.x);
    let dpad_y = apply_i8_sign(mapping.dpad_y, controls.dpad.y);
    set_u8_mask(&mut report[6], DPAD_UP, dpad_y > 0);
    set_u8_mask(&mut report[6], DPAD_DOWN, dpad_y < 0);
    set_u8_mask(&mut report[6], DPAD_LEFT, dpad_x < 0);
    set_u8_mask(&mut report[6], DPAD_RIGHT, dpad_x > 0);
    report[7] = scale_trigger(controls.left_trigger.get());
    report[8] = scale_trigger(controls.right_trigger.get());

    // Bytes 9..19 are two inactive touch contacts with zero coordinates.
    write_i16_axes(&mut report, 19, motion.gyroscope);
    write_i16_axes(&mut report, 25, motion.acceleration);
    Ok(report)
}

pub fn decode_dualshock4_feedback(report: &[u8]) -> Result<ViiperDs4Feedback, ViiperDs4Error> {
    if report.len() != VIIPER_DS4_FEEDBACK_BYTES {
        return Err(ViiperDs4Error::UnexpectedFeedbackLength {
            actual: report.len(),
            expected: VIIPER_DS4_FEEDBACK_BYTES,
        });
    }
    Ok(ViiperDs4Feedback {
        small_motor: report[0],
        large_motor: report[1],
        led_red: report[2],
        led_green: report[3],
        led_blue: report[4],
        flash_on: report[5],
        flash_off: report[6],
    })
}

fn project_motion_axes(
    values: [f64; 3],
    scale: f64,
    field: &'static str,
) -> Result<[i16; 3], ViiperDs4Error> {
    let mut projected = [0_i16; 3];
    for (index, value) in values.into_iter().enumerate() {
        let raw = (value * scale).round();
        if !raw.is_finite() || raw < f64::from(i16::MIN) || raw > f64::from(i16::MAX) {
            return Err(ViiperDs4Error::MotionOutOfRange(field));
        }
        projected[index] = raw as i16;
    }
    Ok(projected)
}

fn write_i16_axes(report: &mut [u8; VIIPER_DS4_INPUT_STATE_BYTES], offset: usize, axes: [i16; 3]) {
    for (index, axis) in axes.into_iter().enumerate() {
        let start = offset + index * 2;
        report[start..start + 2].copy_from_slice(&axis.to_le_bytes());
    }
}

fn set_mask(buttons: &mut u16, mask: u16, pressed: bool) {
    if pressed {
        *buttons |= mask;
    }
}

fn set_u8_mask(buttons: &mut u8, mask: u8, pressed: bool) {
    if pressed {
        *buttons |= mask;
    }
}

const fn apply_i8_sign(sign: ViiperDs4AxisSign, value: i8) -> i8 {
    match sign {
        ViiperDs4AxisSign::Positive => value,
        ViiperDs4AxisSign::Negative => -value,
    }
}

fn apply_i16_sign(sign: ViiperDs4AxisSign, value: i16) -> i16 {
    match sign {
        ViiperDs4AxisSign::Positive => value,
        ViiperDs4AxisSign::Negative => value
            .checked_neg()
            .expect("validated normalized axes exclude i16::MIN"),
    }
}

fn scale_axis(value: i16) -> i8 {
    if value >= 0 {
        let scaled = (i32::from(value) * 127 + 16_383) / 32_767;
        return i8::try_from(scaled).expect("positive normalized axis maps to i8");
    }
    let magnitude = -i32::from(value);
    let scaled = (magnitude * 128 + 16_383) / 32_767;
    i8::try_from(-scaled).expect("negative normalized axis maps to i8")
}

fn scale_trigger(value: u16) -> u8 {
    let scaled = (u32::from(value) * u32::from(u8::MAX) + 32_767) / u32::from(u16::MAX);
    u8::try_from(scaled).expect("u16 trigger scaling maps to u8")
}
