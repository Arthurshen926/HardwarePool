//! Pure lifecycle model for the future native display engine handle.

use std::fmt;

use crate::{
    ValidatedDisplayStreamSession,
    engine_abi::{
        EngineAbiError, EngineCallStatus, EngineProbeV2, EngineStartV2, MAX_STOP_TIMEOUT_MILLIS,
        MIN_STOP_TIMEOUT_MILLIS, ValidatedEngineProbe, WindowsCaptureOutput,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineLifecycleState {
    AwaitingProbe,
    Probed,
    Creating,
    Ready,
    Starting,
    Running,
    Stopping,
    Failed,
    Destroying,
    Destroyed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveEngineIdentity {
    pub adapter_instance_id: [u8; 16],
    pub display_capability_id: [u8; 16],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineLifecycle {
    state: EngineLifecycleState,
    probe: Option<ValidatedEngineProbe>,
    handle_exists: bool,
    active: Option<ActiveEngineIdentity>,
}

impl Default for EngineLifecycle {
    fn default() -> Self {
        Self {
            state: EngineLifecycleState::AwaitingProbe,
            probe: None,
            handle_exists: false,
            active: None,
        }
    }
}

impl EngineLifecycle {
    #[must_use]
    pub const fn from_validated_probe(probe: ValidatedEngineProbe) -> Self {
        Self {
            state: EngineLifecycleState::Probed,
            probe: Some(probe),
            handle_exists: false,
            active: None,
        }
    }

    #[must_use]
    pub const fn state(&self) -> EngineLifecycleState {
        self.state
    }

    #[must_use]
    pub const fn active_identity(&self) -> Option<ActiveEngineIdentity> {
        self.active
    }

    #[must_use]
    pub const fn can_unload_library(&self) -> bool {
        !self.handle_exists
            && matches!(
                self.state,
                EngineLifecycleState::AwaitingProbe
                    | EngineLifecycleState::Probed
                    | EngineLifecycleState::Failed
                    | EngineLifecycleState::Destroyed
            )
    }

    pub fn accept_probe(&mut self, probe: EngineProbeV2) -> Result<(), EngineLifecycleError> {
        self.require_state(EngineLifecycleState::AwaitingProbe)?;
        match probe.validate() {
            Ok(probe) => {
                self.probe = Some(probe);
                self.state = EngineLifecycleState::Probed;
                Ok(())
            }
            Err(error) => {
                self.state = EngineLifecycleState::Failed;
                Err(error.into())
            }
        }
    }

    pub fn begin_create(&mut self) -> Result<(), EngineLifecycleError> {
        self.require_state(EngineLifecycleState::Probed)?;
        self.state = EngineLifecycleState::Creating;
        Ok(())
    }

    pub fn complete_create(
        &mut self,
        status: EngineCallStatus,
    ) -> Result<(), EngineLifecycleError> {
        self.require_state(EngineLifecycleState::Creating)?;
        if status == EngineCallStatus::Ok {
            self.handle_exists = true;
            self.state = EngineLifecycleState::Ready;
            Ok(())
        } else {
            self.state = EngineLifecycleState::Failed;
            Err(EngineLifecycleError::Native(status))
        }
    }

    pub fn prepare_start(
        &mut self,
        session: &ValidatedDisplayStreamSession,
        capture_output: WindowsCaptureOutput,
    ) -> Result<EngineStartV2, EngineLifecycleError> {
        self.require_state(EngineLifecycleState::Ready)?;
        let probe = self.probe.ok_or(EngineLifecycleError::ProbeUnavailable)?;
        let start = EngineStartV2::new(probe, session, capture_output)?;
        self.active = Some(ActiveEngineIdentity {
            adapter_instance_id: start.adapter_instance_id,
            display_capability_id: start.display_capability_id,
        });
        self.state = EngineLifecycleState::Starting;
        Ok(start)
    }

    pub fn complete_start(&mut self, status: EngineCallStatus) -> Result<(), EngineLifecycleError> {
        self.require_state(EngineLifecycleState::Starting)?;
        if status == EngineCallStatus::Ok {
            self.state = EngineLifecycleState::Running;
            Ok(())
        } else {
            self.state = EngineLifecycleState::Failed;
            Err(EngineLifecycleError::Native(status))
        }
    }

    pub fn begin_stop(&mut self, timeout_millis: u32) -> Result<(), EngineLifecycleError> {
        self.require_state(EngineLifecycleState::Running)?;
        if !(MIN_STOP_TIMEOUT_MILLIS..=MAX_STOP_TIMEOUT_MILLIS).contains(&timeout_millis) {
            return Err(EngineLifecycleError::InvalidOperationTimeout);
        }
        self.state = EngineLifecycleState::Stopping;
        Ok(())
    }

    pub fn complete_stop(&mut self, status: EngineCallStatus) -> Result<(), EngineLifecycleError> {
        self.require_state(EngineLifecycleState::Stopping)?;
        match status {
            EngineCallStatus::Ok => {
                self.active = None;
                self.state = EngineLifecycleState::Ready;
                Ok(())
            }
            EngineCallStatus::Timeout => {
                self.state = EngineLifecycleState::Running;
                Err(EngineLifecycleError::Native(status))
            }
            _ => {
                self.state = EngineLifecycleState::Failed;
                Err(EngineLifecycleError::Native(status))
            }
        }
    }

    pub fn begin_destroy(&mut self, timeout_millis: u32) -> Result<(), EngineLifecycleError> {
        if !matches!(
            self.state,
            EngineLifecycleState::Ready | EngineLifecycleState::Failed
        ) {
            return Err(EngineLifecycleError::InvalidState {
                expected: EngineLifecycleState::Ready,
                actual: self.state,
            });
        }
        if !self.handle_exists {
            return Err(EngineLifecycleError::HandleUnavailable);
        }
        if !(MIN_STOP_TIMEOUT_MILLIS..=MAX_STOP_TIMEOUT_MILLIS).contains(&timeout_millis) {
            return Err(EngineLifecycleError::InvalidOperationTimeout);
        }
        self.state = EngineLifecycleState::Destroying;
        Ok(())
    }

    pub fn complete_destroy(
        &mut self,
        status: EngineCallStatus,
    ) -> Result<(), EngineLifecycleError> {
        self.require_state(EngineLifecycleState::Destroying)?;
        if status == EngineCallStatus::Ok {
            self.active = None;
            self.probe = None;
            self.handle_exists = false;
            self.state = EngineLifecycleState::Destroyed;
            Ok(())
        } else {
            self.state = EngineLifecycleState::Failed;
            Err(EngineLifecycleError::Native(status))
        }
    }

    fn require_state(&self, expected: EngineLifecycleState) -> Result<(), EngineLifecycleError> {
        if self.state == expected {
            Ok(())
        } else {
            Err(EngineLifecycleError::InvalidState {
                expected,
                actual: self.state,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineLifecycleError {
    InvalidState {
        expected: EngineLifecycleState,
        actual: EngineLifecycleState,
    },
    ProbeUnavailable,
    HandleUnavailable,
    InvalidOperationTimeout,
    Abi(EngineAbiError),
    Native(EngineCallStatus),
}

impl From<EngineAbiError> for EngineLifecycleError {
    fn from(value: EngineAbiError) -> Self {
        Self::Abi(value)
    }
}

impl fmt::Display for EngineLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidState { expected, actual } => {
                write!(
                    formatter,
                    "expected {expected:?} engine state, got {actual:?}"
                )
            }
            Self::ProbeUnavailable => formatter.write_str("validated engine probe is unavailable"),
            Self::HandleUnavailable => formatter.write_str("native engine handle is unavailable"),
            Self::InvalidOperationTimeout => {
                formatter.write_str("engine operation timeout is outside bounds")
            }
            Self::Abi(error) => write!(formatter, "engine ABI rejected: {error}"),
            Self::Native(status) => write!(formatter, "native engine returned {status:?}"),
        }
    }
}

impl std::error::Error for EngineLifecycleError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DisplayMode, DisplayOnlyPolicy, DisplayStreamSessionRequest, DynamicRange,
        ValidatedDisplayStreamSession, VideoCodec,
        engine_abi::{FEATURE_HDR10, FEATURE_HEVC, REQUIRED_VIDEO_FEATURES},
    };

    fn session() -> ValidatedDisplayStreamSession {
        ValidatedDisplayStreamSession::new(DisplayStreamSessionRequest {
            adapter_instance_id: [3; 16],
            display_capability_id: [4; 16],
            mode: DisplayMode {
                width: 2_640,
                height: 1_216,
                refresh_millihertz: 120_000,
            },
            codec: VideoCodec::Hevc,
            dynamic_range: DynamicRange::Hdr10,
            bitrate_kbps: 45_000,
            port_base: 50_000,
            policy: DisplayOnlyPolicy::CAPYIO,
        })
        .unwrap()
    }

    fn probe() -> EngineProbeV2 {
        EngineProbeV2 {
            enabled_features: REQUIRED_VIDEO_FEATURES | FEATURE_HEVC | FEATURE_HDR10,
            ..EngineProbeV2::display_only_baseline(1)
        }
    }

    fn output() -> WindowsCaptureOutput {
        WindowsCaptureOutput::parse(r"\\.\DISPLAY2").unwrap()
    }

    fn create(lifecycle: &mut EngineLifecycle) {
        lifecycle.accept_probe(probe()).unwrap();
        lifecycle.begin_create().unwrap();
        lifecycle.complete_create(EngineCallStatus::Ok).unwrap();
    }

    #[test]
    fn successful_lifecycle_retains_exact_identity_until_quiescent_stop() {
        let mut lifecycle = EngineLifecycle::default();
        create(&mut lifecycle);
        let start = lifecycle.prepare_start(&session(), output()).unwrap();
        assert_eq!(start.adapter_instance_id, [3; 16]);
        assert_eq!(lifecycle.state(), EngineLifecycleState::Starting);
        lifecycle.complete_start(EngineCallStatus::Ok).unwrap();
        assert_eq!(lifecycle.state(), EngineLifecycleState::Running);
        assert_eq!(
            lifecycle.active_identity().unwrap().display_capability_id,
            [4; 16]
        );

        lifecycle.begin_stop(1_000).unwrap();
        lifecycle.complete_stop(EngineCallStatus::Ok).unwrap();
        assert_eq!(lifecycle.state(), EngineLifecycleState::Ready);
        assert_eq!(lifecycle.active_identity(), None);
        lifecycle.begin_destroy(1_000).unwrap();
        lifecycle.complete_destroy(EngineCallStatus::Ok).unwrap();
        assert_eq!(lifecycle.state(), EngineLifecycleState::Destroyed);
    }

    #[test]
    fn stop_timeout_returns_to_running_and_preserves_owner() {
        let mut lifecycle = EngineLifecycle::default();
        create(&mut lifecycle);
        lifecycle.prepare_start(&session(), output()).unwrap();
        lifecycle.complete_start(EngineCallStatus::Ok).unwrap();
        lifecycle.begin_stop(500).unwrap();
        assert_eq!(
            lifecycle.complete_stop(EngineCallStatus::Timeout),
            Err(EngineLifecycleError::Native(EngineCallStatus::Timeout))
        );
        assert_eq!(lifecycle.state(), EngineLifecycleState::Running);
        assert!(lifecycle.active_identity().is_some());
    }

    #[test]
    fn invalid_probe_and_native_start_failure_fail_closed() {
        let mut lifecycle = EngineLifecycle::default();
        let mut invalid = probe();
        invalid.enabled_features |= crate::engine_abi::FEATURE_AUDIO;
        assert!(matches!(
            lifecycle.accept_probe(invalid),
            Err(EngineLifecycleError::Abi(
                EngineAbiError::ForbiddenSubsystemEnabled
            ))
        ));
        assert_eq!(lifecycle.state(), EngineLifecycleState::Failed);

        lifecycle = EngineLifecycle::default();
        create(&mut lifecycle);
        lifecycle.prepare_start(&session(), output()).unwrap();
        assert_eq!(
            lifecycle.complete_start(EngineCallStatus::CapacityExhausted),
            Err(EngineLifecycleError::Native(
                EngineCallStatus::CapacityExhausted
            ))
        );
        assert_eq!(lifecycle.state(), EngineLifecycleState::Failed);
        assert!(lifecycle.active_identity().is_some());
        assert!(!lifecycle.can_unload_library());
        lifecycle.begin_destroy(1_000).unwrap();
        lifecycle.complete_destroy(EngineCallStatus::Ok).unwrap();
        assert!(lifecycle.can_unload_library());
    }

    #[test]
    fn running_handle_cannot_be_destroyed_or_stopped_with_unbounded_timeout() {
        let mut lifecycle = EngineLifecycle::default();
        create(&mut lifecycle);
        lifecycle.prepare_start(&session(), output()).unwrap();
        lifecycle.complete_start(EngineCallStatus::Ok).unwrap();
        assert_eq!(
            lifecycle.begin_stop(MAX_STOP_TIMEOUT_MILLIS + 1),
            Err(EngineLifecycleError::InvalidOperationTimeout)
        );
        assert!(matches!(
            lifecycle.begin_destroy(1_000),
            Err(EngineLifecycleError::InvalidState { .. })
        ));
        assert_eq!(lifecycle.state(), EngineLifecycleState::Running);
    }

    #[test]
    fn create_failure_leaves_no_handle_and_allows_library_unload() {
        let mut lifecycle = EngineLifecycle::default();
        lifecycle.accept_probe(probe()).unwrap();
        assert_eq!(lifecycle.state(), EngineLifecycleState::Probed);
        lifecycle.begin_create().unwrap();
        assert_eq!(
            lifecycle.complete_create(EngineCallStatus::InternalFailure),
            Err(EngineLifecycleError::Native(
                EngineCallStatus::InternalFailure
            ))
        );
        assert_eq!(lifecycle.state(), EngineLifecycleState::Failed);
        assert!(lifecycle.can_unload_library());
        assert_eq!(
            lifecycle.begin_destroy(1_000),
            Err(EngineLifecycleError::HandleUnavailable)
        );
    }
}
