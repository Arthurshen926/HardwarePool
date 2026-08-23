#![forbid(unsafe_code)]

//! Transport- and platform-independent audio data-plane primitives.
//!
//! This crate defines semantic audio frames, a bounded reordering buffer and a
//! clock-rate estimator. It intentionally does not choose UDP, QUIC, RTP/AOO,
//! an audio API, a codec, or an operating-system projection.

mod drift;
mod error;
mod format;
mod frame;
mod reorder;

pub use drift::{ClockDriftEstimate, ClockDriftEstimator};
pub use error::AudioDataError;
pub use format::{AudioFormat, AudioProcessingSupport, AudioSampleFormat, ChannelLayout};
pub use frame::AudioFrame;
pub use reorder::{FrameBufferStats, InsertOutcome, ReorderBuffer};
