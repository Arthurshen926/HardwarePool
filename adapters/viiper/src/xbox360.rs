use std::error::Error;
use std::fmt::{self, Display, Formatter};

use capyio_input::{GamepadButton, GamepadControls};

pub const VIIPER_XBOX360_INPUT_STATE_BYTES: usize = 20;
pub const VIIPER_XBOX360_RUMBLE_FEEDBACK_BYTES: usize = 2;

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
    | (1 << GamepadButton::Guide as u8);

const BUTTON_DPAD_UP: u32 = 0x0001;
const BUTTON_DPAD_DOWN: u32 = 0x0002;
const BUTTON_DPAD_LEFT: u32 = 0x0004;
const BUTTON_DPAD_RIGHT: u32 = 0x0008;
const BUTTON_START: u32 = 0x0010;
const BUTTON_BACK: u32 = 0x0020;
const BUTTON_LEFT_THUMB: u32 = 0x0040;
const BUTTON_RIGHT_THUMB: u32 = 0x0080;
const BUTTON_LEFT_SHOULDER: u32 = 0x0100;
const BUTTON_RIGHT_SHOULDER: u32 = 0x0200;
const BUTTON_GUIDE: u32 = 0x0400;
const BUTTON_A: u32 = 0x1000;
const BUTTON_B: u32 = 0x2000;
const BUTTON_X: u32 = 0x4000;
const BUTTON_Y: u32 = 0x8000;

/// Selects whether a normalized source axis is preserved or inverted before
/// projection into VIIPER's Xbox 360 report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViiperXbox360AxisSign {
    Positive,
    Negative,
}

/// Explicit source-axis mapping for the Xbox 360 projection.
///
/// Xbox/XInput uses positive X as right and positive Y as up. The portable
/// gamepad contract intentionally does not prescribe a UI coordinate system,
/// so every D-pad/stick sign is selected at the Adapter boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViiperXbox360Mapping {
    dpad_x: ViiperXbox360AxisSign,
    dpad_y: ViiperXbox360AxisSign,
    left_stick_x: ViiperXbox360AxisSign,
    left_stick_y: ViiperXbox360AxisSign,
    right_stick_x: ViiperXbox360AxisSign,
    right_stick_y: ViiperXbox360AxisSign,
}

impl ViiperXbox360Mapping {
    #[must_use]
    pub const fn new(
        dpad_x: ViiperXbox360AxisSign,
        dpad_y: ViiperXbox360AxisSign,
        left_stick_x: ViiperXbox360AxisSign,
        left_stick_y: ViiperXbox360AxisSign,
        right_stick_x: ViiperXbox360AxisSign,
        right_stick_y: ViiperXbox360AxisSign,
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

    /// Preserves every normalized source sign. It is deterministic fixture
    /// policy, not an Android touch-layout or physical mounting decision.
    #[must_use]
    pub const fn preserve() -> Self {
        Self::new(
            ViiperXbox360AxisSign::Positive,
            ViiperXbox360AxisSign::Positive,
            ViiperXbox360AxisSign::Positive,
            ViiperXbox360AxisSign::Positive,
            ViiperXbox360AxisSign::Positive,
            ViiperXbox360AxisSign::Positive,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViiperXbox360Error {
    InvalidControls,
    UnsupportedButtons(u32),
    UnexpectedRumbleLength { actual: usize, expected: usize },
}

impl Display for ViiperXbox360Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidControls => formatter.write_str("invalid normalized gamepad controls"),
            Self::UnsupportedButtons(bits) => write!(
                formatter,
                "normalized gamepad buttons {bits:#010x} have no Xbox 360 field"
            ),
            Self::UnexpectedRumbleLength { actual, expected } => write!(
                formatter,
                "VIIPER Xbox 360 rumble report is {actual} bytes; expected {expected}"
            ),
        }
    }
}

impl Error for ViiperXbox360Error {}

/// Raw motor intensities from one pinned VIIPER Xbox 360 feedback packet.
///
/// The two-byte report contains no duration or Route identity. Mapping it to a
/// CapyIO haptics command therefore remains a later lifecycle-owned operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Xbox360RumbleFeedback {
    pub left_motor: u8,
    pub right_motor: u8,
}

impl Xbox360RumbleFeedback {
    #[must_use]
    pub const fn is_neutral(self) -> bool {
        self.left_motor == 0 && self.right_motor == 0
    }
}

/// Encodes one complete normalized controller snapshot as the pinned VIIPER
/// v0.7.0 Xbox 360 device-stream `InputState` frame.
///
/// This is the external TCP stream's 20-byte `MarshalBinary` layout, not
/// VIIPER's different host-facing `BuildReport` USB layout of the same length.
/// Face positions map to XInput names: South=A, East=B, West=X and North=Y.
/// Touchpad and paddle buttons fail closed because this device has no matching
/// fields. The six reserved tail bytes always remain zero.
pub fn encode_xbox360_input_state(
    controls: GamepadControls,
    mapping: ViiperXbox360Mapping,
) -> Result<[u8; VIIPER_XBOX360_INPUT_STATE_BYTES], ViiperXbox360Error> {
    validate_controls(controls)?;

    let dpad_x = apply_i8_sign(mapping.dpad_x, controls.dpad.x);
    let dpad_y = apply_i8_sign(mapping.dpad_y, controls.dpad.y);
    let mut buttons = 0_u32;
    set_mask(&mut buttons, BUTTON_DPAD_UP, dpad_y > 0);
    set_mask(&mut buttons, BUTTON_DPAD_DOWN, dpad_y < 0);
    set_mask(&mut buttons, BUTTON_DPAD_LEFT, dpad_x < 0);
    set_mask(&mut buttons, BUTTON_DPAD_RIGHT, dpad_x > 0);

    for (button, mask) in [
        (GamepadButton::Start, BUTTON_START),
        (GamepadButton::Select, BUTTON_BACK),
        (GamepadButton::LeftStick, BUTTON_LEFT_THUMB),
        (GamepadButton::RightStick, BUTTON_RIGHT_THUMB),
        (GamepadButton::LeftShoulder, BUTTON_LEFT_SHOULDER),
        (GamepadButton::RightShoulder, BUTTON_RIGHT_SHOULDER),
        (GamepadButton::Guide, BUTTON_GUIDE),
        (GamepadButton::South, BUTTON_A),
        (GamepadButton::East, BUTTON_B),
        (GamepadButton::West, BUTTON_X),
        (GamepadButton::North, BUTTON_Y),
    ] {
        set_mask(&mut buttons, mask, controls.buttons.contains(button));
    }

    let mut report = [0_u8; VIIPER_XBOX360_INPUT_STATE_BYTES];
    report[0..4].copy_from_slice(&buttons.to_le_bytes());
    report[4] = scale_trigger(controls.left_trigger.get());
    report[5] = scale_trigger(controls.right_trigger.get());
    write_axis(
        &mut report,
        6,
        mapping.left_stick_x,
        controls.left_stick.x.get(),
    );
    write_axis(
        &mut report,
        8,
        mapping.left_stick_y,
        controls.left_stick.y.get(),
    );
    write_axis(
        &mut report,
        10,
        mapping.right_stick_x,
        controls.right_stick.x.get(),
    );
    write_axis(
        &mut report,
        12,
        mapping.right_stick_y,
        controls.right_stick.y.get(),
    );
    Ok(report)
}

/// Parses exactly one pinned VIIPER v0.7.0 two-byte Xbox 360 device-stream
/// rumble feedback frame.
pub fn decode_xbox360_rumble(report: &[u8]) -> Result<Xbox360RumbleFeedback, ViiperXbox360Error> {
    if report.len() != VIIPER_XBOX360_RUMBLE_FEEDBACK_BYTES {
        return Err(ViiperXbox360Error::UnexpectedRumbleLength {
            actual: report.len(),
            expected: VIIPER_XBOX360_RUMBLE_FEEDBACK_BYTES,
        });
    }
    Ok(Xbox360RumbleFeedback {
        left_motor: report[0],
        right_motor: report[1],
    })
}

fn validate_controls(controls: GamepadControls) -> Result<(), ViiperXbox360Error> {
    controls
        .validate()
        .map_err(|_| ViiperXbox360Error::InvalidControls)?;
    let unsupported = controls.buttons.bits() & !SUPPORTED_SOURCE_BUTTON_MASK;
    if unsupported != 0 {
        return Err(ViiperXbox360Error::UnsupportedButtons(unsupported));
    }
    Ok(())
}

fn set_mask(buttons: &mut u32, mask: u32, pressed: bool) {
    if pressed {
        *buttons |= mask;
    }
}

const fn apply_i8_sign(sign: ViiperXbox360AxisSign, value: i8) -> i8 {
    match sign {
        ViiperXbox360AxisSign::Positive => value,
        ViiperXbox360AxisSign::Negative => -value,
    }
}

fn apply_i16_sign(sign: ViiperXbox360AxisSign, value: i16) -> i16 {
    match sign {
        ViiperXbox360AxisSign::Positive => value,
        ViiperXbox360AxisSign::Negative => value
            .checked_neg()
            .expect("validated normalized axes exclude i16::MIN"),
    }
}

fn scale_axis(value: i16) -> i16 {
    if value >= 0 {
        return value;
    }
    let magnitude = -i32::from(value);
    let scaled = (magnitude * 32_768 + 16_383) / 32_767;
    i16::try_from(-scaled).expect("normalized negative axis maps to the i16 range")
}

fn write_axis(
    report: &mut [u8; VIIPER_XBOX360_INPUT_STATE_BYTES],
    offset: usize,
    sign: ViiperXbox360AxisSign,
    value: i16,
) {
    let projected = scale_axis(apply_i16_sign(sign, value));
    report[offset..offset + 2].copy_from_slice(&projected.to_le_bytes());
}

fn scale_trigger(value: u16) -> u8 {
    let scaled = (u32::from(value) * u32::from(u8::MAX) + 32_767) / u32::from(u16::MAX);
    u8::try_from(scaled).expect("u16 trigger scaling maps to u8")
}
