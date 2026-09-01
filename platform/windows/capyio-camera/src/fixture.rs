use capyio_core::StreamId;
use capyio_video::{
    VideoColorimetry, VideoFrameDescriptor, VideoFrameFlags, VideoPixelFormat, VideoStreamSpec,
};

use crate::CameraFixtureError;

const FRAME_TIMEBASE_NANOS: u64 = 1_000_000_000;
const FIXTURE_FRAMES_PER_SECOND: u64 = 30;
const COLOR_BAR_COUNT: usize = 8;
const CLOCK_BAND_HEIGHT: usize = 64;
const CLOCK_MARKER_WIDTH: usize = 32;
const CLOCK_CELL_SIZE: usize = 8;

// Studio-range code values. They are a deterministic test palette rather than
// a claim of SMPTE test-signal conformance.
const COLOR_BARS_YUV: [(u8, u8, u8); COLOR_BAR_COUNT] = [
    (235, 128, 128),
    (219, 16, 138),
    (188, 154, 16),
    (173, 42, 26),
    (78, 214, 230),
    (63, 102, 240),
    (32, 240, 118),
    (16, 128, 128),
];

#[must_use]
pub const fn fixture_stream_spec() -> VideoStreamSpec {
    VideoStreamSpec::camera_720p30_nv12()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedVideoFrame {
    pub descriptor: VideoFrameDescriptor,
    pub payload: Vec<u8>,
}

impl GeneratedVideoFrame {
    pub fn validate(&self, selected: &VideoStreamSpec) -> Result<(), CameraFixtureError> {
        self.descriptor.validate(selected)?;
        let actual = self.payload.len();
        if u64::try_from(actual).ok() != Some(self.descriptor.payload_bytes) {
            return Err(CameraFixtureError::PayloadLengthMismatch {
                declared: self.descriptor.payload_bytes,
                actual,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct DeterministicNv12Source {
    stream_id: StreamId,
    stream_epoch: u64,
    next_sequence: u64,
    next_source_timestamp_nanos: u64,
}

impl DeterministicNv12Source {
    pub fn new(
        stream_id: StreamId,
        stream_epoch: u64,
        start_timestamp_nanos: u64,
    ) -> Result<Self, CameraFixtureError> {
        Self::new_at_sequence(stream_id, stream_epoch, 0, start_timestamp_nanos)
    }

    /// Builds a deterministic source whose next frame begins at an existing
    /// output sequence and timestamp.
    ///
    /// This lets a bounded placeholder resume an already-running virtual
    /// stream without rewinding either descriptor field.
    pub fn new_at_sequence(
        stream_id: StreamId,
        stream_epoch: u64,
        next_sequence: u64,
        next_source_timestamp_nanos: u64,
    ) -> Result<Self, CameraFixtureError> {
        if stream_epoch == 0 {
            return Err(CameraFixtureError::InvalidStreamEpoch);
        }
        fixture_stream_spec().validate()?;
        let following_sequence = next_sequence
            .checked_add(1)
            .ok_or(CameraFixtureError::SequenceExhausted)?;
        let timestamp_offset =
            timeline_offset_nanos(next_sequence).ok_or(CameraFixtureError::TimestampOverflow)?;
        let following_offset = timeline_offset_nanos(following_sequence)
            .ok_or(CameraFixtureError::TimestampOverflow)?;
        let duration_nanos = following_offset
            .checked_sub(timestamp_offset)
            .ok_or(CameraFixtureError::TimestampOverflow)?;
        next_source_timestamp_nanos
            .checked_add(duration_nanos)
            .ok_or(CameraFixtureError::TimestampOverflow)?;
        Ok(Self {
            stream_id,
            stream_epoch,
            next_sequence,
            next_source_timestamp_nanos,
        })
    }

    #[must_use]
    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    pub fn next_frame(&mut self) -> Result<GeneratedVideoFrame, CameraFixtureError> {
        let sequence = self.next_sequence;
        let following_sequence = sequence
            .checked_add(1)
            .ok_or(CameraFixtureError::SequenceExhausted)?;
        let timestamp_offset =
            timeline_offset_nanos(sequence).ok_or(CameraFixtureError::TimestampOverflow)?;
        let following_offset = timeline_offset_nanos(following_sequence)
            .ok_or(CameraFixtureError::TimestampOverflow)?;
        let source_timestamp_nanos = self.next_source_timestamp_nanos;
        let duration_nanos = following_offset
            .checked_sub(timestamp_offset)
            .ok_or(CameraFixtureError::TimestampOverflow)?;
        let following_source_timestamp_nanos = source_timestamp_nanos
            .checked_add(duration_nanos)
            .ok_or(CameraFixtureError::TimestampOverflow)?;

        let selected = fixture_stream_spec();
        let payload = render_nv12(sequence, &selected);
        let descriptor = VideoFrameDescriptor {
            stream_id: self.stream_id,
            stream_epoch: self.stream_epoch,
            sequence,
            source_timestamp_nanos,
            duration_nanos,
            payload_bytes: u64::try_from(payload.len())
                .map_err(|_| CameraFixtureError::TimestampOverflow)?,
            flags: VideoFrameFlags::default(),
        };
        let frame = GeneratedVideoFrame {
            descriptor,
            payload,
        };
        frame.validate(&selected)?;
        self.next_sequence = following_sequence;
        self.next_source_timestamp_nanos = following_source_timestamp_nanos;
        Ok(frame)
    }
}

fn timeline_offset_nanos(sequence: u64) -> Option<u64> {
    let offset = u128::from(sequence).checked_mul(u128::from(FRAME_TIMEBASE_NANOS))?
        / u128::from(FIXTURE_FRAMES_PER_SECOND);
    u64::try_from(offset).ok()
}

fn render_nv12(sequence: u64, selected: &VideoStreamSpec) -> Vec<u8> {
    debug_assert_eq!(selected.pixel_format, VideoPixelFormat::Nv12);
    debug_assert_eq!(selected.colorimetry, VideoColorimetry::Bt709Limited);

    let width = selected.width as usize;
    let height = selected.height as usize;
    let luma_bytes = width * height;
    let payload_bytes = luma_bytes + luma_bytes / 2;
    let mut payload = vec![0_u8; payload_bytes];
    let (luma, chroma) = payload.split_at_mut(luma_bytes);

    for row in luma.chunks_exact_mut(width) {
        for (x, sample) in row.iter_mut().enumerate() {
            let bar = x * COLOR_BAR_COUNT / width;
            *sample = COLOR_BARS_YUV[bar].0;
        }
    }

    for row in chroma.chunks_exact_mut(width) {
        for x in (0..width).step_by(2) {
            let bar = x * COLOR_BAR_COUNT / width;
            row[x] = COLOR_BARS_YUV[bar].1;
            row[x + 1] = COLOR_BARS_YUV[bar].2;
        }
    }

    overlay_moving_clock(luma, chroma, width, height, sequence);
    payload
}

fn overlay_moving_clock(
    luma: &mut [u8],
    chroma: &mut [u8],
    width: usize,
    height: usize,
    sequence: u64,
) {
    let band_start = height - CLOCK_BAND_HEIGHT;
    for row in luma[band_start * width..].chunks_exact_mut(width) {
        row.fill(16);
    }
    for row in chroma[(band_start / 2) * width..].chunks_exact_mut(width) {
        row.fill(128);
    }

    let track_cells = (width - CLOCK_MARKER_WIDTH) / CLOCK_CELL_SIZE + 1;
    let marker_x = (sequence % track_cells as u64) as usize * CLOCK_CELL_SIZE;
    for row in luma[band_start * width..(band_start + 32) * width].chunks_exact_mut(width) {
        row[marker_x..marker_x + CLOCK_MARKER_WIDTH].fill(235);
    }

    // Encode the frame sequence as a visible, deterministic 64-bit clock strip.
    let clock_y = height - 16;
    for bit in 0..64 {
        let x = 16 + bit * CLOCK_CELL_SIZE;
        let code = if sequence & (1_u64 << bit) == 0 {
            32
        } else {
            219
        };
        for row in
            luma[clock_y * width..(clock_y + CLOCK_CELL_SIZE) * width].chunks_exact_mut(width)
        {
            row[x..x + CLOCK_CELL_SIZE].fill(code);
        }
    }
}

/// Stable FNV-1a checksum for fixture assertions and diagnostics.
///
/// It detects accidental fixture drift; it is not a security integrity check.
#[must_use]
pub fn frame_checksum64(payload: &[u8]) -> u64 {
    let mut checksum = 0xcbf2_9ce4_8422_2325_u64;
    for byte in payload {
        checksum ^= u64::from(*byte);
        checksum = checksum.wrapping_mul(0x0000_0100_0000_01b3);
    }
    checksum
}
