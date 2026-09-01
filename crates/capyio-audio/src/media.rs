use std::collections::VecDeque;

use capyio_core::{RouteId, SessionId, StreamId};
use serde::{Deserialize, Serialize};

use crate::{AudioDataError, AudioEncoding, AudioFrame, AudioStreamSpec};

/// Stable semantic Profile carried by this media contract.
pub const AUDIO_FRAMES_PROFILE_V1: &str = "capyio.audio.frames/1";

/// Conservative semantic packet bound before a concrete transport selects MTU/fragmentation.
pub const MAX_AUDIO_MEDIA_PACKET_PAYLOAD_BYTES: usize = 1024 * 1024;
/// Maximum number of retained packets in the reference worker queue.
pub const MAX_AUDIO_PACKET_QUEUE_PACKETS: usize = 512;
/// Maximum aggregate payload retained by one reference worker queue.
pub const MAX_AUDIO_PACKET_QUEUE_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;

/// Exact control-to-media binding for one directed audio Route epoch.
///
/// This value intentionally contains no microphone/Speaker or Source/Sink role.
/// Direction remains on the bound Route's Ports. A concrete transport must bind
/// this value to its authenticated control Session rather than accepting IDs
/// supplied by an unauthenticated media peer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioMediaStreamBinding {
    pub session_id: SessionId,
    pub route_id: RouteId,
    pub stream_id: StreamId,
    pub stream_epoch: u32,
    pub selected_spec: AudioStreamSpec,
}

impl AudioMediaStreamBinding {
    pub fn validate(&self) -> Result<(), AudioDataError> {
        if self.session_id.as_uuid().is_nil() {
            return Err(AudioDataError::InvalidMediaBinding(
                "Session ID must not be nil".to_owned(),
            ));
        }
        if self.route_id.as_uuid().is_nil() {
            return Err(AudioDataError::InvalidMediaBinding(
                "Route ID must not be nil".to_owned(),
            ));
        }
        if self.stream_id.as_uuid().is_nil() {
            return Err(AudioDataError::InvalidMediaBinding(
                "Stream ID must not be nil".to_owned(),
            ));
        }
        if self.stream_epoch == 0 {
            return Err(AudioDataError::InvalidMediaBinding(
                "stream epoch must be positive".to_owned(),
            ));
        }

        self.selected_spec.validate()?;
        let samples_per_packet =
            self.selected_spec
                .format
                .samples_per_frame()
                .ok_or_else(|| {
                    AudioDataError::InvalidMediaBinding(
                        "selected sample rate and frame duration do not produce an integral frame"
                            .to_owned(),
                    )
                })?;
        if samples_per_packet == 0 {
            return Err(AudioDataError::InvalidMediaBinding(
                "selected stream produces an empty media packet".to_owned(),
            ));
        }

        if self.selected_spec.encoding.encoding == AudioEncoding::Pcm {
            let required = pcm_payload_bytes(&self.selected_spec, samples_per_packet)?;
            if required > MAX_AUDIO_MEDIA_PACKET_PAYLOAD_BYTES {
                return Err(AudioDataError::MediaPacketPayloadTooLarge {
                    limit: MAX_AUDIO_MEDIA_PACKET_PAYLOAD_BYTES,
                });
            }
        }
        Ok(())
    }

    pub fn samples_per_packet(&self) -> Result<u32, AudioDataError> {
        self.validate()?;
        self.selected_spec
            .format
            .samples_per_frame()
            .ok_or_else(|| {
                AudioDataError::InvalidMediaBinding(
                    "selected sample rate and frame duration do not produce an integral frame"
                        .to_owned(),
                )
            })
    }
}

/// One PCM or encoded audio packet governed by an `AudioMediaStreamBinding`.
///
/// Encoding and format are not repeated per packet, preventing a packet from
/// silently changing the already selected stream contract. This is a semantic
/// in-process value, not a public network byte layout.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioMediaPacket {
    pub stream_id: StreamId,
    pub stream_epoch: u32,
    pub sequence: u64,
    pub source_timestamp_micros: u64,
    pub first_sample_index: u64,
    pub sample_count: u32,
    pub discontinuity: bool,
    pub payload: Vec<u8>,
}

impl AudioMediaPacket {
    pub fn validate_against(
        &self,
        binding: &AudioMediaStreamBinding,
    ) -> Result<(), AudioDataError> {
        binding.validate()?;
        if self.stream_id != binding.stream_id {
            return Err(AudioDataError::WrongMediaStream);
        }
        if self.stream_epoch != binding.stream_epoch {
            return Err(AudioDataError::WrongMediaEpoch {
                expected: binding.stream_epoch,
                actual: self.stream_epoch,
            });
        }

        let expected_samples = binding.samples_per_packet()?;
        if self.sample_count != expected_samples {
            return Err(AudioDataError::MediaPacketSampleCount {
                expected: expected_samples,
                actual: self.sample_count,
            });
        }
        self.first_sample_index
            .checked_add(u64::from(self.sample_count))
            .ok_or(AudioDataError::SizeOverflow)?;

        if self.payload.len() > MAX_AUDIO_MEDIA_PACKET_PAYLOAD_BYTES {
            return Err(AudioDataError::MediaPacketPayloadTooLarge {
                limit: MAX_AUDIO_MEDIA_PACKET_PAYLOAD_BYTES,
            });
        }

        match binding.selected_spec.encoding.encoding {
            AudioEncoding::Pcm => {
                let expected = pcm_payload_bytes(&binding.selected_spec, self.sample_count)?;
                if self.payload.len() != expected {
                    return Err(AudioDataError::PayloadLength {
                        expected,
                        actual: self.payload.len(),
                    });
                }
            }
            AudioEncoding::Opus if self.payload.is_empty() => {
                return Err(AudioDataError::EmptyEncodedMediaPacket);
            }
            AudioEncoding::Opus => {}
        }
        Ok(())
    }

    /// Converts one decoded PCM frame to the common media packet contract.
    pub fn from_pcm_frame(
        frame: AudioFrame,
        binding: &AudioMediaStreamBinding,
    ) -> Result<Self, AudioDataError> {
        if binding.selected_spec.encoding.encoding != AudioEncoding::Pcm {
            return Err(AudioDataError::NonPcmFrameConversion);
        }
        frame.validate(&binding.selected_spec.format)?;
        let packet = Self {
            stream_id: frame.stream_id,
            stream_epoch: frame.stream_epoch,
            sequence: frame.sequence,
            source_timestamp_micros: frame.source_timestamp_micros,
            first_sample_index: frame.first_sample_index,
            sample_count: frame.sample_count,
            discontinuity: frame.discontinuity,
            payload: frame.payload,
        };
        packet.validate_against(binding)?;
        Ok(packet)
    }

    /// Converts a common PCM packet back to the decoded engine frame.
    pub fn into_pcm_frame(
        self,
        binding: &AudioMediaStreamBinding,
    ) -> Result<AudioFrame, AudioDataError> {
        if binding.selected_spec.encoding.encoding != AudioEncoding::Pcm {
            return Err(AudioDataError::NonPcmFrameConversion);
        }
        self.validate_against(binding)?;
        Ok(AudioFrame {
            stream_id: self.stream_id,
            stream_epoch: self.stream_epoch,
            sequence: self.sequence,
            source_timestamp_micros: self.source_timestamp_micros,
            first_sample_index: self.first_sample_index,
            sample_count: self.sample_count,
            discontinuity: self.discontinuity,
            payload: self.payload,
        })
    }
}

/// Explicit outcomes at the bounded platform-engine/transport-worker seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PacketQueuePushOutcome {
    Accepted,
    WrongStream,
    WrongEpoch,
    PacketCapacityReached,
    ByteCapacityReached,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AudioPacketQueueStats {
    pub accepted: u64,
    pub emitted: u64,
    pub wrong_stream: u64,
    pub wrong_epoch: u64,
    pub invalid_packets: u64,
    pub packet_capacity_drops: u64,
    pub byte_capacity_drops: u64,
}

/// Deterministic bounded reference queue for a media worker thread.
///
/// This is not a lock-free callback ring and must not be used directly from a
/// platform real-time audio callback.
#[derive(Clone, Debug)]
pub struct BoundedAudioPacketQueue {
    binding: AudioMediaStreamBinding,
    packet_capacity: usize,
    payload_byte_capacity: usize,
    queued_payload_bytes: usize,
    packets: VecDeque<AudioMediaPacket>,
    stats: AudioPacketQueueStats,
}

impl BoundedAudioPacketQueue {
    pub fn new(
        binding: AudioMediaStreamBinding,
        packet_capacity: usize,
        payload_byte_capacity: usize,
    ) -> Result<Self, AudioDataError> {
        binding.validate()?;
        if packet_capacity == 0 || packet_capacity > MAX_AUDIO_PACKET_QUEUE_PACKETS {
            return Err(AudioDataError::InvalidPacketQueueCapacity {
                limit: MAX_AUDIO_PACKET_QUEUE_PACKETS,
            });
        }
        if payload_byte_capacity == 0
            || payload_byte_capacity > MAX_AUDIO_PACKET_QUEUE_PAYLOAD_BYTES
        {
            return Err(AudioDataError::InvalidPacketQueueByteCapacity {
                limit: MAX_AUDIO_PACKET_QUEUE_PAYLOAD_BYTES,
            });
        }
        if binding.selected_spec.encoding.encoding == AudioEncoding::Pcm {
            let required =
                pcm_payload_bytes(&binding.selected_spec, binding.samples_per_packet()?)?;
            if required > payload_byte_capacity {
                return Err(AudioDataError::PacketQueueCannotHoldPcmFrame {
                    capacity: payload_byte_capacity,
                    required,
                });
            }
        }

        Ok(Self {
            binding,
            packet_capacity,
            payload_byte_capacity,
            queued_payload_bytes: 0,
            packets: VecDeque::with_capacity(packet_capacity),
            stats: AudioPacketQueueStats::default(),
        })
    }

    pub fn try_push(
        &mut self,
        packet: AudioMediaPacket,
    ) -> Result<PacketQueuePushOutcome, AudioDataError> {
        if packet.stream_id != self.binding.stream_id {
            self.stats.wrong_stream = self.stats.wrong_stream.saturating_add(1);
            return Ok(PacketQueuePushOutcome::WrongStream);
        }
        if packet.stream_epoch != self.binding.stream_epoch {
            self.stats.wrong_epoch = self.stats.wrong_epoch.saturating_add(1);
            return Ok(PacketQueuePushOutcome::WrongEpoch);
        }
        if let Err(error) = packet.validate_against(&self.binding) {
            self.stats.invalid_packets = self.stats.invalid_packets.saturating_add(1);
            return Err(error);
        }
        if self.packets.len() >= self.packet_capacity {
            self.stats.packet_capacity_drops = self.stats.packet_capacity_drops.saturating_add(1);
            return Ok(PacketQueuePushOutcome::PacketCapacityReached);
        }
        let Some(next_payload_bytes) = self.queued_payload_bytes.checked_add(packet.payload.len())
        else {
            self.stats.byte_capacity_drops = self.stats.byte_capacity_drops.saturating_add(1);
            return Ok(PacketQueuePushOutcome::ByteCapacityReached);
        };
        if next_payload_bytes > self.payload_byte_capacity {
            self.stats.byte_capacity_drops = self.stats.byte_capacity_drops.saturating_add(1);
            return Ok(PacketQueuePushOutcome::ByteCapacityReached);
        }

        self.queued_payload_bytes = next_payload_bytes;
        self.packets.push_back(packet);
        self.stats.accepted = self.stats.accepted.saturating_add(1);
        Ok(PacketQueuePushOutcome::Accepted)
    }

    pub fn pop(&mut self) -> Option<AudioMediaPacket> {
        let packet = self.packets.pop_front()?;
        self.queued_payload_bytes = self
            .queued_payload_bytes
            .saturating_sub(packet.payload.len());
        self.stats.emitted = self.stats.emitted.saturating_add(1);
        Some(packet)
    }

    #[must_use]
    pub const fn binding(&self) -> &AudioMediaStreamBinding {
        &self.binding
    }

    #[must_use]
    pub fn queued_packets(&self) -> usize {
        self.packets.len()
    }

    #[must_use]
    pub const fn queued_payload_bytes(&self) -> usize {
        self.queued_payload_bytes
    }

    #[must_use]
    pub const fn stats(&self) -> AudioPacketQueueStats {
        self.stats
    }
}

fn pcm_payload_bytes(spec: &AudioStreamSpec, sample_count: u32) -> Result<usize, AudioDataError> {
    let bytes = u64::from(sample_count)
        .checked_mul(u64::from(spec.format.channels))
        .and_then(|value| {
            value.checked_mul(u64::from(spec.format.sample_format.bytes_per_sample()))
        })
        .ok_or(AudioDataError::SizeOverflow)?;
    usize::try_from(bytes).map_err(|_| AudioDataError::SizeOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AudioEncodingSpec;

    fn binding(spec: AudioStreamSpec) -> AudioMediaStreamBinding {
        AudioMediaStreamBinding {
            session_id: SessionId::new(),
            route_id: RouteId::new(),
            stream_id: StreamId::new(),
            stream_epoch: 1,
            selected_spec: spec,
        }
    }

    fn packet(binding: &AudioMediaStreamBinding, sequence: u64) -> AudioMediaPacket {
        let samples = binding.samples_per_packet().expect("samples");
        let payload_len = match binding.selected_spec.encoding.encoding {
            AudioEncoding::Pcm => pcm_payload_bytes(&binding.selected_spec, samples).unwrap(),
            AudioEncoding::Opus => 32,
        };
        AudioMediaPacket {
            stream_id: binding.stream_id,
            stream_epoch: binding.stream_epoch,
            sequence,
            source_timestamp_micros: sequence.saturating_mul(10_000),
            first_sample_index: sequence.saturating_mul(u64::from(samples)),
            sample_count: samples,
            discontinuity: false,
            payload: vec![0; payload_len],
        }
    }

    #[test]
    fn binding_rejects_nil_identity_and_zero_epoch() {
        let mut invalid = binding(AudioStreamSpec::voice_interactive());
        invalid.session_id = "00000000-0000-0000-0000-000000000000"
            .parse()
            .expect("nil UUID");
        assert!(matches!(
            invalid.validate(),
            Err(AudioDataError::InvalidMediaBinding(_))
        ));

        let mut invalid = binding(AudioStreamSpec::voice_interactive());
        invalid.stream_epoch = 0;
        assert!(matches!(
            invalid.validate(),
            Err(AudioDataError::InvalidMediaBinding(_))
        ));
    }

    #[test]
    fn binding_rejects_non_integral_and_oversized_pcm_packets() {
        let mut non_integral = binding(AudioStreamSpec::media_balanced());
        non_integral.selected_spec.format.sample_rate_hz = 44_100;
        non_integral.selected_spec.format.frame_duration_micros = 2_500;
        assert!(matches!(
            non_integral.validate(),
            Err(AudioDataError::InvalidMediaBinding(_))
        ));

        let mut oversized = binding(AudioStreamSpec::music_lossless());
        oversized.selected_spec.format.sample_rate_hz = 384_000;
        oversized.selected_spec.format.sample_format = crate::AudioSampleFormat::FloatF32Le;
        oversized.selected_spec.format.channels = 32;
        oversized.selected_spec.format.channel_layout = crate::ChannelLayout::Discrete;
        oversized.selected_spec.format.frame_duration_micros = 60_000;
        assert_eq!(
            oversized.validate(),
            Err(AudioDataError::MediaPacketPayloadTooLarge {
                limit: MAX_AUDIO_MEDIA_PACKET_PAYLOAD_BYTES
            })
        );
    }

    #[test]
    fn pcm_frame_round_trip_preserves_every_field() {
        let binding = binding(AudioStreamSpec::media_balanced());
        let original = AudioFrame {
            stream_id: binding.stream_id,
            stream_epoch: binding.stream_epoch,
            sequence: 9,
            source_timestamp_micros: 90_000,
            first_sample_index: 4_320,
            sample_count: 480,
            discontinuity: true,
            payload: vec![0x5a; 1_920],
        };

        let restored = AudioMediaPacket::from_pcm_frame(original.clone(), &binding)
            .expect("packet")
            .into_pcm_frame(&binding)
            .expect("frame");
        assert_eq!(restored, original);
    }

    #[test]
    fn encoded_packet_is_bounded_without_importing_a_codec() {
        let mut spec = AudioStreamSpec::voice_interactive();
        spec.encoding = AudioEncodingSpec::opus(64_000);
        let binding = binding(spec);
        let mut packet = packet(&binding, 0);
        packet.validate_against(&binding).expect("encoded packet");
        assert!(matches!(
            packet.clone().into_pcm_frame(&binding),
            Err(AudioDataError::NonPcmFrameConversion)
        ));

        packet.payload.clear();
        assert_eq!(
            packet.validate_against(&binding),
            Err(AudioDataError::EmptyEncodedMediaPacket)
        );

        packet.payload = vec![0; MAX_AUDIO_MEDIA_PACKET_PAYLOAD_BYTES + 1];
        assert_eq!(
            packet.validate_against(&binding),
            Err(AudioDataError::MediaPacketPayloadTooLarge {
                limit: MAX_AUDIO_MEDIA_PACKET_PAYLOAD_BYTES
            })
        );
    }

    #[test]
    fn packet_timeline_and_queue_configuration_are_bounded() {
        let binding = binding(AudioStreamSpec::voice_interactive());
        let mut overflowing = packet(&binding, 0);
        overflowing.first_sample_index = u64::MAX;
        assert_eq!(
            overflowing.validate_against(&binding),
            Err(AudioDataError::SizeOverflow)
        );

        assert!(matches!(
            BoundedAudioPacketQueue::new(binding.clone(), 0, 960),
            Err(AudioDataError::InvalidPacketQueueCapacity { .. })
        ));
        assert!(matches!(
            BoundedAudioPacketQueue::new(binding.clone(), MAX_AUDIO_PACKET_QUEUE_PACKETS + 1, 960),
            Err(AudioDataError::InvalidPacketQueueCapacity { .. })
        ));
        assert!(matches!(
            BoundedAudioPacketQueue::new(binding.clone(), 1, 0),
            Err(AudioDataError::InvalidPacketQueueByteCapacity { .. })
        ));
        assert!(matches!(
            BoundedAudioPacketQueue::new(
                binding.clone(),
                1,
                MAX_AUDIO_PACKET_QUEUE_PAYLOAD_BYTES + 1
            ),
            Err(AudioDataError::InvalidPacketQueueByteCapacity { .. })
        ));
        assert_eq!(
            BoundedAudioPacketQueue::new(binding, 1, 959).unwrap_err(),
            AudioDataError::PacketQueueCannotHoldPcmFrame {
                capacity: 959,
                required: 960
            }
        );
    }

    #[test]
    fn queue_rejects_wrong_identity_invalid_payload_and_both_capacity_limits() {
        let binding = binding(AudioStreamSpec::voice_interactive());
        let packet_bytes = packet(&binding, 0).payload.len();
        let mut queue =
            BoundedAudioPacketQueue::new(binding.clone(), 2, packet_bytes * 2).expect("queue");

        let mut wrong_stream = packet(&binding, 0);
        wrong_stream.stream_id = StreamId::new();
        assert_eq!(
            queue.try_push(wrong_stream).expect("outcome"),
            PacketQueuePushOutcome::WrongStream
        );

        let mut wrong_epoch = packet(&binding, 0);
        wrong_epoch.stream_epoch = 2;
        assert_eq!(
            queue.try_push(wrong_epoch).expect("outcome"),
            PacketQueuePushOutcome::WrongEpoch
        );

        let mut invalid = packet(&binding, 0);
        invalid.payload.pop();
        assert!(matches!(
            queue.try_push(invalid),
            Err(AudioDataError::PayloadLength { .. })
        ));

        assert_eq!(
            queue.try_push(packet(&binding, 0)).unwrap(),
            PacketQueuePushOutcome::Accepted
        );
        assert_eq!(
            queue.try_push(packet(&binding, 1)).unwrap(),
            PacketQueuePushOutcome::Accepted
        );
        assert_eq!(
            queue.try_push(packet(&binding, 2)).unwrap(),
            PacketQueuePushOutcome::PacketCapacityReached
        );

        assert_eq!(queue.pop().expect("first").sequence, 0);
        assert_eq!(queue.queued_payload_bytes(), packet_bytes);
        assert_eq!(queue.stats().invalid_packets, 1);
        assert_eq!(queue.stats().wrong_stream, 1);
        assert_eq!(queue.stats().wrong_epoch, 1);
        assert_eq!(queue.stats().packet_capacity_drops, 1);

        let mut byte_limited =
            BoundedAudioPacketQueue::new(binding.clone(), 2, packet_bytes).expect("queue");
        assert_eq!(
            byte_limited.try_push(packet(&binding, 0)).unwrap(),
            PacketQueuePushOutcome::Accepted
        );
        assert_eq!(
            byte_limited.try_push(packet(&binding, 1)).unwrap(),
            PacketQueuePushOutcome::ByteCapacityReached
        );
        assert_eq!(byte_limited.stats().byte_capacity_drops, 1);
    }
}
