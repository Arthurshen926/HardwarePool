#![forbid(unsafe_code)]

//! Direction-neutral input-event, gamepad-state, and haptics contracts.
//!
//! This crate deliberately contains no Android UI, Windows injection, HID,
//! USB/IP, DSU, VIIPER, socket, async-runtime, or driver implementation.

mod common;
mod error;
mod formats;
mod gamepad;
mod haptics;
mod keyboard;
mod pointer;
mod touch;
mod touchpad;

pub use common::{
    InputFrameHeader, InputSequenceOutcome, InputSequenceTracker, InputStreamDescriptor,
    NormalizedMagnitude, NormalizedPosition, SequenceGap, SignedAxis, TriggerValue,
    validate_input_text,
};
pub use error::InputContractError;
pub use formats::{
    dual_rumble_format, gamepad_state_format, gamepad_state_profile, haptics_feedback_profile,
    key_events_format, key_events_profile, pointer_events_format, pointer_events_profile,
    touch_events_profile, touch_snapshot_format, touchpad_frame_format, touchpad_frames_profile,
};
pub use gamepad::{
    DpadState, GamepadButton, GamepadButtons, GamepadControls, GamepadState, StickState,
};
pub use haptics::{HapticsCommand, HapticsEffect};
pub use keyboard::{KeyEvent, KeyPhase, KeyboardFrame, PhysicalKey};
pub use pointer::{PointerButton, PointerButtonPhase, PointerEvent, PointerFrame, ScrollUnit};
pub use touch::{TouchContact, TouchFrame};
pub use touchpad::{
    MAX_TOUCHPAD_CONTACTS, MAX_TOUCHPAD_FIXTURE_FRAMES, MAX_TOUCHPAD_HIMETRIC_AXIS,
    MIN_TOUCHPAD_CONTACTS, TouchpadButtonState, TouchpadButtonType, TouchpadContact,
    TouchpadContactSize, TouchpadDescriptor, TouchpadFixture, TouchpadFrame, TouchpadFrameKind,
    TouchpadFrameOutcome, TouchpadFrameTracker, TouchpadMetrics, TouchpadPhysicalSize,
    TouchpadPosition,
};
