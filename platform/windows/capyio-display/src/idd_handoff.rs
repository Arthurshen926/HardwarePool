use thiserror::Error;

use capyio_core::ids::CapabilityId;

pub const IDD_HANDOFF_MAGIC: u32 = 0x4459_5043;
pub const IDD_HANDOFF_ABI_VERSION: u16 = 1;
pub const IDD_HANDOFF_HEADER_BYTES: usize = 64;
pub const IDD_PIXEL_FORMAT_BGRA8: u32 = 1;
pub const IDD_MAX_WIDTH: u32 = 4096;
pub const IDD_MAX_HEIGHT: u32 = 2160;
pub const IDD_MAX_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;
pub const IDD_MAX_MONITORS: u32 = 8;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IddMonitorIdentity(CapabilityId);

impl IddMonitorIdentity {
    pub fn new(capability_id: CapabilityId) -> Result<Self, IddHandoffError> {
        if capability_id.as_uuid().is_nil() {
            return Err(IddHandoffError::InvalidMonitorIdentity);
        }
        Ok(Self(capability_id))
    }

    pub fn from_capability_bytes(value: [u8; 16]) -> Result<Self, IddHandoffError> {
        Self::new(CapabilityId::from_uuid(uuid::Uuid::from_bytes(value)))
    }

    #[must_use]
    pub const fn capability_id(self) -> CapabilityId {
        self.0
    }

    #[must_use]
    pub fn mapping_name(self) -> String {
        format!(
            r"Global\CapyIO.DisplayFrame.v2.{}",
            self.0.as_uuid().simple()
        )
    }

    #[cfg(all(windows, test))]
    pub(crate) fn test_mapping_name(self) -> String {
        format!(
            r"Local\CapyIO.DisplayFrame.v2.{}.test",
            self.0.as_uuid().simple()
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IddFrameHeader {
    pub generation: u64,
    pub sequence_lock: u64,
    pub width: u32,
    pub height: u32,
    pub stride_bytes: u32,
    pub pixel_format: u32,
    pub payload_bytes: u64,
    pub source_timestamp_nanos: u64,
    pub flags: u32,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum IddHandoffError {
    #[error("IDD monitor identity must be a non-nil Capability ID")]
    InvalidMonitorIdentity,
    #[error("IDD handoff mapping is shorter than the fixed header")]
    TruncatedHeader,
    #[error("IDD handoff magic does not identify CapyIO")]
    InvalidMagic,
    #[error("unsupported IDD handoff ABI version {0}")]
    UnsupportedVersion(u16),
    #[error("IDD handoff header size is invalid")]
    InvalidHeaderSize,
    #[error("IDD handoff snapshot is being written")]
    SnapshotInProgress,
    #[error("IDD handoff generation must be positive")]
    InvalidGeneration,
    #[error("IDD handoff dimensions exceed the bounded contract")]
    InvalidDimensions,
    #[error("IDD handoff only accepts packed BGRA8")]
    UnsupportedPixelFormat,
    #[error("IDD handoff stride or payload length is inconsistent")]
    InvalidLayout,
    #[error("IDD handoff payload exceeds mapping capacity")]
    CapacityExceeded,
    #[cfg(windows)]
    #[error("the CapyIO display mapping already belongs to another Broker")]
    AlreadyOwned,
    #[cfg(windows)]
    #[error("Windows {operation} failed with error {code}")]
    Windows { operation: &'static str, code: u32 },
}

impl IddFrameHeader {
    pub fn parse(mapping: &[u8]) -> Result<Self, IddHandoffError> {
        if mapping.len() < IDD_HANDOFF_HEADER_BYTES {
            return Err(IddHandoffError::TruncatedHeader);
        }
        if read_u32(mapping, 0) != IDD_HANDOFF_MAGIC {
            return Err(IddHandoffError::InvalidMagic);
        }
        let version = read_u16(mapping, 4);
        if version != IDD_HANDOFF_ABI_VERSION {
            return Err(IddHandoffError::UnsupportedVersion(version));
        }
        if usize::from(read_u16(mapping, 6)) != IDD_HANDOFF_HEADER_BYTES {
            return Err(IddHandoffError::InvalidHeaderSize);
        }

        let header = Self {
            generation: read_u64(mapping, 8),
            sequence_lock: read_u64(mapping, 16),
            width: read_u32(mapping, 24),
            height: read_u32(mapping, 28),
            stride_bytes: read_u32(mapping, 32),
            pixel_format: read_u32(mapping, 36),
            payload_bytes: read_u64(mapping, 40),
            source_timestamp_nanos: read_u64(mapping, 48),
            flags: read_u32(mapping, 56),
        };
        header.validate(mapping.len())?;
        Ok(header)
    }

    pub fn payload_range(&self) -> std::ops::Range<usize> {
        let payload_bytes = usize::try_from(self.payload_bytes)
            .expect("validated payload length always fits usize");
        IDD_HANDOFF_HEADER_BYTES..IDD_HANDOFF_HEADER_BYTES + payload_bytes
    }

    pub(crate) fn validate(&self, mapping_bytes: usize) -> Result<(), IddHandoffError> {
        if !self.sequence_lock.is_multiple_of(2) {
            return Err(IddHandoffError::SnapshotInProgress);
        }
        if self.generation == 0 {
            return Err(IddHandoffError::InvalidGeneration);
        }
        if self.width == 0
            || self.height == 0
            || self.width > IDD_MAX_WIDTH
            || self.height > IDD_MAX_HEIGHT
        {
            return Err(IddHandoffError::InvalidDimensions);
        }
        if self.pixel_format != IDD_PIXEL_FORMAT_BGRA8 {
            return Err(IddHandoffError::UnsupportedPixelFormat);
        }
        let minimum_stride = self
            .width
            .checked_mul(4)
            .ok_or(IddHandoffError::InvalidLayout)?;
        let expected = u64::from(self.stride_bytes)
            .checked_mul(u64::from(self.height))
            .ok_or(IddHandoffError::InvalidLayout)?;
        if self.stride_bytes < minimum_stride
            || self.payload_bytes != expected
            || self.payload_bytes > IDD_MAX_PAYLOAD_BYTES
        {
            return Err(IddHandoffError::InvalidLayout);
        }
        let payload_bytes =
            usize::try_from(self.payload_bytes).map_err(|_| IddHandoffError::CapacityExceeded)?;
        if IDD_HANDOFF_HEADER_BYTES
            .checked_add(payload_bytes)
            .is_none_or(|required| required > mapping_bytes)
        {
            return Err(IddHandoffError::CapacityExceeded);
        }
        Ok(())
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("bounded header"),
    )
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("bounded header"),
    )
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("bounded header"),
    )
}

#[cfg(test)]
mod monitor_identity_tests {
    use super::*;

    #[test]
    fn monitor_mapping_name_is_stable_and_capability_scoped() {
        let first: CapabilityId = "00112233-4455-6677-8899-aabbccddeeff".parse().unwrap();
        let second: CapabilityId = "10112233-4455-6677-8899-aabbccddeeff".parse().unwrap();
        let first = IddMonitorIdentity::new(first).unwrap();
        let second = IddMonitorIdentity::new(second).unwrap();

        assert_eq!(
            first.mapping_name(),
            r"Global\CapyIO.DisplayFrame.v2.00112233445566778899aabbccddeeff"
        );
        assert_ne!(first.mapping_name(), second.mapping_name());
        assert_eq!(IDD_MAX_MONITORS, 8);
    }

    #[test]
    fn nil_capability_cannot_name_a_monitor() {
        let nil: CapabilityId = "00000000-0000-0000-0000-000000000000".parse().unwrap();
        assert_eq!(
            IddMonitorIdentity::new(nil),
            Err(IddHandoffError::InvalidMonitorIdentity)
        );
    }
}
