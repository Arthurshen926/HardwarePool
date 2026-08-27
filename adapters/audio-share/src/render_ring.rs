//! Versioned bounded bridge from the render APO's shared-memory ring.

use thiserror::Error;

const MAX_PAYLOAD_BYTES: usize = 16 * 1024;

#[derive(Debug, Error)]
pub enum RenderRingError {
    #[error(
        "render block is empty, exceeds {limit} bytes, or is not float32 aligned: {actual} bytes"
    )]
    InvalidFloatBlock { actual: usize, limit: usize },
    #[cfg(windows)]
    #[error("the CapyIO render mapping already belongs to another Broker")]
    AlreadyOwned,
    #[cfg(windows)]
    #[error("Windows {operation} failed with error {code}")]
    Windows { operation: &'static str, code: u32 },
    #[cfg(windows)]
    #[error("the render ring contains an invalid bounded layout")]
    InvalidLayout,
}

/// Convert an interleaved little-endian float32 block to bounded S16LE PCM.
pub fn f32le_to_s16le(input: &[u8], output: &mut Vec<u8>) -> Result<(), RenderRingError> {
    if input.is_empty() || input.len() > MAX_PAYLOAD_BYTES || !input.len().is_multiple_of(4) {
        return Err(RenderRingError::InvalidFloatBlock {
            actual: input.len(),
            limit: MAX_PAYLOAD_BYTES,
        });
    }

    output.clear();
    output.reserve(input.len() / 2);
    for bytes in input.chunks_exact(4) {
        let sample = f32::from_le_bytes(bytes.try_into().expect("four-byte chunk"));
        let scaled = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
        output.extend_from_slice(&scaled.to_le_bytes());
    }
    Ok(())
}

#[cfg(windows)]
mod windows {
    use std::{
        io,
        mem::size_of,
        ptr::{self, NonNull},
        sync::atomic::{AtomicI64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
        },
        System::Memory::{
            CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile,
            PAGE_READWRITE, UnmapViewOfFile,
        },
    };

    use super::{MAX_PAYLOAD_BYTES, RenderRingError, f32le_to_s16le};

    const MAGIC: u32 = 0x524f_4950;
    const VERSION: u16 = 1;
    const HEADER_SIZE: usize = 128;
    const SAMPLE_FORMAT_FLOAT32_LE: u16 = 1;
    const SAMPLE_RATE: u32 = 48_000;
    const CHANNELS: u16 = 2;
    const SLOT_COUNT: usize = 32;
    const SLOT_HEADER_SIZE: usize = 16;
    const SLOT_STRIDE: usize = SLOT_HEADER_SIZE + MAX_PAYLOAD_BYTES;
    const TOTAL_SIZE: usize = HEADER_SIZE + SLOT_COUNT * SLOT_STRIDE;
    const MAPPING_NAME: &[u16] = &[
        76, 111, 99, 97, 108, 92, 67, 97, 112, 121, 73, 79, 46, 82, 101, 110, 100, 101, 114, 82,
        105, 110, 103, 46, 118, 49, 0,
    ];

    #[repr(C, align(64))]
    struct Header {
        magic: u32,
        version: u16,
        header_size: u16,
        total_size: u32,
        slot_count: u32,
        slot_stride: u32,
        payload_capacity: u32,
        sample_rate: u32,
        channels: u16,
        sample_format: u16,
        generation: u64,
        write_sequence: AtomicI64,
        read_sequence: AtomicI64,
        dropped_blocks: AtomicI64,
        produced_blocks: AtomicI64,
        reserved: [u8; 56],
    }

    const _: () = assert!(size_of::<Header>() == HEADER_SIZE);

    pub struct RenderRingConsumer {
        mapping: HANDLE,
        view: NonNull<u8>,
        generation: u64,
        float_block: Vec<u8>,
    }

    impl RenderRingConsumer {
        pub fn create_baseline() -> Result<Self, RenderRingError> {
            let size = u32::try_from(TOTAL_SIZE).expect("bounded render mapping fits u32");
            let mapping = unsafe {
                CreateFileMappingW(
                    INVALID_HANDLE_VALUE,
                    ptr::null(),
                    PAGE_READWRITE,
                    0,
                    size,
                    MAPPING_NAME.as_ptr(),
                )
            };
            if mapping.is_null() {
                return Err(last_error("CreateFileMappingW"));
            }
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                unsafe { CloseHandle(mapping) };
                return Err(RenderRingError::AlreadyOwned);
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
                total_size: size,
                slot_count: SLOT_COUNT as u32,
                slot_stride: SLOT_STRIDE as u32,
                payload_capacity: MAX_PAYLOAD_BYTES as u32,
                sample_rate: SAMPLE_RATE,
                channels: CHANNELS,
                sample_format: SAMPLE_FORMAT_FLOAT32_LE,
                generation,
                write_sequence: AtomicI64::new(0),
                read_sequence: AtomicI64::new(0),
                dropped_blocks: AtomicI64::new(0),
                produced_blocks: AtomicI64::new(0),
                reserved: [0; 56],
            };
            unsafe { ptr::write(view.as_ptr().cast::<Header>(), header) };

            Ok(Self {
                mapping,
                view,
                generation,
                float_block: Vec::with_capacity(MAX_PAYLOAD_BYTES),
            })
        }

        /// Drain at most one committed block. The caller owns scheduling and transport.
        pub fn try_read_s16le(&mut self, output: &mut Vec<u8>) -> Result<bool, RenderRingError> {
            let header = unsafe { &*self.view.as_ptr().cast::<Header>() };
            if !valid_header(header) || header.generation != self.generation {
                return Err(RenderRingError::InvalidLayout);
            }
            let read = header.read_sequence.load(Ordering::Relaxed);
            let write = header.write_sequence.load(Ordering::Acquire);
            if read < 0 || write < read || write - read > i64::from(header.slot_count) {
                return Err(RenderRingError::InvalidLayout);
            }
            if read == write {
                return Ok(false);
            }

            let slot_index =
                usize::try_from(read).map_err(|_| RenderRingError::InvalidLayout)? % SLOT_COUNT;
            let slot = unsafe {
                self.view
                    .as_ptr()
                    .add(HEADER_SIZE + slot_index * SLOT_STRIDE)
            };
            let slot_generation = unsafe { ptr::read_unaligned(slot.cast::<u64>()) };
            let byte_count = unsafe { ptr::read_unaligned(slot.add(8).cast::<u32>()) } as usize;
            let frame_count = unsafe { ptr::read_unaligned(slot.add(12).cast::<u32>()) } as usize;
            let expected = frame_count
                .checked_mul(usize::from(CHANNELS))
                .and_then(|count| count.checked_mul(size_of::<f32>()))
                .ok_or(RenderRingError::InvalidLayout)?;
            if slot_generation != self.generation
                || byte_count == 0
                || byte_count > MAX_PAYLOAD_BYTES
                || byte_count != expected
            {
                return Err(RenderRingError::InvalidLayout);
            }

            self.float_block.resize(byte_count, 0);
            unsafe {
                ptr::copy_nonoverlapping(
                    slot.add(SLOT_HEADER_SIZE),
                    self.float_block.as_mut_ptr(),
                    byte_count,
                );
            }
            f32le_to_s16le(&self.float_block, output)?;
            header.read_sequence.store(read + 1, Ordering::Release);
            Ok(true)
        }

        #[must_use]
        pub fn counters(&self) -> (u64, u64) {
            let header = unsafe { &*self.view.as_ptr().cast::<Header>() };
            (
                header.produced_blocks.load(Ordering::Relaxed).max(0) as u64,
                header.dropped_blocks.load(Ordering::Relaxed).max(0) as u64,
            )
        }
    }

    impl Drop for RenderRingConsumer {
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
            && header.slot_count as usize == SLOT_COUNT
            && header.slot_stride as usize == SLOT_STRIDE
            && header.payload_capacity as usize == MAX_PAYLOAD_BYTES
            && header.sample_rate == SAMPLE_RATE
            && header.channels == CHANNELS
            && header.sample_format == SAMPLE_FORMAT_FLOAT32_LE
    }

    fn last_error(operation: &'static str) -> RenderRingError {
        let code = io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_default() as u32;
        RenderRingError::Windows { operation, code }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        static TEST_MAPPING_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

        #[test]
        fn baseline_mapping_has_exact_bounded_layout() {
            assert_eq!(size_of::<Header>(), 128);
            assert_eq!(SLOT_STRIDE, 16_400);
            assert_eq!(TOTAL_SIZE, 524_928);
        }

        #[test]
        fn broker_owns_only_one_named_mapping() {
            let _guard = TEST_MAPPING_LOCK.lock().unwrap();
            let first = RenderRingConsumer::create_baseline().unwrap();
            assert!(matches!(
                RenderRingConsumer::create_baseline(),
                Err(RenderRingError::AlreadyOwned)
            ));
            drop(first);
            RenderRingConsumer::create_baseline().unwrap();
        }

        #[test]
        fn committed_shared_block_is_converted_and_released() {
            let _guard = TEST_MAPPING_LOCK.lock().unwrap();
            let mut consumer = RenderRingConsumer::create_baseline().unwrap();
            let bytes = [0.0_f32, 0.5, -0.5, 1.0]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>();
            let header = unsafe { &*consumer.view.as_ptr().cast::<Header>() };
            let slot = unsafe { consumer.view.as_ptr().add(HEADER_SIZE) };
            unsafe {
                ptr::write_unaligned(slot.cast::<u64>(), consumer.generation);
                ptr::write_unaligned(slot.add(8).cast::<u32>(), bytes.len() as u32);
                ptr::write_unaligned(slot.add(12).cast::<u32>(), 2);
                ptr::copy_nonoverlapping(bytes.as_ptr(), slot.add(SLOT_HEADER_SIZE), bytes.len());
            }
            header.write_sequence.store(1, Ordering::Release);

            let mut output = Vec::new();
            assert!(consumer.try_read_s16le(&mut output).unwrap());
            assert_eq!(
                output
                    .chunks_exact(2)
                    .map(|sample| i16::from_le_bytes(sample.try_into().unwrap()))
                    .collect::<Vec<_>>(),
                [0, 16_384, -16_384, 32_767]
            );
            assert_eq!(header.read_sequence.load(Ordering::Acquire), 1);
            assert!(!consumer.try_read_s16le(&mut output).unwrap());
        }
    }
}

#[cfg(windows)]
pub use windows::RenderRingConsumer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_conversion_clamps_and_preserves_zero() {
        let input = [-2.0_f32, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0, f32::NAN]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>();
        let mut output = Vec::new();
        f32le_to_s16le(&input, &mut output).unwrap();
        let samples = output
            .chunks_exact(2)
            .map(|bytes| i16::from_le_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(samples, [-32767, -32767, -16384, 0, 16384, 32767, 32767, 0]);
    }

    #[test]
    fn float_conversion_rejects_unbounded_or_partial_blocks() {
        let mut output = Vec::new();
        assert!(f32le_to_s16le(&[], &mut output).is_err());
        assert!(f32le_to_s16le(&[0; 3], &mut output).is_err());
        assert!(f32le_to_s16le(&vec![0; MAX_PAYLOAD_BYTES + 4], &mut output).is_err());
    }
}
