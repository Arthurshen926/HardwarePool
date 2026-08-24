#![forbid(unsafe_code)]

//! Bounded semantic data-plane primitives for CapyIO StandardPort Routes.
//!
//! This crate does not open sockets, select a wire encoding, access hardware or
//! execute on real-time callbacks. Concrete transports validate and decode data
//! before constructing these envelopes.

use std::collections::{BTreeMap, VecDeque};

use capyio_core::{ProfileId, StreamId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const IMU_PROFILE_NAME: &str = "capyio.motion.imu-samples";
pub const IMU_PROFILE_MAJOR: u16 = 1;
pub const MAX_CLOCK_DOMAIN_BYTES: usize = 128;
pub const MAX_PAYLOAD_BYTES: usize = 16 * 1024;
pub const MAX_QUEUE_CAPACITY: usize = 4096;
pub const MAX_CONSUMERS: usize = 64;
pub const MAX_CONSUMER_ID_BYTES: usize = 64;
pub const MAX_SENSOR_TEXT_BYTES: usize = 128;
pub const MAX_RECORDER_LINE_BYTES: usize = 64 * 1024;
pub const MAX_RECORDER_RECORDS: usize = 1_000_000;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum DataPlaneError {
    #[error("capacity {capacity} is outside 1..={maximum}")]
    InvalidCapacity { capacity: usize, maximum: usize },
    #[error("clock domain must contain 1..={MAX_CLOCK_DOMAIN_BYTES} bytes")]
    InvalidClockDomain,
    #[error("stream epoch must be positive")]
    InvalidEpoch,
    #[error("unsupported profile: expected {expected}, received {actual}")]
    UnsupportedProfile { expected: String, actual: String },
    #[error("payload is invalid: {0}")]
    InvalidPayload(String),
    #[error("serialized payload is {actual} bytes; maximum is {maximum}")]
    PayloadTooLarge { actual: usize, maximum: usize },
    #[error("payload serialization failed: {0}")]
    Serialization(String),
    #[error("consumer ID must contain 1..={MAX_CONSUMER_ID_BYTES} bytes")]
    InvalidConsumerId,
    #[error("fan-out already contains the maximum of {MAX_CONSUMERS} consumers")]
    TooManyConsumers,
    #[error("consumer already exists: {0}")]
    DuplicateConsumer(String),
    #[error("consumer does not exist: {0}")]
    UnknownConsumer(String),
    #[error("new epoch {new_epoch} must be greater than current epoch {current_epoch}")]
    NonAdvancingEpoch { current_epoch: u64, new_epoch: u64 },
    #[error("recorder limit {limit} is outside 1..={maximum}")]
    InvalidRecorderLimit { limit: usize, maximum: usize },
    #[error("fixture record limit {limit} is outside 1..={maximum}")]
    InvalidFixtureLimit { limit: usize, maximum: usize },
    #[error("invalid IMU fixture line {line}: {reason}")]
    InvalidFixtureLine { line: usize, reason: String },
    #[error("IMU fixture contains more than the configured {maximum} records")]
    FixtureTooLarge { maximum: usize },
    #[error("IMU fixture contains no records")]
    EmptyFixture,
}

pub trait DataPayload: Clone + Serialize {
    fn validate_payload(&self) -> Result<(), DataPlaneError>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataEnvelope<T> {
    pub profile: ProfileId,
    pub stream_id: StreamId,
    pub stream_epoch: u64,
    pub sequence: u64,
    pub source_timestamp_nanos: u64,
    pub receive_timestamp_nanos: u64,
    pub clock_domain_id: String,
    pub payload: T,
}

impl<T: DataPayload> DataEnvelope<T> {
    pub fn validate_for_profile(&self, expected: &ProfileId) -> Result<(), DataPlaneError> {
        if self.profile != *expected {
            return Err(DataPlaneError::UnsupportedProfile {
                expected: profile_label(expected),
                actual: profile_label(&self.profile),
            });
        }
        if self.stream_epoch == 0 {
            return Err(DataPlaneError::InvalidEpoch);
        }
        let clock_len = self.clock_domain_id.len();
        if clock_len == 0 || clock_len > MAX_CLOCK_DOMAIN_BYTES {
            return Err(DataPlaneError::InvalidClockDomain);
        }
        self.payload.validate_payload()?;
        let payload_size = serde_json::to_vec(&self.payload)
            .map_err(|error| DataPlaneError::Serialization(error.to_string()))?
            .len();
        if payload_size > MAX_PAYLOAD_BYTES {
            return Err(DataPlaneError::PayloadTooLarge {
                actual: payload_size,
                maximum: MAX_PAYLOAD_BYTES,
            });
        }
        Ok(())
    }
}

fn profile_label(profile: &ProfileId) -> String {
    format!("{}/{}", profile.name, profile.major)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImuAccuracy {
    Unreliable,
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImuCoordinateFrame {
    AndroidDeviceXRightYUpZOut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImuCalibration {
    Raw,
    FactoryCalibrated,
    RuntimeCalibrated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImuUnitsV1 {
    pub acceleration: AccelerationUnit,
    pub angular_velocity: AngularVelocityUnit,
    pub magnetic_field: MagneticFieldUnit,
}

impl Default for ImuUnitsV1 {
    fn default() -> Self {
        Self {
            acceleration: AccelerationUnit::MetersPerSecondSquared,
            angular_velocity: AngularVelocityUnit::RadiansPerSecond,
            magnetic_field: MagneticFieldUnit::Microtesla,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccelerationUnit {
    MetersPerSecondSquared,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AngularVelocityUnit {
    RadiansPerSecond,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MagneticFieldUnit {
    Microtesla,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImuSensorMetadataV1 {
    pub sensor_name: String,
    pub vendor: String,
    pub version: u32,
    pub android_sensor_type: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImuComponentTimestampsV1 {
    pub acceleration_nanos: u64,
    pub angular_velocity_nanos: u64,
    pub magnetic_field_nanos: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImuSampleV1 {
    pub acceleration: [f64; 3],
    pub angular_velocity: [f64; 3],
    pub magnetic_field: Option<[f64; 3]>,
    pub units: ImuUnitsV1,
    pub coordinate_frame: ImuCoordinateFrame,
    pub accuracy: ImuAccuracy,
    pub calibration: ImuCalibration,
    pub sensor: ImuSensorMetadataV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_timestamps: Option<ImuComponentTimestampsV1>,
}

impl ImuSampleV1 {
    #[must_use]
    pub fn profile() -> ProfileId {
        ProfileId::new(IMU_PROFILE_NAME, IMU_PROFILE_MAJOR)
    }
}

impl DataPayload for ImuSampleV1 {
    fn validate_payload(&self) -> Result<(), DataPlaneError> {
        for value in self
            .acceleration
            .iter()
            .chain(self.angular_velocity.iter())
            .chain(self.magnetic_field.iter().flatten())
        {
            if !value.is_finite() {
                return Err(DataPlaneError::InvalidPayload(
                    "IMU axes must contain only finite values".to_owned(),
                ));
            }
        }
        for (label, value) in [
            ("sensor_name", &self.sensor.sensor_name),
            ("vendor", &self.sensor.vendor),
        ] {
            if value.is_empty() || value.len() > MAX_SENSOR_TEXT_BYTES {
                return Err(DataPlaneError::InvalidPayload(format!(
                    "{label} must contain 1..={MAX_SENSOR_TEXT_BYTES} bytes"
                )));
            }
        }
        if let Some(timestamps) = self.component_timestamps {
            if timestamps.acceleration_nanos == 0 || timestamps.angular_velocity_nanos == 0 {
                return Err(DataPlaneError::InvalidPayload(
                    "required IMU component timestamps must be positive".to_owned(),
                ));
            }
            if timestamps
                .magnetic_field_nanos
                .is_some_and(|value| value == 0)
            {
                return Err(DataPlaneError::InvalidPayload(
                    "magnetic-field component timestamp must be positive".to_owned(),
                ));
            }
            if self.magnetic_field.is_some() != timestamps.magnetic_field_nanos.is_some() {
                return Err(DataPlaneError::InvalidPayload(
                    "magnetic-field value and component timestamp must be present together"
                        .to_owned(),
                ));
            }
        }
        Ok(())
    }
}

/// Parses deterministic JSONL evidence for tests and demos.
///
/// This is not a public network wire decoder. Concrete transports need a
/// separately specified framing, authentication and replay policy.
pub fn parse_imu_fixture_jsonl(
    input: &str,
    max_records: usize,
) -> Result<Vec<DataEnvelope<ImuSampleV1>>, DataPlaneError> {
    if max_records == 0 || max_records > MAX_RECORDER_RECORDS {
        return Err(DataPlaneError::InvalidFixtureLimit {
            limit: max_records,
            maximum: MAX_RECORDER_RECORDS,
        });
    }
    let mut envelopes = Vec::with_capacity(max_records.min(1024));
    for (index, line) in input.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        if line.len() > MAX_RECORDER_LINE_BYTES {
            return Err(DataPlaneError::InvalidFixtureLine {
                line: index + 1,
                reason: format!("line exceeds {MAX_RECORDER_LINE_BYTES} bytes"),
            });
        }
        if envelopes.len() == max_records {
            return Err(DataPlaneError::FixtureTooLarge {
                maximum: max_records,
            });
        }
        let envelope: DataEnvelope<ImuSampleV1> =
            serde_json::from_str(line).map_err(|error| DataPlaneError::InvalidFixtureLine {
                line: index + 1,
                reason: error.to_string(),
            })?;
        envelope
            .validate_for_profile(&ImuSampleV1::profile())
            .map_err(|error| DataPlaneError::InvalidFixtureLine {
                line: index + 1,
                reason: error.to_string(),
            })?;
        envelopes.push(envelope);
    }
    if envelopes.is_empty() {
        return Err(DataPlaneError::EmptyFixture);
    }
    Ok(envelopes)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SequenceGap {
    pub first_missing: u64,
    pub last_missing: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Delivery<T> {
    pub gap_before: Option<SequenceGap>,
    pub envelope: DataEnvelope<T>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PushOutcome {
    Enqueued { gap: Option<SequenceGap> },
    Duplicate { sequence: u64 },
    Late { expected: u64, actual: u64 },
    WrongStream,
    StaleEpoch { current: u64, actual: u64 },
    FutureEpoch { current: u64, actual: u64 },
    Full { capacity: usize },
}

#[derive(Clone, Debug)]
pub struct BoundedEnvelopeQueue<T> {
    expected_profile: ProfileId,
    stream_id: StreamId,
    stream_epoch: u64,
    capacity: usize,
    last_sequence: Option<u64>,
    queue: VecDeque<Delivery<T>>,
}

impl<T: DataPayload> BoundedEnvelopeQueue<T> {
    pub fn new(
        expected_profile: ProfileId,
        stream_id: StreamId,
        stream_epoch: u64,
        capacity: usize,
    ) -> Result<Self, DataPlaneError> {
        if capacity == 0 || capacity > MAX_QUEUE_CAPACITY {
            return Err(DataPlaneError::InvalidCapacity {
                capacity,
                maximum: MAX_QUEUE_CAPACITY,
            });
        }
        if stream_epoch == 0 {
            return Err(DataPlaneError::InvalidEpoch);
        }
        Ok(Self {
            expected_profile,
            stream_id,
            stream_epoch,
            capacity,
            last_sequence: None,
            queue: VecDeque::with_capacity(capacity),
        })
    }

    pub fn push(&mut self, envelope: DataEnvelope<T>) -> Result<PushOutcome, DataPlaneError> {
        envelope.validate_for_profile(&self.expected_profile)?;
        if envelope.stream_id != self.stream_id {
            return Ok(PushOutcome::WrongStream);
        }
        if envelope.stream_epoch < self.stream_epoch {
            return Ok(PushOutcome::StaleEpoch {
                current: self.stream_epoch,
                actual: envelope.stream_epoch,
            });
        }
        if envelope.stream_epoch > self.stream_epoch {
            return Ok(PushOutcome::FutureEpoch {
                current: self.stream_epoch,
                actual: envelope.stream_epoch,
            });
        }
        if let Some(last) = self.last_sequence
            && envelope.sequence <= last
        {
            let duplicate = self
                .queue
                .iter()
                .any(|delivery| delivery.envelope.sequence == envelope.sequence);
            return Ok(if duplicate {
                PushOutcome::Duplicate {
                    sequence: envelope.sequence,
                }
            } else {
                PushOutcome::Late {
                    expected: last.saturating_add(1),
                    actual: envelope.sequence,
                }
            });
        }
        if self.queue.len() == self.capacity {
            return Ok(PushOutcome::Full {
                capacity: self.capacity,
            });
        }
        let gap = self.last_sequence.and_then(|last| {
            let expected = last.checked_add(1)?;
            (envelope.sequence > expected).then_some(SequenceGap {
                first_missing: expected,
                last_missing: envelope.sequence - 1,
            })
        });
        self.last_sequence = Some(envelope.sequence);
        self.queue.push_back(Delivery {
            gap_before: gap,
            envelope,
        });
        Ok(PushOutcome::Enqueued { gap })
    }

    pub fn advance_epoch(&mut self, new_epoch: u64) -> Result<usize, DataPlaneError> {
        if new_epoch <= self.stream_epoch {
            return Err(DataPlaneError::NonAdvancingEpoch {
                current_epoch: self.stream_epoch,
                new_epoch,
            });
        }
        let discarded = self.queue.len();
        self.queue.clear();
        self.stream_epoch = new_epoch;
        self.last_sequence = None;
        Ok(discarded)
    }

    pub fn reset_sequence(&mut self) -> usize {
        let discarded = self.queue.len();
        self.queue.clear();
        self.last_sequence = None;
        discarded
    }

    pub fn pop(&mut self) -> Option<Delivery<T>> {
        self.queue.pop_front()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.stream_epoch
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsumerPublishOutcome {
    Queue(PushOutcome),
    Stopped,
}

#[derive(Clone, Debug)]
struct FanoutConsumer<T> {
    queue: BoundedEnvelopeQueue<T>,
    active: bool,
    rejected_full: u64,
}

#[derive(Clone, Debug)]
pub struct BoundedFanout<T> {
    expected_profile: ProfileId,
    stream_id: StreamId,
    stream_epoch: u64,
    consumers: BTreeMap<String, FanoutConsumer<T>>,
}

impl<T: DataPayload> BoundedFanout<T> {
    #[must_use]
    pub fn new(expected_profile: ProfileId, stream_id: StreamId, stream_epoch: u64) -> Self {
        Self {
            expected_profile,
            stream_id,
            stream_epoch,
            consumers: BTreeMap::new(),
        }
    }

    pub fn register_consumer(
        &mut self,
        consumer_id: impl Into<String>,
        capacity: usize,
    ) -> Result<(), DataPlaneError> {
        let consumer_id = consumer_id.into();
        if consumer_id.is_empty() || consumer_id.len() > MAX_CONSUMER_ID_BYTES {
            return Err(DataPlaneError::InvalidConsumerId);
        }
        if self.consumers.contains_key(&consumer_id) {
            return Err(DataPlaneError::DuplicateConsumer(consumer_id));
        }
        if self.consumers.len() == MAX_CONSUMERS {
            return Err(DataPlaneError::TooManyConsumers);
        }
        let queue = BoundedEnvelopeQueue::new(
            self.expected_profile.clone(),
            self.stream_id,
            self.stream_epoch,
            capacity,
        )?;
        self.consumers.insert(
            consumer_id,
            FanoutConsumer {
                queue,
                active: true,
                rejected_full: 0,
            },
        );
        Ok(())
    }

    pub fn publish(
        &mut self,
        envelope: DataEnvelope<T>,
    ) -> BTreeMap<String, Result<ConsumerPublishOutcome, DataPlaneError>> {
        self.consumers
            .iter_mut()
            .map(|(id, consumer)| {
                let result = if consumer.active {
                    consumer.queue.push(envelope.clone()).map(|outcome| {
                        if matches!(outcome, PushOutcome::Full { .. }) {
                            consumer.rejected_full = consumer.rejected_full.saturating_add(1);
                        }
                        ConsumerPublishOutcome::Queue(outcome)
                    })
                } else {
                    Ok(ConsumerPublishOutcome::Stopped)
                };
                (id.clone(), result)
            })
            .collect()
    }

    pub fn pop(&mut self, consumer_id: &str) -> Result<Option<Delivery<T>>, DataPlaneError> {
        self.consumers
            .get_mut(consumer_id)
            .map(|consumer| consumer.queue.pop())
            .ok_or_else(|| DataPlaneError::UnknownConsumer(consumer_id.to_owned()))
    }

    pub fn stop_consumer(&mut self, consumer_id: &str) -> Result<usize, DataPlaneError> {
        let consumer = self
            .consumers
            .get_mut(consumer_id)
            .ok_or_else(|| DataPlaneError::UnknownConsumer(consumer_id.to_owned()))?;
        consumer.active = false;
        Ok(consumer.queue.reset_sequence())
    }

    pub fn start_consumer(&mut self, consumer_id: &str) -> Result<(), DataPlaneError> {
        let consumer = self
            .consumers
            .get_mut(consumer_id)
            .ok_or_else(|| DataPlaneError::UnknownConsumer(consumer_id.to_owned()))?;
        consumer.queue.reset_sequence();
        consumer.active = true;
        Ok(())
    }

    pub fn advance_epoch(&mut self, new_epoch: u64) -> Result<usize, DataPlaneError> {
        if new_epoch <= self.stream_epoch {
            return Err(DataPlaneError::NonAdvancingEpoch {
                current_epoch: self.stream_epoch,
                new_epoch,
            });
        }
        let mut discarded = 0usize;
        for consumer in self.consumers.values_mut() {
            discarded = discarded.saturating_add(consumer.queue.advance_epoch(new_epoch)?);
        }
        self.stream_epoch = new_epoch;
        Ok(discarded)
    }

    pub fn rejected_full(&self, consumer_id: &str) -> Result<u64, DataPlaneError> {
        self.consumers
            .get(consumer_id)
            .map(|consumer| consumer.rejected_full)
            .ok_or_else(|| DataPlaneError::UnknownConsumer(consumer_id.to_owned()))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NumericImuPanel {
    pub last_sample: Option<ImuSampleV1>,
    pub received: u64,
    pub missing_sequences: u64,
}

impl NumericImuPanel {
    pub fn consume(&mut self, delivery: Delivery<ImuSampleV1>) {
        if let Some(gap) = delivery.gap_before {
            self.missing_sequences = self.missing_sequences.saturating_add(
                gap.last_missing
                    .saturating_sub(gap.first_missing)
                    .saturating_add(1),
            );
        }
        self.received = self.received.saturating_add(1);
        self.last_sample = Some(delivery.envelope.payload);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecorderOutcome {
    Recorded,
    Full,
    LineTooLarge { actual: usize, maximum: usize },
}

#[derive(Clone, Debug)]
pub struct BoundedJsonlRecorder {
    max_records: usize,
    max_line_bytes: usize,
    lines: Vec<String>,
}

impl BoundedJsonlRecorder {
    pub fn new(max_records: usize, max_line_bytes: usize) -> Result<Self, DataPlaneError> {
        if max_records == 0 || max_records > MAX_RECORDER_RECORDS {
            return Err(DataPlaneError::InvalidRecorderLimit {
                limit: max_records,
                maximum: MAX_RECORDER_RECORDS,
            });
        }
        if max_line_bytes == 0 || max_line_bytes > MAX_RECORDER_LINE_BYTES {
            return Err(DataPlaneError::InvalidRecorderLimit {
                limit: max_line_bytes,
                maximum: MAX_RECORDER_LINE_BYTES,
            });
        }
        Ok(Self {
            max_records,
            max_line_bytes,
            lines: Vec::with_capacity(max_records.min(1024)),
        })
    }

    pub fn record<T: Serialize>(
        &mut self,
        delivery: &Delivery<T>,
    ) -> Result<RecorderOutcome, DataPlaneError> {
        if self.lines.len() == self.max_records {
            return Ok(RecorderOutcome::Full);
        }
        let line = serde_json::to_string(delivery)
            .map_err(|error| DataPlaneError::Serialization(error.to_string()))?;
        if line.len() > self.max_line_bytes {
            return Ok(RecorderOutcome::LineTooLarge {
                actual: line.len(),
                maximum: self.max_line_bytes,
            });
        }
        self.lines.push(line);
        Ok(RecorderOutcome::Recorded)
    }

    #[must_use]
    pub fn as_jsonl(&self) -> String {
        if self.lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", self.lines.join("\n"))
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}
