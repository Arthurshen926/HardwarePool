#![forbid(unsafe_code)]

//! Bounded mapping from the documented external SensorServer JSON shape to the
//! CapyIO IMU Profile. This crate intentionally contains no WebSocket client.

use capyio_core::StreamId;
use capyio_data_plane::{
    DataEnvelope, ImuAccuracy, ImuCalibration, ImuComponentTimestampsV1, ImuCoordinateFrame,
    ImuSampleV1, ImuSensorMetadataV1, ImuUnitsV1,
};
use serde::Deserialize;
use thiserror::Error;

pub const MAX_SENSOR_SERVER_MESSAGE_BYTES: usize = 4096;
pub const MAX_PAIR_SKEW_NANOS: u64 = 1_000_000_000;
pub const SENSOR_SERVER_CLOCK_DOMAIN: &str = "android.sensor.elapsed_realtime";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SensorKind {
    Accelerometer,
    Gyroscope,
    MagneticField,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SensorServerReading {
    pub kind: SensorKind,
    pub accuracy: ImuAccuracy,
    pub timestamp_nanos: u64,
    pub values: [f64; 3],
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum SensorServerError {
    #[error("SensorServer message is {actual} bytes; maximum is {maximum}")]
    MessageTooLarge { actual: usize, maximum: usize },
    #[error("SensorServer message is empty")]
    EmptyMessage,
    #[error("SensorServer JSON is invalid: {0}")]
    InvalidJson(String),
    #[error("SensorServer values length is {actual}; expected exactly 3")]
    InvalidValueCount { actual: usize },
    #[error("SensorServer axes must contain only finite values")]
    NonFiniteValue,
    #[error("SensorServer timestamp must be positive")]
    InvalidTimestamp,
    #[error("unsupported Android sensor accuracy value: {0}")]
    UnsupportedAccuracy(i32),
    #[error("{kind:?} timestamp did not increase: previous={previous}, actual={actual}")]
    TimestampNotIncreasing {
        kind: SensorKind,
        previous: u64,
        actual: u64,
    },
    #[error("stream epoch must be positive")]
    InvalidEpoch,
    #[error("pair skew must be within 1..={MAX_PAIR_SKEW_NANOS} nanoseconds")]
    InvalidPairSkew,
    #[error("receive timestamp must be positive")]
    InvalidReceiveTimestamp,
    #[error("IMU output sequence is exhausted")]
    SequenceExhausted,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSensorServerReading {
    accuracy: i32,
    timestamp: u64,
    values: Vec<f64>,
}

pub fn parse_sensor_server_reading(
    kind: SensorKind,
    message: &[u8],
) -> Result<SensorServerReading, SensorServerError> {
    if message.is_empty() {
        return Err(SensorServerError::EmptyMessage);
    }
    if message.len() > MAX_SENSOR_SERVER_MESSAGE_BYTES {
        return Err(SensorServerError::MessageTooLarge {
            actual: message.len(),
            maximum: MAX_SENSOR_SERVER_MESSAGE_BYTES,
        });
    }
    let raw: RawSensorServerReading = serde_json::from_slice(message)
        .map_err(|error| SensorServerError::InvalidJson(error.to_string()))?;
    if raw.timestamp == 0 {
        return Err(SensorServerError::InvalidTimestamp);
    }
    let actual = raw.values.len();
    let values: [f64; 3] = raw
        .values
        .try_into()
        .map_err(|_: Vec<f64>| SensorServerError::InvalidValueCount { actual })?;
    if values.iter().any(|value| !value.is_finite()) {
        return Err(SensorServerError::NonFiniteValue);
    }
    Ok(SensorServerReading {
        kind,
        accuracy: map_accuracy(raw.accuracy)?,
        timestamp_nanos: raw.timestamp,
        values,
    })
}

fn map_accuracy(value: i32) -> Result<ImuAccuracy, SensorServerError> {
    match value {
        0 => Ok(ImuAccuracy::Unreliable),
        1 => Ok(ImuAccuracy::Low),
        2 => Ok(ImuAccuracy::Medium),
        3 => Ok(ImuAccuracy::High),
        value => Err(SensorServerError::UnsupportedAccuracy(value)),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplacedUnpairedReading {
    pub kind: SensorKind,
    pub timestamp_nanos: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AssembleOutcome {
    AwaitingCounterpart {
        missing: SensorKind,
        replaced: Option<ReplacedUnpairedReading>,
    },
    MagneticFieldUpdated,
    SkewExceeded {
        acceleration_timestamp_nanos: u64,
        gyroscope_timestamp_nanos: u64,
        maximum_nanos: u64,
        replaced: Option<ReplacedUnpairedReading>,
    },
    Emitted {
        envelope: Box<DataEnvelope<ImuSampleV1>>,
        replaced: Option<ReplacedUnpairedReading>,
    },
}

#[derive(Clone, Copy, Debug)]
struct PendingReading {
    reading: SensorServerReading,
    unpaired: bool,
}

#[derive(Clone, Debug)]
pub struct SensorServerImuAssembler {
    stream_id: StreamId,
    stream_epoch: u64,
    max_pair_skew_nanos: u64,
    next_sequence: u64,
    acceleration: Option<PendingReading>,
    gyroscope: Option<PendingReading>,
    magnetic_field: Option<PendingReading>,
}

impl SensorServerImuAssembler {
    pub fn new(
        stream_id: StreamId,
        stream_epoch: u64,
        max_pair_skew_nanos: u64,
        initial_sequence: u64,
    ) -> Result<Self, SensorServerError> {
        if stream_epoch == 0 {
            return Err(SensorServerError::InvalidEpoch);
        }
        if max_pair_skew_nanos == 0 || max_pair_skew_nanos > MAX_PAIR_SKEW_NANOS {
            return Err(SensorServerError::InvalidPairSkew);
        }
        Ok(Self {
            stream_id,
            stream_epoch,
            max_pair_skew_nanos,
            next_sequence: initial_sequence,
            acceleration: None,
            gyroscope: None,
            magnetic_field: None,
        })
    }

    pub fn ingest_json(
        &mut self,
        kind: SensorKind,
        message: &[u8],
        receive_timestamp_nanos: u64,
    ) -> Result<AssembleOutcome, SensorServerError> {
        let reading = parse_sensor_server_reading(kind, message)?;
        self.ingest(reading, receive_timestamp_nanos)
    }

    pub fn ingest(
        &mut self,
        reading: SensorServerReading,
        receive_timestamp_nanos: u64,
    ) -> Result<AssembleOutcome, SensorServerError> {
        if receive_timestamp_nanos == 0 {
            return Err(SensorServerError::InvalidReceiveTimestamp);
        }
        let replaced = self.store(reading)?;
        if reading.kind == SensorKind::MagneticField {
            return Ok(AssembleOutcome::MagneticFieldUpdated);
        }

        let Some(acceleration) = self.acceleration else {
            return Ok(AssembleOutcome::AwaitingCounterpart {
                missing: SensorKind::Accelerometer,
                replaced,
            });
        };
        let Some(gyroscope) = self.gyroscope else {
            return Ok(AssembleOutcome::AwaitingCounterpart {
                missing: SensorKind::Gyroscope,
                replaced,
            });
        };
        if !acceleration.unpaired || !gyroscope.unpaired {
            let missing = if !acceleration.unpaired {
                SensorKind::Accelerometer
            } else {
                SensorKind::Gyroscope
            };
            return Ok(AssembleOutcome::AwaitingCounterpart { missing, replaced });
        }

        let skew = acceleration
            .reading
            .timestamp_nanos
            .abs_diff(gyroscope.reading.timestamp_nanos);
        if skew > self.max_pair_skew_nanos {
            return Ok(AssembleOutcome::SkewExceeded {
                acceleration_timestamp_nanos: acceleration.reading.timestamp_nanos,
                gyroscope_timestamp_nanos: gyroscope.reading.timestamp_nanos,
                maximum_nanos: self.max_pair_skew_nanos,
                replaced,
            });
        }

        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(SensorServerError::SequenceExhausted)?;
        self.acceleration.as_mut().expect("checked above").unpaired = false;
        self.gyroscope.as_mut().expect("checked above").unpaired = false;

        let magnetic_field = self.magnetic_field.and_then(|pending| {
            (pending
                .reading
                .timestamp_nanos
                .abs_diff(acceleration.reading.timestamp_nanos)
                <= self.max_pair_skew_nanos
                && pending
                    .reading
                    .timestamp_nanos
                    .abs_diff(gyroscope.reading.timestamp_nanos)
                    <= self.max_pair_skew_nanos)
                .then_some(pending.reading)
        });
        let source_timestamp_nanos = magnetic_field
            .map(|reading| reading.timestamp_nanos)
            .into_iter()
            .chain([
                acceleration.reading.timestamp_nanos,
                gyroscope.reading.timestamp_nanos,
            ])
            .max()
            .expect("required timestamps are present");
        let accuracy = least_accuracy(acceleration.reading.accuracy, gyroscope.reading.accuracy);
        let envelope = DataEnvelope {
            profile: ImuSampleV1::profile(),
            stream_id: self.stream_id,
            stream_epoch: self.stream_epoch,
            sequence,
            source_timestamp_nanos,
            receive_timestamp_nanos,
            clock_domain_id: SENSOR_SERVER_CLOCK_DOMAIN.to_owned(),
            payload: ImuSampleV1 {
                acceleration: acceleration.reading.values,
                angular_velocity: gyroscope.reading.values,
                magnetic_field: magnetic_field.map(|reading| reading.values),
                units: ImuUnitsV1::default(),
                coordinate_frame: ImuCoordinateFrame::AndroidDeviceXRightYUpZOut,
                accuracy,
                calibration: ImuCalibration::Raw,
                sensor: ImuSensorMetadataV1 {
                    sensor_name: "SensorServer accelerometer + gyroscope".to_owned(),
                    vendor: "external SensorServer service".to_owned(),
                    version: 1,
                    android_sensor_type: None,
                },
                component_timestamps: Some(ImuComponentTimestampsV1 {
                    acceleration_nanos: acceleration.reading.timestamp_nanos,
                    angular_velocity_nanos: gyroscope.reading.timestamp_nanos,
                    magnetic_field_nanos: magnetic_field.map(|reading| reading.timestamp_nanos),
                }),
            },
        };
        Ok(AssembleOutcome::Emitted {
            envelope: Box::new(envelope),
            replaced,
        })
    }

    fn store(
        &mut self,
        reading: SensorServerReading,
    ) -> Result<Option<ReplacedUnpairedReading>, SensorServerError> {
        let slot = match reading.kind {
            SensorKind::Accelerometer => &mut self.acceleration,
            SensorKind::Gyroscope => &mut self.gyroscope,
            SensorKind::MagneticField => &mut self.magnetic_field,
        };
        if let Some(previous) = slot
            && reading.timestamp_nanos <= previous.reading.timestamp_nanos
        {
            return Err(SensorServerError::TimestampNotIncreasing {
                kind: reading.kind,
                previous: previous.reading.timestamp_nanos,
                actual: reading.timestamp_nanos,
            });
        }
        let replaced = slot.and_then(|previous| {
            (previous.unpaired && reading.kind != SensorKind::MagneticField).then_some(
                ReplacedUnpairedReading {
                    kind: reading.kind,
                    timestamp_nanos: previous.reading.timestamp_nanos,
                },
            )
        });
        *slot = Some(PendingReading {
            reading,
            unpaired: reading.kind != SensorKind::MagneticField,
        });
        Ok(replaced)
    }
}

fn least_accuracy(left: ImuAccuracy, right: ImuAccuracy) -> ImuAccuracy {
    use ImuAccuracy::{High, Low, Medium, Unreliable};
    match (left, right) {
        (Unreliable, _) | (_, Unreliable) => Unreliable,
        (Low, _) | (_, Low) => Low,
        (Medium, _) | (_, Medium) => Medium,
        (High, High) => High,
    }
}
