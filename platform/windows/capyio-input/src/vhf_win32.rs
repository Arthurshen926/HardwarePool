use capyio_input::TouchpadDescriptor;

use crate::{
    VHF_BROKER_RECORD_SIZE, VhfBrokerClientError, VhfBrokerRecordTransport,
    VhfBrokerTransportError, VhfTouchpadSession, VhfTouchpadSessionError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VhfBrokerInterfaceProbe {
    UnsupportedPlatform,
    Absent,
    Single,
    Multiple,
    Failed(u32),
}

pub struct VhfWin32Transport {
    inner: platform::Transport,
}

impl VhfWin32Transport {
    pub fn open() -> Result<Self, VhfBrokerTransportError> {
        platform::Transport::open().map(|inner| Self { inner })
    }
}

impl VhfTouchpadSession<VhfWin32Transport> {
    /// Open the one present protected CapyIO VHF interface and complete its
    /// Broker Hello handshake. Call only after Runtime Route admission.
    pub fn open_win32(
        descriptor: TouchpadDescriptor,
        generation: u64,
    ) -> Result<Self, VhfTouchpadSessionError> {
        let transport = VhfWin32Transport::open().map_err(|error| {
            VhfTouchpadSessionError::Client(VhfBrokerClientError::Transport(error))
        })?;
        Self::open(transport, descriptor, generation)
    }
}

impl VhfBrokerRecordTransport for VhfWin32Transport {
    fn transact(
        &mut self,
        record: &[u8; VHF_BROKER_RECORD_SIZE],
    ) -> Result<[u8; VHF_BROKER_RECORD_SIZE], VhfBrokerTransportError> {
        self.inner.transact(record)
    }
}

#[must_use]
pub fn probe_vhf_broker_interface() -> VhfBrokerInterfaceProbe {
    platform::probe()
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub(super) struct Transport;

    impl Transport {
        pub(super) fn open() -> Result<Self, VhfBrokerTransportError> {
            Err(VhfBrokerTransportError::UnsupportedPlatform)
        }

        pub(super) fn transact(
            &mut self,
            _record: &[u8; VHF_BROKER_RECORD_SIZE],
        ) -> Result<[u8; VHF_BROKER_RECORD_SIZE], VhfBrokerTransportError> {
            Err(VhfBrokerTransportError::UnsupportedPlatform)
        }
    }

    pub(super) fn probe() -> VhfBrokerInterfaceProbe {
        VhfBrokerInterfaceProbe::UnsupportedPlatform
    }
}

#[cfg(windows)]
mod platform {
    use std::{ffi::c_void, mem, ptr};

    use windows_sys::{
        Win32::{
            Devices::DeviceAndDriverInstallation::{
                DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO, SP_DEVICE_INTERFACE_DATA,
                SP_DEVICE_INTERFACE_DETAIL_DATA_W, SetupDiDestroyDeviceInfoList,
                SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
                SetupDiGetDeviceInterfaceDetailW,
            },
            Foundation::{
                CloseHandle, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, GENERIC_READ,
                GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
            },
            Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING},
            System::IO::DeviceIoControl,
        },
        core::GUID,
    };

    use super::*;

    static INTERFACE_GUID: GUID = GUID {
        data1: 0x398a3698,
        data2: 0x9c4f,
        data3: 0x4be2,
        data4: [0x9a, 0xd2, 0x0e, 0xd8, 0xdf, 0x9b, 0x71, 0x31],
    };
    const MAX_DETAIL_BYTES: u32 = 4096;
    const IOCTL_CAPY_PTP_BROKER_RECORD: u32 = (0x22_u32 << 16) | (2_u32 << 14) | (0x800_u32 << 2);

    struct DeviceInfoSet(HDEVINFO);

    impl DeviceInfoSet {
        fn create() -> Result<Self, u32> {
            // SAFETY: the class GUID is static and all optional pointer-like
            // arguments are null. The returned list is owned by this RAII type.
            let handle = unsafe {
                SetupDiGetClassDevsW(
                    &raw const INTERFACE_GUID,
                    ptr::null(),
                    ptr::null_mut(),
                    DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
                )
            };
            if handle == -1_isize {
                // SAFETY: immediately captures the failed SetupAPI call.
                Err(unsafe { GetLastError() })
            } else {
                Ok(Self(handle))
            }
        }

        fn interface(&self, index: u32) -> Result<SP_DEVICE_INTERFACE_DATA, u32> {
            // SAFETY: zero is a valid initial representation; cbSize is set
            // before SetupAPI observes the structure.
            let mut data: SP_DEVICE_INTERFACE_DATA = unsafe { mem::zeroed() };
            data.cbSize = mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
            // SAFETY: the info set is live and `data` is writable for the call.
            let ok = unsafe {
                SetupDiEnumDeviceInterfaces(
                    self.0,
                    ptr::null(),
                    &raw const INTERFACE_GUID,
                    index,
                    &raw mut data,
                )
            };
            if ok == 0 {
                // SAFETY: immediately captures the failed enumeration call.
                Err(unsafe { GetLastError() })
            } else {
                Ok(data)
            }
        }

        fn unique_interface(&self) -> Result<SP_DEVICE_INTERFACE_DATA, VhfBrokerTransportError> {
            let first = self.interface(0).map_err(|code| {
                if code == ERROR_NO_MORE_ITEMS {
                    VhfBrokerTransportError::DriverInterfaceAbsent
                } else {
                    VhfBrokerTransportError::Win32(code)
                }
            })?;
            match self.interface(1) {
                Err(ERROR_NO_MORE_ITEMS) => Ok(first),
                Ok(_) => Err(VhfBrokerTransportError::AmbiguousDriverInterfaces),
                Err(code) => Err(VhfBrokerTransportError::Win32(code)),
            }
        }

        fn path(
            &self,
            interface: &SP_DEVICE_INTERFACE_DATA,
        ) -> Result<Vec<u16>, VhfBrokerTransportError> {
            let mut required = 0_u32;
            // SAFETY: this is the documented size query with a null detail
            // buffer; `required` is writable and interface/list are live.
            let ok = unsafe {
                SetupDiGetDeviceInterfaceDetailW(
                    self.0,
                    interface,
                    ptr::null_mut(),
                    0,
                    &raw mut required,
                    ptr::null_mut(),
                )
            };
            // SAFETY: immediately captures the size-query result.
            let error = unsafe { GetLastError() };
            if ok != 0 || error != ERROR_INSUFFICIENT_BUFFER {
                return Err(VhfBrokerTransportError::Win32(error));
            }
            if required < mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32
                || required > MAX_DETAIL_BYTES
            {
                return Err(VhfBrokerTransportError::DevicePathTooLong(required));
            }

            let word_size = mem::size_of::<usize>();
            let word_count = (required as usize).div_ceil(word_size);
            let mut storage = vec![0_usize; word_count];
            let detail = storage
                .as_mut_ptr()
                .cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
            // SAFETY: `storage` is suitably aligned and at least `required`
            // bytes; detail's cbSize is the only field read on input.
            unsafe {
                (*detail).cbSize = mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            }
            // SAFETY: all buffers are live, aligned and sized as declared.
            let ok = unsafe {
                SetupDiGetDeviceInterfaceDetailW(
                    self.0,
                    interface,
                    detail,
                    required,
                    &raw mut required,
                    ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Err(VhfBrokerTransportError::Win32(unsafe { GetLastError() }));
            }

            // SAFETY: SetupAPI wrote a NUL-terminated DevicePath inside the
            // bounded detail buffer. The scan remains within `required` bytes.
            let path_pointer = unsafe { ptr::addr_of!((*detail).DevicePath).cast::<u16>() };
            let path_offset = path_pointer as usize - detail as usize;
            let capacity = (required as usize - path_offset) / mem::size_of::<u16>();
            // SAFETY: the slice is wholly contained in `storage`.
            let path_slice = unsafe { std::slice::from_raw_parts(path_pointer, capacity) };
            let nul = path_slice
                .iter()
                .position(|unit| *unit == 0)
                .ok_or(VhfBrokerTransportError::DevicePathTooLong(required))?;
            let mut path = path_slice[..nul].to_vec();
            path.push(0);
            Ok(path)
        }
    }

    impl Drop for DeviceInfoSet {
        fn drop(&mut self) {
            // SAFETY: this handle is owned and destroyed exactly once.
            let _ = unsafe { SetupDiDestroyDeviceInfoList(self.0) };
        }
    }

    pub(super) struct Transport(HANDLE);

    impl Transport {
        pub(super) fn open() -> Result<Self, VhfBrokerTransportError> {
            let info = DeviceInfoSet::create().map_err(VhfBrokerTransportError::Win32)?;
            let interface = info.unique_interface()?;
            let path = info.path(&interface)?;
            // SAFETY: path is a live NUL-terminated UTF-16 device-interface
            // string; no sharing is requested because the driver is exclusive.
            let handle = unsafe {
                CreateFileW(
                    path.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    ptr::null(),
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    ptr::null_mut(),
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                Err(VhfBrokerTransportError::Win32(unsafe { GetLastError() }))
            } else {
                Ok(Self(handle))
            }
        }

        pub(super) fn transact(
            &mut self,
            record: &[u8; VHF_BROKER_RECORD_SIZE],
        ) -> Result<[u8; VHF_BROKER_RECORD_SIZE], VhfBrokerTransportError> {
            let mut output = [0_u8; VHF_BROKER_RECORD_SIZE];
            let mut returned = 0_u32;
            // SAFETY: the handle is live, both fixed buffers are valid for the
            // synchronous call and no OVERLAPPED operation is requested.
            let ok = unsafe {
                DeviceIoControl(
                    self.0,
                    IOCTL_CAPY_PTP_BROKER_RECORD,
                    record.as_ptr().cast::<c_void>(),
                    VHF_BROKER_RECORD_SIZE as u32,
                    output.as_mut_ptr().cast::<c_void>(),
                    VHF_BROKER_RECORD_SIZE as u32,
                    &raw mut returned,
                    ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Err(VhfBrokerTransportError::Win32(unsafe { GetLastError() }));
            }
            if returned != VHF_BROKER_RECORD_SIZE as u32 {
                return Err(VhfBrokerTransportError::UnexpectedOutputSize(returned));
            }
            Ok(output)
        }
    }

    impl Drop for Transport {
        fn drop(&mut self) {
            // SAFETY: this handle is owned and closed exactly once.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    pub(super) fn probe() -> VhfBrokerInterfaceProbe {
        let info = match DeviceInfoSet::create() {
            Ok(info) => info,
            Err(code) => return VhfBrokerInterfaceProbe::Failed(code),
        };
        match info.interface(0) {
            Err(ERROR_NO_MORE_ITEMS) => VhfBrokerInterfaceProbe::Absent,
            Err(code) => VhfBrokerInterfaceProbe::Failed(code),
            Ok(_) => match info.interface(1) {
                Err(ERROR_NO_MORE_ITEMS) => VhfBrokerInterfaceProbe::Single,
                Err(code) => VhfBrokerInterfaceProbe::Failed(code),
                Ok(_) => VhfBrokerInterfaceProbe::Multiple,
            },
        }
    }
}
