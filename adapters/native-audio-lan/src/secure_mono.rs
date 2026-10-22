//! Pure strict record admission for the pinned TLS mono contract. No sockets,
//! reordering, packet loss tolerance, discontinuities or plaintext fallback.
use capyio_audio::{AudioMediaPacket, AudioMediaStreamBinding, AudioStreamSpec};

use crate::{NativeLanError, decode_native_lan_fragment, encode_native_lan_fragment};

pub const SECURE_MONO_PCM_RECORD_BYTES: usize = 1064;
pub const SECURE_MONO_PCM_PAYLOAD_BYTES: usize = 960;
pub const SECURE_MONO_PCM_SAMPLES: u32 = 480;
const FRAME_MICROS: u64 = 10_000;

/// One immutable selected specification and one continuous ordered timeline.
/// Every error is terminal. Owners must also bind this to the exact negotiated
/// Route format/QoS, consent, control connection and original finite leases.
#[derive(Debug)]
pub struct SecureMonoPcmAdmission {
    binding: AudioMediaStreamBinding,
    next_sequence: u64,
    next_sample_index: u64,
    next_timestamp: Option<u64>,
    terminal: bool,
}

impl SecureMonoPcmAdmission {
    pub fn new(binding: AudioMediaStreamBinding) -> Result<Self, NativeLanError> {
        binding.validate()?;
        if binding.stream_epoch > i32::MAX as u32
            || binding.selected_spec != AudioStreamSpec::voice_interactive()
        {
            return Err(NativeLanError::InvalidConfiguration(
                "secure mono requires exact voice-interactive PCM and a supported epoch",
            ));
        }
        Ok(Self {
            binding,
            next_sequence: 0,
            next_sample_index: 0,
            next_timestamp: None,
            terminal: false,
        })
    }

    pub fn binding(&self) -> &AudioMediaStreamBinding {
        &self.binding
    }

    pub const fn terminal(&self) -> bool {
        self.terminal
    }

    /// Validate all bytes and metadata before the single fixed-size allocation.
    pub fn admit_record(&mut self, bytes: &[u8]) -> Result<AudioMediaPacket, NativeLanError> {
        let result = self.admit_inner(bytes);
        if result.is_err() {
            self.terminal = true;
        }
        result
    }

    fn admit_inner(&mut self, bytes: &[u8]) -> Result<AudioMediaPacket, NativeLanError> {
        if self.terminal || bytes.len() != SECURE_MONO_PCM_RECORD_BYTES {
            return Err(NativeLanError::InvalidDatagram(
                "terminal or wrong secure mono record size",
            ));
        }
        let fragment = decode_native_lan_fragment(bytes)?;
        if !fragment.matches_binding(&self.binding)
            || fragment.discontinuity
            || fragment.fragment_index != 0
            || fragment.fragment_count != 1
            || fragment.fragment_offset != 0
            || fragment.total_payload_bytes != SECURE_MONO_PCM_PAYLOAD_BYTES
            || fragment.payload.len() != SECURE_MONO_PCM_PAYLOAD_BYTES
            || fragment.sample_count != SECURE_MONO_PCM_SAMPLES
            || fragment.sequence != self.next_sequence
            || fragment.first_sample_index != self.next_sample_index
            || self
                .next_timestamp
                .is_some_and(|value| value != fragment.source_timestamp_micros)
        {
            return Err(NativeLanError::InvalidDatagram(
                "noncanonical secure mono binding or timeline",
            ));
        }
        // Reject overflow now, including the first timestamp, rather than wrap or
        // admit a last frame whose continuation cannot be represented exactly.
        let next_sequence =
            fragment
                .sequence
                .checked_add(1)
                .ok_or(NativeLanError::InvalidDatagram(
                    "secure mono sequence overflow",
                ))?;
        let next_sample_index = fragment
            .first_sample_index
            .checked_add(u64::from(SECURE_MONO_PCM_SAMPLES))
            .ok_or(NativeLanError::InvalidDatagram(
                "secure mono sample index overflow",
            ))?;
        let next_timestamp = fragment
            .source_timestamp_micros
            .checked_add(FRAME_MICROS)
            .ok_or(NativeLanError::InvalidDatagram(
                "secure mono timestamp overflow",
            ))?;
        let packet = AudioMediaPacket {
            stream_id: fragment.stream_id,
            stream_epoch: fragment.stream_epoch,
            sequence: fragment.sequence,
            source_timestamp_micros: fragment.source_timestamp_micros,
            first_sample_index: fragment.first_sample_index,
            sample_count: fragment.sample_count,
            discontinuity: false,
            payload: fragment.payload.to_vec(),
        };
        self.next_sequence = next_sequence;
        self.next_sample_index = next_sample_index;
        self.next_timestamp = Some(next_timestamp);
        Ok(packet)
    }

    /// Sender and receiver apply the same canonical byte admission and timeline.
    pub fn encode_record(
        &mut self,
        packet: &AudioMediaPacket,
    ) -> Result<[u8; SECURE_MONO_PCM_RECORD_BYTES], NativeLanError> {
        let result = (|| {
            if self.terminal {
                return Err(NativeLanError::InvalidDatagram(
                    "secure mono admission is terminal",
                ));
            }
            let mut bytes = [0; SECURE_MONO_PCM_RECORD_BYTES];
            let written = encode_native_lan_fragment(&self.binding, packet, 0, &mut bytes)?;
            if written != SECURE_MONO_PCM_RECORD_BYTES {
                return Err(NativeLanError::InvalidDatagram(
                    "wrong secure mono encoded length",
                ));
            }
            self.admit_inner(&bytes)?;
            Ok(bytes)
        })();
        if result.is_err() {
            self.terminal = true;
        }
        result
    }
}

/// Independent work/rate budget based only on trusted-local monotonic elapsed
/// time. Peer timestamps never create credit or extend this finite lifetime.
#[derive(Debug)]
pub struct SecureMonoPcmRateBudget {
    lifetime: std::time::Duration,
    last_elapsed: std::time::Duration,
    credit_nanos: u64,
    admitted: u64,
    frame_limit: u64,
    terminal: bool,
}

impl SecureMonoPcmRateBudget {
    const FRAME_NANOS: u64 = 10_000_000;
    const BURST_NANOS: u64 = Self::FRAME_NANOS * 4;

    pub fn new(lifetime: std::time::Duration) -> Result<Self, NativeLanError> {
        if lifetime.is_zero() || lifetime > std::time::Duration::from_secs(60) {
            return Err(NativeLanError::InvalidConfiguration(
                "secure mono lifetime must be in (0,60s]",
            ));
        }
        let nanos = u64::try_from(lifetime.as_nanos())
            .map_err(|_| NativeLanError::InvalidConfiguration("secure mono lifetime overflow"))?;
        let frame_limit = nanos.div_ceil(Self::FRAME_NANOS);
        Ok(Self {
            lifetime,
            last_elapsed: std::time::Duration::ZERO,
            credit_nanos: Self::BURST_NANOS,
            admitted: 0,
            frame_limit,
            terminal: false,
        })
    }

    pub const fn terminal(&self) -> bool {
        self.terminal
    }

    pub fn admit(&mut self, elapsed: std::time::Duration) -> Result<(), NativeLanError> {
        let result = self.admit_inner(elapsed);
        if result.is_err() {
            self.terminal = true;
        }
        result
    }

    fn admit_inner(&mut self, elapsed: std::time::Duration) -> Result<(), NativeLanError> {
        if self.terminal
            || elapsed >= self.lifetime
            || elapsed < self.last_elapsed
            || self.admitted >= self.frame_limit
        {
            return Err(NativeLanError::InvalidDatagram(
                "secure mono rate budget terminal, expired or exhausted",
            ));
        }
        let delta =
            elapsed
                .checked_sub(self.last_elapsed)
                .ok_or(NativeLanError::InvalidDatagram(
                    "secure mono clock regressed",
                ))?;
        let accrual = u64::try_from(delta.as_nanos())
            .map_err(|_| NativeLanError::InvalidDatagram("secure mono clock overflow"))?
            .min(Self::BURST_NANOS);
        let credit = self
            .credit_nanos
            .checked_add(accrual)
            .ok_or(NativeLanError::InvalidDatagram("secure mono rate overflow"))?
            .min(Self::BURST_NANOS);
        self.credit_nanos =
            credit
                .checked_sub(Self::FRAME_NANOS)
                .ok_or(NativeLanError::InvalidDatagram(
                    "secure mono burst exceeded",
                ))?;
        self.admitted = self
            .admitted
            .checked_add(1)
            .ok_or(NativeLanError::InvalidDatagram(
                "secure mono frame count overflow",
            ))?;
        self.last_elapsed = elapsed;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_core::{RouteId, SessionId, StreamId};
    use std::time::Duration;

    fn binding() -> AudioMediaStreamBinding {
        AudioMediaStreamBinding {
            session_id: SessionId::new(),
            route_id: RouteId::new(),
            stream_id: StreamId::new(),
            stream_epoch: 1,
            selected_spec: AudioStreamSpec::voice_interactive(),
        }
    }
    fn packet(binding: &AudioMediaStreamBinding, sequence: u64) -> AudioMediaPacket {
        AudioMediaPacket {
            stream_id: binding.stream_id,
            stream_epoch: binding.stream_epoch,
            sequence,
            source_timestamp_micros: 12345 + sequence * 10000,
            first_sample_index: sequence * 480,
            sample_count: 480,
            discontinuity: false,
            payload: vec![0x5a; 960],
        }
    }
    fn raw(binding: &AudioMediaStreamBinding, packet: &AudioMediaPacket) -> [u8; 1064] {
        let mut bytes = [0; 1064];
        assert_eq!(
            encode_native_lan_fragment(binding, packet, 0, &mut bytes).unwrap(),
            1064
        );
        bytes
    }

    #[test]
    fn secure_mono_roundtrip_preserves_exact_contiguous_packets() {
        let binding = binding();
        let mut sender = SecureMonoPcmAdmission::new(binding.clone()).unwrap();
        let mut receiver = SecureMonoPcmAdmission::new(binding.clone()).unwrap();
        for sequence in 0..100 {
            let expected = packet(&binding, sequence);
            let bytes = sender.encode_record(&expected).unwrap();
            assert_eq!(bytes.len(), SECURE_MONO_PCM_RECORD_BYTES);
            assert_eq!(receiver.admit_record(&bytes).unwrap(), expected);
        }
        assert!(!sender.terminal());
        assert_eq!(sender.binding(), &binding);
    }

    #[test]
    fn secure_mono_rejects_every_noncanonical_header_before_payload_admission() {
        let binding = binding();
        let good = raw(&binding, &packet(&binding, 0));
        // Magic/version/flags/header size/identities/epoch/timeline/sample count/
        // declared total/offset/index/count/fragment length/reserved bits.
        for offset in [
            0, 4, 5, 6, 8, 24, 40, 56, 60, 76, 84, 88, 92, 96, 98, 100, 102,
        ] {
            let mut bytes = good;
            bytes[offset] ^= 1;
            let mut receiver = SecureMonoPcmAdmission::new(binding.clone()).unwrap();
            assert!(receiver.admit_record(&bytes).is_err(), "offset {offset}");
            assert!(receiver.terminal());
            assert!(receiver.admit_record(&good).is_err());
        }
        for length in [0, 104, 1063, 1065, 65535] {
            let mut receiver = SecureMonoPcmAdmission::new(binding.clone()).unwrap();
            assert!(receiver.admit_record(&vec![0; length]).is_err());
            assert!(receiver.terminal());
        }
    }

    #[test]
    fn secure_mono_rejects_replay_gap_discontinuity_and_changed_timestamp() {
        let binding = binding();
        for case in 0..5 {
            let mut receiver = SecureMonoPcmAdmission::new(binding.clone()).unwrap();
            receiver
                .admit_record(&raw(&binding, &packet(&binding, 0)))
                .unwrap();
            let mut next = packet(&binding, 1);
            match case {
                0 => next.sequence = 0,
                1 => next.sequence = 2,
                2 => next.first_sample_index += 1,
                3 => next.source_timestamp_micros += 1,
                _ => next.discontinuity = true,
            }
            assert!(receiver.admit_record(&raw(&binding, &next)).is_err());
            assert!(receiver.terminal());
        }
    }

    #[test]
    fn secure_mono_sender_is_terminal_on_the_same_invalid_input() {
        let binding = binding();
        for case in 0..5 {
            let mut sender = SecureMonoPcmAdmission::new(binding.clone()).unwrap();
            let mut bad = packet(&binding, 0);
            match case {
                0 => bad.discontinuity = true,
                1 => bad.sequence = 1,
                2 => bad.payload.push(0),
                3 => bad.sample_count = 481,
                _ => bad.source_timestamp_micros = u64::MAX - 9999,
            }
            assert!(sender.encode_record(&bad).is_err());
            assert!(sender.terminal());
            assert!(sender.encode_record(&packet(&binding, 0)).is_err());
        }
    }

    #[test]
    fn secure_mono_binding_and_all_timeline_overflows_fail_closed() {
        let binding = binding();
        for case in 0..4 {
            let mut bad = binding.clone();
            match case {
                0 => bad.selected_spec = AudioStreamSpec::media_balanced(),
                1 => bad.selected_spec.qos.target_latency_micros += 1,
                2 => bad.stream_epoch = 0,
                _ => bad.stream_epoch = u32::MAX,
            }
            assert!(SecureMonoPcmAdmission::new(bad).is_err());
        }
        for case in 0..3 {
            let mut receiver = SecureMonoPcmAdmission::new(binding.clone()).unwrap();
            let mut bytes = raw(&binding, &packet(&binding, 0));
            match case {
                0 => {
                    receiver.next_sequence = u64::MAX;
                    bytes[60..68].copy_from_slice(&u64::MAX.to_be_bytes());
                }
                1 => {
                    receiver.next_sample_index = u64::MAX;
                    bytes[76..84].copy_from_slice(&u64::MAX.to_be_bytes());
                }
                _ => bytes[68..76].copy_from_slice(&u64::MAX.to_be_bytes()),
            }
            assert!(receiver.admit_record(&bytes).is_err());
            assert!(receiver.terminal());
        }
        let mut receiver = SecureMonoPcmAdmission::new(binding.clone()).unwrap();
        let mut first = packet(&binding, 0);
        first.source_timestamp_micros = u64::MAX - 10000;
        assert!(receiver.admit_record(&raw(&binding, &first)).is_ok());
    }

    #[test]
    fn secure_mono_rate_bounds_burst_and_capped_accrual() {
        let mut rate = SecureMonoPcmRateBudget::new(Duration::from_secs(60)).unwrap();
        for _ in 0..4 {
            rate.admit(Duration::ZERO).unwrap();
        }
        assert!(rate.admit(Duration::ZERO).is_err());
        assert!(rate.terminal());
        assert!(rate.admit(Duration::from_millis(10)).is_err());
        let mut rate = SecureMonoPcmRateBudget::new(Duration::from_secs(60)).unwrap();
        for _ in 0..4 {
            rate.admit(Duration::from_secs(50)).unwrap();
        }
        assert!(rate.admit(Duration::from_secs(50)).is_err());
    }

    #[test]
    fn secure_mono_rate_uses_exact_10ms_cadence_and_total_ceiling() {
        let mut rate = SecureMonoPcmRateBudget::new(Duration::from_secs(60)).unwrap();
        for n in 0..6000 {
            rate.admit(Duration::from_millis(n * 10)).unwrap();
        }
        assert!(rate.admit(Duration::from_millis(59999)).is_err());
        let mut rate = SecureMonoPcmRateBudget::new(Duration::from_millis(11)).unwrap();
        rate.admit(Duration::ZERO).unwrap();
        rate.admit(Duration::ZERO).unwrap();
        assert!(rate.admit(Duration::ZERO).is_err());
    }

    #[test]
    fn secure_mono_rate_lifetime_regression_and_overflow_are_terminal() {
        assert!(SecureMonoPcmRateBudget::new(Duration::ZERO).is_err());
        assert!(SecureMonoPcmRateBudget::new(Duration::MAX).is_err());
        assert!(
            SecureMonoPcmRateBudget::new(Duration::from_secs(60) + Duration::from_nanos(1))
                .is_err()
        );
        for invalid in [Duration::from_secs(1), Duration::MAX] {
            let mut rate = SecureMonoPcmRateBudget::new(Duration::from_secs(1)).unwrap();
            assert!(rate.admit(invalid).is_err());
            assert!(rate.terminal());
        }
        let mut rate = SecureMonoPcmRateBudget::new(Duration::from_secs(1)).unwrap();
        rate.admit(Duration::from_millis(10)).unwrap();
        assert!(rate.admit(Duration::from_millis(9)).is_err());
        assert!(rate.terminal());
    }
}
