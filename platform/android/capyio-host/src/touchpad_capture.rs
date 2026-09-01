use std::fmt;

use capyio_input::{InputStreamDescriptor, MAX_TOUCHPAD_CONTACTS, TouchpadFrame};

use crate::{
    AndroidMotionAction, AndroidMotionSample, AndroidTouchSurface, AndroidTouchpadMapper,
    AndroidTouchpadMappingError, AndroidTouchpadMotionPolicy,
};

const MAX_TRACKED_CONTACTS: usize = MAX_TOUCHPAD_CONTACTS as usize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AndroidTouchpadCaptureState {
    Stopped,
    Running,
    Closed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AndroidTouchpadCaptureError {
    AlreadyRunning,
    NotRunning,
    Closed,
    InvalidLifecycle(String),
    Mapping(AndroidTouchpadMappingError),
}

impl fmt::Display for AndroidTouchpadCaptureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRunning => {
                formatter.write_str("Android touchpad capture is already running")
            }
            Self::NotRunning => formatter.write_str("Android touchpad capture is not running"),
            Self::Closed => formatter.write_str("Android touchpad capture is closed"),
            Self::InvalidLifecycle(message) => {
                write!(formatter, "invalid Android pointer lifecycle: {message}")
            }
            Self::Mapping(error) => write!(formatter, "Android touchpad mapping failed: {error}"),
        }
    }
}

impl std::error::Error for AndroidTouchpadCaptureError {}

impl From<AndroidTouchpadMappingError> for AndroidTouchpadCaptureError {
    fn from(value: AndroidTouchpadMappingError) -> Self {
        Self::Mapping(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AndroidTouchpadCaptureSession {
    mapper: AndroidTouchpadMapper,
    state: AndroidTouchpadCaptureState,
    active_ids: [u32; MAX_TRACKED_CONTACTS],
    active_count: usize,
}

impl AndroidTouchpadCaptureSession {
    pub fn new(
        stream: InputStreamDescriptor,
        surface: AndroidTouchSurface,
        first_sequence: u64,
    ) -> Result<Self, AndroidTouchpadCaptureError> {
        Self::new_with_motion_policy(
            stream,
            surface,
            first_sequence,
            AndroidTouchpadMotionPolicy::RECOMMENDED,
        )
    }

    pub fn new_with_motion_policy(
        stream: InputStreamDescriptor,
        surface: AndroidTouchSurface,
        first_sequence: u64,
        motion_policy: AndroidTouchpadMotionPolicy,
    ) -> Result<Self, AndroidTouchpadCaptureError> {
        Ok(Self {
            mapper: AndroidTouchpadMapper::new_with_motion_policy(
                stream,
                surface,
                first_sequence,
                motion_policy,
            )?,
            state: AndroidTouchpadCaptureState::Stopped,
            active_ids: [0; MAX_TRACKED_CONTACTS],
            active_count: 0,
        })
    }

    #[must_use]
    pub const fn state(&self) -> AndroidTouchpadCaptureState {
        self.state
    }

    #[must_use]
    pub fn active_contact_ids(&self) -> &[u32] {
        &self.active_ids[..self.active_count]
    }

    pub fn start(
        &mut self,
        event_time_nanos: u64,
    ) -> Result<TouchpadFrame, AndroidTouchpadCaptureError> {
        match self.state {
            AndroidTouchpadCaptureState::Running => {
                return Err(AndroidTouchpadCaptureError::AlreadyRunning);
            }
            AndroidTouchpadCaptureState::Closed => {
                return Err(AndroidTouchpadCaptureError::Closed);
            }
            AndroidTouchpadCaptureState::Stopped => {}
        }
        let frame = self.mapper.cancel_all(event_time_nanos)?;
        self.clear_contacts();
        self.state = AndroidTouchpadCaptureState::Running;
        Ok(frame)
    }

    pub fn map_motion(
        &mut self,
        event: &AndroidMotionSample,
    ) -> Result<TouchpadFrame, AndroidTouchpadCaptureError> {
        match self.state {
            AndroidTouchpadCaptureState::Stopped => {
                return Err(AndroidTouchpadCaptureError::NotRunning);
            }
            AndroidTouchpadCaptureState::Closed => {
                return Err(AndroidTouchpadCaptureError::Closed);
            }
            AndroidTouchpadCaptureState::Running => {}
        }

        let (next_ids, next_count) = validate_lifecycle(event, self.active_contact_ids())?;
        let frame = self.mapper.map_event(event)?;
        self.active_ids = next_ids;
        self.active_count = next_count;
        Ok(frame)
    }

    pub fn stop(
        &mut self,
        event_time_nanos: u64,
    ) -> Result<Option<TouchpadFrame>, AndroidTouchpadCaptureError> {
        match self.state {
            AndroidTouchpadCaptureState::Stopped => Ok(None),
            AndroidTouchpadCaptureState::Closed => Err(AndroidTouchpadCaptureError::Closed),
            AndroidTouchpadCaptureState::Running => {
                let frame = self.mapper.cancel_all(event_time_nanos)?;
                self.clear_contacts();
                self.state = AndroidTouchpadCaptureState::Stopped;
                Ok(Some(frame))
            }
        }
    }

    pub fn close(
        &mut self,
        event_time_nanos: u64,
    ) -> Result<Option<TouchpadFrame>, AndroidTouchpadCaptureError> {
        match self.state {
            AndroidTouchpadCaptureState::Closed => Ok(None),
            AndroidTouchpadCaptureState::Stopped => {
                self.state = AndroidTouchpadCaptureState::Closed;
                Ok(None)
            }
            AndroidTouchpadCaptureState::Running => {
                let frame = self.mapper.cancel_all(event_time_nanos)?;
                self.clear_contacts();
                self.state = AndroidTouchpadCaptureState::Closed;
                Ok(Some(frame))
            }
        }
    }

    fn clear_contacts(&mut self) {
        self.active_ids = [0; MAX_TRACKED_CONTACTS];
        self.active_count = 0;
    }
}

fn validate_lifecycle(
    event: &AndroidMotionSample,
    active_ids: &[u32],
) -> Result<([u32; MAX_TRACKED_CONTACTS], usize), AndroidTouchpadCaptureError> {
    let mut event_ids = [0; MAX_TRACKED_CONTACTS];
    if event.pointers.len() > MAX_TRACKED_CONTACTS {
        return Err(invalid("pointer array exceeds the five-contact bound"));
    }
    for (index, pointer) in event.pointers.iter().enumerate() {
        if event_ids[..index].contains(&pointer.pointer_id) {
            return Err(invalid("pointer IDs must be unique"));
        }
        event_ids[index] = pointer.pointer_id;
    }
    let event_count = event.pointers.len();

    match event.action {
        AndroidMotionAction::Cancel => Ok(([0; MAX_TRACKED_CONTACTS], 0)),
        AndroidMotionAction::Down => {
            if !active_ids.is_empty() || event_count != 1 {
                return Err(invalid(
                    "ACTION_DOWN requires no active pointer and one new pointer",
                ));
            }
            Ok((event_ids, 1))
        }
        AndroidMotionAction::PointerDown { action_index } => {
            if action_index >= event_count
                || event_count != active_ids.len() + 1
                || !same_set(active_ids, &event_ids[..event_count], Some(action_index))
                || active_ids.contains(&event_ids[action_index])
            {
                return Err(invalid(
                    "ACTION_POINTER_DOWN must retain every active ID and add the indexed pointer",
                ));
            }
            Ok((event_ids, event_count))
        }
        AndroidMotionAction::Move => {
            if active_ids.is_empty() || !same_set(active_ids, &event_ids[..event_count], None) {
                return Err(invalid(
                    "ACTION_MOVE must retain exactly the active pointer IDs",
                ));
            }
            Ok((event_ids, event_count))
        }
        AndroidMotionAction::PointerUp { action_index } => {
            if active_ids.len() < 2
                || action_index >= event_count
                || !same_set(active_ids, &event_ids[..event_count], None)
            {
                return Err(invalid(
                    "ACTION_POINTER_UP must contain every active ID and a valid lifted index",
                ));
            }
            let mut next_ids = [0; MAX_TRACKED_CONTACTS];
            let mut next_count = 0;
            for (index, pointer_id) in event_ids[..event_count].iter().enumerate() {
                if index != action_index {
                    next_ids[next_count] = *pointer_id;
                    next_count += 1;
                }
            }
            Ok((next_ids, next_count))
        }
        AndroidMotionAction::Up { action_index } => {
            if active_ids.len() != 1
                || event_count != 1
                || action_index != 0
                || active_ids[0] != event_ids[0]
            {
                return Err(invalid(
                    "ACTION_UP must lift the sole active pointer at index zero",
                ));
            }
            Ok(([0; MAX_TRACKED_CONTACTS], 0))
        }
    }
}

fn same_set(active_ids: &[u32], event_ids: &[u32], omitted_index: Option<usize>) -> bool {
    if active_ids.len() + usize::from(omitted_index.is_some()) != event_ids.len() {
        return false;
    }
    active_ids.iter().all(|active_id| {
        event_ids
            .iter()
            .enumerate()
            .any(|(index, event_id)| Some(index) != omitted_index && event_id == active_id)
    })
}

fn invalid(message: &str) -> AndroidTouchpadCaptureError {
    AndroidTouchpadCaptureError::InvalidLifecycle(message.to_owned())
}
