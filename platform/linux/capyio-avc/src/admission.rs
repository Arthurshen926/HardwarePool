use crate::{FiniteStreamBudget, MAX_FRAMES, MAX_INPUT_BYTES, error};
use capyio_core::StreamId;
use capyio_linux_camera::{FRAME_BYTES, FRAME_DURATION_NS};
use capyio_vcamdroid_adapter::{AvcLayout, AvcRecord, AvcRecordGuard, AvcStreamKey, decode_record};
use capyio_video::{VideoFrameDescriptor, VideoFrameFlags};
use std::{io, time::Instant};

#[cfg_attr(
    not(all(target_os = "linux", feature = "native-avc")),
    allow(dead_code)
)]
pub(crate) struct Request {
    pub kind: u32,
    pub flags: u32,
    pub sequence: u64,
    pub pts_us: u64,
    pub payload: Vec<u8>,
    pub descriptor: Option<VideoFrameDescriptor>,
}
pub(crate) struct Admission {
    guard: AvcRecordGuard,
    stream: AvcStreamKey,
    input_bytes: usize,
    frames: usize,
    max_frames: usize,
    max_input_bytes: usize,
    rate: Option<RateBudget>,
}

const TOKEN_SCALE: u64 = 1_000_000_000;
#[derive(Clone)]
struct TokenBucket {
    tokens: u64,
    capacity: u64,
    rate: u64,
    last: Instant,
}
impl TokenBucket {
    fn new(rate: u64, burst: u64, now: Instant) -> io::Result<Self> {
        let capacity = burst
            .checked_mul(TOKEN_SCALE)
            .ok_or_else(|| error("token capacity overflow"))?;
        Ok(Self {
            tokens: capacity,
            capacity,
            rate,
            last: now,
        })
    }
    fn charge(&mut self, count: usize, now: Instant) -> io::Result<()> {
        let elapsed = now
            .checked_duration_since(self.last)
            .ok_or_else(|| error("monotonic token clock regressed"))?;
        let refill = u64::try_from(elapsed.as_nanos())
            .ok()
            .and_then(|v| v.checked_mul(self.rate))
            .ok_or_else(|| error("token refill overflow"))?;
        let available = self
            .tokens
            .checked_add(refill)
            .ok_or_else(|| error("token balance overflow"))?
            .min(self.capacity);
        let cost = u64::try_from(count)
            .ok()
            .and_then(|v| v.checked_mul(TOKEN_SCALE))
            .ok_or_else(|| error("token cost overflow"))?;
        let remaining = available
            .checked_sub(cost)
            .ok_or_else(|| error("finite local-monotonic rate limit"))?;
        self.tokens = remaining;
        self.last = now;
        Ok(())
    }
}
#[derive(Clone)]
struct RateBudget {
    frames: TokenBucket,
    bytes: TokenBucket,
}
impl RateBudget {
    fn new(now: Instant) -> io::Result<Self> {
        Ok(Self {
            frames: TokenBucket::new(30, 3, now)?,
            bytes: TokenBucket::new(2 * 1024 * 1024, 4 * 1024 * 1024, now)?,
        })
    }
}
impl Admission {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn new(stream: AvcStreamKey) -> io::Result<Self> {
        Self::with_budget(stream, None)
    }
    pub fn with_budget(
        stream: AvcStreamKey,
        budget: Option<FiniteStreamBudget>,
    ) -> io::Result<Self> {
        Self::with_budget_at(stream, budget, Instant::now())
    }
    fn with_budget_at(
        stream: AvcStreamKey,
        budget: Option<FiniteStreamBudget>,
        now: Instant,
    ) -> io::Result<Self> {
        Ok(Self {
            guard: AvcRecordGuard::new(stream).map_err(error)?,
            stream,
            input_bytes: 0,
            frames: 0,
            max_frames: budget.map_or(MAX_FRAMES, |b| b.max_frames),
            max_input_bytes: budget.map_or(MAX_INPUT_BYTES, |b| b.max_input_bytes),
            rate: budget.map(|_| RateBudget::new(now)).transpose()?,
        })
    }
    pub fn accept(&mut self, bytes: &[u8]) -> io::Result<Request> {
        self.accept_at(bytes, Instant::now())
    }
    fn accept_at(&mut self, bytes: &[u8], now: Instant) -> io::Result<Request> {
        let total = self
            .input_bytes
            .checked_add(bytes.len())
            .filter(|v| *v <= self.max_input_bytes)
            .ok_or_else(|| error("cumulative CAVC input limit"))?;
        // Header inspection here is only a conservative resource preflight.
        // The shared parser/guard still validate every field before commit.
        let is_frame = bytes.get(6) == Some(&2) && bytes.get(7).is_some_and(|f| f & 2 == 0);
        if is_frame && self.frames >= self.max_frames {
            return Err(error("decoded frame budget exhausted"));
        }
        let mut rate = self.rate.clone();
        if let Some(rate) = &mut rate {
            rate.bytes.charge(bytes.len(), now)?;
            rate.frames.charge(usize::from(is_frame), now)?;
        }
        let record = decode_record(bytes).map_err(error)?;
        let mut guard = self.guard.clone();
        guard.accept(&record).map_err(error)?;
        let request = match record {
            AvcRecord::Config { config: c, .. } => {
                if c.width != 1280
                    || c.height != 720
                    || c.frames_per_second != 30
                    || c.clockwise_rotation_degrees != 0
                    || c.access_unit_layout != AvcLayout::AnnexB
                    || c.codec_specific_layout != AvcLayout::AnnexB
                    || c.csd0
                        .len()
                        .checked_add(c.csd1.len())
                        .is_none_or(|n| n > 65536)
                {
                    return Err(error(
                        "only canonical 720p30 rotation-zero Annex B config is supported",
                    ));
                }
                let mut payload = c.csd0;
                payload.extend_from_slice(&c.csd1);
                Request {
                    kind: 1,
                    flags: 0,
                    sequence: 0,
                    pts_us: 0,
                    payload,
                    descriptor: None,
                }
            }
            AvcRecord::AccessUnit(u) => {
                let timestamp = u
                    .presentation_time_us
                    .checked_mul(1000)
                    .filter(|n| *n <= i64::MAX as u64)
                    .ok_or_else(|| error("PTS nanoseconds overflow"))?;
                let descriptor = if u.end_of_stream {
                    None
                } else {
                    if self.frames >= self.max_frames {
                        return Err(error("decoded frame budget exhausted"));
                    }
                    // Binding bytes were generated from this same typed StreamId.
                    let id: StreamId = format_uuid(self.stream.stream_id).parse().map_err(error)?;
                    Some(VideoFrameDescriptor {
                        stream_id: id,
                        stream_epoch: self.stream.epoch,
                        sequence: u.sequence,
                        source_timestamp_nanos: timestamp,
                        duration_nanos: FRAME_DURATION_NS,
                        payload_bytes: FRAME_BYTES as u64,
                        flags: VideoFrameFlags {
                            discontinuity: u.discontinuity,
                            end_of_stream: false,
                        },
                    })
                };
                Request {
                    kind: if u.end_of_stream { 3 } else { 2 },
                    flags: u32::from(u.key_frame) | (u32::from(u.discontinuity) << 1),
                    sequence: u.sequence,
                    pts_us: u.presentation_time_us,
                    payload: u.payload,
                    descriptor,
                }
            }
        };
        let frames = self
            .frames
            .checked_add(usize::from(request.kind == 2))
            .filter(|v| *v <= self.max_frames)
            .ok_or_else(|| error("decoded frame budget overflow"))?;
        self.guard = guard;
        self.input_bytes = total;
        self.frames = frames;
        self.rate = rate;
        Ok(request)
    }
}
fn format_uuid(bytes: [u8; 16]) -> String {
    let h: String = bytes.iter().map(|v| format!("{v:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
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
    fn unit(sequence: u64, pts: u64) -> AvcAccessUnit {
        AvcAccessUnit {
            stream: key(),
            sequence,
            presentation_time_us: pts,
            key_frame: true,
            end_of_stream: false,
            discontinuity: false,
            payload: vec![0, 0, 0, 1, 0x65, 0x88],
        }
    }
    fn ready() -> Admission {
        let mut a = Admission::new(key()).unwrap();
        a.accept(&encode_config(key(), &config()).unwrap()).unwrap();
        a
    }
    #[test]
    fn rejects_noncanonical_config_transactionally() {
        let mut a = Admission::new(key()).unwrap();
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
            AvcConfig {
                csd0: vec![1; 65536],
                csd1: vec![2],
                ..config()
            },
        ] {
            assert!(a.accept(&encode_config(key(), &c).unwrap()).is_err());
        }
        a.accept(&encode_config(key(), &config()).unwrap()).unwrap();
        assert!(a.accept(&encode_config(key(), &config()).unwrap()).is_err());
    }
    #[test]
    fn exact_order_identity_pts_and_eos() {
        let mut a = ready();
        let first = a.accept(&encode_access_unit(&unit(1, 0)).unwrap()).unwrap();
        assert_eq!(first.descriptor.unwrap().duration_nanos, 33_333_333);
        assert!(a.accept(&encode_access_unit(&unit(1, 0)).unwrap()).is_err());
        assert!(
            a.accept(&encode_access_unit(&unit(3, 33333)).unwrap())
                .is_err()
        );
        let mut bad = unit(2, 33333);
        bad.stream.epoch = 2;
        assert!(a.accept(&encode_access_unit(&bad).unwrap()).is_err());
        bad = unit(2, (i64::MAX as u64) / 1000 + 1);
        assert!(a.accept(&encode_access_unit(&bad).unwrap()).is_err());
        let frame = a
            .accept(&encode_access_unit(&unit(2, 33333)).unwrap())
            .unwrap();
        assert_eq!(frame.descriptor.unwrap().source_timestamp_nanos, 33_333_000);
        let eos = AvcAccessUnit {
            sequence: 3,
            presentation_time_us: 33333,
            key_frame: false,
            end_of_stream: true,
            payload: vec![],
            ..unit(3, 33333)
        };
        assert_eq!(
            a.accept(&encode_access_unit(&eos).unwrap()).unwrap().kind,
            3
        );
        assert!(
            a.accept(&encode_access_unit(&unit(4, 66666)).unwrap())
                .is_err()
        );
    }
    #[test]
    fn caps_and_discontinuity_are_bounded() {
        let mut a = ready();
        let mut u = unit(9, 1);
        u.discontinuity = true;
        assert_eq!(a.accept(&encode_access_unit(&u).unwrap()).unwrap().flags, 3);
        a.frames = MAX_FRAMES;
        assert!(
            a.accept(&encode_access_unit(&unit(10, 2)).unwrap())
                .is_err()
        );
        a.frames = 1;
        a.input_bytes = MAX_INPUT_BYTES;
        assert!(
            a.accept(&encode_access_unit(&unit(10, 2)).unwrap())
                .is_err()
        );
    }
    #[test]
    fn malformed_records_fail_without_allocating_unbounded_payloads() {
        let mut a = ready();
        let bytes = encode_access_unit(&unit(1, 0)).unwrap();
        for n in 0..bytes.len() {
            assert!(a.accept(&bytes[..n]).is_err());
        }
        let mut bad = bytes.clone();
        bad[52..56].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(a.accept(&bad).is_err());
        a.accept(&bytes).unwrap();
    }
    #[test]
    fn legacy_121st_frame_is_still_rejected_without_rate_limit() {
        let mut a = ready();
        for n in 1..=120 {
            a.accept(&encode_access_unit(&unit(n, n)).unwrap()).unwrap();
        }
        assert!(
            a.accept(&encode_access_unit(&unit(121, 121)).unwrap())
                .is_err()
        );
        assert_eq!(a.frames, 120);
    }
    fn finite_ready(now: Instant, millis: u64) -> Admission {
        let budget =
            FiniteStreamBudget::for_lifetime(std::time::Duration::from_millis(millis)).unwrap();
        let mut a = Admission::with_budget_at(key(), Some(budget), now).unwrap();
        a.accept_at(&encode_config(key(), &config()).unwrap(), now)
            .unwrap();
        a
    }
    #[test]
    fn finite_allows_over_120_but_never_immutable_cap() {
        let now = Instant::now();
        let mut a = finite_ready(now, 5000);
        for n in 1..=150 {
            a.accept_at(
                &encode_access_unit(&unit(n, n)).unwrap(),
                now + std::time::Duration::from_nanos((n - 1) * 33_333_334),
            )
            .unwrap();
        }
        assert_eq!(a.frames, 150);
        assert!(
            a.accept_at(
                &encode_access_unit(&unit(151, 151)).unwrap(),
                now + std::time::Duration::from_secs(6)
            )
            .is_err()
        );
        assert_eq!(a.frames, 150);
    }
    #[test]
    fn finite_rate_uses_local_time_not_pts_or_discontinuity() {
        let now = Instant::now();
        let mut a = finite_ready(now, 60_000);
        for n in 1..=3 {
            a.accept_at(&encode_access_unit(&unit(n, n)).unwrap(), now)
                .unwrap();
        }
        let mut u = unit(99, 9_000_000_000);
        u.discontinuity = true;
        let bytes = encode_access_unit(&u).unwrap();
        assert!(a.accept_at(&bytes, now).is_err());
        assert!(
            a.accept_at(&bytes, now + std::time::Duration::from_nanos(33_333_333))
                .is_err()
        );
        a.accept_at(&bytes, now + std::time::Duration::from_nanos(33_333_334))
            .unwrap();
        // Recovery does not reset the consumed bucket or permit regressed PTS.
        u.sequence += 1;
        u.presentation_time_us += 1;
        assert!(
            a.accept_at(
                &encode_access_unit(&u).unwrap(),
                now + std::time::Duration::from_nanos(33_333_334)
            )
            .is_err()
        );
        u.presentation_time_us = 0;
        assert!(
            a.accept_at(
                &encode_access_unit(&u).unwrap(),
                now + std::time::Duration::from_secs(1)
            )
            .is_err()
        );
        assert_eq!(a.frames, 4);
    }
    #[test]
    fn byte_rate_counts_complete_cavc_and_eos_is_not_a_frame() {
        let now = Instant::now();
        let mut a = finite_ready(now, 60_000);
        let mut u = unit(1, 0);
        // CSD bytes and both fixed headers consume burst, not only AU payload.
        u.payload.resize(4 * 1024 * 1024 - a.input_bytes - 56, 0);
        a.accept_at(&encode_access_unit(&u).unwrap(), now).unwrap();
        let eos = AvcAccessUnit {
            key_frame: false,
            end_of_stream: true,
            payload: vec![],
            ..unit(2, 0)
        };
        let eos = encode_access_unit(&eos).unwrap();
        assert!(a.accept_at(&eos, now).is_err());
        a.accept_at(&eos, now + std::time::Duration::from_millis(1))
            .unwrap();
        assert_eq!(a.frames, 1);
        assert_eq!(a.input_bytes, 4 * 1024 * 1024 + 56);
    }
    #[test]
    fn checked_rate_arithmetic_and_byte_cap_are_transactional() {
        let now = Instant::now();
        let mut bucket = TokenBucket::new(30, 3, now).unwrap();
        assert!(bucket.charge(usize::MAX, now).is_err());
        assert!(
            bucket
                .charge(1, now - std::time::Duration::from_nanos(1))
                .is_err()
        );
        bucket.rate = u64::MAX;
        assert!(
            bucket
                .charge(0, now + std::time::Duration::from_secs(1))
                .is_err()
        );
        assert_eq!(bucket.tokens, 3 * TOKEN_SCALE);
        let mut a = finite_ready(now, 1000);
        a.input_bytes = a.max_input_bytes;
        assert!(
            a.accept_at(&encode_access_unit(&unit(1, 0)).unwrap(), now)
                .is_err()
        );
        assert_eq!(a.frames, 0);
        a.input_bytes = usize::MAX;
        assert!(
            a.accept_at(&encode_access_unit(&unit(1, 0)).unwrap(), now)
                .is_err()
        );
    }
    #[test]
    fn generated_fixture_admission_and_metadata() {
        let bytes = include_bytes!("../../../../fixtures/linux-avc/solid-720p30.cavc");
        let mut stream = [0; 16];
        for (i, b) in stream.iter_mut().enumerate() {
            *b = i as u8 + 1;
        }
        let mut a = Admission::new(AvcStreamKey {
            stream_id: stream,
            epoch: 1,
        })
        .unwrap();
        let mut pos = 0;
        let mut frames = 0;
        while pos < bytes.len() {
            let n = u32::from_be_bytes(bytes[pos + 52..pos + 56].try_into().unwrap()) as usize + 56;
            let r = a.accept(&bytes[pos..pos + n]).unwrap();
            if let Some(d) = r.descriptor {
                assert_eq!(d.sequence, frames + 1);
                assert_eq!(d.source_timestamp_nanos, frames * 33_333_000);
                frames += 1;
            }
            pos += n;
        }
        assert_eq!(frames, 3);
        assert!(a.guard.ended());
    }
}
