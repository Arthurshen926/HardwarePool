//! Versioned bounded Windows bridge into the `CapyIO Microphone` capture APO.

use thiserror::Error;

pub const CAPTURE_SAMPLE_RATE: u32 = 48_000;
pub const CAPTURE_CHANNELS: u16 = 1;
pub const CAPTURE_FRAME_CAPACITY: usize = 16_384;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureWriteOutcome {
    Committed { frames: usize },
    DroppedFull { frames: usize },
}

#[derive(Debug, Error)]
pub enum CaptureRingError {
    #[error("the CapyIO capture mapping already belongs to another service instance")]
    AlreadyOwned,
    #[error("the CapyIO capture mapping is unavailable")]
    MappingUnavailable,
    #[error("the capture ring contains an invalid bounded layout")]
    InvalidLayout,
    #[error("PCM must be non-empty aligned mono S16LE within the capture-ring bound")]
    InvalidPcmBlock,
    #[error("Windows {operation} failed with error {code}")]
    Windows { operation: &'static str, code: u32 },
}

#[cfg(windows)]
mod windows {
    use std::{
        io,
        mem::size_of,
        ptr::{self, NonNull},
        sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_ALREADY_EXISTS, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
            WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
        },
        System::{
            Memory::{
                CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile,
                OpenFileMappingW, PAGE_READWRITE, UnmapViewOfFile,
            },
            Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
        },
    };

    use super::{
        CAPTURE_CHANNELS, CAPTURE_FRAME_CAPACITY, CAPTURE_SAMPLE_RATE, CaptureRingError,
        CaptureRingMetrics, CaptureWriteOutcome,
    };

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
    #[cfg(not(test))]
    const OWNER_MUTEX_NAME: &str = "Global\\CapyIO.CaptureRing.Owner.v1";
    #[cfg(test)]
    const OWNER_MUTEX_NAME: &str = "Local\\CapyIO.CaptureRing.Owner.v1.test";
    const MAPPING_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GRGW;;;LS)(A;;GA;;;BA)(A;;GA;;;OW)";
    static PROCESS_OWNER_CLAIMED: AtomicBool = AtomicBool::new(false);

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

    struct ProcessOwnerClaim;

    impl ProcessOwnerClaim {
        fn acquire() -> Result<Self, CaptureRingError> {
            PROCESS_OWNER_CLAIMED
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .map(|_| Self)
                .map_err(|_| CaptureRingError::AlreadyOwned)
        }
    }

    impl Drop for ProcessOwnerClaim {
        fn drop(&mut self) {
            PROCESS_OWNER_CLAIMED.store(false, Ordering::Release);
        }
    }

    /// Owns and initializes the microphone projection mapping.
    pub struct CaptureRingOwner {
        _process_owner: ProcessOwnerClaim,
        owner_mutex: HANDLE,
        mapping: HANDLE,
        view: NonNull<u8>,
    }

    impl CaptureRingOwner {
        pub fn create_baseline() -> Result<Self, CaptureRingError> {
            let process_owner = ProcessOwnerClaim::acquire()?;
            let mapping_name = wide_null(MAPPING_NAME);
            let mutex_name = wide_null(OWNER_MUTEX_NAME);
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
            let owner_mutex = unsafe { CreateMutexW(&security_attributes, 1, mutex_name.as_ptr()) };
            let mutex_error = last_error_code();
            if owner_mutex.is_null() {
                unsafe { LocalFree(security_descriptor) };
                return Err(CaptureRingError::Windows {
                    operation: "CreateMutexW",
                    code: mutex_error,
                });
            }
            if mutex_error == ERROR_ALREADY_EXISTS {
                let wait = unsafe { WaitForSingleObject(owner_mutex, 0) };
                if wait == WAIT_TIMEOUT {
                    unsafe {
                        CloseHandle(owner_mutex);
                        LocalFree(security_descriptor);
                    }
                    return Err(CaptureRingError::AlreadyOwned);
                }
                if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
                    let error = last_error("WaitForSingleObject");
                    unsafe {
                        CloseHandle(owner_mutex);
                        LocalFree(security_descriptor);
                    }
                    return Err(error);
                }
            }
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
            let mapping_error = last_error_code();
            unsafe { LocalFree(security_descriptor) };
            if mapping.is_null() {
                unsafe {
                    ReleaseMutex(owner_mutex);
                    CloseHandle(owner_mutex);
                }
                return Err(CaptureRingError::Windows {
                    operation: "CreateFileMappingW",
                    code: mapping_error,
                });
            }
            let mapped = unsafe { MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, TOTAL_SIZE) };
            let Some(view) = NonNull::new(mapped.Value.cast::<u8>()) else {
                let error = last_error("MapViewOfFile");
                unsafe {
                    CloseHandle(mapping);
                    ReleaseMutex(owner_mutex);
                    CloseHandle(owner_mutex);
                }
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
            Ok(Self {
                _process_owner: process_owner,
                owner_mutex,
                mapping,
                view,
            })
        }

        #[must_use]
        pub fn metrics(&self) -> CaptureRingMetrics {
            metrics(unsafe { &*self.view.as_ptr().cast::<Header>() })
        }
    }

    impl Drop for CaptureRingOwner {
        fn drop(&mut self) {
            unsafe {
                UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.view.as_ptr().cast(),
                });
                CloseHandle(self.mapping);
                ReleaseMutex(self.owner_mutex);
                CloseHandle(self.owner_mutex);
            }
        }
    }

    /// Attaches one user-mode PCM producer to the existing service-owned ring.
    pub struct CaptureRingProducer {
        mapping: HANDLE,
        view: NonNull<u8>,
        generation: u64,
    }

    impl CaptureRingProducer {
        pub fn attach() -> Result<Self, CaptureRingError> {
            let mapping_name = wide_null(MAPPING_NAME);
            let mapping =
                unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, mapping_name.as_ptr()) };
            if mapping.is_null() {
                return Err(CaptureRingError::MappingUnavailable);
            }
            let mapped = unsafe { MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, TOTAL_SIZE) };
            let Some(view) = NonNull::new(mapped.Value.cast::<u8>()) else {
                let error = last_error("MapViewOfFile");
                unsafe { CloseHandle(mapping) };
                return Err(error);
            };
            let header = unsafe { &*view.as_ptr().cast::<Header>() };
            header
                .producer_attach_attempts
                .fetch_add(1, Ordering::Relaxed);
            header.last_stage.store(101, Ordering::Relaxed);
            header.last_error.store(0, Ordering::Relaxed);
            if !valid_header(header) {
                header.last_stage.store(102, Ordering::Relaxed);
                header.last_error.store(13, Ordering::Relaxed);
                unsafe {
                    UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                        Value: view.as_ptr().cast(),
                    });
                    CloseHandle(mapping);
                }
                return Err(CaptureRingError::InvalidLayout);
            }
            header
                .producer_attach_successes
                .fetch_add(1, Ordering::Relaxed);
            header.last_stage.store(103, Ordering::Relaxed);
            Ok(Self {
                mapping,
                view,
                generation: header.generation,
            })
        }

        pub fn try_write_s16le_mono(
            &mut self,
            pcm: &[u8],
        ) -> Result<CaptureWriteOutcome, CaptureRingError> {
            if pcm.is_empty()
                || !pcm.len().is_multiple_of(size_of::<i16>())
                || pcm.len() / size_of::<i16>() > CAPTURE_FRAME_CAPACITY
            {
                return Err(CaptureRingError::InvalidPcmBlock);
            }
            let header = unsafe { &*self.view.as_ptr().cast::<Header>() };
            if !valid_header(header) || header.generation != self.generation {
                return Err(CaptureRingError::InvalidLayout);
            }
            let frames = pcm.len() / size_of::<i16>();
            let write = header.write_frame_sequence.load(Ordering::Relaxed);
            let read = header.read_frame_sequence.load(Ordering::Acquire);
            if write < 0 || read < 0 || write < read || write - read > CAPTURE_FRAME_CAPACITY as i64
            {
                return Err(CaptureRingError::InvalidLayout);
            }
            let used =
                usize::try_from(write - read).map_err(|_| CaptureRingError::InvalidLayout)?;
            if frames > CAPTURE_FRAME_CAPACITY - used {
                header
                    .dropped_frames
                    .fetch_add(frames as i64, Ordering::Relaxed);
                return Ok(CaptureWriteOutcome::DroppedFull { frames });
            }
            let output = unsafe { self.view.as_ptr().add(HEADER_SIZE).cast::<f32>() };
            for (index, bytes) in pcm.chunks_exact(2).enumerate() {
                let sample = i16::from_le_bytes([bytes[0], bytes[1]]);
                let value = f32::from(sample) / 32_768.0;
                let destination = (write as usize + index) % CAPTURE_FRAME_CAPACITY;
                unsafe { ptr::write(output.add(destination), value) };
            }
            header
                .write_frame_sequence
                .store(write + frames as i64, Ordering::Release);
            header
                .produced_frames
                .fetch_add(frames as i64, Ordering::Relaxed);
            Ok(CaptureWriteOutcome::Committed { frames })
        }

        #[must_use]
        pub fn metrics(&self) -> CaptureRingMetrics {
            metrics(unsafe { &*self.view.as_ptr().cast::<Header>() })
        }
    }

    impl Drop for CaptureRingProducer {
        fn drop(&mut self) {
            unsafe {
                UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.view.as_ptr().cast(),
                });
                CloseHandle(self.mapping);
            }
        }
    }

    fn valid_header(header: &Header) -> bool {
        header.magic == MAGIC
            && header.version == VERSION
            && usize::from(header.header_size) == HEADER_SIZE
            && header.total_size as usize == TOTAL_SIZE
            && header.frame_capacity as usize == CAPTURE_FRAME_CAPACITY
            && header.bytes_per_frame as usize == BYTES_PER_FRAME
            && header.sample_rate == CAPTURE_SAMPLE_RATE
            && header.channels == CAPTURE_CHANNELS
            && header.sample_format == SAMPLE_FORMAT_FLOAT32_LE
            && header.reserved0 == 0
    }

    fn metrics(header: &Header) -> CaptureRingMetrics {
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

    fn nonnegative(value: i64) -> u64 {
        value.max(0) as u64
    }

    fn last_error(operation: &'static str) -> CaptureRingError {
        CaptureRingError::Windows {
            operation,
            code: last_error_code(),
        }
    }

    fn last_error_code() -> u32 {
        io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_default() as u32
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
        fn mapping_has_one_live_service_owner_and_reclaims_stale_mapping() {
            let _guard = TEST_MAPPING_LOCK.lock().unwrap();
            let first = CaptureRingOwner::create_baseline().unwrap();
            assert!(matches!(
                CaptureRingOwner::create_baseline(),
                Err(CaptureRingError::AlreadyOwned)
            ));
            let mapping_name = wide_null(MAPPING_NAME);
            let observer =
                unsafe { OpenFileMappingW(FILE_MAP_ALL_ACCESS, 0, mapping_name.as_ptr()) };
            assert!(!observer.is_null());
            drop(first);
            let replacement = CaptureRingOwner::create_baseline().unwrap();
            assert_eq!(replacement.metrics().produced_frames, 0);
            drop(replacement);
            unsafe { CloseHandle(observer) };
        }

        #[test]
        fn producer_converts_s16le_and_commits_complete_blocks() {
            let _guard = TEST_MAPPING_LOCK.lock().unwrap();
            let owner = CaptureRingOwner::create_baseline().unwrap();
            let mut producer = CaptureRingProducer::attach().unwrap();
            let pcm = [i16::MIN, -16_384, 0, 16_384, i16::MAX]
                .into_iter()
                .flat_map(i16::to_le_bytes)
                .collect::<Vec<_>>();
            assert_eq!(
                producer.try_write_s16le_mono(&pcm).unwrap(),
                CaptureWriteOutcome::Committed { frames: 5 }
            );
            let header = unsafe { &*owner.view.as_ptr().cast::<Header>() };
            let frames = unsafe {
                std::slice::from_raw_parts(owner.view.as_ptr().add(HEADER_SIZE).cast::<f32>(), 5)
            };
            assert_eq!(frames, [-1.0, -0.5, 0.0, 0.5, 32_767.0 / 32_768.0]);
            assert_eq!(header.write_frame_sequence.load(Ordering::Acquire), 5);
            assert_eq!(producer.metrics().produced_frames, 5);
            assert_eq!(producer.metrics().producer_attaches, 1);
        }

        #[test]
        fn full_ring_drops_whole_block_and_invalid_pcm_is_rejected() {
            let _guard = TEST_MAPPING_LOCK.lock().unwrap();
            let owner = CaptureRingOwner::create_baseline().unwrap();
            let mut producer = CaptureRingProducer::attach().unwrap();
            let header = unsafe { &*owner.view.as_ptr().cast::<Header>() };
            header
                .write_frame_sequence
                .store(CAPTURE_FRAME_CAPACITY as i64, Ordering::Release);
            assert_eq!(
                producer.try_write_s16le_mono(&[0, 0]).unwrap(),
                CaptureWriteOutcome::DroppedFull { frames: 1 }
            );
            assert_eq!(producer.metrics().dropped_frames, 1);
            assert!(matches!(
                producer.try_write_s16le_mono(&[0]),
                Err(CaptureRingError::InvalidPcmBlock)
            ));
        }
    }
}

#[cfg(windows)]
pub use windows::{CaptureRingOwner, CaptureRingProducer};
