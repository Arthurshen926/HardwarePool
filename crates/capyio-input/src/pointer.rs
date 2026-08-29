use serde::{Deserialize, Serialize};

use crate::{InputContractError, InputFrameHeader, NormalizedPosition};

const MAX_POINTER_EVENTS: usize = 64;
const MAX_RELATIVE_DELTA: i32 = 1_000_000;
const MAX_SCROLL_DELTA: i32 = 1_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerButtonPhase {
    Pressed,
    Released,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollUnit {
    Detent,
    Pixel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PointerEvent {
    /// Releases every held button and clears Adapter-local pointer state.
    Reset,
    RelativeMotion {
        delta_x: i32,
        delta_y: i32,
    },
    AbsoluteMotion {
        position: NormalizedPosition,
    },
    Button {
        button: PointerButton,
        phase: PointerButtonPhase,
    },
    Scroll {
        horizontal: i32,
        vertical: i32,
        unit: ScrollUnit,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointerFrame {
    pub header: InputFrameHeader,
    pub events: Vec<PointerEvent>,
}

impl PointerFrame {
    pub fn validate(&self) -> Result<(), InputContractError> {
        self.header.validate()?;
        if self.events.is_empty() || self.events.len() > MAX_POINTER_EVENTS {
            return Err(InputContractError::InvalidPointerFrame(format!(
                "pointer frame requires 1..={MAX_POINTER_EVENTS} events"
            )));
        }
        for event in &self.events {
            match *event {
                PointerEvent::RelativeMotion { delta_x, delta_y }
                    if delta_x.unsigned_abs() > MAX_RELATIVE_DELTA as u32
                        || delta_y.unsigned_abs() > MAX_RELATIVE_DELTA as u32 =>
                {
                    return Err(InputContractError::InvalidPointerFrame(format!(
                        "relative motion is bounded to +/-{MAX_RELATIVE_DELTA} counts"
                    )));
                }
                PointerEvent::RelativeMotion {
                    delta_x: 0,
                    delta_y: 0,
                } => {
                    return Err(InputContractError::InvalidPointerFrame(
                        "zero relative motion is not an event".to_owned(),
                    ));
                }
                PointerEvent::Scroll {
                    horizontal,
                    vertical,
                    ..
                } if horizontal.unsigned_abs() > MAX_SCROLL_DELTA as u32
                    || vertical.unsigned_abs() > MAX_SCROLL_DELTA as u32 =>
                {
                    return Err(InputContractError::InvalidPointerFrame(format!(
                        "scroll is bounded to +/-{MAX_SCROLL_DELTA} units"
                    )));
                }
                PointerEvent::Scroll {
                    horizontal: 0,
                    vertical: 0,
                    ..
                } => {
                    return Err(InputContractError::InvalidPointerFrame(
                        "zero scroll is not an event".to_owned(),
                    ));
                }
                _ => {}
            }
        }
        if self.events.len() > 1
            && self
                .events
                .iter()
                .any(|event| matches!(event, PointerEvent::Reset))
        {
            return Err(InputContractError::InvalidPointerFrame(
                "pointer reset must be the only event in its frame".to_owned(),
            ));
        }
        Ok(())
    }
}
