#![cfg_attr(not(windows), forbid(unsafe_code))]
// MSVC recommends marking the two standard COM exports PRIVATE in the import
// library. rustc generates that .def file and the import library is not shipped
// or consumed, so the diagnostic does not describe a DLL correctness issue.
#![cfg_attr(windows, allow(linker_messages))]

//! Windows Media Foundation COM projection for fixture or validated shared frames.

#[cfg(windows)]
mod activation;
#[cfg(windows)]
mod com_server;
#[cfg(windows)]
mod registration_backend;
#[cfg(windows)]
mod windows_impl;

#[cfg(windows)]
pub use capyio_windows_camera_share::{
    CAMERA_SHARED_INGRESS_MAPPING_BYTES, CAMERA_SHARED_INGRESS_MAPPING_NAME,
    CAMERA_SHARED_INGRESS_SLOT_COUNT, CAMERA_SHARED_INGRESS_VERSION, CameraSharedIngressConsumer,
    CameraSharedIngressError, CameraSharedIngressProducer,
};
#[cfg(windows)]
pub use com_server::{
    CAPYIO_CAMERA_SOURCE_GUID, create_media_source_class_factory, server_can_unload,
};
#[cfg(windows)]
pub use registration_backend::{
    MAX_VIRTUAL_CAMERA_SYMBOLIC_LINK_UTF16, MfVirtualCameraBackendError,
    WindowsMfVirtualCameraBackend,
};
#[cfg(windows)]
pub use windows_impl::{
    CapyIoMediaSourceHandle, MediaFoundationRuntime, create_in_process_media_source,
    create_in_process_media_source_with_external_ingress,
    create_in_process_media_source_with_shared_ingress,
};
