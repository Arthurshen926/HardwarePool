#![forbid(unsafe_code)]

//! Deterministic protocol boundary for a planned external VIIPER Adapter.
//!
//! The bounded client uses only the pinned protocol on an explicit IP loopback
//! socket. Read-only `ping` is independently available. Mutating Xbox 360
//! provisioning requires an explicit caller assertion that VIIPER localhost
//! auto-attach is disabled, then owns create/add/stream/neutral/remove as one
//! transaction. This crate does not contain upstream source, start VIIPER,
//! attach USB/IP or perform a driver operation.

mod client;
mod dualshock4;
mod dualshock4_session;
mod session;
mod xbox360;

pub use client::{
    CompatibleViiperProbe, CompatibleViiperVersion, EXPERIMENTAL_VIIPER_DS4WINDOWS_V012_VERSION,
    EXPERIMENTAL_VIIPER_DS4WINDOWS_VERSION, EXPERIMENTAL_VIIPER_URB_FIX_VERSION,
    MAX_VIIPER_CONNECTION_TIMEOUT, MAX_VIIPER_MANAGEMENT_RESPONSE_BYTES, PINNED_VIIPER_SERVER,
    PINNED_VIIPER_VERSION, ViiperClientError, ViiperLoopbackClient, ViiperLoopbackConfig,
};
pub use dualshock4::{
    VIIPER_DS4_ACCEL_COUNTS_PER_METER_PER_SECOND_SQUARED, VIIPER_DS4_FEEDBACK_BYTES,
    VIIPER_DS4_GYRO_COUNTS_PER_DEGREE_PER_SECOND, VIIPER_DS4_INPUT_STATE_BYTES,
    ViiperDs4AxisPermutation, ViiperDs4AxisSign, ViiperDs4ControlsMapping, ViiperDs4Error,
    ViiperDs4Feedback, ViiperDs4MotionMapping, ViiperDs4MotionState, ViiperDs4SignedSourceAxis,
    ViiperDs4SourceAxis, decode_dualshock4_feedback, encode_dualshock4_input_state,
    project_dualshock4_motion,
};
pub use dualshock4_session::{
    ViiperDs4OpenError, ViiperDs4SessionError, ViiperDs4StopError, ViiperDs4Worker,
    ViiperDs4WorkerState,
};
pub use session::{
    ViiperAutoAttachDisabled, ViiperOpenError, ViiperSessionError, ViiperStopError,
    ViiperSubmitOutcome, ViiperXbox360Worker, ViiperXbox360WorkerState,
};
pub use xbox360::{
    VIIPER_XBOX360_INPUT_STATE_BYTES, VIIPER_XBOX360_RUMBLE_FEEDBACK_BYTES, ViiperXbox360AxisSign,
    ViiperXbox360Error, ViiperXbox360Mapping, Xbox360RumbleFeedback, decode_xbox360_rumble,
    encode_xbox360_input_state,
};

pub const IMPLEMENTATION_STATUS: &str = "capy-gamepad-004a-dualshock4-codec";
