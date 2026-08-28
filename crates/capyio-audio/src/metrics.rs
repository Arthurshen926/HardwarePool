use serde::{Deserialize, Serialize};

/// Direction-neutral, monotonic counters and optional worker-thread estimates.
///
/// A concrete Adapter may leave fields at zero/`None` when its private data
/// plane cannot observe them. Zero must not be interpreted as proof of no loss
/// unless the owning Adapter declares that counter observable.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioMetricsSnapshot {
    pub media_blocks_produced: u64,
    pub media_blocks_without_consumer: u64,
    pub payload_bytes_transmitted: u64,
    pub packets_transmitted: u64,
    pub packets_received: u64,
    pub packets_lost: u64,
    pub duplicate_packets: u64,
    pub late_packets: u64,
    pub queue_overruns: u64,
    pub queue_underruns: u64,
    pub transport_errors: u64,
    pub discontinuities: u64,
    pub estimated_jitter_micros: Option<u64>,
    pub buffer_level_micros: Option<u64>,
    pub estimated_clock_drift_ppm: Option<i32>,
}
