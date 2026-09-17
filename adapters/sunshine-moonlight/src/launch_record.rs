//! Fixed authenticated control record shared with the Android GameStream sink.
//!
//! This record is not a public discovery or pairing protocol. It may be sent
//! only after mutual TLS and exact Route binding. Callers own and must wipe the
//! returned byte array after the receiver has acknowledged it.

use std::{
    fmt,
    net::Ipv4Addr,
    sync::atomic::{Ordering, compiler_fence},
};

use crate::{DisplayStreamSessionRequest, ValidatedDisplayStreamSession};

pub const GAMESTREAM_LAUNCH_RECORD_BYTES: usize = 176;
pub const GAMESTREAM_LAUNCH_ACK_BYTES: usize = 64;
const MAGIC: [u8; 8] = *b"CPYGS001";
const ACK_MAGIC: [u8; 8] = *b"CPYGSA01";
const VERSION: u16 = 1;

/// One-shot secret-bearing values generated for one authenticated connection.
/// Deliberately neither `Clone` nor `Debug`.
pub struct GameStreamLaunchSecrets {
    pub launch_session_id: u32,
    pub key: [u8; 16],
    pub key_id: u32,
    pub ping_payload: [u8; 8],
    pub control_connect_data: u32,
}

impl Drop for GameStreamLaunchSecrets {
    fn drop(&mut self) {
        self.launch_session_id = 0;
        self.key.fill(0);
        self.key_id = 0;
        self.ping_payload.fill(0);
        self.control_connect_data = 0;
        compiler_fence(Ordering::SeqCst);
        std::hint::black_box(&self.key);
        std::hint::black_box(&self.ping_payload);
    }
}

/// Non-secret binding that ties the launch to both CapyIO and transport IDs.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct GameStreamLaunchBinding {
    pub session_id: [u8; 16],
    pub route_id: [u8; 16],
    pub stream_id: [u8; 16],
    pub stream_epoch: u64,
    pub host_ipv4: Ipv4Addr,
}

impl fmt::Debug for GameStreamLaunchBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GameStreamLaunchBinding")
            .field("stream_epoch", &self.stream_epoch)
            .finish_non_exhaustive()
    }
}

pub fn encode_gamestream_launch_record(
    session: &ValidatedDisplayStreamSession,
    binding: GameStreamLaunchBinding,
    secrets: &GameStreamLaunchSecrets,
) -> Result<[u8; GAMESTREAM_LAUNCH_RECORD_BYTES], GameStreamLaunchRecordError> {
    let request = session.request();
    validate(request, binding, secrets)?;
    let mut bytes = [0_u8; GAMESTREAM_LAUNCH_RECORD_BYTES];
    bytes[..8].copy_from_slice(&MAGIC);
    put_u16(&mut bytes, 8, VERSION);
    put_u16(&mut bytes, 10, GAMESTREAM_LAUNCH_RECORD_BYTES as u16);
    put_u32(&mut bytes, 12, GAMESTREAM_LAUNCH_RECORD_BYTES as u32);
    bytes[16..32].copy_from_slice(&request.adapter_instance_id);
    bytes[32..48].copy_from_slice(&request.display_capability_id);
    bytes[48..64].copy_from_slice(&binding.session_id);
    bytes[64..80].copy_from_slice(&binding.route_id);
    bytes[80..96].copy_from_slice(&binding.stream_id);
    put_u64(&mut bytes, 96, binding.stream_epoch);
    bytes[104..108].copy_from_slice(&binding.host_ipv4.octets());
    put_u16(&mut bytes, 108, request.port_base);
    put_u32(&mut bytes, 112, request.mode.width);
    put_u32(&mut bytes, 116, request.mode.height);
    put_u32(&mut bytes, 120, request.mode.refresh_millihertz);
    put_u32(&mut bytes, 124, codec(request));
    put_u32(&mut bytes, 128, dynamic_range(request));
    put_u32(&mut bytes, 132, request.bitrate_kbps);
    put_u32(&mut bytes, 136, secrets.launch_session_id);
    bytes[140..156].copy_from_slice(&secrets.key);
    put_u32(&mut bytes, 156, secrets.key_id);
    bytes[160..168].copy_from_slice(&secrets.ping_payload);
    put_u32(&mut bytes, 168, secrets.control_connect_data);
    Ok(bytes)
}

/// Encodes the acknowledgement returned only after the receiver has parsed
/// and authorized the complete launch record.
pub fn encode_gamestream_launch_ack(
    binding: GameStreamLaunchBinding,
    launch_session_id: u32,
) -> Result<[u8; GAMESTREAM_LAUNCH_ACK_BYTES], GameStreamLaunchRecordError> {
    if binding.route_id == [0; 16]
        || binding.stream_id == [0; 16]
        || binding.stream_epoch == 0
        || launch_session_id == 0
    {
        return Err(GameStreamLaunchRecordError::InvalidAcknowledgement);
    }
    let mut bytes = [0_u8; GAMESTREAM_LAUNCH_ACK_BYTES];
    bytes[..8].copy_from_slice(&ACK_MAGIC);
    put_u16(&mut bytes, 8, VERSION);
    put_u16(&mut bytes, 10, GAMESTREAM_LAUNCH_ACK_BYTES as u16);
    put_u32(&mut bytes, 12, launch_session_id);
    bytes[16..32].copy_from_slice(&binding.route_id);
    bytes[32..48].copy_from_slice(&binding.stream_id);
    put_u64(&mut bytes, 48, binding.stream_epoch);
    Ok(bytes)
}

pub fn verify_gamestream_launch_ack(
    acknowledgement: &[u8; GAMESTREAM_LAUNCH_ACK_BYTES],
    binding: GameStreamLaunchBinding,
    launch_session_id: u32,
) -> Result<(), GameStreamLaunchRecordError> {
    let expected = encode_gamestream_launch_ack(binding, launch_session_id)?;
    if acknowledgement == &expected {
        Ok(())
    } else {
        Err(GameStreamLaunchRecordError::InvalidAcknowledgement)
    }
}

fn validate(
    request: &DisplayStreamSessionRequest,
    binding: GameStreamLaunchBinding,
    secrets: &GameStreamLaunchSecrets,
) -> Result<(), GameStreamLaunchRecordError> {
    if binding.session_id == [0; 16]
        || binding.route_id == [0; 16]
        || binding.stream_id == [0; 16]
        || binding.stream_epoch == 0
        || binding.host_ipv4.is_unspecified()
        || binding.host_ipv4.is_broadcast()
    {
        return Err(GameStreamLaunchRecordError::InvalidBinding);
    }
    if secrets.launch_session_id == 0 || secrets.key == [0; 16] {
        return Err(GameStreamLaunchRecordError::InvalidSecrets);
    }
    if request.port_base < 1024 || request.port_base == crate::SUNSHINE_DEFAULT_PORT_BASE {
        return Err(GameStreamLaunchRecordError::InvalidBinding);
    }
    Ok(())
}

const fn codec(request: &DisplayStreamSessionRequest) -> u32 {
    match request.codec {
        crate::VideoCodec::H264 => 1,
        crate::VideoCodec::Hevc => 2,
        crate::VideoCodec::Av1 => 3,
    }
}

const fn dynamic_range(request: &DisplayStreamSessionRequest) -> u32 {
    match request.dynamic_range {
        crate::DynamicRange::Sdr8 => 1,
        crate::DynamicRange::Hdr10 => 2,
    }
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_be_bytes());
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameStreamLaunchRecordError {
    InvalidBinding,
    InvalidSecrets,
    InvalidAcknowledgement,
}

impl fmt::Display for GameStreamLaunchRecordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidBinding => "GameStream launch binding is invalid",
            Self::InvalidSecrets => "GameStream launch secrets are invalid",
            Self::InvalidAcknowledgement => "GameStream launch acknowledgement is invalid",
        })
    }
}

impl std::error::Error for GameStreamLaunchRecordError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DisplayMode, DisplayOnlyPolicy, DisplayStreamSessionRequest, DynamicRange, VideoCodec,
    };

    fn session() -> ValidatedDisplayStreamSession {
        ValidatedDisplayStreamSession::new(DisplayStreamSessionRequest {
            adapter_instance_id: [1; 16],
            display_capability_id: [2; 16],
            mode: DisplayMode {
                width: 2640,
                height: 1216,
                refresh_millihertz: 59_940,
            },
            codec: VideoCodec::Hevc,
            dynamic_range: DynamicRange::Hdr10,
            bitrate_kbps: 45_000,
            port_base: 50_000,
            policy: DisplayOnlyPolicy::CAPYIO,
        })
        .unwrap()
    }

    #[test]
    fn record_is_exact_big_endian_and_contains_all_bindings() {
        let record = encode_gamestream_launch_record(
            &session(),
            GameStreamLaunchBinding {
                session_id: [3; 16],
                route_id: [4; 16],
                stream_id: [5; 16],
                stream_epoch: 9,
                host_ipv4: Ipv4Addr::new(192, 0, 2, 10),
            },
            &GameStreamLaunchSecrets {
                launch_session_id: 7,
                key: [0x42; 16],
                key_id: 0x1234_5678,
                ping_payload: [0x10; 8],
                control_connect_data: 0x90ab_cdef,
            },
        )
        .unwrap();
        assert_eq!(record.len(), GAMESTREAM_LAUNCH_RECORD_BYTES);
        assert_eq!(&record[..16], b"CPYGS001\0\x01\0\xb0\0\0\0\xb0");
        assert_eq!(&record[16..32], &[1; 16]);
        assert_eq!(&record[32..48], &[2; 16]);
        assert_eq!(&record[48..64], &[3; 16]);
        assert_eq!(&record[64..80], &[4; 16]);
        assert_eq!(&record[80..96], &[5; 16]);
        assert_eq!(&record[96..104], &9_u64.to_be_bytes());
        assert_eq!(&record[104..110], &[192, 0, 2, 10, 0xc3, 0x50]);
        assert_eq!(&record[120..124], &59_940_u32.to_be_bytes());
        assert_eq!(&record[124..132], &[0, 0, 0, 2, 0, 0, 0, 2]);
        assert_eq!(&record[140..156], &[0x42; 16]);
        assert_eq!(&record[172..], &[0; 4]);
    }

    #[test]
    fn record_rejects_missing_route_or_key() {
        let mut binding = GameStreamLaunchBinding {
            session_id: [3; 16],
            route_id: [0; 16],
            stream_id: [5; 16],
            stream_epoch: 9,
            host_ipv4: Ipv4Addr::LOCALHOST,
        };
        let mut secrets = GameStreamLaunchSecrets {
            launch_session_id: 7,
            key: [0x42; 16],
            key_id: 1,
            ping_payload: [2; 8],
            control_connect_data: 3,
        };
        assert!(matches!(
            encode_gamestream_launch_record(&session(), binding, &secrets),
            Err(GameStreamLaunchRecordError::InvalidBinding)
        ));
        binding.route_id = [4; 16];
        secrets.key = [0; 16];
        assert!(matches!(
            encode_gamestream_launch_record(&session(), binding, &secrets),
            Err(GameStreamLaunchRecordError::InvalidSecrets)
        ));
    }

    #[test]
    fn acknowledgement_is_exact_and_bound_to_the_launch() {
        let binding = GameStreamLaunchBinding {
            session_id: [3; 16],
            route_id: [4; 16],
            stream_id: [5; 16],
            stream_epoch: 9,
            host_ipv4: Ipv4Addr::LOCALHOST,
        };
        let acknowledgement = encode_gamestream_launch_ack(binding, 7).unwrap();
        assert_eq!(acknowledgement.len(), GAMESTREAM_LAUNCH_ACK_BYTES);
        assert_eq!(&acknowledgement[..16], b"CPYGSA01\0\x01\0@\0\0\0\x07");
        assert_eq!(&acknowledgement[16..32], &[4; 16]);
        assert_eq!(&acknowledgement[32..48], &[5; 16]);
        assert_eq!(&acknowledgement[48..56], &9_u64.to_be_bytes());
        assert_eq!(&acknowledgement[56..], &[0; 8]);
        verify_gamestream_launch_ack(&acknowledgement, binding, 7).unwrap();

        let mut stale = acknowledgement;
        stale[55] ^= 1;
        assert_eq!(
            verify_gamestream_launch_ack(&stale, binding, 7),
            Err(GameStreamLaunchRecordError::InvalidAcknowledgement)
        );
    }
}
