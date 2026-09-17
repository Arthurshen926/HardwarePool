#![deny(unsafe_op_in_unsafe_fn)]

//! Windows-only, hash-pinned loader for the CapyIO display engine ABI.

#[cfg(windows)]
mod event_queue;
#[cfg(windows)]
mod launch_exchange;
#[cfg(windows)]
mod loader;
#[cfg(windows)]
mod worker_client;
#[cfg(windows)]
mod worker_configuration;
#[cfg(windows)]
mod worker_protocol;

#[cfg(windows)]
pub use event_queue::EngineEventBatch;
#[cfg(windows)]
pub use launch_exchange::{
    exchange_authenticated_engine_launch, try_exchange_authenticated_engine_launch,
};
#[cfg(windows)]
pub use loader::{
    AuthenticatedEngineLaunch, DisplayEngineLibrary, DisplayEngineLoadError, DisplayEngineSession,
};
#[cfg(windows)]
pub use worker_client::{
    DisplayWorkerClientError, DisplayWorkerConnection, DisplayWorkerControlEvent,
    DisplayWorkerPipeName, PreparedDisplayWorker, acknowledge_display_worker_lease,
    connect_display_worker_start_client, run_display_worker_lease_client,
    run_display_worker_start_client,
};
#[cfg(windows)]
pub use worker_configuration::{
    DISPLAY_WORKER_CONFIG_ACK_BYTES, DISPLAY_WORKER_CONFIG_BYTES, DisplayWorkerConfigurationError,
    DisplayWorkerEngineConfigRecord, DisplayWorkerEngineConfiguration,
    encode_display_worker_config_ack, verify_display_worker_config_ack,
};
#[cfg(windows)]
pub use worker_protocol::{
    DISPLAY_WORKER_CONTROL_ACK_BYTES, DISPLAY_WORKER_CONTROL_BYTES, DISPLAY_WORKER_LEASE_ACK_BYTES,
    DISPLAY_WORKER_LEASE_BYTES, DISPLAY_WORKER_START_ACK_BYTES, DISPLAY_WORKER_START_BYTES,
    DisplayWorkerControlCommand, DisplayWorkerControlRecord, DisplayWorkerLease,
    DisplayWorkerProtocolError, DisplayWorkerStartAcknowledgement, DisplayWorkerStartRecord,
    acknowledge_display_worker_start, encode_display_worker_control_ack,
    encode_display_worker_lease_ack, verify_display_worker_control_ack,
    verify_display_worker_lease_ack, verify_display_worker_start_ack,
};
