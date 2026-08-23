use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
};

use capyio_core::{AdapterInstanceId, NodeId, RouteId};
use serde::{Deserialize, Serialize};

use crate::RuntimeError;

pub const DEFAULT_MAX_PENDING_OPERATIONS: usize = 64;
pub const DEFAULT_MAX_RETAINED_TERMINAL_OPERATIONS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OperationId(u64);

impl fmt::Display for OperationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "operation-{}", self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostOperation {
    RefreshAdapterCatalog {
        node_id: NodeId,
        adapter_id: AdapterInstanceId,
    },
    StartRoute {
        route_id: RouteId,
    },
    StopRoute {
        route_id: RouteId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostOperationFailureCode {
    PermissionDenied,
    ResourceUnavailable,
    DeviceBusy,
    StartFailed,
    AdapterFailed,
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
    AdapterCatalogRefreshed { capability_count: u32 },
    RouteStarted,
    RouteStopped,
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
        .expect("default operation limits are valid")
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
        self.records.insert(
            id,
            OperationRecord {
                id,
                operation,
                status: OperationStatus::Pending,
                completion: None,
            },
        );
        self.pending_count += 1;
        Ok(id)
    }

    pub fn complete(
        &mut self,
        id: OperationId,
        completion: HostOperationCompletion,
    ) -> Result<OperationUpdate, RuntimeError> {
        let record = self
            .records
            .get(&id)
            .ok_or(RuntimeError::UnknownOperation(id))?;
        if record.status.is_terminal() {
            return Ok(OperationUpdate::AlreadyTerminal(record.status));
        }
        validate_completion(&record.operation, &completion)?;
        let record = self.records.get_mut(&id).expect("record checked above");
        record.status = OperationStatus::Completed;
        record.completion = Some(completion);
        self.finish_pending(id);
        Ok(OperationUpdate::Applied(OperationStatus::Completed))
    }

    pub fn cancel(&mut self, id: OperationId) -> Result<OperationUpdate, RuntimeError> {
        self.terminate(id, OperationStatus::Cancelled)
    }

    pub fn dispose(&mut self, id: OperationId) -> Result<OperationUpdate, RuntimeError> {
        self.terminate(id, OperationStatus::Disposed)
    }

    pub fn record(&self, id: OperationId) -> Result<&OperationRecord, RuntimeError> {
        self.records
            .get(&id)
            .ok_or(RuntimeError::UnknownOperation(id))
    }

    pub fn records(&self) -> impl Iterator<Item = &OperationRecord> {
        self.records.values()
    }

    fn terminate(
        &mut self,
        id: OperationId,
        status: OperationStatus,
    ) -> Result<OperationUpdate, RuntimeError> {
        let record = self
            .records
            .get_mut(&id)
            .ok_or(RuntimeError::UnknownOperation(id))?;
        if record.status.is_terminal() {
            return Ok(OperationUpdate::AlreadyTerminal(record.status));
        }
        record.status = status;
        self.finish_pending(id);
        Ok(OperationUpdate::Applied(status))
    }

    fn finish_pending(&mut self, id: OperationId) {
        self.pending_count = self.pending_count.saturating_sub(1);
        self.terminal_order.push_back(id);
        while self.terminal_order.len() > self.max_retained_terminal {
            if let Some(evicted) = self.terminal_order.pop_front() {
                self.records.remove(&evicted);
            }
        }
    }
}

fn validate_completion(
    operation: &HostOperation,
    completion: &HostOperationCompletion,
) -> Result<(), RuntimeError> {
    let HostOperationCompletion::Succeeded { output } = completion else {
        return Ok(());
    };
    let matching = matches!(
        (operation, output),
        (
            HostOperation::RefreshAdapterCatalog { .. },
            HostOperationOutput::AdapterCatalogRefreshed { .. }
        ) | (
            HostOperation::StartRoute { .. },
            HostOperationOutput::RouteStarted
        ) | (
            HostOperation::StopRoute { .. },
            HostOperationOutput::RouteStopped
        )
    );
    if matching {
        Ok(())
    } else {
        Err(RuntimeError::InvalidOperationCompletion(
            "completion output does not match operation",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start() -> HostOperation {
        HostOperation::StartRoute {
            route_id: RouteId::new(),
        }
    }

    #[test]
    fn completion_wins_late_cancel() {
        let mut registry = OperationRegistry::default();
        let id = registry.begin(start()).expect("begin");
        assert_eq!(
            registry
                .complete(
                    id,
                    HostOperationCompletion::Succeeded {
                        output: HostOperationOutput::RouteStarted,
                    },
                )
                .expect("complete"),
            OperationUpdate::Applied(OperationStatus::Completed)
        );
        assert_eq!(
            registry.cancel(id).expect("cancel"),
            OperationUpdate::AlreadyTerminal(OperationStatus::Completed)
        );
    }

    #[test]
    fn cancellation_wins_late_completion() {
        let mut registry = OperationRegistry::default();
        let id = registry.begin(start()).expect("begin");
        registry.cancel(id).expect("cancel");
        assert_eq!(
            registry
                .complete(
                    id,
                    HostOperationCompletion::Succeeded {
                        output: HostOperationOutput::RouteStopped,
                    },
                )
                .expect("late completion ignored"),
            OperationUpdate::AlreadyTerminal(OperationStatus::Cancelled)
        );
    }

    #[test]
    fn mismatched_completion_is_rejected() {
        let mut registry = OperationRegistry::default();
        let id = registry.begin(start()).expect("begin");
        assert!(matches!(
            registry.complete(
                id,
                HostOperationCompletion::Succeeded {
                    output: HostOperationOutput::RouteStopped,
                },
            ),
            Err(RuntimeError::InvalidOperationCompletion(_))
        ));
    }

    #[test]
    fn pending_and_terminal_storage_are_bounded() {
        let mut registry = OperationRegistry::with_limits(1, 1).expect("limits");
        let first = registry.begin(start()).expect("first");
        assert!(matches!(
            registry.begin(start()),
            Err(RuntimeError::PendingOperationLimitReached { limit: 1 })
        ));
        registry.cancel(first).expect("cancel");
        let second = registry.begin(start()).expect("second");
        registry.dispose(second).expect("dispose");
        assert!(matches!(
            registry.record(first),
            Err(RuntimeError::UnknownOperation(_))
        ));
    }
}
