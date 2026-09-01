#![forbid(unsafe_code)]

//! Deterministic camera fixture and Windows Media Foundation projection seam.
//!
//! The library contains no Media Foundation calls. An opt-in companion binary
//! performs only a read-only virtual-camera-type support query. There is no
//! registration, driver, network, codec, device access, or system mutation.

mod error;
mod fixture;
mod ingress;
mod mf_projection;
mod mf_registration_core;
mod mf_source_core;
mod queue;

pub use error::CameraFixtureError;
pub use fixture::{
    DeterministicNv12Source, GeneratedVideoFrame, fixture_stream_spec, frame_checksum64,
};
pub use ingress::{ExternalNv12FrameIngress, ExternalNv12FrameIngressError};
pub use mf_projection::{
    CAPYIO_CAMERA_SOURCE_CLSID, MAX_VIRTUAL_CAMERA_FRIENDLY_NAME_UTF16,
    MediaFoundationProjectionError, MfNv12BufferLayout, MfSampleTiming, MfSampleTimingMapper,
    MfVirtualCameraAccess, MfVirtualCameraAction, MfVirtualCameraLifecycle,
    MfVirtualCameraLifecycleState, MfVirtualCameraLifetime, MfVirtualCameraPlan,
    copy_nv12_to_strided_buffer,
};
pub use mf_registration_core::{
    MfRegistrationError, MfRegistrationOperation, MfRegistrationState, MfVirtualCameraRegistrar,
    MfVirtualCameraRegistrationBackend,
};
pub use mf_source_core::{
    MAX_PENDING_SAMPLE_REQUESTS, MF_CAMERA_STREAM_ID, MfMediaSourceCore, MfMediaSourceCoreError,
    MfMediaSourceEvent, MfMediaSourceOperation, MfMediaSourceShutdownOutcome, MfMediaSourceState,
    MfMediaSourceStopOutcome, MfMediaStreamState, MfPresentationSelection, MfSampleRequestTicket,
};
pub use queue::{
    BoundedFrameQueue, FrameQueueMetrics, FrameQueueOverflowPolicy, FrameQueuePushOutcome,
    MAX_FIXTURE_QUEUE_BYTES, MAX_FIXTURE_QUEUE_FRAMES,
};

pub const IMPLEMENTATION_STATUS: &str = "fixture-external-ingress-mf-contracts-no-system-calls";
