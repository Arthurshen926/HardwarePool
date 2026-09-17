#![forbid(unsafe_code)]

//! Direction-neutral display sharing contracts and hardware-free test helpers.
//!
//! This crate contains no screen-capture API, Android renderer, codec, socket,
//! async runtime, Windows indirect-display driver, UI, or system registration.

mod error;
mod fixture;
mod frame;
mod slot;
mod spec;

pub use error::DisplayContractError;
pub use fixture::DeterministicDisplayFixture;
pub use frame::{DisplayFrameDescriptor, DisplayFrameFlags, OwnedDisplayFrame};
pub use slot::{DisplaySequenceGap, LatestDisplayFrameSlot, LatestFramePushOutcome};
pub use spec::{
    DisplayCursorMode, DisplaySinkCapabilities, DisplaySourceCapabilities,
    DisplayStreamCapabilities, DisplayStreamSpec, DisplayWorkflow, display_frames_profile,
    negotiate_display_stream,
};
