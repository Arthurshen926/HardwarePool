use capyio_display_tls::{AuthenticatedDisplaySender, DisplayTlsError};
use capyio_display_transport::{
    DisplayTransportBinding, EncodedDisplayError, encode_access_unit_record,
    encode_decoder_config_record,
};
use thiserror::Error;

use crate::{
    IddFrameSnapshot, MediaFoundationH264Encoder, MediaFoundationH264EncoderConfig, MfEncoderError,
};

#[derive(Debug, Error)]
pub enum MfDisplayPipelineError {
    #[error(transparent)]
    Encoder(#[from] MfEncoderError),
    #[error(transparent)]
    Contract(#[from] EncodedDisplayError),
    #[error(transparent)]
    Transport(#[from] DisplayTlsError),
}

/// Converts IDD BGRA snapshots into bounded private display records.
///
/// The binding is supplied by the authenticated outer transport. This producer
/// does not perform networking or authentication; it preserves the selected
/// Session/Route stream identity at the encoder-to-transport process boundary.
pub struct MediaFoundationDisplayPipeline {
    binding: DisplayTransportBinding,
    encoder: MediaFoundationH264Encoder,
    configuration_pending: bool,
}

impl MediaFoundationDisplayPipeline {
    pub fn new(
        binding: DisplayTransportBinding,
        config: MediaFoundationH264EncoderConfig,
    ) -> Result<Self, MfDisplayPipelineError> {
        let encoder = MediaFoundationH264Encoder::new(
            binding.selected().clone(),
            binding.stream_id(),
            binding.stream_epoch(),
            config,
        )?;
        Ok(Self {
            binding,
            encoder,
            configuration_pending: true,
        })
    }

    #[must_use]
    pub const fn binding(&self) -> &DisplayTransportBinding {
        &self.binding
    }

    /// Encodes one snapshot into complete private records. The first successful
    /// call always emits the decoder configuration before any access units.
    pub fn encode_frame_records(
        &mut self,
        frame: &IddFrameSnapshot,
    ) -> Result<Vec<Vec<u8>>, MfDisplayPipelineError> {
        let units = self.encoder.encode(frame)?;
        let mut records = Vec::with_capacity(units.len() + usize::from(self.configuration_pending));
        if self.configuration_pending {
            records.push(encode_decoder_config_record(
                &self.encoder.decoder_config()?,
            )?);
            self.configuration_pending = false;
        }
        for unit in units {
            records.push(encode_access_unit_record(&unit, self.binding.selected())?);
        }
        Ok(records)
    }

    /// Encodes and sends one snapshot only when the TLS sender carries the
    /// exact same authenticated Route binding as this encoder pipeline.
    pub fn encode_frame_to_sender(
        &mut self,
        frame: &IddFrameSnapshot,
        sender: &mut AuthenticatedDisplaySender,
    ) -> Result<usize, MfDisplayPipelineError> {
        if sender.binding() != &self.binding {
            return Err(DisplayTlsError::UnauthorizedBinding.into());
        }
        let records = self.encode_frame_records(frame)?;
        for record in &records {
            sender.send_record(record)?;
        }
        Ok(records.len())
    }
}

#[cfg(test)]
mod tests {
    use capyio_core::{RouteId, SessionId, StreamId};
    use capyio_display_transport::{
        BoundPrivateDisplayReceiver, EncodedDisplayStreamSpec, PrivateDisplayRecord,
    };

    use super::*;
    use crate::IddFrameHeader;

    #[test]
    fn first_frame_produces_bound_configuration_then_access_unit() {
        let selected = EncodedDisplayStreamSpec::fixture_720p30_avc();
        let binding = DisplayTransportBinding::new(
            SessionId::new(),
            RouteId::new(),
            StreamId::new(),
            1,
            selected,
        )
        .unwrap();
        let mut receiver = BoundPrivateDisplayReceiver::new(binding.clone());
        let mut pipeline = MediaFoundationDisplayPipeline::new(
            binding,
            MediaFoundationH264EncoderConfig::default(),
        )
        .unwrap();
        let payload = vec![0_u8; 1280 * 720 * 4];
        let records = pipeline
            .encode_frame_records(&IddFrameSnapshot {
                header: IddFrameHeader {
                    generation: 1,
                    sequence_lock: 2,
                    width: 1280,
                    height: 720,
                    stride_bytes: 1280 * 4,
                    pixel_format: 1,
                    payload_bytes: payload.len() as u64,
                    source_timestamp_nanos: 0,
                    flags: 0,
                },
                payload,
            })
            .unwrap();

        assert!(matches!(
            receiver.accept(&records[0]).unwrap(),
            PrivateDisplayRecord::DecoderConfig(_)
        ));
        assert!(records[1..].iter().any(|record| matches!(
            receiver.accept(record).unwrap(),
            PrivateDisplayRecord::AccessUnit(_)
        )));
    }
}
