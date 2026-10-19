use super::*;
use capyio_enrollment::EnrollmentStatus;

use super::desktop_host::EnrollmentRequest;

pub(crate) fn enrollment_action(
    request: EnrollmentRequest,
    state: &HostState,
) -> Result<EnrollmentStatus, String> {
    state.0.enrollment_action(request)
}

pub(crate) fn enrollment_status(state: &HostState) -> Result<EnrollmentStatus, String> {
    state.0.enrollment_status()
}

pub(crate) fn start_store_worker(
    manager: std::sync::Weak<Mutex<capyio_enrollment::EnrollmentManager>>,
    control: Arc<Mutex<DesktopControlPlaneState>>,
    _id: NodeId,
) {
    thread::spawn(move || {
        loop {
            let Some(owner) = manager.upgrade() else {
                break;
            };
            if let Ok(manager) = owner.lock()
                && matches!(manager.status().phase.as_str(), "preparing" | "saving")
                && let Err(error) = persist(&manager, &control)
            {
                manager.fail_persistence(error);
            }
            drop(owner);
            thread::sleep(Duration::from_millis(200));
        }
    });
}

fn persist(
    manager: &capyio_enrollment::EnrollmentManager,
    control: &Mutex<DesktopControlPlaneState>,
) -> Result<(), String> {
    let mut control = control.lock().map_err(|_| "control lock poisoned")?;
    let store = control
        .pairing_store
        .clone()
        .ok_or("pairing store unavailable")?;
    manager.stage_with(|bytes| store.stage_enrollment(bytes))?;
    let saved = manager.persist_with(|_| {
        store
            .commit_pending_enrollment()?
            .ok_or("missing prepared enrollment")?;
        Ok(())
    })?;
    if saved {
        control.attach_pairing_store(store)?;
    }
    Ok(())
}
