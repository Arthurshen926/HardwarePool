use capyio_core::StreamId;
use capyio_video::VideoPixelFormat;

use crate::{
    DisplayContractError, DisplayFrameDescriptor, DisplayFrameFlags, DisplayStreamSpec,
    OwnedDisplayFrame,
};

/// Generates deterministic BGRA test content without reading a real display.
#[derive(Clone, Debug)]
pub struct DeterministicDisplayFixture {
    selected: DisplayStreamSpec,
    stream_id: StreamId,
    stream_epoch: u64,
    next_sequence: u64,
    start_timestamp_nanos: u64,
    frame_duration_nanos: u64,
    payload_bytes: usize,
}

impl DeterministicDisplayFixture {
    pub fn new(
        selected: DisplayStreamSpec,
        stream_id: StreamId,
        stream_epoch: u64,
        start_timestamp_nanos: u64,
    ) -> Result<Self, DisplayContractError> {
        selected.validate()?;
        if selected.pixel_format != VideoPixelFormat::Bgra8 {
            return Err(DisplayContractError::FixtureRequiresBgra8);
        }
        if stream_epoch == 0 {
            return Err(DisplayContractError::InvalidFrame(
                "fixture epoch must be positive".to_owned(),
            ));
        }
        let payload_bytes = usize::try_from(selected.packed_frame_bytes().ok_or_else(|| {
            DisplayContractError::InvalidStreamSpec("fixture frame size overflowed".to_owned())
        })?)
        .map_err(|_| {
            DisplayContractError::InvalidStreamSpec(
                "fixture frame size does not fit this platform".to_owned(),
            )
        })?;
        let duration = 1_000_000_000_u128
            .checked_mul(u128::from(selected.frame_rate.denominator()))
            .and_then(|value| value.checked_div(u128::from(selected.frame_rate.numerator())))
            .and_then(|value| u64::try_from(value).ok())
            .filter(|value| *value > 0)
            .ok_or(DisplayContractError::FixtureExhausted)?;
        Ok(Self {
            selected,
            stream_id,
            stream_epoch,
            next_sequence: 0,
            start_timestamp_nanos,
            frame_duration_nanos: duration,
            payload_bytes,
        })
    }

    pub fn next_frame(&mut self) -> Result<OwnedDisplayFrame, DisplayContractError> {
        let sequence = self.next_sequence;
        let next_sequence = sequence
            .checked_add(1)
            .ok_or(DisplayContractError::FixtureExhausted)?;
        let timestamp = sequence
            .checked_mul(self.frame_duration_nanos)
            .and_then(|offset| self.start_timestamp_nanos.checked_add(offset))
            .ok_or(DisplayContractError::FixtureExhausted)?;

        let width = usize::try_from(self.selected.width).map_err(|_| {
            DisplayContractError::InvalidStreamSpec("fixture width does not fit usize".to_owned())
        })?;
        let mut payload = vec![0_u8; self.payload_bytes];
        for (index, pixel) in payload.chunks_exact_mut(4).enumerate() {
            let x = index % width;
            let y = index / width;
            let phase = sequence as usize;
            pixel[0] = x.wrapping_add(phase) as u8;
            pixel[1] = y.wrapping_add(phase.wrapping_mul(3)) as u8;
            pixel[2] = x.wrapping_add(y).wrapping_add(phase.wrapping_mul(7)) as u8;
            pixel[3] = u8::MAX;
        }
        let frame = OwnedDisplayFrame {
            descriptor: DisplayFrameDescriptor {
                stream_id: self.stream_id,
                stream_epoch: self.stream_epoch,
                sequence,
                source_timestamp_nanos: timestamp,
                duration_nanos: self.frame_duration_nanos,
                payload_bytes: self.selected.packed_frame_bytes().ok_or_else(|| {
                    DisplayContractError::InvalidFrame("fixture frame size overflowed".to_owned())
                })?,
                flags: DisplayFrameFlags::default(),
            },
            payload,
        };
        frame.validate(&self.selected)?;
        self.next_sequence = next_sequence;
        Ok(frame)
    }
}
