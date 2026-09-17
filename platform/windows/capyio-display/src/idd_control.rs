use thiserror::Error;

use crate::{
    IDD_MAX_MONITORS, IddMonitorIdentity,
    idd_handoff::{IDD_MAX_HEIGHT, IDD_MAX_WIDTH},
};

pub const IDD_CONTROL_MAGIC: u32 = 0x4344_5043;
pub const IDD_CONTROL_ABI_VERSION: u16 = 1;
pub const IDD_CONTROL_MAX_MONITORS: u32 = IDD_MAX_MONITORS;
pub const IDD_CONTROL_MAX_REFRESH_MILLIHZ: u32 = 240_000;
pub const IDD_CONNECT_REQUEST_BYTES: usize = 48;
pub const IDD_DISCONNECT_REQUEST_BYTES: usize = 32;
pub const IDD_MONITOR_INVENTORY_RESPONSE_BYTES: usize = 272;

const FILE_DEVICE_UNKNOWN: u32 = 0x22;
const METHOD_BUFFERED: u32 = 0;
const FILE_READ_WRITE_ACCESS: u32 = 3;

pub const IOCTL_CAPYIO_QUERY_ADAPTER: u32 = ctl_code(0x900);
pub const IOCTL_CAPYIO_CONNECT_MONITOR: u32 = ctl_code(0x901);
pub const IOCTL_CAPYIO_DISCONNECT_MONITOR: u32 = ctl_code(0x902);
pub const IOCTL_CAPYIO_ENUMERATE_MONITORS: u32 = ctl_code(0x903);

const fn ctl_code(function: u32) -> u32 {
    (FILE_DEVICE_UNKNOWN << 16) | (FILE_READ_WRITE_ACCESS << 14) | (function << 2) | METHOD_BUFFERED
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IddMonitorMode {
    pub width: u32,
    pub height: u32,
    pub refresh_millihz: u32,
}

impl IddMonitorMode {
    pub fn validate(self) -> Result<(), IddControlError> {
        if self.width == 0
            || self.height == 0
            || self.width > IDD_MAX_WIDTH
            || self.height > IDD_MAX_HEIGHT
            || self.refresh_millihz == 0
            || self.refresh_millihz > IDD_CONTROL_MAX_REFRESH_MILLIHZ
        {
            return Err(IddControlError::InvalidMode);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IddMonitorConnectRequest {
    pub identity: IddMonitorIdentity,
    pub preferred_mode: IddMonitorMode,
}

impl IddMonitorConnectRequest {
    pub fn encode(self) -> Result<[u8; IDD_CONNECT_REQUEST_BYTES], IddControlError> {
        self.preferred_mode.validate()?;
        let mut bytes = [0_u8; IDD_CONNECT_REQUEST_BYTES];
        write_u32(&mut bytes, 0, IDD_CONTROL_MAGIC);
        write_u16(&mut bytes, 4, IDD_CONTROL_ABI_VERSION);
        write_u16(&mut bytes, 6, IDD_CONNECT_REQUEST_BYTES as u16);
        bytes[8..24].copy_from_slice(self.identity.capability_id().as_uuid().as_bytes());
        write_u32(&mut bytes, 24, self.preferred_mode.width);
        write_u32(&mut bytes, 28, self.preferred_mode.height);
        write_u32(&mut bytes, 32, self.preferred_mode.refresh_millihz);
        Ok(bytes)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IddMonitorDisconnectRequest {
    pub identity: IddMonitorIdentity,
}

impl IddMonitorDisconnectRequest {
    #[must_use]
    pub fn encode(self) -> [u8; IDD_DISCONNECT_REQUEST_BYTES] {
        let mut bytes = [0_u8; IDD_DISCONNECT_REQUEST_BYTES];
        write_u32(&mut bytes, 0, IDD_CONTROL_MAGIC);
        write_u16(&mut bytes, 4, IDD_CONTROL_ABI_VERSION);
        write_u16(&mut bytes, 6, IDD_DISCONNECT_REQUEST_BYTES as u16);
        bytes[8..24].copy_from_slice(self.identity.capability_id().as_uuid().as_bytes());
        bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum IddControlError {
    #[error("IDD monitor mode exceeds the bounded control contract")]
    InvalidMode,
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_core::ids::CapabilityId;

    fn identity() -> IddMonitorIdentity {
        IddMonitorIdentity::new(
            "00112233-4455-6677-8899-aabbccddeeff"
                .parse::<CapabilityId>()
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn ioctl_values_match_the_checked_cpp_contract() {
        assert_eq!(IOCTL_CAPYIO_QUERY_ADAPTER, 0x22e400);
        assert_eq!(IOCTL_CAPYIO_CONNECT_MONITOR, 0x22e404);
        assert_eq!(IOCTL_CAPYIO_DISCONNECT_MONITOR, 0x22e408);
        assert_eq!(IOCTL_CAPYIO_ENUMERATE_MONITORS, 0x22e40c);
        assert_eq!(IDD_CONTROL_MAX_MONITORS, 8);
    }

    #[test]
    fn checked_cpp_header_retains_the_same_closed_contract() {
        let header = include_str!("../../../../drivers/windows-display/idd/CapyIoDisplayControl.h");
        for expected in [
            "Magic = 0x43445043",
            "AbiVersion = 1",
            "MaxMonitors = 8",
            "0x900, METHOD_BUFFERED, FILE_READ_DATA | FILE_WRITE_DATA",
            "0x901, METHOD_BUFFERED, FILE_READ_DATA | FILE_WRITE_DATA",
            "0x902, METHOD_BUFFERED, FILE_READ_DATA | FILE_WRITE_DATA",
            "sizeof(MonitorConnectRequest) == 48",
            "sizeof(MonitorDisconnectRequest) == 32",
            "sizeof(AdapterQueryResponse) == 32",
            "sizeof(MonitorConnectResponse) == 48",
            "sizeof(ConnectedMonitorEntry) == 32",
            "sizeof(MonitorInventoryResponse) == 272",
        ] {
            assert!(
                header.contains(expected),
                "missing C++ ABI token: {expected}"
            );
        }
    }

    #[test]
    fn driver_source_keeps_dynamic_lifecycle_and_identity_handoff_connected() {
        let driver = include_str!("../../../../drivers/windows-display/idd/Driver.cpp");
        let producer =
            include_str!("../../../../drivers/windows-display/idd/CapyIoDisplayProducer.cpp");
        let inf = include_str!("../../../../drivers/windows-display/idd/IddSampleDriver.inf");
        for expected in [
            "IddConfig.EvtIddCxDeviceIoControl = IddSampleIoDeviceControl",
            "AdapterCaps.MaxMonitorsSupported = CapyIoDisplayControl::MaxMonitors",
            "IddCxMonitorArrival(MonitorCreateOut.MonitorObject, &ArrivalOut)",
            "IddCxMonitorDeparture(slot.Monitor)",
            "STATUS_OBJECT_NAME_COLLISION",
            "case CapyIoDisplayControl::IoctlEnumerateMonitors:",
            "pContext->EnumerateMonitors(*response)",
            "FrameProducer FrameProducer(m_MonitorIdentity)",
        ] {
            assert!(
                driver.contains(expected),
                "missing dynamic driver invariant: {expected}"
            );
        }
        assert!(
            producer
                .contains("OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, FALSE, mappingName_)")
        );
        assert!(inf.contains("HKR,,Security,,\"D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;LS)\""));
    }

    #[test]
    fn connect_request_has_exact_closed_layout() {
        let bytes = IddMonitorConnectRequest {
            identity: identity(),
            preferred_mode: IddMonitorMode {
                width: 1980,
                height: 912,
                refresh_millihz: 60_000,
            },
        }
        .encode()
        .unwrap();

        assert_eq!(&bytes[0..4], &IDD_CONTROL_MAGIC.to_le_bytes());
        assert_eq!(&bytes[4..6], &IDD_CONTROL_ABI_VERSION.to_le_bytes());
        assert_eq!(&bytes[6..8], &(48_u16).to_le_bytes());
        assert_eq!(
            &bytes[8..24],
            &[
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
                0xee, 0xff,
            ]
        );
        assert_eq!(&bytes[24..28], &1980_u32.to_le_bytes());
        assert_eq!(&bytes[28..32], &912_u32.to_le_bytes());
        assert_eq!(&bytes[32..36], &60_000_u32.to_le_bytes());
        assert!(bytes[36..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn invalid_modes_fail_before_an_ioctl_buffer_exists() {
        for mode in [
            IddMonitorMode {
                width: 0,
                height: 912,
                refresh_millihz: 60_000,
            },
            IddMonitorMode {
                width: 1980,
                height: IDD_MAX_HEIGHT + 1,
                refresh_millihz: 60_000,
            },
            IddMonitorMode {
                width: 1980,
                height: 912,
                refresh_millihz: IDD_CONTROL_MAX_REFRESH_MILLIHZ + 1,
            },
        ] {
            assert_eq!(
                IddMonitorConnectRequest {
                    identity: identity(),
                    preferred_mode: mode,
                }
                .encode(),
                Err(IddControlError::InvalidMode)
            );
        }
    }

    #[test]
    fn disconnect_request_contains_only_identity_and_zeroed_flags() {
        let bytes = IddMonitorDisconnectRequest {
            identity: identity(),
        }
        .encode();
        assert_eq!(&bytes[0..4], &IDD_CONTROL_MAGIC.to_le_bytes());
        assert_eq!(&bytes[6..8], &(32_u16).to_le_bytes());
        assert!(bytes[24..].iter().all(|byte| *byte == 0));
    }
}
