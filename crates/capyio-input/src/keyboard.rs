use serde::{Deserialize, Serialize};

use crate::{InputContractError, InputFrameHeader};

const MAX_KEY_EVENTS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyPhase {
    Pressed,
    Released,
}

/// CapyIO physical-key semantics. Platform scan codes and HID usages are Adapter mappings.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhysicalKey {
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Escape,
    Enter,
    Tab,
    Backspace,
    Space,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    MetaLeft,
    MetaRight,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum KeyEvent {
    Transition {
        key: PhysicalKey,
        phase: KeyPhase,
        repeat: bool,
    },
    /// Releases every held key and clears Adapter-local keyboard state.
    Reset,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyboardFrame {
    pub header: InputFrameHeader,
    pub events: Vec<KeyEvent>,
}

impl KeyboardFrame {
    pub fn validate(&self) -> Result<(), InputContractError> {
        self.header.validate()?;
        if self.events.is_empty() || self.events.len() > MAX_KEY_EVENTS {
            return Err(InputContractError::InvalidKeyboardFrame(format!(
                "keyboard frame requires 1..={MAX_KEY_EVENTS} events"
            )));
        }
        if self.events.len() > 1
            && self
                .events
                .iter()
                .any(|event| matches!(event, KeyEvent::Reset))
        {
            return Err(InputContractError::InvalidKeyboardFrame(
                "keyboard reset must be the only event in its frame".to_owned(),
            ));
        }
        if self.events.iter().any(|event| {
            matches!(
                event,
                KeyEvent::Transition {
                    phase: KeyPhase::Released,
                    repeat: true,
                    ..
                }
            )
        }) {
            return Err(InputContractError::InvalidKeyboardFrame(
                "released key events cannot be repeats".to_owned(),
            ));
        }
        Ok(())
    }
}
