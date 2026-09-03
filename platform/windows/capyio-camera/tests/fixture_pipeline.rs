use std::str::FromStr;

use capyio_core::StreamId;
use capyio_video::{VideoColorimetry, VideoPixelFormat};
use capyio_windows_camera::{
    BoundedFrameQueue, CameraFixtureError, DeterministicNv12Source, FrameQueueOverflowPolicy,
    FrameQueuePushOutcome, MAX_FIXTURE_QUEUE_BYTES, MAX_FIXTURE_QUEUE_FRAMES, fixture_stream_spec,
    frame_checksum64,
};

fn fixture_source() -> DeterministicNv12Source {
    DeterministicNv12Source::new(
        StreamId::from_str("00000000-0000-4000-8000-00000000c001").unwrap(),
        7,
        5_000_000_000,
    )
    .unwrap()
}

#[test]
fn fixture_uses_the_canonical_720p30_nv12_contract() {
    let selected = fixture_stream_spec();
    selected.validate().unwrap();
    assert_eq!(selected.width, 1280);
    assert_eq!(selected.height, 720);
    assert_eq!(selected.frame_rate.numerator(), 30);
    assert_eq!(selected.frame_rate.denominator(), 1);
    assert_eq!(selected.pixel_format, VideoPixelFormat::Nv12);
    assert_eq!(selected.colorimetry, VideoColorimetry::Bt709Limited);
    assert_eq!(selected.packed_frame_bytes(), Some(1_382_400));
    assert_eq!(MAX_FIXTURE_QUEUE_BYTES, 16_588_800);
}

#[test]
fn generated_frames_are_reproducible_and_have_a_pinned_checksum() {
    let mut first_source = fixture_source();
    let mut second_source = fixture_source();
    let first = first_source.next_frame().unwrap();
    let repeated = second_source.next_frame().unwrap();

    assert_eq!(first, repeated);
    assert_eq!(first.descriptor.sequence, 0);
    assert_eq!(first.descriptor.source_timestamp_nanos, 5_000_000_000);
    assert_eq!(first.descriptor.duration_nanos, 33_333_333);
    assert_eq!(first.payload.len(), 1_382_400);
    assert_eq!(frame_checksum64(&first.payload), 8_998_312_799_283_937_061);
}

#[test]
fn rational_timestamps_reach_one_second_without_drift() {
    let mut source = fixture_source();
    let mut frame = source.next_frame().unwrap();
    for _ in 1..=30 {
        frame = source.next_frame().unwrap();
    }

    assert_eq!(frame.descriptor.sequence, 30);
    assert_eq!(frame.descriptor.source_timestamp_nanos, 6_000_000_000);
    assert_eq!(source.next_sequence(), 31);
}

#[test]
fn fixture_can_resume_an_existing_output_timeline() {
    let stream_id = StreamId::from_str("00000000-0000-4000-8000-00000000c001").unwrap();
    let mut source =
        DeterministicNv12Source::new_at_sequence(stream_id, 9, 30, 8_500_000_123).unwrap();

    let first = source.next_frame().unwrap();
    let second = source.next_frame().unwrap();

    assert_eq!(first.descriptor.stream_id, stream_id);
    assert_eq!(first.descriptor.stream_epoch, 9);
    assert_eq!(first.descriptor.sequence, 30);
    assert_eq!(first.descriptor.source_timestamp_nanos, 8_500_000_123);
    assert_eq!(first.descriptor.duration_nanos, 33_333_333);
    assert_eq!(second.descriptor.sequence, 31);
    assert_eq!(second.descriptor.source_timestamp_nanos, 8_533_333_456);
    assert_eq!(source.next_sequence(), 32);
}

#[test]
fn moving_clock_changes_payload_but_keeps_nv12_bounds() {
    let mut source = fixture_source();
    let first = source.next_frame().unwrap();
    let second = source.next_frame().unwrap();

    assert_ne!(
        frame_checksum64(&first.payload),
        frame_checksum64(&second.payload)
    );
    assert!(
        first
            .payload
            .iter()
            .all(|sample| (16..=240).contains(sample))
    );
    first.validate(&fixture_stream_spec()).unwrap();
    second.validate(&fixture_stream_spec()).unwrap();
}

#[test]
fn drop_oldest_queue_is_bounded_and_marks_the_gap() {
    let mut source = fixture_source();
    let mut queue = BoundedFrameQueue::new(2, FrameQueueOverflowPolicy::DropOldest).unwrap();

    assert_eq!(
        queue.push(source.next_frame().unwrap()).unwrap(),
        FrameQueuePushOutcome::Queued
    );
    assert_eq!(
        queue.push(source.next_frame().unwrap()).unwrap(),
        FrameQueuePushOutcome::Queued
    );
    assert_eq!(
        queue.push(source.next_frame().unwrap()).unwrap(),
        FrameQueuePushOutcome::DroppedOldest { sequence: 0 }
    );

    assert_eq!(queue.len(), 2);
    assert_eq!(queue.front().unwrap().descriptor.sequence, 1);
    let oldest = queue.pop().unwrap();
    assert_eq!(oldest.descriptor.sequence, 1);
    assert!(oldest.descriptor.flags.discontinuity);
    let newest = queue.pop().unwrap();
    assert_eq!(newest.descriptor.sequence, 2);
    assert!(!newest.descriptor.flags.discontinuity);
    assert_eq!(queue.metrics().accepted_frames, 3);
    assert_eq!(queue.metrics().overflow_events, 1);
    assert_eq!(queue.metrics().dropped_oldest_frames, 1);
    assert_eq!(queue.metrics().high_watermark_frames, 2);
}

#[test]
fn reject_newest_queue_preserves_the_queued_frame() {
    let mut source = fixture_source();
    let mut queue = BoundedFrameQueue::new(1, FrameQueueOverflowPolicy::RejectNewest).unwrap();
    queue.push(source.next_frame().unwrap()).unwrap();

    assert_eq!(
        queue.push(source.next_frame().unwrap()),
        Err(CameraFixtureError::QueueFull {
            rejected_sequence: 1
        })
    );
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.front().unwrap().descriptor.sequence, 0);
    assert_eq!(queue.metrics().accepted_frames, 1);
    assert_eq!(queue.metrics().rejected_newest_frames, 1);
}

#[test]
fn invalid_epoch_capacity_and_payload_are_rejected() {
    let stream_id = StreamId::from_str("00000000-0000-4000-8000-00000000c001").unwrap();
    assert_eq!(
        DeterministicNv12Source::new(stream_id, 0, 0).unwrap_err(),
        CameraFixtureError::InvalidStreamEpoch
    );
    assert_eq!(
        DeterministicNv12Source::new_at_sequence(stream_id, 1, u64::MAX, 0).unwrap_err(),
        CameraFixtureError::SequenceExhausted
    );
    assert_eq!(
        DeterministicNv12Source::new_at_sequence(stream_id, 1, 0, u64::MAX).unwrap_err(),
        CameraFixtureError::TimestampOverflow
    );
    assert!(matches!(
        BoundedFrameQueue::new(
            MAX_FIXTURE_QUEUE_FRAMES + 1,
            FrameQueueOverflowPolicy::DropOldest,
        ),
        Err(CameraFixtureError::InvalidQueueCapacity { .. })
    ));

    let mut source = fixture_source();
    let mut frame = source.next_frame().unwrap();
    frame.payload.pop();
    assert_eq!(
        frame.validate(&fixture_stream_spec()),
        Err(CameraFixtureError::PayloadLengthMismatch {
            declared: 1_382_400,
            actual: 1_382_399,
        })
    );
}
