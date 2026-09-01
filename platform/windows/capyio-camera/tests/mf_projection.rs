use std::str::FromStr;

use capyio_core::StreamId;
use capyio_windows_camera::{
    DeterministicNv12Source, MAX_VIRTUAL_CAMERA_FRIENDLY_NAME_UTF16,
    MediaFoundationProjectionError, MfSampleTimingMapper, MfVirtualCameraAccess,
    MfVirtualCameraAction, MfVirtualCameraLifecycle, MfVirtualCameraLifecycleState,
    MfVirtualCameraLifetime, MfVirtualCameraPlan, copy_nv12_to_strided_buffer, fixture_stream_spec,
};

fn source() -> DeterministicNv12Source {
    DeterministicNv12Source::new(
        StreamId::from_str("00000000-0000-4000-8000-00000000c011").unwrap(),
        3,
        7_000_000_000,
    )
    .unwrap()
}

#[test]
fn registration_plan_is_session_current_user_and_bounded() {
    let plan = MfVirtualCameraPlan::capyio_fixture();
    assert_eq!(plan.friendly_name(), "CapyIO Camera");
    assert_eq!(plan.lifetime(), MfVirtualCameraLifetime::Session);
    assert_eq!(plan.access(), MfVirtualCameraAccess::CurrentUser);
    assert!(plan.source_clsid().starts_with('{') && plan.source_clsid().ends_with('}'));

    assert!(MfVirtualCameraPlan::new("").is_err());
    assert!(MfVirtualCameraPlan::new(" leading").is_err());
    assert!(MfVirtualCameraPlan::new("line\nbreak").is_err());
    assert!(MfVirtualCameraPlan::new("x".repeat(MAX_VIRTUAL_CAMERA_FRIENDLY_NAME_UTF16)).is_ok());
    assert!(
        MfVirtualCameraPlan::new("x".repeat(MAX_VIRTUAL_CAMERA_FRIENDLY_NAME_UTF16 + 1)).is_err()
    );
}

#[test]
fn lifecycle_models_start_stop_restart_and_session_shutdown() {
    let mut lifecycle = MfVirtualCameraLifecycle::default();
    assert_eq!(
        lifecycle.apply(MfVirtualCameraAction::Start),
        Ok(MfVirtualCameraLifecycleState::Started)
    );
    assert_eq!(
        lifecycle.apply(MfVirtualCameraAction::Stop),
        Ok(MfVirtualCameraLifecycleState::Stopped)
    );
    assert_eq!(
        lifecycle.apply(MfVirtualCameraAction::Start),
        Ok(MfVirtualCameraLifecycleState::Started)
    );
    assert_eq!(
        lifecycle.apply(MfVirtualCameraAction::Shutdown),
        Ok(MfVirtualCameraLifecycleState::Shutdown)
    );
    assert!(matches!(
        lifecycle.apply(MfVirtualCameraAction::Start),
        Err(MediaFoundationProjectionError::InvalidLifecycleTransition { .. })
    ));
}

#[test]
fn capyio_timestamps_map_to_qpc_correlated_100ns_units_without_drift() {
    let mut source = source();
    let first = source.next_frame().unwrap();
    let selected = fixture_stream_spec();
    let qpc_anchor = 123_000_000_i64;
    let mut mapper = MfSampleTimingMapper::new(&first.descriptor, &selected, qpc_anchor).unwrap();
    let first_timing = mapper.map(&first.descriptor, &selected).unwrap();
    assert_eq!(first_timing.sample_time_100ns, qpc_anchor);
    assert_eq!(first_timing.sample_duration_100ns, 333_333);

    let mut frame = source.next_frame().unwrap();
    for _ in 2..=30 {
        frame = source.next_frame().unwrap();
    }
    let timing = mapper.map(&frame.descriptor, &selected).unwrap();
    assert_eq!(frame.descriptor.sequence, 30);
    assert_eq!(timing.sample_time_100ns, qpc_anchor + 10_000_000);
}

#[test]
fn timing_mapper_rejects_wrong_epoch_duplicate_and_pre_anchor_frames_transactionally() {
    let mut source = source();
    let first = source.next_frame().unwrap();
    let selected = fixture_stream_spec();
    let mut mapper = MfSampleTimingMapper::new(&first.descriptor, &selected, 1_000).unwrap();

    let mut wrong_epoch = first.descriptor.clone();
    wrong_epoch.stream_epoch += 1;
    assert!(matches!(
        mapper.map(&wrong_epoch, &selected),
        Err(MediaFoundationProjectionError::WrongEpoch { .. })
    ));
    assert!(mapper.map(&first.descriptor, &selected).is_ok());
    assert!(matches!(
        mapper.map(&first.descriptor, &selected),
        Err(MediaFoundationProjectionError::NonAdvancingSequence { .. })
    ));

    let mut before_anchor = source.next_frame().unwrap().descriptor;
    before_anchor.source_timestamp_nanos = 6_999_999_999;
    assert_eq!(
        mapper.map(&before_anchor, &selected),
        Err(MediaFoundationProjectionError::TimestampBeforeAnchor)
    );
}

#[test]
fn packed_nv12_is_copied_rowwise_and_padding_is_cleared() {
    let frame = source().next_frame().unwrap();
    let width = 1280_usize;
    let height = 720_usize;
    let pitch = 1344_usize;
    let required = pitch * (height + height / 2);
    let mut destination = vec![0xa5; required + 16];

    let layout = copy_nv12_to_strided_buffer(&frame, pitch, &mut destination).unwrap();
    assert_eq!(layout.row_pitch_bytes, pitch);
    assert_eq!(layout.required_bytes, 1_451_520);
    assert_eq!(&destination[..width], &frame.payload[..width]);
    assert!(destination[width..pitch].iter().all(|byte| *byte == 0));

    let source_chroma = width * height;
    let destination_chroma = pitch * height;
    assert_eq!(
        &destination[destination_chroma..destination_chroma + width],
        &frame.payload[source_chroma..source_chroma + width]
    );
    assert!(destination[required..].iter().all(|byte| *byte == 0xa5));
}

#[test]
fn strided_copy_rejects_short_pitch_and_short_destination() {
    let frame = source().next_frame().unwrap();
    assert!(matches!(
        copy_nv12_to_strided_buffer(&frame, 1279, &mut []),
        Err(MediaFoundationProjectionError::InvalidRowPitch { .. })
    ));
    assert!(matches!(
        copy_nv12_to_strided_buffer(&frame, 1280, &mut [0_u8; 32]),
        Err(MediaFoundationProjectionError::DestinationTooSmall { .. })
    ));
}
