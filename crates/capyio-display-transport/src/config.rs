use capyio_core::StreamId;
use capyio_display::DisplayStreamSpec;

use crate::EncodedDisplayError;

pub const MAX_ACCESS_UNIT_BYTES: u32 = 16 * 1024 * 1024;
const MAX_CODEC_SPECIFIC_DATA_BYTES: usize = 64 * 1024;
const ANNEX_B_START_CODE: [u8; 4] = [0, 0, 0, 1];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodedDisplayCodec {
    AvcAnnexB,
}

impl EncodedDisplayCodec {
    #[must_use]
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::AvcAnnexB => "video/avc",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AvcProfile {
    Baseline,
    Main,
    High,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AvcLevel {
    L3_1,
    L4,
    L4_1,
    L4_2,
    L5,
    L5_1,
    L5_2,
}

impl AvcLevel {
    const ORDERED: [Self; 7] = [
        Self::L3_1,
        Self::L4,
        Self::L4_1,
        Self::L4_2,
        Self::L5,
        Self::L5_1,
        Self::L5_2,
    ];

    /// Returns the lowest represented AVC level whose macroblock limits cover
    /// the exact decoded dimensions and frame rate.
    #[must_use]
    pub fn minimum_for(decoded: &DisplayStreamSpec) -> Option<Self> {
        Self::ORDERED
            .into_iter()
            .find(|level| level.supports(decoded))
    }

    #[must_use]
    pub fn supports(self, decoded: &DisplayStreamSpec) -> bool {
        let width_mbs = u64::from(decoded.width).div_ceil(16);
        let height_mbs = u64::from(decoded.height).div_ceil(16);
        let frame_mbs = match width_mbs.checked_mul(height_mbs) {
            Some(value) => value,
            None => return false,
        };
        let (maximum_frame_mbs, maximum_mbs_per_second) = self.macroblock_limits();
        if frame_mbs > maximum_frame_mbs {
            return false;
        }
        frame_mbs
            .checked_mul(u64::from(decoded.frame_rate.numerator()))
            .is_some_and(|actual| {
                u64::from(maximum_mbs_per_second)
                    .checked_mul(u64::from(decoded.frame_rate.denominator()))
                    .is_some_and(|maximum| actual <= maximum)
            })
    }

    const fn macroblock_limits(self) -> (u64, u32) {
        match self {
            Self::L3_1 => (3_600, 108_000),
            Self::L4 | Self::L4_1 => (8_192, 245_760),
            Self::L4_2 => (8_704, 522_240),
            Self::L5 => (22_080, 589_824),
            Self::L5_1 => (36_864, 983_040),
            Self::L5_2 => (36_864, 2_073_600),
        }
    }
}

/// One exact encoded candidate for a private AdapterManaged data plane.
///
/// `decoded` retains the distinct Display Profile semantics. A platform encoder
/// is an explicit converter from that selected raw mode to this encoded mode;
/// no codec conversion is implied by Core Route compatibility.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedDisplayStreamSpec {
    pub decoded: DisplayStreamSpec,
    pub codec: EncodedDisplayCodec,
    pub avc_profile: AvcProfile,
    pub avc_level: AvcLevel,
    pub maximum_access_unit_bytes: u32,
}

impl EncodedDisplayStreamSpec {
    #[must_use]
    pub fn fixture_720p30_avc() -> Self {
        Self {
            decoded: DisplayStreamSpec::fixture_720p30_bgra(),
            codec: EncodedDisplayCodec::AvcAnnexB,
            avc_profile: AvcProfile::Main,
            avc_level: AvcLevel::L3_1,
            maximum_access_unit_bytes: 2 * 1024 * 1024,
        }
    }

    pub fn validate(&self) -> Result<(), EncodedDisplayError> {
        self.decoded
            .validate()
            .map_err(|error| EncodedDisplayError::InvalidStream(error.to_string()))?;
        if self.maximum_access_unit_bytes == 0
            || self.maximum_access_unit_bytes > MAX_ACCESS_UNIT_BYTES
        {
            return Err(EncodedDisplayError::InvalidStream(format!(
                "maximum access unit bytes must be inside 1..={MAX_ACCESS_UNIT_BYTES}"
            )));
        }
        if !self.avc_level.supports(&self.decoded) {
            return Err(EncodedDisplayError::InvalidStream(
                "selected AVC level does not cover the decoded macroblock rate".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Exact configuration required before AVC frame data enters a decoder.
///
/// Android MediaCodec maps these buffers to `csd-0` (SPS) and `csd-1` (PPS).
/// They are retained only as small decoder configuration so they can be
/// resubmitted after a flush; they are never display pixel payloads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvcDecoderConfig {
    pub stream_id: StreamId,
    pub stream_epoch: u64,
    pub selected: EncodedDisplayStreamSpec,
    pub sequence_parameter_set: Vec<u8>,
    pub picture_parameter_set: Vec<u8>,
}

impl AvcDecoderConfig {
    pub fn validate(&self) -> Result<(), EncodedDisplayError> {
        self.selected.validate()?;
        if self.stream_epoch == 0 {
            return Err(EncodedDisplayError::InvalidDecoderConfig(
                "stream epoch must be positive".to_owned(),
            ));
        }
        validate_csd("sequence parameter set", &self.sequence_parameter_set, 7)?;
        validate_csd("picture parameter set", &self.picture_parameter_set, 8)?;
        Ok(())
    }
}

fn validate_csd(
    label: &str,
    bytes: &[u8],
    expected_nal_unit_type: u8,
) -> Result<(), EncodedDisplayError> {
    if bytes.len() <= ANNEX_B_START_CODE.len() || bytes.len() > MAX_CODEC_SPECIFIC_DATA_BYTES {
        return Err(EncodedDisplayError::InvalidDecoderConfig(format!(
            "{label} must contain a bounded Annex-B parameter set"
        )));
    }
    if !bytes.starts_with(&ANNEX_B_START_CODE) {
        return Err(EncodedDisplayError::InvalidDecoderConfig(format!(
            "{label} must start with a four-byte Annex-B start code"
        )));
    }
    if bytes[ANNEX_B_START_CODE.len()] & 0x1f != expected_nal_unit_type {
        return Err(EncodedDisplayError::InvalidDecoderConfig(format!(
            "{label} has the wrong AVC NAL unit type"
        )));
    }
    Ok(())
}
