use std::fmt;

use capyio_input::{
    InputContractError, InputFrameHeader, InputStreamDescriptor, MAX_TOUCHPAD_CONTACTS,
    NormalizedMagnitude, TouchpadButtonState, TouchpadButtonType, TouchpadContact,
    TouchpadDescriptor, TouchpadFrame, TouchpadFrameKind, TouchpadPosition,
};

const MOTION_SCALE_PER_MILLE: i64 = 1_000;
const MULTI_FINGER_MOTION_THRESHOLD: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AndroidToolType {
    Finger,
    Stylus,
    Mouse,
    Eraser,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AndroidMotionAction {
    Down,
    PointerDown { action_index: usize },
    Move,
    PointerUp { action_index: usize },
    Up { action_index: usize },
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AndroidPointerSample {
    pub pointer_id: u32,
    pub tool_type: AndroidToolType,
    pub x_px: f32,
    pub y_px: f32,
    /// Android pressure: normally 0..=1, but calibrated devices may exceed 1.
    pub pressure: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AndroidMotionSample {
    pub event_time_nanos: u64,
    pub action: AndroidMotionAction,
    /// Complete MotionEvent pointer array, including the pointer at an up index.
    pub pointers: Vec<AndroidPointerSample>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AndroidTouchSurface {
    pub width_px: u32,
    pub height_px: u32,
    pub descriptor: TouchpadDescriptor,
}

impl AndroidTouchSurface {
    pub fn validate(self) -> Result<(), AndroidTouchpadMappingError> {
        if self.width_px == 0 || self.height_px == 0 {
            return Err(AndroidTouchpadMappingError::InvalidSurface(
                "touch area pixel dimensions must be non-zero".to_owned(),
            ));
        }
        if self.descriptor.reports_contact_size {
            return Err(AndroidTouchpadMappingError::InvalidSurface(
                "this DTO does not report axis-aligned contact size".to_owned(),
            ));
        }
        if self.descriptor.button_type != TouchpadButtonType::NonClickable {
            return Err(AndroidTouchpadMappingError::InvalidSurface(
                "the Android touch-area DTO declares a non-clickable surface".to_owned(),
            ));
        }
        self.descriptor
            .validate()
            .map_err(AndroidTouchpadMappingError::Contract)
    }
}

/// Android-owned spatial policy applied after absolute surface mapping.
///
/// One- and two-finger input is always identity mapped. Once one physical
/// gesture reaches three contacts, each active contact is rebased without a
/// positional jump and subsequent deltas are attenuated until all contacts are
/// released. The fixed-point scale keeps the callback path deterministic and
/// bounded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AndroidTouchpadMotionPolicy {
    multi_finger_scale_per_mille: u16,
}

impl AndroidTouchpadMotionPolicy {
    pub const IDENTITY: Self = Self {
        multi_finger_scale_per_mille: 1_000,
    };

    pub const RECOMMENDED: Self = Self {
        multi_finger_scale_per_mille: 700,
    };

    pub fn attenuated(scale_per_mille: u16) -> Result<Self, AndroidTouchpadMappingError> {
        if !(1..=1_000).contains(&scale_per_mille) {
            return Err(AndroidTouchpadMappingError::InvalidMotionPolicy(format!(
                "multi-finger motion scale must be inside 1..=1000 per mille, received {scale_per_mille}"
            )));
        }
        Ok(Self {
            multi_finger_scale_per_mille: scale_per_mille,
        })
    }

    #[must_use]
    pub const fn multi_finger_scale_per_mille(self) -> u16 {
        self.multi_finger_scale_per_mille
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AndroidTouchpadMappingError {
    InvalidSurface(String),
    InvalidMotionPolicy(String),
    InvalidMotion(String),
    CancelAllRequired,
    TimestampRegression { previous: u64, actual: u64 },
    SequenceExhausted,
    Contract(InputContractError),
}

impl fmt::Display for AndroidTouchpadMappingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSurface(message) => {
                write!(formatter, "invalid Android touch surface: {message}")
            }
            Self::InvalidMotionPolicy(message) => {
                write!(
                    formatter,
                    "invalid Android touchpad motion policy: {message}"
                )
            }
            Self::InvalidMotion(message) => {
                write!(formatter, "invalid Android motion sample: {message}")
            }
            Self::CancelAllRequired => formatter
                .write_str("Android touchpad stream must emit cancel_all before ordinary updates"),
            Self::TimestampRegression { previous, actual } => write!(
                formatter,
                "Android event timestamp {actual} regressed below {previous}"
            ),
            Self::SequenceExhausted => formatter.write_str("Android touchpad sequence exhausted"),
            Self::Contract(error) => write!(
                formatter,
                "touchpad contract rejected mapped frame: {error}"
            ),
        }
    }
}

impl std::error::Error for AndroidTouchpadMappingError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AndroidTouchpadMapper {
    stream: InputStreamDescriptor,
    surface: AndroidTouchSurface,
    motion_policy: AndroidTouchpadMotionPolicy,
    motion_state: AndroidTouchpadMotionState,
    next_sequence: Option<u64>,
    last_timestamp_nanos: Option<u64>,
    requires_cancel_all: bool,
}

impl AndroidTouchpadMapper {
    pub fn new(
        stream: InputStreamDescriptor,
        surface: AndroidTouchSurface,
        first_sequence: u64,
    ) -> Result<Self, AndroidTouchpadMappingError> {
        Self::new_with_motion_policy(
            stream,
            surface,
            first_sequence,
            AndroidTouchpadMotionPolicy::IDENTITY,
        )
    }

    pub fn new_with_motion_policy(
        stream: InputStreamDescriptor,
        surface: AndroidTouchSurface,
        first_sequence: u64,
        motion_policy: AndroidTouchpadMotionPolicy,
    ) -> Result<Self, AndroidTouchpadMappingError> {
        stream
            .validate()
            .map_err(AndroidTouchpadMappingError::Contract)?;
        surface.validate()?;
        Ok(Self {
            stream,
            surface,
            motion_policy,
            motion_state: AndroidTouchpadMotionState::default(),
            next_sequence: Some(first_sequence),
            last_timestamp_nanos: None,
            requires_cancel_all: true,
        })
    }

    pub fn cancel_all(
        &mut self,
        event_time_nanos: u64,
    ) -> Result<TouchpadFrame, AndroidTouchpadMappingError> {
        self.map_event(&AndroidMotionSample {
            event_time_nanos,
            action: AndroidMotionAction::Cancel,
            pointers: Vec::new(),
        })
    }

    pub fn map_event(
        &mut self,
        event: &AndroidMotionSample,
    ) -> Result<TouchpadFrame, AndroidTouchpadMappingError> {
        if event.pointers.len() > usize::from(MAX_TOUCHPAD_CONTACTS) {
            return Err(AndroidTouchpadMappingError::InvalidMotion(format!(
                "pointer array exceeds the {MAX_TOUCHPAD_CONTACTS}-contact bound"
            )));
        }
        if self.requires_cancel_all && event.action != AndroidMotionAction::Cancel {
            return Err(AndroidTouchpadMappingError::CancelAllRequired);
        }
        if let Some(previous) = self.last_timestamp_nanos
            && event.event_time_nanos < previous
        {
            return Err(AndroidTouchpadMappingError::TimestampRegression {
                previous,
                actual: event.event_time_nanos,
            });
        }
        let sequence = self
            .next_sequence
            .ok_or(AndroidTouchpadMappingError::SequenceExhausted)?;

        let (kind, omitted_index) = validate_action(event)?;
        let mut contacts = if kind == TouchpadFrameKind::CancelAll {
            Vec::new()
        } else {
            map_contacts(event, omitted_index, self.surface)?
        };
        let mut next_motion_state = self.motion_state;
        next_motion_state.apply(
            &mut contacts,
            self.motion_policy,
            self.surface.descriptor.physical_size.width_himetric,
            self.surface.descriptor.physical_size.height_himetric,
        );
        let frame = TouchpadFrame {
            header: InputFrameHeader {
                stream_id: self.stream.stream_id,
                stream_epoch: self.stream.stream_epoch,
                sequence,
                source_timestamp_nanos: event.event_time_nanos,
            },
            kind,
            button: TouchpadButtonState::Released,
            contacts,
        };
        frame
            .validate(&self.surface.descriptor)
            .map_err(AndroidTouchpadMappingError::Contract)?;

        self.next_sequence = sequence.checked_add(1);
        self.last_timestamp_nanos = Some(event.event_time_nanos);
        self.motion_state = next_motion_state;
        if kind == TouchpadFrameKind::CancelAll {
            self.requires_cancel_all = false;
        }
        Ok(frame)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct AndroidTouchpadMotionAnchor {
    contact_id: u32,
    raw_x: u32,
    raw_y: u32,
    output_x: u32,
    output_y: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct AndroidTouchpadMotionState {
    multi_finger_armed: bool,
    anchors: [AndroidTouchpadMotionAnchor; MAX_TOUCHPAD_CONTACTS as usize],
    anchor_count: usize,
}

impl AndroidTouchpadMotionState {
    fn apply(
        &mut self,
        contacts: &mut [TouchpadContact],
        policy: AndroidTouchpadMotionPolicy,
        max_x: u32,
        max_y: u32,
    ) {
        if contacts.is_empty() {
            *self = Self::default();
            return;
        }

        if !self.multi_finger_armed && contacts.len() >= MULTI_FINGER_MOTION_THRESHOLD {
            self.multi_finger_armed = true;
            self.rebase(contacts);
            return;
        }
        if !self.multi_finger_armed {
            self.rebase(contacts);
            return;
        }

        let previous = *self;
        self.anchor_count = 0;
        self.anchors = [AndroidTouchpadMotionAnchor::default(); MAX_TOUCHPAD_CONTACTS as usize];
        for contact in contacts {
            let anchor = previous
                .anchors
                .iter()
                .take(previous.anchor_count)
                .find(|anchor| anchor.contact_id == contact.contact_id)
                .copied()
                .unwrap_or(AndroidTouchpadMotionAnchor {
                    contact_id: contact.contact_id,
                    raw_x: contact.position.x_himetric,
                    raw_y: contact.position.y_himetric,
                    output_x: contact.position.x_himetric,
                    output_y: contact.position.y_himetric,
                });
            contact.position.x_himetric = scale_from_anchor(
                contact.position.x_himetric,
                anchor.raw_x,
                anchor.output_x,
                policy.multi_finger_scale_per_mille(),
                max_x,
            );
            contact.position.y_himetric = scale_from_anchor(
                contact.position.y_himetric,
                anchor.raw_y,
                anchor.output_y,
                policy.multi_finger_scale_per_mille(),
                max_y,
            );
            self.anchors[self.anchor_count] = anchor;
            self.anchor_count += 1;
        }
    }

    fn rebase(&mut self, contacts: &[TouchpadContact]) {
        self.anchor_count = contacts.len();
        self.anchors = [AndroidTouchpadMotionAnchor::default(); MAX_TOUCHPAD_CONTACTS as usize];
        for (slot, contact) in self.anchors.iter_mut().zip(contacts) {
            *slot = AndroidTouchpadMotionAnchor {
                contact_id: contact.contact_id,
                raw_x: contact.position.x_himetric,
                raw_y: contact.position.y_himetric,
                output_x: contact.position.x_himetric,
                output_y: contact.position.y_himetric,
            };
        }
    }
}

fn scale_from_anchor(
    raw: u32,
    raw_anchor: u32,
    output_anchor: u32,
    scale_per_mille: u16,
    maximum: u32,
) -> u32 {
    let delta = i64::from(raw) - i64::from(raw_anchor);
    let numerator = delta * i64::from(scale_per_mille);
    let rounded = if numerator >= 0 {
        (numerator + MOTION_SCALE_PER_MILLE / 2) / MOTION_SCALE_PER_MILLE
    } else {
        (numerator - MOTION_SCALE_PER_MILLE / 2) / MOTION_SCALE_PER_MILLE
    };
    (i64::from(output_anchor) + rounded).clamp(0, i64::from(maximum)) as u32
}

fn validate_action(
    event: &AndroidMotionSample,
) -> Result<(TouchpadFrameKind, Option<usize>), AndroidTouchpadMappingError> {
    let count = event.pointers.len();
    let invalid = |message: &str| AndroidTouchpadMappingError::InvalidMotion(message.to_owned());
    match event.action {
        AndroidMotionAction::Cancel => Ok((TouchpadFrameKind::CancelAll, None)),
        AndroidMotionAction::Down if count == 1 => Ok((TouchpadFrameKind::Update, None)),
        AndroidMotionAction::PointerDown { action_index } if count >= 2 && action_index < count => {
            Ok((TouchpadFrameKind::Update, None))
        }
        AndroidMotionAction::Move if count >= 1 => Ok((TouchpadFrameKind::Update, None)),
        AndroidMotionAction::PointerUp { action_index } if count >= 2 && action_index < count => {
            Ok((TouchpadFrameKind::Update, Some(action_index)))
        }
        AndroidMotionAction::Up { action_index } if count == 1 && action_index == 0 => {
            Ok((TouchpadFrameKind::Update, Some(action_index)))
        }
        AndroidMotionAction::Down => Err(invalid("ACTION_DOWN must contain exactly one pointer")),
        AndroidMotionAction::PointerDown { .. } => Err(invalid(
            "ACTION_POINTER_DOWN requires at least two pointers and a valid action index",
        )),
        AndroidMotionAction::Move => Err(invalid("ACTION_MOVE requires at least one pointer")),
        AndroidMotionAction::PointerUp { .. } => Err(invalid(
            "ACTION_POINTER_UP requires at least two pointers and a valid action index",
        )),
        AndroidMotionAction::Up { .. } => Err(invalid(
            "ACTION_UP requires one pointer at action index zero",
        )),
    }
}

fn map_contacts(
    event: &AndroidMotionSample,
    omitted_index: Option<usize>,
    surface: AndroidTouchSurface,
) -> Result<Vec<TouchpadContact>, AndroidTouchpadMappingError> {
    for (index, pointer) in event.pointers.iter().enumerate() {
        if event.pointers[..index]
            .iter()
            .any(|previous| previous.pointer_id == pointer.pointer_id)
        {
            return Err(AndroidTouchpadMappingError::InvalidMotion(
                "pointer IDs must be unique inside one MotionEvent".to_owned(),
            ));
        }
        if pointer.tool_type != AndroidToolType::Finger {
            return Err(AndroidTouchpadMappingError::InvalidMotion(
                "the touchpad area accepts finger pointers only".to_owned(),
            ));
        }
    }

    event
        .pointers
        .iter()
        .enumerate()
        .filter(|(index, _)| Some(*index) != omitted_index)
        .map(|(_, pointer)| map_contact(*pointer, surface))
        .collect()
}

fn map_contact(
    pointer: AndroidPointerSample,
    surface: AndroidTouchSurface,
) -> Result<TouchpadContact, AndroidTouchpadMappingError> {
    let x_himetric = scale_axis(
        pointer.x_px,
        surface.width_px,
        surface.descriptor.physical_size.width_himetric,
        "X",
    )?;
    let y_himetric = scale_axis(
        pointer.y_px,
        surface.height_px,
        surface.descriptor.physical_size.height_himetric,
        "Y",
    )?;
    let pressure = pointer.pressure.map(map_pressure).transpose()?;
    if pressure.is_some() && !surface.descriptor.reports_pressure {
        return Err(AndroidTouchpadMappingError::InvalidMotion(
            "pressure was supplied but the touchpad descriptor does not advertise it".to_owned(),
        ));
    }
    Ok(TouchpadContact {
        contact_id: pointer.pointer_id,
        position: TouchpadPosition {
            x_himetric,
            y_himetric,
        },
        confidence: true,
        size: None,
        pressure,
    })
}

fn scale_axis(
    value: f32,
    source_max: u32,
    target_max: u32,
    label: &str,
) -> Result<u32, AndroidTouchpadMappingError> {
    if !value.is_finite() || value < 0.0 || f64::from(value) > f64::from(source_max) {
        return Err(AndroidTouchpadMappingError::InvalidMotion(format!(
            "{label} coordinate must be finite and inside the Android touch area"
        )));
    }
    Ok((f64::from(value) * f64::from(target_max) / f64::from(source_max)).round() as u32)
}

fn map_pressure(value: f32) -> Result<NormalizedMagnitude, AndroidTouchpadMappingError> {
    if !value.is_finite() || value < 0.0 {
        return Err(AndroidTouchpadMappingError::InvalidMotion(
            "pressure must be finite and non-negative".to_owned(),
        ));
    }
    let normalized = (f64::from(value.min(1.0)) * f64::from(u16::MAX)).round() as u16;
    Ok(NormalizedMagnitude::new(normalized))
}
