use capyio_core::StreamId;
use serde::{Deserialize, Serialize};

use crate::{AudioDataError, AudioFormat};

/// Conservative bootstrap bound before a transport-specific MTU/fragmentation policy exists.
const MAX_BOOTSTRAP_PAYLOAD_BYTES: usize = 1024 * 1024;

/// One decoded, uncompressed audio block belonging to a negotiated stream epoch.
///
/// `source_timestamp_micros` is measured by a monotonic source clock. It is not wall-clock time.
/// The timestamp, sample index and sequence number remain independent so loss and clock drift can
/// be diagnosed without guessing from packet arrival time.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioFrame {
    pub stream_id: StreamId,
    pub stream_epoch: u32,
    pub sequence: u64,
    pub source_timestamp_micros: u64,
    pub first_sample_index: u64,
    pub sample_count: u32,
    pub discontinuity: bool,
    pub payload: Vec<u8>,
}

impl AudioFrame {
    /// Validates bounded PCM payload size against the already-negotiated audio format.
    pub fn validate(&self, format: &AudioFormat) -> Result<(), AudioDataError> {
        if self.sample_count == 0 {
            return Err(AudioDataError::EmptyFrame);
        }

        let expected = u64::from(self.sample_count)
            .checked_mul(u64::from(format.channels))
            .and_then(|value| value.checked_mul(u64::from(format.sample_format.bytes_per_sample())))
            .ok_or(AudioDataError::SizeOverflow)?;
        let expected = usize::try_from(expected).map_err(|_| AudioDataError::SizeOverflow)?;

        if expected > MAX_BOOTSTRAP_PAYLOAD_BYTES {
            return Err(AudioDataError::PayloadTooLarge {
                limit: MAX_BOOTSTRAP_PAYLOAD_BYTES,
            });
        }
        if self.payload.len() != expected {
            return Err(AudioDataError::PayloadLength {
                expected,
                actual: self.payload.len(),
            });
        }
        Ok(())
    }

    /// Returns the exclusive sample index immediately after this frame.
    #[must_use]
    pub fn end_sample_index(&self) -> u64 {
        self.first_sample_index
            .saturating_add(u64::from(self.sample_count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microphone_baseline_payload_is_validated() {
        let format = AudioFormat::microphone_baseline();
        let frame = AudioFrame {
            stream_id: StreamId::new(),
            stream_epoch: 1,
            sequence: 7,
            source_timestamp_micros: 10_000,
            first_sample_index: 480,
            sample_count: 480,
            discontinuity: false,
            payload: vec![0; 480 * 2],
        };

        frame.validate(&format).expect("valid PCM frame");
        assert_eq!(frame.end_sample_index(), 960);
    }

    #[test]
    fn payload_size_mismatch_is_rejected() {
        let format = AudioFormat::speaker_baseline();
        let frame = AudioFrame {
            stream_id: StreamId::new(),
            stream_epoch: 1,
            sequence: 0,
            source_timestamp_micros: 0,
            first_sample_index: 0,
            sample_count: 480,
            discontinuity: false,
            payload: vec![0; 480 * 2],
        };

        assert!(matches!(
            frame.validate(&format),
            Err(AudioDataError::PayloadLength { .. })
        ));
    }
}
