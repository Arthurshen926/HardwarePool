#![forbid(unsafe_code)]

//! Android-owned pure data mappings. No JNI, Gradle, manifest, permission,
//! service, or APK exists.

mod catalog_identity;
mod touchpad;
mod touchpad_capture;

pub use catalog_identity::{
    AndroidCatalogIdentityError, android_adapter_id, android_capability_id, android_port_id,
};
pub use touchpad::{
    AndroidMotionAction, AndroidMotionSample, AndroidPointerSample, AndroidToolType,
    AndroidTouchSurface, AndroidTouchpadMapper, AndroidTouchpadMappingError,
    AndroidTouchpadMotionPolicy,
};
pub use touchpad_capture::{
    AndroidTouchpadCaptureError, AndroidTouchpadCaptureSession, AndroidTouchpadCaptureState,
};

pub const IMPLEMENTATION_STATUS: &str =
    "catalog-identity-and-touchpad-runtime-boundaries-no-jni-or-apk";
