//! Portable sender admission only: no decoder, capture device or Linux dependency.
use capyio_core::FormatDescriptor;
use capyio_vcamdroid_adapter::{
    AVC_WIRE_HEADER_BYTES, AVC_WIRE_MAX_ACCESS_UNIT_BYTES, AvcLayout, AvcRecord, AvcRecordGuard,
    AvcStreamKey, decode_record,
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CameraSenderProfile {
    Legacy,
    Finite720p30V1,
}
impl CameraSenderProfile {
    pub(crate) fn from_selection(selected: Option<&FormatDescriptor>) -> Result<Self, String> {
        for profile in [Self::Legacy, Self::Finite720p30V1] {
            if profile.matches_selection(selected) {
                return Ok(profile);
            }
        }
        Err("unsupported exact camera sender format".into())
    }
    pub(crate) fn matches_selection(self, selected: Option<&FormatDescriptor>) -> bool {
        selected == Some(&FormatDescriptor::new(self.format_name()))
    }
    /// A parameter mutation cannot turn the secure-only finite identifier into lab plaintext.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn requires_secure_transport(selected: Option<&FormatDescriptor>) -> bool {
        selected.is_some_and(|format| format.id == Self::Finite720p30V1.format_name())
    }
    pub(crate) fn format_name(self) -> &'static str {
        match self {
            Self::Legacy => "cavc-avc-access-unit-v1",
            Self::Finite720p30V1 => "cavc-avc-finite-720p30-v1",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WorkBudget {
    max_frames: usize,
    max_bytes: usize,
}
impl WorkBudget {
    fn new(remaining_ms: u64) -> Result<Self, String> {
        if !(1..=60_000).contains(&remaining_ms) {
            return Err("finite camera lifetime must be 1..60000 milliseconds".into());
        }
        Ok(Self {
            max_frames: ((remaining_ms * 30).div_ceil(1000)) as usize,
            max_bytes: ((2 * 1024 * 1024 * remaining_ms / 1000 + 4 * 1024 * 1024)
                .min(32 * 1024 * 1024)) as usize,
        })
    }
}
const SCALE: u64 = 1_000_000_000;
#[derive(Clone)]
struct Bucket {
    tokens: u64,
    capacity: u64,
    rate: u64,
    last: Instant,
}
impl Bucket {
    fn new(rate: u64, burst: u64, now: Instant) -> Self {
        Self {
            tokens: burst * SCALE,
            capacity: burst * SCALE,
            rate,
            last: now,
        }
    }
    fn charge(&mut self, count: usize, now: Instant) -> Result<(), String> {
        let elapsed = now
            .checked_duration_since(self.last)
            .ok_or("camera monotonic clock regressed")?;
        let refill = elapsed
            .as_nanos()
            .checked_mul(u128::from(self.rate))
            .ok_or("camera rate overflow")?;
        let available = (u128::from(self.tokens) + refill).min(u128::from(self.capacity));
        let cost = (count as u128)
            .checked_mul(u128::from(SCALE))
            .ok_or("camera rate cost overflow")?;
        self.tokens = u64::try_from(
            available
                .checked_sub(cost)
                .ok_or("finite camera arrival rate exceeded")?,
        )
        .map_err(|_| "camera token overflow")?;
        self.last = now;
        Ok(())
    }
}
#[derive(Clone)]
struct FiniteAdmission {
    budget: WorkBudget,
    frames: usize,
    bytes: usize,
    frame_rate: Bucket,
    byte_rate: Bucket,
}
/// Cloning preparation preserves its original clock and budget. A failed record
/// consumes no work counters, but latches terminal failure (never a retry reset).
#[derive(Clone)]
pub(super) struct Admission {
    guard: AvcRecordGuard,
    finite: Option<FiniteAdmission>,
    failed: bool,
}
impl Admission {
    pub(super) fn new(
        key: AvcStreamKey,
        profile: CameraSenderProfile,
        remaining_ms: u64,
        now: Instant,
    ) -> Result<Self, String> {
        Ok(Self {
            guard: AvcRecordGuard::new(key).map_err(|e| e.to_string())?,
            finite: match profile {
                CameraSenderProfile::Legacy => None,
                CameraSenderProfile::Finite720p30V1 => Some(FiniteAdmission {
                    budget: WorkBudget::new(remaining_ms)?,
                    frames: 0,
                    bytes: 0,
                    frame_rate: Bucket::new(30, 3, now),
                    byte_rate: Bucket::new(2 * 1024 * 1024, 4 * 1024 * 1024, now),
                }),
            },
            failed: false,
        })
    }
    pub(super) fn ended(&self) -> bool {
        self.guard.ended()
    }
    pub(super) fn accept(&mut self, bytes: &[u8], now: Instant) -> Result<(), String> {
        if self.failed {
            return Err("camera sender admission is terminal".into());
        }
        let result = self.try_accept(bytes, now);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn try_accept(&mut self, bytes: &[u8], now: Instant) -> Result<(), String> {
        // Reject excessive work and rate before the portable parser allocates.
        if bytes.len() > AVC_WIRE_HEADER_BYTES + AVC_WIRE_MAX_ACCESS_UNIT_BYTES {
            return Err("camera record allocation bound exceeded".into());
        }
        let mut finite = self.finite.clone();
        if let Some(f) = &mut finite {
            f.bytes = f
                .bytes
                .checked_add(bytes.len())
                .filter(|n| *n <= f.budget.max_bytes)
                .ok_or("finite complete CAVC byte budget exceeded")?;
            let is_frame = bytes.get(6) == Some(&2) && bytes.get(7).is_some_and(|v| v & 2 == 0);
            f.frames = f
                .frames
                .checked_add(usize::from(is_frame))
                .filter(|n| *n <= f.budget.max_frames)
                .ok_or("finite camera frame budget exceeded")?;
            f.byte_rate.charge(bytes.len(), now)?;
            f.frame_rate.charge(usize::from(is_frame), now)?;
            if bytes.get(6) == Some(&1) && bytes.len() > 84 + 65536 {
                return Err("finite camera CSD allocation bound exceeded".into());
            }
        }
        let record = decode_record(bytes).map_err(|e| e.to_string())?;
        let mut guard = self.guard.clone();
        guard.accept(&record).map_err(|e| e.to_string())?;
        if finite.is_some()
            && let AvcRecord::Config { config: c, .. } = &record
            && (c.width != 1280
                || c.height != 720
                || c.frames_per_second != 30
                || c.clockwise_rotation_degrees != 0
                || c.access_unit_layout != AvcLayout::AnnexB
                || c.codec_specific_layout != AvcLayout::AnnexB
                || c.csd0
                    .len()
                    .checked_add(c.csd1.len())
                    .is_none_or(|n| n > 65536))
        {
            return Err(
                "finite camera requires canonical 720p30 rotation-zero Annex B config".into(),
            );
        }
        if finite.is_some()
            && let AvcRecord::AccessUnit(unit) = &record
            && unit
                .presentation_time_us
                .checked_mul(1000)
                .is_none_or(|n| n > i64::MAX as u64)
        {
            return Err("finite camera PTS nanoseconds overflow".into());
        }
        self.guard = guard;
        self.finite = finite;
        Ok(())
    }
}

/// Exact independent wall and monotonic authority, anchored before handshake.
#[derive(Clone, Debug)]
pub(super) struct AuthorityClock {
    pub(super) deadline: Instant,
    last_wall: u64,
    last_instant: Instant,
}
impl AuthorityClock {
    pub(super) fn new(now: u64, lease_deadline: u64, instant: Instant) -> Result<Self, String> {
        let remaining = lease_deadline
            .checked_sub(now)
            .filter(|n| *n > 0)
            .ok_or("Source lease expired")?;
        Ok(Self {
            deadline: instant
                .checked_add(Duration::from_millis(remaining))
                .ok_or("camera deadline overflow")?,
            last_wall: now,
            last_instant: instant,
        })
    }
    pub(super) fn check(
        &mut self,
        wall: u64,
        lease_deadline: u64,
        instant: Instant,
    ) -> Result<(), String> {
        if wall < self.last_wall
            || wall >= lease_deadline
            || instant < self.last_instant
            || instant >= self.deadline
        {
            return Err("camera Source lease expired or clock regressed".into());
        }
        self.last_wall = wall;
        self.last_instant = instant;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_vcamdroid_adapter::{AvcAccessUnit, AvcConfig, encode_access_unit, encode_config};
    fn key() -> AvcStreamKey {
        AvcStreamKey {
            stream_id: [7; 16],
            epoch: 1,
        }
    }
    fn config() -> AvcConfig {
        AvcConfig {
            width: 1280,
            height: 720,
            frames_per_second: 30,
            bitrate_bits_per_second: 4_000_000,
            clockwise_rotation_degrees: 0,
            access_unit_layout: AvcLayout::AnnexB,
            codec_specific_layout: AvcLayout::AnnexB,
            csd0: vec![0, 0, 0, 1, 0x67],
            csd1: vec![0, 0, 0, 1, 0x68],
        }
    }
    fn unit(n: u64) -> AvcAccessUnit {
        AvcAccessUnit {
            stream: key(),
            sequence: n,
            presentation_time_us: n,
            key_frame: true,
            end_of_stream: false,
            discontinuity: false,
            payload: vec![0, 0, 0, 1, 0x65, 0x88],
        }
    }
    fn ready(profile: CameraSenderProfile, millis: u64, now: Instant) -> Admission {
        let mut a = Admission::new(key(), profile, millis, now).unwrap();
        a.accept(&encode_config(key(), &config()).unwrap(), now)
            .unwrap();
        a
    }
    #[test]
    fn exact_profiles_reject_parameters_aliases_and_mutation() {
        let mut finite = FormatDescriptor::new(CameraSenderProfile::Finite720p30V1.format_name());
        assert!(CameraSenderProfile::requires_secure_transport(Some(
            &finite
        )));
        finite
            .parameters
            .insert("unexpected".into(), "value".into());
        assert!(CameraSenderProfile::requires_secure_transport(Some(
            &finite
        )));
        assert!(!CameraSenderProfile::requires_secure_transport(None));
        assert!(!CameraSenderProfile::requires_secure_transport(Some(
            &FormatDescriptor::new(CameraSenderProfile::Legacy.format_name(),)
        )));

        for profile in [
            CameraSenderProfile::Legacy,
            CameraSenderProfile::Finite720p30V1,
        ] {
            let mut selected = FormatDescriptor::new(profile.format_name());
            assert_eq!(
                CameraSenderProfile::from_selection(Some(&selected)).unwrap(),
                profile
            );
            selected.parameters.insert("width".into(), "1280".into());
            assert!(!profile.matches_selection(Some(&selected)));
            assert!(CameraSenderProfile::from_selection(Some(&selected)).is_err());
        }
        assert!(!CameraSenderProfile::Finite720p30V1.matches_selection(Some(
            &FormatDescriptor::new(CameraSenderProfile::Legacy.format_name())
        )));
        for id in [
            "",
            "CAVC-avc-finite-720p30-v1",
            "cavc-avc-finite-720p30-v1 ",
        ] {
            assert!(CameraSenderProfile::from_selection(Some(&FormatDescriptor::new(id))).is_err());
        }
        assert!(CameraSenderProfile::from_selection(None).is_err());
    }
    #[test]
    fn immutable_work_ceiling_boundaries() {
        for ms in [0, 60_001, u64::MAX] {
            assert!(WorkBudget::new(ms).is_err());
        }
        for (ms, frames) in [(1, 1), (33, 1), (34, 2), (1000, 30), (60_000, 1800)] {
            let b = WorkBudget::new(ms).unwrap();
            assert_eq!(b.max_frames, frames);
            assert_eq!(
                b.max_bytes,
                ((2 * 1024 * 1024 * ms / 1000 + 4 * 1024 * 1024).min(32 * 1024 * 1024)) as usize
            );
        }
    }
    #[test]
    fn finite_canonical_config_is_independent_and_terminal() {
        for c in [
            AvcConfig {
                width: 640,
                ..config()
            },
            AvcConfig {
                height: 480,
                ..config()
            },
            AvcConfig {
                frames_per_second: 60,
                ..config()
            },
            AvcConfig {
                clockwise_rotation_degrees: 90,
                ..config()
            },
            AvcConfig {
                access_unit_layout: AvcLayout::LengthPrefixed4,
                ..config()
            },
            AvcConfig {
                codec_specific_layout: AvcLayout::AvcDecoderConfigurationRecord,
                ..config()
            },
        ] {
            let now = Instant::now();
            let mut a =
                Admission::new(key(), CameraSenderProfile::Finite720p30V1, 1000, now).unwrap();
            assert!(a.accept(&encode_config(key(), &c).unwrap(), now).is_err());
            assert!(!a.guard.configured());
            assert_eq!(a.finite.as_ref().unwrap().bytes, 0);
            assert!(
                a.accept(&encode_config(key(), &config()).unwrap(), now)
                    .is_err()
            );
        }
    }
    #[test]
    fn finite_pts_must_fit_receiver_nanoseconds_without_reset() {
        let now = Instant::now();
        let mut a = ready(CameraSenderProfile::Finite720p30V1, 5000, now);
        let mut u = unit(1);
        u.presentation_time_us = i64::MAX as u64 / 1000 + 1;
        assert_eq!(
            a.accept(&encode_access_unit(&u).unwrap(), now).unwrap_err(),
            "finite camera PTS nanoseconds overflow"
        );
        assert_eq!(a.finite.as_ref().unwrap().frames, 0);
        assert!(
            a.accept(&encode_access_unit(&unit(1)).unwrap(), now)
                .is_err()
        );
    }

    #[test]
    fn legacy_remains_noncanonical_and_without_new_work_or_rate_cap() {
        let now = Instant::now();
        let mut a = Admission::new(key(), CameraSenderProfile::Legacy, 100_000, now).unwrap();
        let c = AvcConfig {
            width: 640,
            height: 480,
            frames_per_second: 60,
            ..config()
        };
        a.accept(&encode_config(key(), &c).unwrap(), now).unwrap();
        for n in 1..=1801 {
            a.accept(&encode_access_unit(&unit(n)).unwrap(), now)
                .unwrap();
        }
        assert!(a.finite.is_none());
    }
    #[test]
    fn finite_paced_frames_cross_120_and_stop_at_original_maximum() {
        let now = Instant::now();
        let mut a = ready(CameraSenderProfile::Finite720p30V1, 5000, now);
        for n in 1..=150 {
            a.accept(
                &encode_access_unit(&unit(n)).unwrap(),
                now + Duration::from_nanos((n - 1) * 33_333_334),
            )
            .unwrap();
        }
        assert!(
            a.accept(
                &encode_access_unit(&unit(151)).unwrap(),
                now + Duration::from_secs(6)
            )
            .is_err()
        );
        assert_eq!(a.finite.as_ref().unwrap().frames, 150);
    }
    #[test]
    fn rate_never_uses_pts_discontinuity_or_retry_to_refill() {
        let now = Instant::now();
        let mut a = ready(CameraSenderProfile::Finite720p30V1, 60_000, now);
        for n in 1..=3 {
            a.accept(&encode_access_unit(&unit(n)).unwrap(), now)
                .unwrap();
        }
        let u = AvcAccessUnit {
            sequence: 999,
            presentation_time_us: 9_000_000_000,
            discontinuity: true,
            ..unit(4)
        };
        assert!(a.accept(&encode_access_unit(&u).unwrap(), now).is_err());
        assert_eq!(a.finite.as_ref().unwrap().frames, 3);
        assert_eq!(a.guard.last_sequence(), Some(3));
        assert!(
            a.accept(
                &encode_access_unit(&u).unwrap(),
                now + Duration::from_secs(1)
            )
            .is_err()
        );
        // Nanosecond boundary tested on separate unfailed preparations.
        for (elapsed, ok) in [(33_333_333, false), (33_333_334, true)] {
            let mut a = ready(CameraSenderProfile::Finite720p30V1, 1000, now);
            for n in 1..=3 {
                a.accept(&encode_access_unit(&unit(n)).unwrap(), now)
                    .unwrap();
            }
            assert_eq!(
                a.accept(
                    &encode_access_unit(&unit(4)).unwrap(),
                    now + Duration::from_nanos(elapsed)
                )
                .is_ok(),
                ok
            );
        }
    }
    #[test]
    fn complete_record_bytes_and_eos_are_charged() {
        let now = Instant::now();
        let mut a = ready(CameraSenderProfile::Finite720p30V1, 60_000, now);
        let config_bytes = a.finite.as_ref().unwrap().bytes;
        assert_eq!(config_bytes, 94);
        let mut u = unit(1);
        u.payload.resize(4 * 1024 * 1024 - config_bytes - 56, 0);
        a.accept(&encode_access_unit(&u).unwrap(), now).unwrap();
        assert_eq!(a.finite.as_ref().unwrap().bytes, 4 * 1024 * 1024);
        let eos = AvcAccessUnit {
            end_of_stream: true,
            key_frame: false,
            payload: vec![],
            ..unit(2)
        };
        let mut fails = a.clone();
        assert!(
            fails
                .accept(&encode_access_unit(&eos).unwrap(), now)
                .is_err()
        );
        a.accept(
            &encode_access_unit(&eos).unwrap(),
            now + Duration::from_millis(1),
        )
        .unwrap();
        assert_eq!(a.finite.as_ref().unwrap().frames, 1);
        assert_eq!(a.finite.as_ref().unwrap().bytes, 4 * 1024 * 1024 + 56);
        assert!(a.ended());
        assert!(
            a.accept(
                &encode_access_unit(&unit(3)).unwrap(),
                now + Duration::from_secs(1)
            )
            .is_err()
        );
    }
    #[test]
    fn cumulative_bytes_and_malformed_records_fail_transactionally() {
        let now = Instant::now();
        let ready = ready(CameraSenderProfile::Finite720p30V1, 1000, now);
        let bytes = encode_access_unit(&unit(1)).unwrap();
        for len in 0..bytes.len() {
            let mut a = ready.clone();
            assert!(a.accept(&bytes[..len], now).is_err());
            assert_eq!(a.guard.last_sequence(), None);
        }
        for used in [ready.finite.as_ref().unwrap().budget.max_bytes, usize::MAX] {
            let mut a = ready.clone();
            a.finite.as_mut().unwrap().bytes = used;
            assert!(a.accept(&bytes, now).is_err());
            assert_eq!(a.finite.as_ref().unwrap().frames, 0);
        }
        let mut a = ready.clone();
        let mut wrong = unit(1);
        wrong.stream.epoch = 2;
        assert!(a.accept(&encode_access_unit(&wrong).unwrap(), now).is_err());
        let mut a = ready.clone();
        a.accept(&bytes, now).unwrap();
        assert!(a.accept(&bytes, now + Duration::from_secs(1)).is_err());
    }
    #[test]
    fn preparation_clock_clone_never_refreshes_or_ceil_extends_authority() {
        let now = Instant::now();
        let original = AuthorityClock::new(1000, 1001, now).unwrap();
        let mut clone = original.clone();
        assert_eq!(clone.deadline, original.deadline);
        clone
            .check(1000, 1001, now + Duration::from_micros(999))
            .unwrap();
        assert!(
            clone
                .check(1000, 1001, now + Duration::from_millis(1))
                .is_err()
        );
        assert!(original.clone().check(1001, 1001, now).is_err());
        assert!(original.clone().check(999, 1001, now).is_err());
        assert!(
            original
                .clone()
                .check(1000, 1001, now - Duration::from_nanos(1))
                .is_err()
        );
        assert!(AuthorityClock::new(1000, 1000, now).is_err());
        // A whole handshake consumes the same original deadline even if wall time stalls.
        let mut clock = AuthorityClock::new(1000, 2000, now).unwrap();
        clock
            .check(1000, 2000, now + Duration::from_millis(999))
            .unwrap();
        assert!(
            clock
                .check(1000, 2000, now + Duration::from_millis(1000))
                .is_err()
        );
    }
}
