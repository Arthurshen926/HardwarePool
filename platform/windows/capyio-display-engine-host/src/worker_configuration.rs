use std::{
    fmt,
    path::{Path, PathBuf},
};

use capyio_display_stream_adapter::{
    DisplayMode, DisplayOnlyPolicy, DisplayStreamSessionRequest, DynamicRange,
    ValidatedDisplayStreamSession, VideoCodec,
};
use sha2::{Digest, Sha256};

use crate::DisplayWorkerLease;

pub const DISPLAY_WORKER_CONFIG_BYTES: usize = 1_200;
pub const DISPLAY_WORKER_CONFIG_ACK_BYTES: usize = 64;
const CONFIG_MAGIC: [u8; 8] = *b"CPYDWCF1";
const CONFIG_ACK_MAGIC: [u8; 8] = *b"CPYDWCA1";
const VERSION: u16 = 1;
const PATH_OFFSET: usize = 160;
const PATH_CAPACITY: usize = 1_024;

/// Complete non-secret policy needed by the interactive worker. It is bound
/// to the acknowledged lease and contains no capture selector or launch key.
pub struct DisplayWorkerEngineConfiguration {
    library_path: PathBuf,
    expected_sha256: [u8; 32],
    session: ValidatedDisplayStreamSession,
}

impl DisplayWorkerEngineConfiguration {
    #[must_use]
    pub fn library_path(&self) -> &Path {
        &self.library_path
    }

    #[must_use]
    pub const fn expected_sha256(&self) -> [u8; 32] {
        self.expected_sha256
    }

    #[must_use]
    pub const fn session(&self) -> &ValidatedDisplayStreamSession {
        &self.session
    }
}

/// Fixed record sent after lease acknowledgement and before any secret.
pub struct DisplayWorkerEngineConfigRecord([u8; DISPLAY_WORKER_CONFIG_BYTES]);

impl DisplayWorkerEngineConfigRecord {
    pub fn new(
        lease: &DisplayWorkerLease,
        library_path: &Path,
        expected_sha256: [u8; 32],
        session: &ValidatedDisplayStreamSession,
    ) -> Result<Self, DisplayWorkerConfigurationError> {
        validate_session_binding(lease, session)?;
        let path = validate_library_path(library_path)?;
        if expected_sha256 == [0; 32] {
            return Err(DisplayWorkerConfigurationError::InvalidEngineHash);
        }
        let request = session.request();
        let mut bytes = [0_u8; DISPLAY_WORKER_CONFIG_BYTES];
        bytes[..8].copy_from_slice(&CONFIG_MAGIC);
        put_u16(&mut bytes, 8, VERSION);
        put_u16(&mut bytes, 10, DISPLAY_WORKER_CONFIG_BYTES as u16);
        put_u64(&mut bytes, 16, lease.request_id);
        put_u32(&mut bytes, 24, lease.windows_session_id.get());
        bytes[32..64].copy_from_slice(&Sha256::digest(lease.encode()));
        bytes[64..80].copy_from_slice(&request.adapter_instance_id);
        bytes[80..96].copy_from_slice(&request.display_capability_id);
        put_u32(&mut bytes, 96, request.mode.width);
        put_u32(&mut bytes, 100, request.mode.height);
        put_u32(&mut bytes, 104, request.mode.refresh_millihertz);
        put_u32(&mut bytes, 108, request.bitrate_kbps);
        put_u16(&mut bytes, 112, request.port_base);
        bytes[114] = encode_codec(request.codec);
        bytes[115] = encode_dynamic_range(request.dynamic_range);
        bytes[120..152].copy_from_slice(&expected_sha256);
        put_u16(
            &mut bytes,
            152,
            u16::try_from(path.len()).map_err(|_| DisplayWorkerConfigurationError::InvalidPath)?,
        );
        bytes[PATH_OFFSET..PATH_OFFSET + path.len()].copy_from_slice(path.as_bytes());
        Ok(Self(bytes))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; DISPLAY_WORKER_CONFIG_BYTES] {
        &self.0
    }

    #[must_use]
    pub fn from_bytes(bytes: [u8; DISPLAY_WORKER_CONFIG_BYTES]) -> Self {
        Self(bytes)
    }

    pub fn decode(
        bytes: [u8; DISPLAY_WORKER_CONFIG_BYTES],
        lease: &DisplayWorkerLease,
    ) -> Result<DisplayWorkerEngineConfiguration, DisplayWorkerConfigurationError> {
        if bytes[..8] != CONFIG_MAGIC
            || read_u16(&bytes, 8) != VERSION
            || usize::from(read_u16(&bytes, 10)) != DISPLAY_WORKER_CONFIG_BYTES
            || bytes[12..16] != [0; 4]
            || bytes[116..120] != [0; 4]
            || bytes[154..160] != [0; 6]
        {
            return Err(DisplayWorkerConfigurationError::InvalidLayout);
        }
        if read_u64(&bytes, 16) != lease.request_id
            || read_u32(&bytes, 24) != lease.windows_session_id.get()
            || bytes[28..32] != [0; 4]
            || bytes[32..64] != Sha256::digest(lease.encode())[..]
            || bytes[64..80] != lease.adapter_instance_id
            || bytes[80..96] != lease.display_capability_id
        {
            return Err(DisplayWorkerConfigurationError::LeaseMismatch);
        }
        let path_length = usize::from(read_u16(&bytes, 152));
        if path_length == 0
            || path_length > PATH_CAPACITY
            || bytes[PATH_OFFSET + path_length..]
                != [0; DISPLAY_WORKER_CONFIG_BYTES][PATH_OFFSET + path_length..]
        {
            return Err(DisplayWorkerConfigurationError::InvalidPath);
        }
        let path_text = std::str::from_utf8(&bytes[PATH_OFFSET..PATH_OFFSET + path_length])
            .map_err(|_| DisplayWorkerConfigurationError::InvalidPath)?;
        let library_path = PathBuf::from(path_text);
        validate_library_path(&library_path)?;
        let expected_sha256: [u8; 32] = bytes[120..152].try_into().expect("fixed hash slice");
        if expected_sha256 == [0; 32] {
            return Err(DisplayWorkerConfigurationError::InvalidEngineHash);
        }
        let session = ValidatedDisplayStreamSession::new(DisplayStreamSessionRequest {
            adapter_instance_id: bytes[64..80].try_into().expect("fixed identity slice"),
            display_capability_id: bytes[80..96].try_into().expect("fixed identity slice"),
            mode: DisplayMode {
                width: read_u32(&bytes, 96),
                height: read_u32(&bytes, 100),
                refresh_millihertz: read_u32(&bytes, 104),
            },
            codec: decode_codec(bytes[114])?,
            dynamic_range: decode_dynamic_range(bytes[115])?,
            bitrate_kbps: read_u32(&bytes, 108),
            port_base: read_u16(&bytes, 112),
            policy: DisplayOnlyPolicy::CAPYIO,
        })
        .map_err(|_| DisplayWorkerConfigurationError::InvalidSession)?;
        validate_session_binding(lease, &session)?;
        Ok(DisplayWorkerEngineConfiguration {
            library_path,
            expected_sha256,
            session,
        })
    }
}

pub fn encode_display_worker_config_ack(
    record: &DisplayWorkerEngineConfigRecord,
    lease: &DisplayWorkerLease,
) -> Result<[u8; DISPLAY_WORKER_CONFIG_ACK_BYTES], DisplayWorkerConfigurationError> {
    DisplayWorkerEngineConfigRecord::decode(*record.as_bytes(), lease)?;
    let mut acknowledgement = [0_u8; DISPLAY_WORKER_CONFIG_ACK_BYTES];
    acknowledgement[..8].copy_from_slice(&CONFIG_ACK_MAGIC);
    put_u16(&mut acknowledgement, 8, VERSION);
    put_u16(
        &mut acknowledgement,
        10,
        DISPLAY_WORKER_CONFIG_ACK_BYTES as u16,
    );
    put_u64(&mut acknowledgement, 16, lease.request_id);
    put_u32(&mut acknowledgement, 24, lease.windows_session_id.get());
    acknowledgement[32..64].copy_from_slice(&Sha256::digest(record.as_bytes()));
    Ok(acknowledgement)
}

pub fn verify_display_worker_config_ack(
    record: &DisplayWorkerEngineConfigRecord,
    lease: &DisplayWorkerLease,
    acknowledgement: &[u8; DISPLAY_WORKER_CONFIG_ACK_BYTES],
) -> Result<(), DisplayWorkerConfigurationError> {
    let expected = encode_display_worker_config_ack(record, lease)?;
    if acknowledgement == &expected {
        Ok(())
    } else {
        Err(DisplayWorkerConfigurationError::InvalidAcknowledgement)
    }
}

fn validate_session_binding(
    lease: &DisplayWorkerLease,
    session: &ValidatedDisplayStreamSession,
) -> Result<(), DisplayWorkerConfigurationError> {
    let request = session.request();
    if request.adapter_instance_id != lease.adapter_instance_id
        || request.display_capability_id != lease.display_capability_id
    {
        Err(DisplayWorkerConfigurationError::LeaseMismatch)
    } else {
        Ok(())
    }
}

fn validate_library_path(path: &Path) -> Result<&str, DisplayWorkerConfigurationError> {
    let text = path
        .to_str()
        .ok_or(DisplayWorkerConfigurationError::InvalidPath)?;
    let extended_unc = text
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(r"\\?\UNC\"));
    let ordinary_unc = text.starts_with(r"\\") && !text.starts_with(r"\\?\");
    if text.is_empty()
        || text.len() > PATH_CAPACITY
        || text.as_bytes().contains(&0)
        || !path.is_absolute()
        || extended_unc
        || ordinary_unc
        || text.starts_with("//")
        || !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value == "dll")
    {
        return Err(DisplayWorkerConfigurationError::InvalidPath);
    }
    Ok(text)
}

const fn encode_codec(codec: VideoCodec) -> u8 {
    match codec {
        VideoCodec::H264 => 1,
        VideoCodec::Hevc => 2,
        VideoCodec::Av1 => 3,
    }
}

fn decode_codec(value: u8) -> Result<VideoCodec, DisplayWorkerConfigurationError> {
    match value {
        1 => Ok(VideoCodec::H264),
        2 => Ok(VideoCodec::Hevc),
        3 => Ok(VideoCodec::Av1),
        _ => Err(DisplayWorkerConfigurationError::InvalidSession),
    }
}

const fn encode_dynamic_range(value: DynamicRange) -> u8 {
    match value {
        DynamicRange::Sdr8 => 1,
        DynamicRange::Hdr10 => 2,
    }
}

fn decode_dynamic_range(value: u8) -> Result<DynamicRange, DisplayWorkerConfigurationError> {
    match value {
        1 => Ok(DynamicRange::Sdr8),
        2 => Ok(DynamicRange::Hdr10),
        _ => Err(DisplayWorkerConfigurationError::InvalidSession),
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
fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("fixed u16 slice"),
    )
}
fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed u32 slice"),
    )
}
fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("fixed u64 slice"),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayWorkerConfigurationError {
    InvalidLayout,
    LeaseMismatch,
    InvalidPath,
    InvalidEngineHash,
    InvalidSession,
    InvalidAcknowledgement,
}

impl fmt::Display for DisplayWorkerConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLayout => "display worker engine configuration layout is invalid",
            Self::LeaseMismatch => "display worker engine configuration does not match its lease",
            Self::InvalidPath => "display worker engine library path is invalid",
            Self::InvalidEngineHash => "display worker engine hash is invalid",
            Self::InvalidSession => "display worker engine session is invalid",
            Self::InvalidAcknowledgement => {
                "display worker engine configuration acknowledgement is invalid"
            }
        })
    }
}

impl std::error::Error for DisplayWorkerConfigurationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_display_stream_adapter::launch_record::GameStreamLaunchBinding;
    use capyio_windows_display::InteractiveDisplaySessionId;
    use std::net::Ipv4Addr;

    fn session() -> ValidatedDisplayStreamSession {
        ValidatedDisplayStreamSession::new(DisplayStreamSessionRequest {
            adapter_instance_id: [1; 16],
            display_capability_id: [2; 16],
            mode: DisplayMode {
                width: 2640,
                height: 1216,
                refresh_millihertz: 120_000,
            },
            codec: VideoCodec::Hevc,
            dynamic_range: DynamicRange::Hdr10,
            bitrate_kbps: 45_000,
            port_base: 50_000,
            policy: DisplayOnlyPolicy::CAPYIO,
        })
        .unwrap()
    }

    fn lease() -> DisplayWorkerLease {
        DisplayWorkerLease::new(
            7,
            InteractiveDisplaySessionId::new(3).unwrap(),
            &session(),
            GameStreamLaunchBinding {
                session_id: [3; 16],
                route_id: [4; 16],
                stream_id: [5; 16],
                stream_epoch: 6,
                host_ipv4: Ipv4Addr::LOCALHOST,
            },
        )
        .unwrap()
    }

    #[test]
    fn configuration_round_trips_and_acknowledges_every_byte() {
        let lease = lease();
        let record = DisplayWorkerEngineConfigRecord::new(
            &lease,
            Path::new(r"C:\CapyIO\capyio-display-engine.dll"),
            [9; 32],
            &session(),
        )
        .unwrap();
        let decoded = DisplayWorkerEngineConfigRecord::decode(*record.as_bytes(), &lease).unwrap();
        assert_eq!(
            decoded.library_path(),
            Path::new(r"C:\CapyIO\capyio-display-engine.dll")
        );
        assert_eq!(decoded.expected_sha256(), [9; 32]);
        assert_eq!(decoded.session(), &session());
        let ack = encode_display_worker_config_ack(&record, &lease).unwrap();
        verify_display_worker_config_ack(&record, &lease, &ack).unwrap();
        let mut changed = *record.as_bytes();
        changed[104] ^= 1;
        let changed = DisplayWorkerEngineConfigRecord(changed);
        assert!(verify_display_worker_config_ack(&changed, &lease, &ack).is_err());
    }

    #[test]
    fn configuration_rejects_cross_lease_paths_hashes_and_reserved_bytes() {
        let lease = lease();
        for path in [
            r"engine.dll",
            r"\\server\engine.dll",
            r"C:\CapyIO\engine.DLL",
        ] {
            assert!(
                DisplayWorkerEngineConfigRecord::new(&lease, Path::new(path), [9; 32], &session())
                    .is_err()
            );
        }
        assert!(
            DisplayWorkerEngineConfigRecord::new(
                &lease,
                Path::new(r"C:\CapyIO\engine.dll"),
                [0; 32],
                &session()
            )
            .is_err()
        );
        let record = DisplayWorkerEngineConfigRecord::new(
            &lease,
            Path::new(r"C:\CapyIO\engine.dll"),
            [9; 32],
            &session(),
        )
        .unwrap();
        let mut changed = *record.as_bytes();
        changed[119] = 1;
        assert_eq!(
            DisplayWorkerEngineConfigRecord::decode(changed, &lease).err(),
            Some(DisplayWorkerConfigurationError::InvalidLayout)
        );
        let mut other = lease;
        other.request_id = 8;
        assert!(DisplayWorkerEngineConfigRecord::decode(*record.as_bytes(), &other).is_err());
    }
}
