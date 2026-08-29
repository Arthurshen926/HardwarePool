use serde::{Deserialize, Serialize};

use crate::{AudioDataError, AudioFormat};

const MAX_STREAM_CANDIDATES: usize = 32;
const MIN_COMPRESSED_BITRATE_BPS: u32 = 6_000;
const MAX_COMPRESSED_BITRATE_BPS: u32 = 2_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioEncoding {
    Pcm,
    Opus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioEncodingSpec {
    pub encoding: AudioEncoding,
    pub target_bitrate_bps: Option<u32>,
}

impl AudioEncodingSpec {
    #[must_use]
    pub const fn pcm() -> Self {
        Self {
            encoding: AudioEncoding::Pcm,
            target_bitrate_bps: None,
        }
    }

    #[must_use]
    pub const fn opus(target_bitrate_bps: u32) -> Self {
        Self {
            encoding: AudioEncoding::Opus,
            target_bitrate_bps: Some(target_bitrate_bps),
        }
    }

    pub fn validate(self) -> Result<(), AudioDataError> {
        match (self.encoding, self.target_bitrate_bps) {
            (AudioEncoding::Pcm, None) => Ok(()),
            (AudioEncoding::Pcm, Some(_)) => Err(AudioDataError::InvalidStreamSpec(
                "PCM does not accept a compressed target bitrate".to_owned(),
            )),
            (AudioEncoding::Opus, Some(bitrate))
                if (MIN_COMPRESSED_BITRATE_BPS..=MAX_COMPRESSED_BITRATE_BPS).contains(&bitrate) =>
            {
                Ok(())
            }
            (AudioEncoding::Opus, Some(bitrate)) => {
                Err(AudioDataError::InvalidStreamSpec(format!(
                    "Opus target bitrate {bitrate} is outside \
                     {MIN_COMPRESSED_BITRATE_BPS}..={MAX_COMPRESSED_BITRATE_BPS}"
                )))
            }
            (AudioEncoding::Opus, None) => Err(AudioDataError::InvalidStreamSpec(
                "Opus requires an explicit target bitrate".to_owned(),
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioUseCase {
    VoiceInteractive,
    MediaBalanced,
    MusicLossless,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioProcessingRequest {
    pub acoustic_echo_cancellation: bool,
    pub noise_suppression: bool,
    pub automatic_gain_control: bool,
    pub raw_capture: bool,
}

impl AudioProcessingRequest {
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        self.acoustic_echo_cancellation || self.noise_suppression || self.automatic_gain_control
    }

    fn validate_for(self, use_case: AudioUseCase) -> Result<(), AudioDataError> {
        if self.raw_capture && self.is_enabled() {
            return Err(AudioDataError::InvalidStreamSpec(
                "raw capture cannot enable AEC, noise suppression or AGC".to_owned(),
            ));
        }
        if use_case != AudioUseCase::VoiceInteractive && (self.is_enabled() || self.raw_capture) {
            return Err(AudioDataError::InvalidStreamSpec(
                "voice processing and raw capture are valid only for voice-interactive streams"
                    .to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioQosPolicy {
    pub use_case: AudioUseCase,
    pub target_latency_micros: u32,
    pub maximum_latency_micros: u32,
    pub target_jitter_buffer_micros: u32,
    pub maximum_jitter_buffer_micros: u32,
    pub reorder_window_frames: u16,
    pub allow_packet_loss_concealment: bool,
    pub allow_retransmission: bool,
}

impl AudioQosPolicy {
    #[must_use]
    pub const fn voice_interactive() -> Self {
        Self {
            use_case: AudioUseCase::VoiceInteractive,
            target_latency_micros: 60_000,
            maximum_latency_micros: 180_000,
            target_jitter_buffer_micros: 30_000,
            maximum_jitter_buffer_micros: 120_000,
            reorder_window_frames: 12,
            allow_packet_loss_concealment: true,
            allow_retransmission: false,
        }
    }

    #[must_use]
    pub const fn media_balanced() -> Self {
        Self {
            use_case: AudioUseCase::MediaBalanced,
            target_latency_micros: 100_000,
            maximum_latency_micros: 300_000,
            target_jitter_buffer_micros: 60_000,
            maximum_jitter_buffer_micros: 200_000,
            reorder_window_frames: 24,
            allow_packet_loss_concealment: true,
            allow_retransmission: false,
        }
    }

    #[must_use]
    pub const fn music_lossless() -> Self {
        Self {
            use_case: AudioUseCase::MusicLossless,
            target_latency_micros: 180_000,
            maximum_latency_micros: 600_000,
            target_jitter_buffer_micros: 120_000,
            maximum_jitter_buffer_micros: 400_000,
            reorder_window_frames: 48,
            allow_packet_loss_concealment: false,
            allow_retransmission: true,
        }
    }

    pub fn validate(self) -> Result<(), AudioDataError> {
        if self.target_latency_micros == 0
            || self.maximum_latency_micros < self.target_latency_micros
        {
            return Err(AudioDataError::InvalidStreamSpec(
                "maximum latency must be at least the non-zero target latency".to_owned(),
            ));
        }
        if self.maximum_latency_micros > 2_000_000 {
            return Err(AudioDataError::InvalidStreamSpec(
                "maximum latency exceeds the two-second contract bound".to_owned(),
            ));
        }
        if self.maximum_jitter_buffer_micros < self.target_jitter_buffer_micros
            || self.maximum_jitter_buffer_micros > self.maximum_latency_micros
        {
            return Err(AudioDataError::InvalidStreamSpec(
                "jitter-buffer bounds must fit inside the latency budget".to_owned(),
            ));
        }
        if self.reorder_window_frames == 0 || self.reorder_window_frames > 512 {
            return Err(AudioDataError::InvalidStreamSpec(
                "reorder window must be inside 1..=512 frames".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioStreamSpec {
    pub format: AudioFormat,
    pub encoding: AudioEncodingSpec,
    pub qos: AudioQosPolicy,
    pub processing: AudioProcessingRequest,
}

impl AudioStreamSpec {
    #[must_use]
    pub const fn voice_interactive() -> Self {
        Self {
            format: AudioFormat::microphone_baseline(),
            encoding: AudioEncodingSpec::pcm(),
            qos: AudioQosPolicy::voice_interactive(),
            processing: AudioProcessingRequest {
                acoustic_echo_cancellation: false,
                noise_suppression: false,
                automatic_gain_control: false,
                raw_capture: false,
            },
        }
    }

    #[must_use]
    pub const fn media_balanced() -> Self {
        Self {
            format: AudioFormat::speaker_baseline(),
            encoding: AudioEncodingSpec::pcm(),
            qos: AudioQosPolicy::media_balanced(),
            processing: AudioProcessingRequest {
                acoustic_echo_cancellation: false,
                noise_suppression: false,
                automatic_gain_control: false,
                raw_capture: false,
            },
        }
    }

    #[must_use]
    pub const fn music_lossless() -> Self {
        Self {
            format: AudioFormat::speaker_baseline(),
            encoding: AudioEncodingSpec::pcm(),
            qos: AudioQosPolicy::music_lossless(),
            processing: AudioProcessingRequest {
                acoustic_echo_cancellation: false,
                noise_suppression: false,
                automatic_gain_control: false,
                raw_capture: false,
            },
        }
    }

    pub fn validate(&self) -> Result<(), AudioDataError> {
        self.format.validate()?;
        self.encoding.validate()?;
        self.qos.validate()?;
        self.processing.validate_for(self.qos.use_case)?;
        if self.qos.use_case == AudioUseCase::MusicLossless
            && self.encoding.encoding != AudioEncoding::Pcm
        {
            return Err(AudioDataError::InvalidStreamSpec(
                "music-lossless requires PCM in the current contract".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioStreamCapabilities {
    pub candidates: Vec<AudioStreamSpec>,
}

impl AudioStreamCapabilities {
    pub fn new(candidates: Vec<AudioStreamSpec>) -> Result<Self, AudioDataError> {
        let capabilities = Self { candidates };
        capabilities.validate()?;
        Ok(capabilities)
    }

    pub fn validate(&self) -> Result<(), AudioDataError> {
        if self.candidates.is_empty() {
            return Err(AudioDataError::EmptyStreamCandidates);
        }
        if self.candidates.len() > MAX_STREAM_CANDIDATES {
            return Err(AudioDataError::TooManyStreamCandidates {
                actual: self.candidates.len(),
                limit: MAX_STREAM_CANDIDATES,
            });
        }
        for (index, candidate) in self.candidates.iter().enumerate() {
            candidate.validate()?;
            if self.candidates[..index].contains(candidate) {
                return Err(AudioDataError::DuplicateStreamCandidate);
            }
        }
        Ok(())
    }
}

/// Selects the first Source-preferred complete candidate also advertised by the Sink.
///
/// This bootstrap contract never silently changes format, processing, codec or QoS.
pub fn negotiate_audio_stream(
    source: &AudioStreamCapabilities,
    sink: &AudioStreamCapabilities,
    use_case: AudioUseCase,
) -> Result<AudioStreamSpec, AudioDataError> {
    source.validate()?;
    sink.validate()?;

    let source_supports = source
        .candidates
        .iter()
        .any(|candidate| candidate.qos.use_case == use_case);
    let sink_supports = sink
        .candidates
        .iter()
        .any(|candidate| candidate.qos.use_case == use_case);
    if !source_supports || !sink_supports {
        return Err(AudioDataError::UnsupportedAudioUseCase);
    }

    source
        .candidates
        .iter()
        .filter(|candidate| candidate.qos.use_case == use_case)
        .find(|candidate| sink.candidates.contains(candidate))
        .cloned()
        .ok_or(AudioDataError::NoCompatibleAudioStream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_valid_and_direction_neutral() {
        AudioStreamSpec::voice_interactive()
            .validate()
            .expect("voice");
        AudioStreamSpec::media_balanced().validate().expect("media");
        AudioStreamSpec::music_lossless().validate().expect("music");
    }

    #[test]
    fn media_cannot_silently_enable_voice_processing() {
        let mut invalid = AudioStreamSpec::media_balanced();
        invalid.processing.noise_suppression = true;
        assert!(matches!(
            invalid.validate(),
            Err(AudioDataError::InvalidStreamSpec(_))
        ));
    }

    #[test]
    fn negotiation_uses_source_preference_and_exact_intersection() {
        let voice = AudioStreamSpec::voice_interactive();
        let media = AudioStreamSpec::media_balanced();
        let music = AudioStreamSpec::music_lossless();
        let source = AudioStreamCapabilities::new(vec![music.clone(), media.clone(), voice])
            .expect("source");
        let sink = AudioStreamCapabilities::new(vec![media.clone(), music.clone()]).expect("sink");

        assert_eq!(
            negotiate_audio_stream(&source, &sink, AudioUseCase::MusicLossless).expect("match"),
            music
        );
        assert!(matches!(
            negotiate_audio_stream(&source, &sink, AudioUseCase::VoiceInteractive),
            Err(AudioDataError::UnsupportedAudioUseCase)
        ));
    }

    #[test]
    fn negotiation_does_not_implicitly_convert_compatible_looking_audio() {
        let source_spec = AudioStreamSpec::media_balanced();
        let mut sink_spec = source_spec.clone();
        sink_spec.format.sample_format = crate::AudioSampleFormat::SignedI24Le;
        let source = AudioStreamCapabilities::new(vec![source_spec]).expect("source");
        let sink = AudioStreamCapabilities::new(vec![sink_spec]).expect("sink");

        assert!(matches!(
            negotiate_audio_stream(&source, &sink, AudioUseCase::MediaBalanced),
            Err(AudioDataError::NoCompatibleAudioStream)
        ));
    }

    #[test]
    fn candidate_inventory_is_bounded_and_unique() {
        assert!(matches!(
            AudioStreamCapabilities::new(Vec::new()),
            Err(AudioDataError::EmptyStreamCandidates)
        ));
        assert!(matches!(
            AudioStreamCapabilities::new(vec![
                AudioStreamSpec::media_balanced(),
                AudioStreamSpec::media_balanced(),
            ]),
            Err(AudioDataError::DuplicateStreamCandidate)
        ));
        assert!(matches!(
            AudioStreamCapabilities::new(vec![AudioStreamSpec::media_balanced(); 33]),
            Err(AudioDataError::TooManyStreamCandidates {
                actual: 33,
                limit: 32
            })
        ));
    }

    #[test]
    fn qos_bounds_and_raw_processing_conflicts_are_rejected() {
        let mut invalid_latency = AudioStreamSpec::voice_interactive();
        invalid_latency.qos.maximum_latency_micros = invalid_latency.qos.target_latency_micros - 1;
        assert!(matches!(
            invalid_latency.validate(),
            Err(AudioDataError::InvalidStreamSpec(_))
        ));

        let mut invalid_raw = AudioStreamSpec::voice_interactive();
        invalid_raw.processing.raw_capture = true;
        invalid_raw.processing.acoustic_echo_cancellation = true;
        assert!(matches!(
            invalid_raw.validate(),
            Err(AudioDataError::InvalidStreamSpec(_))
        ));
    }
}
