use capyio_core::StreamId;

use crate::{DisplayContractError, DisplayStreamSpec, OwnedDisplayFrame};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplaySequenceGap {
    pub first_missing: u64,
    pub last_missing: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LatestFramePushOutcome {
    Stored {
        gap: Option<DisplaySequenceGap>,
        replaced_pending: bool,
    },
    Duplicate {
        sequence: u64,
    },
    Late {
        expected: u64,
        actual: u64,
    },
    WrongStream,
    StaleEpoch {
        current: u64,
        actual: u64,
    },
    FutureEpoch {
        current: u64,
        actual: u64,
    },
}

/// A deterministic capacity-one handoff for interactive display frames.
///
/// Publishing never waits for a consumer. A newer accepted frame replaces the
/// pending frame and reports the replacement, preventing latency from growing
/// through an unbounded queue.
#[derive(Debug)]
pub struct LatestDisplayFrameSlot {
    selected: DisplayStreamSpec,
    stream_id: StreamId,
    stream_epoch: u64,
    last_sequence: Option<u64>,
    pending: Option<OwnedDisplayFrame>,
    replacements: u64,
}

impl LatestDisplayFrameSlot {
    pub fn new(
        selected: DisplayStreamSpec,
        stream_id: StreamId,
        stream_epoch: u64,
    ) -> Result<Self, DisplayContractError> {
        selected.validate()?;
        if stream_epoch == 0 {
            return Err(DisplayContractError::InvalidFrame(
                "slot epoch must be positive".to_owned(),
            ));
        }
        Ok(Self {
            selected,
            stream_id,
            stream_epoch,
            last_sequence: None,
            pending: None,
            replacements: 0,
        })
    }

    pub fn push(
        &mut self,
        frame: OwnedDisplayFrame,
    ) -> Result<LatestFramePushOutcome, DisplayContractError> {
        frame.validate(&self.selected)?;
        let descriptor = &frame.descriptor;
        if descriptor.stream_id != self.stream_id {
            return Ok(LatestFramePushOutcome::WrongStream);
        }
        if descriptor.stream_epoch < self.stream_epoch {
            return Ok(LatestFramePushOutcome::StaleEpoch {
                current: self.stream_epoch,
                actual: descriptor.stream_epoch,
            });
        }
        if descriptor.stream_epoch > self.stream_epoch {
            return Ok(LatestFramePushOutcome::FutureEpoch {
                current: self.stream_epoch,
                actual: descriptor.stream_epoch,
            });
        }
        if let Some(last) = self.last_sequence {
            if descriptor.sequence == last {
                return Ok(LatestFramePushOutcome::Duplicate {
                    sequence: descriptor.sequence,
                });
            }
            if descriptor.sequence < last {
                return Ok(LatestFramePushOutcome::Late {
                    expected: last.saturating_add(1),
                    actual: descriptor.sequence,
                });
            }
        }
        let gap = self.last_sequence.and_then(|last| {
            let expected = last.checked_add(1)?;
            (descriptor.sequence > expected).then_some(DisplaySequenceGap {
                first_missing: expected,
                last_missing: descriptor.sequence - 1,
            })
        });
        let replaced_pending = self.pending.replace(frame).is_some();
        if replaced_pending {
            self.replacements = self.replacements.saturating_add(1);
        }
        self.last_sequence = self
            .pending
            .as_ref()
            .map(|pending| pending.descriptor.sequence);
        Ok(LatestFramePushOutcome::Stored {
            gap,
            replaced_pending,
        })
    }

    pub fn advance_epoch(&mut self, new_epoch: u64) -> Result<bool, DisplayContractError> {
        if new_epoch <= self.stream_epoch {
            return Err(DisplayContractError::NonAdvancingEpoch {
                current_epoch: self.stream_epoch,
                new_epoch,
            });
        }
        let discarded = self.pending.take().is_some();
        self.stream_epoch = new_epoch;
        self.last_sequence = None;
        Ok(discarded)
    }

    /// Replaces the exact selected mode while advancing to a new stream epoch.
    ///
    /// This is used after an explicitly negotiated display-size change. It is
    /// not an implicit resize or conversion and discards any pending frame from
    /// the older mode before accepting the new epoch.
    pub fn reconfigure(
        &mut self,
        selected: DisplayStreamSpec,
        new_epoch: u64,
    ) -> Result<bool, DisplayContractError> {
        selected.validate()?;
        if new_epoch <= self.stream_epoch {
            return Err(DisplayContractError::NonAdvancingEpoch {
                current_epoch: self.stream_epoch,
                new_epoch,
            });
        }
        let discarded = self.pending.take().is_some();
        self.selected = selected;
        self.stream_epoch = new_epoch;
        self.last_sequence = None;
        Ok(discarded)
    }

    pub fn take(&mut self) -> Option<OwnedDisplayFrame> {
        self.pending.take()
    }

    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.stream_epoch
    }

    #[must_use]
    pub const fn replacements(&self) -> u64 {
        self.replacements
    }

    #[must_use]
    pub const fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
}
