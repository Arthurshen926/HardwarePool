#![forbid(unsafe_code)]

//! Bounded DSU v1001 motion-controller projection boundary.
//!
//! The deterministic codec and IMU projection are transport-free. The optional
//! endpoint binds only IPv4 loopback, and the optional caller-owned worker has
//! bounded input and polling budgets. This crate does not access hardware, own
//! CapyIO Route/session state, select an emulator or accept remote clients.

mod crc32;
mod motion;
mod protocol;
mod transport;
mod worker;

pub use crc32::crc32_ieee;
pub use motion::{
    AxisPermutation, AxisSign, DsuMotionMapping, DsuMotionSample, MotionProjectionError,
    SignedSourceAxis, SourceAxis, project_imu_envelope,
};
pub use protocol::{
    DSU_PAD_DATA_PACKET_BYTES, DSU_PROTOCOL_VERSION, DsuControlsMapping, DsuFaceButtonLayout,
    DsuPacketError, DsuPadSelector, DsuRequest, DsuRequestedSlots, MAX_DSU_DATAGRAM_BYTES,
    encode_neutral_pad_data, encode_pad_data, encode_port_info_response, encode_version_response,
    parse_client_request,
};
pub use transport::{
    DEFAULT_DSU_DATAGRAMS_PER_POLL, DEFAULT_DSU_SUBSCRIBER_CAPACITY,
    DEFAULT_DSU_SUBSCRIPTION_TTL_MILLIS, DSU_CONVENTIONAL_PORT, DsuLoopbackConfig,
    DsuLoopbackServer, DsuPollStats, DsuPublishStats, DsuTransportError,
    MAX_DSU_DATAGRAMS_PER_POLL, MAX_DSU_SUBSCRIBERS, MAX_DSU_SUBSCRIPTION_TTL_MILLIS,
    MIN_DSU_SUBSCRIPTION_TTL_MILLIS,
};
pub use worker::{
    DEFAULT_DSU_CONTROLS_QUEUE_CAPACITY, DEFAULT_DSU_WORKER_POLL_INTERVAL,
    DEFAULT_DSU_WORKER_QUEUE_CAPACITY, DsuGamepadWorkerSender, DsuImuWorker, DsuImuWorkerConfig,
    DsuImuWorkerSender, DsuImuWorkerStats, DsuNeutralOutcome, DsuSubmitOutcome, DsuWorkerError,
    MAX_DSU_WORKER_POLL_INTERVAL, MAX_DSU_WORKER_QUEUE_CAPACITY,
};

pub const IMPLEMENTATION_STATUS: &str = "capy-gamepad-002c-isolated-input-queues";
