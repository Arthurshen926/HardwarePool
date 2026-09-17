use std::{mem::ManuallyDrop, ptr};

use capyio_core::StreamId;
use capyio_display_transport::{
    AvcDecoderConfig, AvcLevel, AvcProfile, EncodedAccessUnitDescriptor, EncodedAccessUnitFlags,
    EncodedDisplayStreamSpec, OwnedEncodedAccessUnit,
};
use thiserror::Error;
use windows::{
    Win32::{
        Media::MediaFoundation::{
            CLSID_MSH264EncoderMFT, CODECAPI_AVEncCommonRateControlMode,
            CODECAPI_AVEncCommonRealTime, CODECAPI_AVEncMPVDefaultBPictureCount,
            CODECAPI_AVLowLatencyMode, ICodecAPI, IMFAttributes, IMFMediaBuffer, IMFSample,
            IMFTransform, MF_E_TRANSFORM_NEED_MORE_INPUT, MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE,
            MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MPEG_SEQUENCE_HEADER,
            MF_MT_MPEG2_LEVEL, MF_MT_MPEG2_PROFILE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE,
            MF_VERSION, MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFMediaType_Video,
            MFSTARTUP_FULL, MFSampleExtension_CleanPoint, MFShutdown, MFStartup,
            MFT_MESSAGE_COMMAND_DRAIN, MFT_MESSAGE_COMMAND_FLUSH,
            MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, MFT_MESSAGE_NOTIFY_END_OF_STREAM,
            MFT_MESSAGE_NOTIFY_END_STREAMING, MFT_MESSAGE_NOTIFY_START_OF_STREAM,
            MFT_OUTPUT_DATA_BUFFER, MFT_OUTPUT_STREAM_PROVIDES_SAMPLES, MFVideoFormat_H264,
            MFVideoFormat_NV12, MFVideoInterlace_Progressive,
        },
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        },
        System::Variant::VARIANT,
    },
    core::Interface,
};

use crate::{BgraToNv12Error, IddFrameSnapshot, bgra8_to_nv12};

const ANNEX_B_START: [u8; 4] = [0, 0, 0, 1];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaFoundationH264EncoderConfig {
    pub average_bitrate: u32,
}

impl Default for MediaFoundationH264EncoderConfig {
    fn default() -> Self {
        Self {
            average_bitrate: 8_000_000,
        }
    }
}

#[derive(Debug, Error)]
pub enum MfEncoderError {
    #[error("Media Foundation encoder bitrate must be inside 250000..=50000000")]
    InvalidBitrate,
    #[error("Media Foundation operation failed while trying to {operation}: {source}")]
    Windows {
        operation: &'static str,
        source: windows::core::Error,
    },
    #[error(transparent)]
    Conversion(#[from] BgraToNv12Error),
    #[error("Media Foundation emitted an invalid AVC payload: {0}")]
    InvalidAvc(String),
    #[error("Media Foundation encoded output exceeded the selected bound")]
    OutputTooLarge,
    #[error("Media Foundation encoder sequence arithmetic exhausted")]
    ArithmeticExhausted,
    #[error(transparent)]
    Contract(#[from] capyio_display_transport::EncodedDisplayError),
}

pub struct MediaFoundationH264Encoder {
    transform: Option<IMFTransform>,
    selected: EncodedDisplayStreamSpec,
    stream_id: StreamId,
    stream_epoch: u64,
    next_sequence: u64,
    next_input_time_hns: u64,
    duration_nanos: u64,
    nv12: Vec<u8>,
    mf_started: bool,
    com_initialized: bool,
}

impl MediaFoundationH264Encoder {
    pub fn new(
        selected: EncodedDisplayStreamSpec,
        stream_id: StreamId,
        stream_epoch: u64,
        config: MediaFoundationH264EncoderConfig,
    ) -> Result<Self, MfEncoderError> {
        selected.validate()?;
        if stream_epoch == 0 {
            return Err(
                capyio_display_transport::EncodedDisplayError::InvalidStream(
                    "encoder stream epoch must be positive".to_owned(),
                )
                .into(),
            );
        }
        if !(250_000..=50_000_000).contains(&config.average_bitrate) {
            return Err(MfEncoderError::InvalidBitrate);
        }
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|source| windows_error("initialize COM", source))?;
        if let Err(source) = unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) } {
            unsafe { CoUninitialize() };
            return Err(windows_error("start Media Foundation", source));
        }

        let build = unsafe { build_transform(&selected, config) };
        let transform = match build {
            Ok(transform) => transform,
            Err(error) => {
                let _ = unsafe { MFShutdown() };
                unsafe { CoUninitialize() };
                return Err(error);
            }
        };
        let output_info = unsafe { transform.GetOutputStreamInfo(0) }
            .map_err(|source| windows_error("query encoder output stream", source))?;
        let provides_samples =
            output_info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0;
        if !provides_samples && output_info.cbSize > selected.maximum_access_unit_bytes {
            drop(transform);
            let _ = unsafe { MFShutdown() };
            unsafe { CoUninitialize() };
            return Err(MfEncoderError::OutputTooLarge);
        }
        let duration_nanos = frame_duration_nanos(&selected)?;
        Ok(Self {
            transform: Some(transform),
            selected,
            stream_id,
            stream_epoch,
            next_sequence: 0,
            next_input_time_hns: 0,
            duration_nanos,
            nv12: Vec::new(),
            mf_started: true,
            com_initialized: true,
        })
    }

    pub fn decoder_config(&self) -> Result<AvcDecoderConfig, MfEncoderError> {
        let transform = self.transform.as_ref().expect("live encoder transform");
        let media_type = unsafe { transform.GetOutputCurrentType(0) }
            .map_err(|source| windows_error("read encoder output type", source))?;
        let attributes: IMFAttributes = media_type
            .cast()
            .map_err(|source| windows_error("query encoder output attributes", source))?;
        let size = unsafe { attributes.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER) }
            .map_err(|source| windows_error("read AVC sequence-header size", source))?;
        let mut sequence_header =
            vec![0; usize::try_from(size).map_err(|_| MfEncoderError::OutputTooLarge)?];
        unsafe { attributes.GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut sequence_header, None) }
            .map_err(|source| windows_error("read AVC sequence header", source))?;
        let (sequence_parameter_set, picture_parameter_set) =
            parse_sequence_header(&sequence_header)?;
        let config = AvcDecoderConfig {
            stream_id: self.stream_id,
            stream_epoch: self.stream_epoch,
            selected: self.selected.clone(),
            sequence_parameter_set,
            picture_parameter_set,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn encode(
        &mut self,
        frame: &IddFrameSnapshot,
    ) -> Result<Vec<OwnedEncodedAccessUnit>, MfEncoderError> {
        if frame.header.width != self.selected.decoded.width
            || frame.header.height != self.selected.decoded.height
        {
            return Err(MfEncoderError::InvalidAvc(
                "raw frame dimensions do not match selected encoder mode".to_owned(),
            ));
        }
        bgra8_to_nv12(
            &frame.payload,
            frame.header.width,
            frame.header.height,
            &mut self.nv12,
        )?;
        let sample_time = i64::try_from(self.next_input_time_hns)
            .map_err(|_| MfEncoderError::ArithmeticExhausted)?;
        let sample_duration = i64::try_from(self.duration_nanos / 100)
            .map_err(|_| MfEncoderError::ArithmeticExhausted)?;
        let next_input_time_hns = self
            .next_input_time_hns
            .checked_add(u64::try_from(sample_duration).expect("positive frame duration"))
            .ok_or(MfEncoderError::ArithmeticExhausted)?;
        let input = unsafe { sample_from_bytes(&self.nv12, sample_time, sample_duration) }?;
        let transform = self.transform.as_ref().expect("live encoder transform");
        unsafe { transform.ProcessInput(0, &input, 0) }
            .map_err(|source| windows_error("submit NV12 encoder input", source))?;
        self.next_input_time_hns = next_input_time_hns;
        self.drain_available_output()
    }

    fn drain_available_output(&mut self) -> Result<Vec<OwnedEncodedAccessUnit>, MfEncoderError> {
        let mut units = Vec::new();
        loop {
            match self.try_output()? {
                Some(unit) => units.push(unit),
                None => return Ok(units),
            }
        }
    }

    fn try_output(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, MfEncoderError> {
        let transform = self.transform.as_ref().expect("live encoder transform");
        let info = unsafe { transform.GetOutputStreamInfo(0) }
            .map_err(|source| windows_error("query encoder output stream", source))?;
        let provides_samples = info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0;
        let supplied = if provides_samples {
            None
        } else {
            let sample = unsafe { MFCreateSample() }
                .map_err(|source| windows_error("create encoder output sample", source))?;
            let buffer = unsafe { MFCreateMemoryBuffer(self.selected.maximum_access_unit_bytes) }
                .map_err(|source| windows_error("create encoder output buffer", source))?;
            unsafe { sample.AddBuffer(&buffer) }
                .map_err(|source| windows_error("attach encoder output buffer", source))?;
            Some(sample)
        };
        let mut output = MFT_OUTPUT_DATA_BUFFER {
            dwStreamID: 0,
            pSample: ManuallyDrop::new(supplied),
            dwStatus: 0,
            pEvents: ManuallyDrop::new(None),
        };
        let mut status = 0;
        let result =
            unsafe { transform.ProcessOutput(0, std::slice::from_mut(&mut output), &mut status) };
        let returned_sample = unsafe { ManuallyDrop::take(&mut output.pSample) };
        let returned_events = unsafe { ManuallyDrop::take(&mut output.pEvents) };
        drop(returned_events);
        if let Err(source) = result {
            if source.code() == MF_E_TRANSFORM_NEED_MORE_INPUT {
                return Ok(None);
            }
            return Err(windows_error("retrieve encoder output", source));
        }
        let sample = returned_sample.ok_or_else(|| {
            MfEncoderError::InvalidAvc("encoder returned success without a sample".to_owned())
        })?;
        let payload = unsafe { bytes_from_sample(&sample) }?;
        let payload = normalize_access_unit(&payload)?;
        if payload.len() > self.selected.maximum_access_unit_bytes as usize {
            return Err(MfEncoderError::OutputTooLarge);
        }
        let presentation_hns = unsafe { sample.GetSampleTime() }
            .map_err(|source| windows_error("read output presentation time", source))?;
        let key_frame =
            unsafe { sample.GetUINT32(&MFSampleExtension_CleanPoint) }.unwrap_or(0) != 0;
        let sequence = self.next_sequence;
        self.next_sequence = sequence
            .checked_add(1)
            .ok_or(MfEncoderError::ArithmeticExhausted)?;
        let unit = OwnedEncodedAccessUnit {
            descriptor: EncodedAccessUnitDescriptor {
                stream_id: self.stream_id,
                stream_epoch: self.stream_epoch,
                sequence,
                presentation_time_micros: u64::try_from(presentation_hns)
                    .map_err(|_| MfEncoderError::ArithmeticExhausted)?
                    / 10,
                duration_nanos: self.duration_nanos,
                payload_bytes: u32::try_from(payload.len())
                    .map_err(|_| MfEncoderError::OutputTooLarge)?,
                flags: EncodedAccessUnitFlags {
                    key_frame,
                    discontinuity: false,
                    end_of_stream: false,
                },
            },
            payload,
        };
        unit.validate(&self.selected)?;
        Ok(Some(unit))
    }
}

impl Drop for MediaFoundationH264Encoder {
    fn drop(&mut self) {
        if let Some(transform) = self.transform.take() {
            if self.mf_started {
                let _ = unsafe { transform.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0) };
                let _ = unsafe { transform.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0) };
                let _ = unsafe { transform.ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0) };
                let _ = unsafe { transform.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0) };
            }
            drop(transform);
        }
        if self.mf_started {
            let _ = unsafe { MFShutdown() };
            self.mf_started = false;
        }
        if self.com_initialized {
            unsafe { CoUninitialize() };
            self.com_initialized = false;
        }
    }
}

unsafe fn build_transform(
    selected: &EncodedDisplayStreamSpec,
    config: MediaFoundationH264EncoderConfig,
) -> Result<IMFTransform, MfEncoderError> {
    let transform: IMFTransform =
        unsafe { CoCreateInstance(&CLSID_MSH264EncoderMFT, None, CLSCTX_INPROC_SERVER) }
            .map_err(|source| windows_error("create Microsoft H.264 encoder MFT", source))?;
    let codec_api: ICodecAPI = transform
        .cast()
        .map_err(|source| windows_error("query H.264 codec controls", source))?;
    unsafe {
        try_set_codec_property(&codec_api, &CODECAPI_AVLowLatencyMode, true.into());
        try_set_codec_property(&codec_api, &CODECAPI_AVEncCommonRealTime, true.into());
        try_set_codec_property(
            &codec_api,
            &CODECAPI_AVEncMPVDefaultBPictureCount,
            0_u32.into(),
        );
        try_set_codec_property(
            &codec_api,
            &CODECAPI_AVEncCommonRateControlMode,
            4_u32.into(),
        );
    }

    let output = unsafe { MFCreateMediaType() }
        .map_err(|source| windows_error("create H.264 output type", source))?;
    let output_attributes: IMFAttributes = output
        .cast()
        .map_err(|source| windows_error("query H.264 output attributes", source))?;
    unsafe {
        configure_video_type(&output_attributes, selected, &MFVideoFormat_H264)?;
        output_attributes.SetUINT32(&MF_MT_AVG_BITRATE, config.average_bitrate)
    }
    .map_err(|source| windows_error("set H.264 bitrate", source))?;
    unsafe {
        output_attributes.SetUINT32(&MF_MT_MPEG2_PROFILE, profile_value(selected.avc_profile))
    }
    .map_err(|source| windows_error("set H.264 profile", source))?;
    unsafe { output_attributes.SetUINT32(&MF_MT_MPEG2_LEVEL, level_value(selected.avc_level)) }
        .map_err(|source| windows_error("set H.264 level", source))?;
    unsafe { transform.SetOutputType(0, &output, 0) }
        .map_err(|source| windows_error("set H.264 output type", source))?;

    let input = unsafe { MFCreateMediaType() }
        .map_err(|source| windows_error("create NV12 input type", source))?;
    let input_attributes: IMFAttributes = input
        .cast()
        .map_err(|source| windows_error("query NV12 input attributes", source))?;
    unsafe { configure_video_type(&input_attributes, selected, &MFVideoFormat_NV12) }?;
    unsafe { transform.SetInputType(0, &input, 0) }
        .map_err(|source| windows_error("set NV12 input type", source))?;
    unsafe { transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0) }
        .map_err(|source| windows_error("begin H.264 streaming", source))?;
    unsafe { transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0) }
        .map_err(|source| windows_error("start H.264 stream", source))?;
    Ok(transform)
}

unsafe fn try_set_codec_property(
    codec_api: &ICodecAPI,
    property: &windows::core::GUID,
    value: VARIANT,
) {
    if unsafe { codec_api.IsSupported(property) }.is_ok() {
        // Codec MFTs in the field sometimes report a control as supported and
        // still return E_NOTIMPL from SetValue. These controls are latency
        // hints, not requirements for correct H.264 output, so preserve the
        // interoperable baseline when an individual hint is unavailable.
        let _ = unsafe { codec_api.SetValue(property, &value) };
    }
}

unsafe fn configure_video_type(
    attributes: &IMFAttributes,
    selected: &EncodedDisplayStreamSpec,
    subtype: &windows::core::GUID,
) -> Result<(), MfEncoderError> {
    unsafe { attributes.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video) }
        .map_err(|source| windows_error("set video major type", source))?;
    unsafe { attributes.SetGUID(&MF_MT_SUBTYPE, subtype) }
        .map_err(|source| windows_error("set video subtype", source))?;
    unsafe {
        attributes.SetUINT64(
            &MF_MT_FRAME_SIZE,
            pack_ratio(selected.decoded.width, selected.decoded.height),
        )
    }
    .map_err(|source| windows_error("set video frame size", source))?;
    unsafe {
        attributes.SetUINT64(
            &MF_MT_FRAME_RATE,
            pack_ratio(
                selected.decoded.frame_rate.numerator(),
                selected.decoded.frame_rate.denominator(),
            ),
        )
    }
    .map_err(|source| windows_error("set video frame rate", source))?;
    unsafe { attributes.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack_ratio(1, 1)) }
        .map_err(|source| windows_error("set pixel aspect ratio", source))?;
    unsafe { attributes.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32) }
        .map_err(|source| windows_error("set progressive video mode", source))?;
    Ok(())
}

unsafe fn sample_from_bytes(
    bytes: &[u8],
    sample_time: i64,
    sample_duration: i64,
) -> Result<IMFSample, MfEncoderError> {
    let length = u32::try_from(bytes.len()).map_err(|_| MfEncoderError::OutputTooLarge)?;
    let buffer = unsafe { MFCreateMemoryBuffer(length) }
        .map_err(|source| windows_error("create NV12 input buffer", source))?;
    let mut destination = ptr::null_mut();
    unsafe { buffer.Lock(&mut destination, None, None) }
        .map_err(|source| windows_error("lock NV12 input buffer", source))?;
    unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len()) };
    let unlock = unsafe { buffer.Unlock() };
    unlock.map_err(|source| windows_error("unlock NV12 input buffer", source))?;
    unsafe { buffer.SetCurrentLength(length) }
        .map_err(|source| windows_error("set NV12 input length", source))?;
    let sample = unsafe { MFCreateSample() }
        .map_err(|source| windows_error("create NV12 input sample", source))?;
    unsafe { sample.AddBuffer(&buffer) }
        .map_err(|source| windows_error("attach NV12 input buffer", source))?;
    unsafe { sample.SetSampleTime(sample_time) }
        .map_err(|source| windows_error("set NV12 input timestamp", source))?;
    unsafe { sample.SetSampleDuration(sample_duration) }
        .map_err(|source| windows_error("set NV12 input duration", source))?;
    Ok(sample)
}

unsafe fn bytes_from_sample(sample: &IMFSample) -> Result<Vec<u8>, MfEncoderError> {
    let buffer: IMFMediaBuffer = unsafe { sample.ConvertToContiguousBuffer() }
        .map_err(|source| windows_error("coalesce H.264 output", source))?;
    let length = unsafe { buffer.GetCurrentLength() }
        .map_err(|source| windows_error("read H.264 output length", source))?;
    let mut source_ptr = ptr::null_mut();
    unsafe { buffer.Lock(&mut source_ptr, None, None) }
        .map_err(|source| windows_error("lock H.264 output", source))?;
    let bytes = unsafe {
        std::slice::from_raw_parts(
            source_ptr,
            usize::try_from(length).map_err(|_| MfEncoderError::OutputTooLarge)?,
        )
        .to_vec()
    };
    unsafe { buffer.Unlock() }.map_err(|source| windows_error("unlock H.264 output", source))?;
    Ok(bytes)
}

fn parse_sequence_header(bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>), MfEncoderError> {
    let units = if bytes.first() == Some(&1) {
        parse_avcc_configuration(bytes)?
    } else {
        split_annex_b(bytes)?
    };
    let sps = units
        .iter()
        .find(|unit| unit.first().is_some_and(|byte| byte & 0x1f == 7))
        .ok_or_else(|| MfEncoderError::InvalidAvc("sequence header lacks SPS".to_owned()))?;
    let pps = units
        .iter()
        .find(|unit| unit.first().is_some_and(|byte| byte & 0x1f == 8))
        .ok_or_else(|| MfEncoderError::InvalidAvc("sequence header lacks PPS".to_owned()))?;
    Ok((annex_b_unit(sps), annex_b_unit(pps)))
}

fn normalize_access_unit(bytes: &[u8]) -> Result<Vec<u8>, MfEncoderError> {
    let units = if starts_with_annex_b(bytes) {
        split_annex_b(bytes)?
    } else {
        split_length_prefixed(bytes)?
    };
    if units.is_empty() {
        return Err(MfEncoderError::InvalidAvc(
            "encoded access unit contains no NAL units".to_owned(),
        ));
    }
    Ok(units.iter().flat_map(|unit| annex_b_unit(unit)).collect())
}

fn parse_avcc_configuration(bytes: &[u8]) -> Result<Vec<Vec<u8>>, MfEncoderError> {
    if bytes.len() < 7 {
        return Err(MfEncoderError::InvalidAvc(
            "AVCDecoderConfigurationRecord is truncated".to_owned(),
        ));
    }
    let mut offset = 6;
    let sps_count = usize::from(bytes[5] & 0x1f);
    let mut units = Vec::new();
    for _ in 0..sps_count {
        units.push(read_u16_unit(bytes, &mut offset)?);
    }
    let pps_count = *bytes.get(offset).ok_or_else(|| {
        MfEncoderError::InvalidAvc("AVC configuration lacks PPS count".to_owned())
    })?;
    offset += 1;
    for _ in 0..pps_count {
        units.push(read_u16_unit(bytes, &mut offset)?);
    }
    Ok(units)
}

fn read_u16_unit(bytes: &[u8], offset: &mut usize) -> Result<Vec<u8>, MfEncoderError> {
    let length_bytes = bytes.get(*offset..*offset + 2).ok_or_else(|| {
        MfEncoderError::InvalidAvc("AVC configuration NAL length is truncated".to_owned())
    })?;
    *offset += 2;
    let length = usize::from(u16::from_be_bytes(
        length_bytes.try_into().expect("two bytes"),
    ));
    if length == 0 {
        return Err(MfEncoderError::InvalidAvc(
            "AVC configuration contains an empty NAL unit".to_owned(),
        ));
    }
    let unit = bytes
        .get(*offset..*offset + length)
        .ok_or_else(|| MfEncoderError::InvalidAvc("AVC configuration NAL is truncated".to_owned()))?
        .to_vec();
    *offset += length;
    Ok(unit)
}

fn split_length_prefixed(bytes: &[u8]) -> Result<Vec<Vec<u8>>, MfEncoderError> {
    let mut offset = 0;
    let mut units = Vec::new();
    while offset < bytes.len() {
        let length_bytes = bytes.get(offset..offset + 4).ok_or_else(|| {
            MfEncoderError::InvalidAvc("length-prefixed AVC header is truncated".to_owned())
        })?;
        offset += 4;
        let length = usize::try_from(u32::from_be_bytes(
            length_bytes.try_into().expect("four bytes"),
        ))
        .map_err(|_| MfEncoderError::OutputTooLarge)?;
        if length == 0 {
            return Err(MfEncoderError::InvalidAvc(
                "length-prefixed AVC contains an empty NAL".to_owned(),
            ));
        }
        let unit = bytes
            .get(offset..offset + length)
            .ok_or_else(|| MfEncoderError::InvalidAvc("AVC NAL is truncated".to_owned()))?
            .to_vec();
        offset += length;
        units.push(unit);
    }
    Ok(units)
}

fn split_annex_b(bytes: &[u8]) -> Result<Vec<Vec<u8>>, MfEncoderError> {
    let mut starts = Vec::new();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let length = if bytes.get(index..index + 4) == Some(&[0, 0, 0, 1]) {
            4
        } else if bytes.get(index..index + 3) == Some(&[0, 0, 1]) {
            3
        } else {
            index += 1;
            continue;
        };
        starts.push((index, length));
        index += length;
    }
    if starts.is_empty() {
        return Err(MfEncoderError::InvalidAvc(
            "AVC data has neither Annex-B nor length prefixes".to_owned(),
        ));
    }
    let mut units = Vec::new();
    for (position, (start, prefix)) in starts.iter().enumerate() {
        let end = starts
            .get(position + 1)
            .map_or(bytes.len(), |(next, _)| *next);
        let unit = &bytes[start + prefix..end];
        if !unit.is_empty() {
            units.push(unit.to_vec());
        }
    }
    Ok(units)
}

fn starts_with_annex_b(bytes: &[u8]) -> bool {
    bytes.starts_with(&ANNEX_B_START) || bytes.starts_with(&[0, 0, 1])
}

fn annex_b_unit(unit: &[u8]) -> Vec<u8> {
    ANNEX_B_START
        .into_iter()
        .chain(unit.iter().copied())
        .collect()
}

const fn pack_ratio(numerator: u32, denominator: u32) -> u64 {
    ((numerator as u64) << 32) | denominator as u64
}

fn profile_value(profile: AvcProfile) -> u32 {
    match profile {
        AvcProfile::Baseline => 66,
        AvcProfile::Main => 77,
        AvcProfile::High => 100,
    }
}

fn level_value(level: AvcLevel) -> u32 {
    match level {
        AvcLevel::L3_1 => 31,
        AvcLevel::L4 => 40,
        AvcLevel::L4_1 => 41,
        AvcLevel::L4_2 => 42,
        AvcLevel::L5 => 50,
        AvcLevel::L5_1 => 51,
        AvcLevel::L5_2 => 52,
    }
}

fn frame_duration_nanos(selected: &EncodedDisplayStreamSpec) -> Result<u64, MfEncoderError> {
    1_000_000_000_u128
        .checked_mul(u128::from(selected.decoded.frame_rate.denominator()))
        .and_then(|value| value.checked_div(u128::from(selected.decoded.frame_rate.numerator())))
        .and_then(|value| u64::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or(MfEncoderError::ArithmeticExhausted)
}

fn windows_error(operation: &'static str, source: windows::core::Error) -> MfEncoderError {
    MfEncoderError::Windows { operation, source }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IddFrameHeader;

    #[test]
    fn avcc_configuration_extracts_annex_b_sps_and_pps() {
        let avcc = [1, 66, 0, 31, 0xff, 0xe1, 0, 2, 0x67, 1, 1, 0, 2, 0x68, 2];
        let (sps, pps) = parse_sequence_header(&avcc).unwrap();
        assert_eq!(sps, [0, 0, 0, 1, 0x67, 1]);
        assert_eq!(pps, [0, 0, 0, 1, 0x68, 2]);
    }

    #[test]
    fn three_byte_annex_b_and_length_prefix_normalize() {
        assert_eq!(
            normalize_access_unit(&[0, 0, 1, 0x65, 1, 0, 0, 1, 0x41, 2]).unwrap(),
            [0, 0, 0, 1, 0x65, 1, 0, 0, 0, 1, 0x41, 2]
        );
        assert_eq!(
            normalize_access_unit(&[0, 0, 0, 2, 0x65, 1]).unwrap(),
            [0, 0, 0, 1, 0x65, 1]
        );
    }

    #[test]
    fn microsoft_encoder_emits_bounded_annex_b_and_decoder_config() {
        let selected = EncodedDisplayStreamSpec::fixture_720p30_avc();
        let stream_id = StreamId::new();
        let mut encoder = MediaFoundationH264Encoder::new(
            selected.clone(),
            stream_id,
            1,
            MediaFoundationH264EncoderConfig::default(),
        )
        .unwrap();
        let config = encoder.decoder_config().unwrap();
        assert_eq!(config.stream_id, stream_id);

        let payload = vec![0_u8; 1280 * 720 * 4];
        let mut units = Vec::new();
        for index in 0..1_u64 {
            units.extend(
                encoder
                    .encode(&IddFrameSnapshot {
                        header: IddFrameHeader {
                            generation: 1,
                            sequence_lock: (index + 1) * 2,
                            width: 1280,
                            height: 720,
                            stride_bytes: 1280 * 4,
                            pixel_format: 1,
                            payload_bytes: payload.len() as u64,
                            source_timestamp_nanos: index * 33_333_333,
                            flags: 0,
                        },
                        payload: payload.clone(),
                    })
                    .unwrap(),
            );
            if !units.is_empty() {
                break;
            }
        }
        assert!(!units.is_empty());
        assert!(units[0].payload.starts_with(&ANNEX_B_START));
        units[0].validate(&selected).unwrap();
    }

    #[test]
    fn microsoft_encoder_accepts_exact_phone_aspect_level_four_mode() {
        let mut selected = EncodedDisplayStreamSpec::fixture_720p30_avc();
        selected.decoded.width = 1_980;
        selected.decoded.height = 912;
        selected.avc_level = AvcLevel::L4;
        assert!(matches!(
            MediaFoundationH264Encoder::new(
                selected.clone(),
                StreamId::new(),
                1,
                MediaFoundationH264EncoderConfig::default(),
            ),
            Err(MfEncoderError::OutputTooLarge)
        ));
        selected.maximum_access_unit_bytes = 4 * 1024 * 1024;
        selected.validate().unwrap();

        let mut encoder = MediaFoundationH264Encoder::new(
            selected.clone(),
            StreamId::new(),
            1,
            MediaFoundationH264EncoderConfig::default(),
        )
        .unwrap();
        let config = encoder.decoder_config().unwrap();
        assert_eq!(config.selected.avc_level, AvcLevel::L4);
        assert_eq!(config.selected.decoded.width, 1_980);
        assert_eq!(config.selected.decoded.height, 912);

        let payload = vec![0_u8; 1_980 * 912 * 4];
        let units = encoder
            .encode(&IddFrameSnapshot {
                header: IddFrameHeader {
                    generation: 1,
                    sequence_lock: 2,
                    width: 1_980,
                    height: 912,
                    stride_bytes: 1_980 * 4,
                    pixel_format: 1,
                    payload_bytes: payload.len() as u64,
                    source_timestamp_nanos: 0,
                    flags: 0,
                },
                payload,
            })
            .unwrap();
        assert!(!units.is_empty());
        units[0].validate(&selected).unwrap();
    }
}
