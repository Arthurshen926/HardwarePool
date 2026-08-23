use std::str::FromStr;

use capyio_core::{ProfileId, StreamId};
use capyio_data_plane::{
    BoundedEnvelopeQueue, BoundedFanout, BoundedJsonlRecorder, ConsumerPublishOutcome,
    DataEnvelope, DataPlaneError, ImuSampleV1, NumericImuPanel, PushOutcome, RecorderOutcome,
    SequenceGap, parse_imu_fixture_jsonl,
};

const FIXTURE: &str = include_str!("../../../fixtures/imu/imu_samples_v1.jsonl");

fn fixture() -> Vec<DataEnvelope<ImuSampleV1>> {
    parse_imu_fixture_jsonl(FIXTURE, 64).expect("valid deterministic IMU JSONL")
}

fn outcome<'a>(
    outcomes: &'a std::collections::BTreeMap<
        String,
        Result<ConsumerPublishOutcome, DataPlaneError>,
    >,
    id: &str,
) -> &'a ConsumerPublishOutcome {
    outcomes
        .get(id)
        .expect("registered consumer")
        .as_ref()
        .expect("valid fixture")
}

#[test]
fn deterministic_fixture_is_profile_valid_and_monotonic() {
    let fixture = fixture();
    assert_eq!(fixture.len(), 6);
    for (index, envelope) in fixture.iter().enumerate() {
        envelope
            .validate_for_profile(&ImuSampleV1::profile())
            .expect("valid IMU envelope");
        assert_eq!(envelope.sequence, index as u64);
        assert_eq!(envelope.stream_epoch, 1);
    }
    assert!(fixture.windows(2).all(|pair| {
        pair[0].source_timestamp_nanos < pair[1].source_timestamp_nanos
            && pair[0].receive_timestamp_nanos < pair[1].receive_timestamp_nanos
    }));
}

#[test]
fn panel_and_recorder_consume_the_same_profile_independently() {
    let fixture = fixture();
    let stream_id = fixture[0].stream_id;
    let mut fanout = BoundedFanout::new(ImuSampleV1::profile(), stream_id, 1);
    fanout.register_consumer("panel", 16).unwrap();
    fanout.register_consumer("recorder", 16).unwrap();

    for envelope in fixture.iter().cloned() {
        let outcomes = fanout.publish(envelope);
        assert_eq!(
            outcome(&outcomes, "panel"),
            &ConsumerPublishOutcome::Queue(PushOutcome::Enqueued { gap: None })
        );
        assert_eq!(
            outcome(&outcomes, "recorder"),
            &ConsumerPublishOutcome::Queue(PushOutcome::Enqueued { gap: None })
        );
    }

    let mut panel = NumericImuPanel::default();
    let mut recorder = BoundedJsonlRecorder::new(16, 4096).unwrap();
    while let Some(delivery) = fanout.pop("panel").unwrap() {
        panel.consume(delivery);
    }
    while let Some(delivery) = fanout.pop("recorder").unwrap() {
        assert_eq!(
            recorder.record(&delivery).unwrap(),
            RecorderOutcome::Recorded
        );
    }

    assert_eq!(panel.received, fixture.len() as u64);
    assert_eq!(panel.missing_sequences, 0);
    assert_eq!(panel.last_sample.as_ref(), Some(&fixture[5].payload));
    assert_eq!(recorder.len(), fixture.len());
    for line in recorder.as_jsonl().lines() {
        let _: serde_json::Value = serde_json::from_str(line).expect("valid JSONL line");
    }
}

#[test]
fn slow_or_stopped_recorder_does_not_block_panel() {
    let fixture = fixture();
    let mut fanout = BoundedFanout::new(ImuSampleV1::profile(), fixture[0].stream_id, 1);
    fanout.register_consumer("panel", 8).unwrap();
    fanout.register_consumer("recorder", 1).unwrap();

    fanout.publish(fixture[0].clone());
    let first_panel = fanout.pop("panel").unwrap().unwrap();

    let second = fanout.publish(fixture[1].clone());
    assert_eq!(
        outcome(&second, "panel"),
        &ConsumerPublishOutcome::Queue(PushOutcome::Enqueued { gap: None })
    );
    assert_eq!(
        outcome(&second, "recorder"),
        &ConsumerPublishOutcome::Queue(PushOutcome::Full { capacity: 1 })
    );
    assert_eq!(fanout.rejected_full("recorder").unwrap(), 1);

    assert_eq!(fanout.stop_consumer("recorder").unwrap(), 1);
    let third = fanout.publish(fixture[2].clone());
    assert_eq!(
        outcome(&third, "recorder"),
        &ConsumerPublishOutcome::Stopped
    );

    let mut panel = NumericImuPanel::default();
    panel.consume(first_panel);
    while let Some(delivery) = fanout.pop("panel").unwrap() {
        panel.consume(delivery);
    }
    assert_eq!(panel.received, 3);

    fanout.start_consumer("recorder").unwrap();
    let fourth = fanout.publish(fixture[3].clone());
    assert_eq!(
        outcome(&fourth, "recorder"),
        &ConsumerPublishOutcome::Queue(PushOutcome::Enqueued { gap: None })
    );
}

#[test]
fn gaps_duplicates_late_samples_and_epochs_are_explicit() {
    let fixture = fixture();
    let mut queue =
        BoundedEnvelopeQueue::new(ImuSampleV1::profile(), fixture[0].stream_id, 1, 4).unwrap();

    assert_eq!(
        queue.push(fixture[0].clone()).unwrap(),
        PushOutcome::Enqueued { gap: None }
    );
    queue.pop();
    assert_eq!(
        queue.push(fixture[2].clone()).unwrap(),
        PushOutcome::Enqueued {
            gap: Some(SequenceGap {
                first_missing: 1,
                last_missing: 1,
            })
        }
    );
    assert_eq!(
        queue.push(fixture[2].clone()).unwrap(),
        PushOutcome::Duplicate { sequence: 2 }
    );
    queue.pop();
    assert_eq!(
        queue.push(fixture[1].clone()).unwrap(),
        PushOutcome::Late {
            expected: 3,
            actual: 1,
        }
    );

    queue.advance_epoch(2).unwrap();
    assert_eq!(
        queue.push(fixture[3].clone()).unwrap(),
        PushOutcome::StaleEpoch {
            current: 2,
            actual: 1,
        }
    );
    let mut future = fixture[3].clone();
    future.stream_epoch = 3;
    assert_eq!(
        queue.push(future.clone()).unwrap(),
        PushOutcome::FutureEpoch {
            current: 2,
            actual: 3,
        }
    );
    queue.advance_epoch(3).unwrap();
    assert_eq!(
        queue.push(future).unwrap(),
        PushOutcome::Enqueued { gap: None }
    );
}

#[test]
fn wrong_profile_stream_and_invalid_payload_fail_without_coercion() {
    let fixture = fixture();
    let stream_id = StreamId::from_str("00000000-0000-4000-8000-00000000a002").unwrap();
    let mut queue = BoundedEnvelopeQueue::new(ImuSampleV1::profile(), stream_id, 1, 2).unwrap();
    assert_eq!(
        queue.push(fixture[0].clone()).unwrap(),
        PushOutcome::WrongStream
    );

    let mut wrong_profile = fixture[0].clone();
    wrong_profile.profile = ProfileId::new("capyio.motion.imu-samples", 2);
    assert!(matches!(
        queue.push(wrong_profile),
        Err(DataPlaneError::UnsupportedProfile { .. })
    ));

    let mut invalid = fixture[0].clone();
    invalid.stream_id = stream_id;
    invalid.payload.acceleration[0] = f64::NAN;
    assert!(matches!(
        queue.push(invalid),
        Err(DataPlaneError::InvalidPayload(_))
    ));
}

#[test]
fn recorder_enforces_record_and_line_bounds() {
    let fixture = fixture();
    let delivery = capyio_data_plane::Delivery {
        gap_before: None,
        envelope: fixture[0].clone(),
    };
    let mut one_record = BoundedJsonlRecorder::new(1, 4096).unwrap();
    assert_eq!(
        one_record.record(&delivery).unwrap(),
        RecorderOutcome::Recorded
    );
    assert_eq!(one_record.record(&delivery).unwrap(), RecorderOutcome::Full);

    let mut tiny_line = BoundedJsonlRecorder::new(2, 16).unwrap();
    assert!(matches!(
        tiny_line.record(&delivery).unwrap(),
        RecorderOutcome::LineTooLarge { .. }
    ));
}

#[test]
fn empty_fixture_and_sequence_exhaustion_fail_closed() {
    assert_eq!(
        parse_imu_fixture_jsonl("\n", 8),
        Err(DataPlaneError::EmptyFixture)
    );
    let fixture = fixture();
    let mut terminal = fixture[0].clone();
    terminal.sequence = u64::MAX;
    let mut queue = BoundedEnvelopeQueue::new(
        ImuSampleV1::profile(),
        terminal.stream_id,
        terminal.stream_epoch,
        2,
    )
    .unwrap();
    assert_eq!(
        queue.push(terminal.clone()).unwrap(),
        PushOutcome::Enqueued { gap: None }
    );
    queue.pop();
    assert_eq!(
        queue.push(terminal).unwrap(),
        PushOutcome::Late {
            expected: u64::MAX,
            actual: u64::MAX,
        }
    );
}
