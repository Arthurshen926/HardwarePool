use thiserror::Error;

use crate::{IddMonitorConnectRequest, IddMonitorIdentity};

pub const SUDOVDA_PROTOCOL_MAJOR: u8 = 0;
pub const SUDOVDA_PROTOCOL_MINOR: u8 = 2;
pub const SUDOVDA_PROTOCOL_INCREMENTAL: u8 = 1;
pub const SUDOVDA_PROTOCOL_TEST_BUILD: bool = true;

pub const SUDOVDA_ADD_REQUEST_BYTES: usize = 56;
pub const SUDOVDA_ADD_RESPONSE_BYTES: usize = 12;
pub const SUDOVDA_REMOVE_REQUEST_BYTES: usize = 16;
pub const SUDOVDA_PROTOCOL_RESPONSE_BYTES: usize = 4;

const FILE_DEVICE_UNKNOWN: u32 = 0x22;
const METHOD_BUFFERED: u32 = 0;
const FILE_ANY_ACCESS: u32 = 0;

pub const IOCTL_SUDOVDA_ADD_VIRTUAL_DISPLAY: u32 = ctl_code(0x800);
pub const IOCTL_SUDOVDA_REMOVE_VIRTUAL_DISPLAY: u32 = ctl_code(0x801);
pub const IOCTL_SUDOVDA_DRIVER_PING: u32 = ctl_code(0x888);
pub const IOCTL_SUDOVDA_GET_PROTOCOL_VERSION: u32 = ctl_code(0x8ff);

const fn ctl_code(function: u32) -> u32 {
    (FILE_DEVICE_UNKNOWN << 16) | (FILE_ANY_ACCESS << 14) | (function << 2) | METHOD_BUFFERED
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SudoVdaProtocolVersion {
    pub major: u8,
    pub minor: u8,
    pub incremental: u8,
    pub test_build: bool,
}

impl SudoVdaProtocolVersion {
    #[must_use]
    pub const fn supported(self) -> bool {
        self.major == SUDOVDA_PROTOCOL_MAJOR
            && self.minor == SUDOVDA_PROTOCOL_MINOR
            && self.incremental == SUDOVDA_PROTOCOL_INCREMENTAL
            && self.test_build == SUDOVDA_PROTOCOL_TEST_BUILD
    }

    pub fn parse(
        bytes: [u8; SUDOVDA_PROTOCOL_RESPONSE_BYTES],
    ) -> Result<Self, SudoVdaControlError> {
        if bytes[3] > 1 {
            return Err(SudoVdaControlError::InvalidProtocolVersion);
        }
        Ok(Self {
            major: bytes[0],
            minor: bytes[1],
            incremental: bytes[2],
            test_build: bytes[3] != 0,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SudoVdaAddRequest {
    pub identity: IddMonitorIdentity,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
}

impl TryFrom<IddMonitorConnectRequest> for SudoVdaAddRequest {
    type Error = SudoVdaControlError;

    fn try_from(request: IddMonitorConnectRequest) -> Result<Self, Self::Error> {
        request
            .preferred_mode
            .validate()
            .map_err(|_| SudoVdaControlError::InvalidMode)?;
        if !request.preferred_mode.refresh_millihz.is_multiple_of(1000) {
            return Err(SudoVdaControlError::FractionalRefreshUnsupported);
        }
        Ok(Self {
            identity: request.identity,
            width: request.preferred_mode.width,
            height: request.preferred_mode.height,
            refresh_hz: request.preferred_mode.refresh_millihz / 1000,
        })
    }
}

impl SudoVdaAddRequest {
    #[must_use]
    pub fn encode(self) -> [u8; SUDOVDA_ADD_REQUEST_BYTES] {
        let mut bytes = [0_u8; SUDOVDA_ADD_REQUEST_BYTES];
        write_u32(&mut bytes, 0, self.width);
        write_u32(&mut bytes, 4, self.height);
        write_u32(&mut bytes, 8, self.refresh_hz);
        bytes[12..28].copy_from_slice(&windows_guid_bytes(self.identity));

        let identity = self.identity.capability_id().as_uuid().simple().to_string();
        write_c_string(&mut bytes[28..42], &format!("CapyIO-{}", &identity[..6]));
        write_c_string(&mut bytes[42..56], &identity[..12]);
        bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SudoVdaAddResponse {
    pub adapter_luid_low: u32,
    pub adapter_luid_high: i32,
    pub target_id: u32,
}

impl SudoVdaAddResponse {
    #[must_use]
    pub fn parse(bytes: [u8; SUDOVDA_ADD_RESPONSE_BYTES]) -> Self {
        Self {
            adapter_luid_low: read_u32(&bytes, 0),
            adapter_luid_high: read_i32(&bytes, 4),
            target_id: read_u32(&bytes, 8),
        }
    }
}

#[must_use]
pub fn encode_sudovda_remove(identity: IddMonitorIdentity) -> [u8; SUDOVDA_REMOVE_REQUEST_BYTES] {
    windows_guid_bytes(identity)
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SudoVdaControlError {
    #[error("SudoVDA display mode is outside the CapyIO bound")]
    InvalidMode,
    #[error("SudoVDA protocol 0.2.1 accepts only integral refresh rates")]
    FractionalRefreshUnsupported,
    #[error("SudoVDA returned a malformed protocol version")]
    InvalidProtocolVersion,
}

fn windows_guid_bytes(identity: IddMonitorIdentity) -> [u8; 16] {
    let uuid = *identity.capability_id().as_uuid().as_bytes();
    let mut bytes = uuid;
    bytes[0..4].reverse();
    bytes[4..6].reverse();
    bytes[6..8].reverse();
    bytes
}

fn write_c_string(target: &mut [u8], value: &str) {
    let count = value.len().min(target.len().saturating_sub(1));
    target[..count].copy_from_slice(&value.as_bytes()[..count]);
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed u32 field"),
    )
}

fn read_i32(bytes: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed i32 field"),
    )
}

#[cfg(test)]
mod tests {
    use capyio_core::ids::CapabilityId;

    use super::*;
    use crate::IddMonitorMode;

    fn identity() -> IddMonitorIdentity {
        IddMonitorIdentity::new(
            "00112233-4455-6677-8899-aabbccddeeff"
                .parse::<CapabilityId>()
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn ioctl_values_match_sudovda_protocol_0_2_1() {
        assert_eq!(IOCTL_SUDOVDA_ADD_VIRTUAL_DISPLAY, 0x222000);
        assert_eq!(IOCTL_SUDOVDA_REMOVE_VIRTUAL_DISPLAY, 0x222004);
        assert_eq!(IOCTL_SUDOVDA_DRIVER_PING, 0x222220);
        assert_eq!(IOCTL_SUDOVDA_GET_PROTOCOL_VERSION, 0x2223fc);
    }

    #[test]
    fn add_request_matches_windows_guid_and_fixed_c_layout() {
        let request = SudoVdaAddRequest::try_from(IddMonitorConnectRequest {
            identity: identity(),
            preferred_mode: IddMonitorMode {
                width: 2640,
                height: 1216,
                refresh_millihz: 120_000,
            },
        })
        .unwrap()
        .encode();
        assert_eq!(request.len(), SUDOVDA_ADD_REQUEST_BYTES);
        assert_eq!(&request[0..4], &2640_u32.to_le_bytes());
        assert_eq!(&request[4..8], &1216_u32.to_le_bytes());
        assert_eq!(&request[8..12], &120_u32.to_le_bytes());
        assert_eq!(
            &request[12..28],
            &[
                0x33, 0x22, 0x11, 0x00, 0x55, 0x44, 0x77, 0x66, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
                0xee, 0xff
            ]
        );
        assert_eq!(&request[28..42], b"CapyIO-001122\0");
        assert_eq!(&request[42..56], b"001122334455\0\0");
    }

    #[test]
    fn remove_contains_only_the_exact_capability_guid() {
        assert_eq!(
            encode_sudovda_remove(identity()),
            [
                0x33, 0x22, 0x11, 0x00, 0x55, 0x44, 0x77, 0x66, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
                0xee, 0xff
            ]
        );
    }

    #[test]
    fn fractional_refresh_fails_before_driver_io() {
        let error = SudoVdaAddRequest::try_from(IddMonitorConnectRequest {
            identity: identity(),
            preferred_mode: IddMonitorMode {
                width: 1920,
                height: 1080,
                refresh_millihz: 59_940,
            },
        });
        assert_eq!(
            error,
            Err(SudoVdaControlError::FractionalRefreshUnsupported)
        );
    }

    #[test]
    fn protocol_version_is_exact_and_boolean_is_bounded() {
        let supported = SudoVdaProtocolVersion::parse([0, 2, 1, 1]).unwrap();
        assert!(supported.supported());
        assert!(
            !SudoVdaProtocolVersion::parse([0, 2, 0, 1])
                .unwrap()
                .supported()
        );
        assert_eq!(
            SudoVdaProtocolVersion::parse([0, 2, 1, 2]),
            Err(SudoVdaControlError::InvalidProtocolVersion)
        );
    }
}
