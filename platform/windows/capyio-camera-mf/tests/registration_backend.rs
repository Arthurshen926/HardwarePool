#![cfg(windows)]

use capyio_windows_camera::{MfVirtualCameraPlan, MfVirtualCameraRegistrationBackend};
use capyio_windows_camera_mf::{MfVirtualCameraBackendError, WindowsMfVirtualCameraBackend};

#[test]
fn default_backend_is_inert_and_shutdown_is_idempotent() {
    let mut backend = WindowsMfVirtualCameraBackend::default();
    assert!(!backend.is_prepared());
    assert!(!backend.is_started());
    assert_eq!(backend.symbolic_link(), None);
    assert!(matches!(
        backend.get_media_source(),
        Err(MfVirtualCameraBackendError::NotStarted)
    ));
    assert!(matches!(
        backend.remove(),
        Err(MfVirtualCameraBackendError::NotPrepared)
    ));
    assert!(matches!(
        backend.stop(),
        Err(MfVirtualCameraBackendError::NotStarted)
    ));
    backend.shutdown().unwrap();
    backend.shutdown().unwrap();
}

#[test]
fn plan_remains_closed_to_session_current_user_fixture() {
    let plan = MfVirtualCameraPlan::capyio_fixture();
    assert_eq!(plan.friendly_name(), "CapyIO Camera");
    assert_eq!(
        plan.source_clsid(),
        "{35754be3-54b6-4133-a1c7-1716395c6f1c}"
    );
}
