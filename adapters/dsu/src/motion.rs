use std::error::Error;
use std::fmt::{self, Display, Formatter};

use capyio_data_plane::{DataEnvelope, DataPlaneError, ImuSampleV1};

/// Exact standard-gravity conversion used for DSU acceleration fields.
pub const STANDARD_GRAVITY_METERS_PER_SECOND_SQUARED: f64 = 9.806_65;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceAxis {
    X,
    Y,
    Z,
}

impl SourceAxis {
    const fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }

    const fn bit(self) -> u8 {
        1 << self.index()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AxisSign {
    Positive,
    Negative,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignedSourceAxis {
    axis: SourceAxis,
    sign: AxisSign,
}

impl SignedSourceAxis {
    #[must_use]
    pub const fn positive(axis: SourceAxis) -> Self {
        Self {
            axis,
            sign: AxisSign::Positive,
        }
    }

    #[must_use]
    pub const fn negative(axis: SourceAxis) -> Self {
        Self {
            axis,
            sign: AxisSign::Negative,
        }
    }

    fn apply(self, values: [f64; 3]) -> f64 {
        let value = values[self.axis.index()];
        match self.sign {
            AxisSign::Positive => value,
            AxisSign::Negative => -value,
        }
    }
}

/// A validated, signed permutation from source X/Y/Z to three DSU fields.
///
/// For acceleration the outputs are DSU X/Y/Z. For angular velocity they are
/// DSU pitch/yaw/roll. Requiring a permutation prevents accidental axis loss or
/// duplication while still making mounting-specific sign changes explicit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AxisPermutation {
    outputs: [SignedSourceAxis; 3],
}

impl AxisPermutation {
    pub fn new(
        output_0: SignedSourceAxis,
        output_1: SignedSourceAxis,
        output_2: SignedSourceAxis,
    ) -> Result<Self, MotionProjectionError> {
        let outputs = [output_0, output_1, output_2];
        let mut seen = 0_u8;
        for output in outputs {
            if seen & output.axis.bit() != 0 {
                return Err(MotionProjectionError::DuplicateSourceAxis(output.axis));
            }
            seen |= output.axis.bit();
        }
        Ok(Self { outputs })
    }

    #[must_use]
    pub const fn identity() -> Self {
        Self {
            outputs: [
                SignedSourceAxis::positive(SourceAxis::X),
                SignedSourceAxis::positive(SourceAxis::Y),
                SignedSourceAxis::positive(SourceAxis::Z),
            ],
        }
    }

    fn apply(self, values: [f64; 3]) -> [f64; 3] {
        [
            self.outputs[0].apply(values),
            self.outputs[1].apply(values),
            self.outputs[2].apply(values),
        ]
    }
}

/// Explicit route-level coordinate mapping into DSU motion fields.
///
/// `identity()` is useful for deterministic fixtures. It is not a claim that
/// every physical phone mounting has the same orientation as an emulator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DsuMotionMapping {
    acceleration: AxisPermutation,
    angular_velocity: AxisPermutation,
}

impl DsuMotionMapping {
    #[must_use]
    pub const fn new(acceleration: AxisPermutation, angular_velocity: AxisPermutation) -> Self {
        Self {
            acceleration,
            angular_velocity,
        }
    }

    #[must_use]
    pub const fn identity() -> Self {
        Self::new(AxisPermutation::identity(), AxisPermutation::identity())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DsuMotionSample {
    timestamp_micros: u64,
    acceleration_g: [f32; 3],
    gyroscope_degrees_per_second: [f32; 3],
}

impl DsuMotionSample {
    #[must_use]
    pub const fn timestamp_micros(self) -> u64 {
        self.timestamp_micros
    }

    #[must_use]
    pub const fn acceleration_g(self) -> [f32; 3] {
        self.acceleration_g
    }

    #[must_use]
    pub const fn gyroscope_degrees_per_second(self) -> [f32; 3] {
        self.gyroscope_degrees_per_second
    }
}

#[derive(Debug)]
pub enum MotionProjectionError {
    InvalidEnvelope(DataPlaneError),
    DuplicateSourceAxis(SourceAxis),
    DsuFloatOutOfRange,
}

impl Display for MotionProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEnvelope(error) => write!(formatter, "invalid IMU envelope: {error}"),
            Self::DuplicateSourceAxis(axis) => {
                write!(formatter, "source axis {axis:?} appears more than once")
            }
            Self::DsuFloatOutOfRange => {
                formatter.write_str("projected DSU motion value is outside finite f32 range")
            }
        }
    }
}

impl Error for MotionProjectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidEnvelope(error) => Some(error),
            Self::DuplicateSourceAxis(_) | Self::DsuFloatOutOfRange => None,
        }
    }
}

impl From<DataPlaneError> for MotionProjectionError {
    fn from(error: DataPlaneError) -> Self {
        Self::InvalidEnvelope(error)
    }
}

/// Validates and projects one canonical IMU envelope without retaining state.
///
/// Sequence, epoch and queue policy remain with the route's bounded data-plane
/// consumer. The acceleration component timestamp is used when present because
/// DSU defines its motion timestamp in terms of accelerometer updates.
pub fn project_imu_envelope(
    envelope: &DataEnvelope<ImuSampleV1>,
    mapping: DsuMotionMapping,
) -> Result<DsuMotionSample, MotionProjectionError> {
    // This canonical public helper avoids duplicating a profile string in the
    // Adapter. The shared contract can change its internal construction without
    // changing this boundary.
    envelope.validate_for_profile(&ImuSampleV1::profile())?;

    let acceleration = mapping.acceleration.apply(envelope.payload.acceleration);
    let angular_velocity = mapping
        .angular_velocity
        .apply(envelope.payload.angular_velocity);

    let acceleration_g = finite_f32_array(
        acceleration.map(|value| value / STANDARD_GRAVITY_METERS_PER_SECOND_SQUARED),
    )?;
    let gyroscope_degrees_per_second = finite_f32_array(angular_velocity.map(f64::to_degrees))?;
    let timestamp_nanos = envelope
        .payload
        .component_timestamps
        .map_or(envelope.source_timestamp_nanos, |timestamps| {
            timestamps.acceleration_nanos
        });

    Ok(DsuMotionSample {
        timestamp_micros: timestamp_nanos / 1_000,
        acceleration_g,
        gyroscope_degrees_per_second,
    })
}

fn finite_f32_array(values: [f64; 3]) -> Result<[f32; 3], MotionProjectionError> {
    let projected = values.map(|value| value as f32);
    if projected.iter().all(|value| value.is_finite()) {
        Ok(projected)
    } else {
        Err(MotionProjectionError::DsuFloatOutOfRange)
    }
}

#[cfg(test)]
mod tests {
    use super::{AxisPermutation, MotionProjectionError, SignedSourceAxis, SourceAxis};

    #[test]
    fn axis_mapping_rejects_duplicate_source_axes() {
        assert!(matches!(
            AxisPermutation::new(
                SignedSourceAxis::positive(SourceAxis::X),
                SignedSourceAxis::negative(SourceAxis::X),
                SignedSourceAxis::positive(SourceAxis::Z),
            ),
            Err(MotionProjectionError::DuplicateSourceAxis(SourceAxis::X))
        ));
    }
}
