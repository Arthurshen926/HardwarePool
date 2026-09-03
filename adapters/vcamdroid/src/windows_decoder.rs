use std::{
    collections::VecDeque,
    error::Error,
    fmt,
    mem::ManuallyDrop,
    ptr,
    time::{Duration, Instant},
};

use windows::{
    Win32::{
        Media::MediaFoundation::{
            CMSH264DecoderMFT, CODECAPI_AVLowLatencyMode, ICodecAPI, IMFMediaBuffer, IMFSample,
            IMFTransform, MF_E_NO_MORE_TYPES, MF_E_NOTACCEPTING, MF_E_TRANSFORM_NEED_MORE_INPUT,
            MF_E_TRANSFORM_STREAM_CHANGE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE,
            MF_MT_MAJOR_TYPE, MF_MT_MPEG_SEQUENCE_HEADER, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE,
            MF_VERSION, MFCreateAlignedMemoryBuffer, MFCreateMediaType, MFCreateMemoryBuffer,
            MFCreateSample, MFMediaType_Video, MFSTARTUP_FULL, MFSampleExtension_CleanPoint,
            MFSampleExtension_Discontinuity, MFShutdown, MFStartup, MFT_MESSAGE_COMMAND_DRAIN,
            MFT_MESSAGE_COMMAND_FLUSH, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
            MFT_MESSAGE_NOTIFY_END_OF_STREAM, MFT_MESSAGE_NOTIFY_START_OF_STREAM,
            MFT_OUTPUT_DATA_BUFFER, MFT_OUTPUT_STREAM_PROVIDES_SAMPLES, MFVideoFormat_H264,
            MFVideoFormat_NV12, MFVideoInterlace_MixedInterlaceOrProgressive,
        },
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        },
        System::Variant::VARIANT,
    },
    core::{Error as WindowsError, Interface},
};

use crate::{AvcAccessUnit, AvcConfig, AvcLayout};

const INPUT_STREAM_ID: u32 = 0;
const OUTPUT_STREAM_ID: u32 = 0;
const HUNDRED_NS_PER_SECOND: u64 = 10_000_000;
const HUNDRED_NS_PER_MICROSECOND: u64 = 10;
const MAX_DECODED_FRAME_BYTES: usize = 32 * 1024 * 1024;
const MAX_PENDING_SAMPLES: usize = 64;
const MAX_OUTPUTS_PER_DRAIN: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedNv12Frame {
    pub source_sequence: u64,
    pub presentation_time_us: u64,
    pub discontinuity: bool,
    pub width: u32,
    pub height: u32,
    pub payload: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StageLatencyStats {
    pub samples: u64,
    pub total_micros: u64,
    pub max_micros: u64,
}

impl StageLatencyStats {
    pub fn observe(&mut self, elapsed: Duration) {
        let micros = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
        self.samples = self.samples.saturating_add(1);
        self.total_micros = self.total_micros.saturating_add(micros);
        self.max_micros = self.max_micros.max(micros);
    }

    pub fn average_micros(self) -> u64 {
        self.total_micros.checked_div(self.samples).unwrap_or(0)
    }
}

#[derive(Debug)]
pub enum MfAvcDecoderError {
    Invalid(&'static str),
    Windows {
        stage: &'static str,
        source: WindowsError,
    },
}

impl MfAvcDecoderError {
    fn windows(stage: &'static str, source: WindowsError) -> Self {
        Self::Windows { stage, source }
    }
}

impl fmt::Display for MfAvcDecoderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(reason) => {
                write!(formatter, "invalid Media Foundation AVC input: {reason}")
            }
            Self::Windows { stage, source } => {
                write!(
                    formatter,
                    "Media Foundation AVC decoder failed at {stage}: {source}"
                )
            }
        }
    }
}

impl Error for MfAvcDecoderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Invalid(_) => None,
            Self::Windows { source, .. } => Some(source),
        }
    }
}

struct MediaFoundationRuntime {
    active: bool,
}

impl MediaFoundationRuntime {
    fn startup() -> Result<Self, MfAvcDecoderError> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|error| MfAvcDecoderError::windows("CoInitializeEx", error))?;
            if let Err(error) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
                CoUninitialize();
                return Err(MfAvcDecoderError::windows("MFStartup", error));
            }
        }
        Ok(Self { active: true })
    }
}

impl Drop for MediaFoundationRuntime {
    fn drop(&mut self) {
        if self.active {
            unsafe {
                let _ = MFShutdown();
                CoUninitialize();
            }
            self.active = false;
        }
    }
}

#[derive(Clone, Copy)]
struct PendingSample {
    sequence: u64,
    presentation_time_us: u64,
    sample_time_100ns: i64,
    discontinuity: bool,
    submitted_at: Instant,
}

pub struct MfAvcDecoder {
    transform: Option<IMFTransform>,
    _runtime: MediaFoundationRuntime,
    width: u32,
    height: u32,
    frame_duration_100ns: i64,
    packed_frame_bytes: usize,
    output_stride: usize,
    sequence_header: Vec<u8>,
    needs_sequence_header: bool,
    drained: bool,
    pending: VecDeque<PendingSample>,
    low_latency_enabled: bool,
    latency_stats: StageLatencyStats,
}

impl MfAvcDecoder {
    pub fn new(config: &AvcConfig) -> Result<Self, MfAvcDecoderError> {
        let validated = ValidatedConfig::new(config)?;
        let runtime = MediaFoundationRuntime::startup()?;
        let transform: IMFTransform = unsafe {
            CoCreateInstance(&CMSH264DecoderMFT, None, CLSCTX_INPROC_SERVER)
                .map_err(|error| MfAvcDecoderError::windows("CoCreateInstance", error))?
        };
        let low_latency_enabled = enable_low_latency(&transform)?;

        let input_type = unsafe { MFCreateMediaType() }
            .map_err(|error| MfAvcDecoderError::windows("MFCreateMediaType(input)", error))?;
        unsafe {
            input_type
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(|error| MfAvcDecoderError::windows("input.SetGUID(major)", error))?;
            input_type
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)
                .map_err(|error| MfAvcDecoderError::windows("input.SetGUID(subtype)", error))?;
            input_type
                .SetUINT64(
                    &MF_MT_FRAME_SIZE,
                    pack_ratio(validated.width, validated.height),
                )
                .map_err(|error| {
                    MfAvcDecoderError::windows("input.SetUINT64(frame-size)", error)
                })?;
            input_type
                .SetUINT64(
                    &MF_MT_FRAME_RATE,
                    pack_ratio(u32::from(config.frames_per_second), 1),
                )
                .map_err(|error| {
                    MfAvcDecoderError::windows("input.SetUINT64(frame-rate)", error)
                })?;
            input_type
                .SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack_ratio(1, 1))
                .map_err(|error| MfAvcDecoderError::windows("input.SetUINT64(aspect)", error))?;
            input_type
                .SetUINT32(
                    &MF_MT_INTERLACE_MODE,
                    MFVideoInterlace_MixedInterlaceOrProgressive.0 as u32,
                )
                .map_err(|error| MfAvcDecoderError::windows("input.SetUINT32(interlace)", error))?;
            input_type
                .SetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &validated.sequence_header)
                .map_err(|error| {
                    MfAvcDecoderError::windows("input.SetBlob(sequence-header)", error)
                })?;
            transform
                .SetInputType(INPUT_STREAM_ID, &input_type, 0)
                .map_err(|error| MfAvcDecoderError::windows("SetInputType", error))?;
        }

        let (output_type, output_stride) =
            select_nv12_output_type(&transform, validated.width, validated.height)?;
        unsafe {
            transform
                .SetOutputType(OUTPUT_STREAM_ID, &output_type, 0)
                .map_err(|error| MfAvcDecoderError::windows("SetOutputType", error))?;
            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
                .map_err(|error| MfAvcDecoderError::windows("BEGIN_STREAMING", error))?;
            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
                .map_err(|error| MfAvcDecoderError::windows("START_OF_STREAM", error))?;
        }

        Ok(Self {
            transform: Some(transform),
            _runtime: runtime,
            width: validated.width,
            height: validated.height,
            frame_duration_100ns: validated.frame_duration_100ns,
            packed_frame_bytes: validated.packed_frame_bytes,
            output_stride,
            sequence_header: validated.sequence_header,
            needs_sequence_header: true,
            drained: false,
            pending: VecDeque::with_capacity(MAX_PENDING_SAMPLES),
            low_latency_enabled,
            latency_stats: StageLatencyStats::default(),
        })
    }

    pub fn pending_samples(&self) -> usize {
        self.pending.len()
    }

    pub fn low_latency_enabled(&self) -> bool {
        self.low_latency_enabled
    }

    pub fn latency_stats(&self) -> StageLatencyStats {
        self.latency_stats
    }

    pub fn decode(
        &mut self,
        unit: &AvcAccessUnit,
    ) -> Result<Vec<DecodedNv12Frame>, MfAvcDecoderError> {
        if unit.end_of_stream {
            return Err(MfAvcDecoderError::Invalid(
                "end-of-stream records must be drained separately",
            ));
        }
        if self.drained {
            return Err(MfAvcDecoderError::Invalid(
                "decoder cannot accept input after drain",
            ));
        }
        if unit.payload.is_empty() {
            return Err(MfAvcDecoderError::Invalid("access unit payload is empty"));
        }
        if unit.discontinuity {
            self.flush()?;
        }
        if self.pending.len() >= MAX_PENDING_SAMPLES {
            return Err(MfAvcDecoderError::Invalid(
                "decoder pending-sample bound was exceeded",
            ));
        }
        if self.needs_sequence_header && !unit.key_frame {
            return Err(MfAvcDecoderError::Invalid(
                "decoder restart requires a key frame for the SPS/PPS prefix",
            ));
        }

        let sample_time_100ns = unit
            .presentation_time_us
            .checked_mul(HUNDRED_NS_PER_MICROSECOND)
            .and_then(|value| i64::try_from(value).ok())
            .ok_or(MfAvcDecoderError::Invalid(
                "presentation timestamp exceeds Media Foundation range",
            ))?;
        let sequence_header = self
            .needs_sequence_header
            .then_some(self.sequence_header.as_slice());
        let sample = create_input_sample(
            unit,
            sequence_header,
            sample_time_100ns,
            self.frame_duration_100ns,
        )?;
        let mut decoded = Vec::new();
        let submitted_at = Instant::now();

        for attempt in 0..=1 {
            let result = unsafe { self.transform().ProcessInput(INPUT_STREAM_ID, &sample, 0) };
            match result {
                Ok(()) => break,
                Err(error) if error.code() == MF_E_NOTACCEPTING && attempt == 0 => {
                    decoded.extend(self.drain_available()?);
                }
                Err(error) => {
                    return Err(MfAvcDecoderError::windows("ProcessInput", error));
                }
            }
        }

        self.needs_sequence_header = false;
        self.pending.push_back(PendingSample {
            sequence: unit.sequence,
            presentation_time_us: unit.presentation_time_us,
            sample_time_100ns,
            discontinuity: unit.discontinuity,
            submitted_at,
        });
        decoded.extend(self.drain_available()?);
        Ok(decoded)
    }

    pub fn finish(&mut self) -> Result<Vec<DecodedNv12Frame>, MfAvcDecoderError> {
        if self.drained {
            return Err(MfAvcDecoderError::Invalid("decoder was already drained"));
        }
        unsafe {
            self.transform()
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, INPUT_STREAM_ID as usize)
                .map_err(|error| MfAvcDecoderError::windows("END_OF_STREAM", error))?;
            self.transform()
                .ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)
                .map_err(|error| MfAvcDecoderError::windows("DRAIN", error))?;
        }
        self.drained = true;
        let output_bound = self.pending.len();
        let mut decoded = Vec::with_capacity(output_bound);
        for _ in 0..=output_bound {
            match self.process_one_output()? {
                Some(frame) => decoded.push(frame),
                None => {
                    self.pending.clear();
                    return Ok(decoded);
                }
            }
        }
        Err(MfAvcDecoderError::Invalid(
            "decoder drain exceeded the pending-sample bound",
        ))
    }

    fn flush(&mut self) -> Result<(), MfAvcDecoderError> {
        unsafe {
            self.transform()
                .ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0)
                .map_err(|error| MfAvcDecoderError::windows("FLUSH", error))?;
            self.transform()
                .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
                .map_err(|error| {
                    MfAvcDecoderError::windows("START_OF_STREAM after flush", error)
                })?;
        }
        self.pending.clear();
        self.needs_sequence_header = true;
        Ok(())
    }

    fn drain_available(&mut self) -> Result<Vec<DecodedNv12Frame>, MfAvcDecoderError> {
        let mut decoded = Vec::new();
        for _ in 0..MAX_OUTPUTS_PER_DRAIN {
            match self.process_one_output()? {
                Some(frame) => decoded.push(frame),
                None => return Ok(decoded),
            }
        }
        Err(MfAvcDecoderError::Invalid(
            "decoder produced too many frames in one bounded drain",
        ))
    }

    fn process_one_output(&mut self) -> Result<Option<DecodedNv12Frame>, MfAvcDecoderError> {
        self.process_one_output_with_stream_changes(0)
    }

    fn process_one_output_with_stream_changes(
        &mut self,
        stream_changes: usize,
    ) -> Result<Option<DecodedNv12Frame>, MfAvcDecoderError> {
        let stream_info = unsafe { self.transform().GetOutputStreamInfo(OUTPUT_STREAM_ID) }
            .map_err(|error| MfAvcDecoderError::windows("GetOutputStreamInfo", error))?;
        let provides_samples =
            stream_info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0;
        let caller_sample = if provides_samples {
            None
        } else {
            Some(create_output_sample(
                stream_info.cbSize.max(self.packed_frame_bytes as u32),
                stream_info.cbAlignment,
            )?)
        };
        let mut output = MFT_OUTPUT_DATA_BUFFER {
            dwStreamID: OUTPUT_STREAM_ID,
            pSample: ManuallyDrop::new(caller_sample),
            dwStatus: 0,
            pEvents: ManuallyDrop::new(None),
        };
        let mut status = 0;
        let result = unsafe {
            self.transform()
                .ProcessOutput(0, std::slice::from_mut(&mut output), &mut status)
        };
        let sample = unsafe { ManuallyDrop::take(&mut output.pSample) };
        let events = unsafe { ManuallyDrop::take(&mut output.pEvents) };
        drop(events);

        match result {
            Ok(()) => {
                let sample = sample.ok_or(MfAvcDecoderError::Invalid(
                    "decoder succeeded without an output sample",
                ))?;
                self.copy_output_sample(&sample).map(Some)
            }
            Err(error) if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => Ok(None),
            Err(error) if error.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                if stream_changes >= 2 {
                    return Err(MfAvcDecoderError::Invalid(
                        "decoder exceeded the bounded output stream-change count",
                    ));
                }
                let (output_type, output_stride) =
                    select_nv12_output_type(self.transform(), self.width, self.height)?;
                unsafe {
                    self.transform()
                        .SetOutputType(OUTPUT_STREAM_ID, &output_type, 0)
                        .map_err(|error| {
                            MfAvcDecoderError::windows("SetOutputType(stream-change)", error)
                        })?;
                }
                self.output_stride = output_stride;
                self.process_one_output_with_stream_changes(stream_changes + 1)
            }
            Err(error) => Err(MfAvcDecoderError::windows("ProcessOutput", error)),
        }
    }

    fn copy_output_sample(
        &mut self,
        sample: &IMFSample,
    ) -> Result<DecodedNv12Frame, MfAvcDecoderError> {
        let sample_time_100ns = unsafe { sample.GetSampleTime() }
            .map_err(|error| MfAvcDecoderError::windows("GetSampleTime", error))?;
        let pending_index = self
            .pending
            .iter()
            .position(|entry| entry.sample_time_100ns == sample_time_100ns)
            .ok_or(MfAvcDecoderError::Invalid(
                "decoded timestamp did not match a submitted access unit",
            ))?;
        let pending = self
            .pending
            .remove(pending_index)
            .ok_or(MfAvcDecoderError::Invalid("pending timestamp disappeared"))?;
        let buffer = unsafe { sample.ConvertToContiguousBuffer() }
            .map_err(|error| MfAvcDecoderError::windows("ConvertToContiguousBuffer", error))?;
        let payload = copy_packed_nv12(
            &buffer,
            self.width as usize,
            self.height as usize,
            self.output_stride,
            self.packed_frame_bytes,
        )?;
        self.latency_stats.observe(pending.submitted_at.elapsed());
        Ok(DecodedNv12Frame {
            source_sequence: pending.sequence,
            presentation_time_us: pending.presentation_time_us,
            discontinuity: pending.discontinuity,
            width: self.width,
            height: self.height,
            payload,
        })
    }

    fn transform(&self) -> &IMFTransform {
        self.transform
            .as_ref()
            .expect("transform is present until decoder drop")
    }
}

fn enable_low_latency(transform: &IMFTransform) -> Result<bool, MfAvcDecoderError> {
    let codec_api: ICodecAPI = transform
        .cast()
        .map_err(|error| MfAvcDecoderError::windows("cast ICodecAPI", error))?;
    unsafe {
        codec_api
            .IsSupported(&CODECAPI_AVLowLatencyMode)
            .map_err(|error| MfAvcDecoderError::windows("IsSupported(low-latency)", error))?;
        let requested = VARIANT::from(1_u32);
        codec_api
            .SetValue(&CODECAPI_AVLowLatencyMode, &requested)
            .map_err(|error| MfAvcDecoderError::windows("SetValue(low-latency)", error))?;
        let actual = codec_api
            .GetValue(&CODECAPI_AVLowLatencyMode)
            .map_err(|error| MfAvcDecoderError::windows("GetValue(low-latency)", error))?;
        let enabled = u32::try_from(&actual)
            .map_err(|error| MfAvcDecoderError::windows("read low-latency value", error))?;
        if enabled == 0 {
            return Err(MfAvcDecoderError::Invalid(
                "Media Foundation H.264 decoder rejected low-latency mode",
            ));
        }
    }
    Ok(true)
}

impl Drop for MfAvcDecoder {
    fn drop(&mut self) {
        self.transform.take();
    }
}

struct ValidatedConfig {
    width: u32,
    height: u32,
    packed_frame_bytes: usize,
    frame_duration_100ns: i64,
    sequence_header: Vec<u8>,
}

impl ValidatedConfig {
    fn new(config: &AvcConfig) -> Result<Self, MfAvcDecoderError> {
        if config.access_unit_layout != AvcLayout::AnnexB
            || config.codec_specific_layout != AvcLayout::AnnexB
        {
            return Err(MfAvcDecoderError::Invalid(
                "the bootstrap decoder accepts Annex-B access units and CSD only",
            ));
        }
        if config.width == 0
            || config.height == 0
            || !config.width.is_multiple_of(2)
            || !config.height.is_multiple_of(2)
        {
            return Err(MfAvcDecoderError::Invalid(
                "NV12 dimensions must be positive and even",
            ));
        }
        if config.frames_per_second == 0 {
            return Err(MfAvcDecoderError::Invalid("frame rate must be positive"));
        }
        if !has_annex_b_start_code(&config.csd0)
            || (!config.csd1.is_empty() && !has_annex_b_start_code(&config.csd1))
        {
            return Err(MfAvcDecoderError::Invalid(
                "codec-specific data is missing an Annex-B start code",
            ));
        }
        let width = u32::from(config.width);
        let height = u32::from(config.height);
        let packed_frame_bytes = nv12_frame_bytes(width, height)?;
        let frame_duration_100ns =
            i64::try_from(HUNDRED_NS_PER_SECOND / u64::from(config.frames_per_second))
                .map_err(|_| MfAvcDecoderError::Invalid("frame duration overflowed"))?;
        let mut sequence_header = Vec::with_capacity(config.csd0.len() + config.csd1.len());
        sequence_header.extend_from_slice(&config.csd0);
        sequence_header.extend_from_slice(&config.csd1);
        Ok(Self {
            width,
            height,
            packed_frame_bytes,
            frame_duration_100ns,
            sequence_header,
        })
    }
}

fn select_nv12_output_type(
    transform: &IMFTransform,
    expected_width: u32,
    expected_height: u32,
) -> Result<(windows::Win32::Media::MediaFoundation::IMFMediaType, usize), MfAvcDecoderError> {
    for index in 0..64 {
        let media_type = match unsafe { transform.GetOutputAvailableType(OUTPUT_STREAM_ID, index) }
        {
            Ok(media_type) => media_type,
            Err(error) if error.code() == MF_E_NO_MORE_TYPES => break,
            Err(error) => {
                return Err(MfAvcDecoderError::windows("GetOutputAvailableType", error));
            }
        };
        let subtype = unsafe { media_type.GetGUID(&MF_MT_SUBTYPE) }
            .map_err(|error| MfAvcDecoderError::windows("output.GetGUID(subtype)", error))?;
        if subtype != MFVideoFormat_NV12 {
            continue;
        }
        if let Ok(packed_size) = unsafe { media_type.GetUINT64(&MF_MT_FRAME_SIZE) } {
            let (width, height) = unpack_ratio(packed_size);
            if width != expected_width || height != expected_height {
                continue;
            }
        }
        let output_stride = unsafe {
            media_type.GetUINT32(&windows::Win32::Media::MediaFoundation::MF_MT_DEFAULT_STRIDE)
        }
        .ok()
        .and_then(|value| usize::try_from(value as i32).ok())
        .filter(|stride| *stride >= expected_width as usize)
        .unwrap_or(expected_width as usize);
        return Ok((media_type, output_stride));
    }
    Err(MfAvcDecoderError::Invalid(
        "decoder did not expose the requested NV12 output type",
    ))
}

fn create_input_sample(
    unit: &AvcAccessUnit,
    sequence_header: Option<&[u8]>,
    sample_time_100ns: i64,
    frame_duration_100ns: i64,
) -> Result<IMFSample, MfAvcDecoderError> {
    let prefix = sequence_header.unwrap_or_default();
    let combined_length =
        prefix
            .len()
            .checked_add(unit.payload.len())
            .ok_or(MfAvcDecoderError::Invalid(
                "prefixed access-unit length overflowed",
            ))?;
    let length = u32::try_from(combined_length)
        .map_err(|_| MfAvcDecoderError::Invalid("access unit exceeds u32"))?;
    let buffer = unsafe { MFCreateMemoryBuffer(length) }
        .map_err(|error| MfAvcDecoderError::windows("MFCreateMemoryBuffer(input)", error))?;
    copy_into_buffer(&buffer, prefix, &unit.payload)?;
    let sample = unsafe { MFCreateSample() }
        .map_err(|error| MfAvcDecoderError::windows("MFCreateSample(input)", error))?;
    unsafe {
        sample
            .AddBuffer(&buffer)
            .map_err(|error| MfAvcDecoderError::windows("input.AddBuffer", error))?;
        sample
            .SetSampleTime(sample_time_100ns)
            .map_err(|error| MfAvcDecoderError::windows("input.SetSampleTime", error))?;
        sample
            .SetSampleDuration(frame_duration_100ns)
            .map_err(|error| MfAvcDecoderError::windows("input.SetSampleDuration", error))?;
        sample
            .SetUINT32(&MFSampleExtension_CleanPoint, u32::from(unit.key_frame))
            .map_err(|error| MfAvcDecoderError::windows("input.SetCleanPoint", error))?;
        sample
            .SetUINT32(
                &MFSampleExtension_Discontinuity,
                u32::from(unit.discontinuity),
            )
            .map_err(|error| MfAvcDecoderError::windows("input.SetDiscontinuity", error))?;
    }
    Ok(sample)
}

fn create_output_sample(length: u32, alignment: u32) -> Result<IMFSample, MfAvcDecoderError> {
    let buffer = unsafe {
        if alignment == 0 {
            MFCreateMemoryBuffer(length)
        } else {
            MFCreateAlignedMemoryBuffer(length, alignment)
        }
    }
    .map_err(|error| MfAvcDecoderError::windows("MFCreateMemoryBuffer(output)", error))?;
    let sample = unsafe { MFCreateSample() }
        .map_err(|error| MfAvcDecoderError::windows("MFCreateSample(output)", error))?;
    unsafe {
        sample
            .AddBuffer(&buffer)
            .map_err(|error| MfAvcDecoderError::windows("output.AddBuffer", error))?;
    }
    Ok(sample)
}

fn copy_into_buffer(
    buffer: &IMFMediaBuffer,
    prefix: &[u8],
    payload: &[u8],
) -> Result<(), MfAvcDecoderError> {
    let combined_length =
        prefix
            .len()
            .checked_add(payload.len())
            .ok_or(MfAvcDecoderError::Invalid(
                "prefixed access-unit length overflowed",
            ))?;
    let mut target = ptr::null_mut();
    let mut maximum = 0;
    unsafe {
        buffer
            .Lock(&mut target, Some(&mut maximum), None)
            .map_err(|error| MfAvcDecoderError::windows("input-buffer.Lock", error))?;
        if maximum < combined_length as u32 {
            let _ = buffer.Unlock();
            return Err(MfAvcDecoderError::Invalid(
                "input buffer is smaller than the access unit",
            ));
        }
        ptr::copy_nonoverlapping(prefix.as_ptr(), target, prefix.len());
        ptr::copy_nonoverlapping(payload.as_ptr(), target.add(prefix.len()), payload.len());
        buffer
            .Unlock()
            .map_err(|error| MfAvcDecoderError::windows("input-buffer.Unlock", error))?;
        buffer
            .SetCurrentLength(combined_length as u32)
            .map_err(|error| MfAvcDecoderError::windows("input-buffer.SetCurrentLength", error))?;
    }
    Ok(())
}

fn copy_packed_nv12(
    buffer: &IMFMediaBuffer,
    width: usize,
    height: usize,
    stride: usize,
    packed_frame_bytes: usize,
) -> Result<Vec<u8>, MfAvcDecoderError> {
    let mut source = ptr::null_mut();
    let mut current = 0;
    unsafe {
        buffer
            .Lock(&mut source, None, Some(&mut current))
            .map_err(|error| MfAvcDecoderError::windows("output-buffer.Lock", error))?;
    }
    let result = (|| {
        let current = current as usize;
        if stride == width && current >= packed_frame_bytes {
            let bytes = unsafe { std::slice::from_raw_parts(source, packed_frame_bytes) };
            return Ok(bytes.to_vec());
        }
        let rows = height
            .checked_add(height / 2)
            .ok_or(MfAvcDecoderError::Invalid("NV12 row count overflowed"))?;
        let strided_bytes = stride
            .checked_mul(rows)
            .ok_or(MfAvcDecoderError::Invalid("strided NV12 size overflowed"))?;
        if stride < width || current < strided_bytes {
            return Err(MfAvcDecoderError::Invalid(
                "decoder output buffer is smaller than the declared NV12 layout",
            ));
        }
        let mut packed = vec![0_u8; packed_frame_bytes];
        for row in 0..rows {
            unsafe {
                ptr::copy_nonoverlapping(
                    source.add(row * stride),
                    packed.as_mut_ptr().add(row * width),
                    width,
                );
            }
        }
        Ok(packed)
    })();
    let unlock_result = unsafe { buffer.Unlock() }
        .map_err(|error| MfAvcDecoderError::windows("output-buffer.Unlock", error));
    match (result, unlock_result) {
        (Ok(payload), Ok(())) => Ok(payload),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn nv12_frame_bytes(width: u32, height: u32) -> Result<usize, MfAvcDecoderError> {
    let pixels = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or(MfAvcDecoderError::Invalid("NV12 dimensions overflowed"))?;
    let bytes = pixels
        .checked_mul(3)
        .and_then(|value| value.checked_div(2))
        .ok_or(MfAvcDecoderError::Invalid("NV12 byte length overflowed"))?;
    if bytes > MAX_DECODED_FRAME_BYTES {
        return Err(MfAvcDecoderError::Invalid(
            "decoded NV12 frame exceeds the bootstrap bound",
        ));
    }
    Ok(bytes)
}

fn has_annex_b_start_code(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0, 0, 1]) || bytes.starts_with(&[0, 0, 0, 1])
}

const fn pack_ratio(numerator: u32, denominator: u32) -> u64 {
    ((numerator as u64) << 32) | denominator as u64
}

const fn unpack_ratio(value: u64) -> (u32, u32) {
    ((value >> 32) as u32, value as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AvcConfig {
        AvcConfig {
            width: 1280,
            height: 720,
            frames_per_second: 30,
            bitrate_bits_per_second: 4_000_000,
            clockwise_rotation_degrees: 0,
            access_unit_layout: AvcLayout::AnnexB,
            codec_specific_layout: AvcLayout::AnnexB,
            csd0: vec![0, 0, 0, 1, 0x67, 0x64, 0, 0x1f],
            csd1: vec![0, 0, 0, 1, 0x68, 0xee, 0x3c, 0x80],
        }
    }

    #[test]
    fn validates_bounded_annex_b_nv12_contract() {
        let validated = ValidatedConfig::new(&config()).unwrap();
        assert_eq!(validated.width, 1280);
        assert_eq!(validated.height, 720);
        assert_eq!(validated.packed_frame_bytes, 1_382_400);
        assert_eq!(validated.frame_duration_100ns, 333_333);
        assert_eq!(validated.sequence_header.len(), 16);
    }

    #[test]
    fn inbox_h264_decoder_enables_low_latency_mode() {
        let decoder = MfAvcDecoder::new(&config()).unwrap();
        assert!(decoder.low_latency_enabled());
        assert_eq!(decoder.pending_samples(), 0);
        assert_eq!(decoder.latency_stats(), StageLatencyStats::default());
    }

    #[test]
    fn stage_latency_stats_are_bounded_and_explicit() {
        let mut stats = StageLatencyStats::default();
        stats.observe(Duration::from_micros(700));
        stats.observe(Duration::from_micros(1_300));
        assert_eq!(stats.samples, 2);
        assert_eq!(stats.total_micros, 2_000);
        assert_eq!(stats.average_micros(), 1_000);
        assert_eq!(stats.max_micros, 1_300);
    }

    #[test]
    fn rejects_length_prefixed_access_units() {
        let mut invalid = config();
        invalid.access_unit_layout = AvcLayout::LengthPrefixed4;
        assert!(matches!(
            ValidatedConfig::new(&invalid),
            Err(MfAvcDecoderError::Invalid(_))
        ));
    }

    #[test]
    fn rejects_csd_without_annex_b_start_code() {
        let mut invalid = config();
        invalid.csd0 = vec![0x67, 0x64, 0, 0x1f];
        assert!(matches!(
            ValidatedConfig::new(&invalid),
            Err(MfAvcDecoderError::Invalid(_))
        ));
    }
}
