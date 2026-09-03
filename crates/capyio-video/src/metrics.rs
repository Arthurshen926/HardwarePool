use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoMetrics {
    /// `None` means the owning Adapter cannot observe this counter.
    pub frames_produced: Option<u64>,
    pub frames_delivered: Option<u64>,
    pub frames_dropped_source: Option<u64>,
    pub frames_dropped_transport: Option<u64>,
    pub frames_dropped_sink: Option<u64>,
    pub payload_bytes: Option<u64>,
    pub queue_overruns: Option<u64>,
    pub discontinuities: Option<u64>,
    pub estimated_capture_to_sink_millis: Option<u64>,
    pub estimated_jitter_millis: Option<u64>,
    pub buffer_level_frames: Option<u32>,
}
