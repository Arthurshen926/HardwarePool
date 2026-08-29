use serde::{Deserialize, Serialize};

use crate::{InputContractError, InputFrameHeader, NormalizedMagnitude};

const MAX_RUMBLE_DURATION_MILLIS: u32 = 10_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HapticsEffect {
    Stop,
    Rumble {
        low_frequency: NormalizedMagnitude,
        high_frequency: NormalizedMagnitude,
        duration_millis: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HapticsCommand {
    pub header: InputFrameHeader,
    pub effect: HapticsEffect,
}

impl HapticsCommand {
    pub fn validate(&self) -> Result<(), InputContractError> {
        self.header.validate()?;
        if let HapticsEffect::Rumble {
            low_frequency,
            high_frequency,
            duration_millis,
        } = self.effect
        {
            if duration_millis == 0 || duration_millis > MAX_RUMBLE_DURATION_MILLIS {
                return Err(InputContractError::InvalidHapticsCommand(format!(
                    "rumble duration must be inside 1..={MAX_RUMBLE_DURATION_MILLIS} milliseconds"
                )));
            }
            if low_frequency.get() == 0 && high_frequency.get() == 0 {
                return Err(InputContractError::InvalidHapticsCommand(
                    "zero-amplitude rumble must be expressed as stop".to_owned(),
                ));
            }
        }
        Ok(())
    }
}
