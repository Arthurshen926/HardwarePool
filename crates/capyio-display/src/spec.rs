use std::collections::BTreeSet;

use capyio_core::{FormatDescriptor, ProfileId};
use capyio_video::{FrameRate, VideoColorimetry, VideoPixelFormat};
use serde::{Deserialize, Serialize};

use crate::DisplayContractError;

const MAX_STREAM_CANDIDATES: usize = 32;
const MAX_DIMENSION_PIXELS: u32 = 8_192;
pub(crate) const MAX_FRAME_PAYLOAD_BYTES: u64 = 128 * 1024 * 1024;

/// The Windows-side origin of one Display Source.
///
/// `VirtualMonitor` is the product path: Windows chooses duplicate or extend in
/// its display topology before presenting frames to the IDD. Capturing an
/// already visible display remains an explicit no-virtual-monitor fallback.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayWorkflow {
    VirtualMonitor,
    ExistingDisplayCapture,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayCursorMode {
    Composited,
    Hidden,
}

impl DisplayCursorMode {
    #[must_use]
    const fn id(self) -> &'static str {
        match self {
            Self::Composited => "composited",
            Self::Hidden => "hidden",
        }
    }
}

/// One exact decoded packed-raw display candidate.
///
/// Pixel value types are shared with `capyio-video`, but this remains the
/// distinct `capyio.display.frames/1` Profile. A camera/video Port is therefore
/// not directly compatible with a display Port.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayStreamSpec {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    pub pixel_format: VideoPixelFormat,
    pub colorimetry: VideoColorimetry,
    pub cursor_mode: DisplayCursorMode,
    pub target_latency_millis: u32,
    pub maximum_latency_millis: u32,
    pub allow_frame_drop: bool,
}

impl DisplayStreamSpec {
    #[must_use]
    pub const fn fixture_720p30_bgra() -> Self {
        Self {
            width: 1_280,
            height: 720,
            frame_rate: FrameRate::fps_30(),
            pixel_format: VideoPixelFormat::Bgra8,
            colorimetry: VideoColorimetry::SrgbFull,
            cursor_mode: DisplayCursorMode::Composited,
            target_latency_millis: 50,
            maximum_latency_millis: 150,
            allow_frame_drop: true,
        }
    }

    pub fn validate(&self) -> Result<(), DisplayContractError> {
        if self.width == 0
            || self.height == 0
            || self.width > MAX_DIMENSION_PIXELS
            || self.height > MAX_DIMENSION_PIXELS
        {
            return Err(DisplayContractError::InvalidStreamSpec(format!(
                "dimensions must be inside 1..={MAX_DIMENSION_PIXELS} pixels"
            )));
        }
        if self.pixel_format == VideoPixelFormat::Nv12
            && (!self.width.is_multiple_of(2) || !self.height.is_multiple_of(2))
        {
            return Err(DisplayContractError::InvalidStreamSpec(
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
                return Err(DisplayContractError::InvalidStreamSpec(
                    "BGRA8 requires sRGB full range; NV12 requires a limited-range BT preset"
                        .to_owned(),
                ));
            }
        }
        self.frame_rate
            .validate()
            .map_err(|error| DisplayContractError::InvalidStreamSpec(error.to_string()))?;
        if self.target_latency_millis == 0
            || self.maximum_latency_millis < self.target_latency_millis
            || self.maximum_latency_millis > 10_000
        {
            return Err(DisplayContractError::InvalidStreamSpec(
                "latency requires a positive target and a maximum inside ten seconds".to_owned(),
            ));
        }
        if !self.allow_frame_drop {
            return Err(DisplayContractError::InvalidStreamSpec(
                "interactive display v1 requires frame replacement instead of unbounded backlog"
                    .to_owned(),
            ));
        }
        let payload_bytes = self.packed_frame_bytes().ok_or_else(|| {
            DisplayContractError::InvalidStreamSpec("raw frame size overflowed".to_owned())
        })?;
        if payload_bytes > MAX_FRAME_PAYLOAD_BYTES {
            return Err(DisplayContractError::InvalidStreamSpec(format!(
                "packed raw frame exceeds {MAX_FRAME_PAYLOAD_BYTES} bytes"
            )));
        }
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

    #[must_use]
    pub fn format_descriptor(&self) -> FormatDescriptor {
        let mut descriptor = FormatDescriptor::new("packed-raw-display-v1");
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
            .parameters
            .insert("cursor_mode".to_owned(), self.cursor_mode.id().to_owned());
        descriptor
    }
}

#[must_use]
pub fn display_frames_profile() -> ProfileId {
    ProfileId::display_frames_v1()
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayStreamCapabilities {
    pub candidates: Vec<DisplayStreamSpec>,
}

impl DisplayStreamCapabilities {
    pub fn new(candidates: Vec<DisplayStreamSpec>) -> Result<Self, DisplayContractError> {
        let capabilities = Self { candidates };
        capabilities.validate()?;
        Ok(capabilities)
    }

    pub fn validate(&self) -> Result<(), DisplayContractError> {
        if self.candidates.is_empty() {
            return Err(DisplayContractError::EmptyStreamCandidates);
        }
        if self.candidates.len() > MAX_STREAM_CANDIDATES {
            return Err(DisplayContractError::TooManyStreamCandidates {
                actual: self.candidates.len(),
                limit: MAX_STREAM_CANDIDATES,
            });
        }
        for (index, candidate) in self.candidates.iter().enumerate() {
            candidate.validate()?;
            if self.candidates[..index].contains(candidate) {
                return Err(DisplayContractError::DuplicateStreamCandidate);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplaySourceCapabilities {
    pub workflows: BTreeSet<DisplayWorkflow>,
    pub streams: DisplayStreamCapabilities,
}

impl DisplaySourceCapabilities {
    pub fn validate(&self) -> Result<(), DisplayContractError> {
        if self.workflows.is_empty() {
            return Err(DisplayContractError::EmptySourceWorkflows);
        }
        self.streams.validate()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplaySinkCapabilities {
    pub streams: DisplayStreamCapabilities,
}

impl DisplaySinkCapabilities {
    pub fn validate(&self) -> Result<(), DisplayContractError> {
        self.streams.validate()
    }
}

/// Selects the first Source-preferred exact candidate advertised by the Sink.
/// No resize, rotate, mirror, color conversion, codec conversion or QoS rewrite
/// is implicit.
pub fn negotiate_display_stream(
    source: &DisplaySourceCapabilities,
    sink: &DisplaySinkCapabilities,
    workflow: DisplayWorkflow,
) -> Result<DisplayStreamSpec, DisplayContractError> {
    source.validate()?;
    sink.validate()?;
    if !source.workflows.contains(&workflow) {
        return Err(DisplayContractError::UnsupportedWorkflow);
    }
    source
        .streams
        .candidates
        .iter()
        .find(|candidate| sink.streams.candidates.contains(candidate))
        .cloned()
        .ok_or(DisplayContractError::NoCompatibleDisplayStream)
}
