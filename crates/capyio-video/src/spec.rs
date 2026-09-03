use capyio_core::{FormatDescriptor, ProfileId};
use serde::{Deserialize, Serialize};

use crate::VideoContractError;

const MAX_STREAM_CANDIDATES: usize = 32;
const MAX_DIMENSION_PIXELS: u32 = 8_192;
const MAX_FRAME_RATE: f64 = 480.0;
pub(crate) const MAX_FRAME_PAYLOAD_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameRate {
    numerator: u32,
    denominator: u32,
}

impl FrameRate {
    /// Creates a positive reduced rational, so equivalent rates compare equal.
    #[must_use]
    pub const fn new(numerator: u32, denominator: u32) -> Option<Self> {
        if numerator == 0 || denominator == 0 {
            return None;
        }
        let divisor = greatest_common_divisor(numerator, denominator);
        Some(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    #[must_use]
    pub const fn fps_30() -> Self {
        Self {
            numerator: 30,
            denominator: 1,
        }
    }

    #[must_use]
    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    #[must_use]
    pub const fn denominator(self) -> u32 {
        self.denominator
    }

    pub fn validate(self) -> Result<(), VideoContractError> {
        if self.numerator == 0 || self.denominator == 0 {
            return Err(VideoContractError::InvalidStreamSpec(
                "frame-rate numerator and denominator must be positive".to_owned(),
            ));
        }
        if greatest_common_divisor(self.numerator, self.denominator) != 1 {
            return Err(VideoContractError::InvalidStreamSpec(
                "frame rate must use a reduced rational".to_owned(),
            ));
        }
        let frames_per_second = f64::from(self.numerator) / f64::from(self.denominator);
        if frames_per_second > MAX_FRAME_RATE {
            return Err(VideoContractError::InvalidStreamSpec(format!(
                "frame rate {frames_per_second} exceeds {MAX_FRAME_RATE} fps"
            )));
        }
        Ok(())
    }
}

const fn greatest_common_divisor(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoPixelFormat {
    Nv12,
    Bgra8,
}

impl VideoPixelFormat {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Nv12 => "nv12",
            Self::Bgra8 => "bgra8",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoColorimetry {
    SrgbFull,
    Bt601Limited,
    Bt709Limited,
    Bt2020Limited,
}

impl VideoColorimetry {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::SrgbFull => "srgb_full",
            Self::Bt601Limited => "bt601_limited",
            Self::Bt709Limited => "bt709_limited",
            Self::Bt2020Limited => "bt2020_limited",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoUseCase {
    InteractivePreview,
    CameraBalanced,
    RecordingQuality,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoQosPolicy {
    pub use_case: VideoUseCase,
    pub target_latency_millis: u32,
    pub maximum_latency_millis: u32,
    pub maximum_buffered_frames: u16,
    pub allow_frame_drop: bool,
}

impl VideoQosPolicy {
    #[must_use]
    pub const fn camera_balanced() -> Self {
        Self {
            use_case: VideoUseCase::CameraBalanced,
            target_latency_millis: 120,
            maximum_latency_millis: 500,
            maximum_buffered_frames: 12,
            allow_frame_drop: true,
        }
    }

    pub fn validate(self) -> Result<(), VideoContractError> {
        if self.target_latency_millis == 0
            || self.maximum_latency_millis < self.target_latency_millis
            || self.maximum_latency_millis > 10_000
        {
            return Err(VideoContractError::InvalidStreamSpec(
                "latency must have a positive target and a maximum inside the ten-second bound"
                    .to_owned(),
            ));
        }
        if self.maximum_buffered_frames == 0 || self.maximum_buffered_frames > 512 {
            return Err(VideoContractError::InvalidStreamSpec(
                "maximum buffered frames must be inside 1..=512".to_owned(),
            ));
        }
        Ok(())
    }
}

/// One canonical packed raw-video candidate.
///
/// Frames are already upright and unmirrored. Rotation, mirroring, decode and
/// color conversion are explicit Adapter/Converter work, never negotiation side effects.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoStreamSpec {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    pub pixel_format: VideoPixelFormat,
    pub colorimetry: VideoColorimetry,
    pub qos: VideoQosPolicy,
}

impl VideoStreamSpec {
    #[must_use]
    pub const fn camera_720p30_nv12() -> Self {
        Self {
            width: 1280,
            height: 720,
            frame_rate: FrameRate::fps_30(),
            pixel_format: VideoPixelFormat::Nv12,
            colorimetry: VideoColorimetry::Bt709Limited,
            qos: VideoQosPolicy::camera_balanced(),
        }
    }

    pub fn validate(&self) -> Result<(), VideoContractError> {
        if self.width == 0
            || self.height == 0
            || self.width > MAX_DIMENSION_PIXELS
            || self.height > MAX_DIMENSION_PIXELS
        {
            return Err(VideoContractError::InvalidStreamSpec(format!(
                "dimensions must be inside 1..={MAX_DIMENSION_PIXELS} pixels"
            )));
        }
        if self.pixel_format == VideoPixelFormat::Nv12
            && (!self.width.is_multiple_of(2) || !self.height.is_multiple_of(2))
        {
            return Err(VideoContractError::InvalidStreamSpec(
                "NV12 requires even width and height".to_owned(),
            ));
        }
        match (self.pixel_format, self.colorimetry) {
            (VideoPixelFormat::Bgra8, VideoColorimetry::SrgbFull)
            | (
                VideoPixelFormat::Nv12,
                VideoColorimetry::Bt601Limited
                | VideoColorimetry::Bt709Limited
                | VideoColorimetry::Bt2020Limited,
            ) => {}
            _ => {
                return Err(VideoContractError::InvalidStreamSpec(
                    "BGRA8 requires sRGB full range; NV12 requires a limited-range BT preset"
                        .to_owned(),
                ));
            }
        }
        let payload_bytes = self.packed_frame_bytes().ok_or_else(|| {
            VideoContractError::InvalidStreamSpec("raw frame size overflowed".to_owned())
        })?;
        if payload_bytes > MAX_FRAME_PAYLOAD_BYTES {
            return Err(VideoContractError::InvalidStreamSpec(format!(
                "packed raw frame exceeds {MAX_FRAME_PAYLOAD_BYTES} bytes"
            )));
        }
        self.frame_rate.validate()?;
        self.qos.validate()?;
        Ok(())
    }

    #[must_use]
    pub fn packed_frame_bytes(&self) -> Option<u64> {
        match self.pixel_format {
            VideoPixelFormat::Nv12 => u64::from(self.width)
                .checked_mul(u64::from(self.height))
                .and_then(|pixels| pixels.checked_mul(3))
                .map(|bytes_times_two| bytes_times_two / 2),
            VideoPixelFormat::Bgra8 => u64::from(self.width)
                .checked_mul(u64::from(self.height))
                .and_then(|pixels| pixels.checked_mul(4)),
        }
    }

    /// Core catalog descriptor for this exact semantic candidate.
    #[must_use]
    pub fn format_descriptor(&self) -> FormatDescriptor {
        let mut descriptor = FormatDescriptor::new("packed-raw-video-v1");
        descriptor
            .parameters
            .insert("width".to_owned(), self.width.to_string());
        descriptor
            .parameters
            .insert("height".to_owned(), self.height.to_string());
        descriptor.parameters.insert(
            "frame_rate".to_owned(),
            format!(
                "{}/{}",
                self.frame_rate.numerator(),
                self.frame_rate.denominator()
            ),
        );
        descriptor
            .parameters
            .insert("pixel_format".to_owned(), self.pixel_format.id().to_owned());
        descriptor
            .parameters
            .insert("colorimetry".to_owned(), self.colorimetry.id().to_owned());
        descriptor
    }
}

#[must_use]
pub fn video_frames_profile() -> ProfileId {
    ProfileId::video_frames_v1()
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoStreamCapabilities {
    pub candidates: Vec<VideoStreamSpec>,
}

impl VideoStreamCapabilities {
    pub fn new(candidates: Vec<VideoStreamSpec>) -> Result<Self, VideoContractError> {
        let capabilities = Self { candidates };
        capabilities.validate()?;
        Ok(capabilities)
    }

    pub fn validate(&self) -> Result<(), VideoContractError> {
        if self.candidates.is_empty() {
            return Err(VideoContractError::EmptyStreamCandidates);
        }
        if self.candidates.len() > MAX_STREAM_CANDIDATES {
            return Err(VideoContractError::TooManyStreamCandidates {
                actual: self.candidates.len(),
                limit: MAX_STREAM_CANDIDATES,
            });
        }
        for (index, candidate) in self.candidates.iter().enumerate() {
            candidate.validate()?;
            if self.candidates[..index].contains(candidate) {
                return Err(VideoContractError::DuplicateStreamCandidate);
            }
        }
        Ok(())
    }
}

/// Selects the first Source-preferred complete candidate also advertised by the Sink.
/// No resize, rotation, decode, color conversion, or QoS rewrite is implicit.
pub fn negotiate_video_stream(
    source: &VideoStreamCapabilities,
    sink: &VideoStreamCapabilities,
    use_case: VideoUseCase,
) -> Result<VideoStreamSpec, VideoContractError> {
    source.validate()?;
    sink.validate()?;
    if !source
        .candidates
        .iter()
        .any(|candidate| candidate.qos.use_case == use_case)
        || !sink
            .candidates
            .iter()
            .any(|candidate| candidate.qos.use_case == use_case)
    {
        return Err(VideoContractError::UnsupportedVideoUseCase);
    }
    source
        .candidates
        .iter()
        .filter(|candidate| candidate.qos.use_case == use_case)
        .find(|candidate| sink.candidates.contains(candidate))
        .cloned()
        .ok_or(VideoContractError::NoCompatibleVideoStream)
}
