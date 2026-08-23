use serde::{Deserialize, Serialize};

use crate::AudioDataError;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelLayout {
    Mono,
    Stereo,
    Discrete,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioFormat {
    pub sample_rate_hz: u32,
    pub sample_format: AudioSampleFormat,
    pub channels: u16,
    pub channel_layout: ChannelLayout,
    pub frame_duration_micros: u32,
}

impl AudioFormat {
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

    pub fn validate(&self) -> Result<(), AudioDataError> {
        if !(8_000..=384_000).contains(&self.sample_rate_hz) {
            return Err(AudioDataError::InvalidFormat(format!(
                "sample rate {} Hz is outside 8000..=384000",
                self.sample_rate_hz
            )));
        }
        if !(1..=32).contains(&self.channels) {
            return Err(AudioDataError::InvalidFormat(format!(
                "channel count {} is outside 1..=32",
                self.channels
            )));
        }
        match self.channel_layout {
            ChannelLayout::Mono if self.channels != 1 => {
                return Err(AudioDataError::InvalidFormat(
                    "mono layout requires one channel".to_owned(),
                ));
            }
            ChannelLayout::Stereo if self.channels != 2 => {
                return Err(AudioDataError::InvalidFormat(
                    "stereo layout requires two channels".to_owned(),
                ));
            }
            _ => {}
        }
        if !(2_500..=60_000).contains(&self.frame_duration_micros) {
            return Err(AudioDataError::InvalidFormat(format!(
                "frame duration {} us is outside 2500..=60000",
                self.frame_duration_micros
            )));
        }
        Ok(())
    }

    #[must_use]
    pub fn samples_per_frame(&self) -> Option<u32> {
        let numerator = u64::from(self.sample_rate_hz) * u64::from(self.frame_duration_micros);
        if !numerator.is_multiple_of(1_000_000) {
            return None;
        }
        u32::try_from(numerator / 1_000_000).ok()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioProcessingSupport {
    pub acoustic_echo_cancellation: bool,
    pub noise_suppression: bool,
    pub automatic_gain_control: bool,
    pub raw_capture: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_formats_are_valid_and_integral() {
        let microphone = AudioFormat::microphone_baseline();
        let speaker = AudioFormat::speaker_baseline();
        microphone.validate().expect("microphone");
        speaker.validate().expect("speaker");
        assert_eq!(microphone.samples_per_frame(), Some(480));
        assert_eq!(speaker.samples_per_frame(), Some(480));
    }

    #[test]
    fn layout_must_match_channels() {
        let invalid = AudioFormat {
            channels: 2,
            channel_layout: ChannelLayout::Mono,
            ..AudioFormat::speaker_baseline()
        };
        assert!(matches!(
            invalid.validate(),
            Err(AudioDataError::InvalidFormat(_))
        ));
    }
}
