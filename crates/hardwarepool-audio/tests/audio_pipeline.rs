use hardwarepool_audio::{AudioFrame, ClockDriftEstimator, InsertOutcome, ReorderBuffer};
use hardwarepool_core::{AudioFormat, StreamId};

#[test]
fn decoded_frames_can_be_validated_reordered_and_measured() {
    let format = AudioFormat::microphone_baseline();
    let stream_id = StreamId::new();
    let make_frame = |sequence: u64| AudioFrame {
        stream_id,
        stream_epoch: 1,
        sequence,
        source_timestamp_micros: sequence * 10_000,
        first_sample_index: sequence * 480,
        sample_count: 480,
        discontinuity: false,
        payload: vec![0; 960],
    };

    let later = make_frame(1);
    let first = make_frame(0);
    later.validate(&format).expect("later frame");
    first.validate(&format).expect("first frame");

    let mut buffer = ReorderBuffer::new(stream_id, 1, 0, 8).expect("buffer");
    assert_eq!(buffer.insert(later), InsertOutcome::Accepted);
    assert_eq!(buffer.insert(first), InsertOutcome::Accepted);
    assert_eq!(buffer.pop_next().expect("sequence 0").sequence, 0);
    assert_eq!(buffer.pop_next().expect("sequence 1").sequence, 1);

    let mut drift = ClockDriftEstimator::new(format.sample_rate_hz);
    assert!(drift.observe(0, 0).is_none());
    let estimate = drift.observe(48_000, 1_000_000).expect("estimate");
    assert!((estimate.rate_ratio - 1.0).abs() < f64::EPSILON);
}
