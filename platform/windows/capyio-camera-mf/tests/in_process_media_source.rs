#![cfg(windows)]

use std::{
    ptr,
    str::FromStr,
    sync::{Arc, Mutex},
};

use capyio_core::StreamId;
use capyio_windows_camera::{
    DeterministicNv12Source, ExternalNv12FrameIngress, fixture_stream_spec,
};
use capyio_windows_camera_mf::{
    MediaFoundationRuntime, create_in_process_media_source,
    create_in_process_media_source_with_external_ingress,
};
use windows::{
    Win32::{
        Foundation::ERROR_SET_NOT_FOUND,
        Media::KernelStreaming::IKsControl,
        Media::MediaFoundation::{
            IMF2DBuffer, IMFGetService, IMFMediaStream2, IMFSample, IMFSampleAllocatorControl,
            IMFSensorProfileCollection, IMFVideoSampleAllocator, MEMediaSample, MENewStream,
            MESourceStarted, MESourceStopped, MEStreamStarted, MEStreamStopped, MEUpdatedStream,
            MF_DEVICEMFT_SENSORPROFILE_COLLECTION, MF_DEVICESTREAM_FRAMESERVER_SHARED,
            MF_DEVICESTREAM_STREAM_CATEGORY, MF_DEVICESTREAM_STREAM_ID, MF_E_UNSUPPORTED_SERVICE,
            MF_EVENT_FLAG_NO_WAIT, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
            MF_MT_SUBTYPE, MF_STREAM_STATE_PAUSED, MF_STREAM_STATE_RUNNING,
            MFCreateVideoSampleAllocatorEx, MFMediaType_Video, MFSampleAllocatorUsage,
            MFSampleAllocatorUsage_UsesProvidedAllocator, MFSampleExtension_Discontinuity,
            MFSampleExtension_Token, MFVideoFormat_NV12,
        },
        System::Com::StructuredStorage::PROPVARIANT,
    },
    core::{IUnknown, Interface, Result},
};

#[test]
fn local_com_source_exposes_events_media_type_samples_and_shutdown() -> Result<()> {
    let _media_foundation = MediaFoundationRuntime::startup()?;
    let handle = create_in_process_media_source()?;
    let source = handle.source();
    let stream = handle.stream();

    let allocator_control: IMFSampleAllocatorControl = source.cast()?;
    let mut input_stream_id = u32::MAX;
    let mut allocator_usage = MFSampleAllocatorUsage::default();
    unsafe {
        allocator_control.GetAllocatorUsage(0, &mut input_stream_id, &mut allocator_usage)?;
    }
    assert_eq!(input_stream_id, 0);
    assert_eq!(
        allocator_usage,
        MFSampleAllocatorUsage_UsesProvidedAllocator
    );
    let allocator = create_video_sample_allocator()?;
    unsafe { allocator_control.SetDefaultAllocator(0, &allocator)? };

    let service: IMFGetService = source.cast()?;
    let service_error = unsafe { service.GetService::<IUnknown>(&windows::core::GUID::zeroed()) }
        .expect_err("the source exposes no optional service");
    assert_eq!(service_error.code(), MF_E_UNSUPPORTED_SERVICE);
    let controls: IKsControl = source.cast()?;
    let mut bytes_returned = u32::MAX;
    let control_error =
        unsafe { controls.KsProperty(ptr::null(), 0, ptr::null_mut(), 0, &mut bytes_returned) }
            .expect_err("the deterministic fixture exposes no camera controls");
    assert_eq!(
        control_error.code(),
        windows::core::HRESULT::from_win32(ERROR_SET_NOT_FOUND.0)
    );
    assert_eq!(bytes_returned, 0);

    let source_attributes = unsafe { source.GetSourceAttributes()? };
    let stream_attributes = unsafe { source.GetStreamAttributes(0)? };
    assert_eq!(
        unsafe { stream_attributes.GetUINT32(&MF_DEVICESTREAM_STREAM_ID)? },
        0
    );
    assert_eq!(
        unsafe { stream_attributes.GetUINT32(&MF_DEVICESTREAM_FRAMESERVER_SHARED)? },
        1
    );
    let stream_descriptor = unsafe { stream.GetStreamDescriptor()? };
    assert_eq!(
        unsafe { stream_attributes.GetGUID(&MF_DEVICESTREAM_STREAM_CATEGORY)? },
        unsafe { stream_descriptor.GetGUID(&MF_DEVICESTREAM_STREAM_CATEGORY)? }
    );
    let profiles: IMFSensorProfileCollection =
        unsafe { source_attributes.GetUnknown(&MF_DEVICEMFT_SENSORPROFILE_COLLECTION)? };
    assert_eq!(unsafe { profiles.GetProfileCount() }, 1);

    let presentation = unsafe { source.CreatePresentationDescriptor()? };
    assert_eq!(unsafe { presentation.GetStreamDescriptorCount()? }, 1);
    let descriptor = unsafe { stream.GetStreamDescriptor()? };
    let handler = unsafe { descriptor.GetMediaTypeHandler()? };
    let media_type = unsafe { handler.GetCurrentMediaType()? };
    assert_eq!(
        unsafe { media_type.GetGUID(&MF_MT_MAJOR_TYPE)? },
        MFMediaType_Video
    );
    assert_eq!(
        unsafe { media_type.GetGUID(&MF_MT_SUBTYPE)? },
        MFVideoFormat_NV12
    );
    assert_eq!(
        unsafe { media_type.GetUINT64(&MF_MT_FRAME_SIZE)? },
        pack_ratio(1280, 720)
    );
    assert_eq!(
        unsafe { media_type.GetUINT64(&MF_MT_FRAME_RATE)? },
        pack_ratio(30, 1)
    );

    let start_position = PROPVARIANT::from(0_i64);
    unsafe { source.Start(&presentation, ptr::null(), &start_position)? };

    let source_new = unsafe { source.GetEvent(MF_EVENT_FLAG_NO_WAIT)? };
    assert_eq!(unsafe { source_new.GetType()? }, MENewStream.0 as u32);
    let announced: IUnknown = IUnknown::try_from(&unsafe { source_new.GetValue()? })?;
    let announced_stream: IMFMediaStream2 = announced.cast()?;
    assert_eq!(
        unsafe {
            announced_stream
                .GetStreamDescriptor()?
                .GetStreamIdentifier()?
        },
        0
    );
    assert_eq!(
        unsafe { stream.GetEvent(MF_EVENT_FLAG_NO_WAIT)?.GetType()? },
        MEStreamStarted.0 as u32
    );
    assert_eq!(
        unsafe { source.GetEvent(MF_EVENT_FLAG_NO_WAIT)?.GetType()? },
        MESourceStarted.0 as u32
    );

    let token: IUnknown = source.cast()?;
    unsafe { stream.RequestSample(&token)? };
    let first_sample = next_sample(stream)?;
    let retained_token: IUnknown = unsafe { first_sample.GetUnknown(&MFSampleExtension_Token)? };
    assert_eq!(retained_token.as_raw(), token.as_raw());
    assert_eq!(unsafe { first_sample.GetSampleDuration()? }, 333_333);
    assert_eq!(unsafe { first_sample.GetBufferCount()? }, 1);
    assert!(
        unsafe { first_sample.GetTotalLength()? }
            >= u32::try_from(fixture_stream_spec().packed_frame_bytes().unwrap()).unwrap()
    );
    inspect_nv12_buffer(&first_sample)?;

    unsafe { stream.RequestSample(None::<&IUnknown>)? };
    let second_sample = next_sample(stream)?;
    assert_eq!(
        unsafe { second_sample.GetSampleTime()? - first_sample.GetSampleTime()? },
        333_333
    );

    unsafe { source.Start(&presentation, ptr::null(), &start_position)? };
    assert_eq!(
        unsafe { source.GetEvent(MF_EVENT_FLAG_NO_WAIT)?.GetType()? },
        MEUpdatedStream.0 as u32
    );
    assert_eq!(
        unsafe { stream.GetEvent(MF_EVENT_FLAG_NO_WAIT)?.GetType()? },
        MEStreamStarted.0 as u32
    );
    assert_eq!(
        unsafe { source.GetEvent(MF_EVENT_FLAG_NO_WAIT)?.GetType()? },
        MESourceStarted.0 as u32
    );
    unsafe { stream.RequestSample(None::<&IUnknown>)? };
    let third_sample = next_sample(stream)?;
    assert_eq!(
        unsafe { third_sample.GetSampleTime()? - second_sample.GetSampleTime()? },
        333_333
    );

    unsafe { stream.SetStreamState(MF_STREAM_STATE_PAUSED)? };
    let paused_error = unsafe { stream.RequestSample(None::<&IUnknown>) }.unwrap_err();
    assert_eq!(
        paused_error.code(),
        windows::Win32::Media::MediaFoundation::MF_E_NOTACCEPTING
    );
    unsafe { stream.SetStreamState(MF_STREAM_STATE_RUNNING)? };

    unsafe { source.Stop()? };
    assert_eq!(
        unsafe { stream.GetEvent(MF_EVENT_FLAG_NO_WAIT)?.GetType()? },
        MEStreamStopped.0 as u32
    );
    assert_eq!(
        unsafe { source.GetEvent(MF_EVENT_FLAG_NO_WAIT)?.GetType()? },
        MESourceStopped.0 as u32
    );

    unsafe { source.Start(&presentation, ptr::null(), &start_position)? };
    assert_eq!(
        unsafe { source.GetEvent(MF_EVENT_FLAG_NO_WAIT)?.GetType()? },
        MEUpdatedStream.0 as u32
    );
    assert_eq!(
        unsafe { stream.GetEvent(MF_EVENT_FLAG_NO_WAIT)?.GetType()? },
        MEStreamStarted.0 as u32
    );
    assert_eq!(
        unsafe { source.GetEvent(MF_EVENT_FLAG_NO_WAIT)?.GetType()? },
        MESourceStarted.0 as u32
    );
    unsafe { source.Stop()? };
    let _ = unsafe { stream.GetEvent(MF_EVENT_FLAG_NO_WAIT)? };
    let _ = unsafe { source.GetEvent(MF_EVENT_FLAG_NO_WAIT)? };

    unsafe { source.Shutdown()? };
    unsafe { source.Shutdown()? };
    let shutdown_error = unsafe { source.CreatePresentationDescriptor() }.unwrap_err();
    assert_eq!(
        shutdown_error.code(),
        windows::Win32::Media::MediaFoundation::MF_E_SHUTDOWN
    );
    Ok(())
}

#[test]
fn external_ingress_frames_reach_media_foundation_without_fixture_generation() -> Result<()> {
    let _media_foundation = MediaFoundationRuntime::startup()?;
    let stream_id =
        StreamId::from_str("00000000-0000-4000-8000-00000000c013").expect("fixed stream id");
    let mut generator = DeterministicNv12Source::new(stream_id, 17, 7_000_000_000)
        .expect("valid external-frame fixture");
    let mut first = generator.next_frame().expect("first external frame");
    let mut second = generator.next_frame().expect("second external frame");
    let mut third = generator.next_frame().expect("third external frame");
    first.payload[0] = 42;
    second.payload[0] = 84;
    third.payload[0] = 126;

    let ingress = Arc::new(Mutex::new(
        ExternalNv12FrameIngress::new(stream_id, 17, 2).expect("bounded ingress"),
    ));
    {
        let mut writer = ingress.lock().expect("test ingress lock");
        writer.push(first).expect("queue first external frame");
        writer.push(second).expect("queue second external frame");
        writer
            .push(third)
            .expect("drop the oldest external frame within the fixed bound");
    }

    let handle = create_in_process_media_source_with_external_ingress(Arc::clone(&ingress))?;
    let source = handle.source();
    let stream = handle.stream();
    let allocator_control: IMFSampleAllocatorControl = source.cast()?;
    let allocator = create_video_sample_allocator()?;
    unsafe { allocator_control.SetDefaultAllocator(0, &allocator)? };

    let presentation = unsafe { source.CreatePresentationDescriptor()? };
    let start_position = PROPVARIANT::from(0_i64);
    unsafe { source.Start(&presentation, ptr::null(), &start_position)? };
    let _ = unsafe { source.GetEvent(MF_EVENT_FLAG_NO_WAIT)? };
    let _ = unsafe { stream.GetEvent(MF_EVENT_FLAG_NO_WAIT)? };
    let _ = unsafe { source.GetEvent(MF_EVENT_FLAG_NO_WAIT)? };

    unsafe { stream.RequestSample(None::<&IUnknown>)? };
    let first_sample = next_sample(stream)?;
    assert_eq!(read_first_luma(&first_sample)?, 84);
    assert_eq!(
        unsafe { first_sample.GetUINT32(&MFSampleExtension_Discontinuity)? },
        1
    );
    unsafe { stream.RequestSample(None::<&IUnknown>)? };
    let second_sample = next_sample(stream)?;
    assert_eq!(read_first_luma(&second_sample)?, 126);
    assert_eq!(
        unsafe { second_sample.GetSampleTime()? - first_sample.GetSampleTime()? },
        333_333
    );
    assert_eq!(
        ingress.lock().expect("test ingress lock").pending_frames(),
        0
    );

    let empty_error = unsafe { stream.RequestSample(None::<&IUnknown>) }
        .expect_err("an empty external ingress must fail without blocking");
    assert_eq!(
        empty_error.code(),
        windows::Win32::Media::MediaFoundation::MF_E_NOTACCEPTING
    );

    unsafe { source.Stop()? };
    let _ = unsafe { stream.GetEvent(MF_EVENT_FLAG_NO_WAIT)? };
    let _ = unsafe { source.GetEvent(MF_EVENT_FLAG_NO_WAIT)? };
    unsafe { source.Shutdown()? };
    Ok(())
}

fn next_sample(stream: &IMFMediaStream2) -> Result<IMFSample> {
    let event = unsafe { stream.GetEvent(MF_EVENT_FLAG_NO_WAIT)? };
    assert_eq!(unsafe { event.GetType()? }, MEMediaSample.0 as u32);
    let unknown: IUnknown = IUnknown::try_from(&unsafe { event.GetValue()? })?;
    unknown.cast()
}

fn inspect_nv12_buffer(sample: &IMFSample) -> Result<()> {
    let first_luma = read_first_luma(sample)?;
    assert!((16..=235).contains(&first_luma));
    Ok(())
}

fn read_first_luma(sample: &IMFSample) -> Result<u8> {
    let buffer = unsafe { sample.GetBufferByIndex(0)? };
    let buffer_2d: IMF2DBuffer = buffer.cast()?;
    let mut scanline = ptr::null_mut();
    let mut pitch = 0_i32;
    unsafe { buffer_2d.Lock2D(&mut scanline, &mut pitch)? };
    assert!(!scanline.is_null());
    assert!(pitch >= 1280);
    let first_luma = unsafe { *scanline };
    unsafe { buffer_2d.Unlock2D()? };
    Ok(first_luma)
}

const fn pack_ratio(numerator: u32, denominator: u32) -> u64 {
    ((numerator as u64) << 32) | denominator as u64
}

fn create_video_sample_allocator() -> Result<IMFVideoSampleAllocator> {
    let mut raw = ptr::null_mut();
    unsafe {
        MFCreateVideoSampleAllocatorEx(&IMFVideoSampleAllocator::IID, &mut raw)?;
        Ok(IMFVideoSampleAllocator::from_raw(raw))
    }
}
