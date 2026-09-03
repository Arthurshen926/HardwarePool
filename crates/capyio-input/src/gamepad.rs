use serde::{Deserialize, Serialize};

use crate::{InputContractError, InputFrameHeader, SignedAxis, TriggerValue};

const KNOWN_BUTTON_MASK: u32 = (1 << 16) - 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum GamepadButton {
    South = 0,
    East = 1,
    West = 2,
    North = 3,
    LeftShoulder = 4,
    RightShoulder = 5,
    LeftStick = 6,
    RightStick = 7,
    Select = 8,
    Start = 9,
    Guide = 10,
    Touchpad = 11,
    Paddle1 = 12,
    Paddle2 = 13,
    Paddle3 = 14,
    Paddle4 = 15,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GamepadButtons(u32);

impl GamepadButtons {
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[must_use]
    pub const fn with(mut self, button: GamepadButton) -> Self {
        self.0 |= 1_u32 << button as u8;
        self
    }

    #[must_use]
    pub const fn contains(self, button: GamepadButton) -> bool {
        self.0 & (1_u32 << button as u8) != 0
    }

    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    pub fn validate(self) -> Result<(), InputContractError> {
        if self.0 & !KNOWN_BUTTON_MASK != 0 {
            return Err(InputContractError::InvalidGamepadState(
                "gamepad button mask contains unknown required bits".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StickState {
    pub x: SignedAxis,
    pub y: SignedAxis,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DpadState {
    pub x: i8,
    pub y: i8,
}

impl DpadState {
    pub fn validate(self) -> Result<(), InputContractError> {
        if !(-1..=1).contains(&self.x) || !(-1..=1).contains(&self.y) {
            return Err(InputContractError::InvalidGamepadState(
                "D-pad axes must be -1, 0, or 1".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GamepadControls {
    pub buttons: GamepadButtons,
    pub dpad: DpadState,
    pub left_stick: StickState,
    pub right_stick: StickState,
    pub left_trigger: TriggerValue,
    pub right_trigger: TriggerValue,
}

impl GamepadControls {
    #[must_use]
    pub const fn neutral() -> Self {
        Self {
            buttons: GamepadButtons::empty(),
            dpad: DpadState { x: 0, y: 0 },
            left_stick: StickState {
                x: SignedAxis::neutral(),
                y: SignedAxis::neutral(),
            },
            right_stick: StickState {
                x: SignedAxis::neutral(),
                y: SignedAxis::neutral(),
            },
            left_trigger: TriggerValue::idle(),
            right_trigger: TriggerValue::idle(),
        }
    }

    pub fn validate(self) -> Result<(), InputContractError> {
        self.buttons.validate()?;
        self.dpad.validate()?;
        for axis in [
            self.left_stick.x,
            self.left_stick.y,
            self.right_stick.x,
            self.right_stick.y,
        ] {
            axis.validate()?;
        }
        Ok(())
    }
}

/// Complete controller snapshot. IMU, touch, haptics and battery remain separate semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GamepadState {
    pub header: InputFrameHeader,
    pub controls: GamepadControls,
}

impl GamepadState {
    pub fn validate(self) -> Result<(), InputContractError> {
        self.header.validate()?;
        self.controls.validate()
    }
}
