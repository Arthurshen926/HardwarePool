use std::str::FromStr;

use capyio_core::StreamId;
use capyio_video::VideoFrameFlags;
use capyio_windows_camera::{
    CameraFixtureError, DeterministicNv12Source, ExternalNv12FrameIngress,
    ExternalNv12FrameIngressError, FrameQueuePushOutcome, GeneratedVideoFrame,
};

const STREAM: &str = "00000000-0000-4000-8000-00000000c013";
const OTHER_STREAM: &str = "00000000-0000-4000-8000-00000000c014";

fn source(stream_id: StreamId, epoch: u64) -> DeterministicNv12Source {
    DeterministicNv12Source::new(stream_id, epoch, 4_000_000_000).unwrap()
}

#[test]
fn ingress_is_fixed_bounded_and_drops_oldest_with_discontinuity() {
    let stream_id = StreamId::from_str(STREAM).unwrap();
    let mut source = source(stream_id, 9);
    let mut ingress = ExternalNv12FrameIngress::new(stream_id, 9, 2).unwrap();

    assert_eq!(
        ingress.push(source.next_frame().unwrap()).unwrap(),
        FrameQueuePushOutcome::Queued
    );
    assert_eq!(
        ingress.push(source.next_frame().unwrap()).unwrap(),
        FrameQueuePushOutcome::Queued
    );
    assert_eq!(
        ingress.push(source.next_frame().unwrap()).unwrap(),
        FrameQueuePushOutcome::DroppedOldest { sequence: 0 }
    );

    assert_eq!(ingress.stream_id(), stream_id);
    assert_eq!(ingress.stream_epoch(), 9);
    assert_eq!(ingress.capacity(), 2);
    assert_eq!(ingress.pending_frames(), 2);
    assert_eq!(ingress.last_accepted_sequence(), Some(2));
    let first = ingress.pop().unwrap();
    assert_eq!(first.descriptor.sequence, 1);
    assert!(first.descriptor.flags.discontinuity);
    assert_eq!(ingress.pop().unwrap().descriptor.sequence, 2);
    assert!(ingress.pop().is_none());
    assert_eq!(ingress.metrics().accepted_frames, 3);
    assert_eq!(ingress.metrics().dropped_oldest_frames, 1);
}

#[test]
fn ingress_rejects_wrong_identity_and_nonadvancing_frames_transactionally() {
    let stream_id = StreamId::from_str(STREAM).unwrap();
    let other_stream_id = StreamId::from_str(OTHER_STREAM).unwrap();
    let mut ingress = ExternalNv12FrameIngress::new(stream_id, 9, 2).unwrap();

    let wrong_stream = source(other_stream_id, 9).next_frame().unwrap();
    assert_eq!(
        ingress.push(wrong_stream),
        Err(ExternalNv12FrameIngressError::WrongStream {
            expected: stream_id,
            actual: other_stream_id,
        })
    );
    let wrong_epoch = source(stream_id, 10).next_frame().unwrap();
    assert_eq!(
        ingress.push(wrong_epoch),
        Err(ExternalNv12FrameIngressError::WrongEpoch {
            expected: 9,
            actual: 10,
        })
    );

    let mut valid_source = source(stream_id, 9);
    let first = valid_source.next_frame().unwrap();
    ingress.push(first.clone()).unwrap();
    assert_eq!(
        ingress.push(first),
        Err(ExternalNv12FrameIngressError::NonAdvancingSequence {
            previous: 0,
            actual: 0,
        })
    );

    let mut second = valid_source.next_frame().unwrap();
    second.descriptor.source_timestamp_nanos = 4_000_000_000;
    assert_eq!(
        ingress.push(second),
        Err(ExternalNv12FrameIngressError::NonAdvancingSourceTimestamp {
            previous: 4_000_000_000,
            actual: 4_000_000_000,
        })
    );
    assert_eq!(ingress.pending_frames(), 1);
    assert_eq!(ingress.last_accepted_sequence(), Some(0));

    let second = valid_source.next_frame().unwrap();
    assert_eq!(second.descriptor.sequence, 2);
    ingress.push(second).unwrap();
    assert_eq!(ingress.last_accepted_sequence(), Some(2));
}

#[test]
fn ingress_rejects_invalid_epoch_payload_and_end_of_stream() {
    let stream_id = StreamId::from_str(STREAM).unwrap();
    assert_eq!(
        ExternalNv12FrameIngress::new(stream_id, 0, 2).unwrap_err(),
        ExternalNv12FrameIngressError::InvalidStreamEpoch
    );
    assert!(matches!(
        ExternalNv12FrameIngress::new(stream_id, 9, 0),
        Err(ExternalNv12FrameIngressError::Frame(
            CameraFixtureError::InvalidQueueCapacity { .. }
        ))
    ));

    let mut ingress = ExternalNv12FrameIngress::new(stream_id, 9, 2).unwrap();
    let mut short = source(stream_id, 9).next_frame().unwrap();
    short.payload.pop();
    assert!(matches!(
        ingress.push(short),
        Err(ExternalNv12FrameIngressError::Frame(
            CameraFixtureError::PayloadLengthMismatch { .. }
        ))
    ));

    let end = GeneratedVideoFrame {
        descriptor: capyio_video::VideoFrameDescriptor {
            stream_id,
            stream_epoch: 9,
            sequence: 0,
            source_timestamp_nanos: 4_000_000_000,
            duration_nanos: 33_333_333,
            payload_bytes: 0,
            flags: VideoFrameFlags {
                discontinuity: false,
                end_of_stream: true,
            },
        },
        payload: Vec::new(),
    };
    assert_eq!(
        ingress.push(end),
        Err(ExternalNv12FrameIngressError::EndOfStreamUnsupported)
    );
}
