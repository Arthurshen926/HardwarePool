use std::{fmt, str::FromStr};

use capyio_input::{
    InputFrameHeader, InputStreamDescriptor, TouchpadButtonState, TouchpadButtonType,
    TouchpadContact, TouchpadDescriptor, TouchpadFrame, TouchpadFrameKind, TouchpadPhysicalSize,
    TouchpadPosition,
};

use crate::{
    WindowsTouchpadContactPhase, WindowsTouchpadProjectionError, WindowsTouchpadProjector,
};

pub const FIXED_INJECTION_UPDATE_FRAMES: u64 = 8;
pub const FIXED_INJECTION_INTERVAL_MILLIS: u64 = 15;
pub const FIXED_DOUBLE_TAP_DRAG_FRAMES: u64 = 22;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntheticTouchpadGesture {
    OneFingerTap,
    OneFingerDoubleTapDrag,
    OneFingerMotion,
    TwoFingerPan,
    ThreeFingerSwipe,
    FourFingerSwipe,
}

impl SyntheticTouchpadGesture {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OneFingerTap => "one-finger-tap",
            Self::OneFingerDoubleTapDrag => "one-finger-double-tap-drag",
            Self::OneFingerMotion => "one-finger-motion",
            Self::TwoFingerPan => "two-finger-pan",
            Self::ThreeFingerSwipe => "three-finger-swipe",
            Self::FourFingerSwipe => "four-finger-swipe",
        }
    }

    #[must_use]
    pub const fn contact_count(self) -> u8 {
        match self {
            Self::OneFingerTap | Self::OneFingerDoubleTapDrag | Self::OneFingerMotion => 1,
            Self::TwoFingerPan => 2,
            Self::ThreeFingerSwipe => 3,
            Self::FourFingerSwipe => 4,
        }
    }
}

impl fmt::Display for SyntheticTouchpadGesture {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl FromStr for SyntheticTouchpadGesture {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "one-finger-tap" => Ok(Self::OneFingerTap),
            "one-finger-double-tap-drag" => Ok(Self::OneFingerDoubleTapDrag),
            "one-finger-motion" => Ok(Self::OneFingerMotion),
            "two-finger-pan" => Ok(Self::TwoFingerPan),
            "three-finger-swipe" => Ok(Self::ThreeFingerSwipe),
            "four-finger-swipe" => Ok(Self::FourFingerSwipe),
            _ => Err(format!("unsupported fixed touchpad gesture: {value}")),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TouchpadInjectionFixture {
    pub gesture: SyntheticTouchpadGesture,
    pub stream: InputStreamDescriptor,
    pub descriptor: TouchpadDescriptor,
    pub frames: Vec<TouchpadFrame>,
    pub interval_millis: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TouchpadInjectionDryRun {
    pub frames_projected: u64,
    pub batches_encoded: u64,
    pub contact_records_encoded: u64,
    pub active_records: u64,
    pub released_records: u64,
    pub cancelled_records: u64,
    pub peak_batch_contacts: u8,
    pub peak_batches_per_frame: u8,
}

impl TouchpadInjectionFixture {
    pub fn dry_run(&self) -> Result<TouchpadInjectionDryRun, WindowsTouchpadProjectionError> {
        let first_sequence = self.frames.first().map_or(0, |frame| frame.header.sequence);
        let mut projector =
            WindowsTouchpadProjector::new(&self.stream, self.descriptor, first_sequence)?;
        let mut metrics = TouchpadInjectionDryRun::default();
        for frame in &self.frames {
            let projection = projector.project(frame)?;
            metrics.frames_projected += 1;
            metrics.peak_batches_per_frame =
                metrics.peak_batches_per_frame.max(projection.batch_count());
            for batch in projection.batches() {
                metrics.batches_encoded += 1;
                metrics.contact_records_encoded += u64::from(batch.len());
                metrics.peak_batch_contacts = metrics.peak_batch_contacts.max(batch.len());
                for contact in batch.contacts() {
                    match contact.phase {
                        WindowsTouchpadContactPhase::Pressed
                        | WindowsTouchpadContactPhase::Updated => metrics.active_records += 1,
                        WindowsTouchpadContactPhase::Released => metrics.released_records += 1,
                        WindowsTouchpadContactPhase::Cancelled => metrics.cancelled_records += 1,
                    }
                }
            }
        }
        Ok(metrics)
    }
}

#[must_use]
pub fn build_touchpad_injection_fixture(
    gesture: SyntheticTouchpadGesture,
    stream: InputStreamDescriptor,
) -> TouchpadInjectionFixture {
    let descriptor = TouchpadDescriptor {
        physical_size: TouchpadPhysicalSize {
            width_himetric: 10_000,
            height_himetric: 6_000,
        },
        max_contacts: 5,
        button_type: TouchpadButtonType::NonClickable,
        reports_contact_size: false,
        reports_pressure: false,
    };
    if gesture == SyntheticTouchpadGesture::OneFingerDoubleTapDrag {
        return TouchpadInjectionFixture {
            gesture,
            frames: double_tap_drag_frames(&stream),
            stream,
            descriptor,
            interval_millis: FIXED_INJECTION_INTERVAL_MILLIS,
        };
    }
    let mut frames = Vec::with_capacity(FIXED_INJECTION_UPDATE_FRAMES as usize + 3);
    frames.push(frame(&stream, 0, TouchpadFrameKind::CancelAll, Vec::new()));
    for step in 0..FIXED_INJECTION_UPDATE_FRAMES {
        frames.push(frame(
            &stream,
            step + 1,
            TouchpadFrameKind::Update,
            contacts_at_step(gesture, step),
        ));
    }
    frames.push(frame(
        &stream,
        FIXED_INJECTION_UPDATE_FRAMES + 1,
        TouchpadFrameKind::Update,
        Vec::new(),
    ));
    frames.push(frame(
        &stream,
        FIXED_INJECTION_UPDATE_FRAMES + 2,
        TouchpadFrameKind::CancelAll,
        Vec::new(),
    ));
    TouchpadInjectionFixture {
        gesture,
        stream,
        descriptor,
        frames,
        interval_millis: FIXED_INJECTION_INTERVAL_MILLIS,
    }
}

fn double_tap_drag_frames(stream: &InputStreamDescriptor) -> Vec<TouchpadFrame> {
    let mut frames = Vec::with_capacity(FIXED_DOUBLE_TAP_DRAG_FRAMES as usize);
    frames.push(frame(stream, 0, TouchpadFrameKind::CancelAll, Vec::new()));

    // A 30 ms stationary first tap, followed by a 45 ms fully released gap.
    frames.push(frame(
        stream,
        1,
        TouchpadFrameKind::Update,
        vec![contact(1, 5_000, 3_000)],
    ));
    frames.push(frame(
        stream,
        2,
        TouchpadFrameKind::Update,
        vec![contact(1, 5_000, 3_000)],
    ));
    frames.push(frame(stream, 3, TouchpadFrameKind::Update, Vec::new()));
    frames.push(frame(stream, 4, TouchpadFrameKind::Update, Vec::new()));
    frames.push(frame(stream, 5, TouchpadFrameKind::Update, Vec::new()));

    // The second contact remains stationary for 15 ms, then stays down while
    // moving 30 mm horizontally over 180 ms before its explicit release.
    frames.push(frame(
        stream,
        6,
        TouchpadFrameKind::Update,
        vec![contact(2, 5_000, 3_000)],
    ));
    frames.push(frame(
        stream,
        7,
        TouchpadFrameKind::Update,
        vec![contact(2, 5_000, 3_000)],
    ));
    for sequence in 8..=19 {
        let x = 5_000 + ((sequence - 7) * 250) as u32;
        frames.push(frame(
            stream,
            sequence,
            TouchpadFrameKind::Update,
            vec![contact(2, x, 3_000)],
        ));
    }
    frames.push(frame(stream, 20, TouchpadFrameKind::Update, Vec::new()));
    frames.push(frame(stream, 21, TouchpadFrameKind::CancelAll, Vec::new()));
    debug_assert_eq!(frames.len(), FIXED_DOUBLE_TAP_DRAG_FRAMES as usize);
    frames
}

fn frame(
    stream: &InputStreamDescriptor,
    sequence: u64,
    kind: TouchpadFrameKind,
    contacts: Vec<TouchpadContact>,
) -> TouchpadFrame {
    TouchpadFrame {
        header: InputFrameHeader {
            stream_id: stream.stream_id,
            stream_epoch: stream.stream_epoch,
            sequence,
            source_timestamp_nanos: 1_000_000_000
                + sequence * FIXED_INJECTION_INTERVAL_MILLIS * 1_000_000,
        },
        kind,
        button: TouchpadButtonState::Released,
        contacts,
    }
}

fn contacts_at_step(gesture: SyntheticTouchpadGesture, step: u64) -> Vec<TouchpadContact> {
    let denominator = FIXED_INJECTION_UPDATE_FRAMES - 1;
    let interpolate = |start: u32, end: u32| -> u32 {
        let start = u64::from(start);
        let end = u64::from(end);
        (start + (end - start) * step / denominator) as u32
    };
    let (x, ys): (u32, &[u32]) = match gesture {
        SyntheticTouchpadGesture::OneFingerTap => (5_000, &[3_000]),
        SyntheticTouchpadGesture::OneFingerDoubleTapDrag => {
            unreachable!("double-tap-and-drag uses its dedicated frame builder")
        }
        SyntheticTouchpadGesture::OneFingerMotion => (interpolate(3_500, 6_500), &[3_000]),
        SyntheticTouchpadGesture::TwoFingerPan => {
            let y = interpolate(1_500, 4_500);
            return vec![contact(1, 3_500, y), contact(2, 6_500, y)];
        }
        SyntheticTouchpadGesture::ThreeFingerSwipe => {
            (interpolate(2_500, 7_500), &[2_000, 3_000, 4_000])
        }
        SyntheticTouchpadGesture::FourFingerSwipe => {
            (interpolate(2_500, 7_500), &[1_500, 2_500, 3_500, 4_500])
        }
    };
    ys.iter()
        .enumerate()
        .map(|(index, y)| contact(index as u32 + 1, x, *y))
        .collect()
}

fn contact(contact_id: u32, x_himetric: u32, y_himetric: u32) -> TouchpadContact {
    TouchpadContact {
        contact_id,
        position: TouchpadPosition {
            x_himetric,
            y_himetric,
        },
        confidence: true,
        size: None,
        pressure: None,
    }
}
