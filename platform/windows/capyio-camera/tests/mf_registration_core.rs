use std::{collections::VecDeque, fmt};

use capyio_windows_camera::{
    MfRegistrationError, MfRegistrationOperation, MfRegistrationState, MfVirtualCameraPlan,
    MfVirtualCameraRegistrar, MfVirtualCameraRegistrationBackend,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FakeError {
    Prepare,
    Start,
    Stop,
    Shutdown,
}

impl fmt::Display for FakeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for FakeError {}

#[derive(Debug, Default)]
struct FakeBackend {
    calls: Vec<&'static str>,
    failures: VecDeque<FakeError>,
}

impl FakeBackend {
    fn failing(failures: impl IntoIterator<Item = FakeError>) -> Self {
        Self {
            calls: Vec::new(),
            failures: failures.into_iter().collect(),
        }
    }

    fn result_for(&mut self, call: &'static str, error: FakeError) -> Result<(), FakeError> {
        self.calls.push(call);
        if self.failures.front() == Some(&error) {
            Err(self.failures.pop_front().expect("front failure is present"))
        } else {
            Ok(())
        }
    }
}

impl MfVirtualCameraRegistrationBackend for FakeBackend {
    type Error = FakeError;

    fn prepare(&mut self, plan: &MfVirtualCameraPlan) -> Result<(), Self::Error> {
        assert_eq!(plan.friendly_name(), "CapyIO Camera");
        self.result_for("prepare", FakeError::Prepare)
    }

    fn start(&mut self) -> Result<(), Self::Error> {
        self.result_for("start", FakeError::Start)
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        self.result_for("stop", FakeError::Stop)
    }

    fn shutdown(&mut self) -> Result<(), Self::Error> {
        self.result_for("shutdown", FakeError::Shutdown)
    }
}

fn registrar(backend: FakeBackend) -> MfVirtualCameraRegistrar<FakeBackend> {
    MfVirtualCameraRegistrar::new(MfVirtualCameraPlan::capyio_fixture(), backend)
}

#[test]
fn successful_lifecycle_is_explicit_ordered_and_idempotently_shutdown() {
    let mut registrar = registrar(FakeBackend::default());
    registrar.prepare().unwrap();
    registrar.start().unwrap();
    registrar.stop().unwrap();
    registrar.shutdown().unwrap();
    registrar.shutdown().unwrap();

    assert_eq!(registrar.state(), MfRegistrationState::Shutdown);
    assert_eq!(
        registrar.backend().calls,
        ["prepare", "start", "stop", "shutdown"]
    );
}

#[test]
fn invalid_transition_does_not_call_backend() {
    let mut registrar = registrar(FakeBackend::default());
    assert_eq!(
        registrar.start(),
        Err(MfRegistrationError::InvalidState {
            operation: MfRegistrationOperation::Start,
            state: MfRegistrationState::Planned,
        })
    );
    assert!(registrar.backend().calls.is_empty());
}

#[test]
fn start_failure_immediately_shuts_down_prepared_backend() {
    let mut registrar = registrar(FakeBackend::failing([FakeError::Start]));
    registrar.prepare().unwrap();
    assert_eq!(
        registrar.start(),
        Err(MfRegistrationError::StartFailedRolledBack(FakeError::Start))
    );
    assert_eq!(registrar.state(), MfRegistrationState::Shutdown);
    assert_eq!(registrar.backend().calls, ["prepare", "start", "shutdown"]);
}

#[test]
fn failed_start_and_rollback_are_visible_and_cleanup_can_be_retried() {
    let mut registrar = registrar(FakeBackend::failing([
        FakeError::Start,
        FakeError::Shutdown,
    ]));
    registrar.prepare().unwrap();
    assert_eq!(
        registrar.start(),
        Err(MfRegistrationError::StartAndRollbackFailed {
            start_error: FakeError::Start,
            rollback_error: FakeError::Shutdown,
        })
    );
    assert_eq!(registrar.state(), MfRegistrationState::CleanupRequired);

    registrar.shutdown().unwrap();
    assert_eq!(registrar.state(), MfRegistrationState::Shutdown);
    assert_eq!(
        registrar.backend().calls,
        ["prepare", "start", "shutdown", "shutdown"]
    );
}

#[test]
fn shutdown_while_started_stops_before_releasing_backend() {
    let mut registrar = registrar(FakeBackend::default());
    registrar.prepare().unwrap();
    registrar.start().unwrap();
    registrar.shutdown().unwrap();

    assert_eq!(registrar.state(), MfRegistrationState::Shutdown);
    assert_eq!(
        registrar.backend().calls,
        ["prepare", "start", "stop", "shutdown"]
    );
}

#[test]
fn stop_failure_during_shutdown_still_attempts_terminal_cleanup() {
    let mut registrar = registrar(FakeBackend::failing([FakeError::Stop]));
    registrar.prepare().unwrap();
    registrar.start().unwrap();
    assert_eq!(
        registrar.shutdown(),
        Err(MfRegistrationError::StopFailedRolledBack(FakeError::Stop))
    );
    assert_eq!(registrar.state(), MfRegistrationState::Shutdown);
    assert_eq!(
        registrar.backend().calls,
        ["prepare", "start", "stop", "shutdown"]
    );
}
