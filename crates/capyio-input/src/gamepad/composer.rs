use capyio_core::StreamId;

use super::{DpadState, GamepadButton, GamepadControls, GamepadState, StickState};
use crate::{InputContractError, InputFrameHeader, TriggerValue};

/// Selects one of the two semantic gamepad sticks for a local state update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GamepadStick {
    Left,
    Right,
}

/// Selects one of the two semantic gamepad triggers for a local state update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GamepadTrigger {
    Left,
    Right,
}

/// One platform-local semantic change used to compose a complete gamepad state.
///
/// This is a source-side helper API, not an event Profile or wire contract.
/// Touch geometry, pointer ownership and widget hit-testing remain UI policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GamepadControlUpdate {
    Button {
        button: GamepadButton,
        pressed: bool,
    },
    Dpad(DpadState),
    Stick {
        stick: GamepadStick,
        state: StickState,
    },
    Trigger {
        trigger: GamepadTrigger,
        value: TriggerValue,
    },
    Reset,
}

/// Success-path allocation-free source helper that turns semantic control
/// changes into complete, monotonically sequenced [`GamepadState`] snapshots.
///
/// The composer is bound to one stream epoch. A new Route epoch requires a new
/// composer. Every successful update emits and consumes one sequence, including
/// a repeated value or an already-neutral reset. Invalid updates and exhausted
/// sequences do not change the retained controls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GamepadStateComposer {
    stream_id: StreamId,
    stream_epoch: u64,
    next_sequence: Option<u64>,
    controls: GamepadControls,
}

impl GamepadStateComposer {
    pub fn new(
        stream_id: StreamId,
        stream_epoch: u64,
        first_sequence: u64,
    ) -> Result<Self, InputContractError> {
        if stream_epoch == 0 {
            return Err(InputContractError::InvalidStream(
                "stream epoch must be positive".to_owned(),
            ));
        }
        Ok(Self {
            stream_id,
            stream_epoch,
            next_sequence: Some(first_sequence),
            controls: GamepadControls::neutral(),
        })
    }

    #[must_use]
    pub const fn controls(&self) -> GamepadControls {
        self.controls
    }

    #[must_use]
    pub const fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    #[must_use]
    pub const fn stream_epoch(&self) -> u64 {
        self.stream_epoch
    }

    #[must_use]
    pub const fn next_sequence(&self) -> Option<u64> {
        self.next_sequence
    }

    /// Returns a validated description of the next snapshot without emitting
    /// it or consuming its sequence. This can anchor a fixed-epoch consumer
    /// before the first call to [`Self::apply`].
    pub fn anchor(&self, source_timestamp_nanos: u64) -> Result<GamepadState, InputContractError> {
        let sequence = self
            .next_sequence
            .ok_or(InputContractError::SequenceExhausted)?;
        let state = GamepadState {
            header: InputFrameHeader {
                stream_id: self.stream_id,
                stream_epoch: self.stream_epoch,
                sequence,
                source_timestamp_nanos,
            },
            controls: self.controls,
        };
        state.validate()?;
        Ok(state)
    }

    pub fn apply(
        &mut self,
        update: GamepadControlUpdate,
        source_timestamp_nanos: u64,
    ) -> Result<GamepadState, InputContractError> {
        let sequence = self
            .next_sequence
            .ok_or(InputContractError::SequenceExhausted)?;
        let candidate = apply_update(self.controls, update);
        candidate.validate()?;
        let state = GamepadState {
            header: InputFrameHeader {
                stream_id: self.stream_id,
                stream_epoch: self.stream_epoch,
                sequence,
                source_timestamp_nanos,
            },
            controls: candidate,
        };
        state.validate()?;
        self.controls = candidate;
        self.next_sequence = sequence.checked_add(1);
        Ok(state)
    }
}

fn apply_update(mut controls: GamepadControls, update: GamepadControlUpdate) -> GamepadControls {
    match update {
        GamepadControlUpdate::Button { button, pressed } => {
            controls.buttons = if pressed {
                controls.buttons.with(button)
            } else {
                controls.buttons.without(button)
            };
        }
        GamepadControlUpdate::Dpad(dpad) => controls.dpad = dpad,
        GamepadControlUpdate::Stick { stick, state } => match stick {
            GamepadStick::Left => controls.left_stick = state,
            GamepadStick::Right => controls.right_stick = state,
        },
        GamepadControlUpdate::Trigger { trigger, value } => match trigger {
            GamepadTrigger::Left => controls.left_trigger = value,
            GamepadTrigger::Right => controls.right_trigger = value,
        },
        GamepadControlUpdate::Reset => controls = GamepadControls::neutral(),
    }
    controls
}
