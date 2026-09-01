use std::{error::Error, fmt};

use capyio_core::StreamId;
use capyio_video::{VideoContractError, VideoFrameDescriptor, VideoStreamSpec};

use crate::{CameraFixtureError, GeneratedVideoFrame, fixture_stream_spec};

const NANOS_PER_MEDIA_FOUNDATION_TICK: u64 = 100;
pub const MAX_VIRTUAL_CAMERA_FRIENDLY_NAME_UTF16: usize = 64;
pub const CAPYIO_CAMERA_SOURCE_CLSID: &str = "{35754be3-54b6-4133-a1c7-1716395c6f1c}";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MfVirtualCameraLifetime {
    Session,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MfVirtualCameraAccess {
    CurrentUser,
}

/// Closed registration plan for the first Windows lab slice.
///
/// System lifetime and all-user access are intentionally not representable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MfVirtualCameraPlan {
    friendly_name: String,
}

impl MfVirtualCameraPlan {
    pub fn new(friendly_name: impl Into<String>) -> Result<Self, MediaFoundationProjectionError> {
        let friendly_name = friendly_name.into();
        let utf16_units = friendly_name.encode_utf16().count();
        if friendly_name.is_empty()
            || friendly_name.trim() != friendly_name
            || friendly_name.chars().any(char::is_control)
            || utf16_units > MAX_VIRTUAL_CAMERA_FRIENDLY_NAME_UTF16
        {
            return Err(MediaFoundationProjectionError::InvalidFriendlyName { utf16_units });
        }
        Ok(Self { friendly_name })
    }

    pub fn capyio_fixture() -> Self {
        Self::new("CapyIO Camera").expect("the fixed friendly name is valid")
    }

    #[must_use]
    pub fn friendly_name(&self) -> &str {
        &self.friendly_name
    }

    #[must_use]
    pub const fn source_clsid(&self) -> &'static str {
        CAPYIO_CAMERA_SOURCE_CLSID
    }

    #[must_use]
    pub const fn lifetime(&self) -> MfVirtualCameraLifetime {
        MfVirtualCameraLifetime::Session
    }

    #[must_use]
    pub const fn access(&self) -> MfVirtualCameraAccess {
        MfVirtualCameraAccess::CurrentUser
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MfVirtualCameraLifecycleState {
    Configured,
    Started,
    Stopped,
    Shutdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MfVirtualCameraAction {
    Start,
    Stop,
    Shutdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MfVirtualCameraLifecycle {
    state: MfVirtualCameraLifecycleState,
}

impl Default for MfVirtualCameraLifecycle {
    fn default() -> Self {
        Self {
            state: MfVirtualCameraLifecycleState::Configured,
        }
    }
}

impl MfVirtualCameraLifecycle {
    #[must_use]
    pub const fn state(self) -> MfVirtualCameraLifecycleState {
        self.state
    }

    pub fn apply(
        &mut self,
        action: MfVirtualCameraAction,
    ) -> Result<MfVirtualCameraLifecycleState, MediaFoundationProjectionError> {
        let next = match (self.state, action) {
            (
                MfVirtualCameraLifecycleState::Configured | MfVirtualCameraLifecycleState::Stopped,
                MfVirtualCameraAction::Start,
            ) => MfVirtualCameraLifecycleState::Started,
            (MfVirtualCameraLifecycleState::Started, MfVirtualCameraAction::Stop) => {
                MfVirtualCameraLifecycleState::Stopped
            }
            (
                MfVirtualCameraLifecycleState::Configured
                | MfVirtualCameraLifecycleState::Started
                | MfVirtualCameraLifecycleState::Stopped,
                MfVirtualCameraAction::Shutdown,
            ) => MfVirtualCameraLifecycleState::Shutdown,
            (state, action) => {
                return Err(MediaFoundationProjectionError::InvalidLifecycleTransition {
                    state,
                    action,
                });
            }
        };
        self.state = next;
        Ok(next)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MfSampleTiming {
    pub sample_time_100ns: i64,
    pub sample_duration_100ns: i64,
}

/// Maps one CapyIO stream epoch onto a QPC-correlated Media Foundation anchor.
#[derive(Clone, Debug)]
pub struct MfSampleTimingMapper {
    stream_id: StreamId,
    stream_epoch: u64,
    source_anchor_nanos: u64,
    qpc_anchor_100ns: i64,
    last_sequence: Option<u64>,
    last_sample_time_100ns: Option<i64>,
}

impl MfSampleTimingMapper {
    pub fn new(
        first_frame: &VideoFrameDescriptor,
        selected: &VideoStreamSpec,
        qpc_anchor_100ns: i64,
    ) -> Result<Self, MediaFoundationProjectionError> {
        first_frame.validate(selected)?;
        if qpc_anchor_100ns < 0 {
            return Err(MediaFoundationProjectionError::InvalidQpcAnchor(
                qpc_anchor_100ns,
            ));
        }
        Ok(Self {
            stream_id: first_frame.stream_id,
            stream_epoch: first_frame.stream_epoch,
            source_anchor_nanos: first_frame.source_timestamp_nanos,
            qpc_anchor_100ns,
            last_sequence: None,
            last_sample_time_100ns: None,
        })
    }

    pub fn map(
        &mut self,
        frame: &VideoFrameDescriptor,
        selected: &VideoStreamSpec,
    ) -> Result<MfSampleTiming, MediaFoundationProjectionError> {
        frame.validate(selected)?;
        if frame.stream_id != self.stream_id {
            return Err(MediaFoundationProjectionError::WrongStream);
        }
        if frame.stream_epoch != self.stream_epoch {
            return Err(MediaFoundationProjectionError::WrongEpoch {
                expected: self.stream_epoch,
                actual: frame.stream_epoch,
            });
        }
        if self
            .last_sequence
            .is_some_and(|last_sequence| frame.sequence <= last_sequence)
        {
            return Err(MediaFoundationProjectionError::NonAdvancingSequence {
                previous: self.last_sequence.expect("checked as present"),
                actual: frame.sequence,
            });
        }
        let delta_nanos = frame
            .source_timestamp_nanos
            .checked_sub(self.source_anchor_nanos)
            .ok_or(MediaFoundationProjectionError::TimestampBeforeAnchor)?;
        let delta_100ns = i64::try_from(delta_nanos / NANOS_PER_MEDIA_FOUNDATION_TICK)
            .map_err(|_| MediaFoundationProjectionError::TimingOverflow)?;
        let sample_time_100ns = self
            .qpc_anchor_100ns
            .checked_add(delta_100ns)
            .ok_or(MediaFoundationProjectionError::TimingOverflow)?;
        if self
            .last_sample_time_100ns
            .is_some_and(|last_sample_time| sample_time_100ns <= last_sample_time)
        {
            return Err(MediaFoundationProjectionError::NonMonotonicSampleTime);
        }
        let sample_duration_100ns =
            i64::try_from(frame.duration_nanos / NANOS_PER_MEDIA_FOUNDATION_TICK)
                .map_err(|_| MediaFoundationProjectionError::TimingOverflow)?;
        if sample_duration_100ns == 0 {
            return Err(MediaFoundationProjectionError::TimingOverflow);
        }

        self.last_sequence = Some(frame.sequence);
        self.last_sample_time_100ns = Some(sample_time_100ns);
        Ok(MfSampleTiming {
            sample_time_100ns,
            sample_duration_100ns,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MfNv12BufferLayout {
    pub row_pitch_bytes: usize,
    pub required_bytes: usize,
}

/// Copies the canonical packed fixture into a positive-pitch MF 2D NV12 buffer.
///
/// Padding is zeroed so no stale process bytes can reach the media pipeline.
pub fn copy_nv12_to_strided_buffer(
    frame: &GeneratedVideoFrame,
    row_pitch_bytes: usize,
    destination: &mut [u8],
) -> Result<MfNv12BufferLayout, MediaFoundationProjectionError> {
    let selected = fixture_stream_spec();
    frame.validate(&selected)?;
    let width = selected.width as usize;
    let height = selected.height as usize;
    if row_pitch_bytes < width {
        return Err(MediaFoundationProjectionError::InvalidRowPitch {
            actual: row_pitch_bytes,
            minimum: width,
        });
    }
    let rows = height
        .checked_add(height / 2)
        .ok_or(MediaFoundationProjectionError::BufferSizeOverflow)?;
    let required_bytes = row_pitch_bytes
        .checked_mul(rows)
        .ok_or(MediaFoundationProjectionError::BufferSizeOverflow)?;
    if destination.len() < required_bytes {
        return Err(MediaFoundationProjectionError::DestinationTooSmall {
            actual: destination.len(),
            required: required_bytes,
        });
    }

    destination[..required_bytes].fill(0);
    let luma_bytes = width
        .checked_mul(height)
        .ok_or(MediaFoundationProjectionError::BufferSizeOverflow)?;
    let (source_luma, source_chroma) = frame.payload.split_at(luma_bytes);
    let (destination_luma, destination_chroma) =
        destination[..required_bytes].split_at_mut(row_pitch_bytes * height);

    for (source_row, destination_row) in source_luma
        .chunks_exact(width)
        .zip(destination_luma.chunks_exact_mut(row_pitch_bytes))
    {
        destination_row[..width].copy_from_slice(source_row);
    }
    for (source_row, destination_row) in source_chroma
        .chunks_exact(width)
        .zip(destination_chroma.chunks_exact_mut(row_pitch_bytes))
    {
        destination_row[..width].copy_from_slice(source_row);
    }

    Ok(MfNv12BufferLayout {
        row_pitch_bytes,
        required_bytes,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MediaFoundationProjectionError {
    InvalidFriendlyName {
        utf16_units: usize,
    },
    InvalidLifecycleTransition {
        state: MfVirtualCameraLifecycleState,
        action: MfVirtualCameraAction,
    },
    InvalidQpcAnchor(i64),
    WrongStream,
    WrongEpoch {
        expected: u64,
        actual: u64,
    },
    NonAdvancingSequence {
        previous: u64,
        actual: u64,
    },
    TimestampBeforeAnchor,
    NonMonotonicSampleTime,
    TimingOverflow,
    InvalidRowPitch {
        actual: usize,
        minimum: usize,
    },
    DestinationTooSmall {
        actual: usize,
        required: usize,
    },
    BufferSizeOverflow,
    VideoContract(VideoContractError),
    Fixture(CameraFixtureError),
}

impl fmt::Display for MediaFoundationProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFriendlyName { utf16_units } => write!(
                formatter,
                "virtual-camera friendly name is invalid or exceeds the {MAX_VIRTUAL_CAMERA_FRIENDLY_NAME_UTF16}-unit bound ({utf16_units})"
            ),
            Self::InvalidLifecycleTransition { state, action } => {
                write!(
                    formatter,
                    "cannot apply {action:?} while camera is {state:?}"
                )
            }
            Self::InvalidQpcAnchor(anchor) => {
                write!(
                    formatter,
                    "Media Foundation QPC anchor {anchor} is negative"
                )
            }
            Self::WrongStream => formatter.write_str("frame belongs to another video stream"),
            Self::WrongEpoch { expected, actual } => {
                write!(formatter, "frame epoch {actual} does not match {expected}")
            }
            Self::NonAdvancingSequence { previous, actual } => write!(
                formatter,
                "frame sequence {actual} does not advance previous sequence {previous}"
            ),
            Self::TimestampBeforeAnchor => {
                formatter.write_str("frame source timestamp precedes the projection anchor")
            }
            Self::NonMonotonicSampleTime => {
                formatter.write_str("mapped Media Foundation sample time did not advance")
            }
            Self::TimingOverflow => formatter.write_str("Media Foundation timing overflowed"),
            Self::InvalidRowPitch { actual, minimum } => write!(
                formatter,
                "NV12 row pitch {actual} is smaller than width {minimum}"
            ),
            Self::DestinationTooSmall { actual, required } => write!(
                formatter,
                "MF 2D buffer has {actual} bytes; {required} are required"
            ),
            Self::BufferSizeOverflow => {
                formatter.write_str("Media Foundation buffer layout overflowed")
            }
            Self::VideoContract(error) => error.fmt(formatter),
            Self::Fixture(error) => error.fmt(formatter),
        }
    }
}

impl Error for MediaFoundationProjectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::VideoContract(error) => Some(error),
            Self::Fixture(error) => Some(error),
            _ => None,
        }
    }
}

impl From<VideoContractError> for MediaFoundationProjectionError {
    fn from(value: VideoContractError) -> Self {
        Self::VideoContract(value)
    }
}

impl From<CameraFixtureError> for MediaFoundationProjectionError {
    fn from(value: CameraFixtureError) -> Self {
        Self::Fixture(value)
    }
}
