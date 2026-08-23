use std::collections::BTreeMap;

use capyio_core::StreamId;

use crate::{AudioDataError, AudioFrame};

/// Observable outcomes when a decoded frame enters the bounded reorder buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InsertOutcome {
    Accepted,
    Duplicate,
    Late,
    WrongStream,
    WrongEpoch,
    TooFarAhead,
    Full,
}

/// Counters owned by one stream epoch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameBufferStats {
    pub accepted: u64,
    pub emitted: u64,
    pub duplicates: u64,
    pub late: u64,
    pub wrong_stream: u64,
    pub wrong_epoch: u64,
    pub too_far_ahead: u64,
    pub missing: u64,
    pub full_drops: u64,
}

/// Worker-thread reordering queue with an explicit frame-count and sequence-window bound.
///
/// This is not a lock-free audio-callback ring. A platform Adapter should move ready PCM from this
/// queue into its own preallocated real-time buffer.
#[derive(Clone, Debug)]
pub struct ReorderBuffer {
    stream_id: StreamId,
    stream_epoch: u32,
    expected_sequence: u64,
    capacity_frames: usize,
    frames: BTreeMap<u64, AudioFrame>,
    stats: FrameBufferStats,
}

impl ReorderBuffer {
    pub fn new(
        stream_id: StreamId,
        stream_epoch: u32,
        expected_sequence: u64,
        capacity_frames: usize,
    ) -> Result<Self, AudioDataError> {
        if capacity_frames == 0 {
            return Err(AudioDataError::ZeroCapacity);
        }
        Ok(Self {
            stream_id,
            stream_epoch,
            expected_sequence,
            capacity_frames,
            frames: BTreeMap::new(),
            stats: FrameBufferStats::default(),
        })
    }

    /// Inserts a frame from the configured stream epoch without silently evicting older data.
    pub fn insert(&mut self, frame: AudioFrame) -> InsertOutcome {
        if frame.stream_id != self.stream_id {
            self.stats.wrong_stream = self.stats.wrong_stream.saturating_add(1);
            return InsertOutcome::WrongStream;
        }
        if frame.stream_epoch != self.stream_epoch {
            self.stats.wrong_epoch = self.stats.wrong_epoch.saturating_add(1);
            return InsertOutcome::WrongEpoch;
        }
        if frame.sequence < self.expected_sequence {
            self.stats.late = self.stats.late.saturating_add(1);
            return InsertOutcome::Late;
        }

        let max_accepted_sequence = self
            .expected_sequence
            .saturating_add(u64::try_from(self.capacity_frames).unwrap_or(u64::MAX))
            .saturating_sub(1);
        if frame.sequence > max_accepted_sequence {
            self.stats.too_far_ahead = self.stats.too_far_ahead.saturating_add(1);
            return InsertOutcome::TooFarAhead;
        }
        if self.frames.contains_key(&frame.sequence) {
            self.stats.duplicates = self.stats.duplicates.saturating_add(1);
            return InsertOutcome::Duplicate;
        }
        if self.frames.len() >= self.capacity_frames {
            self.stats.full_drops = self.stats.full_drops.saturating_add(1);
            return InsertOutcome::Full;
        }

        let previous = self.frames.insert(frame.sequence, frame);
        debug_assert!(
            previous.is_none(),
            "duplicates are rejected before insertion"
        );
        self.stats.accepted = self.stats.accepted.saturating_add(1);
        InsertOutcome::Accepted
    }

    /// Emits the exact next sequence when available.
    pub fn pop_next(&mut self) -> Option<AudioFrame> {
        let frame = self.frames.remove(&self.expected_sequence)?;
        self.expected_sequence = self.expected_sequence.saturating_add(1);
        self.stats.emitted = self.stats.emitted.saturating_add(1);
        Some(frame)
    }

    /// Declares a missing gap lost and emits the earliest buffered frame.
    ///
    /// A caller should use this only after its jitter deadline expires. Keeping that deadline out
    /// of this type avoids coupling the queue to a particular clock or async runtime.
    pub fn skip_gap_and_pop(&mut self) -> Option<AudioFrame> {
        if let Some(frame) = self.pop_next() {
            return Some(frame);
        }
        let next_sequence = *self.frames.first_key_value()?.0;
        let missing = next_sequence.saturating_sub(self.expected_sequence);
        self.stats.missing = self.stats.missing.saturating_add(missing);
        self.expected_sequence = next_sequence;
        self.pop_next()
    }

    #[must_use]
    pub const fn expected_sequence(&self) -> u64 {
        self.expected_sequence
    }

    #[must_use]
    pub fn buffered_frames(&self) -> usize {
        self.frames.len()
    }

    #[must_use]
    pub const fn stats(&self) -> FrameBufferStats {
        self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(stream_id: StreamId, epoch: u32, sequence: u64) -> AudioFrame {
        AudioFrame {
            stream_id,
            stream_epoch: epoch,
            sequence,
            source_timestamp_micros: sequence * 10_000,
            first_sample_index: sequence * 480,
            sample_count: 480,
            discontinuity: false,
            payload: vec![0; 960],
        }
    }

    #[test]
    fn reorders_and_emits_without_counting_loss() {
        let stream_id = StreamId::new();
        let mut buffer = ReorderBuffer::new(stream_id, 2, 10, 4).expect("buffer");
        assert_eq!(
            buffer.insert(frame(stream_id, 2, 11)),
            InsertOutcome::Accepted
        );
        assert_eq!(
            buffer.insert(frame(stream_id, 2, 10)),
            InsertOutcome::Accepted
        );
        assert_eq!(buffer.pop_next().expect("10").sequence, 10);
        assert_eq!(buffer.pop_next().expect("11").sequence, 11);
        assert_eq!(buffer.stats().missing, 0);
    }

    #[test]
    fn duplicate_late_wrong_epoch_and_gap_are_explicit() {
        let stream_id = StreamId::new();
        let mut buffer = ReorderBuffer::new(stream_id, 3, 20, 3).expect("buffer");
        assert_eq!(
            buffer.insert(frame(stream_id, 3, 22)),
            InsertOutcome::Accepted
        );
        assert_eq!(
            buffer.insert(frame(stream_id, 3, 22)),
            InsertOutcome::Duplicate
        );
        assert_eq!(
            buffer.insert(frame(stream_id, 2, 20)),
            InsertOutcome::WrongEpoch
        );
        assert_eq!(
            buffer.insert(frame(StreamId::new(), 3, 20)),
            InsertOutcome::WrongStream
        );
        assert_eq!(buffer.skip_gap_and_pop().expect("22").sequence, 22);
        assert_eq!(buffer.insert(frame(stream_id, 3, 21)), InsertOutcome::Late);

        let stats = buffer.stats();
        assert_eq!(stats.missing, 2);
        assert_eq!(stats.duplicates, 1);
        assert_eq!(stats.wrong_epoch, 1);
        assert_eq!(stats.wrong_stream, 1);
        assert_eq!(stats.late, 1);
    }

    #[test]
    fn buffer_never_accepts_beyond_capacity_window() {
        let stream_id = StreamId::new();
        let mut buffer = ReorderBuffer::new(stream_id, 1, 0, 2).expect("buffer");
        assert_eq!(
            buffer.insert(frame(stream_id, 1, 1)),
            InsertOutcome::Accepted
        );
        assert_eq!(
            buffer.insert(frame(stream_id, 1, 2)),
            InsertOutcome::TooFarAhead
        );
        assert_eq!(
            buffer.insert(frame(stream_id, 1, 0)),
            InsertOutcome::Accepted
        );
        assert_eq!(buffer.buffered_frames(), 2);
        assert_eq!(buffer.stats().too_far_ahead, 1);
    }
}
