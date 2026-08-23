use serde::{Deserialize, Serialize};

use crate::{CapabilityId, CoreError};

/// PCM sample representation negotiated for an audio stream epoch.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSampleFormat {
    SignedI16Le,
    SignedI24Le,
    SignedI32Le,
    FloatF32Le,
}

impl AudioSampleFormat {
    #[must_use]
    pub const fn bytes_per_sample(self) -> u8 {
        match self {
            Self::SignedI16Le => 2,
            Self::SignedI24Le => 3,
            Self::SignedI32Le | Self::FloatF32Le => 4,
        }
    }
}

/// Logical channel layout. V1 keeps the model intentionally small.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelLayout {
    Mono,
    Stereo,
    Discrete,
}

/// Negotiated raw media format for one continuous audio epoch.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioFormat {
    pub sample_rate_hz: u32,
    pub sample_format: AudioSampleFormat,
    pub channels: u16,
    pub channel_layout: ChannelLayout,
    pub frame_duration_micros: u32,
}

impl AudioFormat {
    /// Baseline Android microphone format for the MVP.
    #[must_use]
    pub const fn microphone_baseline() -> Self {
        Self {
            sample_rate_hz: 48_000,
            sample_format: AudioSampleFormat::SignedI16Le,
            channels: 1,
            channel_layout: ChannelLayout::Mono,
            frame_duration_micros: 10_000,
        }
    }

    /// Baseline Android speaker format for the MVP.
    #[must_use]
    pub const fn speaker_baseline() -> Self {
        Self {
            sample_rate_hz: 48_000,
            sample_format: AudioSampleFormat::SignedI16Le,
            channels: 2,
            channel_layout: ChannelLayout::Stereo,
            frame_duration_micros: 10_000,
        }
    }

    /// Validates parser/buffer safety limits and channel-layout consistency.
    pub fn validate(&self) -> Result<(), CoreError> {
        if !(8_000..=384_000).contains(&self.sample_rate_hz) {
            return Err(CoreError::InvalidAudioFormat(format!(
                "sample rate {} Hz is outside 8000..=384000",
                self.sample_rate_hz
            )));
        }

        if !(1..=32).contains(&self.channels) {
            return Err(CoreError::InvalidAudioFormat(format!(
                "channel count {} is outside 1..=32",
                self.channels
            )));
        }

        match self.channel_layout {
            ChannelLayout::Mono if self.channels != 1 => {
                return Err(CoreError::InvalidAudioFormat(
                    "mono layout requires exactly one channel".to_owned(),
                ));
            }
            ChannelLayout::Stereo if self.channels != 2 => {
                return Err(CoreError::InvalidAudioFormat(
                    "stereo layout requires exactly two channels".to_owned(),
                ));
            }
            _ => {}
        }

        if !(2_500..=60_000).contains(&self.frame_duration_micros) {
            return Err(CoreError::InvalidAudioFormat(format!(
                "frame duration {} us is outside 2500..=60000",
                self.frame_duration_micros
            )));
        }

        Ok(())
    }

    /// Number of samples per channel in one frame when the duration is integral.
    #[must_use]
    pub fn samples_per_frame(&self) -> Option<u32> {
        let numerator = u64::from(self.sample_rate_hz) * u64::from(self.frame_duration_micros);
        if !numerator.is_multiple_of(1_000_000) {
            return None;
        }
        u32::try_from(numerator / 1_000_000).ok()
    }
}

/// User-visible latency/quality intent, independent of the concrete transport.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioQosMode {
    MediaPlayback,
    VoiceInteractive,
    RawLan,
    RawDuplex,
}

/// Processing features that the provider can actually support.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioProcessingSupport {
    pub acoustic_echo_cancellation: bool,
    pub noise_suppression: bool,
    pub automatic_gain_control: bool,
    pub raw_capture: bool,
}

/// Static details attached to an audio capture or render capability.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioCapabilitySpec {
    pub formats: Vec<AudioFormat>,
    pub qos_modes: Vec<AudioQosMode>,
    pub processing: AudioProcessingSupport,
    pub supports_volume_control: bool,
    pub supports_mute: bool,
}

impl AudioCapabilitySpec {
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.formats.is_empty() {
            return Err(CoreError::InvalidAudioFormat(
                "at least one audio format must be advertised".to_owned(),
            ));
        }

        for format in &self.formats {
            format.validate()?;
        }

        if self.qos_modes.is_empty() {
            return Err(CoreError::InvalidAudioFormat(
                "at least one QoS mode must be advertised".to_owned(),
            ));
        }

        Ok(())
    }

    #[must_use]
    pub fn supports_format(&self, format: &AudioFormat) -> bool {
        self.formats.iter().any(|candidate| candidate == format)
    }
}

/// Relationship metadata for a capture/render pair on the same provider device.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioBundleSpec {
    pub capture_capability_id: CapabilityId,
    pub render_capability_id: CapabilityId,
    pub shared_acoustic_environment: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_formats_are_valid_and_integral() {
        let microphone = AudioFormat::microphone_baseline();
        let speaker = AudioFormat::speaker_baseline();

        microphone.validate().expect("microphone format");
        speaker.validate().expect("speaker format");
        assert_eq!(microphone.samples_per_frame(), Some(480));
        assert_eq!(speaker.samples_per_frame(), Some(480));
    }

    #[test]
    fn layout_must_match_channel_count() {
        let invalid = AudioFormat {
            channels: 2,
            channel_layout: ChannelLayout::Mono,
            ..AudioFormat::speaker_baseline()
        };

        assert!(matches!(
            invalid.validate(),
            Err(CoreError::InvalidAudioFormat(_))
        ));
    }
}
