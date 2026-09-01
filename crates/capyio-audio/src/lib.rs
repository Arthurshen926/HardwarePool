#![forbid(unsafe_code)]

//! Transport- and platform-independent audio data-plane primitives.
//!
//! This crate defines selected stream contracts, semantic audio frames and
//! packets, bounded worker-thread queues/reordering and a clock-rate estimator.
//! It intentionally does not choose UDP, QUIC, RTP/AOO, an audio API, a codec,
//! or an operating-system projection.

mod backend;
mod drift;
mod error;
mod format;
mod frame;
mod media;
mod metrics;
mod reorder;
mod spec;

pub use backend::{
    AudioTransportBackendContract, AudioTransportEncodingSupport, AudioTransportFieldFidelity,
    AudioTransportInteroperability, AudioTransportMediaAccess, AudioTransportMetadataFidelity,
    AudioTransportSecurity, MAX_AUDIO_TRANSPORT_BACKEND_ID_BYTES,
};
pub use drift::{ClockDriftEstimate, ClockDriftEstimator};
pub use error::AudioDataError;
pub use format::{AudioFormat, AudioProcessingSupport, AudioSampleFormat, ChannelLayout};
pub use frame::AudioFrame;
pub use media::{
    AUDIO_FRAMES_PROFILE_V1, AudioMediaPacket, AudioMediaStreamBinding, AudioPacketQueueStats,
    BoundedAudioPacketQueue, MAX_AUDIO_MEDIA_PACKET_PAYLOAD_BYTES, MAX_AUDIO_PACKET_QUEUE_PACKETS,
    MAX_AUDIO_PACKET_QUEUE_PAYLOAD_BYTES, PacketQueuePushOutcome,
};
pub use metrics::AudioMetricsSnapshot;
pub use reorder::{FrameBufferStats, InsertOutcome, ReorderBuffer};
pub use spec::{
    AudioEncoding, AudioEncodingSpec, AudioProcessingRequest, AudioQosPolicy,
    AudioStreamCapabilities, AudioStreamSpec, AudioUseCase, negotiate_audio_stream,
};
