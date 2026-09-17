use std::{
    io,
    mem::size_of,
    ptr::{self, NonNull},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, INVALID_HANDLE_VALUE, LocalFree,
    },
    Security::{
        Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1},
        PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
    },
    System::Memory::{
        CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile,
        PAGE_READWRITE, UnmapViewOfFile,
    },
};

use crate::{
    CaptureBackendPoll, CaptureSize, CapturedBgraFrame, IDD_HANDOFF_ABI_VERSION,
    IDD_HANDOFF_HEADER_BYTES, IDD_HANDOFF_MAGIC, IddFrameHeader, IddHandoffError,
    IddMonitorIdentity, MirrorCaptureBackend, WindowsDisplayError,
    idd_handoff::IDD_MAX_PAYLOAD_BYTES,
};

const TOTAL_BYTES: usize = IDD_HANDOFF_HEADER_BYTES + IDD_MAX_PAYLOAD_BYTES as usize;
#[cfg(not(test))]
const LEGACY_MAPPING_NAME: &str = r"Global\CapyIO.DisplayFrame.v1";
#[cfg(test)]
const LEGACY_MAPPING_NAME: &str = r"Local\CapyIO.DisplayFrame.v1.test";
const MAPPING_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GRGW;;;LS)(A;;GA;;;BA)(A;;GA;;;OW)";

#[repr(C, align(8))]
struct SharedHeader {
    magic: u32,
    version: u16,
    header_size: u16,
    generation: AtomicU64,
    sequence_lock: AtomicU64,
    width: u32,
    height: u32,
    stride_bytes: u32,
    pixel_format: u32,
    payload_bytes: u64,
    source_timestamp_nanos: u64,
    flags: u32,
    reserved: u32,
}

const _: () = assert!(size_of::<SharedHeader>() == IDD_HANDOFF_HEADER_BYTES);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IddFrameSnapshot {
    pub header: IddFrameHeader,
    pub payload: Vec<u8>,
}

pub struct IddFrameBrokerMapping {
    mapping: windows_sys::Win32::Foundation::HANDLE,
    view: NonNull<u8>,
    generation: u64,
    last_sequence: u64,
}

pub struct IddFrameBackend {
    mapping: Option<IddFrameBrokerMapping>,
    monitor_identity: Option<IddMonitorIdentity>,
    selected: CaptureSize,
    running: bool,
}

impl IddFrameBackend {
    pub fn new(selected: CaptureSize) -> Result<Self, WindowsDisplayError> {
        selected.validate()?;
        Ok(Self {
            mapping: None,
            monitor_identity: None,
            selected,
            running: false,
        })
    }

    pub fn new_for_monitor(
        selected: CaptureSize,
        monitor_identity: IddMonitorIdentity,
    ) -> Result<Self, WindowsDisplayError> {
        selected.validate()?;
        Ok(Self {
            mapping: None,
            monitor_identity: Some(monitor_identity),
            selected,
            running: false,
        })
    }
}

impl MirrorCaptureBackend for IddFrameBackend {
    fn start(&mut self) -> Result<(), WindowsDisplayError> {
        if self.running || self.mapping.is_some() {
            return Err(WindowsDisplayError::Backend(
                "IDD frame Broker mapping is already running".to_owned(),
            ));
        }
        let mapping = match self.monitor_identity {
            Some(identity) => IddFrameBrokerMapping::create_for_monitor(identity),
            None => IddFrameBrokerMapping::create(),
        };
        self.mapping = Some(mapping.map_err(|error| {
            WindowsDisplayError::Backend(format!("create IDD frame mapping: {error}"))
        })?);
        self.running = true;
        Ok(())
    }

    fn poll(&mut self) -> Result<CaptureBackendPoll, WindowsDisplayError> {
        if !self.running {
            return Err(WindowsDisplayError::Backend(
                "IDD frame Broker mapping is not running".to_owned(),
            ));
        }
        let snapshot = self
            .mapping
            .as_mut()
            .expect("running backend owns mapping")
            .try_read()
            .map_err(|error| WindowsDisplayError::Backend(error.to_string()))?;
        let Some(snapshot) = snapshot else {
            return Ok(CaptureBackendPoll::Idle);
        };
        let size = CaptureSize {
            width: snapshot.header.width,
            height: snapshot.header.height,
        };
        if size != self.selected {
            return Ok(CaptureBackendPoll::SizeChanged(size));
        }
        let packed_stride = size
            .width
            .checked_mul(4)
            .ok_or(WindowsDisplayError::ArithmeticExhausted)?;
        if snapshot.header.stride_bytes != packed_stride {
            return Err(WindowsDisplayError::InvalidFrame(
                "IDD handoff must contain packed BGRA rows".to_owned(),
            ));
        }
        Ok(CaptureBackendPoll::Frame(CapturedBgraFrame {
            size,
            source_timestamp_nanos: snapshot.header.source_timestamp_nanos,
            payload: snapshot.payload,
        }))
    }

    fn reconfigure(&mut self, size: CaptureSize) -> Result<(), WindowsDisplayError> {
        if !self.running {
            return Err(WindowsDisplayError::Backend(
                "cannot reconfigure a stopped IDD frame mapping".to_owned(),
            ));
        }
        size.validate()?;
        self.selected = size;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), WindowsDisplayError> {
        self.mapping = None;
        self.running = false;
        Ok(())
    }
}

impl IddFrameBrokerMapping {
    pub fn create() -> Result<Self, IddHandoffError> {
        Self::create_named(LEGACY_MAPPING_NAME)
    }

    pub fn create_for_monitor(identity: IddMonitorIdentity) -> Result<Self, IddHandoffError> {
        #[cfg(not(test))]
        let mapping_name = identity.mapping_name();
        #[cfg(test)]
        let mapping_name = identity.test_mapping_name();
        Self::create_named(&mapping_name)
    }

    fn create_named(mapping_name: &str) -> Result<Self, IddHandoffError> {
        let mapping_name = wide_null(mapping_name);
        let mapping_sddl = wide_null(MAPPING_SDDL);
        let mut security_descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                mapping_sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut security_descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(last_error(
                "ConvertStringSecurityDescriptorToSecurityDescriptorW",
            ));
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security_descriptor,
            bInheritHandle: 0,
        };
        let size = u32::try_from(TOTAL_BYTES).expect("bounded display mapping fits u32");
        let mapping = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                &attributes,
                PAGE_READWRITE,
                0,
                size,
                mapping_name.as_ptr(),
            )
        };
        let mapping_error = unsafe { GetLastError() };
        unsafe { LocalFree(security_descriptor) };
        if mapping.is_null() {
            return Err(IddHandoffError::Windows {
                operation: "CreateFileMappingW",
                code: mapping_error,
            });
        }
        if mapping_error == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(mapping) };
            return Err(IddHandoffError::AlreadyOwned);
        }

        let mapped = unsafe { MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, TOTAL_BYTES) };
        let Some(view) = NonNull::new(mapped.Value.cast::<u8>()) else {
            let error = last_error("MapViewOfFile");
            unsafe { CloseHandle(mapping) };
            return Err(error);
        };
        unsafe { ptr::write_bytes(view.as_ptr(), 0, TOTAL_BYTES) };

        let generation = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .max(1) as u64;
        let header = SharedHeader {
            magic: IDD_HANDOFF_MAGIC,
            version: IDD_HANDOFF_ABI_VERSION,
            header_size: IDD_HANDOFF_HEADER_BYTES as u16,
            generation: AtomicU64::new(generation),
            sequence_lock: AtomicU64::new(0),
            width: 0,
            height: 0,
            stride_bytes: 0,
            pixel_format: 0,
            payload_bytes: 0,
            source_timestamp_nanos: 0,
            flags: 0,
            reserved: 0,
        };
        unsafe { ptr::write(view.as_ptr().cast::<SharedHeader>(), header) };

        Ok(Self {
            mapping,
            view,
            generation,
            last_sequence: 0,
        })
    }

    /// Copies one stable newest-frame snapshot. Superseded or absent frames are
    /// reported as None so callers never queue stale display content.
    pub fn try_read(&mut self) -> Result<Option<IddFrameSnapshot>, IddHandoffError> {
        let shared = unsafe { &*self.view.as_ptr().cast::<SharedHeader>() };
        if shared.magic != IDD_HANDOFF_MAGIC
            || shared.version != IDD_HANDOFF_ABI_VERSION
            || usize::from(shared.header_size) != IDD_HANDOFF_HEADER_BYTES
            || shared.generation.load(Ordering::Acquire) != self.generation
        {
            return Err(IddHandoffError::InvalidHeaderSize);
        }

        let before = shared.sequence_lock.load(Ordering::Acquire);
        if before == 0 || !before.is_multiple_of(2) || before == self.last_sequence {
            return Ok(None);
        }

        let header = IddFrameHeader {
            generation: shared.generation.load(Ordering::Acquire),
            sequence_lock: before,
            width: shared.width,
            height: shared.height,
            stride_bytes: shared.stride_bytes,
            pixel_format: shared.pixel_format,
            payload_bytes: shared.payload_bytes,
            source_timestamp_nanos: shared.source_timestamp_nanos,
            flags: shared.flags,
        };
        header.validate(TOTAL_BYTES)?;
        let payload_len =
            usize::try_from(header.payload_bytes).map_err(|_| IddHandoffError::CapacityExceeded)?;
        let mut payload = vec![0; payload_len];
        unsafe {
            ptr::copy_nonoverlapping(
                self.view.as_ptr().add(IDD_HANDOFF_HEADER_BYTES),
                payload.as_mut_ptr(),
                payload_len,
            );
        }

        let after = shared.sequence_lock.load(Ordering::Acquire);
        if before != after || !after.is_multiple_of(2) {
            return Ok(None);
        }
        self.last_sequence = after;
        Ok(Some(IddFrameSnapshot { header, payload }))
    }
}

impl Drop for IddFrameBrokerMapping {
    fn drop(&mut self) {
        let shared = unsafe { &*self.view.as_ptr().cast::<SharedHeader>() };
        shared.generation.store(0, Ordering::Release);
        shared.sequence_lock.store(u64::MAX, Ordering::Release);
        unsafe {
            UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.view.as_ptr().cast(),
            });
            CloseHandle(self.mapping);
        }
    }
}

fn last_error(operation: &'static str) -> IddHandoffError {
    let code = io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_default() as u32;
    IddHandoffError::Windows { operation, code }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    static MAPPING_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn mapping_is_single_owner_and_exactly_bounded() {
        let _guard = MAPPING_LOCK.lock().unwrap();
        let first = IddFrameBrokerMapping::create().unwrap();
        assert!(matches!(
            IddFrameBrokerMapping::create(),
            Err(IddHandoffError::AlreadyOwned)
        ));
        assert_eq!(TOTAL_BYTES, 64 + 64 * 1024 * 1024);
        drop(first);
        IddFrameBrokerMapping::create().unwrap();
    }

    #[test]
    fn reader_copies_one_committed_latest_frame_once() {
        let _guard = MAPPING_LOCK.lock().unwrap();
        let mut mapping = IddFrameBrokerMapping::create().unwrap();
        let shared = unsafe { &*mapping.view.as_ptr().cast::<SharedHeader>() };
        let payload = [7_u8; 64 * 36 * 4];
        unsafe {
            ptr::copy_nonoverlapping(
                payload.as_ptr(),
                mapping.view.as_ptr().add(IDD_HANDOFF_HEADER_BYTES),
                payload.len(),
            );
            let writable = mapping.view.as_ptr().cast::<SharedHeader>();
            (*writable).width = 64;
            (*writable).height = 36;
            (*writable).stride_bytes = 64 * 4;
            (*writable).pixel_format = 1;
            (*writable).payload_bytes = payload.len() as u64;
            (*writable).source_timestamp_nanos = 123;
        }
        shared.sequence_lock.store(2, Ordering::Release);

        let snapshot = mapping.try_read().unwrap().unwrap();
        assert_eq!(snapshot.header.source_timestamp_nanos, 123);
        assert_eq!(snapshot.payload, payload);
        assert!(mapping.try_read().unwrap().is_none());
    }

    #[test]
    fn backend_owns_mapping_and_is_idle_before_driver_frames() {
        let _guard = MAPPING_LOCK.lock().unwrap();
        let mut backend = IddFrameBackend::new(CaptureSize {
            width: 1280,
            height: 720,
        })
        .unwrap();
        backend.start().unwrap();
        assert_eq!(backend.poll().unwrap(), CaptureBackendPoll::Idle);
        backend.stop().unwrap();
        assert!(matches!(
            backend.poll(),
            Err(WindowsDisplayError::Backend(_))
        ));
    }

    #[test]
    fn different_monitor_identities_own_independent_mappings() {
        let _guard = MAPPING_LOCK.lock().unwrap();
        let first =
            IddMonitorIdentity::new("00112233-4455-6677-8899-aabbccddeeff".parse().unwrap())
                .unwrap();
        let second =
            IddMonitorIdentity::new("10112233-4455-6677-8899-aabbccddeeff".parse().unwrap())
                .unwrap();

        let first_mapping = IddFrameBrokerMapping::create_for_monitor(first).unwrap();
        let second_mapping = IddFrameBrokerMapping::create_for_monitor(second).unwrap();
        assert!(matches!(
            IddFrameBrokerMapping::create_for_monitor(first),
            Err(IddHandoffError::AlreadyOwned)
        ));

        drop(first_mapping);
        drop(second_mapping);
    }
}
