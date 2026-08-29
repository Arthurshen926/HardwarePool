use capyio_core::StreamId;
use capyio_video::{
    CameraControlDescriptor, CameraDescriptor, LensFacing, VideoColorimetry, VideoContractError,
    VideoFrameDescriptor, VideoFrameFlags, VideoPixelFormat, VideoStreamCapabilities,
    VideoStreamSpec, VideoUseCase, negotiate_video_stream,
};

#[test]
fn camera_baseline_negotiates_only_an_exact_complete_candidate() {
    let baseline = VideoStreamSpec::camera_720p30_nv12();
    baseline.validate().expect("720p30 NV12 baseline");
    let source = VideoStreamCapabilities::new(vec![baseline.clone()]).expect("source");
    let sink = VideoStreamCapabilities::new(vec![baseline.clone()]).expect("sink");
    assert_eq!(
        negotiate_video_stream(&source, &sink, VideoUseCase::CameraBalanced).expect("match"),
        baseline
    );
    let format = baseline.format_descriptor();
    format.validate().expect("Core format descriptor");
    assert_eq!(format.id, "packed-raw-video-v1");
    assert_eq!(format.parameters["pixel_format"], "nv12");
    assert_eq!(format.parameters["colorimetry"], "bt709_limited");

    let mut resized = VideoStreamSpec::camera_720p30_nv12();
    resized.width = 1920;
    resized.height = 1080;
    let resized_sink = VideoStreamCapabilities::new(vec![resized]).expect("resized sink");
    assert!(matches!(
        negotiate_video_stream(&source, &resized_sink, VideoUseCase::CameraBalanced),
        Err(VideoContractError::NoCompatibleVideoStream)
    ));
}

#[test]
fn candidate_inventory_is_bounded_unique_and_nonempty() {
    assert!(matches!(
        VideoStreamCapabilities::new(Vec::new()),
        Err(VideoContractError::EmptyStreamCandidates)
    ));
    let baseline = VideoStreamSpec::camera_720p30_nv12();
    assert!(matches!(
        VideoStreamCapabilities::new(vec![baseline.clone(), baseline.clone()]),
        Err(VideoContractError::DuplicateStreamCandidate)
    ));
    assert!(matches!(
        VideoStreamCapabilities::new(vec![baseline; 33]),
        Err(VideoContractError::TooManyStreamCandidates {
            actual: 33,
            limit: 32
        })
    ));
}

#[test]
fn stream_validation_rejects_malformed_raw_contracts() {
    let mut odd_nv12 = VideoStreamSpec::camera_720p30_nv12();
    odd_nv12.width = 1279;
    assert!(matches!(
        odd_nv12.validate(),
        Err(VideoContractError::InvalidStreamSpec(_))
    ));

    let mut excessive_rate = VideoStreamSpec::camera_720p30_nv12();
    excessive_rate.frame_rate = capyio_video::FrameRate::new(481, 1).expect("rate");
    assert!(matches!(
        excessive_rate.validate(),
        Err(VideoContractError::InvalidStreamSpec(_))
    ));

    let mut invalid_color = VideoStreamSpec::camera_720p30_nv12();
    invalid_color.colorimetry = VideoColorimetry::SrgbFull;
    assert!(matches!(
        invalid_color.validate(),
        Err(VideoContractError::InvalidStreamSpec(_))
    ));
}

#[test]
fn equivalent_frame_rates_are_canonical_and_oversized_raw_frames_are_rejected() {
    assert_eq!(
        capyio_video::FrameRate::new(60_000, 2_000),
        Some(capyio_video::FrameRate::fps_30())
    );
    let noncanonical: capyio_video::FrameRate =
        serde_json::from_str(r#"{"numerator":60000,"denominator":2000}"#)
            .expect("diagnostic shape");
    assert!(matches!(
        noncanonical.validate(),
        Err(VideoContractError::InvalidStreamSpec(_))
    ));

    let mut oversized = VideoStreamSpec::camera_720p30_nv12();
    oversized.width = 8_192;
    oversized.height = 8_192;
    oversized.pixel_format = VideoPixelFormat::Bgra8;
    oversized.colorimetry = VideoColorimetry::SrgbFull;
    assert!(matches!(
        oversized.validate(),
        Err(VideoContractError::InvalidStreamSpec(_))
    ));
}

#[test]
fn camera_descriptor_keeps_core_identity_and_platform_inventory_outside_the_contract() {
    let descriptor = CameraDescriptor {
        lens_facing: LensFacing::Back,
        sensor_orientation_degrees: 90,
        streams: VideoStreamCapabilities::new(vec![VideoStreamSpec::camera_720p30_nv12()])
            .expect("streams"),
        controls: vec![CameraControlDescriptor::ZoomRatio {
            minimum_milli: 1_000,
            maximum_milli: 10_000,
            step_milli: 100,
            default_milli: 1_000,
        }],
    };
    descriptor.validate().expect("camera descriptor");

    let mut invalid = descriptor;
    invalid.controls = vec![CameraControlDescriptor::ZoomRatio {
        minimum_milli: 2_000,
        maximum_milli: 1_000,
        step_milli: 0,
        default_milli: 1_000,
    }];
    assert!(matches!(
        invalid.validate(),
        Err(VideoContractError::InvalidCameraControl(_))
    ));
}

#[test]
fn frame_descriptor_validates_epoch_selected_stream_and_packed_payload_size() {
    let selected = VideoStreamSpec::camera_720p30_nv12();
    let frame = VideoFrameDescriptor {
        stream_id: StreamId::new(),
        stream_epoch: 1,
        sequence: 7,
        source_timestamp_nanos: 1_000_000_000,
        duration_nanos: 33_333_333,
        payload_bytes: 1280 * 720 * 3 / 2,
        flags: VideoFrameFlags::default(),
    };
    frame.validate(&selected).expect("packed NV12 frame");

    let mut stale = frame.clone();
    stale.stream_epoch = 0;
    assert!(matches!(
        stale.validate(&selected),
        Err(VideoContractError::InvalidFrameDescriptor(_))
    ));

    let mut wrong_size = frame;
    wrong_size.payload_bytes -= 1;
    assert!(matches!(
        wrong_size.validate(&selected),
        Err(VideoContractError::InvalidFrameDescriptor(_))
    ));

    let end = VideoFrameDescriptor {
        payload_bytes: 0,
        flags: VideoFrameFlags {
            discontinuity: false,
            end_of_stream: true,
        },
        ..wrong_size
    };
    end.validate(&selected).expect("end of stream");
}
