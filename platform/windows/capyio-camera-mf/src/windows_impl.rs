use std::{
    collections::VecDeque,
    ptr,
    str::FromStr,
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, TryLockError,
        atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering},
    },
};

use capyio_core::StreamId;
use capyio_windows_camera::{
    DeterministicNv12Source, ExternalNv12FrameIngress, GeneratedVideoFrame,
    MAX_PENDING_SAMPLE_REQUESTS, MF_CAMERA_STREAM_ID, MfMediaSourceCore, MfSampleTimingMapper,
    copy_nv12_to_strided_buffer, fixture_stream_spec,
};

use crate::{CameraSharedIngressConsumer, CameraSharedIngressError, com_server::ComServerLease};
use windows::{
    Win32::{
        Foundation::{
            E_INVALIDARG, E_NOTIMPL, E_POINTER, E_UNEXPECTED, ERROR_FILE_NOT_FOUND,
            ERROR_SET_NOT_FOUND, S_OK,
        },
        Media::{
            KernelStreaming::{
                IKsControl, IKsControl_Impl, KSCAMERAPROFILE_Legacy, KSIDENTIFIER,
                PINNAME_VIDEO_CAPTURE,
            },
            MediaFoundation::{
                IMF2DBuffer2, IMFAsyncCallback, IMFAsyncCallback_Impl, IMFAsyncResult,
                IMFAttributes, IMFGetService, IMFGetService_Impl, IMFMediaBuffer, IMFMediaEvent,
                IMFMediaEventGenerator_Impl, IMFMediaEventQueue, IMFMediaSource,
                IMFMediaSource_Impl, IMFMediaSourceEx, IMFMediaSourceEx_Impl, IMFMediaStream_Impl,
                IMFMediaStream2, IMFMediaStream2_Impl, IMFMediaType, IMFPresentationDescriptor,
                IMFSample, IMFSampleAllocatorControl, IMFSampleAllocatorControl_Impl,
                IMFStreamDescriptor, IMFVideoSampleAllocator,
                MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS, MEError, MEMediaSample, MENewStream,
                MESourceStarted, MESourceStopped, MEStreamStarted, MEStreamStopped,
                MEUpdatedStream, MF_DEVICEMFT_SENSORPROFILE_COLLECTION,
                MF_DEVICESTREAM_ATTRIBUTE_FRAMESOURCE_TYPES, MF_DEVICESTREAM_FRAMESERVER_SHARED,
                MF_DEVICESTREAM_STREAM_CATEGORY, MF_DEVICESTREAM_STREAM_ID,
                MF_E_INVALID_STATE_TRANSITION, MF_E_INVALIDREQUEST, MF_E_NOT_INITIALIZED,
                MF_E_NOTACCEPTING, MF_E_SHUTDOWN, MF_E_UNSUPPORTED_SERVICE,
                MF_E_UNSUPPORTED_TIME_FORMAT, MF_MT_ALL_SAMPLES_INDEPENDENT,
                MF_MT_FIXED_SIZE_SAMPLES, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE,
                MF_MT_MAJOR_TYPE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SAMPLE_SIZE, MF_MT_SUBTYPE,
                MF_STREAM_STATE, MF_STREAM_STATE_PAUSED, MF_STREAM_STATE_RUNNING,
                MF_STREAM_STATE_STOPPED, MF_VERSION, MFASYNC_CALLBACK_QUEUE_STANDARD,
                MFAllocateSerialWorkQueue, MFCreateAttributes, MFCreateEventQueue,
                MFCreateMediaType, MFCreatePresentationDescriptor, MFCreateSensorProfile,
                MFCreateSensorProfileCollection, MFCreateStreamDescriptor,
                MFFrameSourceTypes_Color, MFGetSystemTime, MFMEDIASOURCE_IS_LIVE,
                MFMediaType_Video, MFPutWorkItem, MFSTARTUP_FULL, MFSampleAllocatorUsage,
                MFSampleAllocatorUsage_UsesProvidedAllocator, MFSampleExtension_Discontinuity,
                MFSampleExtension_Token, MFScheduleWorkItem, MFShutdown, MFStartup,
                MFUnlockWorkQueue, MFVideoFormat_NV12, MFVideoInterlace_Progressive,
            },
        },
        System::Com::{
            COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize, StructuredStorage::PROPVARIANT,
        },
    },
    core::{BOOL, ComObject, Error, GUID, IUnknown, Interface, PCWSTR, Ref, Result, Weak, w},
};

const CAMERA_STREAM_ID: &str = "00000000-0000-4000-8000-00000000c012";
const FRAME_SERVER_SAMPLE_POOL_SIZE: u32 = 10;
const SHARED_SAMPLE_POLL_MILLISECONDS: i64 = -5;
const LATE_SHARED_PROBE_INTERVAL_PLACEHOLDER_FRAMES: u32 = 15;
const LATE_SHARED_MAX_EMPTY_LIVE_POLLS: u32 = 400;

pub struct MediaFoundationRuntime {
    active: bool,
}

impl MediaFoundationRuntime {
    pub fn startup() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            if let Err(error) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
                CoUninitialize();
                return Err(error);
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

#[derive(Clone)]
pub struct CapyIoMediaSourceHandle {
    source: IMFMediaSourceEx,
    stream: IMFMediaStream2,
}

impl CapyIoMediaSourceHandle {
    #[must_use]
    pub fn source(&self) -> &IMFMediaSourceEx {
        &self.source
    }

    #[must_use]
    pub fn stream(&self) -> &IMFMediaStream2 {
        &self.stream
    }
}

pub fn create_in_process_media_source() -> Result<CapyIoMediaSourceHandle> {
    let attributes = create_attributes(1)?;
    create_media_source_with_attributes(&attributes, FrameProvider::Fixture { generator: None })
}

/// Builds a non-registered source that consumes a caller-owned decoded-frame
/// ingress. This is an integration seam for worker/process-boundary tests.
pub fn create_in_process_media_source_with_external_ingress(
    ingress: Arc<Mutex<ExternalNv12FrameIngress>>,
) -> Result<CapyIoMediaSourceHandle> {
    let attributes = create_attributes(1)?;
    create_media_source_with_attributes(&attributes, FrameProvider::External(ingress))
}

/// Builds a non-registered source backed by the versioned, read-only camera
/// shared-memory consumer.
pub fn create_in_process_media_source_with_shared_ingress(
    consumer: CameraSharedIngressConsumer,
) -> Result<CapyIoMediaSourceHandle> {
    let attributes = create_attributes(1)?;
    create_media_source_with_attributes(&attributes, FrameProvider::Shared(consumer))
}

pub(crate) fn create_registered_media_source_with_attributes(
    activation_attributes: &IMFAttributes,
) -> Result<CapyIoMediaSourceHandle> {
    let shared = match CameraSharedIngressConsumer::open_current() {
        Ok(consumer) => Some(consumer),
        Err(error) if shared_mapping_is_absent(&error) => None,
        Err(_) => return Err(hresult(E_UNEXPECTED)),
    };
    let provider = FrameProvider::LateShared(LateSharedFrameProvider::production(shared));
    create_media_source_with_attributes(activation_attributes, provider)
}

fn shared_mapping_is_absent(error: &CameraSharedIngressError) -> bool {
    matches!(
        error,
        CameraSharedIngressError::Windows {
            operation: "OpenFileMappingW",
            code,
        } if *code == ERROR_FILE_NOT_FOUND.0
    )
}

fn create_media_source_with_attributes(
    activation_attributes: &IMFAttributes,
    frame_provider: FrameProvider,
) -> Result<CapyIoMediaSourceHandle> {
    let uses_async_shared_pump = frame_provider.uses_async_sample_pump();
    let media_type = create_canonical_media_type()?;
    let stream_attributes = create_attributes(4)?;
    set_stream_attributes(&stream_attributes)?;
    let stream_descriptor =
        unsafe { MFCreateStreamDescriptor(MF_CAMERA_STREAM_ID, &[Some(media_type.clone())])? };
    set_stream_attributes(&stream_descriptor)?;
    unsafe {
        stream_descriptor
            .GetMediaTypeHandler()?
            .SetCurrentMediaType(&media_type)?;
    }
    let presentation_descriptor =
        unsafe { MFCreatePresentationDescriptor(Some(&[Some(stream_descriptor.clone())]))? };
    unsafe { presentation_descriptor.SelectStream(0)? };

    let runtime = Arc::new(Mutex::new(StreamRuntime::new(frame_provider)));
    let stream_event_queue = unsafe { MFCreateEventQueue()? };
    // The two COM identities share this state, but Rust never moves the Arc to
    // a Rust thread. Frame Server owns COM dispatch under ThreadingModel=Both,
    // and every allocator access is serialized or uses a non-blocking try_lock.
    // Resolving an AgileReference per sample could allocate on the request path.
    #[allow(clippy::arc_with_non_send_sync)]
    let stream_shared = Arc::new(StreamShared {
        runtime: Arc::clone(&runtime),
        sample_allocator: Mutex::new(None),
        allocator_initialized: AtomicBool::new(false),
        state: AtomicI32::new(MF_STREAM_STATE_STOPPED.0),
        shutdown: AtomicBool::new(false),
    });
    let shared_sample_pump = uses_async_shared_pump
        .then(|| {
            SharedSamplePumpController::new(Arc::clone(&stream_shared), stream_event_queue.clone())
        })
        .transpose()?;
    let source_attributes = create_attributes(4)?;
    unsafe { activation_attributes.CopyAllItems(&source_attributes)? };
    add_legacy_sensor_profile(&source_attributes)?;
    let source_object = ComObject::new(CapyIoMediaSource {
        _server_lease: ComServerLease::new()?,
        event_queue: unsafe { MFCreateEventQueue()? },
        attributes: source_attributes,
        presentation_descriptor,
        runtime,
        stream_shared: Arc::clone(&stream_shared),
        stream_event_queue: stream_event_queue.clone(),
        stream_attributes: stream_attributes.clone(),
        stream: OnceLock::new(),
        shutdown: AtomicBool::new(false),
    });
    let source: IMFMediaSourceEx = source_object.to_interface();
    let source_base: IMFMediaSource = source.cast()?;
    let source_weak = source_base.downgrade()?;
    let stream: IMFMediaStream2 = CapyIoMediaStream {
        _server_lease: ComServerLease::new()?,
        shared: stream_shared,
        event_queue: stream_event_queue,
        descriptor: stream_descriptor,
        source: source_weak,
        shared_sample_pump,
    }
    .into();
    source_object
        .stream
        .set(stream.clone())
        .map_err(|_| hresult(E_UNEXPECTED))?;

    Ok(CapyIoMediaSourceHandle { source, stream })
}

fn create_attributes(capacity: u32) -> Result<IMFAttributes> {
    let mut attributes = None;
    unsafe { MFCreateAttributes(&mut attributes, capacity)? };
    attributes.ok_or_else(|| hresult(E_UNEXPECTED))
}

fn create_canonical_media_type() -> Result<IMFMediaType> {
    let selected = fixture_stream_spec();
    let media_type = unsafe { MFCreateMediaType()? };
    unsafe {
        media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        media_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        media_type.SetUINT64(
            &MF_MT_FRAME_SIZE,
            pack_ratio(selected.width, selected.height),
        )?;
        media_type.SetUINT64(
            &MF_MT_FRAME_RATE,
            pack_ratio(
                selected.frame_rate.numerator(),
                selected.frame_rate.denominator(),
            ),
        )?;
        media_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack_ratio(1, 1))?;
        media_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        media_type.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
        media_type.SetUINT32(&MF_MT_FIXED_SIZE_SAMPLES, 1)?;
        media_type.SetUINT32(
            &MF_MT_SAMPLE_SIZE,
            u32::try_from(
                selected
                    .packed_frame_bytes()
                    .ok_or_else(|| hresult(E_UNEXPECTED))?,
            )
            .map_err(|_| hresult(E_UNEXPECTED))?,
        )?;
    }
    Ok(media_type)
}

fn set_stream_attributes(attributes: &IMFAttributes) -> Result<()> {
    unsafe {
        attributes.SetGUID(&MF_DEVICESTREAM_STREAM_CATEGORY, &PINNAME_VIDEO_CAPTURE)?;
        attributes.SetUINT32(&MF_DEVICESTREAM_STREAM_ID, MF_CAMERA_STREAM_ID)?;
        attributes.SetUINT32(&MF_DEVICESTREAM_FRAMESERVER_SHARED, 1)?;
        attributes.SetUINT32(
            &MF_DEVICESTREAM_ATTRIBUTE_FRAMESOURCE_TYPES,
            MFFrameSourceTypes_Color.0 as u32,
        )?;
    }
    Ok(())
}

fn add_legacy_sensor_profile(attributes: &IMFAttributes) -> Result<()> {
    let profiles = unsafe { MFCreateSensorProfileCollection()? };
    let legacy = unsafe { MFCreateSensorProfile(&KSCAMERAPROFILE_Legacy, 0, PCWSTR::null())? };
    unsafe {
        legacy.AddProfileFilter(MF_CAMERA_STREAM_ID, w!("((RES==;FRT<=30,1;SUT==))"))?;
        profiles.AddProfile(&legacy)?;
        attributes.SetUnknown(&MF_DEVICEMFT_SENSORPROFILE_COLLECTION, &profiles)
    }
}

const fn pack_ratio(numerator: u32, denominator: u32) -> u64 {
    ((numerator as u64) << 32) | denominator as u64
}

enum RegisteredSharedMappingTarget {
    Production,
    #[cfg(test)]
    LocalTest(String),
}

impl RegisteredSharedMappingTarget {
    fn open_current(
        &self,
    ) -> std::result::Result<CameraSharedIngressConsumer, CameraSharedIngressError> {
        match self {
            Self::Production => CameraSharedIngressConsumer::open_current(),
            #[cfg(test)]
            Self::LocalTest(mapping_name) => {
                CameraSharedIngressConsumer::open_local_test_current(mapping_name)
            }
        }
    }

    fn initial_probe_delay(&self) -> u32 {
        match self {
            Self::Production => LATE_SHARED_PROBE_INTERVAL_PLACEHOLDER_FRAMES,
            #[cfg(test)]
            Self::LocalTest(_) => 0,
        }
    }
}

struct LateSharedOutputTimeline {
    stream_id: StreamId,
    stream_epoch: u64,
    next_sequence: u64,
    next_source_timestamp_nanos: u64,
}

impl LateSharedOutputTimeline {
    const fn new(stream_id: StreamId, stream_epoch: u64) -> Self {
        Self {
            stream_id,
            stream_epoch,
            next_sequence: 0,
            next_source_timestamp_nanos: 0,
        }
    }

    fn observe_placeholder(&mut self, frame: &GeneratedVideoFrame) -> Result<()> {
        if frame.descriptor.stream_id != self.stream_id
            || frame.descriptor.stream_epoch != self.stream_epoch
            || frame.descriptor.sequence != self.next_sequence
            || frame.descriptor.source_timestamp_nanos != self.next_source_timestamp_nanos
        {
            return Err(hresult(E_UNEXPECTED));
        }
        self.advance(frame.descriptor.duration_nanos)
    }

    fn rebase_live_frame(
        &mut self,
        mut frame: GeneratedVideoFrame,
        first_live_frame: bool,
    ) -> Result<GeneratedVideoFrame> {
        frame.descriptor.stream_id = self.stream_id;
        frame.descriptor.stream_epoch = self.stream_epoch;
        frame.descriptor.sequence = self.next_sequence;
        frame.descriptor.source_timestamp_nanos = self.next_source_timestamp_nanos;
        frame.descriptor.flags.discontinuity |= first_live_frame;
        frame
            .validate(&fixture_stream_spec())
            .map_err(|_| hresult(E_UNEXPECTED))?;
        self.advance(frame.descriptor.duration_nanos)?;
        Ok(frame)
    }

    fn advance(&mut self, duration_nanos: u64) -> Result<()> {
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or_else(|| hresult(E_UNEXPECTED))?;
        self.next_source_timestamp_nanos = self
            .next_source_timestamp_nanos
            .checked_add(duration_nanos)
            .ok_or_else(|| hresult(E_UNEXPECTED))?;
        Ok(())
    }

    fn resume_placeholder_source(&self) -> Result<DeterministicNv12Source> {
        DeterministicNv12Source::new_at_sequence(
            self.stream_id,
            self.stream_epoch,
            self.next_sequence,
            self.next_source_timestamp_nanos,
        )
        .map_err(|_| hresult(E_UNEXPECTED))
    }
}

#[derive(Clone, Copy)]
struct LateSharedSourceCursor {
    generation: u64,
    stream_id: StreamId,
    stream_epoch: u64,
    sequence: u64,
    source_timestamp_nanos: u64,
}

impl LateSharedSourceCursor {
    fn new(consumer: &CameraSharedIngressConsumer, frame: &GeneratedVideoFrame) -> Self {
        Self {
            generation: consumer.generation(),
            stream_id: consumer.stream_id(),
            stream_epoch: consumer.stream_epoch(),
            sequence: frame.descriptor.sequence,
            source_timestamp_nanos: frame.descriptor.source_timestamp_nanos,
        }
    }

    fn is_not_newer_than(self, previous: Self) -> bool {
        self.generation == previous.generation
            && self.stream_id == previous.stream_id
            && self.stream_epoch == previous.stream_epoch
            && (self.sequence <= previous.sequence
                || self.source_timestamp_nanos <= previous.source_timestamp_nanos)
    }
}

struct LateSharedFrameProvider {
    target: RegisteredSharedMappingTarget,
    placeholder: Option<DeterministicNv12Source>,
    shared: Option<CameraSharedIngressConsumer>,
    output_timeline: Option<LateSharedOutputTimeline>,
    placeholder_frames_until_probe: u32,
    live_started: bool,
    empty_live_polls: u32,
    last_live_source: Option<LateSharedSourceCursor>,
    placeholder_discontinuity_pending: bool,
}

impl LateSharedFrameProvider {
    fn production(shared: Option<CameraSharedIngressConsumer>) -> Self {
        Self {
            target: RegisteredSharedMappingTarget::Production,
            placeholder: None,
            shared,
            output_timeline: None,
            placeholder_frames_until_probe: LATE_SHARED_PROBE_INTERVAL_PLACEHOLDER_FRAMES,
            live_started: false,
            empty_live_polls: 0,
            last_live_source: None,
            placeholder_discontinuity_pending: false,
        }
    }

    #[cfg(test)]
    fn local_test(mapping_name: String) -> Self {
        Self {
            target: RegisteredSharedMappingTarget::LocalTest(mapping_name),
            placeholder: None,
            shared: None,
            output_timeline: None,
            placeholder_frames_until_probe: 0,
            live_started: false,
            empty_live_polls: 0,
            last_live_source: None,
            placeholder_discontinuity_pending: false,
        }
    }

    fn reset_for_start(&mut self, stream_id: StreamId, stream_epoch: u64) -> Result<()> {
        self.placeholder = Some(
            DeterministicNv12Source::new(stream_id, stream_epoch, 0)
                .map_err(|_| hresult(E_UNEXPECTED))?,
        );
        self.output_timeline = Some(LateSharedOutputTimeline::new(stream_id, stream_epoch));
        if self.shared.is_none() {
            self.placeholder_frames_until_probe = self.target.initial_probe_delay();
        }
        self.live_started = false;
        self.empty_live_polls = 0;
        self.placeholder_discontinuity_pending = false;
        Ok(())
    }

    fn clear_frames(&mut self) {
        self.placeholder = None;
        self.shared = None;
        self.output_timeline = None;
        self.live_started = false;
        self.empty_live_polls = 0;
        self.placeholder_discontinuity_pending = false;
    }

    fn next_frame(&mut self) -> Result<GeneratedVideoFrame> {
        if self.shared.is_none() && self.should_probe() {
            match self.target.open_current() {
                Ok(consumer) => self.shared = Some(consumer),
                Err(error) if shared_mapping_is_absent(&error) => {}
                Err(_) => return Err(hresult(E_UNEXPECTED)),
            }
        }

        let live_frame = match self.shared.as_mut() {
            Some(consumer) => {
                let frame = consumer
                    .try_read_latest()
                    .map_err(|_| hresult(E_UNEXPECTED))?;
                frame.map(|frame| {
                    let cursor = LateSharedSourceCursor::new(consumer, &frame);
                    (cursor, frame)
                })
            }
            None => None,
        };
        if let Some((cursor, frame)) = live_frame {
            if self
                .last_live_source
                .is_some_and(|previous| cursor.is_not_newer_than(previous))
            {
                if self.live_started {
                    return Err(hresult(E_UNEXPECTED));
                }
                self.shared = None;
            } else {
                let first_live_frame = !self.live_started;
                let frame = self
                    .output_timeline
                    .as_mut()
                    .ok_or_else(|| hresult(MF_E_NOTACCEPTING))?
                    .rebase_live_frame(frame, first_live_frame)?;
                self.live_started = true;
                self.empty_live_polls = 0;
                self.last_live_source = Some(cursor);
                self.placeholder_discontinuity_pending = false;
                return Ok(frame);
            }
        } else if self.live_started {
            self.empty_live_polls = self
                .empty_live_polls
                .checked_add(1)
                .ok_or_else(|| hresult(E_UNEXPECTED))?;
            if self.empty_live_polls < LATE_SHARED_MAX_EMPTY_LIVE_POLLS {
                return Err(hresult(MF_E_NOTACCEPTING));
            }
            self.fall_back_to_placeholder()?;
        } else if self.shared.is_some() {
            // A newly opened mapping with no publication must not remain held
            // open while placeholder mode waits for a producer.
            self.shared = None;
        }

        let mut frame = self
            .placeholder
            .as_mut()
            .ok_or_else(|| hresult(MF_E_NOTACCEPTING))?
            .next_frame()
            .map_err(|_| hresult(E_UNEXPECTED))?;
        if self.placeholder_discontinuity_pending {
            frame.descriptor.flags.discontinuity = true;
            self.placeholder_discontinuity_pending = false;
        }
        self.output_timeline
            .as_mut()
            .ok_or_else(|| hresult(MF_E_NOTACCEPTING))?
            .observe_placeholder(&frame)?;
        Ok(frame)
    }

    fn fall_back_to_placeholder(&mut self) -> Result<()> {
        self.placeholder = Some(
            self.output_timeline
                .as_ref()
                .ok_or_else(|| hresult(MF_E_NOTACCEPTING))?
                .resume_placeholder_source()?,
        );
        self.shared = None;
        self.live_started = false;
        self.empty_live_polls = 0;
        self.placeholder_frames_until_probe = LATE_SHARED_PROBE_INTERVAL_PLACEHOLDER_FRAMES;
        self.placeholder_discontinuity_pending = true;
        Ok(())
    }

    fn should_probe(&mut self) -> bool {
        if self.placeholder_frames_until_probe == 0 {
            self.placeholder_frames_until_probe = LATE_SHARED_PROBE_INTERVAL_PLACEHOLDER_FRAMES;
            true
        } else {
            self.placeholder_frames_until_probe -= 1;
            false
        }
    }
}

enum FrameProvider {
    Fixture {
        generator: Option<DeterministicNv12Source>,
    },
    External(Arc<Mutex<ExternalNv12FrameIngress>>),
    Shared(CameraSharedIngressConsumer),
    LateShared(LateSharedFrameProvider),
}

impl FrameProvider {
    const fn uses_async_sample_pump(&self) -> bool {
        matches!(self, Self::Shared(_) | Self::LateShared(_))
    }
}

struct StreamRuntime {
    core: MfMediaSourceCore,
    frame_provider: FrameProvider,
    timing: Option<MfSampleTimingMapper>,
    qpc_anchor_100ns: Option<i64>,
}

impl StreamRuntime {
    fn new(frame_provider: FrameProvider) -> Self {
        Self {
            core: MfMediaSourceCore::default(),
            frame_provider,
            timing: None,
            qpc_anchor_100ns: None,
        }
    }

    fn reset_for_start(&mut self, start_time_100ns: i64) -> Result<()> {
        let generation = self.core.stream_generation();
        let stream_id = StreamId::from_str(CAMERA_STREAM_ID).map_err(|_| hresult(E_UNEXPECTED))?;
        match &mut self.frame_provider {
            FrameProvider::Fixture { generator } => {
                *generator = Some(
                    DeterministicNv12Source::new(stream_id, generation, 0)
                        .map_err(|_| hresult(E_UNEXPECTED))?,
                );
            }
            FrameProvider::LateShared(provider) => {
                provider.reset_for_start(stream_id, generation)?;
            }
            FrameProvider::External(_) | FrameProvider::Shared(_) => {}
        }
        self.timing = None;
        self.qpc_anchor_100ns = Some(start_time_100ns);
        Ok(())
    }

    fn clear_frames(&mut self) {
        match &mut self.frame_provider {
            FrameProvider::Fixture { generator } => *generator = None,
            FrameProvider::LateShared(provider) => provider.clear_frames(),
            FrameProvider::External(_) | FrameProvider::Shared(_) => {}
        }
        self.timing = None;
        self.qpc_anchor_100ns = None;
    }

    fn next_frame(&mut self) -> Result<GeneratedVideoFrame> {
        match &mut self.frame_provider {
            FrameProvider::Fixture { generator } => generator
                .as_mut()
                .ok_or_else(|| hresult(MF_E_NOTACCEPTING))?
                .next_frame()
                .map_err(|_| hresult(E_UNEXPECTED)),
            FrameProvider::External(ingress) => match ingress.try_lock() {
                Ok(mut ingress) => ingress.pop().ok_or_else(|| hresult(MF_E_NOTACCEPTING)),
                Err(TryLockError::WouldBlock) => Err(hresult(MF_E_NOTACCEPTING)),
                Err(TryLockError::Poisoned(_)) => Err(hresult(E_UNEXPECTED)),
            },
            FrameProvider::Shared(consumer) => consumer
                .try_read_latest()
                .map_err(|_| hresult(E_UNEXPECTED))?
                .ok_or_else(|| hresult(MF_E_NOTACCEPTING)),
            FrameProvider::LateShared(provider) => provider.next_frame(),
        }
    }
}

struct MediaFoundationSerialQueue {
    id: u32,
}

impl MediaFoundationSerialQueue {
    fn new() -> Result<Self> {
        Ok(Self {
            id: unsafe { MFAllocateSerialWorkQueue(MFASYNC_CALLBACK_QUEUE_STANDARD)? },
        })
    }
}

impl Drop for MediaFoundationSerialQueue {
    fn drop(&mut self) {
        unsafe {
            let _ = MFUnlockWorkQueue(self.id);
        }
    }
}

struct PendingSharedSamples {
    tokens: VecDeque<Option<IUnknown>>,
    pump_scheduled: bool,
}

impl PendingSharedSamples {
    fn new() -> Self {
        Self {
            tokens: VecDeque::with_capacity(MAX_PENDING_SAMPLE_REQUESTS),
            pump_scheduled: false,
        }
    }
}

struct SharedSamplePumpState {
    shared: Arc<StreamShared>,
    event_queue: IMFMediaEventQueue,
    queue: Arc<MediaFoundationSerialQueue>,
    pending: Mutex<PendingSharedSamples>,
    reservations: AtomicUsize,
}

impl SharedSamplePumpState {
    fn reserve(&self) -> bool {
        self.reservations
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < MAX_PENDING_SAMPLE_REQUESTS).then_some(current + 1)
            })
            .is_ok()
    }

    fn release_reservation(&self) {
        let previous = self.reservations.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0);
    }

    fn accepting(&self) -> bool {
        !self.shared.shutdown.load(Ordering::Acquire)
            && self.shared.state.load(Ordering::Acquire) == MF_STREAM_STATE_RUNNING.0
    }

    fn enqueue(&self, token: Option<IUnknown>) -> Result<bool> {
        if !self.accepting() {
            self.release_reservation();
            return Ok(false);
        }
        let mut pending = self.pending.lock().map_err(|_| hresult(E_UNEXPECTED))?;
        if pending.tokens.len() == MAX_PENDING_SAMPLE_REQUESTS {
            self.release_reservation();
            return Err(hresult(E_UNEXPECTED));
        }
        pending.tokens.push_back(token);
        if pending.pump_scheduled {
            Ok(false)
        } else {
            pending.pump_scheduled = true;
            Ok(true)
        }
    }

    fn pending_token(&self) -> Result<Option<Option<IUnknown>>> {
        let pending = self.pending.lock().map_err(|_| hresult(E_UNEXPECTED))?;
        Ok(pending.tokens.front().cloned())
    }

    fn complete_pending(&self) -> Result<bool> {
        let mut pending = self.pending.lock().map_err(|_| hresult(E_UNEXPECTED))?;
        pending
            .tokens
            .pop_front()
            .ok_or_else(|| hresult(E_UNEXPECTED))?;
        self.release_reservation();
        if pending.tokens.is_empty() {
            pending.pump_scheduled = false;
            Ok(false)
        } else {
            Ok(true)
        }
    }

    fn cancel_pending(&self) -> Result<()> {
        let mut pending = self.pending.lock().map_err(|_| hresult(E_UNEXPECTED))?;
        let cancelled = pending.tokens.len();
        pending.tokens.clear();
        pending.pump_scheduled = false;
        if cancelled > 0 {
            let previous = self.reservations.fetch_sub(cancelled, Ordering::AcqRel);
            debug_assert!(previous >= cancelled);
        }
        Ok(())
    }

    fn queue_error(&self, error: &Error) -> Result<()> {
        unsafe {
            self.event_queue.QueueEventParamVar(
                MEError.0 as u32,
                &GUID::zeroed(),
                error.code(),
                ptr::null(),
            )
        }
    }

    fn try_emit_one(&self) -> Result<SharedPumpOutcome> {
        if !self.accepting() {
            self.cancel_pending()?;
            return Ok(SharedPumpOutcome::Stopped);
        }
        let token = match self.pending_token()? {
            Some(token) => token,
            None => return Ok(SharedPumpOutcome::Idle),
        };
        let allocator = match self.shared.try_clone_allocator() {
            Ok(allocator) => allocator,
            Err(error)
                if error.code() == MF_E_NOT_INITIALIZED || error.code() == MF_E_NOTACCEPTING =>
            {
                return Ok(SharedPumpOutcome::Retry);
            }
            Err(error) => return Err(error),
        };
        let mut runtime = match self.shared.runtime.try_lock() {
            Ok(runtime) => runtime,
            Err(TryLockError::WouldBlock) => return Ok(SharedPumpOutcome::Retry),
            Err(TryLockError::Poisoned(_)) => return Err(hresult(E_UNEXPECTED)),
        };
        let frame = match runtime.next_frame() {
            Ok(frame) => frame,
            Err(error) if error.code() == MF_E_NOTACCEPTING => {
                return Ok(SharedPumpOutcome::Retry);
            }
            Err(error) => return Err(error),
        };
        let ticket = runtime
            .core
            .request_sample()
            .map_err(|_| hresult(E_UNEXPECTED))?;
        let sample = match create_sample(&mut runtime, &allocator, token, frame) {
            Ok((sample, sequence)) => {
                runtime
                    .core
                    .complete_sample(ticket, sequence)
                    .map_err(|_| hresult(E_UNEXPECTED))?;
                sample
            }
            Err(error) => {
                runtime
                    .core
                    .cancel_sample(ticket)
                    .map_err(|_| hresult(E_UNEXPECTED))?;
                return Err(error);
            }
        };
        drop(runtime);

        let more = self.complete_pending()?;
        let sample_unknown: IUnknown = sample.cast()?;
        unsafe {
            self.event_queue.QueueEventParamUnk(
                MEMediaSample.0 as u32,
                &GUID::zeroed(),
                S_OK,
                &sample_unknown,
            )?;
        }
        Ok(SharedPumpOutcome::Emitted { more })
    }
}

enum SharedPumpOutcome {
    Emitted { more: bool },
    Retry,
    Idle,
    Stopped,
}

#[windows::core::implement(IMFAsyncCallback)]
struct SharedSampleRetryCallback {
    state: Arc<SharedSamplePumpState>,
}

impl IMFAsyncCallback_Impl for SharedSampleRetryCallback_Impl {
    fn GetParameters(&self, _flags: *mut u32, _queue: *mut u32) -> Result<()> {
        Err(hresult(E_NOTIMPL))
    }

    fn Invoke(&self, result: Ref<'_, IMFAsyncResult>) -> Result<()> {
        let pump: IMFAsyncCallback = unsafe { result.ok()?.GetState()? }.cast()?;
        if !self.state.accepting() {
            return self.state.cancel_pending();
        }
        if let Err(error) = unsafe { MFPutWorkItem(self.state.queue.id, &pump, &pump) } {
            self.state.cancel_pending()?;
            self.state.queue_error(&error)?;
        }
        Ok(())
    }
}

#[windows::core::implement(IMFAsyncCallback)]
struct SharedSamplePumpCallback {
    state: Arc<SharedSamplePumpState>,
    retry: IMFAsyncCallback,
}

impl SharedSamplePumpCallback_Impl {
    fn schedule_retry(&self, pump: &IMFAsyncCallback) -> Result<()> {
        unsafe { MFScheduleWorkItem(&self.retry, pump, SHARED_SAMPLE_POLL_MILLISECONDS, None) }
    }

    fn fail_pending(&self, error: Error) -> Result<()> {
        self.state.cancel_pending()?;
        self.state.queue_error(&error)
    }
}

impl IMFAsyncCallback_Impl for SharedSamplePumpCallback_Impl {
    fn GetParameters(&self, _flags: *mut u32, _queue: *mut u32) -> Result<()> {
        Err(hresult(E_NOTIMPL))
    }

    fn Invoke(&self, result: Ref<'_, IMFAsyncResult>) -> Result<()> {
        let pump: IMFAsyncCallback = unsafe { result.ok()?.GetState()? }.cast()?;
        match self.state.try_emit_one() {
            Ok(SharedPumpOutcome::Emitted { more: true }) => {
                if let Err(error) = unsafe { MFPutWorkItem(self.state.queue.id, &pump, &pump) } {
                    self.fail_pending(error)?;
                }
            }
            Ok(SharedPumpOutcome::Retry) => {
                if let Err(error) = self.schedule_retry(&pump) {
                    self.fail_pending(error)?;
                }
            }
            Ok(
                SharedPumpOutcome::Emitted { more: false }
                | SharedPumpOutcome::Idle
                | SharedPumpOutcome::Stopped,
            ) => {}
            Err(error) => self.fail_pending(error)?,
        }
        Ok(())
    }
}

#[windows::core::implement(IMFAsyncCallback)]
struct SharedSampleEnqueueCallback {
    state: Arc<SharedSamplePumpState>,
    pump: IMFAsyncCallback,
}

impl IMFAsyncCallback_Impl for SharedSampleEnqueueCallback_Impl {
    fn GetParameters(&self, _flags: *mut u32, _queue: *mut u32) -> Result<()> {
        Err(hresult(E_NOTIMPL))
    }

    fn Invoke(&self, result: Ref<'_, IMFAsyncResult>) -> Result<()> {
        let request: IMFAttributes = unsafe { result.ok()?.GetState()? }.cast()?;
        let token = unsafe { request.GetUnknown::<IUnknown>(&MFSampleExtension_Token) }.ok();
        if self.state.enqueue(token)?
            && let Err(error) =
                unsafe { MFPutWorkItem(self.state.queue.id, &self.pump, &self.pump) }
        {
            self.state.cancel_pending()?;
            self.state.queue_error(&error)?;
        }
        Ok(())
    }
}

struct SharedSamplePumpController {
    state: Arc<SharedSamplePumpState>,
    enqueue_callback: IMFAsyncCallback,
}

impl SharedSamplePumpController {
    fn new(shared: Arc<StreamShared>, event_queue: IMFMediaEventQueue) -> Result<Self> {
        let queue = Arc::new(MediaFoundationSerialQueue::new()?);
        // Media Foundation owns callback dispatch across its work-queue threads.
        // The event queue and provided sample allocator are MF free-threaded COM
        // objects; every Rust-owned mutable field below is atomic or mutex-bound.
        // `windows` conservatively leaves arbitrary interface wrappers !Send/!Sync.
        #[allow(clippy::arc_with_non_send_sync)]
        let state = Arc::new(SharedSamplePumpState {
            shared,
            event_queue,
            queue,
            pending: Mutex::new(PendingSharedSamples::new()),
            reservations: AtomicUsize::new(0),
        });
        let retry: IMFAsyncCallback = SharedSampleRetryCallback {
            state: Arc::clone(&state),
        }
        .into();
        let pump: IMFAsyncCallback = SharedSamplePumpCallback {
            state: Arc::clone(&state),
            retry,
        }
        .into();
        let enqueue_callback: IMFAsyncCallback = SharedSampleEnqueueCallback {
            state: Arc::clone(&state),
            pump,
        }
        .into();
        Ok(Self {
            state,
            enqueue_callback,
        })
    }

    fn request(&self, token: Option<IUnknown>) -> Result<()> {
        if !self.state.reserve() {
            // The Media Foundation contract permits a live source to accept a
            // request and release its token when the fixed request bound is full.
            return Ok(());
        }
        let request = match create_attributes(1) {
            Ok(request) => request,
            Err(error) => {
                self.state.release_reservation();
                return Err(error);
            }
        };
        if let Some(token) = token
            && let Err(error) = unsafe { request.SetUnknown(&MFSampleExtension_Token, &token) }
        {
            self.state.release_reservation();
            return Err(error);
        }
        if let Err(error) =
            unsafe { MFPutWorkItem(self.state.queue.id, &self.enqueue_callback, &request) }
        {
            self.state.release_reservation();
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{process::Command, ptr, str::FromStr, thread, time::Duration};

    use capyio_windows_camera::DeterministicNv12Source;
    use windows::{
        Win32::Media::MediaFoundation::{
            IMF2DBuffer, IMFSample, IMFSampleAllocatorControl, IMFVideoSampleAllocator,
            MEMediaSample, MF_EVENT_FLAG_NO_WAIT, MFCreateVideoSampleAllocatorEx,
        },
        core::{IUnknown, Interface},
    };

    use super::*;
    use crate::{CAMERA_SHARED_INGRESS_VERSION, CameraSharedIngressProducer};

    const MF_CHILD_FLAG: &str = "CAPYIO_CAMERA_SHARED_INGRESS_MF_CHILD";
    const MF_CHILD_MAPPING: &str = "CAPYIO_CAMERA_SHARED_INGRESS_MF_TEST_MAPPING";

    #[test]
    fn registered_provider_falls_back_only_when_the_fixed_mapping_is_absent() {
        assert!(shared_mapping_is_absent(
            &CameraSharedIngressError::Windows {
                operation: "OpenFileMappingW",
                code: ERROR_FILE_NOT_FOUND.0,
            }
        ));
        assert!(!shared_mapping_is_absent(
            &CameraSharedIngressError::Windows {
                operation: "OpenFileMappingW",
                code: 5,
            }
        ));
        assert!(!shared_mapping_is_absent(
            &CameraSharedIngressError::InvalidLayout
        ));
    }

    #[test]
    fn late_registered_provider_rebases_placeholder_then_live_frames() {
        let mapping_name = format!(
            "Local\\CapyIO.CameraIngress.v{CAMERA_SHARED_INGRESS_VERSION}.test.{}.late",
            std::process::id()
        );
        let frame_provider =
            FrameProvider::LateShared(LateSharedFrameProvider::local_test(mapping_name.clone()));
        assert!(frame_provider.uses_async_sample_pump());
        let mut runtime = StreamRuntime::new(frame_provider);
        runtime
            .core
            .start(
                capyio_windows_camera::MfPresentationSelection::canonical(),
                1_000_000,
            )
            .expect("start virtual output timeline");
        runtime
            .reset_for_start(1_000_000)
            .expect("initialize placeholder");

        let first_placeholder = runtime.next_frame().expect("first placeholder");
        let output_stream_id = first_placeholder.descriptor.stream_id;
        let output_epoch = first_placeholder.descriptor.stream_epoch;
        assert_eq!(first_placeholder.descriptor.sequence, 0);

        let live_stream_id =
            StreamId::from_str("00000000-0000-4000-8000-00000000c026").expect("fixed id");
        let mut producer =
            CameraSharedIngressProducer::create_local_test(&mapping_name, live_stream_id, 73)
                .expect("late producer");
        let mut live_source =
            DeterministicNv12Source::new(live_stream_id, 73, 21_000_000_000).expect("live source");
        let mut first_live_input = live_source.next_frame().expect("first live input");
        first_live_input.payload[0] = 197;
        producer
            .publish(first_live_input)
            .expect("publish first live input");

        let mut next_output_timestamp = first_placeholder.descriptor.source_timestamp_nanos
            + first_placeholder.descriptor.duration_nanos;
        for expected_sequence in 1..=u64::from(LATE_SHARED_PROBE_INTERVAL_PLACEHOLDER_FRAMES) {
            let placeholder = runtime.next_frame().expect("bounded placeholder interval");
            assert_eq!(placeholder.descriptor.sequence, expected_sequence);
            assert_eq!(
                placeholder.descriptor.source_timestamp_nanos,
                next_output_timestamp
            );
            next_output_timestamp += placeholder.descriptor.duration_nanos;
        }

        let first_live = runtime.next_frame().expect("late live frame");
        assert_eq!(first_live.payload[0], 197);
        assert_eq!(first_live.descriptor.stream_id, output_stream_id);
        assert_eq!(first_live.descriptor.stream_epoch, output_epoch);
        assert_eq!(
            first_live.descriptor.sequence,
            u64::from(LATE_SHARED_PROBE_INTERVAL_PLACEHOLDER_FRAMES) + 1
        );
        assert_eq!(
            first_live.descriptor.source_timestamp_nanos,
            next_output_timestamp
        );
        assert!(first_live.descriptor.flags.discontinuity);

        assert_eq!(
            runtime
                .next_frame()
                .expect_err("live mode must not interleave placeholder frames")
                .code(),
            MF_E_NOTACCEPTING
        );
        let mut second_live_input = live_source.next_frame().expect("second live input");
        second_live_input.payload[0] = 198;
        producer
            .publish(second_live_input)
            .expect("publish second live input");
        let second_live = runtime.next_frame().expect("second live frame");
        assert_eq!(second_live.payload[0], 198);
        assert_eq!(
            second_live.descriptor.sequence,
            first_live.descriptor.sequence + 1
        );
        assert_eq!(
            second_live.descriptor.source_timestamp_nanos,
            first_live.descriptor.source_timestamp_nanos + first_live.descriptor.duration_nanos
        );
        assert!(!second_live.descriptor.flags.discontinuity);
    }

    #[test]
    fn late_registered_provider_falls_back_and_reattaches_after_producer_restart() {
        let mapping_name = format!(
            "Local\\CapyIO.CameraIngress.v{CAMERA_SHARED_INGRESS_VERSION}.test.{}.restart",
            std::process::id()
        );
        let first_stream_id =
            StreamId::from_str("00000000-0000-4000-8000-00000000c127").expect("fixed id");
        let mut first_producer =
            CameraSharedIngressProducer::create_local_test(&mapping_name, first_stream_id, 81)
                .expect("first producer");
        let mut first_source = DeterministicNv12Source::new(first_stream_id, 81, 31_000_000_000)
            .expect("first source");
        let mut first_input = first_source.next_frame().expect("first input");
        first_input.payload[0] = 121;
        first_producer
            .publish(first_input)
            .expect("publish first input");

        let mut runtime = StreamRuntime::new(FrameProvider::LateShared(
            LateSharedFrameProvider::local_test(mapping_name.clone()),
        ));
        runtime
            .core
            .start(
                capyio_windows_camera::MfPresentationSelection::canonical(),
                1_000_000,
            )
            .expect("start virtual output timeline");
        runtime
            .reset_for_start(1_000_000)
            .expect("initialize provider");

        let first_live = runtime.next_frame().expect("initial live frame");
        assert_eq!(first_live.payload[0], 121);
        assert_eq!(first_live.descriptor.sequence, 0);
        assert!(first_live.descriptor.flags.discontinuity);
        drop(first_producer);

        for _ in 1..LATE_SHARED_MAX_EMPTY_LIVE_POLLS {
            assert_eq!(
                runtime
                    .next_frame()
                    .expect_err("bounded stall interval must retain the pending request")
                    .code(),
                MF_E_NOTACCEPTING
            );
        }
        let fallback = runtime.next_frame().expect("fallback placeholder");
        assert_eq!(fallback.descriptor.sequence, 1);
        assert_eq!(
            fallback.descriptor.source_timestamp_nanos,
            first_live.descriptor.source_timestamp_nanos + first_live.descriptor.duration_nanos
        );
        assert!(fallback.descriptor.flags.discontinuity);

        let second_stream_id =
            StreamId::from_str("00000000-0000-4000-8000-00000000c128").expect("fixed id");
        let mut second_producer =
            CameraSharedIngressProducer::create_local_test(&mapping_name, second_stream_id, 82)
                .expect("replacement producer");
        let mut second_source = DeterministicNv12Source::new(second_stream_id, 82, 41_000_000_000)
            .expect("replacement source");
        let mut second_input = second_source.next_frame().expect("replacement input");
        second_input.payload[0] = 122;
        second_producer
            .publish(second_input)
            .expect("publish replacement input");

        let mut next_sequence = fallback.descriptor.sequence + 1;
        let mut next_timestamp =
            fallback.descriptor.source_timestamp_nanos + fallback.descriptor.duration_nanos;
        for _ in 0..LATE_SHARED_PROBE_INTERVAL_PLACEHOLDER_FRAMES {
            let placeholder = runtime.next_frame().expect("bounded reprobe interval");
            assert_eq!(placeholder.descriptor.sequence, next_sequence);
            assert_eq!(
                placeholder.descriptor.source_timestamp_nanos,
                next_timestamp
            );
            assert!(!placeholder.descriptor.flags.discontinuity);
            next_sequence += 1;
            next_timestamp += placeholder.descriptor.duration_nanos;
        }

        let resumed = runtime.next_frame().expect("replacement live frame");
        assert_eq!(resumed.payload[0], 122);
        assert_eq!(resumed.descriptor.sequence, next_sequence);
        assert_eq!(resumed.descriptor.source_timestamp_nanos, next_timestamp);
        assert!(resumed.descriptor.flags.discontinuity);
    }

    #[test]
    fn late_registered_provider_does_not_replay_a_stale_publication() {
        let mapping_name = format!(
            "Local\\CapyIO.CameraIngress.v{CAMERA_SHARED_INGRESS_VERSION}.test.{}.stale",
            std::process::id()
        );
        let live_stream_id =
            StreamId::from_str("00000000-0000-4000-8000-00000000c129").expect("fixed id");
        let mut producer =
            CameraSharedIngressProducer::create_local_test(&mapping_name, live_stream_id, 83)
                .expect("producer");
        let mut live_source =
            DeterministicNv12Source::new(live_stream_id, 83, 51_000_000_000).expect("live source");
        let mut first_input = live_source.next_frame().expect("first input");
        first_input.payload[0] = 123;
        producer.publish(first_input).expect("publish first input");

        let mut runtime = StreamRuntime::new(FrameProvider::LateShared(
            LateSharedFrameProvider::local_test(mapping_name),
        ));
        runtime
            .core
            .start(
                capyio_windows_camera::MfPresentationSelection::canonical(),
                1_000_000,
            )
            .expect("start virtual output timeline");
        runtime
            .reset_for_start(1_000_000)
            .expect("initialize provider");
        assert_eq!(runtime.next_frame().expect("first live").payload[0], 123);

        for _ in 1..LATE_SHARED_MAX_EMPTY_LIVE_POLLS {
            assert_eq!(
                runtime
                    .next_frame()
                    .expect_err("bounded stall interval")
                    .code(),
                MF_E_NOTACCEPTING
            );
        }
        runtime.next_frame().expect("fallback placeholder");
        for _ in 0..LATE_SHARED_PROBE_INTERVAL_PLACEHOLDER_FRAMES {
            runtime
                .next_frame()
                .expect("placeholder before stale probe");
        }
        let stale_probe_output = runtime
            .next_frame()
            .expect("stale publication must leave placeholder active");
        assert_ne!(stale_probe_output.payload[0], 123);
        assert!(!stale_probe_output.descriptor.flags.discontinuity);

        let mut second_input = live_source.next_frame().expect("second input");
        second_input.payload[0] = 124;
        producer
            .publish(second_input)
            .expect("publish second input");
        for _ in 0..LATE_SHARED_PROBE_INTERVAL_PLACEHOLDER_FRAMES {
            runtime
                .next_frame()
                .expect("placeholder before fresh probe");
        }
        let resumed = runtime
            .next_frame()
            .expect("fresh publication resumes live");
        assert_eq!(resumed.payload[0], 124);
        assert!(resumed.descriptor.flags.discontinuity);
    }

    #[test]
    fn late_registered_provider_fails_closed_for_an_invalid_mapping_target() {
        let mut runtime = StreamRuntime::new(FrameProvider::LateShared(
            LateSharedFrameProvider::local_test("Local\\invalid".to_owned()),
        ));
        runtime
            .core
            .start(
                capyio_windows_camera::MfPresentationSelection::canonical(),
                1_000_000,
            )
            .expect("start virtual output timeline");
        runtime
            .reset_for_start(1_000_000)
            .expect("initialize placeholder");
        assert_eq!(
            runtime
                .next_frame()
                .expect_err("invalid mapping target must not fall back")
                .code(),
            E_UNEXPECTED
        );
    }

    #[test]
    fn shared_provider_supplies_latest_frame_without_blocking_on_empty() {
        let stream_id =
            StreamId::from_str("00000000-0000-4000-8000-00000000c016").expect("fixed id");
        let mapping_name = format!(
            "Local\\CapyIO.CameraIngress.v{CAMERA_SHARED_INGRESS_VERSION}.test.{}.provider",
            std::process::id()
        );
        let mut producer =
            CameraSharedIngressProducer::create_local_test(&mapping_name, stream_id, 31)
                .expect("test producer");
        let consumer = CameraSharedIngressConsumer::open_local_test(&mapping_name, stream_id, 31)
            .expect("test consumer");
        let mut source =
            DeterministicNv12Source::new(stream_id, 31, 11_000_000_000).expect("test source");
        let mut expected = source.next_frame().expect("test frame");
        expected.payload[0] = 91;
        producer.publish(expected.clone()).expect("publish frame");

        let mut runtime = StreamRuntime::new(FrameProvider::Shared(consumer));
        assert_eq!(runtime.next_frame().expect("shared frame"), expected);
        assert_eq!(
            runtime
                .next_frame()
                .expect_err("empty mapping must remain non-blocking")
                .code(),
            MF_E_NOTACCEPTING
        );
    }

    #[test]
    fn shared_provider_completes_a_pending_request_after_later_publication() {
        let _media_foundation = MediaFoundationRuntime::startup().expect("start Media Foundation");
        let stream_id =
            StreamId::from_str("00000000-0000-4000-8000-00000000c018").expect("fixed id");
        let mapping_name = format!(
            "Local\\CapyIO.CameraIngress.v{CAMERA_SHARED_INGRESS_VERSION}.test.{}.async",
            std::process::id()
        );
        let mut producer =
            CameraSharedIngressProducer::create_local_test(&mapping_name, stream_id, 41)
                .expect("test producer");
        let consumer = CameraSharedIngressConsumer::open_local_test(&mapping_name, stream_id, 41)
            .expect("test consumer");
        let mut generator =
            DeterministicNv12Source::new(stream_id, 41, 13_000_000_000).expect("test source");
        let mut first = generator.next_frame().expect("first frame");
        first.payload[0] = 43;
        producer.publish(first).expect("publish first frame");

        let handle = create_in_process_media_source_with_shared_ingress(consumer)
            .expect("create shared media source");
        let source = handle.source();
        let stream = handle.stream();
        let allocator_control: IMFSampleAllocatorControl =
            source.cast().expect("allocator control");
        let allocator = create_video_sample_allocator();
        unsafe {
            allocator_control
                .SetDefaultAllocator(0, &allocator)
                .expect("set allocator");
        }
        let presentation = unsafe {
            source
                .CreatePresentationDescriptor()
                .expect("presentation descriptor")
        };
        let start_position = PROPVARIANT::from(0_i64);
        unsafe {
            source
                .Start(&presentation, ptr::null(), &start_position)
                .expect("start source");
            source
                .GetEvent(MF_EVENT_FLAG_NO_WAIT)
                .expect("new stream event");
            stream
                .GetEvent(MF_EVENT_FLAG_NO_WAIT)
                .expect("stream started event");
            source
                .GetEvent(MF_EVENT_FLAG_NO_WAIT)
                .expect("source started event");
            stream
                .RequestSample(None::<&IUnknown>)
                .expect("request first sample");
        }
        assert_eq!(sample_luma_from_event(wait_for_stream_event(stream)), 43);

        unsafe {
            stream
                .RequestSample(None::<&IUnknown>)
                .expect("retain request while mapping has no newer frame");
        }
        thread::sleep(Duration::from_millis(20));
        let mut second = generator.next_frame().expect("second frame");
        second.payload[0] = 87;
        producer.publish(second).expect("publish second frame");
        assert_eq!(sample_luma_from_event(wait_for_stream_event(stream)), 87);

        unsafe {
            source.Stop().expect("stop source");
            stream
                .GetEvent(MF_EVENT_FLAG_NO_WAIT)
                .expect("stream stopped event");
            source
                .GetEvent(MF_EVENT_FLAG_NO_WAIT)
                .expect("source stopped event");
            source.Shutdown().expect("shutdown source");
        }
    }

    #[test]
    fn separate_process_projects_shared_payload_as_media_foundation_sample() {
        let stream_id =
            StreamId::from_str("00000000-0000-4000-8000-00000000c017").expect("fixed id");
        let mapping_name = format!(
            "Local\\CapyIO.CameraIngress.v{CAMERA_SHARED_INGRESS_VERSION}.test.{}.mf-process",
            std::process::id()
        );
        let mut producer =
            CameraSharedIngressProducer::create_local_test(&mapping_name, stream_id, 37)
                .expect("test producer");
        let mut source =
            DeterministicNv12Source::new(stream_id, 37, 12_000_000_000).expect("test source");
        let mut frame = source.next_frame().expect("test frame");
        frame.payload[0] = 137;
        producer.publish(frame).expect("publish shared frame");

        let status = Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "windows_impl::tests::cross_process_mf_consumer_child",
                "--nocapture",
            ])
            .env(MF_CHILD_FLAG, "1")
            .env(MF_CHILD_MAPPING, &mapping_name)
            .status()
            .expect("spawn Media Foundation consumer");
        assert!(status.success());
    }

    #[test]
    fn cross_process_mf_consumer_child() {
        if std::env::var_os(MF_CHILD_FLAG).as_deref() != Some(std::ffi::OsStr::new("1")) {
            return;
        }
        let stream_id =
            StreamId::from_str("00000000-0000-4000-8000-00000000c017").expect("fixed id");
        let mapping_name = std::env::var(MF_CHILD_MAPPING).expect("parent mapping name");
        let consumer = CameraSharedIngressConsumer::open_local_test_current(&mapping_name)
            .expect("open parent mapping");
        assert_eq!(consumer.stream_id(), stream_id);
        assert_eq!(consumer.stream_epoch(), 37);
        project_one_shared_sample(consumer);
    }

    fn project_one_shared_sample(consumer: CameraSharedIngressConsumer) {
        let _media_foundation = MediaFoundationRuntime::startup().expect("start Media Foundation");
        let handle = create_in_process_media_source_with_shared_ingress(consumer)
            .expect("create shared media source");
        let source = handle.source();
        let stream = handle.stream();
        let allocator_control: IMFSampleAllocatorControl =
            source.cast().expect("allocator control");
        let allocator = create_video_sample_allocator();
        unsafe {
            allocator_control
                .SetDefaultAllocator(0, &allocator)
                .expect("set allocator");
        }

        let presentation = unsafe {
            source
                .CreatePresentationDescriptor()
                .expect("presentation descriptor")
        };
        let start_position = PROPVARIANT::from(0_i64);
        unsafe {
            source
                .Start(&presentation, ptr::null(), &start_position)
                .expect("start source");
            source
                .GetEvent(MF_EVENT_FLAG_NO_WAIT)
                .expect("new stream event");
            stream
                .GetEvent(MF_EVENT_FLAG_NO_WAIT)
                .expect("stream started event");
            source
                .GetEvent(MF_EVENT_FLAG_NO_WAIT)
                .expect("source started event");
            stream
                .RequestSample(None::<&IUnknown>)
                .expect("request sample");
        }

        let event = wait_for_stream_event(stream);
        assert_eq!(
            unsafe { event.GetType().expect("sample event type") },
            MEMediaSample.0 as u32
        );
        let unknown = IUnknown::try_from(&unsafe { event.GetValue().expect("sample value") })
            .expect("sample unknown");
        let sample: IMFSample = unknown.cast().expect("sample interface");
        assert_eq!(read_first_luma(&sample), 137);
        assert_eq!(
            unsafe { sample.GetSampleDuration().expect("sample duration") },
            333_333
        );

        unsafe {
            stream
                .RequestSample(None::<&IUnknown>)
                .expect("empty live source must retain one bounded async request");
        }
        unsafe {
            source.Stop().expect("stop source");
            stream
                .GetEvent(MF_EVENT_FLAG_NO_WAIT)
                .expect("stream stopped event");
            source
                .GetEvent(MF_EVENT_FLAG_NO_WAIT)
                .expect("source stopped event");
            source.Shutdown().expect("shutdown source");
        }
    }

    fn wait_for_stream_event(stream: &IMFMediaStream2) -> IMFMediaEvent {
        for _ in 0..100 {
            match unsafe { stream.GetEvent(MF_EVENT_FLAG_NO_WAIT) } {
                Ok(event) => return event,
                Err(error)
                    if error.code()
                        == windows::Win32::Media::MediaFoundation::MF_E_NO_EVENTS_AVAILABLE =>
                {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("sample event: {error}"),
            }
        }
        panic!("sample event did not arrive within 500 ms")
    }

    fn sample_luma_from_event(event: IMFMediaEvent) -> u8 {
        assert_eq!(
            unsafe { event.GetType().expect("sample event type") },
            MEMediaSample.0 as u32
        );
        let unknown = IUnknown::try_from(&unsafe { event.GetValue().expect("sample value") })
            .expect("sample unknown");
        let sample: IMFSample = unknown.cast().expect("sample interface");
        read_first_luma(&sample)
    }

    fn create_video_sample_allocator() -> IMFVideoSampleAllocator {
        let mut raw = ptr::null_mut();
        unsafe {
            MFCreateVideoSampleAllocatorEx(&IMFVideoSampleAllocator::IID, &mut raw)
                .expect("create sample allocator");
            IMFVideoSampleAllocator::from_raw(raw)
        }
    }

    fn read_first_luma(sample: &IMFSample) -> u8 {
        let buffer = unsafe { sample.GetBufferByIndex(0).expect("sample buffer") };
        let buffer_2d: IMF2DBuffer = buffer.cast().expect("2D sample buffer");
        let mut scanline = ptr::null_mut();
        let mut pitch = 0_i32;
        unsafe {
            buffer_2d
                .Lock2D(&mut scanline, &mut pitch)
                .expect("lock 2D sample");
        }
        assert!(!scanline.is_null());
        assert!(pitch >= 1280);
        let first_luma = unsafe { *scanline };
        unsafe { buffer_2d.Unlock2D().expect("unlock 2D sample") };
        first_luma
    }
}

struct StreamShared {
    runtime: Arc<Mutex<StreamRuntime>>,
    sample_allocator: Mutex<Option<IMFVideoSampleAllocator>>,
    allocator_initialized: AtomicBool,
    state: AtomicI32,
    shutdown: AtomicBool,
}

impl StreamShared {
    fn check_active(&self) -> Result<()> {
        if self.shutdown.load(Ordering::Acquire) {
            Err(hresult(MF_E_SHUTDOWN))
        } else {
            Ok(())
        }
    }

    fn start(&self, event_queue: &IMFMediaEventQueue, start_time_100ns: i64) -> Result<()> {
        self.check_active()?;
        self.state
            .store(MF_STREAM_STATE_RUNNING.0, Ordering::Release);
        let value = PROPVARIANT::from(start_time_100ns);
        unsafe {
            event_queue.QueueEventParamVar(MEStreamStarted.0 as u32, &GUID::zeroed(), S_OK, &value)
        }
    }

    fn set_default_allocator(&self, allocator: IMFVideoSampleAllocator) -> Result<()> {
        self.check_active()?;
        if self.state.load(Ordering::Acquire) != MF_STREAM_STATE_STOPPED.0 {
            return Err(hresult(MF_E_INVALIDREQUEST));
        }
        let mut stored = self
            .sample_allocator
            .lock()
            .map_err(|_| hresult(E_UNEXPECTED))?;
        *stored = Some(allocator);
        self.allocator_initialized.store(false, Ordering::Release);
        Ok(())
    }

    fn initialize_allocator(&self, media_type: &IMFMediaType) -> Result<()> {
        self.check_active()?;
        let stored = self
            .sample_allocator
            .lock()
            .map_err(|_| hresult(E_UNEXPECTED))?;
        let allocator = stored
            .as_ref()
            .ok_or_else(|| hresult(MF_E_NOT_INITIALIZED))?;
        unsafe {
            allocator.InitializeSampleAllocator(FRAME_SERVER_SAMPLE_POOL_SIZE, media_type)?;
        }
        self.allocator_initialized.store(true, Ordering::Release);
        Ok(())
    }

    fn uninitialize_allocator(&self) -> Result<()> {
        if !self.allocator_initialized.swap(false, Ordering::AcqRel) {
            return Ok(());
        }
        let stored = self
            .sample_allocator
            .lock()
            .map_err(|_| hresult(E_UNEXPECTED))?;
        let allocator = stored.as_ref().ok_or_else(|| hresult(E_UNEXPECTED))?;
        unsafe { allocator.UninitializeSampleAllocator() }
    }

    fn try_clone_allocator(&self) -> Result<IMFVideoSampleAllocator> {
        if !self.allocator_initialized.load(Ordering::Acquire) {
            return Err(hresult(MF_E_NOT_INITIALIZED));
        }
        match self.sample_allocator.try_lock() {
            Ok(stored) => stored.clone().ok_or_else(|| hresult(E_UNEXPECTED)),
            Err(TryLockError::WouldBlock) => Err(hresult(MF_E_NOTACCEPTING)),
            Err(TryLockError::Poisoned(_)) => Err(hresult(E_UNEXPECTED)),
        }
    }

    fn stop(&self, event_queue: &IMFMediaEventQueue) -> Result<()> {
        self.check_active()?;
        self.state
            .store(MF_STREAM_STATE_STOPPED.0, Ordering::Release);
        self.uninitialize_allocator()?;
        unsafe {
            event_queue.QueueEventParamVar(
                MEStreamStopped.0 as u32,
                &GUID::zeroed(),
                S_OK,
                ptr::null(),
            )
        }
    }

    fn shutdown(&self, event_queue: &IMFMediaEventQueue) -> Result<()> {
        if !self.shutdown.swap(true, Ordering::AcqRel) {
            self.state
                .store(MF_STREAM_STATE_STOPPED.0, Ordering::Release);
            self.uninitialize_allocator()?;
            unsafe { event_queue.Shutdown()? };
        }
        Ok(())
    }
}

#[windows::core::implement(IMFMediaSourceEx, IMFGetService, IKsControl, IMFSampleAllocatorControl)]
struct CapyIoMediaSource {
    _server_lease: ComServerLease,
    event_queue: IMFMediaEventQueue,
    attributes: IMFAttributes,
    presentation_descriptor: IMFPresentationDescriptor,
    runtime: Arc<Mutex<StreamRuntime>>,
    stream_shared: Arc<StreamShared>,
    stream_event_queue: IMFMediaEventQueue,
    stream_attributes: IMFAttributes,
    stream: OnceLock<IMFMediaStream2>,
    shutdown: AtomicBool,
}

impl CapyIoMediaSource {
    fn check_active(&self) -> Result<()> {
        if self.shutdown.load(Ordering::Acquire) {
            Err(hresult(MF_E_SHUTDOWN))
        } else {
            Ok(())
        }
    }

    fn stream(&self) -> Result<&IMFMediaStream2> {
        self.stream.get().ok_or_else(|| hresult(E_UNEXPECTED))
    }

    fn lock_runtime(&self) -> Result<MutexGuard<'_, StreamRuntime>> {
        self.runtime.lock().map_err(|_| hresult(E_UNEXPECTED))
    }
}

impl IMFMediaEventGenerator_Impl for CapyIoMediaSource_Impl {
    fn GetEvent(&self, flags: MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS) -> Result<IMFMediaEvent> {
        self.check_active()?;
        unsafe { self.event_queue.GetEvent(flags.0) }
    }

    fn BeginGetEvent(
        &self,
        callback: Ref<'_, IMFAsyncCallback>,
        state: Ref<'_, IUnknown>,
    ) -> Result<()> {
        self.check_active()?;
        unsafe {
            self.event_queue
                .BeginGetEvent(callback.ok()?, state.as_ref())
        }
    }

    fn EndGetEvent(&self, result: Ref<'_, IMFAsyncResult>) -> Result<IMFMediaEvent> {
        self.check_active()?;
        unsafe { self.event_queue.EndGetEvent(result.ok()?) }
    }

    fn QueueEvent(
        &self,
        event_type: u32,
        extended_type: *const GUID,
        status: windows::core::HRESULT,
        value: *const PROPVARIANT,
    ) -> Result<()> {
        self.check_active()?;
        unsafe {
            self.event_queue
                .QueueEventParamVar(event_type, extended_type, status, value)
        }
    }
}

impl IMFMediaSource_Impl for CapyIoMediaSource_Impl {
    fn GetCharacteristics(&self) -> Result<u32> {
        self.check_active()?;
        Ok(MFMEDIASOURCE_IS_LIVE.0 as u32)
    }

    fn CreatePresentationDescriptor(&self) -> Result<IMFPresentationDescriptor> {
        self.check_active()?;
        unsafe { self.presentation_descriptor.Clone() }
    }

    fn Start(
        &self,
        presentation_descriptor: Ref<'_, IMFPresentationDescriptor>,
        time_format: *const GUID,
        start_position: *const PROPVARIANT,
    ) -> Result<()> {
        self.check_active()?;
        if start_position.is_null() {
            return Err(hresult(E_INVALIDARG));
        }
        if !time_format.is_null() && unsafe { *time_format != GUID::zeroed() } {
            return Err(hresult(MF_E_UNSUPPORTED_TIME_FORMAT));
        }
        let selected_media_type = selected_media_type(presentation_descriptor.ok()?)?;
        let start_time_100ns = unsafe { MFGetSystemTime() };
        let events = {
            let mut runtime = self.lock_runtime()?;
            let repeated_start =
                runtime.core.source_state() == capyio_windows_camera::MfMediaSourceState::Started;
            if !repeated_start {
                self.stream_shared
                    .initialize_allocator(&selected_media_type)?;
            }
            let events = runtime
                .core
                .start(
                    capyio_windows_camera::MfPresentationSelection::canonical(),
                    start_time_100ns,
                )
                .map_err(|_| hresult(MF_E_INVALID_STATE_TRANSITION))?;
            if !repeated_start {
                runtime.reset_for_start(start_time_100ns)?;
            }
            events
        };

        let stream = self.stream()?;
        let stream_unknown: IUnknown = stream.cast()?;
        let first_event = events[0];
        let event_type = match first_event {
            capyio_windows_camera::MfMediaSourceEvent::NewStream { .. } => MENewStream,
            capyio_windows_camera::MfMediaSourceEvent::UpdatedStream { .. } => MEUpdatedStream,
            _ => return Err(hresult(E_UNEXPECTED)),
        };
        unsafe {
            self.event_queue.QueueEventParamUnk(
                event_type.0 as u32,
                &GUID::zeroed(),
                S_OK,
                &stream_unknown,
            )?;
        }
        self.stream_shared
            .start(&self.stream_event_queue, start_time_100ns)?;
        let value = PROPVARIANT::from(start_time_100ns);
        unsafe {
            self.event_queue.QueueEventParamVar(
                MESourceStarted.0 as u32,
                &GUID::zeroed(),
                S_OK,
                &value,
            )
        }
    }

    fn Stop(&self) -> Result<()> {
        self.check_active()?;
        {
            let mut runtime = self.lock_runtime()?;
            runtime
                .core
                .stop()
                .map_err(|_| hresult(MF_E_INVALID_STATE_TRANSITION))?;
            runtime.clear_frames();
        }
        self.stream_shared.stop(&self.stream_event_queue)?;
        unsafe {
            self.event_queue.QueueEventParamVar(
                MESourceStopped.0 as u32,
                &GUID::zeroed(),
                S_OK,
                ptr::null(),
            )
        }
    }

    fn Pause(&self) -> Result<()> {
        Err(hresult(MF_E_INVALID_STATE_TRANSITION))
    }

    fn Shutdown(&self) -> Result<()> {
        if !self.shutdown.swap(true, Ordering::AcqRel) {
            if let Ok(mut runtime) = self.runtime.lock() {
                runtime.core.shutdown();
                runtime.clear_frames();
            }
            let stream_result = self.stream_shared.shutdown(&self.stream_event_queue);
            let source_result = unsafe { self.event_queue.Shutdown() };
            stream_result?;
            source_result?;
        }
        Ok(())
    }
}

impl IMFMediaSourceEx_Impl for CapyIoMediaSource_Impl {
    fn GetSourceAttributes(&self) -> Result<IMFAttributes> {
        self.check_active()?;
        Ok(self.attributes.clone())
    }

    fn GetStreamAttributes(&self, stream_id: u32) -> Result<IMFAttributes> {
        self.check_active()?;
        if stream_id != MF_CAMERA_STREAM_ID {
            return Err(hresult(E_INVALIDARG));
        }
        Ok(self.stream_attributes.clone())
    }

    fn SetD3DManager(&self, _manager: Ref<'_, IUnknown>) -> Result<()> {
        self.check_active()?;
        Err(hresult(E_NOTIMPL))
    }
}

impl IMFGetService_Impl for CapyIoMediaSource_Impl {
    fn GetService(
        &self,
        _service: *const GUID,
        _interface: *const GUID,
        object: *mut *mut core::ffi::c_void,
    ) -> Result<()> {
        self.check_active()?;
        if object.is_null() {
            return Err(hresult(E_POINTER));
        }
        unsafe { object.write(ptr::null_mut()) };
        Err(hresult(MF_E_UNSUPPORTED_SERVICE))
    }
}

impl IKsControl_Impl for CapyIoMediaSource_Impl {
    fn KsProperty(
        &self,
        _property: *const KSIDENTIFIER,
        _property_length: u32,
        _property_data: *mut core::ffi::c_void,
        _data_length: u32,
        bytes_returned: *mut u32,
    ) -> Result<()> {
        self.unsupported_ks_control(bytes_returned)
    }

    fn KsMethod(
        &self,
        _method: *const KSIDENTIFIER,
        _method_length: u32,
        _method_data: *mut core::ffi::c_void,
        _data_length: u32,
        bytes_returned: *mut u32,
    ) -> Result<()> {
        self.unsupported_ks_control(bytes_returned)
    }

    fn KsEvent(
        &self,
        _event: *const KSIDENTIFIER,
        _event_length: u32,
        _event_data: *mut core::ffi::c_void,
        _data_length: u32,
        bytes_returned: *mut u32,
    ) -> Result<()> {
        self.unsupported_ks_control(bytes_returned)
    }
}

impl IMFSampleAllocatorControl_Impl for CapyIoMediaSource_Impl {
    fn SetDefaultAllocator(
        &self,
        output_stream_id: u32,
        allocator: Ref<'_, IUnknown>,
    ) -> Result<()> {
        self.check_active()?;
        if output_stream_id != MF_CAMERA_STREAM_ID {
            return Err(hresult(E_INVALIDARG));
        }
        self.stream_shared
            .set_default_allocator(allocator.ok()?.cast()?)
    }

    fn GetAllocatorUsage(
        &self,
        output_stream_id: u32,
        input_stream_id: *mut u32,
        usage: *mut MFSampleAllocatorUsage,
    ) -> Result<()> {
        self.check_active()?;
        if output_stream_id != MF_CAMERA_STREAM_ID {
            return Err(hresult(E_INVALIDARG));
        }
        if input_stream_id.is_null() || usage.is_null() {
            return Err(hresult(E_POINTER));
        }
        unsafe {
            input_stream_id.write(MF_CAMERA_STREAM_ID);
            usage.write(MFSampleAllocatorUsage_UsesProvidedAllocator);
        }
        Ok(())
    }
}

impl CapyIoMediaSource_Impl {
    fn unsupported_ks_control(&self, bytes_returned: *mut u32) -> Result<()> {
        self.check_active()?;
        if !bytes_returned.is_null() {
            unsafe { bytes_returned.write(0) };
        }
        Err(hresult(windows::core::HRESULT::from_win32(
            ERROR_SET_NOT_FOUND.0,
        )))
    }
}

#[windows::core::implement(IMFMediaStream2)]
struct CapyIoMediaStream {
    _server_lease: ComServerLease,
    shared: Arc<StreamShared>,
    event_queue: IMFMediaEventQueue,
    descriptor: IMFStreamDescriptor,
    source: Weak<IMFMediaSource>,
    shared_sample_pump: Option<SharedSamplePumpController>,
}

impl CapyIoMediaStream {
    fn check_accepting_samples(&self) -> Result<()> {
        self.shared.check_active()?;
        if self.shared.state.load(Ordering::Acquire) == MF_STREAM_STATE_RUNNING.0 {
            Ok(())
        } else {
            Err(hresult(MF_E_NOTACCEPTING))
        }
    }

    fn try_lock_runtime(&self) -> Result<MutexGuard<'_, StreamRuntime>> {
        match self.shared.runtime.try_lock() {
            Ok(runtime) => Ok(runtime),
            Err(TryLockError::WouldBlock) => Err(hresult(MF_E_NOTACCEPTING)),
            Err(TryLockError::Poisoned(_)) => Err(hresult(E_UNEXPECTED)),
        }
    }
}

impl IMFMediaEventGenerator_Impl for CapyIoMediaStream_Impl {
    fn GetEvent(&self, flags: MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS) -> Result<IMFMediaEvent> {
        self.shared.check_active()?;
        unsafe { self.event_queue.GetEvent(flags.0) }
    }

    fn BeginGetEvent(
        &self,
        callback: Ref<'_, IMFAsyncCallback>,
        state: Ref<'_, IUnknown>,
    ) -> Result<()> {
        self.shared.check_active()?;
        unsafe {
            self.event_queue
                .BeginGetEvent(callback.ok()?, state.as_ref())
        }
    }

    fn EndGetEvent(&self, result: Ref<'_, IMFAsyncResult>) -> Result<IMFMediaEvent> {
        self.shared.check_active()?;
        unsafe { self.event_queue.EndGetEvent(result.ok()?) }
    }

    fn QueueEvent(
        &self,
        event_type: u32,
        extended_type: *const GUID,
        status: windows::core::HRESULT,
        value: *const PROPVARIANT,
    ) -> Result<()> {
        self.shared.check_active()?;
        unsafe {
            self.event_queue
                .QueueEventParamVar(event_type, extended_type, status, value)
        }
    }
}

impl IMFMediaStream_Impl for CapyIoMediaStream_Impl {
    fn GetMediaSource(&self) -> Result<IMFMediaSource> {
        self.shared.check_active()?;
        self.source.upgrade().ok_or_else(|| hresult(MF_E_SHUTDOWN))
    }

    fn GetStreamDescriptor(&self) -> Result<IMFStreamDescriptor> {
        self.shared.check_active()?;
        Ok(self.descriptor.clone())
    }

    fn RequestSample(&self, token: Ref<'_, IUnknown>) -> Result<()> {
        self.check_accepting_samples()?;
        if let Some(pump) = self.shared_sample_pump.as_ref() {
            return pump.request(token.cloned());
        }
        let allocator = self.shared.try_clone_allocator()?;
        let mut runtime = self.try_lock_runtime()?;
        let frame = runtime.next_frame()?;
        let ticket = runtime
            .core
            .request_sample()
            .map_err(|_| hresult(MF_E_NOTACCEPTING))?;
        let result = create_sample(&mut runtime, &allocator, token.cloned(), frame);
        let (sample, sequence) = match result {
            Ok(value) => value,
            Err(error) => {
                runtime
                    .core
                    .cancel_sample(ticket)
                    .map_err(|_| hresult(E_UNEXPECTED))?;
                drop(runtime);
                unsafe {
                    self.event_queue.QueueEventParamVar(
                        MEError.0 as u32,
                        &GUID::zeroed(),
                        error.code(),
                        ptr::null(),
                    )?;
                }
                return Ok(());
            }
        };
        runtime
            .core
            .complete_sample(ticket, sequence)
            .map_err(|_| hresult(E_UNEXPECTED))?;
        drop(runtime);

        let sample_unknown: IUnknown = sample.cast()?;
        unsafe {
            self.event_queue.QueueEventParamUnk(
                MEMediaSample.0 as u32,
                &GUID::zeroed(),
                S_OK,
                &sample_unknown,
            )
        }
    }
}

impl IMFMediaStream2_Impl for CapyIoMediaStream_Impl {
    fn SetStreamState(&self, state: MF_STREAM_STATE) -> Result<()> {
        self.shared.check_active()?;
        match state {
            MF_STREAM_STATE_STOPPED | MF_STREAM_STATE_PAUSED | MF_STREAM_STATE_RUNNING => {
                self.shared.state.store(state.0, Ordering::Release);
                Ok(())
            }
            _ => Err(hresult(E_INVALIDARG)),
        }
    }

    fn GetStreamState(&self) -> Result<MF_STREAM_STATE> {
        self.shared.check_active()?;
        Ok(MF_STREAM_STATE(self.shared.state.load(Ordering::Acquire)))
    }
}

fn create_sample(
    runtime: &mut StreamRuntime,
    allocator: &IMFVideoSampleAllocator,
    token: Option<IUnknown>,
    frame: GeneratedVideoFrame,
) -> Result<(IMFSample, u64)> {
    let selected = fixture_stream_spec();
    if runtime.timing.is_none() {
        runtime.timing = Some(
            MfSampleTimingMapper::new(
                &frame.descriptor,
                &selected,
                runtime
                    .qpc_anchor_100ns
                    .ok_or_else(|| hresult(E_UNEXPECTED))?,
            )
            .map_err(|_| hresult(E_UNEXPECTED))?,
        );
    }
    let timing = runtime
        .timing
        .as_mut()
        .ok_or_else(|| hresult(E_UNEXPECTED))?
        .map(&frame.descriptor, &selected)
        .map_err(|_| hresult(E_UNEXPECTED))?;

    let sample = unsafe { allocator.AllocateSample()? };
    let buffer = unsafe { sample.GetBufferByIndex(0)? };
    write_frame_to_buffer(&frame, &buffer)?;
    unsafe {
        sample.SetSampleTime(timing.sample_time_100ns)?;
        sample.SetSampleDuration(timing.sample_duration_100ns)?;
        if frame.descriptor.flags.discontinuity {
            sample.SetUINT32(&MFSampleExtension_Discontinuity, 1)?;
        }
        if let Some(token) = token {
            sample.SetUnknown(&MFSampleExtension_Token, &token)?;
        }
    }
    Ok((sample, frame.descriptor.sequence))
}

fn write_frame_to_buffer(
    frame: &capyio_windows_camera::GeneratedVideoFrame,
    buffer: &IMFMediaBuffer,
) -> Result<()> {
    let buffer_2d: IMF2DBuffer2 = buffer.cast()?;
    let mut scanline = ptr::null_mut();
    let mut pitch = 0_i32;
    let mut buffer_start = ptr::null_mut();
    let mut buffer_length = 0_u32;
    unsafe {
        buffer_2d.Lock2DSize(
            windows::Win32::Media::MediaFoundation::MF2DBuffer_LockFlags_Write,
            &mut scanline,
            &mut pitch,
            &mut buffer_start,
            &mut buffer_length,
        )?;
    }
    let copy_result = if scanline.is_null() || buffer_start.is_null() || pitch <= 0 {
        Err(hresult(E_UNEXPECTED))
    } else {
        let scanline_address = scanline as usize;
        let buffer_address = buffer_start as usize;
        let length = buffer_length as usize;
        if scanline_address < buffer_address || scanline_address - buffer_address > length {
            Err(hresult(E_UNEXPECTED))
        } else {
            let available = length - (scanline_address - buffer_address);
            let destination = unsafe { std::slice::from_raw_parts_mut(scanline, available) };
            copy_nv12_to_strided_buffer(frame, pitch as usize, destination)
                .map_err(|_| hresult(E_UNEXPECTED))
        }
    };
    let unlock_result = unsafe { buffer_2d.Unlock2D() };
    let layout = copy_result?;
    unlock_result?;
    unsafe {
        buffer.SetCurrentLength(
            u32::try_from(layout.required_bytes).map_err(|_| hresult(E_UNEXPECTED))?,
        )
    }
}

fn selected_media_type(descriptor: &IMFPresentationDescriptor) -> Result<IMFMediaType> {
    if unsafe { descriptor.GetStreamDescriptorCount()? } != 1 {
        return Err(hresult(E_INVALIDARG));
    }
    let mut selected = BOOL::default();
    let mut stream_descriptor = None;
    unsafe {
        descriptor.GetStreamDescriptorByIndex(0, &mut selected, &mut stream_descriptor)?;
    }
    let stream_descriptor = stream_descriptor.ok_or_else(|| hresult(E_INVALIDARG))?;
    if !selected.as_bool()
        || unsafe { stream_descriptor.GetStreamIdentifier()? } != MF_CAMERA_STREAM_ID
    {
        return Err(hresult(E_INVALIDARG));
    }
    unsafe {
        stream_descriptor
            .GetMediaTypeHandler()?
            .GetCurrentMediaType()
    }
}

fn hresult(code: windows::core::HRESULT) -> Error {
    Error::from_hresult(code)
}
