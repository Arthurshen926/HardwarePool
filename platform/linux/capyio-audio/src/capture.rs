//! Bounded stereo PCM assembly at an already-authorized worker boundary.
//! The caller owns consent, fresh Route/Source lease checks and UDP delivery.
//! This pump neither authorizes capture nor opens a process or network socket.
use capyio_audio::{AudioMediaPacket, AudioMediaStreamBinding, AudioStreamSpec};
use std::{io, time::Duration};

/// 480 sample frames, two channels, signed little-endian 16-bit samples.
pub const CAPTURE_PCM_PACKET_BYTES: usize = 1_920;
const SAMPLES_PER_PACKET: u32 = 480;
const MICROS_PER_PACKET: u64 = 10_000;

/// Worker-only source. Implementations must return promptly without waiting for
/// PCM; neither operation is an OS real-time callback or an authorization check.
pub trait PcmCaptureSource {
    fn try_read_pcm(&mut self, bytes: &mut [u8]) -> io::Result<usize>;
    fn check_alive(&mut self) -> io::Result<()>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CaptureMetrics {
    /// Complete locally assembled packets, not delivered or rendered packets.
    pub packets_captured: u64,
    pub bytes_read: u64,
}

/// One fixed-size assembly buffer and at most one nonblocking read per poll.
/// Partial sample frames are retained without padding, reordering or conversion.
/// The partial-frame deadline never renews on progress. Every error is terminal;
/// restarting requires a fresh owner, binding and explicit resource admission.
pub struct CapturePump {
    binding: AudioMediaStreamBinding,
    pending: [u8; CAPTURE_PCM_PACKET_BYTES],
    filled: usize,
    partial_since: Option<Duration>,
    sequence: u64,
    last_tick: Duration,
    lifetime: Duration,
    terminal: bool,
    metrics: CaptureMetrics,
}

impl CapturePump {
    /// Exact dynamic binding only. No implicit static lab identity or fallback.
    pub fn with_binding(lifetime: Duration, binding: AudioMediaStreamBinding) -> io::Result<Self> {
        if !(Duration::from_secs(1)..=Duration::from_secs(300)).contains(&lifetime)
            || binding.selected_spec != AudioStreamSpec::media_balanced()
            || binding.validate().is_err()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid stereo PCM binding or capture lifetime",
            ));
        }
        Ok(Self {
            binding,
            pending: [0; CAPTURE_PCM_PACKET_BYTES],
            filled: 0,
            partial_since: None,
            sequence: 0,
            last_tick: Duration::ZERO,
            lifetime,
            terminal: false,
            metrics: CaptureMetrics::default(),
        })
    }

    /// The owner must revalidate finite authority before calling and again before
    /// sending the returned packet. A successful poll is not permission to send.
    pub fn poll(
        &mut self,
        source: &mut impl PcmCaptureSource,
        now: Duration,
    ) -> io::Result<Option<AudioMediaPacket>> {
        if self.terminal {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "PCM capture pump is stopped",
            ));
        }
        let result = self.poll_inner(source, now);
        if result.is_err() {
            self.discard();
        }
        result
    }

    fn poll_inner(
        &mut self,
        source: &mut impl PcmCaptureSource,
        now: Duration,
    ) -> io::Result<Option<AudioMediaPacket>> {
        if now < self.last_tick {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "capture clock regressed",
            ));
        }
        if now >= self.lifetime {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "capture lifetime expired",
            ));
        }
        self.last_tick = now;
        if self
            .partial_since
            .is_some_and(|since| now.saturating_sub(since) >= crate::MAX_STALL)
        {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "partial PCM capture exceeded assembly budget",
            ));
        }
        source.check_alive()?;
        let remaining = CAPTURE_PCM_PACKET_BYTES - self.filled;
        let bytes = match source.try_read_pcm(&mut self.pending[self.filled..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    if self.filled == 0 {
                        "PCM capture reached EOF"
                    } else {
                        "PCM capture ended with a truncated packet"
                    },
                ));
            }
            Ok(bytes) if bytes <= remaining => bytes,
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "PCM capture returned an invalid read length",
                ));
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        self.partial_since.get_or_insert(now);
        self.filled += bytes;
        self.metrics.bytes_read = self.metrics.bytes_read.saturating_add(bytes as u64);
        if self.filled != CAPTURE_PCM_PACKET_BYTES {
            return Ok(None);
        }

        let exhausted =
            || io::Error::new(io::ErrorKind::InvalidData, "PCM capture timeline exhausted");
        let next_sequence = self.sequence.checked_add(1).ok_or_else(exhausted)?;
        let first_sample_index = self
            .sequence
            .checked_mul(u64::from(SAMPLES_PER_PACKET))
            .ok_or_else(exhausted)?;
        first_sample_index
            .checked_add(u64::from(SAMPLES_PER_PACKET))
            .ok_or_else(exhausted)?;
        // pw-cat does not expose the hardware clock. This is a stream-relative
        // PCM sample timeline, not a wall-clock timestamp or cross-device sync.
        let source_timestamp_micros = self
            .sequence
            .checked_mul(MICROS_PER_PACKET)
            .and_then(|micros| micros.checked_add(1))
            .ok_or_else(exhausted)?;
        let packet = AudioMediaPacket {
            stream_id: self.binding.stream_id,
            stream_epoch: self.binding.stream_epoch,
            sequence: self.sequence,
            source_timestamp_micros,
            first_sample_index,
            sample_count: SAMPLES_PER_PACKET,
            discontinuity: false,
            payload: self.pending.to_vec(),
        };
        packet
            .validate_against(&self.binding)
            .map_err(io::Error::other)?;
        self.sequence = next_sequence;
        self.pending.fill(0);
        self.filled = 0;
        self.partial_since = None;
        self.metrics.packets_captured = self.metrics.packets_captured.saturating_add(1);
        Ok(Some(packet))
    }

    /// Terminal, idempotent local discard. The owner still closes its resources.
    pub fn discard(&mut self) {
        self.terminal = true;
        self.pending.fill(0);
        self.filled = 0;
        self.partial_since = None;
    }

    pub const fn metrics(&self) -> CaptureMetrics {
        self.metrics
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use capyio_core::{RouteId, SessionId, StreamId};

    pub(crate) fn binding() -> AudioMediaStreamBinding {
        AudioMediaStreamBinding {
            session_id: SessionId::new(),
            route_id: RouteId::new(),
            stream_id: StreamId::new(),
            stream_epoch: 9,
            selected_spec: AudioStreamSpec::media_balanced(),
        }
    }

    struct Source {
        bytes: Vec<u8>,
        offset: usize,
        limit: usize,
        alive: bool,
        error: Option<io::ErrorKind>,
        reads: usize,
        checks: usize,
    }
    impl Source {
        fn new(bytes: Vec<u8>, limit: usize) -> Self {
            Self {
                bytes,
                offset: 0,
                limit,
                alive: true,
                error: None,
                reads: 0,
                checks: 0,
            }
        }
    }
    impl PcmCaptureSource for Source {
        fn try_read_pcm(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            if let Some(kind) = self.error {
                return Err(kind.into());
            }
            let available = self.bytes.len() - self.offset;
            let count = bytes.len().min(available).min(self.limit);
            bytes[..count].copy_from_slice(&self.bytes[self.offset..self.offset + count]);
            self.offset += count;
            Ok(count)
        }
        fn check_alive(&mut self) -> io::Result<()> {
            self.checks += 1;
            if self.alive {
                Ok(())
            } else {
                Err(io::ErrorKind::BrokenPipe.into())
            }
        }
    }

    #[test]
    fn capture_requires_exact_dynamic_stereo_binding_and_finite_lifetime() {
        for lifetime in [
            Duration::ZERO,
            Duration::from_millis(999),
            Duration::from_secs(301),
        ] {
            assert!(CapturePump::with_binding(lifetime, binding()).is_err());
        }
        for lifetime in [Duration::from_secs(1), Duration::from_secs(300)] {
            assert!(CapturePump::with_binding(lifetime, binding()).is_ok());
        }
        for spec in [
            AudioStreamSpec::voice_interactive(),
            AudioStreamSpec::music_lossless(),
        ] {
            let mut wrong = binding();
            wrong.selected_spec = spec;
            assert!(CapturePump::with_binding(Duration::from_secs(1), wrong).is_err());
        }
        let mut wrong = binding();
        wrong.stream_epoch = 0;
        assert!(CapturePump::with_binding(Duration::from_secs(1), wrong).is_err());
        let mut wrong = binding();
        wrong.stream_id = "00000000-0000-0000-0000-000000000000".parse().unwrap();
        assert!(CapturePump::with_binding(Duration::from_secs(1), wrong).is_err());
    }

    #[test]
    fn capture_partial_reads_preserve_exact_stereo_packets_and_timeline() {
        let media = binding();
        let data: Vec<_> = (0..CAPTURE_PCM_PACKET_BYTES * 2)
            .map(|i| (i % 251) as u8)
            .collect();
        let mut source = Source::new(data.clone(), 17);
        let mut pump = CapturePump::with_binding(Duration::from_secs(1), media.clone()).unwrap();
        let mut packets = Vec::new();
        for tick in 0..228 {
            let previous_reads = source.reads;
            if let Some(packet) = pump.poll(&mut source, Duration::from_millis(tick)).unwrap() {
                packet.validate_against(&media).unwrap();
                packets.push(packet);
                if packets.len() == 2 {
                    break;
                }
            }
            assert_eq!(source.reads, previous_reads + 1);
        }
        assert_eq!(packets.len(), 2);
        for (index, packet) in packets.iter().enumerate() {
            assert_eq!(packet.stream_id, media.stream_id);
            assert_eq!(packet.stream_epoch, 9);
            assert_eq!(packet.sequence, index as u64);
            assert_eq!(packet.first_sample_index, index as u64 * 480);
            assert_eq!(packet.source_timestamp_micros, index as u64 * 10_000 + 1);
            assert_eq!(packet.sample_count, 480);
            assert!(!packet.discontinuity);
            assert_eq!(
                packet.payload,
                data[index * CAPTURE_PCM_PACKET_BYTES..(index + 1) * CAPTURE_PCM_PACKET_BYTES]
            );
        }
        assert_eq!(
            pump.metrics(),
            CaptureMetrics {
                packets_captured: 2,
                bytes_read: 3_840
            }
        );
        assert!(pump.pending.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn capture_trickled_partial_packet_cannot_extend_absolute_assembly_budget() {
        let mut pump = CapturePump::with_binding(Duration::from_secs(2), binding()).unwrap();
        let mut source = Source::new(vec![1; CAPTURE_PCM_PACKET_BYTES], 1);
        for tick in [50, 100, 200, 299] {
            assert!(
                pump.poll(&mut source, Duration::from_millis(tick))
                    .unwrap()
                    .is_none()
            );
        }
        let reads = source.reads;
        assert_eq!(
            pump.poll(&mut source, Duration::from_millis(300))
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(source.reads, reads);
        assert_eq!(pump.filled, 0);
        assert!(pump.pending.iter().all(|byte| *byte == 0));
        assert!(pump.poll(&mut source, Duration::from_millis(301)).is_err());
        assert_eq!(source.reads, reads);
    }

    #[test]
    fn capture_idle_and_interrupted_reads_are_bounded_without_padding() {
        let mut pump = CapturePump::with_binding(Duration::from_secs(1), binding()).unwrap();
        let mut source = Source::new(vec![1; CAPTURE_PCM_PACKET_BYTES], CAPTURE_PCM_PACKET_BYTES);
        for (index, kind) in [io::ErrorKind::WouldBlock, io::ErrorKind::Interrupted]
            .into_iter()
            .enumerate()
        {
            source.error = Some(kind);
            assert!(
                pump.poll(&mut source, Duration::from_millis(index as u64 * 300))
                    .unwrap()
                    .is_none()
            );
            assert_eq!(source.reads, index + 1);
        }
        assert_eq!(pump.metrics(), CaptureMetrics::default());
        source.error = None;
        assert!(
            pump.poll(&mut source, Duration::from_millis(600))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn capture_eof_and_truncation_are_terminal_without_emitting_padding() {
        for count in [0, 1, CAPTURE_PCM_PACKET_BYTES - 1] {
            let mut pump = CapturePump::with_binding(Duration::from_secs(1), binding()).unwrap();
            let mut source = Source::new(vec![2; count], CAPTURE_PCM_PACKET_BYTES);
            if count != 0 {
                assert!(pump.poll(&mut source, Duration::ZERO).unwrap().is_none());
            }
            assert_eq!(
                pump.poll(&mut source, Duration::ZERO).unwrap_err().kind(),
                io::ErrorKind::UnexpectedEof
            );
            assert_eq!(pump.metrics().packets_captured, 0);
            assert_eq!(pump.filled, 0);
            let reads = source.reads;
            assert!(pump.poll(&mut source, Duration::ZERO).is_err());
            assert_eq!(source.reads, reads);
        }
    }

    #[test]
    fn capture_lifetime_clock_failure_and_explicit_discard_stop_before_read() {
        for (initial, next, expected) in [
            (100, 99, io::ErrorKind::InvalidInput),
            (0, 1_000, io::ErrorKind::TimedOut),
        ] {
            let mut pump = CapturePump::with_binding(Duration::from_secs(1), binding()).unwrap();
            let mut source = Source::new(vec![3; CAPTURE_PCM_PACKET_BYTES], 1);
            assert!(
                pump.poll(&mut source, Duration::from_millis(initial))
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                pump.poll(&mut source, Duration::from_millis(next))
                    .unwrap_err()
                    .kind(),
                expected
            );
            assert_eq!(source.reads, 1);
            assert!(pump.pending.iter().all(|byte| *byte == 0));
        }
        let mut pump = CapturePump::with_binding(Duration::from_secs(1), binding()).unwrap();
        let mut source = Source::new(vec![3; CAPTURE_PCM_PACKET_BYTES], 1);
        pump.discard();
        pump.discard();
        assert!(pump.poll(&mut source, Duration::ZERO).is_err());
        assert_eq!(source.checks, 0);
        assert_eq!(source.reads, 0);
    }

    #[test]
    fn capture_source_failure_invalid_read_and_timeline_exhaustion_fail_closed() {
        let mut pump = CapturePump::with_binding(Duration::from_secs(1), binding()).unwrap();
        let mut source = Source::new(vec![4; CAPTURE_PCM_PACKET_BYTES], CAPTURE_PCM_PACKET_BYTES);
        source.alive = false;
        assert_eq!(
            pump.poll(&mut source, Duration::ZERO).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
        assert_eq!(source.reads, 0);
        source.alive = true;
        assert!(pump.poll(&mut source, Duration::ZERO).is_err());
        assert_eq!(source.reads, 0);

        let mut pump = CapturePump::with_binding(Duration::from_secs(1), binding()).unwrap();
        source.error = Some(io::ErrorKind::ConnectionReset);
        assert_eq!(
            pump.poll(&mut source, Duration::ZERO).unwrap_err().kind(),
            io::ErrorKind::ConnectionReset
        );
        assert!(pump.terminal);

        struct Oversized;
        impl PcmCaptureSource for Oversized {
            fn try_read_pcm(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                Ok(bytes.len() + 1)
            }
            fn check_alive(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut pump = CapturePump::with_binding(Duration::from_secs(1), binding()).unwrap();
        assert_eq!(
            pump.poll(&mut Oversized, Duration::ZERO)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert!(pump.terminal);

        let mut pump = CapturePump::with_binding(Duration::from_secs(1), binding()).unwrap();
        pump.sequence = u64::MAX / MICROS_PER_PACKET + 1;
        source.error = None;
        assert_eq!(
            pump.poll(&mut source, Duration::ZERO).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(pump.metrics().packets_captured, 0);
        assert!(pump.terminal);
    }
}
