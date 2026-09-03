use std::fmt;

use capyio_input::{
    InputContractError, InputStreamDescriptor, MAX_TOUCHPAD_CONTACTS, SequenceGap,
    TouchpadButtonType, TouchpadContact, TouchpadDescriptor, TouchpadFrame, TouchpadFrameOutcome,
    TouchpadFrameTracker,
};

use crate::SyntheticTouchpadParameters;

pub const MAX_WINDOWS_TOUCHPAD_BATCHES: u8 = 2;
const BATCH_CAPACITY: usize = MAX_TOUCHPAD_CONTACTS as usize;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WindowsTouchpadContactPhase {
    #[default]
    Updated,
    Pressed,
    Released,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowsTouchpadContact {
    pub contact_id: u32,
    pub x_himetric: u32,
    pub y_himetric: u32,
    pub confidence: bool,
    pub phase: WindowsTouchpadContactPhase,
    /// Retained for a future documented native conversion; not encoded yet.
    pub contact_width_himetric: Option<u32>,
    /// Retained for a future documented native conversion; not encoded yet.
    pub contact_height_himetric: Option<u32>,
    /// Retained for a future documented native conversion; not encoded yet.
    pub pressure: Option<u16>,
}

impl WindowsTouchpadContact {
    fn from_contact(contact: TouchpadContact, phase: WindowsTouchpadContactPhase) -> Self {
        Self {
            contact_id: contact.contact_id,
            x_himetric: contact.position.x_himetric,
            y_himetric: contact.position.y_himetric,
            confidence: contact.confidence,
            phase,
            contact_width_himetric: contact.size.map(|size| size.width_himetric),
            contact_height_himetric: contact.size.map(|size| size.height_himetric),
            pressure: contact.pressure.map(|pressure| pressure.get()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowsTouchpadBatch {
    contacts: [WindowsTouchpadContact; BATCH_CAPACITY],
    count: u8,
}

impl WindowsTouchpadBatch {
    #[must_use]
    pub fn contacts(&self) -> &[WindowsTouchpadContact] {
        &self.contacts[..usize::from(self.count)]
    }

    #[must_use]
    pub const fn len(&self) -> u8 {
        self.count
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn push(&mut self, contact: WindowsTouchpadContact) {
        debug_assert!(usize::from(self.count) < BATCH_CAPACITY);
        self.contacts[usize::from(self.count)] = contact;
        self.count += 1;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsTouchpadProjectionDisposition {
    Applied,
    Cancelled,
    GapCancelled(SequenceGap),
    GapRequiresCancelAll(SequenceGap),
    SuppressedUntilCancelAll,
    EpochCancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowsTouchpadProjection {
    batches: [WindowsTouchpadBatch; MAX_WINDOWS_TOUCHPAD_BATCHES as usize],
    batch_count: u8,
    pub disposition: WindowsTouchpadProjectionDisposition,
}

impl WindowsTouchpadProjection {
    fn empty(disposition: WindowsTouchpadProjectionDisposition) -> Self {
        Self {
            batches: [WindowsTouchpadBatch::default(); MAX_WINDOWS_TOUCHPAD_BATCHES as usize],
            batch_count: 0,
            disposition,
        }
    }

    #[must_use]
    pub fn batches(&self) -> &[WindowsTouchpadBatch] {
        &self.batches[..usize::from(self.batch_count)]
    }

    #[must_use]
    pub const fn batch_count(&self) -> u8 {
        self.batch_count
    }

    fn push_batch(&mut self, batch: WindowsTouchpadBatch) {
        if batch.is_empty() {
            return;
        }
        debug_assert!(self.batch_count < MAX_WINDOWS_TOUCHPAD_BATCHES);
        self.batches[usize::from(self.batch_count)] = batch;
        self.batch_count += 1;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WindowsTouchpadProjectionError {
    UnsupportedButtonType(TouchpadButtonType),
    Contract(InputContractError),
}

impl fmt::Display for WindowsTouchpadProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedButtonType(button_type) => write!(
                formatter,
                "Windows frame projection does not yet encode {button_type:?} integrated-button state"
            ),
            Self::Contract(error) => {
                write!(formatter, "touchpad contract rejected projection: {error}")
            }
        }
    }
}

impl std::error::Error for WindowsTouchpadProjectionError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowsTouchpadProjector {
    descriptor: TouchpadDescriptor,
    tracker: TouchpadFrameTracker,
    active: [Option<TouchpadContact>; BATCH_CAPACITY],
    active_count: u8,
}

impl WindowsTouchpadProjector {
    pub fn new(
        stream: &InputStreamDescriptor,
        descriptor: TouchpadDescriptor,
        first_sequence: u64,
    ) -> Result<Self, WindowsTouchpadProjectionError> {
        descriptor
            .validate()
            .map_err(WindowsTouchpadProjectionError::Contract)?;
        if descriptor.button_type != TouchpadButtonType::NonClickable {
            return Err(WindowsTouchpadProjectionError::UnsupportedButtonType(
                descriptor.button_type,
            ));
        }
        let tracker = TouchpadFrameTracker::new(stream, first_sequence)
            .map_err(WindowsTouchpadProjectionError::Contract)?;
        Ok(Self {
            descriptor,
            tracker,
            active: [None; BATCH_CAPACITY],
            active_count: 0,
        })
    }

    pub fn project(
        &mut self,
        frame: &TouchpadFrame,
    ) -> Result<WindowsTouchpadProjection, WindowsTouchpadProjectionError> {
        let mut candidate = *self;
        let projection = candidate.project_candidate(frame)?;
        *self = candidate;
        Ok(projection)
    }

    #[must_use]
    pub fn device_parameters(&self) -> SyntheticTouchpadParameters {
        SyntheticTouchpadParameters {
            max_contacts: u32::from(self.descriptor.max_contacts),
            width_himetric: self.descriptor.physical_size.width_himetric,
            height_himetric: self.descriptor.physical_size.height_himetric,
        }
    }

    pub fn advance_epoch(
        &mut self,
        new_epoch: u64,
        first_sequence: u64,
    ) -> Result<WindowsTouchpadProjection, WindowsTouchpadProjectionError> {
        let mut candidate = *self;
        candidate
            .tracker
            .advance_epoch(new_epoch, first_sequence)
            .map_err(WindowsTouchpadProjectionError::Contract)?;
        let projection = candidate.clear_active(
            WindowsTouchpadContactPhase::Cancelled,
            WindowsTouchpadProjectionDisposition::EpochCancelled,
        );
        *self = candidate;
        Ok(projection)
    }

    /// Cancel every native contact retained by this projector.
    ///
    /// This is the bounded Route-stop/session-failure cleanup path. It does
    /// not advance the source contract tracker because the caller must close
    /// or re-establish the session before accepting additional frames.
    #[must_use]
    pub fn cancel_active(&mut self) -> WindowsTouchpadProjection {
        self.clear_active(
            WindowsTouchpadContactPhase::Cancelled,
            WindowsTouchpadProjectionDisposition::Cancelled,
        )
    }

    fn project_candidate(
        &mut self,
        frame: &TouchpadFrame,
    ) -> Result<WindowsTouchpadProjection, WindowsTouchpadProjectionError> {
        let outcome = self
            .tracker
            .observe(frame, &self.descriptor)
            .map_err(WindowsTouchpadProjectionError::Contract)?;
        Ok(match outcome {
            TouchpadFrameOutcome::Applied => self.apply_update(frame),
            TouchpadFrameOutcome::Cancelled => self.clear_active(
                WindowsTouchpadContactPhase::Cancelled,
                WindowsTouchpadProjectionDisposition::Cancelled,
            ),
            TouchpadFrameOutcome::GapCancelled(gap) => self.clear_active(
                WindowsTouchpadContactPhase::Cancelled,
                WindowsTouchpadProjectionDisposition::GapCancelled(gap),
            ),
            TouchpadFrameOutcome::GapRequiresCancelAll(gap) => self.clear_active(
                WindowsTouchpadContactPhase::Cancelled,
                WindowsTouchpadProjectionDisposition::GapRequiresCancelAll(gap),
            ),
            TouchpadFrameOutcome::SuppressedUntilCancelAll => WindowsTouchpadProjection::empty(
                WindowsTouchpadProjectionDisposition::SuppressedUntilCancelAll,
            ),
        })
    }

    fn apply_update(&mut self, frame: &TouchpadFrame) -> WindowsTouchpadProjection {
        let (current, current_count) = sorted_contacts(&frame.contacts);
        let mut releases = [None; BATCH_CAPACITY];
        let mut release_count = 0_u8;
        for previous in self.active[..usize::from(self.active_count)]
            .iter()
            .flatten()
            .copied()
        {
            let still_active = current[..usize::from(current_count)]
                .iter()
                .flatten()
                .any(|contact| contact.contact_id == previous.contact_id);
            if !still_active {
                releases[usize::from(release_count)] = Some(previous);
                release_count += 1;
            }
        }

        let mut projection =
            WindowsTouchpadProjection::empty(WindowsTouchpadProjectionDisposition::Applied);
        if usize::from(current_count + release_count) <= BATCH_CAPACITY {
            let mut batch = WindowsTouchpadBatch::default();
            push_current_contacts(
                &mut batch,
                &current,
                current_count,
                &self.active,
                self.active_count,
            );
            push_contacts(
                &mut batch,
                &releases,
                release_count,
                WindowsTouchpadContactPhase::Released,
            );
            projection.push_batch(batch);
        } else {
            let mut release_batch = WindowsTouchpadBatch::default();
            push_contacts(
                &mut release_batch,
                &releases,
                release_count,
                WindowsTouchpadContactPhase::Released,
            );
            projection.push_batch(release_batch);

            let mut active_batch = WindowsTouchpadBatch::default();
            push_current_contacts(
                &mut active_batch,
                &current,
                current_count,
                &self.active,
                self.active_count,
            );
            projection.push_batch(active_batch);
        }

        self.active = current;
        self.active_count = current_count;
        projection
    }

    fn clear_active(
        &mut self,
        phase: WindowsTouchpadContactPhase,
        disposition: WindowsTouchpadProjectionDisposition,
    ) -> WindowsTouchpadProjection {
        let mut projection = WindowsTouchpadProjection::empty(disposition);
        let mut batch = WindowsTouchpadBatch::default();
        push_contacts(&mut batch, &self.active, self.active_count, phase);
        projection.push_batch(batch);
        self.active = [None; BATCH_CAPACITY];
        self.active_count = 0;
        projection
    }
}

fn sorted_contacts(
    contacts: &[TouchpadContact],
) -> ([Option<TouchpadContact>; BATCH_CAPACITY], u8) {
    let mut sorted: [Option<TouchpadContact>; BATCH_CAPACITY] = [None; BATCH_CAPACITY];
    let mut count = 0_usize;
    for contact in contacts.iter().copied() {
        let mut insert_at = count;
        while insert_at > 0
            && sorted[insert_at - 1]
                .is_some_and(|previous| previous.contact_id > contact.contact_id)
        {
            sorted[insert_at] = sorted[insert_at - 1];
            insert_at -= 1;
        }
        sorted[insert_at] = Some(contact);
        count += 1;
    }
    (sorted, count as u8)
}

fn push_contacts(
    batch: &mut WindowsTouchpadBatch,
    contacts: &[Option<TouchpadContact>; BATCH_CAPACITY],
    count: u8,
    phase: WindowsTouchpadContactPhase,
) {
    for contact in contacts[..usize::from(count)].iter().flatten().copied() {
        batch.push(WindowsTouchpadContact::from_contact(contact, phase));
    }
}

fn push_current_contacts(
    batch: &mut WindowsTouchpadBatch,
    current: &[Option<TouchpadContact>; BATCH_CAPACITY],
    current_count: u8,
    previous: &[Option<TouchpadContact>; BATCH_CAPACITY],
    previous_count: u8,
) {
    for contact in current[..usize::from(current_count)]
        .iter()
        .flatten()
        .copied()
    {
        let phase = if previous[..usize::from(previous_count)]
            .iter()
            .flatten()
            .any(|item| item.contact_id == contact.contact_id)
        {
            WindowsTouchpadContactPhase::Updated
        } else {
            WindowsTouchpadContactPhase::Pressed
        };
        batch.push(WindowsTouchpadContact::from_contact(contact, phase));
    }
}

#[cfg(windows)]
mod native {
    use windows_sys::Win32::{
        Foundation::POINT,
        UI::{
            Controls::POINTER_TYPE_INFO,
            Input::Pointer::{
                POINTER_FLAG_CANCELED, POINTER_FLAG_CONFIDENCE, POINTER_FLAG_DOWN,
                POINTER_FLAG_INCONTACT, POINTER_FLAG_INRANGE, POINTER_FLAG_UP, POINTER_FLAG_UPDATE,
                POINTER_TOUCH_INFO,
            },
            WindowsAndMessaging::PT_TOUCHPAD,
        },
    };

    use super::{BATCH_CAPACITY, WindowsTouchpadBatch, WindowsTouchpadContactPhase};

    #[derive(Clone, Copy)]
    pub struct NativeTouchpadBatch {
        contacts: [POINTER_TYPE_INFO; BATCH_CAPACITY],
        count: u8,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct NativeTouchpadContactView {
        pub pointer_type: i32,
        pub pointer_id: u32,
        pub pointer_flags: u32,
        pub x_himetric: i32,
        pub y_himetric: i32,
        pub x_himetric_raw: i32,
        pub y_himetric_raw: i32,
        pub dw_time: u32,
        pub performance_count: u64,
        pub touch_mask: u32,
    }

    impl NativeTouchpadBatch {
        #[must_use]
        pub fn encode(batch: &WindowsTouchpadBatch) -> Self {
            let mut native = Self {
                contacts: [POINTER_TYPE_INFO::default(); BATCH_CAPACITY],
                count: batch.len(),
            };
            for (target, source) in native.contacts.iter_mut().zip(batch.contacts()) {
                let confidence = if source.confidence {
                    POINTER_FLAG_CONFIDENCE
                } else {
                    0
                };
                let pointer_flags = match source.phase {
                    WindowsTouchpadContactPhase::Pressed => {
                        POINTER_FLAG_INRANGE
                            | POINTER_FLAG_INCONTACT
                            | POINTER_FLAG_DOWN
                            | confidence
                    }
                    WindowsTouchpadContactPhase::Updated => {
                        POINTER_FLAG_INRANGE
                            | POINTER_FLAG_INCONTACT
                            | POINTER_FLAG_UPDATE
                            | confidence
                    }
                    WindowsTouchpadContactPhase::Released => POINTER_FLAG_UP | confidence,
                    WindowsTouchpadContactPhase::Cancelled => {
                        POINTER_FLAG_CANCELED | POINTER_FLAG_UP | confidence
                    }
                };
                let point = POINT {
                    x: source.x_himetric as i32,
                    y: source.y_himetric as i32,
                };
                let mut touch = POINTER_TOUCH_INFO::default();
                touch.pointerInfo.pointerType = PT_TOUCHPAD;
                touch.pointerInfo.pointerId = source.contact_id;
                touch.pointerInfo.pointerFlags = pointer_flags;
                touch.pointerInfo.ptHimetricLocation = point;
                touch.pointerInfo.ptHimetricLocationRaw = point;
                target.r#type = PT_TOUCHPAD;
                target.Anonymous.touchInfo = touch;
            }
            native
        }

        #[must_use]
        pub const fn len(&self) -> u8 {
            self.count
        }

        #[must_use]
        pub const fn is_empty(&self) -> bool {
            self.count == 0
        }

        #[must_use]
        pub fn as_ptr(&self) -> *const POINTER_TYPE_INFO {
            self.contacts.as_ptr()
        }

        #[must_use]
        pub fn inspect(&self, index: usize) -> Option<NativeTouchpadContactView> {
            if index >= usize::from(self.count) {
                return None;
            }
            // SAFETY: the encoder initialized the active union member as
            // `touchInfo` for every index below `count`, and the structure is
            // copied by value before the safe view is returned.
            let touch = unsafe { self.contacts[index].Anonymous.touchInfo };
            Some(NativeTouchpadContactView {
                pointer_type: self.contacts[index].r#type,
                pointer_id: touch.pointerInfo.pointerId,
                pointer_flags: touch.pointerInfo.pointerFlags,
                x_himetric: touch.pointerInfo.ptHimetricLocation.x,
                y_himetric: touch.pointerInfo.ptHimetricLocation.y,
                x_himetric_raw: touch.pointerInfo.ptHimetricLocationRaw.x,
                y_himetric_raw: touch.pointerInfo.ptHimetricLocationRaw.y,
                dw_time: touch.pointerInfo.dwTime,
                performance_count: touch.pointerInfo.PerformanceCount,
                touch_mask: touch.touchMask,
            })
        }
    }
}

#[cfg(windows)]
pub use native::{NativeTouchpadBatch, NativeTouchpadContactView};
