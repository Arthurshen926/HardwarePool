use std::{error::Error, fmt};

use crate::MfVirtualCameraPlan;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MfRegistrationState {
    Planned,
    Prepared,
    Started,
    Stopped,
    CleanupRequired,
    Shutdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MfRegistrationOperation {
    Prepare,
    Start,
    Stop,
    Shutdown,
}

/// Execution boundary for a future Windows `IMFVirtualCamera` backend.
///
/// CAPY-CAMERA-001B1A intentionally provides no system backend. Tests use an
/// in-memory implementation; a later approved slice can translate these four
/// calls to create/start/stop/shutdown without changing rollback policy.
pub trait MfVirtualCameraRegistrationBackend {
    type Error;

    fn prepare(&mut self, plan: &MfVirtualCameraPlan) -> Result<(), Self::Error>;
    fn start(&mut self) -> Result<(), Self::Error>;
    fn stop(&mut self) -> Result<(), Self::Error>;
    fn shutdown(&mut self) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub struct MfVirtualCameraRegistrar<B> {
    plan: MfVirtualCameraPlan,
    backend: B,
    state: MfRegistrationState,
}

impl<B> MfVirtualCameraRegistrar<B>
where
    B: MfVirtualCameraRegistrationBackend,
{
    #[must_use]
    pub const fn new(plan: MfVirtualCameraPlan, backend: B) -> Self {
        Self {
            plan,
            backend,
            state: MfRegistrationState::Planned,
        }
    }

    #[must_use]
    pub const fn state(&self) -> MfRegistrationState {
        self.state
    }

    #[must_use]
    pub const fn plan(&self) -> &MfVirtualCameraPlan {
        &self.plan
    }

    #[must_use]
    pub const fn backend(&self) -> &B {
        &self.backend
    }

    pub fn prepare(&mut self) -> Result<(), MfRegistrationError<B::Error>> {
        self.require_state(
            MfRegistrationOperation::Prepare,
            MfRegistrationState::Planned,
        )?;
        self.backend
            .prepare(&self.plan)
            .map_err(MfRegistrationError::PrepareFailed)?;
        self.state = MfRegistrationState::Prepared;
        Ok(())
    }

    pub fn start(&mut self) -> Result<(), MfRegistrationError<B::Error>> {
        self.require_state(
            MfRegistrationOperation::Start,
            MfRegistrationState::Prepared,
        )?;
        match self.backend.start() {
            Ok(()) => {
                self.state = MfRegistrationState::Started;
                Ok(())
            }
            Err(start_error) => match self.backend.shutdown() {
                Ok(()) => {
                    self.state = MfRegistrationState::Shutdown;
                    Err(MfRegistrationError::StartFailedRolledBack(start_error))
                }
                Err(rollback_error) => {
                    self.state = MfRegistrationState::CleanupRequired;
                    Err(MfRegistrationError::StartAndRollbackFailed {
                        start_error,
                        rollback_error,
                    })
                }
            },
        }
    }

    pub fn stop(&mut self) -> Result<(), MfRegistrationError<B::Error>> {
        self.require_state(MfRegistrationOperation::Stop, MfRegistrationState::Started)?;
        self.backend
            .stop()
            .map_err(MfRegistrationError::StopFailed)?;
        self.state = MfRegistrationState::Stopped;
        Ok(())
    }

    pub fn shutdown(&mut self) -> Result<(), MfRegistrationError<B::Error>> {
        match self.state {
            MfRegistrationState::Shutdown => Ok(()),
            MfRegistrationState::Planned => {
                self.state = MfRegistrationState::Shutdown;
                Ok(())
            }
            MfRegistrationState::Started => self.shutdown_started(),
            MfRegistrationState::Prepared
            | MfRegistrationState::Stopped
            | MfRegistrationState::CleanupRequired => {
                self.backend
                    .shutdown()
                    .map_err(MfRegistrationError::ShutdownFailed)?;
                self.state = MfRegistrationState::Shutdown;
                Ok(())
            }
        }
    }

    fn shutdown_started(&mut self) -> Result<(), MfRegistrationError<B::Error>> {
        match self.backend.stop() {
            Ok(()) => {
                self.state = MfRegistrationState::Stopped;
                self.backend
                    .shutdown()
                    .map_err(MfRegistrationError::ShutdownFailed)?;
                self.state = MfRegistrationState::Shutdown;
                Ok(())
            }
            Err(stop_error) => match self.backend.shutdown() {
                Ok(()) => {
                    self.state = MfRegistrationState::Shutdown;
                    Err(MfRegistrationError::StopFailedRolledBack(stop_error))
                }
                Err(rollback_error) => {
                    self.state = MfRegistrationState::CleanupRequired;
                    Err(MfRegistrationError::StopAndRollbackFailed {
                        stop_error,
                        rollback_error,
                    })
                }
            },
        }
    }

    fn require_state(
        &self,
        operation: MfRegistrationOperation,
        required: MfRegistrationState,
    ) -> Result<(), MfRegistrationError<B::Error>> {
        if self.state == required {
            Ok(())
        } else {
            Err(MfRegistrationError::InvalidState {
                operation,
                state: self.state,
            })
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum MfRegistrationError<E> {
    InvalidState {
        operation: MfRegistrationOperation,
        state: MfRegistrationState,
    },
    PrepareFailed(E),
    StartFailedRolledBack(E),
    StartAndRollbackFailed {
        start_error: E,
        rollback_error: E,
    },
    StopFailed(E),
    StopFailedRolledBack(E),
    StopAndRollbackFailed {
        stop_error: E,
        rollback_error: E,
    },
    ShutdownFailed(E),
}

impl<E> fmt::Display for MfRegistrationError<E>
where
    E: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidState { operation, state } => {
                write!(
                    formatter,
                    "cannot {operation:?} while registrar is {state:?}"
                )
            }
            Self::PrepareFailed(error) => write!(formatter, "camera prepare failed: {error}"),
            Self::StartFailedRolledBack(error) => {
                write!(
                    formatter,
                    "camera start failed and was rolled back: {error}"
                )
            }
            Self::StartAndRollbackFailed {
                start_error,
                rollback_error,
            } => write!(
                formatter,
                "camera start failed ({start_error}) and rollback failed ({rollback_error})"
            ),
            Self::StopFailed(error) => write!(formatter, "camera stop failed: {error}"),
            Self::StopFailedRolledBack(error) => {
                write!(
                    formatter,
                    "camera stop failed but shutdown succeeded: {error}"
                )
            }
            Self::StopAndRollbackFailed {
                stop_error,
                rollback_error,
            } => write!(
                formatter,
                "camera stop failed ({stop_error}) and shutdown failed ({rollback_error})"
            ),
            Self::ShutdownFailed(error) => write!(formatter, "camera shutdown failed: {error}"),
        }
    }
}

impl<E> Error for MfRegistrationError<E> where E: Error + 'static {}
