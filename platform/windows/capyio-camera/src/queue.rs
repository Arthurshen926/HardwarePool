use std::collections::VecDeque;

use crate::{CameraFixtureError, GeneratedVideoFrame, fixture_stream_spec};

pub const MAX_FIXTURE_QUEUE_FRAMES: usize = 12;
pub const MAX_FIXTURE_QUEUE_BYTES: usize = 1_382_400 * MAX_FIXTURE_QUEUE_FRAMES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameQueueOverflowPolicy {
    RejectNewest,
    DropOldest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameQueuePushOutcome {
    Queued,
    DroppedOldest { sequence: u64 },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameQueueMetrics {
    pub accepted_frames: u64,
    pub overflow_events: u64,
    pub dropped_oldest_frames: u64,
    pub rejected_newest_frames: u64,
    pub high_watermark_frames: usize,
}

#[derive(Clone, Debug)]
pub struct BoundedFrameQueue {
    capacity: usize,
    overflow_policy: FrameQueueOverflowPolicy,
    frames: VecDeque<GeneratedVideoFrame>,
    metrics: FrameQueueMetrics,
}

impl BoundedFrameQueue {
    pub fn new(
        capacity: usize,
        overflow_policy: FrameQueueOverflowPolicy,
    ) -> Result<Self, CameraFixtureError> {
        fixture_stream_spec().validate()?;
        if capacity == 0 || capacity > MAX_FIXTURE_QUEUE_FRAMES {
            return Err(CameraFixtureError::InvalidQueueCapacity {
                actual: capacity,
                maximum: MAX_FIXTURE_QUEUE_FRAMES,
            });
        }
        Ok(Self {
            capacity,
            overflow_policy,
            frames: VecDeque::with_capacity(capacity),
            metrics: FrameQueueMetrics::default(),
        })
    }

    pub fn push(
        &mut self,
        mut frame: GeneratedVideoFrame,
    ) -> Result<FrameQueuePushOutcome, CameraFixtureError> {
        frame.validate(&fixture_stream_spec())?;
        let outcome = if self.frames.len() == self.capacity {
            self.metrics.overflow_events = self.metrics.overflow_events.saturating_add(1);
            match self.overflow_policy {
                FrameQueueOverflowPolicy::RejectNewest => {
                    self.metrics.rejected_newest_frames =
                        self.metrics.rejected_newest_frames.saturating_add(1);
                    return Err(CameraFixtureError::QueueFull {
                        rejected_sequence: frame.descriptor.sequence,
                    });
                }
                FrameQueueOverflowPolicy::DropOldest => {
                    let removed = self
                        .frames
                        .pop_front()
                        .expect("a queue at its positive capacity has a front frame");
                    self.metrics.dropped_oldest_frames =
                        self.metrics.dropped_oldest_frames.saturating_add(1);
                    if let Some(first_retained) = self.frames.front_mut() {
                        first_retained.descriptor.flags.discontinuity = true;
                    } else {
                        frame.descriptor.flags.discontinuity = true;
                    }
                    FrameQueuePushOutcome::DroppedOldest {
                        sequence: removed.descriptor.sequence,
                    }
                }
            }
        } else {
            FrameQueuePushOutcome::Queued
        };

        self.frames.push_back(frame);
        self.metrics.accepted_frames = self.metrics.accepted_frames.saturating_add(1);
        self.metrics.high_watermark_frames =
            self.metrics.high_watermark_frames.max(self.frames.len());
        Ok(outcome)
    }

    pub fn pop(&mut self) -> Option<GeneratedVideoFrame> {
        self.frames.pop_front()
    }

    #[must_use]
    pub fn front(&self) -> Option<&GeneratedVideoFrame> {
        self.frames.front()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    #[must_use]
    pub const fn metrics(&self) -> FrameQueueMetrics {
        self.metrics
    }
}
