#![cfg_attr(not(windows), forbid(unsafe_code))]
#![cfg(windows)]

//! Versioned Windows shared-memory boundary for decoded camera frames.

use std::{
    error::Error,
    fmt, io,
    mem::size_of,
    ptr::{self, NonNull},
    sync::atomic::{AtomicU64, Ordering, fence},
    time::{SystemTime, UNIX_EPOCH},
};

use capyio_core::StreamId;
use capyio_video::{VideoFrameDescriptor, VideoFrameFlags};
use capyio_windows_camera::{
    ExternalNv12FrameIngress, ExternalNv12FrameIngressError, GeneratedVideoFrame,
    fixture_stream_spec,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
    },
    Security::{
        Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1},
        PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
    },
    System::{
        Memory::{
            CreateFileMappingW, FILE_MAP_ALL_ACCESS, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS,
            MapViewOfFile, OpenFileMappingW, PAGE_READWRITE, UnmapViewOfFile,
        },
        Threading::GetCurrentProcessId,
    },
};

const CAMERA_SHARED_INGRESS_MAGIC: u32 = 0x4341_4D49;
pub const CAMERA_SHARED_INGRESS_VERSION: u16 = 1;
pub const CAMERA_SHARED_INGRESS_SLOT_COUNT: usize = 3;
const CAMERA_SHARED_INGRESS_HEADER_BYTES: usize = 256;
const CAMERA_SHARED_INGRESS_SLOT_HEADER_BYTES: usize = 64;
const CAMERA_SHARED_INGRESS_PAYLOAD_BYTES: usize = 1_382_400;
const CAMERA_SHARED_INGRESS_SLOT_BYTES: usize =
    CAMERA_SHARED_INGRESS_SLOT_HEADER_BYTES + CAMERA_SHARED_INGRESS_PAYLOAD_BYTES;
pub const CAMERA_SHARED_INGRESS_MAPPING_BYTES: usize = CAMERA_SHARED_INGRESS_HEADER_BYTES
    + CAMERA_SHARED_INGRESS_SLOT_COUNT * CAMERA_SHARED_INGRESS_SLOT_BYTES;
pub const CAMERA_SHARED_INGRESS_MAPPING_NAME: &str = "Global\\CapyIO.CameraIngress.v1";
#[cfg(feature = "lab-support")]
pub const CAMERA_SHARED_INGRESS_LOCAL_LAB_MAPPING_NAME: &str = "Local\\CapyIO.CameraIngress.v1.lab";
const CAMERA_SHARED_INGRESS_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GR;;;LS)(A;;GA;;;BA)(A;;GA;;;OW)";
const PIXEL_FORMAT_NV12: u32 = 1;
const COLORIMETRY_BT709_LIMITED: u32 = 1;
const FRAME_FLAG_DISCONTINUITY: u32 = 1;
#[cfg(feature = "test-support")]
const TEST_MAPPING_PREFIX: &str = "Local\\CapyIO.CameraIngress.v1.test.";
#[cfg(feature = "test-support")]
const MAX_TEST_MAPPING_NAME_UTF16: usize = 240;

#[repr(C, align(64))]
struct SharedIngressHeader {
    magic: u32,
    version: u16,
    header_bytes: u16,
    total_bytes: u32,
    slot_count: u32,
    slot_bytes: u32,
    payload_bytes: u32,
    width: u32,
    height: u32,
    frame_rate_numerator: u32,
    frame_rate_denominator: u32,
    pixel_format: u32,
    colorimetry: u32,
    producer_process_id: u32,
    reserved_u32: u32,
    stream_epoch: u64,
    generation: u64,
    published_sequence: AtomicU64,
    produced_frames: AtomicU64,
    stream_id: [u8; 16],
    reserved: [u8; 152],
}

const _: () = assert!(size_of::<SharedIngressHeader>() == CAMERA_SHARED_INGRESS_HEADER_BYTES);

#[repr(C, align(64))]
struct SharedIngressSlotHeader {
    committed_publication: AtomicU64,
    generation: u64,
    sequence: u64,
    source_timestamp_nanos: u64,
    duration_nanos: u64,
    payload_bytes: u32,
    flags: u32,
    reserved: [u8; 16],
}

const _: () =
    assert!(size_of::<SharedIngressSlotHeader>() == CAMERA_SHARED_INGRESS_SLOT_HEADER_BYTES);

pub struct CameraSharedIngressProducer {
    mapping: HANDLE,
    view: NonNull<u8>,
    stream_id: StreamId,
    stream_epoch: u64,
    generation: u64,
    validator: ExternalNv12FrameIngress,
}

// SAFETY: the mapping handle and writable view are exclusively owned by this
// value, Windows file-mapping handles/views have no thread affinity, and all
// mutation requires `&mut self`. The type is intentionally not `Sync`.
unsafe impl Send for CameraSharedIngressProducer {}

impl CameraSharedIngressProducer {
    pub fn create(
        stream_id: StreamId,
        stream_epoch: u64,
    ) -> Result<Self, CameraSharedIngressError> {
        Self::create_named(CAMERA_SHARED_INGRESS_MAPPING_NAME, stream_id, stream_epoch)
    }

    /// Creates the fixed current-session mapping used only by the explicit
    /// camera integration lab. It cannot accept a caller-controlled name.
    #[cfg(feature = "lab-support")]
    pub fn create_local_lab(
        stream_id: StreamId,
        stream_epoch: u64,
    ) -> Result<Self, CameraSharedIngressError> {
        Self::create_named(
            CAMERA_SHARED_INGRESS_LOCAL_LAB_MAPPING_NAME,
            stream_id,
            stream_epoch,
        )
    }

    fn create_named(
        mapping_name: &str,
        stream_id: StreamId,
        stream_epoch: u64,
    ) -> Result<Self, CameraSharedIngressError> {
        let validator = ExternalNv12FrameIngress::new(stream_id, stream_epoch, 1)?;
        let mapping_name = wide_null(mapping_name);
        let mapping_sddl = wide_null(CAMERA_SHARED_INGRESS_SDDL);
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
                u32::try_from(CAMERA_SHARED_INGRESS_MAPPING_BYTES)
                    .expect("the fixed mapping fits u32"),
                mapping_name.as_ptr(),
            )
        };
        let mapping_error = unsafe { GetLastError() };
        unsafe { LocalFree(security_descriptor) };
        if mapping.is_null() {
            return Err(CameraSharedIngressError::Windows {
                operation: "CreateFileMappingW",
                code: mapping_error,
            });
        }
        if mapping_error == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(mapping) };
            return Err(CameraSharedIngressError::AlreadyOwned);
        }

        let mapped = unsafe {
            MapViewOfFile(
                mapping,
                FILE_MAP_ALL_ACCESS,
                0,
                0,
                CAMERA_SHARED_INGRESS_MAPPING_BYTES,
            )
        };
        let Some(view) = NonNull::new(mapped.Value.cast::<u8>()) else {
            let error = last_error("MapViewOfFile");
            unsafe { CloseHandle(mapping) };
            return Err(error);
        };

        unsafe { ptr::write_bytes(view.as_ptr(), 0, CAMERA_SHARED_INGRESS_MAPPING_BYTES) };
        let generation = mapping_generation();
        let selected = fixture_stream_spec();
        let header = SharedIngressHeader {
            magic: CAMERA_SHARED_INGRESS_MAGIC,
            version: CAMERA_SHARED_INGRESS_VERSION,
            header_bytes: CAMERA_SHARED_INGRESS_HEADER_BYTES as u16,
            total_bytes: CAMERA_SHARED_INGRESS_MAPPING_BYTES as u32,
            slot_count: CAMERA_SHARED_INGRESS_SLOT_COUNT as u32,
            slot_bytes: CAMERA_SHARED_INGRESS_SLOT_BYTES as u32,
            payload_bytes: CAMERA_SHARED_INGRESS_PAYLOAD_BYTES as u32,
            width: selected.width,
            height: selected.height,
            frame_rate_numerator: selected.frame_rate.numerator(),
            frame_rate_denominator: selected.frame_rate.denominator(),
            pixel_format: PIXEL_FORMAT_NV12,
            colorimetry: COLORIMETRY_BT709_LIMITED,
            producer_process_id: unsafe { GetCurrentProcessId() },
            reserved_u32: 0,
            stream_epoch,
            generation,
            published_sequence: AtomicU64::new(0),
            produced_frames: AtomicU64::new(0),
            stream_id: *stream_id.as_uuid().as_bytes(),
            reserved: [0; 152],
        };
        unsafe { ptr::write(view.as_ptr().cast::<SharedIngressHeader>(), header) };
        for index in 0..CAMERA_SHARED_INGRESS_SLOT_COUNT {
            let slot = unsafe { view.as_ptr().add(slot_offset(index)) };
            unsafe {
                ptr::write(
                    slot.cast::<SharedIngressSlotHeader>(),
                    SharedIngressSlotHeader {
                        committed_publication: AtomicU64::new(0),
                        generation,
                        sequence: 0,
                        source_timestamp_nanos: 0,
                        duration_nanos: 0,
                        payload_bytes: 0,
                        flags: 0,
                        reserved: [0; 16],
                    },
                )
            };
        }

        Ok(Self {
            mapping,
            view,
            stream_id,
            stream_epoch,
            generation,
            validator,
        })
    }

    /// Creates an isolated local mapping for cross-crate process tests.
    ///
    /// Production builds do not enable this feature, and the supplied name is
    /// restricted to the bounded CapyIO test namespace.
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn create_local_test(
        mapping_name: &str,
        stream_id: StreamId,
        stream_epoch: u64,
    ) -> Result<Self, CameraSharedIngressError> {
        validate_test_mapping_name(mapping_name)?;
        Self::create_named(mapping_name, stream_id, stream_epoch)
    }

    pub fn publish(&mut self, frame: GeneratedVideoFrame) -> Result<u64, CameraSharedIngressError> {
        self.validator.push(frame)?;
        let frame = self
            .validator
            .pop()
            .expect("a successfully validated capacity-one ingress contains its frame");
        let header = unsafe { &*self.view.as_ptr().cast::<SharedIngressHeader>() };
        if !valid_header(header, self.stream_id, self.stream_epoch, self.generation) {
            return Err(CameraSharedIngressError::InvalidLayout);
        }
        let previous = header.published_sequence.load(Ordering::Acquire);
        let publication = previous
            .checked_add(1)
            .ok_or(CameraSharedIngressError::PublicationExhausted)?;
        let index = usize::try_from(publication - 1)
            .map_err(|_| CameraSharedIngressError::InvalidLayout)?
            % CAMERA_SHARED_INGRESS_SLOT_COUNT;
        let slot = unsafe { self.view.as_ptr().add(slot_offset(index)) };
        let slot_header = unsafe { &mut *slot.cast::<SharedIngressSlotHeader>() };
        slot_header
            .committed_publication
            .store(0, Ordering::Release);
        unsafe {
            ptr::write_volatile(&mut slot_header.generation, self.generation);
            ptr::write_volatile(&mut slot_header.sequence, frame.descriptor.sequence);
            ptr::write_volatile(
                &mut slot_header.source_timestamp_nanos,
                frame.descriptor.source_timestamp_nanos,
            );
            ptr::write_volatile(
                &mut slot_header.duration_nanos,
                frame.descriptor.duration_nanos,
            );
            ptr::write_volatile(
                &mut slot_header.payload_bytes,
                u32::try_from(frame.payload.len()).expect("validated payload length fits u32"),
            );
            ptr::write_volatile(
                &mut slot_header.flags,
                u32::from(frame.descriptor.flags.discontinuity) * FRAME_FLAG_DISCONTINUITY,
            );
            ptr::copy_nonoverlapping(
                frame.payload.as_ptr(),
                slot.add(CAMERA_SHARED_INGRESS_SLOT_HEADER_BYTES),
                frame.payload.len(),
            );
        }
        slot_header
            .committed_publication
            .store(publication, Ordering::Release);
        header.produced_frames.fetch_add(1, Ordering::Relaxed);
        header
            .published_sequence
            .store(publication, Ordering::Release);
        Ok(publication)
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
}

impl Drop for CameraSharedIngressProducer {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.view.as_ptr().cast(),
            });
            CloseHandle(self.mapping);
        }
    }
}

pub struct CameraSharedIngressConsumer {
    mapping: HANDLE,
    view: NonNull<u8>,
    stream_id: StreamId,
    stream_epoch: u64,
    generation: u64,
    last_publication: u64,
    last_frame_sequence: Option<u64>,
    last_source_timestamp_nanos: Option<u64>,
}

// SAFETY: the mapping handle and read-only view are exclusively owned by this
// value, Windows file-mapping handles/views have no thread affinity, and the
// per-reader cursor can only change through `&mut self`. The type is not `Sync`.
unsafe impl Send for CameraSharedIngressConsumer {}

impl CameraSharedIngressConsumer {
    pub fn open(stream_id: StreamId, stream_epoch: u64) -> Result<Self, CameraSharedIngressError> {
        Self::open_named(CAMERA_SHARED_INGRESS_MAPPING_NAME, stream_id, stream_epoch)
    }

    /// Opens the production mapping and adopts the stream identity/epoch that
    /// were fixed by its ACL-protected producer header.
    pub fn open_current() -> Result<Self, CameraSharedIngressError> {
        Self::open_named_current(CAMERA_SHARED_INGRESS_MAPPING_NAME)
    }

    /// Opens the fixed current-session mapping used only by the explicit
    /// camera integration lab and adopts its validated identity.
    #[cfg(feature = "lab-support")]
    pub fn open_local_lab_current() -> Result<Self, CameraSharedIngressError> {
        Self::open_named_current(CAMERA_SHARED_INGRESS_LOCAL_LAB_MAPPING_NAME)
    }

    fn open_named(
        mapping_name: &str,
        stream_id: StreamId,
        stream_epoch: u64,
    ) -> Result<Self, CameraSharedIngressError> {
        let consumer = Self::open_named_current(mapping_name)?;
        if consumer.stream_id != stream_id || consumer.stream_epoch != stream_epoch {
            return Err(CameraSharedIngressError::InvalidLayout);
        }
        Ok(consumer)
    }

    fn open_named_current(mapping_name: &str) -> Result<Self, CameraSharedIngressError> {
        let mapping_name = wide_null(mapping_name);
        let mapping = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, mapping_name.as_ptr()) };
        if mapping.is_null() {
            return Err(last_error("OpenFileMappingW"));
        }
        let mapped = unsafe {
            MapViewOfFile(
                mapping,
                FILE_MAP_READ,
                0,
                0,
                CAMERA_SHARED_INGRESS_MAPPING_BYTES,
            )
        };
        let Some(view) = NonNull::new(mapped.Value.cast::<u8>()) else {
            let error = last_error("MapViewOfFile");
            unsafe { CloseHandle(mapping) };
            return Err(error);
        };
        let header = unsafe { &*view.as_ptr().cast::<SharedIngressHeader>() };
        let stream_id = StreamId::from_uuid(uuid::Uuid::from_bytes(header.stream_id));
        let stream_epoch = header.stream_epoch;
        let generation = header.generation;
        if !valid_header(header, stream_id, stream_epoch, generation) {
            unsafe {
                let _ = UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: view.as_ptr().cast(),
                });
                CloseHandle(mapping);
            }
            return Err(CameraSharedIngressError::InvalidLayout);
        }
        Ok(Self {
            mapping,
            view,
            stream_id,
            stream_epoch,
            generation,
            last_publication: 0,
            last_frame_sequence: None,
            last_source_timestamp_nanos: None,
        })
    }

    /// Opens an isolated local mapping for cross-crate process tests.
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn open_local_test(
        mapping_name: &str,
        stream_id: StreamId,
        stream_epoch: u64,
    ) -> Result<Self, CameraSharedIngressError> {
        validate_test_mapping_name(mapping_name)?;
        Self::open_named(mapping_name, stream_id, stream_epoch)
    }

    /// Opens a local test mapping and adopts the producer-bound identity.
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn open_local_test_current(mapping_name: &str) -> Result<Self, CameraSharedIngressError> {
        validate_test_mapping_name(mapping_name)?;
        Self::open_named_current(mapping_name)
    }

    pub fn try_read_latest(
        &mut self,
    ) -> Result<Option<GeneratedVideoFrame>, CameraSharedIngressError> {
        let header = unsafe { &*self.view.as_ptr().cast::<SharedIngressHeader>() };
        if !valid_header(header, self.stream_id, self.stream_epoch, self.generation) {
            return Err(CameraSharedIngressError::InvalidLayout);
        }
        let publication = header.published_sequence.load(Ordering::Acquire);
        if publication == self.last_publication {
            return Ok(None);
        }
        if publication < self.last_publication {
            return Err(CameraSharedIngressError::InvalidLayout);
        }
        let index = usize::try_from(publication - 1)
            .map_err(|_| CameraSharedIngressError::InvalidLayout)?
            % CAMERA_SHARED_INGRESS_SLOT_COUNT;
        let slot = unsafe { self.view.as_ptr().add(slot_offset(index)) };
        let slot_header = unsafe { &*slot.cast::<SharedIngressSlotHeader>() };
        if slot_header.committed_publication.load(Ordering::Acquire) != publication {
            return Ok(None);
        }
        let generation = unsafe { ptr::read_volatile(&slot_header.generation) };
        let sequence = unsafe { ptr::read_volatile(&slot_header.sequence) };
        let source_timestamp_nanos =
            unsafe { ptr::read_volatile(&slot_header.source_timestamp_nanos) };
        let duration_nanos = unsafe { ptr::read_volatile(&slot_header.duration_nanos) };
        let payload_bytes = unsafe { ptr::read_volatile(&slot_header.payload_bytes) };
        let flags = unsafe { ptr::read_volatile(&slot_header.flags) };
        if generation != self.generation
            || payload_bytes as usize != CAMERA_SHARED_INGRESS_PAYLOAD_BYTES
            || flags & !FRAME_FLAG_DISCONTINUITY != 0
        {
            return Err(CameraSharedIngressError::InvalidLayout);
        }
        let mut payload = vec![0_u8; CAMERA_SHARED_INGRESS_PAYLOAD_BYTES];
        unsafe {
            ptr::copy_nonoverlapping(
                slot.add(CAMERA_SHARED_INGRESS_SLOT_HEADER_BYTES),
                payload.as_mut_ptr(),
                payload.len(),
            );
        }
        fence(Ordering::Acquire);
        if slot_header.committed_publication.load(Ordering::Acquire) != publication {
            return Ok(None);
        }
        if self
            .last_frame_sequence
            .is_some_and(|previous| sequence <= previous)
            || self
                .last_source_timestamp_nanos
                .is_some_and(|previous| source_timestamp_nanos <= previous)
        {
            return Err(CameraSharedIngressError::InvalidLayout);
        }
        let skipped = self.last_publication != 0 && publication > self.last_publication + 1;
        let frame = GeneratedVideoFrame {
            descriptor: VideoFrameDescriptor {
                stream_id: self.stream_id,
                stream_epoch: self.stream_epoch,
                sequence,
                source_timestamp_nanos,
                duration_nanos,
                payload_bytes: u64::from(payload_bytes),
                flags: VideoFrameFlags {
                    discontinuity: flags & FRAME_FLAG_DISCONTINUITY != 0 || skipped,
                    end_of_stream: false,
                },
            },
            payload,
        };
        frame
            .validate(&fixture_stream_spec())
            .map_err(ExternalNv12FrameIngressError::from)?;
        self.last_publication = publication;
        self.last_frame_sequence = Some(sequence);
        self.last_source_timestamp_nanos = Some(source_timestamp_nanos);
        Ok(Some(frame))
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    #[must_use]
    pub const fn stream_epoch(&self) -> u64 {
        self.stream_epoch
    }
}

impl Drop for CameraSharedIngressConsumer {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.view.as_ptr().cast(),
            });
            CloseHandle(self.mapping);
        }
    }
}

#[derive(Debug)]
pub enum CameraSharedIngressError {
    AlreadyOwned,
    InvalidMappingName,
    InvalidLayout,
    PublicationExhausted,
    Windows { operation: &'static str, code: u32 },
    Frame(ExternalNv12FrameIngressError),
}

impl fmt::Display for CameraSharedIngressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyOwned => {
                formatter.write_str("the camera ingress mapping already has an owner")
            }
            Self::InvalidMappingName => {
                formatter.write_str("the camera ingress mapping name is outside the test namespace")
            }
            Self::InvalidLayout => {
                formatter.write_str("the camera ingress mapping has an invalid bounded layout")
            }
            Self::PublicationExhausted => {
                formatter.write_str("the camera ingress publication sequence is exhausted")
            }
            Self::Windows { operation, code } => {
                write!(formatter, "Windows {operation} failed with error {code}")
            }
            Self::Frame(error) => error.fmt(formatter),
        }
    }
}

impl Error for CameraSharedIngressError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Frame(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ExternalNv12FrameIngressError> for CameraSharedIngressError {
    fn from(value: ExternalNv12FrameIngressError) -> Self {
        Self::Frame(value)
    }
}

fn valid_header(
    header: &SharedIngressHeader,
    stream_id: StreamId,
    stream_epoch: u64,
    generation: u64,
) -> bool {
    let selected = fixture_stream_spec();
    header.magic == CAMERA_SHARED_INGRESS_MAGIC
        && header.version == CAMERA_SHARED_INGRESS_VERSION
        && usize::from(header.header_bytes) == CAMERA_SHARED_INGRESS_HEADER_BYTES
        && header.total_bytes as usize == CAMERA_SHARED_INGRESS_MAPPING_BYTES
        && header.slot_count as usize == CAMERA_SHARED_INGRESS_SLOT_COUNT
        && header.slot_bytes as usize == CAMERA_SHARED_INGRESS_SLOT_BYTES
        && header.payload_bytes as usize == CAMERA_SHARED_INGRESS_PAYLOAD_BYTES
        && header.width == selected.width
        && header.height == selected.height
        && header.frame_rate_numerator == selected.frame_rate.numerator()
        && header.frame_rate_denominator == selected.frame_rate.denominator()
        && header.pixel_format == PIXEL_FORMAT_NV12
        && header.colorimetry == COLORIMETRY_BT709_LIMITED
        && header.producer_process_id != 0
        && header.reserved_u32 == 0
        && header.stream_epoch == stream_epoch
        && header.generation == generation
        && generation != 0
        && header.stream_id == *stream_id.as_uuid().as_bytes()
        && header.reserved.iter().all(|byte| *byte == 0)
}

const fn slot_offset(index: usize) -> usize {
    CAMERA_SHARED_INGRESS_HEADER_BYTES + index * CAMERA_SHARED_INGRESS_SLOT_BYTES
}

fn mapping_generation() -> u64 {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let process = u64::from(unsafe { GetCurrentProcessId() });
    let generation = time ^ process.rotate_left(32);
    generation.max(1)
}

fn last_error(operation: &'static str) -> CameraSharedIngressError {
    let code = io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_default() as u32;
    CameraSharedIngressError::Windows { operation, code }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[cfg(feature = "test-support")]
fn validate_test_mapping_name(mapping_name: &str) -> Result<(), CameraSharedIngressError> {
    let valid = mapping_name.starts_with(TEST_MAPPING_PREFIX)
        && mapping_name.encode_utf16().count() <= MAX_TEST_MAPPING_NAME_UTF16
        && !mapping_name
            .chars()
            .any(|character| character == '\0' || character.is_control());
    if valid {
        Ok(())
    } else {
        Err(CameraSharedIngressError::InvalidMappingName)
    }
}

#[cfg(test)]
mod tests {
    use std::{process::Command, str::FromStr, sync::atomic::AtomicU32};

    use capyio_windows_camera::DeterministicNv12Source;

    use super::*;

    const TEST_STREAM: &str = "00000000-0000-4000-8000-00000000c015";
    const CHILD_FLAG: &str = "CAPYIO_CAMERA_SHARED_INGRESS_CHILD";
    const CHILD_MAPPING: &str = "CAPYIO_CAMERA_SHARED_INGRESS_TEST_MAPPING";
    static TEST_NAME_SEQUENCE: AtomicU32 = AtomicU32::new(0);

    fn test_stream_id() -> StreamId {
        StreamId::from_str(TEST_STREAM).expect("fixed stream id")
    }

    fn test_mapping_name(label: &str) -> String {
        let sequence = TEST_NAME_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        format!(
            "Local\\CapyIO.CameraIngress.v1.test.{}.{}.{}",
            unsafe { GetCurrentProcessId() },
            label,
            sequence
        )
    }

    fn source() -> DeterministicNv12Source {
        DeterministicNv12Source::new(test_stream_id(), 23, 9_000_000_000).expect("valid source")
    }

    #[test]
    fn layout_is_fixed_and_only_one_producer_can_own_a_name() {
        assert_eq!(size_of::<SharedIngressHeader>(), 256);
        assert_eq!(size_of::<SharedIngressSlotHeader>(), 64);
        assert_eq!(CAMERA_SHARED_INGRESS_SLOT_BYTES, 1_382_464);
        assert_eq!(CAMERA_SHARED_INGRESS_MAPPING_BYTES, 4_147_648);
        assert_eq!(
            CAMERA_SHARED_INGRESS_SDDL,
            "D:P(A;;GA;;;SY)(A;;GR;;;LS)(A;;GA;;;BA)(A;;GA;;;OW)"
        );

        let name = test_mapping_name("owner");
        let producer = CameraSharedIngressProducer::create_named(&name, test_stream_id(), 23)
            .expect("first owner");
        assert!(matches!(
            CameraSharedIngressProducer::create_named(&name, test_stream_id(), 23),
            Err(CameraSharedIngressError::AlreadyOwned)
        ));
        drop(producer);
        CameraSharedIngressProducer::create_named(&name, test_stream_id(), 23)
            .expect("mapping disappears with its final handle");
    }

    #[test]
    fn independent_readers_take_the_latest_stable_frame_and_mark_skips() {
        let name = test_mapping_name("readers");
        let mut producer =
            CameraSharedIngressProducer::create_named(&name, test_stream_id(), 23).unwrap();
        let mut first =
            CameraSharedIngressConsumer::open_named(&name, test_stream_id(), 23).unwrap();
        let mut second =
            CameraSharedIngressConsumer::open_named(&name, test_stream_id(), 23).unwrap();
        let mut source = source();

        let frame = source.next_frame().unwrap();
        producer.publish(frame.clone()).unwrap();
        assert_eq!(first.try_read_latest().unwrap(), Some(frame.clone()));
        assert_eq!(second.try_read_latest().unwrap(), Some(frame));
        assert!(first.try_read_latest().unwrap().is_none());

        producer.publish(source.next_frame().unwrap()).unwrap();
        let newest = source.next_frame().unwrap();
        producer.publish(newest.clone()).unwrap();
        let mut observed = first.try_read_latest().unwrap().unwrap();
        assert_eq!(observed.payload, newest.payload);
        assert_eq!(observed.descriptor.sequence, newest.descriptor.sequence);
        assert!(observed.descriptor.flags.discontinuity);
        observed.descriptor.flags.discontinuity = false;
        assert_eq!(observed, newest);
    }

    #[test]
    fn separate_process_opens_read_only_mapping_and_reads_owned_payload() {
        let name = test_mapping_name("process");
        let mut producer =
            CameraSharedIngressProducer::create_named(&name, test_stream_id(), 23).unwrap();
        let mut frame = source().next_frame().unwrap();
        frame.payload[0] = 77;
        producer.publish(frame).unwrap();

        let status = Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "tests::cross_process_consumer_child",
                "--nocapture",
            ])
            .env(CHILD_FLAG, "1")
            .env(CHILD_MAPPING, &name)
            .status()
            .expect("spawn child consumer");
        assert!(status.success());
    }

    #[test]
    fn cross_process_consumer_child() {
        if std::env::var_os(CHILD_FLAG).as_deref() != Some(std::ffi::OsStr::new("1")) {
            return;
        }
        let name = std::env::var(CHILD_MAPPING).expect("parent mapping name");
        let mut consumer =
            CameraSharedIngressConsumer::open_named(&name, test_stream_id(), 23).unwrap();
        let frame = consumer.try_read_latest().unwrap().unwrap();
        assert_eq!(frame.descriptor.sequence, 0);
        assert_eq!(frame.payload[0], 77);
    }
}
