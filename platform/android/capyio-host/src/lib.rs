#![forbid(unsafe_code)]

//! Android-owned pure data mappings. No JNI, Gradle, manifest, permission,
//! service, or APK exists.

mod catalog_identity;
mod catalog_projection;
mod touchpad;
mod touchpad_capture;

pub use catalog_identity::{
    AndroidCatalogIdentityError, android_adapter_id, android_capability_id, android_port_id,
};
pub use catalog_projection::{
    ANDROID_CATALOG_SCHEMA_VERSION, AndroidAdapterRouteModeV1, AndroidCapabilityAvailabilityV1,
    AndroidCapabilityClassV1, AndroidCapabilityDtoV1, AndroidCatalogDtoV1,
    AndroidCatalogProjectionError, AndroidInteroperabilityModeV1, AndroidPermissionRequirementV1,
    AndroidPortDirectionV1, AndroidPortDtoV1, AndroidQosModeV1, MAX_ANDROID_CATALOG_JSON_BYTES,
    decode_android_catalog_json, project_android_catalog,
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
