use std::{collections::HashSet, ffi::c_void, ptr};

use capyio_core::ids::CapabilityId;
use thiserror::Error;
use uuid::Uuid;
use windows_sys::{
    Win32::{
        Devices::DeviceAndDriverInstallation::{
            CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_Get_Device_Interface_List_SizeW,
            CM_Get_Device_Interface_ListW, CR_BUFFER_SMALL, CR_SUCCESS,
        },
        Foundation::{
            CloseHandle, GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
        },
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::IO::DeviceIoControl,
    },
    core::GUID,
};

use crate::{
    IDD_CONTROL_ABI_VERSION, IDD_CONTROL_MAGIC, IDD_CONTROL_MAX_MONITORS,
    IDD_MONITOR_INVENTORY_RESPONSE_BYTES, IOCTL_CAPYIO_CONNECT_MONITOR,
    IOCTL_CAPYIO_DISCONNECT_MONITOR, IOCTL_CAPYIO_ENUMERATE_MONITORS, IOCTL_CAPYIO_QUERY_ADAPTER,
    IddControlError, IddMonitorConnectRequest, IddMonitorDisconnectRequest, IddMonitorIdentity,
};

const IDD_DEVICE_INTERFACE: GUID = GUID::from_u128(0x4f7bc2f1_940b_4d82_81d8_7e83bfa5e65c);
const ADAPTER_QUERY_RESPONSE_BYTES: usize = 32;
const MONITOR_CONNECT_RESPONSE_BYTES: usize = 48;
const MAX_INTERFACE_LIST_CHARS: u32 = 32_768;
const ENUMERATION_ATTEMPTS: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IddAdapterStatus {
    pub maximum_monitors: u32,
    pub connected_monitors: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IddConnectedMonitor {
    pub identity: IddMonitorIdentity,
    pub connector_index: u32,
    pub target_id: u32,
    pub adapter_luid_low: u32,
    pub adapter_luid_high: i32,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum IddDeviceControlError {
    #[error("no present CapyIO display-control device interface was found")]
    NotFound,
    #[error("more than one present CapyIO display-control interface was found")]
    AmbiguousInterface,
    #[error("CapyIO display-control interface enumeration failed with CONFIGRET {code}")]
    Enumeration { code: u32 },
    #[error("CapyIO display-control interface list is malformed or exceeds its bound")]
    InvalidInterfaceList,
    #[error("opening the CapyIO display-control interface failed with Win32 error {code}")]
    Open { code: u32 },
    #[error("the {operation} display-control IOCTL failed with Win32 error {code}")]
    Ioctl { operation: &'static str, code: u32 },
    #[error("the {operation} display-control response had an unexpected length")]
    UnexpectedResponseLength { operation: &'static str },
    #[error("the {operation} display-control response violated the closed ABI")]
    InvalidResponse { operation: &'static str },
    #[error("the connect response identity did not match the request")]
    IdentityMismatch,
    #[error(transparent)]
    InvalidRequest(#[from] IddControlError),
}

trait IddIoctlTransport {
    fn call(
        &mut self,
        operation: &'static str,
        code: u32,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<usize, IddDeviceControlError>;
}

struct Win32IoctlTransport {
    handle: HANDLE,
}

impl Drop for Win32IoctlTransport {
    fn drop(&mut self) {
        // SAFETY: `handle` is owned by this value and is created only after
        // CreateFileW returned a valid handle. Drop runs exactly once.
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

impl IddIoctlTransport for Win32IoctlTransport {
    fn call(
        &mut self,
        operation: &'static str,
        code: u32,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<usize, IddDeviceControlError> {
        let input_ptr = if input.is_empty() {
            ptr::null()
        } else {
            input.as_ptr().cast::<c_void>()
        };
        let output_ptr = if output.is_empty() {
            ptr::null_mut()
        } else {
            output.as_mut_ptr().cast::<c_void>()
        };
        let mut returned = 0_u32;
        // SAFETY: the handle is valid for the lifetime of this transport. The
        // optional buffers remain alive and are exactly bounded by their lengths;
        // this synchronous call does not retain their pointers.
        let succeeded = unsafe {
            DeviceIoControl(
                self.handle,
                code,
                input_ptr,
                input.len() as u32,
                output_ptr,
                output.len() as u32,
                &mut returned,
                ptr::null_mut(),
            )
        };
        if succeeded == 0 {
            // SAFETY: GetLastError has no preconditions and is read immediately
            // after the failing Win32 call.
            let code = unsafe { GetLastError() };
            return Err(IddDeviceControlError::Ioctl { operation, code });
        }
        Ok(returned as usize)
    }
}

pub struct IddDeviceController {
    transport: Win32IoctlTransport,
}

impl IddDeviceController {
    pub fn open_unique() -> Result<Self, IddDeviceControlError> {
        let path = enumerate_unique_interface()?;
        // SAFETY: `path` is a validated, NUL-terminated device-interface path.
        // The returned handle is checked before ownership is established.
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            // SAFETY: GetLastError has no preconditions and is read immediately
            // after the failing Win32 call.
            let code = unsafe { GetLastError() };
            return Err(IddDeviceControlError::Open { code });
        }
        Ok(Self {
            transport: Win32IoctlTransport { handle },
        })
    }

    pub fn query(&mut self) -> Result<IddAdapterStatus, IddDeviceControlError> {
        query_with(&mut self.transport)
    }

    pub fn enumerate_monitors(
        &mut self,
    ) -> Result<Vec<IddConnectedMonitor>, IddDeviceControlError> {
        enumerate_with(&mut self.transport)
    }

    pub fn connect(
        &mut self,
        request: IddMonitorConnectRequest,
    ) -> Result<IddConnectedMonitor, IddDeviceControlError> {
        connect_with(&mut self.transport, request)
    }

    pub fn disconnect(
        &mut self,
        request: IddMonitorDisconnectRequest,
    ) -> Result<(), IddDeviceControlError> {
        disconnect_with(&mut self.transport, request)
    }
}

fn enumerate_unique_interface() -> Result<Vec<u16>, IddDeviceControlError> {
    for _ in 0..ENUMERATION_ATTEMPTS {
        let mut required_chars = 0_u32;
        // SAFETY: all pointers reference valid storage or are explicitly null;
        // the class GUID is a process-lifetime constant.
        let result = unsafe {
            CM_Get_Device_Interface_List_SizeW(
                &mut required_chars,
                &IDD_DEVICE_INTERFACE,
                ptr::null(),
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if result != CR_SUCCESS {
            return Err(IddDeviceControlError::Enumeration { code: result });
        }
        if required_chars == 0 || required_chars > MAX_INTERFACE_LIST_CHARS {
            return Err(IddDeviceControlError::InvalidInterfaceList);
        }
        let mut list = vec![0_u16; required_chars as usize];
        // SAFETY: `list` provides exactly `required_chars` writable UTF-16
        // elements, and other pointers have the same validity as above.
        let result = unsafe {
            CM_Get_Device_Interface_ListW(
                &IDD_DEVICE_INTERFACE,
                ptr::null(),
                list.as_mut_ptr(),
                required_chars,
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if result == CR_BUFFER_SMALL {
            continue;
        }
        if result != CR_SUCCESS {
            return Err(IddDeviceControlError::Enumeration { code: result });
        }
        return select_unique_multi_sz(&list);
    }
    Err(IddDeviceControlError::Enumeration {
        code: CR_BUFFER_SMALL,
    })
}

fn select_unique_multi_sz(list: &[u16]) -> Result<Vec<u16>, IddDeviceControlError> {
    if list.is_empty() || list.last() != Some(&0) {
        return Err(IddDeviceControlError::InvalidInterfaceList);
    }
    if list.len() == 1 {
        return Err(IddDeviceControlError::NotFound);
    }
    if list[list.len() - 2] != 0 {
        return Err(IddDeviceControlError::InvalidInterfaceList);
    }
    let mut entries = list
        .split(|character| *character == 0)
        .filter(|entry| !entry.is_empty());
    let Some(path) = entries.next() else {
        return Err(IddDeviceControlError::NotFound);
    };
    if entries.next().is_some() {
        return Err(IddDeviceControlError::AmbiguousInterface);
    }
    let mut terminated = Vec::with_capacity(path.len() + 1);
    terminated.extend_from_slice(path);
    terminated.push(0);
    Ok(terminated)
}

fn query_with<T: IddIoctlTransport>(
    transport: &mut T,
) -> Result<IddAdapterStatus, IddDeviceControlError> {
    let mut output = [0_u8; ADAPTER_QUERY_RESPONSE_BYTES];
    let returned = transport.call("query", IOCTL_CAPYIO_QUERY_ADAPTER, &[], &mut output)?;
    require_exact_length("query", returned, output.len())?;
    require_header("query", &output, ADAPTER_QUERY_RESPONSE_BYTES)?;
    let maximum_monitors = read_u32(&output, 8);
    let connected_monitors = read_u32(&output, 12);
    if maximum_monitors != IDD_CONTROL_MAX_MONITORS
        || connected_monitors > maximum_monitors
        || output[16..].iter().any(|byte| *byte != 0)
    {
        return Err(IddDeviceControlError::InvalidResponse { operation: "query" });
    }
    Ok(IddAdapterStatus {
        maximum_monitors,
        connected_monitors,
    })
}

fn connect_with<T: IddIoctlTransport>(
    transport: &mut T,
    request: IddMonitorConnectRequest,
) -> Result<IddConnectedMonitor, IddDeviceControlError> {
    let input = request.encode()?;
    let mut output = [0_u8; MONITOR_CONNECT_RESPONSE_BYTES];
    let returned = transport.call("connect", IOCTL_CAPYIO_CONNECT_MONITOR, &input, &mut output)?;
    require_exact_length("connect", returned, output.len())?;
    require_header("connect", &output, MONITOR_CONNECT_RESPONSE_BYTES)?;
    if output[8..24] != input[8..24] {
        return Err(IddDeviceControlError::IdentityMismatch);
    }
    let connector_index = read_u32(&output, 24);
    if connector_index >= IDD_CONTROL_MAX_MONITORS || output[40..].iter().any(|byte| *byte != 0) {
        return Err(IddDeviceControlError::InvalidResponse {
            operation: "connect",
        });
    }
    Ok(IddConnectedMonitor {
        identity: request.identity,
        connector_index,
        target_id: read_u32(&output, 28),
        adapter_luid_low: read_u32(&output, 32),
        adapter_luid_high: read_i32(&output, 36),
    })
}

fn enumerate_with<T: IddIoctlTransport>(
    transport: &mut T,
) -> Result<Vec<IddConnectedMonitor>, IddDeviceControlError> {
    let mut output = [0_u8; IDD_MONITOR_INVENTORY_RESPONSE_BYTES];
    let returned = transport.call(
        "enumerate",
        IOCTL_CAPYIO_ENUMERATE_MONITORS,
        &[],
        &mut output,
    )?;
    require_exact_length("enumerate", returned, output.len())?;
    require_header("enumerate", &output, output.len())?;
    let expected_count = read_u32(&output, 8);
    if expected_count > IDD_CONTROL_MAX_MONITORS || read_u32(&output, 12) != 0 {
        return Err(IddDeviceControlError::InvalidResponse {
            operation: "enumerate",
        });
    }

    let mut monitors = Vec::with_capacity(expected_count as usize);
    let mut identities = HashSet::with_capacity(expected_count as usize);
    let mut adapter_luid = None;
    for connector_index in 0..IDD_CONTROL_MAX_MONITORS {
        let offset = 16 + connector_index as usize * 32;
        let entry = &output[offset..offset + 32];
        if entry[0..16].iter().all(|byte| *byte == 0) {
            if entry[16..].iter().any(|byte| *byte != 0) {
                return Err(IddDeviceControlError::InvalidResponse {
                    operation: "enumerate",
                });
            }
            continue;
        }
        let identity_bytes: [u8; 16] = entry[0..16].try_into().expect("fixed identity field");
        let identity =
            IddMonitorIdentity::new(CapabilityId::from_uuid(Uuid::from_bytes(identity_bytes)))
                .map_err(|_| IddDeviceControlError::InvalidResponse {
                    operation: "enumerate",
                })?;
        let reported_connector = read_u32(entry, 16);
        let current_luid = (read_u32(entry, 24), read_i32(entry, 28));
        if reported_connector != connector_index
            || !identities.insert(identity)
            || adapter_luid.is_some_and(|known| known != current_luid)
        {
            return Err(IddDeviceControlError::InvalidResponse {
                operation: "enumerate",
            });
        }
        adapter_luid = Some(current_luid);
        monitors.push(IddConnectedMonitor {
            identity,
            connector_index,
            target_id: read_u32(entry, 20),
            adapter_luid_low: current_luid.0,
            adapter_luid_high: current_luid.1,
        });
    }
    if monitors.len() != expected_count as usize {
        return Err(IddDeviceControlError::InvalidResponse {
            operation: "enumerate",
        });
    }
    Ok(monitors)
}

fn disconnect_with<T: IddIoctlTransport>(
    transport: &mut T,
    request: IddMonitorDisconnectRequest,
) -> Result<(), IddDeviceControlError> {
    let input = request.encode();
    let returned = transport.call(
        "disconnect",
        IOCTL_CAPYIO_DISCONNECT_MONITOR,
        &input,
        &mut [],
    )?;
    require_exact_length("disconnect", returned, 0)
}

fn require_header(
    operation: &'static str,
    response: &[u8],
    expected_size: usize,
) -> Result<(), IddDeviceControlError> {
    if read_u32(response, 0) != IDD_CONTROL_MAGIC
        || read_u16(response, 4) != IDD_CONTROL_ABI_VERSION
        || read_u16(response, 6) as usize != expected_size
    {
        return Err(IddDeviceControlError::InvalidResponse { operation });
    }
    Ok(())
}

fn require_exact_length(
    operation: &'static str,
    returned: usize,
    expected: usize,
) -> Result<(), IddDeviceControlError> {
    if returned != expected {
        return Err(IddDeviceControlError::UnexpectedResponseLength { operation });
    }
    Ok(())
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("fixed ABI field"),
    )
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed ABI field"),
    )
}

fn read_i32(bytes: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed ABI field"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IDD_CONNECT_REQUEST_BYTES, IDD_DISCONNECT_REQUEST_BYTES, IddMonitorMode};
    use capyio_core::ids::CapabilityId;

    #[derive(Default)]
    struct FakeTransport {
        expected_code: u32,
        expected_input: Vec<u8>,
        response: Vec<u8>,
        returned: usize,
        calls: usize,
    }

    impl IddIoctlTransport for FakeTransport {
        fn call(
            &mut self,
            _operation: &'static str,
            code: u32,
            input: &[u8],
            output: &mut [u8],
        ) -> Result<usize, IddDeviceControlError> {
            self.calls += 1;
            assert_eq!(code, self.expected_code);
            assert_eq!(input, self.expected_input);
            assert!(self.response.len() <= output.len());
            output[..self.response.len()].copy_from_slice(&self.response);
            Ok(self.returned)
        }
    }

    fn identity(text: &str) -> IddMonitorIdentity {
        IddMonitorIdentity::new(text.parse::<CapabilityId>().unwrap()).unwrap()
    }

    fn connect_request() -> IddMonitorConnectRequest {
        IddMonitorConnectRequest {
            identity: identity("00112233-4455-6677-8899-aabbccddeeff"),
            preferred_mode: IddMonitorMode {
                width: 1980,
                height: 912,
                refresh_millihz: 60_000,
            },
        }
    }

    fn header(size: usize) -> Vec<u8> {
        let mut bytes = vec![0_u8; size];
        bytes[0..4].copy_from_slice(&IDD_CONTROL_MAGIC.to_le_bytes());
        bytes[4..6].copy_from_slice(&IDD_CONTROL_ABI_VERSION.to_le_bytes());
        bytes[6..8].copy_from_slice(&(size as u16).to_le_bytes());
        bytes
    }

    #[test]
    fn device_interface_guid_matches_the_cpp_contract() {
        assert_eq!(IDD_DEVICE_INTERFACE.data1, 0x4f7bc2f1);
        let header = include_str!("../../../../drivers/windows-display/idd/CapyIoDisplayControl.h");
        assert!(header.contains("4f7bc2f1, 0x940b, 0x4d82"));
        assert!(header.contains("0x81, 0xd8, 0x7e, 0x83, 0xbf, 0xa5, 0xe6, 0x5c"));
    }

    #[test]
    fn multi_sz_selection_fails_closed() {
        assert_eq!(
            select_unique_multi_sz(&[0]),
            Err(IddDeviceControlError::NotFound)
        );
        assert_eq!(
            select_unique_multi_sz(&[0, 0]),
            Err(IddDeviceControlError::NotFound)
        );
        assert_eq!(
            select_unique_multi_sz(&[b'a' as u16, 0, b'b' as u16, 0, 0]),
            Err(IddDeviceControlError::AmbiguousInterface)
        );
        assert_eq!(
            select_unique_multi_sz(&[b'a' as u16, 0]),
            Err(IddDeviceControlError::InvalidInterfaceList)
        );
        assert_eq!(
            select_unique_multi_sz(&[b'a' as u16, 0, 0]).unwrap(),
            vec![b'a' as u16, 0]
        );
    }

    #[test]
    fn query_accepts_only_the_exact_bounded_response() {
        let mut response = header(ADAPTER_QUERY_RESPONSE_BYTES);
        response[8..12].copy_from_slice(&IDD_CONTROL_MAX_MONITORS.to_le_bytes());
        response[12..16].copy_from_slice(&2_u32.to_le_bytes());
        let mut transport = FakeTransport {
            expected_code: IOCTL_CAPYIO_QUERY_ADAPTER,
            response,
            returned: ADAPTER_QUERY_RESPONSE_BYTES,
            ..Default::default()
        };
        assert_eq!(
            query_with(&mut transport).unwrap(),
            IddAdapterStatus {
                maximum_monitors: 8,
                connected_monitors: 2,
            }
        );
        assert_eq!(transport.calls, 1);
    }

    #[test]
    fn query_rejects_invalid_counts_reserved_bytes_and_lengths() {
        for mutate in [8_usize, 12, 16] {
            let mut response = header(ADAPTER_QUERY_RESPONSE_BYTES);
            response[8..12].copy_from_slice(&IDD_CONTROL_MAX_MONITORS.to_le_bytes());
            response[12..16].copy_from_slice(&1_u32.to_le_bytes());
            response[mutate] = if mutate == 8 {
                7
            } else if mutate == 12 {
                9
            } else {
                1
            };
            let mut transport = FakeTransport {
                expected_code: IOCTL_CAPYIO_QUERY_ADAPTER,
                response,
                returned: ADAPTER_QUERY_RESPONSE_BYTES,
                ..Default::default()
            };
            assert_eq!(
                query_with(&mut transport),
                Err(IddDeviceControlError::InvalidResponse { operation: "query" })
            );
        }
        for returned in [
            ADAPTER_QUERY_RESPONSE_BYTES - 1,
            ADAPTER_QUERY_RESPONSE_BYTES + 1,
        ] {
            let mut transport = FakeTransport {
                expected_code: IOCTL_CAPYIO_QUERY_ADAPTER,
                response: header(ADAPTER_QUERY_RESPONSE_BYTES),
                returned,
                ..Default::default()
            };
            assert_eq!(
                query_with(&mut transport),
                Err(IddDeviceControlError::UnexpectedResponseLength { operation: "query" })
            );
        }
    }

    #[test]
    fn connect_sends_exact_request_and_parses_exact_response() {
        let request = connect_request();
        let input = request.encode().unwrap();
        let mut response = header(MONITOR_CONNECT_RESPONSE_BYTES);
        response[8..24].copy_from_slice(&input[8..24]);
        response[24..28].copy_from_slice(&3_u32.to_le_bytes());
        response[28..32].copy_from_slice(&17_u32.to_le_bytes());
        response[32..36].copy_from_slice(&0x89ab_cdef_u32.to_le_bytes());
        response[36..40].copy_from_slice(&(-2_i32).to_le_bytes());
        let mut transport = FakeTransport {
            expected_code: IOCTL_CAPYIO_CONNECT_MONITOR,
            expected_input: input.to_vec(),
            response,
            returned: MONITOR_CONNECT_RESPONSE_BYTES,
            ..Default::default()
        };
        assert_eq!(
            connect_with(&mut transport, request).unwrap(),
            IddConnectedMonitor {
                identity: request.identity,
                connector_index: 3,
                target_id: 17,
                adapter_luid_low: 0x89ab_cdef,
                adapter_luid_high: -2,
            }
        );
    }

    #[test]
    fn connect_rejects_wrong_identity_connector_and_reserved_bytes() {
        let request = connect_request();
        let input = request.encode().unwrap();
        let mut wrong_identity = header(MONITOR_CONNECT_RESPONSE_BYTES);
        wrong_identity[8..24].copy_from_slice(
            identity("ffeeddcc-bbaa-9988-7766-554433221100")
                .capability_id()
                .as_uuid()
                .as_bytes(),
        );
        let mut transport = FakeTransport {
            expected_code: IOCTL_CAPYIO_CONNECT_MONITOR,
            expected_input: input.to_vec(),
            response: wrong_identity,
            returned: MONITOR_CONNECT_RESPONSE_BYTES,
            ..Default::default()
        };
        assert_eq!(
            connect_with(&mut transport, request),
            Err(IddDeviceControlError::IdentityMismatch)
        );

        for (offset, value) in [(24_usize, 8_u8), (40, 1)] {
            let mut response = header(MONITOR_CONNECT_RESPONSE_BYTES);
            response[8..24].copy_from_slice(&input[8..24]);
            response[offset] = value;
            let mut transport = FakeTransport {
                expected_code: IOCTL_CAPYIO_CONNECT_MONITOR,
                expected_input: input.to_vec(),
                response,
                returned: MONITOR_CONNECT_RESPONSE_BYTES,
                ..Default::default()
            };
            assert_eq!(
                connect_with(&mut transport, request),
                Err(IddDeviceControlError::InvalidResponse {
                    operation: "connect"
                })
            );
        }
    }

    fn write_inventory_entry(
        response: &mut [u8],
        slot: usize,
        identity: IddMonitorIdentity,
        connector: u32,
        target: u32,
        luid_low: u32,
        luid_high: i32,
    ) {
        let offset = 16 + slot * 32;
        response[offset..offset + 16]
            .copy_from_slice(identity.capability_id().as_uuid().as_bytes());
        response[offset + 16..offset + 20].copy_from_slice(&connector.to_le_bytes());
        response[offset + 20..offset + 24].copy_from_slice(&target.to_le_bytes());
        response[offset + 24..offset + 28].copy_from_slice(&luid_low.to_le_bytes());
        response[offset + 28..offset + 32].copy_from_slice(&luid_high.to_le_bytes());
    }

    #[test]
    fn inventory_accepts_sparse_slots_with_one_adapter_identity() {
        let first = identity("00112233-4455-6677-8899-aabbccddeeff");
        let second = identity("10112233-4455-6677-8899-aabbccddeeff");
        let mut response = header(IDD_MONITOR_INVENTORY_RESPONSE_BYTES);
        response[8..12].copy_from_slice(&2_u32.to_le_bytes());
        write_inventory_entry(&mut response, 0, first, 0, 11, 42, -3);
        write_inventory_entry(&mut response, 5, second, 5, 19, 42, -3);
        let mut transport = FakeTransport {
            expected_code: IOCTL_CAPYIO_ENUMERATE_MONITORS,
            response,
            returned: IDD_MONITOR_INVENTORY_RESPONSE_BYTES,
            ..Default::default()
        };
        assert_eq!(
            enumerate_with(&mut transport).unwrap(),
            vec![
                IddConnectedMonitor {
                    identity: first,
                    connector_index: 0,
                    target_id: 11,
                    adapter_luid_low: 42,
                    adapter_luid_high: -3,
                },
                IddConnectedMonitor {
                    identity: second,
                    connector_index: 5,
                    target_id: 19,
                    adapter_luid_low: 42,
                    adapter_luid_high: -3,
                }
            ]
        );
    }

    #[test]
    fn inventory_rejects_count_slot_identity_luid_and_length_drift() {
        let first = identity("00112233-4455-6677-8899-aabbccddeeff");
        let cases = [
            "count",
            "inactive",
            "connector",
            "duplicate",
            "luid",
            "flags",
        ];
        for case in cases {
            let mut response = header(IDD_MONITOR_INVENTORY_RESPONSE_BYTES);
            response[8..12].copy_from_slice(&2_u32.to_le_bytes());
            write_inventory_entry(&mut response, 0, first, 0, 11, 42, -3);
            write_inventory_entry(
                &mut response,
                1,
                identity("10112233-4455-6677-8899-aabbccddeeff"),
                1,
                12,
                42,
                -3,
            );
            match case {
                "count" => response[8..12].copy_from_slice(&1_u32.to_le_bytes()),
                "inactive" => response[48..64].fill(0),
                "connector" => response[64..68].copy_from_slice(&2_u32.to_le_bytes()),
                "duplicate" => {
                    response[48..64].copy_from_slice(first.capability_id().as_uuid().as_bytes())
                }
                "luid" => response[72..76].copy_from_slice(&43_u32.to_le_bytes()),
                "flags" => response[12] = 1,
                _ => unreachable!(),
            }
            let mut transport = FakeTransport {
                expected_code: IOCTL_CAPYIO_ENUMERATE_MONITORS,
                response,
                returned: IDD_MONITOR_INVENTORY_RESPONSE_BYTES,
                ..Default::default()
            };
            assert_eq!(
                enumerate_with(&mut transport),
                Err(IddDeviceControlError::InvalidResponse {
                    operation: "enumerate"
                }),
                "case {case}"
            );
        }

        let mut transport = FakeTransport {
            expected_code: IOCTL_CAPYIO_ENUMERATE_MONITORS,
            response: header(IDD_MONITOR_INVENTORY_RESPONSE_BYTES),
            returned: IDD_MONITOR_INVENTORY_RESPONSE_BYTES - 1,
            ..Default::default()
        };
        assert_eq!(
            enumerate_with(&mut transport),
            Err(IddDeviceControlError::UnexpectedResponseLength {
                operation: "enumerate"
            })
        );
    }

    #[test]
    fn disconnect_sends_exact_request_and_requires_empty_response() {
        let request = IddMonitorDisconnectRequest {
            identity: connect_request().identity,
        };
        let mut transport = FakeTransport {
            expected_code: IOCTL_CAPYIO_DISCONNECT_MONITOR,
            expected_input: request.encode().to_vec(),
            returned: 0,
            ..Default::default()
        };
        disconnect_with(&mut transport, request).unwrap();

        transport.returned = 1;
        assert_eq!(
            disconnect_with(&mut transport, request),
            Err(IddDeviceControlError::UnexpectedResponseLength {
                operation: "disconnect"
            })
        );
    }

    #[test]
    fn request_layout_constants_stay_exact() {
        assert_eq!(IDD_CONNECT_REQUEST_BYTES, 48);
        assert_eq!(IDD_DISCONNECT_REQUEST_BYTES, 32);
    }
}
