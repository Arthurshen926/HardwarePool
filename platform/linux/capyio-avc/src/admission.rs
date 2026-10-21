use crate::{MAX_FRAMES, MAX_INPUT_BYTES, error};
use capyio_core::StreamId;
use capyio_linux_camera::{FRAME_BYTES, FRAME_DURATION_NS};
use capyio_vcamdroid_adapter::{AvcLayout, AvcRecord, AvcRecordGuard, AvcStreamKey, decode_record};
use capyio_video::{VideoFrameDescriptor, VideoFrameFlags};
use std::io;

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
}
impl Admission {
    pub fn new(stream: AvcStreamKey) -> io::Result<Self> {
        Ok(Self {
            guard: AvcRecordGuard::new(stream).map_err(error)?,
            stream,
            input_bytes: 0,
            frames: 0,
        })
    }
    pub fn accept(&mut self, bytes: &[u8]) -> io::Result<Request> {
        let total = self
            .input_bytes
            .checked_add(bytes.len())
            .filter(|v| *v <= MAX_INPUT_BYTES)
            .ok_or_else(|| error("cumulative CAVC input limit"))?;
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
                    if self.frames >= MAX_FRAMES {
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
        self.guard = guard;
        self.input_bytes = total;
        self.frames += usize::from(request.kind == 2);
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
