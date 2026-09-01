use serde::{Deserialize, Serialize};

use crate::{
    InputContractError, InputFrameHeader, InputSequenceOutcome, InputSequenceTracker,
    InputStreamDescriptor, NormalizedMagnitude, SequenceGap,
};

pub const MIN_TOUCHPAD_CONTACTS: u8 = 3;
pub const MAX_TOUCHPAD_CONTACTS: u8 = 5;
pub const MAX_TOUCHPAD_HIMETRIC_AXIS: u32 = 1_000_000;
pub const MAX_TOUCHPAD_FIXTURE_FRAMES: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TouchpadPhysicalSize {
    /// Width in himetric units (1/100 mm).
    pub width_himetric: u32,
    /// Height in himetric units (1/100 mm).
    pub height_himetric: u32,
}

impl TouchpadPhysicalSize {
    fn validate(self) -> Result<(), InputContractError> {
        if self.width_himetric == 0
            || self.height_himetric == 0
            || self.width_himetric > MAX_TOUCHPAD_HIMETRIC_AXIS
            || self.height_himetric > MAX_TOUCHPAD_HIMETRIC_AXIS
        {
            return Err(InputContractError::InvalidTouchpadDescriptor(format!(
                "physical dimensions must be inside 1..={MAX_TOUCHPAD_HIMETRIC_AXIS} himetric units"
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchpadButtonType {
    ClickPad,
    PressurePad,
    NonClickable,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchpadButtonState {
    #[default]
    Released,
    Pressed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TouchpadDescriptor {
    pub physical_size: TouchpadPhysicalSize,
    pub max_contacts: u8,
    pub button_type: TouchpadButtonType,
    pub reports_contact_size: bool,
    pub reports_pressure: bool,
}

impl TouchpadDescriptor {
    pub fn validate(&self) -> Result<(), InputContractError> {
        self.physical_size.validate()?;
        if !(MIN_TOUCHPAD_CONTACTS..=MAX_TOUCHPAD_CONTACTS).contains(&self.max_contacts) {
            return Err(InputContractError::InvalidTouchpadDescriptor(format!(
                "Precision Touchpad contact count must be inside {MIN_TOUCHPAD_CONTACTS}..={MAX_TOUCHPAD_CONTACTS}"
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TouchpadPosition {
    pub x_himetric: u32,
    pub y_himetric: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TouchpadContactSize {
    pub width_himetric: u32,
    pub height_himetric: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TouchpadContact {
    /// Stable for the lifetime of this active contact; reusable after release.
    pub contact_id: u32,
    pub position: TouchpadPosition,
    /// True when the source currently classifies this as an intentional contact.
    pub confidence: bool,
    pub size: Option<TouchpadContactSize>,
    pub pressure: Option<NormalizedMagnitude>,
}

impl TouchpadContact {
    fn validate(self, descriptor: &TouchpadDescriptor) -> Result<(), InputContractError> {
        if self.position.x_himetric > descriptor.physical_size.width_himetric
            || self.position.y_himetric > descriptor.physical_size.height_himetric
        {
            return Err(InputContractError::InvalidTouchpadFrame(
                "contact position is outside the declared physical surface".to_owned(),
            ));
        }
        match self.size {
            Some(_) if !descriptor.reports_contact_size => {
                return Err(InputContractError::InvalidTouchpadFrame(
                    "contact size was supplied but the descriptor does not advertise it".to_owned(),
                ));
            }
            Some(size)
                if size.width_himetric == 0
                    || size.height_himetric == 0
                    || size.width_himetric > descriptor.physical_size.width_himetric
                    || size.height_himetric > descriptor.physical_size.height_himetric =>
            {
                return Err(InputContractError::InvalidTouchpadFrame(
                    "contact size must be non-zero and fit inside the physical surface".to_owned(),
                ));
            }
            _ => {}
        }
        if self.pressure.is_some() && !descriptor.reports_pressure {
            return Err(InputContractError::InvalidTouchpadFrame(
                "contact pressure was supplied but the descriptor does not advertise it".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchpadFrameKind {
    /// Complete snapshot of contacts currently on the surface.
    Update,
    /// Release/cancel every contact and the integrated button.
    CancelAll,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TouchpadFrame {
    pub header: InputFrameHeader,
    pub kind: TouchpadFrameKind,
    pub button: TouchpadButtonState,
    pub contacts: Vec<TouchpadContact>,
}

impl TouchpadFrame {
    pub fn validate(&self, descriptor: &TouchpadDescriptor) -> Result<(), InputContractError> {
        descriptor.validate()?;
        self.header.validate()?;
        if self.contacts.len() > usize::from(descriptor.max_contacts) {
            return Err(InputContractError::InvalidTouchpadFrame(format!(
                "frame contains {} contacts; descriptor limit is {}",
                self.contacts.len(),
                descriptor.max_contacts
            )));
        }
        if self.kind == TouchpadFrameKind::CancelAll
            && (!self.contacts.is_empty() || self.button != TouchpadButtonState::Released)
        {
            return Err(InputContractError::InvalidTouchpadFrame(
                "cancel_all must contain no contacts and a released button".to_owned(),
            ));
        }
        if descriptor.button_type == TouchpadButtonType::NonClickable
            && self.button != TouchpadButtonState::Released
        {
            return Err(InputContractError::InvalidTouchpadFrame(
                "a non-clickable touchpad cannot report an integrated button press".to_owned(),
            ));
        }

        for (index, contact) in self.contacts.iter().enumerate() {
            if self.contacts[..index]
                .iter()
                .any(|previous| previous.contact_id == contact.contact_id)
            {
                return Err(InputContractError::InvalidTouchpadFrame(
                    "contact IDs must be unique inside a complete snapshot".to_owned(),
                ));
            }
            contact.validate(descriptor)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn is_released(&self) -> bool {
        self.contacts.is_empty() && self.button == TouchpadButtonState::Released
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TouchpadFrameOutcome {
    Applied,
    Cancelled,
    GapCancelled(SequenceGap),
    GapRequiresCancelAll(SequenceGap),
    SuppressedUntilCancelAll,
}

/// Allocation-free fail-safe guard for one touchpad stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TouchpadFrameTracker {
    sequence: InputSequenceTracker,
    requires_cancel_all: bool,
    last_timestamp_nanos: Option<u64>,
}

impl TouchpadFrameTracker {
    pub fn new(
        stream: &InputStreamDescriptor,
        first_sequence: u64,
    ) -> Result<Self, InputContractError> {
        stream.validate()?;
        Ok(Self {
            sequence: InputSequenceTracker::new(
                stream.stream_id,
                stream.stream_epoch,
                first_sequence,
            )?,
            requires_cancel_all: true,
            last_timestamp_nanos: None,
        })
    }

    pub fn observe(
        &mut self,
        frame: &TouchpadFrame,
        descriptor: &TouchpadDescriptor,
    ) -> Result<TouchpadFrameOutcome, InputContractError> {
        frame.validate(descriptor)?;
        let mut candidate = *self;
        let outcome = candidate.observe_validated(frame)?;
        *self = candidate;
        Ok(outcome)
    }

    pub fn advance_epoch(
        &mut self,
        new_epoch: u64,
        first_sequence: u64,
    ) -> Result<(), InputContractError> {
        let mut candidate = *self;
        candidate
            .sequence
            .advance_epoch(new_epoch, first_sequence)?;
        candidate.requires_cancel_all = true;
        candidate.last_timestamp_nanos = None;
        *self = candidate;
        Ok(())
    }

    fn observe_validated(
        &mut self,
        frame: &TouchpadFrame,
    ) -> Result<TouchpadFrameOutcome, InputContractError> {
        if let Some(previous) = self.last_timestamp_nanos
            && frame.header.source_timestamp_nanos < previous
        {
            return Err(InputContractError::TouchpadTimestampRegression {
                previous,
                actual: frame.header.source_timestamp_nanos,
            });
        }
        let sequence = self.sequence.observe(frame.header)?;
        self.last_timestamp_nanos = Some(frame.header.source_timestamp_nanos);

        if let InputSequenceOutcome::Gap(gap) = sequence {
            if frame.kind == TouchpadFrameKind::CancelAll {
                self.requires_cancel_all = false;
                return Ok(TouchpadFrameOutcome::GapCancelled(gap));
            }
            self.requires_cancel_all = true;
            return Ok(TouchpadFrameOutcome::GapRequiresCancelAll(gap));
        }

        if frame.kind == TouchpadFrameKind::CancelAll {
            self.requires_cancel_all = false;
            return Ok(TouchpadFrameOutcome::Cancelled);
        }
        if self.requires_cancel_all {
            return Ok(TouchpadFrameOutcome::SuppressedUntilCancelAll);
        }
        Ok(TouchpadFrameOutcome::Applied)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TouchpadMetrics {
    pub frames_observed: u64,
    pub contact_samples_observed: u64,
    pub peak_contacts: u8,
    pub sequence_gaps: u64,
    pub cancel_all_frames: u64,
    pub suppressed_frames: u64,
}

/// Bounded JSON fixture/diagnostic shape; not a production wire contract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TouchpadFixture {
    pub stream: InputStreamDescriptor,
    pub descriptor: TouchpadDescriptor,
    pub frames: Vec<TouchpadFrame>,
}

impl TouchpadFixture {
    pub fn validate(&self) -> Result<TouchpadMetrics, InputContractError> {
        self.stream.validate()?;
        self.descriptor.validate()?;
        if self.frames.is_empty() || self.frames.len() > MAX_TOUCHPAD_FIXTURE_FRAMES {
            return Err(InputContractError::InvalidTouchpadFixture(format!(
                "fixture must contain 1..={MAX_TOUCHPAD_FIXTURE_FRAMES} frames"
            )));
        }

        let mut tracker = TouchpadFrameTracker::new(&self.stream, self.frames[0].header.sequence)?;
        let mut metrics = TouchpadMetrics::default();
        for frame in &self.frames {
            let outcome = tracker.observe(frame, &self.descriptor)?;
            metrics.frames_observed += 1;
            metrics.contact_samples_observed += frame.contacts.len() as u64;
            metrics.peak_contacts = metrics.peak_contacts.max(frame.contacts.len() as u8);
            if matches!(
                outcome,
                TouchpadFrameOutcome::GapCancelled(_)
                    | TouchpadFrameOutcome::GapRequiresCancelAll(_)
            ) {
                metrics.sequence_gaps += 1;
            }
            if frame.kind == TouchpadFrameKind::CancelAll {
                metrics.cancel_all_frames += 1;
            }
            if matches!(
                outcome,
                TouchpadFrameOutcome::GapRequiresCancelAll(_)
                    | TouchpadFrameOutcome::SuppressedUntilCancelAll
            ) {
                metrics.suppressed_frames += 1;
            }
        }
        if !self.frames.last().is_some_and(TouchpadFrame::is_released) {
            return Err(InputContractError::InvalidTouchpadFixture(
                "fixture must end with every contact and button released".to_owned(),
            ));
        }
        Ok(metrics)
    }
}
