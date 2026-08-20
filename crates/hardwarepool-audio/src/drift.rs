/// One measured relationship between the source sample clock and receiver monotonic clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockDriftEstimate {
    /// Observed source samples per second over the measurement window.
    pub observed_source_rate_hz: f64,
    /// `observed_source_rate_hz / nominal_source_rate_hz`.
    pub rate_ratio: f64,
    /// Signed deviation from the nominal rate in parts per million.
    pub drift_ppm: f64,
}

/// Simple long-window estimator used to establish a deterministic baseline.
///
/// Production audio may add robust filtering and window rotation, but the semantic output stays the
/// same. Receiver timestamps must come from one monotonic clock domain.
#[derive(Clone, Copy, Debug)]
pub struct ClockDriftEstimator {
    nominal_source_rate_hz: u32,
    origin_source_sample: Option<u64>,
    origin_receiver_micros: Option<u64>,
}

impl ClockDriftEstimator {
    #[must_use]
    pub const fn new(nominal_source_rate_hz: u32) -> Self {
        Self {
            nominal_source_rate_hz,
            origin_source_sample: None,
            origin_receiver_micros: None,
        }
    }

    /// Records a correlated sample index and receiver time.
    ///
    /// The first observation establishes the origin. Non-increasing observations are ignored.
    pub fn observe(
        &mut self,
        source_sample_index: u64,
        receiver_micros: u64,
    ) -> Option<ClockDriftEstimate> {
        let (origin_sample, origin_time) =
            match (self.origin_source_sample, self.origin_receiver_micros) {
                (Some(sample), Some(time)) => (sample, time),
                _ => {
                    self.origin_source_sample = Some(source_sample_index);
                    self.origin_receiver_micros = Some(receiver_micros);
                    return None;
                }
            };

        let sample_delta = source_sample_index.checked_sub(origin_sample)?;
        let time_delta_micros = receiver_micros.checked_sub(origin_time)?;
        if sample_delta == 0 || time_delta_micros == 0 || self.nominal_source_rate_hz == 0 {
            return None;
        }

        let observed_source_rate_hz = sample_delta as f64 * 1_000_000.0 / time_delta_micros as f64;
        let rate_ratio = observed_source_rate_hz / f64::from(self.nominal_source_rate_hz);
        Some(ClockDriftEstimate {
            observed_source_rate_hz,
            rate_ratio,
            drift_ppm: (rate_ratio - 1.0) * 1_000_000.0,
        })
    }

    pub fn reset(&mut self) {
        self.origin_source_sample = None;
        self.origin_receiver_micros = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_positive_clock_drift() {
        let mut estimator = ClockDriftEstimator::new(48_000);
        assert!(estimator.observe(0, 1_000_000).is_none());
        let estimate = estimator
            .observe(48_005, 2_000_000)
            .expect("second observation");

        assert!((estimate.observed_source_rate_hz - 48_005.0).abs() < 0.001);
        assert!((estimate.drift_ppm - 104.166_666).abs() < 0.01);
    }

    #[test]
    fn non_increasing_observation_is_ignored() {
        let mut estimator = ClockDriftEstimator::new(48_000);
        let _origin = estimator.observe(100, 100);
        assert!(estimator.observe(99, 200).is_none());
        assert!(estimator.observe(200, 100).is_none());
    }
}
