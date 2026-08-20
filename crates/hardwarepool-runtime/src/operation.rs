use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
};

use hardwarepool_core::{AudioFormat, CapabilityId, ProjectionKind, SessionId};
use serde::{Deserialize, Serialize};

use crate::RuntimeError;

pub const DEFAULT_MAX_PENDING_OPERATIONS: usize = 64;
pub const DEFAULT_MAX_RETAINED_TERMINAL_OPERATIONS: usize = 128;

/// Opaque, process-local identity assigned by one Runtime instance.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OperationId(u64);

impl fmt::Display for OperationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "operation-{}", self.0)
    }
}

/// Platform work that an Android or Windows host must execute asynchronously.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostOperation {
    StartAudioStream {
        session_id: SessionId,
        capability_id: CapabilityId,
        projection_kind: ProjectionKind,
        requested_format: AudioFormat,
    },
    StopAudioStream {
        session_id: SessionId,
        capability_id: CapabilityId,
    },
}

/// Actual stream configuration accepted by the platform audio stack.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActualAudioStreamParameters {
    pub format: AudioFormat,
    pub frames_per_burst: Option<u32>,
    pub buffer_capacity_frames: u32,
}

impl ActualAudioStreamParameters {
    fn validate(&self) -> Result<(), RuntimeError> {
        self.format.validate()?;
        if self.buffer_capacity_frames == 0 {
            return Err(RuntimeError::InvalidOperationCompletion(
                "audio buffer capacity must be greater than zero",
            ));
        }
        if let Some(frames_per_burst) = self.frames_per_burst {
            if frames_per_burst == 0 {
                return Err(RuntimeError::InvalidOperationCompletion(
                    "frames per burst must be greater than zero when reported",
                ));
            }
            if frames_per_burst > self.buffer_capacity_frames {
                return Err(RuntimeError::InvalidOperationCompletion(
                    "frames per burst cannot exceed audio buffer capacity",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostOperationFailureCode {
    PermissionDenied,
    PermissionRevoked,
    AudioRouteUnavailable,
    DeviceBusy,
    AudioFocusDenied,
    StreamOpenFailed,
    PlatformUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HostOperationFailure {
    pub code: HostOperationFailureCode,
    pub retryable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostOperationOutput {
    AudioStreamStarted { actual: ActualAudioStreamParameters },
    AudioStreamStopped,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum HostOperationCompletion {
    Succeeded { output: HostOperationOutput },
    Failed { failure: HostOperationFailure },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Pending,
    Completed,
    Cancelled,
    Disposed,
}

impl OperationStatus {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Pending)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OperationRecord {
    pub id: OperationId,
    pub operation: HostOperation,
    pub status: OperationStatus,
    pub completion: Option<HostOperationCompletion>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationUpdate {
    Applied(OperationStatus),
    AlreadyTerminal(OperationStatus),
}

/// Fixed-capacity owner for asynchronous host-operation state.
#[derive(Clone, Debug)]
pub struct OperationRegistry {
    records: BTreeMap<OperationId, OperationRecord>,
    terminal_order: VecDeque<OperationId>,
    next_id: u64,
    pending_count: usize,
    max_pending: usize,
    max_retained_terminal: usize,
}

impl Default for OperationRegistry {
    fn default() -> Self {
        Self::with_limits(
            DEFAULT_MAX_PENDING_OPERATIONS,
            DEFAULT_MAX_RETAINED_TERMINAL_OPERATIONS,
        )
        .expect("default operation limits are non-zero")
    }
}

impl OperationRegistry {
    pub fn with_limits(
        max_pending: usize,
        max_retained_terminal: usize,
    ) -> Result<Self, RuntimeError> {
        if max_pending == 0 || max_retained_terminal == 0 {
            return Err(RuntimeError::InvalidOperationLimits);
        }
        Ok(Self {
            records: BTreeMap::new(),
            terminal_order: VecDeque::new(),
            next_id: 1,
            pending_count: 0,
            max_pending,
            max_retained_terminal,
        })
    }

    pub fn begin(&mut self, operation: HostOperation) -> Result<OperationId, RuntimeError> {
        validate_operation(&operation)?;
        if self.pending_count >= self.max_pending {
            return Err(RuntimeError::PendingOperationLimitReached {
                limit: self.max_pending,
            });
        }
        let id = OperationId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(RuntimeError::OperationIdExhausted)?;
        let previous = self.records.insert(
            id,
            OperationRecord {
                id,
                operation,
                status: OperationStatus::Pending,
                completion: None,
            },
        );
        debug_assert!(previous.is_none(), "operation IDs must not repeat");
        self.pending_count += 1;
        Ok(id)
    }

    pub fn complete(
        &mut self,
        id: OperationId,
        completion: HostOperationCompletion,
    ) -> Result<OperationUpdate, RuntimeError> {
        let status = self.record(id)?.status;
        if status != OperationStatus::Pending {
            return Ok(OperationUpdate::AlreadyTerminal(status));
        }
        let operation = self.record(id)?.operation.clone();
        validate_completion(&operation, &completion)?;
        let record = self.record_mut(id)?;
        record.status = OperationStatus::Completed;
        record.completion = Some(completion);
        self.pending_count -= 1;
        self.remember_terminal(id);
        Ok(OperationUpdate::Applied(OperationStatus::Completed))
    }

    pub fn cancel(&mut self, id: OperationId) -> Result<OperationUpdate, RuntimeError> {
        let record = self.record_mut(id)?;
        if record.status != OperationStatus::Pending {
            return Ok(OperationUpdate::AlreadyTerminal(record.status));
        }
        record.status = OperationStatus::Cancelled;
        self.pending_count -= 1;
        self.remember_terminal(id);
        Ok(OperationUpdate::Applied(OperationStatus::Cancelled))
    }

    pub fn dispose(&mut self, id: OperationId) -> Result<OperationUpdate, RuntimeError> {
        let previous = self.record(id)?.status;
        if previous == OperationStatus::Disposed {
            return Ok(OperationUpdate::AlreadyTerminal(OperationStatus::Disposed));
        }
        if previous == OperationStatus::Pending {
            self.pending_count -= 1;
        }
        let record = self.record_mut(id)?;
        record.status = OperationStatus::Disposed;
        record.completion = None;
        if previous == OperationStatus::Pending {
            self.remember_terminal(id);
        }
        Ok(OperationUpdate::Applied(OperationStatus::Disposed))
    }

    pub fn record(&self, id: OperationId) -> Result<&OperationRecord, RuntimeError> {
        self.records
            .get(&id)
            .ok_or(RuntimeError::UnknownOperation(id))
    }

    pub fn records(&self) -> impl Iterator<Item = &OperationRecord> {
        self.records.values()
    }

    fn record_mut(&mut self, id: OperationId) -> Result<&mut OperationRecord, RuntimeError> {
        self.records
            .get_mut(&id)
            .ok_or(RuntimeError::UnknownOperation(id))
    }

    fn remember_terminal(&mut self, id: OperationId) {
        self.terminal_order.push_back(id);
        while self.terminal_order.len() > self.max_retained_terminal {
            if let Some(evicted) = self.terminal_order.pop_front() {
                let removed = self.records.remove(&evicted);
                debug_assert!(
                    removed.is_some_and(|record| record.status.is_terminal()),
                    "only terminal operations may be evicted"
                );
            }
        }
    }
}

fn validate_operation(operation: &HostOperation) -> Result<(), RuntimeError> {
    if let HostOperation::StartAudioStream {
        requested_format, ..
    } = operation
    {
        requested_format.validate()?;
    }
    Ok(())
}

fn validate_completion(
    operation: &HostOperation,
    completion: &HostOperationCompletion,
) -> Result<(), RuntimeError> {
    let HostOperationCompletion::Succeeded { output } = completion else {
        return Ok(());
    };
    match (operation, output) {
        (
            HostOperation::StartAudioStream { .. },
            HostOperationOutput::AudioStreamStarted { actual },
        ) => actual.validate(),
        (HostOperation::StopAudioStream { .. }, HostOperationOutput::AudioStreamStopped) => Ok(()),
        _ => Err(RuntimeError::InvalidOperationCompletion(
            "completion output does not match operation kind",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start_operation() -> HostOperation {
        HostOperation::StartAudioStream {
            session_id: SessionId::new(),
            capability_id: CapabilityId::new(),
            projection_kind: ProjectionKind::ApplicationStream,
            requested_format: AudioFormat::microphone_baseline(),
        }
    }

    fn successful_start() -> HostOperationCompletion {
        HostOperationCompletion::Succeeded {
            output: HostOperationOutput::AudioStreamStarted {
                actual: ActualAudioStreamParameters {
                    format: AudioFormat::microphone_baseline(),
                    frames_per_burst: Some(192),
                    buffer_capacity_frames: 960,
                },
            },
        }
    }

    #[test]
    fn cancellation_wins_a_late_completion_race() {
        let mut registry = OperationRegistry::default();
        let id = registry.begin(start_operation()).expect("begin");
        assert_eq!(
            registry.cancel(id).expect("cancel"),
            OperationUpdate::Applied(OperationStatus::Cancelled)
        );
        assert_eq!(
            registry
                .complete(
                    id,
                    HostOperationCompletion::Succeeded {
                        output: HostOperationOutput::AudioStreamStopped,
                    },
                )
                .expect("late"),
            OperationUpdate::AlreadyTerminal(OperationStatus::Cancelled)
        );
        assert_eq!(registry.record(id).expect("record").completion, None);
    }

    #[test]
    fn completion_wins_a_late_cancellation_race() {
        let mut registry = OperationRegistry::default();
        let id = registry.begin(start_operation()).expect("begin");
        registry.complete(id, successful_start()).expect("complete");
        assert_eq!(
            registry.cancel(id).expect("late cancel"),
            OperationUpdate::AlreadyTerminal(OperationStatus::Completed)
        );
        assert!(registry.record(id).expect("record").completion.is_some());
    }

    #[test]
    fn disposing_pending_work_rejects_late_completion() {
        let mut registry = OperationRegistry::default();
        let id = registry.begin(start_operation()).expect("begin");
        registry.dispose(id).expect("dispose");
        assert_eq!(
            registry.complete(id, successful_start()).expect("late"),
            OperationUpdate::AlreadyTerminal(OperationStatus::Disposed)
        );
    }

    #[test]
    fn pending_and_terminal_storage_are_bounded() {
        let mut registry = OperationRegistry::with_limits(1, 2).expect("limits");
        let first = registry.begin(start_operation()).expect("first");
        assert!(matches!(
            registry.begin(start_operation()),
            Err(RuntimeError::PendingOperationLimitReached { limit: 1 })
        ));
        registry.cancel(first).expect("cancel first");

        let second = registry.begin(start_operation()).expect("second");
        registry.cancel(second).expect("cancel second");
        let third = registry.begin(start_operation()).expect("third");
        registry.cancel(third).expect("cancel third");

        assert!(matches!(
            registry.record(first),
            Err(RuntimeError::UnknownOperation(id)) if id == first
        ));
        assert_eq!(registry.records().count(), 2);
    }

    #[test]
    fn invalid_actual_parameters_do_not_complete_operation() {
        let mut registry = OperationRegistry::default();
        let id = registry.begin(start_operation()).expect("begin");
        let invalid = HostOperationCompletion::Succeeded {
            output: HostOperationOutput::AudioStreamStarted {
                actual: ActualAudioStreamParameters {
                    format: AudioFormat::microphone_baseline(),
                    frames_per_burst: Some(961),
                    buffer_capacity_frames: 960,
                },
            },
        };
        assert!(matches!(
            registry.complete(id, invalid),
            Err(RuntimeError::InvalidOperationCompletion(_))
        ));
        assert_eq!(
            registry.record(id).expect("record").status,
            OperationStatus::Pending
        );
    }
}
