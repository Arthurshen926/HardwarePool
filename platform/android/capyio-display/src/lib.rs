#![forbid(unsafe_code)]

//! Host-testable lifecycle seam for a future Android display Surface decoder.
//!
//! No Android SDK, JNI, codec, Surface, network or APK is present here.

use capyio_core::StreamId;
use capyio_display_transport::{
    AvcDecoderConfig, EncodedAccessUnitDescriptor, EncodedDisplayError, OwnedEncodedAccessUnit,
};
use thiserror::Error;

pub trait SurfaceDecoder {
    type Error: std::fmt::Display;

    fn configure(&mut self, config: &AvcDecoderConfig) -> Result<(), Self::Error>;

    /// Flushes decoder buffers and resubmits the exact codec-specific data.
    ///
    /// Android MediaCodec may lose codec-specific data after a flush, so a
    /// backend must implement these as one lifecycle operation.
    fn flush_and_resubmit_config(&mut self, config: &AvcDecoderConfig) -> Result<(), Self::Error>;

    /// Copies one complete access unit into a decoder-owned input buffer.
    ///
    /// The borrowed payload cannot be retained by this interface. Implementors
    /// return only after the bytes have been copied or rejected.
    fn queue_access_unit(
        &mut self,
        descriptor: &EncodedAccessUnitDescriptor,
        payload: &[u8],
    ) -> Result<(), Self::Error>;

    fn stop(&mut self) -> Result<(), Self::Error>;
    fn clear_surface(&mut self) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AndroidDisplaySinkState {
    Idle,
    AwaitingKeyFrame,
    Rendering,
    Draining,
    Stopped,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessUnitOutcome {
    DroppedAwaitingKeyFrame,
    Submitted { resynchronized: bool },
    EndOfStreamQueued,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AndroidDisplaySinkMetrics {
    pub submitted_access_units: u64,
    pub dropped_awaiting_key_frame: u64,
    pub sequence_gaps: u64,
    pub decoder_flushes: u64,
    pub end_of_stream_units: u64,
    pub last_sequence: Option<u64>,
    pub last_presentation_time_micros: Option<u64>,
    pub retained_payload_bytes: u64,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum AndroidDisplaySinkError {
    #[error("invalid Android display sink state: {0}")]
    InvalidState(&'static str),
    #[error("encoded display contract failed: {0}")]
    Contract(#[from] EncodedDisplayError),
    #[error("access unit belongs to another stream")]
    WrongStream,
    #[error("access unit belongs to another stream epoch")]
    WrongEpoch,
    #[error("stream reconfiguration requires a newer epoch")]
    NonAdvancingEpoch,
    #[error("duplicate or late access unit sequence")]
    DuplicateOrLateSequence,
    #[error("presentation timestamp did not advance")]
    NonAdvancingPresentationTime,
    #[error("Android Surface decoder failed during {context}: {detail}")]
    Decoder {
        context: &'static str,
        detail: String,
    },
}

pub struct AndroidDisplaySink<D> {
    decoder: D,
    state: AndroidDisplaySinkState,
    config: Option<AvcDecoderConfig>,
    last_configured_stream: Option<StreamId>,
    last_configured_epoch: Option<u64>,
    resynchronizing: bool,
    metrics: AndroidDisplaySinkMetrics,
}

impl<D: SurfaceDecoder> AndroidDisplaySink<D> {
    #[must_use]
    pub fn new(decoder: D) -> Self {
        Self {
            decoder,
            state: AndroidDisplaySinkState::Idle,
            config: None,
            last_configured_stream: None,
            last_configured_epoch: None,
            resynchronizing: false,
            metrics: AndroidDisplaySinkMetrics::default(),
        }
    }

    #[must_use]
    pub const fn state(&self) -> AndroidDisplaySinkState {
        self.state
    }

    #[must_use]
    pub const fn metrics(&self) -> AndroidDisplaySinkMetrics {
        self.metrics
    }

    pub fn configure(&mut self, config: AvcDecoderConfig) -> Result<(), AndroidDisplaySinkError> {
        if !matches!(
            self.state,
            AndroidDisplaySinkState::Idle | AndroidDisplaySinkState::Stopped
        ) {
            return Err(AndroidDisplaySinkError::InvalidState(
                "stop the current decoder before configuring another stream",
            ));
        }
        config.validate()?;
        if self.last_configured_stream == Some(config.stream_id)
            && self
                .last_configured_epoch
                .is_some_and(|epoch| config.stream_epoch <= epoch)
        {
            return Err(AndroidDisplaySinkError::NonAdvancingEpoch);
        }
        if let Err(error) = self.decoder.configure(&config) {
            return Err(self.fail_decoder("configure", error));
        }
        self.last_configured_stream = Some(config.stream_id);
        self.last_configured_epoch = Some(config.stream_epoch);
        self.config = Some(config);
        self.metrics.last_sequence = None;
        self.metrics.last_presentation_time_micros = None;
        self.metrics.retained_payload_bytes = 0;
        self.resynchronizing = false;
        self.state = AndroidDisplaySinkState::AwaitingKeyFrame;
        Ok(())
    }

    pub fn accept(
        &mut self,
        unit: OwnedEncodedAccessUnit,
    ) -> Result<AccessUnitOutcome, AndroidDisplaySinkError> {
        if !matches!(
            self.state,
            AndroidDisplaySinkState::AwaitingKeyFrame | AndroidDisplaySinkState::Rendering
        ) {
            return Err(AndroidDisplaySinkError::InvalidState(
                "access units require a configured running decoder",
            ));
        }
        let config = self
            .config
            .as_ref()
            .ok_or(AndroidDisplaySinkError::InvalidState(
                "decoder configuration is unavailable",
            ))?;
        unit.validate(&config.selected)?;
        if unit.descriptor.stream_id != config.stream_id {
            return Err(AndroidDisplaySinkError::WrongStream);
        }
        if unit.descriptor.stream_epoch != config.stream_epoch {
            return Err(AndroidDisplaySinkError::WrongEpoch);
        }
        if self
            .metrics
            .last_sequence
            .is_some_and(|sequence| unit.descriptor.sequence <= sequence)
        {
            return Err(AndroidDisplaySinkError::DuplicateOrLateSequence);
        }

        if unit.descriptor.flags.end_of_stream {
            if let Err(error) = self
                .decoder
                .queue_access_unit(&unit.descriptor, &unit.payload)
            {
                return Err(self.fail_decoder("queue end of stream", error));
            }
            self.metrics.last_sequence = Some(unit.descriptor.sequence);
            self.metrics.end_of_stream_units += 1;
            self.metrics.retained_payload_bytes = 0;
            self.state = AndroidDisplaySinkState::Draining;
            return Ok(AccessUnitOutcome::EndOfStreamQueued);
        }

        let gap = self.metrics.last_sequence.is_some_and(|sequence| {
            sequence
                .checked_add(1)
                .is_none_or(|next| unit.descriptor.sequence != next)
        });
        let discontinuity = gap || unit.descriptor.flags.discontinuity;
        if discontinuity {
            let flush_config = config.clone();
            if let Err(error) = self.decoder.flush_and_resubmit_config(&flush_config) {
                return Err(self.fail_decoder("flush and resubmit codec config", error));
            }
            if gap {
                self.metrics.sequence_gaps += 1;
            }
            self.metrics.decoder_flushes += 1;
            self.metrics.last_presentation_time_micros = None;
            self.resynchronizing = true;
            self.state = AndroidDisplaySinkState::AwaitingKeyFrame;
        }

        if self.state == AndroidDisplaySinkState::AwaitingKeyFrame
            && !unit.descriptor.flags.key_frame
        {
            self.metrics.dropped_awaiting_key_frame += 1;
            self.metrics.last_sequence = Some(unit.descriptor.sequence);
            self.metrics.retained_payload_bytes = 0;
            return Ok(AccessUnitOutcome::DroppedAwaitingKeyFrame);
        }
        if self
            .metrics
            .last_presentation_time_micros
            .is_some_and(|timestamp| unit.descriptor.presentation_time_micros <= timestamp)
        {
            return Err(AndroidDisplaySinkError::NonAdvancingPresentationTime);
        }

        if let Err(error) = self
            .decoder
            .queue_access_unit(&unit.descriptor, &unit.payload)
        {
            return Err(self.fail_decoder("queue access unit", error));
        }
        self.metrics.submitted_access_units += 1;
        self.metrics.last_sequence = Some(unit.descriptor.sequence);
        self.metrics.last_presentation_time_micros = Some(unit.descriptor.presentation_time_micros);
        self.metrics.retained_payload_bytes = 0;
        self.state = AndroidDisplaySinkState::Rendering;
        let resynchronized = self.resynchronizing;
        self.resynchronizing = false;
        Ok(AccessUnitOutcome::Submitted { resynchronized })
    }

    pub fn stop(&mut self) -> Result<(), AndroidDisplaySinkError> {
        if self.state == AndroidDisplaySinkState::Stopped {
            return Ok(());
        }
        if self.state != AndroidDisplaySinkState::Idle
            && let Err(error) = self.decoder.stop()
        {
            return Err(self.fail_decoder("stop", error));
        }
        if let Err(error) = self.decoder.clear_surface() {
            return Err(self.fail_decoder("clear output surface", error));
        }
        self.config = None;
        self.metrics.retained_payload_bytes = 0;
        self.resynchronizing = false;
        self.state = AndroidDisplaySinkState::Stopped;
        Ok(())
    }

    #[must_use]
    pub fn into_decoder(self) -> D {
        self.decoder
    }

    fn fail_decoder(&mut self, context: &'static str, error: D::Error) -> AndroidDisplaySinkError {
        let _ = self.decoder.stop();
        let _ = self.decoder.clear_surface();
        self.config = None;
        self.metrics.retained_payload_bytes = 0;
        self.resynchronizing = false;
        self.state = AndroidDisplaySinkState::Failed;
        AndroidDisplaySinkError::Decoder {
            context,
            detail: error.to_string(),
        }
    }
}
