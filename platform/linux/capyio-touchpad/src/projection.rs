use capyio_input::{
    InputContractError, InputStreamDescriptor, TouchpadButtonType, TouchpadDescriptor,
    TouchpadFrame, TouchpadFrameKind, TouchpadFrameOutcome, TouchpadFrameTracker,
    TouchpadPhysicalSize,
};
use std::{error::Error, fmt};

pub const MAX_EVENTS_PER_FRAME: usize = 64;
pub const TOUCHPAD_SLOTS: usize = 5;
const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const EV_ABS: u16 = 3;
const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_TRACKING_ID: u16 = 0x39;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_PRESSURE: u16 = 0x3a;
const BTN_TOUCH: u16 = 0x14a;
const TOOL_KEYS: [u16; 5] = [0x145, 0x14d, 0x14e, 0x14f, 0x148];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InputEvent {
    pub event_type: u16,
    pub code: u16,
    pub value: i32,
}

/// Fixed allocation-free upper bound, including the terminating SYN_REPORT.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventBatch {
    events: [InputEvent; MAX_EVENTS_PER_FRAME],
    len: usize,
}
impl Default for EventBatch {
    fn default() -> Self {
        Self {
            events: [InputEvent::default(); MAX_EVENTS_PER_FRAME],
            len: 0,
        }
    }
}
impl EventBatch {
    pub fn as_slice(&self) -> &[InputEvent] {
        &self.events[..self.len]
    }
    fn push(&mut self, event_type: u16, code: u16, value: i32) {
        // At most 5 releases + 5 introductions + 5 positions/pressure + 6 keys
        // + 3 compatibility axes + SYN = 50. No peer-controlled growth.
        assert!(self.len < MAX_EVENTS_PER_FRAME);
        self.events[self.len] = InputEvent {
            event_type,
            code,
            value,
        };
        self.len += 1;
    }
    fn keys(&mut self, count: usize) {
        // BTN_TOUCH must be first in a synchronization frame.
        self.push(EV_KEY, BTN_TOUCH, i32::from(count != 0));
        for (index, code) in TOOL_KEYS.iter().enumerate() {
            self.push(EV_KEY, *code, i32::from(count == index + 1));
        }
    }
    fn sync(&mut self) {
        self.push(EV_SYN, 0, 0);
    }
}

pub fn linux_touchpad_descriptor() -> TouchpadDescriptor {
    TouchpadDescriptor {
        physical_size: TouchpadPhysicalSize {
            width_himetric: 12_000,
            height_himetric: 7_000,
        },
        max_contacts: 5,
        button_type: TouchpadButtonType::NonClickable,
        reports_contact_size: false,
        reports_pressure: true,
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectionError {
    Contract(InputContractError),
    UnsupportedDescriptor,
    TrackingIdsExhausted,
}
impl fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Contract(e) => e.fmt(f),
            Self::UnsupportedDescriptor => f.write_str("Linux touchpad requires the fixed five-contact 120 x 70 mm NonClickable pressure descriptor"),
            Self::TrackingIdsExhausted => f.write_str("Linux local tracking ID space exhausted; recreate the admitted device"),
        }
    }
}
impl Error for ProjectionError {}
impl From<InputContractError> for ProjectionError {
    fn from(error: InputContractError) -> Self {
        Self::Contract(error)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Slot {
    wire_id: u32,
    tracking_id: i32,
}

/// Transactional, bounded full-snapshot projection. IDs on the wire are never
/// cast to Linux signed IDs. Each new lifetime receives a monotonically fresh
/// local nonnegative ID; exhaustion terminates instead of wrapping.
#[derive(Clone, Debug)]
pub struct TouchpadProjector {
    tracker: TouchpadFrameTracker,
    slots: [Option<Slot>; TOUCHPAD_SLOTS],
    next_tracking_id: Option<i32>,
    confidence_suppressed: bool,
}
impl TouchpadProjector {
    pub fn new(
        stream: &InputStreamDescriptor,
        descriptor: TouchpadDescriptor,
        first_sequence: u64,
    ) -> Result<Self, ProjectionError> {
        if descriptor != linux_touchpad_descriptor() {
            return Err(ProjectionError::UnsupportedDescriptor);
        }
        Ok(Self {
            tracker: TouchpadFrameTracker::new(stream, first_sequence)?,
            slots: [None; TOUCHPAD_SLOTS],
            next_tracking_id: Some(0),
            confidence_suppressed: false,
        })
    }
    pub fn project(&mut self, frame: &TouchpadFrame) -> Result<EventBatch, ProjectionError> {
        let mut candidate = self.clone();
        let batch = candidate.project_inner(frame)?;
        *self = candidate;
        Ok(batch)
    }
    fn project_inner(&mut self, frame: &TouchpadFrame) -> Result<EventBatch, ProjectionError> {
        let outcome = self.tracker.observe(frame, &linux_touchpad_descriptor())?;
        if frame.kind == TouchpadFrameKind::CancelAll {
            self.confidence_suppressed = false;
        }
        if frame.contacts.iter().any(|c| !c.confidence) {
            // Conservative palm/uncertainty handling: never resurrect a contact
            // merely because confidence flips back while fingers remain down.
            self.confidence_suppressed = true;
        }
        if outcome != TouchpadFrameOutcome::Applied || self.confidence_suppressed {
            return Ok(self.release_all());
        }
        let mut batch = EventBatch::default();
        batch.keys(frame.contacts.len());
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_some_and(|s| !frame.contacts.iter().any(|c| c.contact_id == s.wire_id)) {
                batch.push(EV_ABS, ABS_MT_SLOT, index as i32);
                batch.push(EV_ABS, ABS_MT_TRACKING_ID, -1);
                *slot = None;
            }
        }
        for contact in &frame.contacts {
            let existing = self
                .slots
                .iter()
                .position(|s| s.is_some_and(|s| s.wire_id == contact.contact_id));
            let index = existing.unwrap_or_else(|| {
                self.slots
                    .iter()
                    .position(Option::is_none)
                    .expect("validated five-contact snapshot")
            });
            batch.push(EV_ABS, ABS_MT_SLOT, index as i32);
            if existing.is_none() {
                let tracking_id = self
                    .next_tracking_id
                    .ok_or(ProjectionError::TrackingIdsExhausted)?;
                self.next_tracking_id = tracking_id.checked_add(1);
                self.slots[index] = Some(Slot {
                    wire_id: contact.contact_id,
                    tracking_id,
                });
                batch.push(EV_ABS, ABS_MT_TRACKING_ID, tracking_id);
            }
            // Coordinates validated against 12000/7000 before these conversions.
            batch.push(
                EV_ABS,
                ABS_MT_POSITION_X,
                i32::try_from(contact.position.x_himetric).expect("validated coordinate"),
            );
            batch.push(
                EV_ABS,
                ABS_MT_POSITION_Y,
                i32::try_from(contact.position.y_himetric).expect("validated coordinate"),
            );
            // Pressure is normalized, not a force measurement: retain 0..65535
            // exactly. Missing optional measurement is 0, never invented force.
            batch.push(
                EV_ABS,
                ABS_MT_PRESSURE,
                contact.pressure.map_or(0, |p| i32::from(p.get())),
            );
        }
        // Compatibility axes follow the lowest occupied slot, not peer list order.
        let primary = self
            .slots
            .iter()
            .flatten()
            .next()
            .and_then(|s| frame.contacts.iter().find(|c| c.contact_id == s.wire_id));
        if let Some(contact) = primary {
            batch.push(
                EV_ABS,
                0,
                i32::try_from(contact.position.x_himetric).expect("validated coordinate"),
            );
            batch.push(
                EV_ABS,
                1,
                i32::try_from(contact.position.y_himetric).expect("validated coordinate"),
            );
            batch.push(
                EV_ABS,
                0x18,
                contact.pressure.map_or(0, |p| i32::from(p.get())),
            );
        } else {
            batch.push(EV_ABS, 0x18, 0);
        }
        batch.sync();
        Ok(batch)
    }
    pub fn release_all(&mut self) -> EventBatch {
        let mut batch = EventBatch::default();
        batch.keys(0);
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.take().is_some() {
                batch.push(EV_ABS, ABS_MT_SLOT, index as i32);
                batch.push(EV_ABS, ABS_MT_TRACKING_ID, -1);
            }
        }
        batch.push(EV_ABS, 0x18, 0);
        batch.sync();
        batch
    }
    pub fn advance_epoch(
        &mut self,
        new_epoch: u64,
        first_sequence: u64,
    ) -> Result<EventBatch, ProjectionError> {
        self.tracker.advance_epoch(new_epoch, first_sequence)?;
        self.confidence_suppressed = false;
        Ok(self.release_all())
    }
}
