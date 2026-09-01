#![forbid(unsafe_code)]

//! Android-owned pure data mappings. No JNI, Gradle, manifest, permission,
//! service, or APK exists.

mod touchpad;
mod touchpad_capture;

pub use touchpad::{
    AndroidMotionAction, AndroidMotionSample, AndroidPointerSample, AndroidToolType,
    AndroidTouchSurface, AndroidTouchpadMapper, AndroidTouchpadMappingError,
    AndroidTouchpadMotionPolicy,
};
pub use touchpad_capture::{
    AndroidTouchpadCaptureError, AndroidTouchpadCaptureSession, AndroidTouchpadCaptureState,
};

pub const IMPLEMENTATION_STATUS: &str = "touchpad-runtime-capture-boundary-no-jni-or-apk";
