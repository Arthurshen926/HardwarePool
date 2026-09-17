use std::{
    ffi::c_void,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use thiserror::Error;
use windows_sys::{
    Win32::{
        Devices::DeviceAndDriverInstallation::{
            CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_Get_Device_Interface_List_SizeW,
            CM_Get_Device_Interface_ListW, CR_BUFFER_SMALL, CR_SUCCESS,
        },
        Foundation::{GENERIC_READ, GENERIC_WRITE, GetLastError, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::IO::DeviceIoControl,
    },
    core::GUID,
};

use crate::{
    IOCTL_SUDOVDA_ADD_VIRTUAL_DISPLAY, IOCTL_SUDOVDA_DRIVER_PING,
    IOCTL_SUDOVDA_GET_PROTOCOL_VERSION, IOCTL_SUDOVDA_REMOVE_VIRTUAL_DISPLAY, IddConnectedMonitor,
    IddMonitorConnectRequest, IddMonitorIdentity, IddMonitorMode, SUDOVDA_ADD_RESPONSE_BYTES,
    SUDOVDA_PROTOCOL_RESPONSE_BYTES, SudoVdaAddRequest, SudoVdaAddResponse, SudoVdaControlError,
    SudoVdaProtocolVersion, WindowsDisplayTopologyLease, encode_sudovda_remove,
};

const SUDOVDA_DEVICE_INTERFACE: GUID = GUID::from_u128(0xe5bcc234_1e0c_418a_a0d4_ef8b7501414d);
const MAX_INTERFACE_LIST_CHARS: u32 = 32_768;
const ENUMERATION_ATTEMPTS: usize = 3;
const WATCHDOG_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SudoVdaConnectedMonitor {
    pub identity: IddMonitorIdentity,
    pub mode: IddMonitorMode,
    pub target_id: u32,
    pub adapter_luid_low: u32,
    pub adapter_luid_high: i32,
}

impl SudoVdaConnectedMonitor {
    #[must_use]
    pub const fn as_idd_monitor(self) -> IddConnectedMonitor {
        IddConnectedMonitor {
            identity: self.identity,
            connector_index: self.target_id,
            target_id: self.target_id,
            adapter_luid_low: self.adapter_luid_low,
            adapter_luid_high: self.adapter_luid_high,
        }
    }
}

#[derive(Debug, Error)]
pub enum SudoVdaDeviceControlError {
    #[error("no present SudoVDA device interface was found")]
    NotFound,
    #[error("more than one present SudoVDA device interface was found")]
    AmbiguousInterface,
    #[error("SudoVDA device-interface enumeration failed with CONFIGRET {code}")]
    Enumeration { code: u32 },
    #[error("SudoVDA device-interface list is malformed or exceeds its bound")]
    InvalidInterfaceList,
    #[error("opening the SudoVDA device interface failed with Win32 error {code}")]
    Open { code: u32 },
    #[error("the {operation} SudoVDA IOCTL failed with Win32 error {code}")]
    Ioctl { operation: &'static str, code: u32 },
    #[error("the {operation} SudoVDA response had an unexpected length")]
    UnexpectedResponseLength { operation: &'static str },
    #[error(
        "installed SudoVDA protocol {major}.{minor}.{incremental} test={test_build} is unsupported"
    )]
    UnsupportedProtocol {
        major: u8,
        minor: u8,
        incremental: u8,
        test_build: bool,
    },
    #[error(transparent)]
    Contract(#[from] SudoVdaControlError),
    #[error("SudoVDA watchdog thread could not be started")]
    WatchdogStart,
    #[error("SudoVDA watchdog thread panicked")]
    WatchdogPanicked,
    #[error("SudoVDA monitor lease was already released")]
    AlreadyReleased,
    #[error(transparent)]
    Topology(#[from] crate::DisplayTopologyError),
}

trait SudoVdaIoctlTransport {
    fn call(
        &mut self,
        operation: &'static str,
        code: u32,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<usize, SudoVdaDeviceControlError>;
}

struct Win32IoctlTransport {
    handle: OwnedHandle,
}

impl SudoVdaIoctlTransport for Win32IoctlTransport {
    fn call(
        &mut self,
        operation: &'static str,
        code: u32,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<usize, SudoVdaDeviceControlError> {
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
        // SAFETY: buffers remain live for the synchronous call and are bounded
        // by the lengths passed to DeviceIoControl.
        let succeeded = unsafe {
            DeviceIoControl(
                self.handle.as_raw_handle().cast(),
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
            // SAFETY: read immediately after the failing Win32 call.
            let code = unsafe { GetLastError() };
            return Err(SudoVdaDeviceControlError::Ioctl { operation, code });
        }
        Ok(returned as usize)
    }
}

struct Controller<T> {
    transport: T,
}

impl<T: SudoVdaIoctlTransport> Controller<T> {
    fn verify_protocol(&mut self) -> Result<SudoVdaProtocolVersion, SudoVdaDeviceControlError> {
        let mut output = [0_u8; SUDOVDA_PROTOCOL_RESPONSE_BYTES];
        let returned = self.transport.call(
            "query protocol",
            IOCTL_SUDOVDA_GET_PROTOCOL_VERSION,
            &[],
            &mut output,
        )?;
        require_exact_length("query protocol", returned, output.len())?;
        let version = SudoVdaProtocolVersion::parse(output)?;
        if !version.supported() {
            return Err(SudoVdaDeviceControlError::UnsupportedProtocol {
                major: version.major,
                minor: version.minor,
                incremental: version.incremental,
                test_build: version.test_build,
            });
        }
        Ok(version)
    }

    fn add(
        &mut self,
        request: IddMonitorConnectRequest,
    ) -> Result<SudoVdaConnectedMonitor, SudoVdaDeviceControlError> {
        let input = SudoVdaAddRequest::try_from(request)?.encode();
        let mut output = [0_u8; SUDOVDA_ADD_RESPONSE_BYTES];
        let returned = self.transport.call(
            "add virtual display",
            IOCTL_SUDOVDA_ADD_VIRTUAL_DISPLAY,
            &input,
            &mut output,
        )?;
        require_exact_length("add virtual display", returned, output.len())?;
        let response = SudoVdaAddResponse::parse(output);
        Ok(SudoVdaConnectedMonitor {
            identity: request.identity,
            mode: request.preferred_mode,
            target_id: response.target_id,
            adapter_luid_low: response.adapter_luid_low,
            adapter_luid_high: response.adapter_luid_high,
        })
    }

    fn remove(&mut self, identity: IddMonitorIdentity) -> Result<(), SudoVdaDeviceControlError> {
        let input = encode_sudovda_remove(identity);
        let returned = self.transport.call(
            "remove virtual display",
            IOCTL_SUDOVDA_REMOVE_VIRTUAL_DISPLAY,
            &input,
            &mut [],
        )?;
        require_exact_length("remove virtual display", returned, 0)
    }

    fn ping(&mut self) -> Result<(), SudoVdaDeviceControlError> {
        let returned =
            self.transport
                .call("feed watchdog", IOCTL_SUDOVDA_DRIVER_PING, &[], &mut [])?;
        require_exact_length("feed watchdog", returned, 0)
    }
}

pub struct SudoVdaDeviceController {
    inner: Controller<Win32IoctlTransport>,
}

impl SudoVdaDeviceController {
    pub fn open_unique() -> Result<Self, SudoVdaDeviceControlError> {
        let path = enumerate_unique_interface()?;
        // SAFETY: path is a validated NUL-terminated device-interface path.
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
            // SAFETY: read immediately after the failing CreateFileW call.
            let code = unsafe { GetLastError() };
            return Err(SudoVdaDeviceControlError::Open { code });
        }
        // SAFETY: CreateFileW returned a valid, exclusively owned handle. The
        // resulting OwnedHandle closes it exactly once.
        let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
        let mut controller = Self {
            inner: Controller {
                transport: Win32IoctlTransport { handle },
            },
        };
        controller.inner.verify_protocol()?;
        Ok(controller)
    }

    pub fn add(
        &mut self,
        request: IddMonitorConnectRequest,
    ) -> Result<SudoVdaConnectedMonitor, SudoVdaDeviceControlError> {
        self.inner.add(request)
    }

    pub fn remove(
        &mut self,
        identity: IddMonitorIdentity,
    ) -> Result<(), SudoVdaDeviceControlError> {
        self.inner.remove(identity)
    }

    pub fn ping(&mut self) -> Result<(), SudoVdaDeviceControlError> {
        self.inner.ping()
    }
}

struct Watchdog {
    stop: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Watchdog {
    fn start() -> Result<Self, SudoVdaDeviceControlError> {
        // Open and validate the exact device synchronously.  A successful
        // lease creation must never race a background open failure.
        let mut controller = SudoVdaDeviceController::open_unique()?;
        let stop = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread_failed = Arc::clone(&failed);
        let thread = thread::Builder::new()
            .name("capyio-sudovda-watchdog".to_owned())
            .spawn(move || {
                while !thread_stop.load(Ordering::Acquire) {
                    if controller.ping().is_err() {
                        thread_failed.store(true, Ordering::Release);
                        return;
                    }
                    for _ in 0..10 {
                        if thread_stop.load(Ordering::Acquire) {
                            return;
                        }
                        thread::sleep(WATCHDOG_INTERVAL / 10);
                    }
                }
            })
            .map_err(|_| SudoVdaDeviceControlError::WatchdogStart)?;
        Ok(Self {
            stop,
            failed,
            thread: Some(thread),
        })
    }

    fn healthy(&self) -> bool {
        !self.failed.load(Ordering::Acquire)
    }

    fn stop(&mut self) -> Result<(), SudoVdaDeviceControlError> {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| SudoVdaDeviceControlError::WatchdogPanicked)?;
        }
        Ok(())
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub struct SudoVdaMonitorLease {
    controller: SudoVdaDeviceController,
    monitor: Option<SudoVdaConnectedMonitor>,
    watchdog: Watchdog,
    topology: WindowsDisplayTopologyLease,
}

impl SudoVdaMonitorLease {
    pub fn create(request: IddMonitorConnectRequest) -> Result<Self, SudoVdaDeviceControlError> {
        let mut topology = WindowsDisplayTopologyLease::capture()?;
        let mut controller = SudoVdaDeviceController::open_unique()?;
        controller.ping()?;
        let monitor = controller.add(request)?;
        let watchdog = match Watchdog::start() {
            Ok(watchdog) => watchdog,
            Err(error) => {
                let _ = controller.remove(request.identity);
                let _ = topology.restore_after_target_departure(
                    monitor.adapter_luid_low,
                    monitor.adapter_luid_high,
                    monitor.target_id,
                );
                return Err(error);
            }
        };
        Ok(Self {
            controller,
            monitor: Some(monitor),
            watchdog,
            topology,
        })
    }

    pub fn monitor(&self) -> Result<SudoVdaConnectedMonitor, SudoVdaDeviceControlError> {
        self.monitor
            .ok_or(SudoVdaDeviceControlError::AlreadyReleased)
    }

    #[must_use]
    pub fn watchdog_healthy(&self) -> bool {
        self.watchdog.healthy()
    }

    pub fn release(&mut self) -> Result<(), SudoVdaDeviceControlError> {
        let monitor = self
            .monitor
            .ok_or(SudoVdaDeviceControlError::AlreadyReleased)?;
        let remove_result = self.controller.remove(monitor.identity);
        if remove_result.is_ok() {
            self.monitor = None;
        }
        let topology_result = if remove_result.is_ok() {
            self.topology
                .restore_after_target_departure(
                    monitor.adapter_luid_low,
                    monitor.adapter_luid_high,
                    monitor.target_id,
                )
                .map_err(SudoVdaDeviceControlError::from)
        } else {
            Ok(())
        };
        let watchdog_result = self.watchdog.stop();
        remove_result?;
        topology_result?;
        watchdog_result
    }
}

impl Drop for SudoVdaMonitorLease {
    fn drop(&mut self) {
        let removed = self.monitor.and_then(|monitor| {
            self.controller
                .remove(monitor.identity)
                .is_ok()
                .then_some(monitor)
        });
        if let Some(monitor) = removed {
            self.monitor = None;
            if !self.topology.is_restored() {
                let _ = self.topology.restore_after_target_departure(
                    monitor.adapter_luid_low,
                    monitor.adapter_luid_high,
                    monitor.target_id,
                );
            }
        }
        let _ = self.watchdog.stop();
    }
}

fn require_exact_length(
    operation: &'static str,
    actual: usize,
    expected: usize,
) -> Result<(), SudoVdaDeviceControlError> {
    if actual == expected {
        Ok(())
    } else {
        Err(SudoVdaDeviceControlError::UnexpectedResponseLength { operation })
    }
}

fn enumerate_unique_interface() -> Result<Vec<u16>, SudoVdaDeviceControlError> {
    for _ in 0..ENUMERATION_ATTEMPTS {
        let mut required_chars = 0_u32;
        // SAFETY: all pointers reference valid storage or are null.
        let result = unsafe {
            CM_Get_Device_Interface_List_SizeW(
                &mut required_chars,
                &SUDOVDA_DEVICE_INTERFACE,
                ptr::null(),
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if result != CR_SUCCESS {
            return Err(SudoVdaDeviceControlError::Enumeration { code: result });
        }
        if required_chars == 0 || required_chars > MAX_INTERFACE_LIST_CHARS {
            return Err(SudoVdaDeviceControlError::InvalidInterfaceList);
        }
        let mut list = vec![0_u16; required_chars as usize];
        // SAFETY: list contains exactly required_chars writable elements.
        let result = unsafe {
            CM_Get_Device_Interface_ListW(
                &SUDOVDA_DEVICE_INTERFACE,
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
            return Err(SudoVdaDeviceControlError::Enumeration { code: result });
        }
        return select_unique_multi_sz(&list);
    }
    Err(SudoVdaDeviceControlError::Enumeration {
        code: CR_BUFFER_SMALL,
    })
}

fn select_unique_multi_sz(list: &[u16]) -> Result<Vec<u16>, SudoVdaDeviceControlError> {
    if list.is_empty() || list.last() != Some(&0) {
        return Err(SudoVdaDeviceControlError::InvalidInterfaceList);
    }
    if list.len() == 1 {
        return Err(SudoVdaDeviceControlError::NotFound);
    }
    if list[list.len() - 2] != 0 {
        return Err(SudoVdaDeviceControlError::InvalidInterfaceList);
    }
    let mut entries = list
        .split(|character| *character == 0)
        .filter(|entry| !entry.is_empty());
    let Some(path) = entries.next() else {
        return Err(SudoVdaDeviceControlError::NotFound);
    };
    if entries.next().is_some() {
        return Err(SudoVdaDeviceControlError::AmbiguousInterface);
    }
    let mut terminated = Vec::with_capacity(path.len() + 1);
    terminated.extend_from_slice(path);
    terminated.push(0);
    Ok(terminated)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use capyio_core::ids::CapabilityId;

    use super::*;

    struct ExpectedCall {
        operation: &'static str,
        code: u32,
        input: Vec<u8>,
        output: Vec<u8>,
    }

    struct FakeTransport {
        calls: VecDeque<ExpectedCall>,
    }

    impl SudoVdaIoctlTransport for FakeTransport {
        fn call(
            &mut self,
            operation: &'static str,
            code: u32,
            input: &[u8],
            output: &mut [u8],
        ) -> Result<usize, SudoVdaDeviceControlError> {
            let expected = self.calls.pop_front().expect("unexpected IOCTL");
            assert_eq!(operation, expected.operation);
            assert_eq!(code, expected.code);
            assert_eq!(input, expected.input);
            assert_eq!(output.len(), expected.output.len());
            output.copy_from_slice(&expected.output);
            Ok(output.len())
        }
    }

    fn identity() -> IddMonitorIdentity {
        IddMonitorIdentity::new(
            "00112233-4455-6677-8899-aabbccddeeff"
                .parse::<CapabilityId>()
                .unwrap(),
        )
        .unwrap()
    }

    fn request() -> IddMonitorConnectRequest {
        IddMonitorConnectRequest {
            identity: identity(),
            preferred_mode: IddMonitorMode {
                width: 2640,
                height: 1216,
                refresh_millihz: 120_000,
            },
        }
    }

    #[test]
    fn exact_protocol_add_ping_and_identity_remove_sequence() {
        let encoded_add = SudoVdaAddRequest::try_from(request()).unwrap().encode();
        let encoded_remove = encode_sudovda_remove(identity());
        let mut controller = Controller {
            transport: FakeTransport {
                calls: VecDeque::from([
                    ExpectedCall {
                        operation: "query protocol",
                        code: IOCTL_SUDOVDA_GET_PROTOCOL_VERSION,
                        input: vec![],
                        output: vec![0, 2, 1, 1],
                    },
                    ExpectedCall {
                        operation: "add virtual display",
                        code: IOCTL_SUDOVDA_ADD_VIRTUAL_DISPLAY,
                        input: encoded_add.to_vec(),
                        output: [
                            42_u32.to_le_bytes(),
                            (-3_i32).to_le_bytes(),
                            9_u32.to_le_bytes(),
                        ]
                        .concat(),
                    },
                    ExpectedCall {
                        operation: "feed watchdog",
                        code: IOCTL_SUDOVDA_DRIVER_PING,
                        input: vec![],
                        output: vec![],
                    },
                    ExpectedCall {
                        operation: "remove virtual display",
                        code: IOCTL_SUDOVDA_REMOVE_VIRTUAL_DISPLAY,
                        input: encoded_remove.to_vec(),
                        output: vec![],
                    },
                ]),
            },
        };

        assert!(controller.verify_protocol().unwrap().supported());
        assert_eq!(
            controller.add(request()).unwrap(),
            SudoVdaConnectedMonitor {
                identity: identity(),
                mode: request().preferred_mode,
                target_id: 9,
                adapter_luid_low: 42,
                adapter_luid_high: -3,
            }
        );
        controller.ping().unwrap();
        controller.remove(identity()).unwrap();
        assert!(controller.transport.calls.is_empty());
    }

    #[test]
    fn protocol_mismatch_fails_before_monitor_creation() {
        let mut controller = Controller {
            transport: FakeTransport {
                calls: VecDeque::from([ExpectedCall {
                    operation: "query protocol",
                    code: IOCTL_SUDOVDA_GET_PROTOCOL_VERSION,
                    input: vec![],
                    output: vec![0, 2, 0, 1],
                }]),
            },
        };
        assert!(matches!(
            controller.verify_protocol(),
            Err(SudoVdaDeviceControlError::UnsupportedProtocol { .. })
        ));
        assert!(controller.transport.calls.is_empty());
    }

    #[test]
    fn interface_selection_rejects_zero_and_multiple_devices() {
        assert!(matches!(
            select_unique_multi_sz(&[0]),
            Err(SudoVdaDeviceControlError::NotFound)
        ));
        assert!(matches!(
            select_unique_multi_sz(&[b'a' as u16, 0, b'b' as u16, 0, 0]),
            Err(SudoVdaDeviceControlError::AmbiguousInterface)
        ));
        assert_eq!(
            select_unique_multi_sz(&[b'a' as u16, 0, 0]).unwrap(),
            vec![b'a' as u16, 0]
        );
    }
}
