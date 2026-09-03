#![forbid(unsafe_code)]

//! Direction-neutral camera and video stream contracts.
//!
//! This crate deliberately contains no Camera2, Media Foundation, codec,
//! socket, async-runtime, UI, or virtual-camera implementation.

mod camera;
mod error;
mod frame;
mod metrics;
mod spec;

pub use camera::{CameraControlDescriptor, CameraDescriptor, LensFacing};
pub use error::VideoContractError;
pub use frame::{VideoFrameDescriptor, VideoFrameFlags};
pub use metrics::VideoMetrics;
pub use spec::{
    FrameRate, VideoColorimetry, VideoPixelFormat, VideoQosPolicy, VideoStreamCapabilities,
    VideoStreamSpec, VideoUseCase, negotiate_video_stream, video_frames_profile,
};
