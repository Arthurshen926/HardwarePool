use std::str::FromStr;

use capyio_core::StreamId;
use capyio_data_plane::{DataPlaneError, ImuAccuracy, ImuSampleV1};
use capyio_sensor_server_adapter::{
    AssembleOutcome, MAX_PAIR_SKEW_NANOS, MAX_SENSOR_SERVER_MESSAGE_BYTES, ReplacedUnpairedReading,
    SENSOR_SERVER_CLOCK_DOMAIN, SensorKind, SensorServerError, SensorServerImuAssembler,
    parse_sensor_server_reading,
};

const ACCELEROMETER: &[u8] = include_bytes!("../../../fixtures/sensor-server/accelerometer.json");
const GYROSCOPE: &[u8] = include_bytes!("../../../fixtures/sensor-server/gyroscope.json");

fn stream_id() -> StreamId {
    StreamId::from_str("00000000-0000-4000-8000-00000000b001").unwrap()
}

fn reading(accuracy: i32, timestamp: u64, values: [f64; 3]) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "accuracy": accuracy,
        "timestamp": timestamp,
        "values": values,
    }))
    .unwrap()
}

#[test]
fn documented_message_shape_maps_exact_axes_timestamp_and_accuracy() {
    let acceleration =
        parse_sensor_server_reading(SensorKind::Accelerometer, ACCELEROMETER).unwrap();
    assert_eq!(acceleration.accuracy, ImuAccuracy::Medium);
    assert_eq!(acceleration.timestamp_nanos, 3_925_657_519_043_709);
    assert_eq!(acceleration.values, [0.31892395, -0.97802734, 10.049896]);

    let gyroscope = parse_sensor_server_reading(SensorKind::Gyroscope, GYROSCOPE).unwrap();
    assert_eq!(gyroscope.accuracy, ImuAccuracy::High);
    assert_eq!(gyroscope.values, [0.15387291, -0.22954187, 0.08163925]);
}

#[test]
fn parser_rejects_unbounded_or_ambiguous_input() {
    assert_eq!(
        parse_sensor_server_reading(SensorKind::Accelerometer, b""),
        Err(SensorServerError::EmptyMessage)
    );
    let oversized = vec![b' '; MAX_SENSOR_SERVER_MESSAGE_BYTES + 1];
    assert_eq!(
        parse_sensor_server_reading(SensorKind::Accelerometer, &oversized),
        Err(SensorServerError::MessageTooLarge {
            actual: MAX_SENSOR_SERVER_MESSAGE_BYTES + 1,
            maximum: MAX_SENSOR_SERVER_MESSAGE_BYTES,
        })
    );
    assert!(matches!(
        parse_sensor_server_reading(
            SensorKind::Accelerometer,
            br#"{"accuracy":2,"timestamp":1,"values":[1,2,3],"extra":true}"#,
        ),
        Err(SensorServerError::InvalidJson(_))
    ));
    assert_eq!(
        parse_sensor_server_reading(
            SensorKind::Accelerometer,
            br#"{"accuracy":2,"timestamp":1,"values":[1,2]}"#,
        ),
        Err(SensorServerError::InvalidValueCount { actual: 2 })
    );
    assert_eq!(
        parse_sensor_server_reading(
            SensorKind::Accelerometer,
            br#"{"accuracy":4,"timestamp":1,"values":[1,2,3]}"#,
        ),
        Err(SensorServerError::UnsupportedAccuracy(4))
    );
    assert_eq!(
        parse_sensor_server_reading(
            SensorKind::Accelerometer,
            br#"{"accuracy":2,"timestamp":0,"values":[1,2,3]}"#,
        ),
        Err(SensorServerError::InvalidTimestamp)
    );
}

#[test]
fn either_arrival_order_emits_one_profile_valid_pair() {
    let mut assembler = SensorServerImuAssembler::new(stream_id(), 3, 20_000_000, 7).unwrap();
    assert_eq!(
        assembler
            .ingest_json(SensorKind::Gyroscope, GYROSCOPE, 9_000_000)
            .unwrap(),
        AssembleOutcome::AwaitingCounterpart {
            missing: SensorKind::Accelerometer,
            replaced: None,
        }
    );
    let AssembleOutcome::Emitted { envelope, replaced } = assembler
        .ingest_json(SensorKind::Accelerometer, ACCELEROMETER, 9_100_000)
        .unwrap()
    else {
        panic!("second required component should emit")
    };
    assert_eq!(replaced, None);
    assert_eq!(envelope.stream_epoch, 3);
    assert_eq!(envelope.sequence, 7);
    assert_eq!(envelope.receive_timestamp_nanos, 9_100_000);
    assert_eq!(envelope.clock_domain_id, SENSOR_SERVER_CLOCK_DOMAIN);
    assert_eq!(envelope.payload.accuracy, ImuAccuracy::Medium);
    assert_eq!(
        envelope.payload.acceleration,
        [0.31892395, -0.97802734, 10.049896]
    );
    assert_eq!(
        envelope.payload.angular_velocity,
        [0.15387291, -0.22954187, 0.08163925]
    );
    let components = envelope.payload.component_timestamps.unwrap();
    assert_eq!(components.acceleration_nanos, 3_925_657_519_043_709);
    assert_eq!(components.angular_velocity_nanos, 3_925_657_520_043_709);
    assert_eq!(
        envelope.source_timestamp_nanos,
        components.angular_velocity_nanos
    );
    envelope
        .validate_for_profile(&ImuSampleV1::profile())
        .unwrap();
}

#[test]
fn each_required_component_is_consumed_once() {
    let mut assembler = SensorServerImuAssembler::new(stream_id(), 1, 20, 0).unwrap();
    assembler
        .ingest_json(
            SensorKind::Accelerometer,
            &reading(3, 100, [1.0, 2.0, 3.0]),
            1,
        )
        .unwrap();
    assert!(matches!(
        assembler
            .ingest_json(SensorKind::Gyroscope, &reading(3, 105, [4.0, 5.0, 6.0]), 2)
            .unwrap(),
        AssembleOutcome::Emitted { .. }
    ));
    assert_eq!(
        assembler
            .ingest_json(
                SensorKind::Accelerometer,
                &reading(3, 110, [7.0, 8.0, 9.0]),
                3
            )
            .unwrap(),
        AssembleOutcome::AwaitingCounterpart {
            missing: SensorKind::Gyroscope,
            replaced: None,
        }
    );
    let AssembleOutcome::Emitted { envelope, .. } = assembler
        .ingest_json(
            SensorKind::Gyroscope,
            &reading(3, 112, [10.0, 11.0, 12.0]),
            4,
        )
        .unwrap()
    else {
        panic!("fresh pair should emit")
    };
    assert_eq!(envelope.sequence, 1);
}

#[test]
fn skew_and_unpaired_replacement_are_explicit_and_recoverable() {
    let mut assembler = SensorServerImuAssembler::new(stream_id(), 1, 10, 0).unwrap();
    assembler
        .ingest_json(
            SensorKind::Accelerometer,
            &reading(2, 100, [1.0, 2.0, 3.0]),
            1,
        )
        .unwrap();
    assert_eq!(
        assembler
            .ingest_json(SensorKind::Gyroscope, &reading(2, 200, [4.0, 5.0, 6.0]), 2)
            .unwrap(),
        AssembleOutcome::SkewExceeded {
            acceleration_timestamp_nanos: 100,
            gyroscope_timestamp_nanos: 200,
            maximum_nanos: 10,
            replaced: None,
        }
    );
    let AssembleOutcome::Emitted { envelope, replaced } = assembler
        .ingest_json(
            SensorKind::Accelerometer,
            &reading(2, 195, [7.0, 8.0, 9.0]),
            3,
        )
        .unwrap()
    else {
        panic!("new in-skew acceleration should recover")
    };
    assert_eq!(
        replaced,
        Some(ReplacedUnpairedReading {
            kind: SensorKind::Accelerometer,
            timestamp_nanos: 100,
        })
    );
    assert_eq!(envelope.sequence, 0);
}

#[test]
fn timestamp_regression_fails_without_replacing_last_valid_reading() {
    let mut assembler = SensorServerImuAssembler::new(stream_id(), 1, 20, 0).unwrap();
    assembler
        .ingest_json(SensorKind::Gyroscope, &reading(3, 200, [4.0, 5.0, 6.0]), 1)
        .unwrap();
    assert_eq!(
        assembler.ingest_json(SensorKind::Gyroscope, &reading(3, 199, [0.0; 3]), 2),
        Err(SensorServerError::TimestampNotIncreasing {
            kind: SensorKind::Gyroscope,
            previous: 200,
            actual: 199,
        })
    );
    let AssembleOutcome::Emitted { envelope, .. } = assembler
        .ingest_json(
            SensorKind::Accelerometer,
            &reading(3, 195, [1.0, 2.0, 3.0]),
            3,
        )
        .unwrap()
    else {
        panic!("valid prior gyroscope should remain available")
    };
    assert_eq!(
        envelope
            .payload
            .component_timestamps
            .unwrap()
            .angular_velocity_nanos,
        200
    );
}

#[test]
fn fresh_magnetic_field_is_optional_and_timestamped() {
    let mut assembler = SensorServerImuAssembler::new(stream_id(), 1, 20, 0).unwrap();
    assert_eq!(
        assembler
            .ingest_json(
                SensorKind::MagneticField,
                &reading(1, 102, [7.0, 8.0, 9.0]),
                1
            )
            .unwrap(),
        AssembleOutcome::MagneticFieldUpdated
    );
    assembler
        .ingest_json(
            SensorKind::Accelerometer,
            &reading(3, 100, [1.0, 2.0, 3.0]),
            2,
        )
        .unwrap();
    let AssembleOutcome::Emitted { envelope, .. } = assembler
        .ingest_json(SensorKind::Gyroscope, &reading(2, 105, [4.0, 5.0, 6.0]), 3)
        .unwrap()
    else {
        panic!("required pair should emit")
    };
    assert_eq!(envelope.payload.magnetic_field, Some([7.0, 8.0, 9.0]));
    assert_eq!(
        envelope
            .payload
            .component_timestamps
            .unwrap()
            .magnetic_field_nanos,
        Some(102)
    );
    assert_eq!(envelope.payload.accuracy, ImuAccuracy::Medium);
}

#[test]
fn construction_and_sequence_bounds_fail_closed() {
    assert!(matches!(
        SensorServerImuAssembler::new(stream_id(), 0, 1, 0),
        Err(SensorServerError::InvalidEpoch)
    ));
    assert!(matches!(
        SensorServerImuAssembler::new(stream_id(), 1, MAX_PAIR_SKEW_NANOS + 1, 0),
        Err(SensorServerError::InvalidPairSkew)
    ));
    let mut exhausted = SensorServerImuAssembler::new(stream_id(), 1, 10, u64::MAX).unwrap();
    exhausted
        .ingest_json(SensorKind::Accelerometer, &reading(3, 100, [1.0; 3]), 1)
        .unwrap();
    assert_eq!(
        exhausted.ingest_json(SensorKind::Gyroscope, &reading(3, 101, [2.0; 3]), 2),
        Err(SensorServerError::SequenceExhausted)
    );
}

#[test]
fn profile_rejects_inconsistent_component_timestamp_metadata() {
    let mut assembler = SensorServerImuAssembler::new(stream_id(), 1, 10, 0).unwrap();
    assembler
        .ingest_json(SensorKind::Accelerometer, &reading(3, 100, [1.0; 3]), 1)
        .unwrap();
    let AssembleOutcome::Emitted { mut envelope, .. } = assembler
        .ingest_json(SensorKind::Gyroscope, &reading(3, 101, [2.0; 3]), 2)
        .unwrap()
    else {
        panic!("pair should emit")
    };
    envelope.payload.magnetic_field = Some([3.0; 3]);
    assert!(matches!(
        envelope.validate_for_profile(&ImuSampleV1::profile()),
        Err(DataPlaneError::InvalidPayload(_))
    ));
}
