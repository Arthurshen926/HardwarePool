use std::{
    fmt,
    mem::size_of,
    ptr::{self, NonNull},
    sync::atomic::{AtomicI64, AtomicU32, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
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

pub const CAPTURE_SAMPLE_RATE: u32 = 48_000;
pub const CAPTURE_CHANNELS: u16 = 1;
pub const CAPTURE_FRAME_CAPACITY: usize = 16_384;
const MAGIC: u32 = 0x434f_4950;
const VERSION: u16 = 1;
const HEADER_SIZE: usize = 128;
const SAMPLE_FORMAT_FLOAT32_LE: u16 = 1;
const BYTES_PER_FRAME: usize = size_of::<f32>();
const TOTAL_SIZE: usize = HEADER_SIZE + CAPTURE_FRAME_CAPACITY * BYTES_PER_FRAME;
#[cfg(not(test))]
const MAPPING_NAME: &str = "Global\\CapyIO.CaptureRing.v1";
#[cfg(test)]
const MAPPING_NAME: &str = "Local\\CapyIO.CaptureRing.v1.test";
const MAPPING_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GRGW;;;LS)(A;;GA;;;BA)(A;;GA;;;OW)";

#[repr(C, align(64))]
struct Header {
    magic: u32,
    version: u16,
    header_size: u16,
    total_size: u32,
    frame_capacity: u32,
    bytes_per_frame: u32,
    sample_rate: u32,
    channels: u16,
    sample_format: u16,
    reserved0: u32,
    generation: u64,
    write_frame_sequence: AtomicI64,
    read_frame_sequence: AtomicI64,
    dropped_frames: AtomicI64,
    produced_frames: AtomicI64,
    consumed_frames: AtomicI64,
    underrun_frames: AtomicI64,
    producer_attach_attempts: AtomicI64,
    producer_attach_successes: AtomicI64,
    consumer_attach_attempts: AtomicI64,
    consumer_attach_successes: AtomicI64,
    last_stage: AtomicU32,
    last_error: AtomicU32,
}

const _: () = assert!(size_of::<Header>() == HEADER_SIZE);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureRingMetrics {
    pub produced_frames: u64,
    pub consumed_frames: u64,
    pub dropped_frames: u64,
    pub underrun_frames: u64,
    pub producer_attaches: u64,
    pub consumer_attaches: u64,
    pub last_stage: u32,
    pub last_error: u32,
}

#[derive(Debug, Eq, PartialEq)]
pub enum CaptureRingError {
    AlreadyOwned,
    Windows { operation: &'static str, code: u32 },
}

impl fmt::Display for CaptureRingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyOwned => formatter.write_str(
                "the CapyIO capture mapping already belongs to another service instance",
            ),
            Self::Windows { operation, code } => {
                write!(formatter, "Windows {operation} failed with error {code}")
            }
        }
    }
}

impl std::error::Error for CaptureRingError {}

/// Owns and initializes the microphone projection mapping.
///
/// APOs are the sole frame producer and consumer. The service only owns the
/// lifetime and reads bounded diagnostic counters.
#[derive(Debug)]
pub struct CaptureRingOwner {
    mapping: HANDLE,
    view: NonNull<u8>,
}

impl CaptureRingOwner {
    pub fn create_baseline() -> Result<Self, CaptureRingError> {
        let mapping_name = wide_null(MAPPING_NAME);
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

        let security_attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security_descriptor,
            bInheritHandle: 0,
        };
        let mapping = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                &security_attributes,
                PAGE_READWRITE,
                0,
                TOTAL_SIZE as u32,
                mapping_name.as_ptr(),
            )
        };
        let mapping_error = unsafe { GetLastError() };
        unsafe { LocalFree(security_descriptor) };
        if mapping.is_null() {
            return Err(CaptureRingError::Windows {
                operation: "CreateFileMappingW",
                code: mapping_error,
            });
        }
        if mapping_error == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(mapping) };
            return Err(CaptureRingError::AlreadyOwned);
        }

        let mapped = unsafe { MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, TOTAL_SIZE) };
        let Some(view) = NonNull::new(mapped.Value.cast::<u8>()) else {
            let error = last_error("MapViewOfFile");
            unsafe { CloseHandle(mapping) };
            return Err(error);
        };
        unsafe { ptr::write_bytes(view.as_ptr(), 0, TOTAL_SIZE) };

        let generation = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let header = Header {
            magic: MAGIC,
            version: VERSION,
            header_size: HEADER_SIZE as u16,
            total_size: TOTAL_SIZE as u32,
            frame_capacity: CAPTURE_FRAME_CAPACITY as u32,
            bytes_per_frame: BYTES_PER_FRAME as u32,
            sample_rate: CAPTURE_SAMPLE_RATE,
            channels: CAPTURE_CHANNELS,
            sample_format: SAMPLE_FORMAT_FLOAT32_LE,
            reserved0: 0,
            generation,
            write_frame_sequence: AtomicI64::new(0),
            read_frame_sequence: AtomicI64::new(0),
            dropped_frames: AtomicI64::new(0),
            produced_frames: AtomicI64::new(0),
            consumed_frames: AtomicI64::new(0),
            underrun_frames: AtomicI64::new(0),
            producer_attach_attempts: AtomicI64::new(0),
            producer_attach_successes: AtomicI64::new(0),
            consumer_attach_attempts: AtomicI64::new(0),
            consumer_attach_successes: AtomicI64::new(0),
            last_stage: AtomicU32::new(0),
            last_error: AtomicU32::new(0),
        };
        unsafe { ptr::write(view.as_ptr().cast::<Header>(), header) };

        Ok(Self { mapping, view })
    }

    #[must_use]
    pub fn metrics(&self) -> CaptureRingMetrics {
        let header = unsafe { &*self.view.as_ptr().cast::<Header>() };
        CaptureRingMetrics {
            produced_frames: nonnegative(header.produced_frames.load(Ordering::Relaxed)),
            consumed_frames: nonnegative(header.consumed_frames.load(Ordering::Relaxed)),
            dropped_frames: nonnegative(header.dropped_frames.load(Ordering::Relaxed)),
            underrun_frames: nonnegative(header.underrun_frames.load(Ordering::Relaxed)),
            producer_attaches: nonnegative(
                header.producer_attach_successes.load(Ordering::Relaxed),
            ),
            consumer_attaches: nonnegative(
                header.consumer_attach_successes.load(Ordering::Relaxed),
            ),
            last_stage: header.last_stage.load(Ordering::Relaxed),
            last_error: header.last_error.load(Ordering::Relaxed),
        }
    }
}

impl Drop for CaptureRingOwner {
    fn drop(&mut self) {
        unsafe {
            UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.view.as_ptr().cast(),
            });
            CloseHandle(self.mapping);
        }
    }
}

fn nonnegative(value: i64) -> u64 {
    value.max(0) as u64
}

fn last_error(operation: &'static str) -> CaptureRingError {
    CaptureRingError::Windows {
        operation,
        code: unsafe { GetLastError() },
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_MAPPING_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn baseline_layout_is_fixed_and_zeroed() {
        let _guard = TEST_MAPPING_LOCK.lock().unwrap();
        let owner = CaptureRingOwner::create_baseline().unwrap();
        assert_eq!(size_of::<Header>(), 128);
        assert_eq!(TOTAL_SIZE, 65_664);
        assert_eq!(
            owner.metrics(),
            CaptureRingMetrics {
                produced_frames: 0,
                consumed_frames: 0,
                dropped_frames: 0,
                underrun_frames: 0,
                producer_attaches: 0,
                consumer_attaches: 0,
                last_stage: 0,
                last_error: 0,
            }
        );
    }

    #[test]
    fn mapping_has_one_service_owner() {
        let _guard = TEST_MAPPING_LOCK.lock().unwrap();
        let first = CaptureRingOwner::create_baseline().unwrap();
        assert_eq!(
            CaptureRingOwner::create_baseline().unwrap_err(),
            CaptureRingError::AlreadyOwned
        );
        drop(first);
        CaptureRingOwner::create_baseline().unwrap();
    }

    #[test]
    fn diagnostic_counters_are_read_without_mutating_the_ring() {
        let _guard = TEST_MAPPING_LOCK.lock().unwrap();
        let owner = CaptureRingOwner::create_baseline().unwrap();
        let header = unsafe { &*owner.view.as_ptr().cast::<Header>() };
        header.produced_frames.store(480, Ordering::Relaxed);
        header.consumed_frames.store(240, Ordering::Relaxed);
        header.dropped_frames.store(12, Ordering::Relaxed);
        header.underrun_frames.store(24, Ordering::Relaxed);
        header.producer_attach_successes.store(1, Ordering::Relaxed);
        header.consumer_attach_successes.store(2, Ordering::Relaxed);
        assert_eq!(owner.metrics().produced_frames, 480);
        assert_eq!(owner.metrics().consumed_frames, 240);
        assert_eq!(owner.metrics().dropped_frames, 12);
        assert_eq!(owner.metrics().underrun_frames, 24);
        assert_eq!(owner.metrics().producer_attaches, 1);
        assert_eq!(owner.metrics().consumer_attaches, 2);
    }
}
