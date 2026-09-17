use std::{
    fmt, ptr,
    sync::atomic::{Ordering, compiler_fence},
};

use capyio_display_stream_adapter::{
    ValidatedDisplayStreamSession, engine_abi::EngineLaunchMaterialV1,
    launch_record::GameStreamLaunchBinding,
};
use capyio_windows_display::InteractiveDisplaySessionId;
use sha2::{Digest, Sha256};

pub const DISPLAY_WORKER_LEASE_BYTES: usize = 120;
pub const DISPLAY_WORKER_LEASE_ACK_BYTES: usize = 64;
pub const DISPLAY_WORKER_START_BYTES: usize = 160;
pub const DISPLAY_WORKER_START_ACK_BYTES: usize = 64;
pub const DISPLAY_WORKER_CONTROL_BYTES: usize = 64;
pub const DISPLAY_WORKER_CONTROL_ACK_BYTES: usize = 64;
const LEASE_MAGIC: [u8; 8] = *b"CPYDW001";
const ACK_MAGIC: [u8; 8] = *b"CPYDWA01";
const START_MAGIC: [u8; 8] = *b"CPYDWS01";
const START_ACK_MAGIC: [u8; 8] = *b"CPYDWSA1";
const CONTROL_MAGIC: [u8; 8] = *b"CPYDWC01";
const CONTROL_ACK_MAGIC: [u8; 8] = *b"CPYDWR01";
const VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayWorkerLease {
    pub request_id: u64,
    pub windows_session_id: InteractiveDisplaySessionId,
    pub adapter_instance_id: [u8; 16],
    pub display_capability_id: [u8; 16],
    pub capyio_session_id: [u8; 16],
    pub route_id: [u8; 16],
    pub stream_id: [u8; 16],
    pub stream_epoch: u64,
}

impl DisplayWorkerLease {
    pub fn new(
        request_id: u64,
        windows_session_id: InteractiveDisplaySessionId,
        session: &ValidatedDisplayStreamSession,
        binding: GameStreamLaunchBinding,
    ) -> Result<Self, DisplayWorkerProtocolError> {
        let request = session.request();
        let lease = Self {
            request_id,
            windows_session_id,
            adapter_instance_id: request.adapter_instance_id,
            display_capability_id: request.display_capability_id,
            capyio_session_id: binding.session_id,
            route_id: binding.route_id,
            stream_id: binding.stream_id,
            stream_epoch: binding.stream_epoch,
        };
        lease.validate()?;
        Ok(lease)
    }

    fn validate(self) -> Result<(), DisplayWorkerProtocolError> {
        if self.request_id == 0 || self.stream_epoch == 0 {
            return Err(DisplayWorkerProtocolError::InvalidLease);
        }
        for identity in [
            self.adapter_instance_id,
            self.display_capability_id,
            self.capyio_session_id,
            self.route_id,
            self.stream_id,
        ] {
            if identity == [0; 16] {
                return Err(DisplayWorkerProtocolError::InvalidLease);
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn encode(self) -> [u8; DISPLAY_WORKER_LEASE_BYTES] {
        let mut bytes = [0_u8; DISPLAY_WORKER_LEASE_BYTES];
        bytes[..8].copy_from_slice(&LEASE_MAGIC);
        put_u16(&mut bytes, 8, VERSION);
        put_u16(&mut bytes, 10, DISPLAY_WORKER_LEASE_BYTES as u16);
        put_u64(&mut bytes, 16, self.request_id);
        put_u32(&mut bytes, 24, self.windows_session_id.get());
        bytes[32..48].copy_from_slice(&self.adapter_instance_id);
        bytes[48..64].copy_from_slice(&self.display_capability_id);
        bytes[64..80].copy_from_slice(&self.capyio_session_id);
        bytes[80..96].copy_from_slice(&self.route_id);
        bytes[96..112].copy_from_slice(&self.stream_id);
        put_u64(&mut bytes, 112, self.stream_epoch);
        bytes
    }

    pub fn decode(
        bytes: &[u8; DISPLAY_WORKER_LEASE_BYTES],
    ) -> Result<Self, DisplayWorkerProtocolError> {
        if bytes[..8] != LEASE_MAGIC
            || read_u16(bytes, 8) != VERSION
            || usize::from(read_u16(bytes, 10)) != DISPLAY_WORKER_LEASE_BYTES
            || bytes[12..16] != [0; 4]
            || bytes[28..32] != [0; 4]
        {
            return Err(DisplayWorkerProtocolError::InvalidLayout);
        }
        let lease = Self {
            request_id: read_u64(bytes, 16),
            windows_session_id: InteractiveDisplaySessionId::new(read_u32(bytes, 24))
                .map_err(|_| DisplayWorkerProtocolError::InvalidLease)?,
            adapter_instance_id: copy_identity(bytes, 32),
            display_capability_id: copy_identity(bytes, 48),
            capyio_session_id: copy_identity(bytes, 64),
            route_id: copy_identity(bytes, 80),
            stream_id: copy_identity(bytes, 96),
            stream_epoch: read_u64(bytes, 112),
        };
        lease.validate()?;
        Ok(lease)
    }
}

pub fn encode_display_worker_lease_ack(
    lease_record: &[u8; DISPLAY_WORKER_LEASE_BYTES],
) -> Result<[u8; DISPLAY_WORKER_LEASE_ACK_BYTES], DisplayWorkerProtocolError> {
    let lease = DisplayWorkerLease::decode(lease_record)?;
    let mut bytes = [0_u8; DISPLAY_WORKER_LEASE_ACK_BYTES];
    bytes[..8].copy_from_slice(&ACK_MAGIC);
    put_u16(&mut bytes, 8, VERSION);
    put_u16(&mut bytes, 10, DISPLAY_WORKER_LEASE_ACK_BYTES as u16);
    put_u64(&mut bytes, 16, lease.request_id);
    put_u32(&mut bytes, 24, lease.windows_session_id.get());
    bytes[32..64].copy_from_slice(&Sha256::digest(lease_record));
    Ok(bytes)
}

pub fn verify_display_worker_lease_ack(
    lease_record: &[u8; DISPLAY_WORKER_LEASE_BYTES],
    acknowledgement: &[u8; DISPLAY_WORKER_LEASE_ACK_BYTES],
) -> Result<(), DisplayWorkerProtocolError> {
    if acknowledgement[..8] != ACK_MAGIC
        || read_u16(acknowledgement, 8) != VERSION
        || usize::from(read_u16(acknowledgement, 10)) != DISPLAY_WORKER_LEASE_ACK_BYTES
        || acknowledgement[12..16] != [0; 4]
        || acknowledgement[28..32] != [0; 4]
    {
        return Err(DisplayWorkerProtocolError::InvalidAcknowledgement);
    }
    let expected = encode_display_worker_lease_ack(lease_record)?;
    if acknowledgement == &expected {
        Ok(())
    } else {
        Err(DisplayWorkerProtocolError::InvalidAcknowledgement)
    }
}

/// Fixed secret-bearing record sent only after the exact worker has
/// acknowledged the complete non-secret lease. Deliberately neither `Clone`
/// nor `Debug`; its storage is wiped on every drop path.
pub struct DisplayWorkerStartRecord([u8; DISPLAY_WORKER_START_BYTES]);

impl DisplayWorkerStartRecord {
    pub(crate) fn new(
        lease: &DisplayWorkerLease,
        adapter_instance_id: [u8; 16],
        display_capability_id: [u8; 16],
        material: &EngineLaunchMaterialV1,
    ) -> Result<Self, DisplayWorkerProtocolError> {
        if adapter_instance_id != lease.adapter_instance_id
            || display_capability_id != lease.display_capability_id
        {
            return Err(DisplayWorkerProtocolError::StartLeaseMismatch);
        }
        if material.validate().is_err() {
            return Err(DisplayWorkerProtocolError::InvalidStartSecret);
        }
        let lease_record = lease.encode();
        let mut bytes = [0_u8; DISPLAY_WORKER_START_BYTES];
        bytes[..8].copy_from_slice(&START_MAGIC);
        put_u16(&mut bytes, 8, VERSION);
        put_u16(&mut bytes, 10, DISPLAY_WORKER_START_BYTES as u16);
        put_u64(&mut bytes, 16, lease.request_id);
        put_u32(&mut bytes, 24, lease.windows_session_id.get());
        bytes[32..64].copy_from_slice(&Sha256::digest(lease_record));
        bytes[64..80].copy_from_slice(&adapter_instance_id);
        bytes[80..96].copy_from_slice(&display_capability_id);
        put_u32(&mut bytes, 96, material.launch_session_id);
        bytes[100..116].copy_from_slice(&material.key);
        put_u32(&mut bytes, 116, material.key_id);
        bytes[120..128].copy_from_slice(&material.ping_payload);
        put_u32(&mut bytes, 128, material.control_connect_data);
        Ok(Self(bytes))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; DISPLAY_WORKER_START_BYTES] {
        &self.0
    }

    /// Moves bytes received from the pipe into the owned, wiping record and
    /// explicitly clears the caller's receive buffer before returning.
    #[must_use]
    pub fn take_from(bytes: &mut [u8; DISPLAY_WORKER_START_BYTES]) -> Self {
        let record = Self(*bytes);
        volatile_zero(bytes);
        record
    }

    pub fn into_authenticated_launch(
        self,
        lease: &DisplayWorkerLease,
    ) -> Result<crate::AuthenticatedEngineLaunch, DisplayWorkerProtocolError> {
        self.validate_for(lease)?;
        crate::AuthenticatedEngineLaunch::new(
            copy_identity(&self.0, 64),
            copy_identity(&self.0, 80),
            read_u32(&self.0, 96),
            copy_identity(&self.0, 100),
            read_u32(&self.0, 116),
            copy_identity(&self.0, 120),
            read_u32(&self.0, 128),
        )
        .map_err(|_| DisplayWorkerProtocolError::InvalidStartSecret)
    }

    pub fn validate_for(
        &self,
        lease: &DisplayWorkerLease,
    ) -> Result<(), DisplayWorkerProtocolError> {
        if self.0[..8] != START_MAGIC
            || read_u16(&self.0, 8) != VERSION
            || usize::from(read_u16(&self.0, 10)) != DISPLAY_WORKER_START_BYTES
            || self.0[12..16] != [0; 4]
            || self.0[132..] != [0; DISPLAY_WORKER_START_BYTES - 132]
        {
            return Err(DisplayWorkerProtocolError::InvalidStartLayout);
        }
        let lease_record = lease.encode();
        if read_u64(&self.0, 16) != lease.request_id
            || read_u32(&self.0, 24) != lease.windows_session_id.get()
            || self.0[28..32] != [0; 4]
            || self.0[32..64] != Sha256::digest(lease_record)[..]
            || self.0[64..80] != lease.adapter_instance_id
            || self.0[80..96] != lease.display_capability_id
        {
            return Err(DisplayWorkerProtocolError::StartLeaseMismatch);
        }
        if read_u32(&self.0, 96) == 0 || self.0[100..116] == [0; 16] {
            return Err(DisplayWorkerProtocolError::InvalidStartSecret);
        }
        Ok(())
    }

    fn wipe(&mut self) {
        volatile_zero(&mut self.0);
    }
}

impl Drop for DisplayWorkerStartRecord {
    fn drop(&mut self) {
        self.wipe();
    }
}

/// Correlation returned only after the worker validates and moves the start
/// record into its independently wiped authenticated-launch owner.
pub struct DisplayWorkerStartAcknowledgement([u8; DISPLAY_WORKER_START_ACK_BYTES]);

impl DisplayWorkerStartAcknowledgement {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; DISPLAY_WORKER_START_ACK_BYTES] {
        &self.0
    }

    pub fn from_bytes(bytes: [u8; DISPLAY_WORKER_START_ACK_BYTES]) -> Self {
        Self(bytes)
    }

    /// Moves pipe input into an owned acknowledgement and clears the raw
    /// receive storage even when later validation fails.
    #[must_use]
    pub fn take_from(bytes: &mut [u8; DISPLAY_WORKER_START_ACK_BYTES]) -> Self {
        let acknowledgement = Self(*bytes);
        volatile_zero(bytes);
        acknowledgement
    }

    fn wipe(&mut self) {
        volatile_zero(&mut self.0);
    }
}

impl Drop for DisplayWorkerStartAcknowledgement {
    fn drop(&mut self) {
        self.wipe();
    }
}

pub fn acknowledge_display_worker_start(
    record: DisplayWorkerStartRecord,
    lease: &DisplayWorkerLease,
) -> Result<
    (
        crate::AuthenticatedEngineLaunch,
        DisplayWorkerStartAcknowledgement,
    ),
    DisplayWorkerProtocolError,
> {
    record.validate_for(lease)?;
    let mut acknowledgement = [0_u8; DISPLAY_WORKER_START_ACK_BYTES];
    acknowledgement[..8].copy_from_slice(&START_ACK_MAGIC);
    put_u16(&mut acknowledgement, 8, VERSION);
    put_u16(
        &mut acknowledgement,
        10,
        DISPLAY_WORKER_START_ACK_BYTES as u16,
    );
    put_u64(&mut acknowledgement, 16, lease.request_id);
    put_u32(&mut acknowledgement, 24, lease.windows_session_id.get());
    acknowledgement[32..64].copy_from_slice(&Sha256::digest(record.as_bytes()));
    let launch = record.into_authenticated_launch(lease)?;
    Ok((launch, DisplayWorkerStartAcknowledgement(acknowledgement)))
}

pub fn verify_display_worker_start_ack(
    record: &DisplayWorkerStartRecord,
    lease: &DisplayWorkerLease,
    acknowledgement: &DisplayWorkerStartAcknowledgement,
) -> Result<(), DisplayWorkerProtocolError> {
    record.validate_for(lease)?;
    if acknowledgement.0[..8] != START_ACK_MAGIC
        || read_u16(&acknowledgement.0, 8) != VERSION
        || usize::from(read_u16(&acknowledgement.0, 10)) != DISPLAY_WORKER_START_ACK_BYTES
        || acknowledgement.0[12..16] != [0; 4]
        || read_u64(&acknowledgement.0, 16) != lease.request_id
        || read_u32(&acknowledgement.0, 24) != lease.windows_session_id.get()
        || acknowledgement.0[28..32] != [0; 4]
        || acknowledgement.0[32..64] != Sha256::digest(record.as_bytes())[..]
    {
        return Err(DisplayWorkerProtocolError::InvalidStartAcknowledgement);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum DisplayWorkerControlCommand {
    RequestKeyFrame = 1,
    GracefulStop = 2,
}

impl DisplayWorkerControlCommand {
    fn from_raw(value: u16) -> Result<Self, DisplayWorkerProtocolError> {
        match value {
            1 => Ok(Self::RequestKeyFrame),
            2 => Ok(Self::GracefulStop),
            _ => Err(DisplayWorkerProtocolError::InvalidControlCommand),
        }
    }
}

/// Fixed post-start command bound to the exact worker lease. This record is
/// intentionally small and contains no variable-length or secret material.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayWorkerControlRecord([u8; DISPLAY_WORKER_CONTROL_BYTES]);

impl DisplayWorkerControlRecord {
    pub fn new(
        lease: &DisplayWorkerLease,
        command: DisplayWorkerControlCommand,
        sequence: u32,
    ) -> Result<Self, DisplayWorkerProtocolError> {
        if sequence == 0 {
            return Err(DisplayWorkerProtocolError::InvalidControlSequence);
        }
        let mut bytes = [0_u8; DISPLAY_WORKER_CONTROL_BYTES];
        bytes[..8].copy_from_slice(&CONTROL_MAGIC);
        put_u16(&mut bytes, 8, VERSION);
        put_u16(&mut bytes, 10, DISPLAY_WORKER_CONTROL_BYTES as u16);
        put_u16(&mut bytes, 12, command as u16);
        put_u64(&mut bytes, 16, lease.request_id);
        put_u32(&mut bytes, 24, lease.windows_session_id.get());
        put_u32(&mut bytes, 28, sequence);
        bytes[32..64].copy_from_slice(&Sha256::digest(lease.encode()));
        Ok(Self(bytes))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; DISPLAY_WORKER_CONTROL_BYTES] {
        &self.0
    }

    #[must_use]
    pub fn from_bytes(bytes: [u8; DISPLAY_WORKER_CONTROL_BYTES]) -> Self {
        Self(bytes)
    }

    pub fn decode_for(
        &self,
        lease: &DisplayWorkerLease,
    ) -> Result<(DisplayWorkerControlCommand, u32), DisplayWorkerProtocolError> {
        if self.0[..8] != CONTROL_MAGIC
            || read_u16(&self.0, 8) != VERSION
            || usize::from(read_u16(&self.0, 10)) != DISPLAY_WORKER_CONTROL_BYTES
            || self.0[14..16] != [0; 2]
        {
            return Err(DisplayWorkerProtocolError::InvalidControlLayout);
        }
        let command = DisplayWorkerControlCommand::from_raw(read_u16(&self.0, 12))?;
        let sequence = read_u32(&self.0, 28);
        if sequence == 0 {
            return Err(DisplayWorkerProtocolError::InvalidControlSequence);
        }
        if read_u64(&self.0, 16) != lease.request_id
            || read_u32(&self.0, 24) != lease.windows_session_id.get()
            || self.0[32..64] != Sha256::digest(lease.encode())[..]
        {
            return Err(DisplayWorkerProtocolError::ControlLeaseMismatch);
        }
        Ok((command, sequence))
    }
}

pub fn encode_display_worker_control_ack(
    record: &DisplayWorkerControlRecord,
    lease: &DisplayWorkerLease,
) -> Result<[u8; DISPLAY_WORKER_CONTROL_ACK_BYTES], DisplayWorkerProtocolError> {
    let (command, sequence) = record.decode_for(lease)?;
    let mut acknowledgement = [0_u8; DISPLAY_WORKER_CONTROL_ACK_BYTES];
    acknowledgement[..8].copy_from_slice(&CONTROL_ACK_MAGIC);
    put_u16(&mut acknowledgement, 8, VERSION);
    put_u16(
        &mut acknowledgement,
        10,
        DISPLAY_WORKER_CONTROL_ACK_BYTES as u16,
    );
    put_u16(&mut acknowledgement, 12, command as u16);
    put_u64(&mut acknowledgement, 16, lease.request_id);
    put_u32(&mut acknowledgement, 24, lease.windows_session_id.get());
    put_u32(&mut acknowledgement, 28, sequence);
    acknowledgement[32..64].copy_from_slice(&Sha256::digest(record.as_bytes()));
    Ok(acknowledgement)
}

pub fn verify_display_worker_control_ack(
    record: &DisplayWorkerControlRecord,
    lease: &DisplayWorkerLease,
    acknowledgement: &[u8; DISPLAY_WORKER_CONTROL_ACK_BYTES],
) -> Result<(), DisplayWorkerProtocolError> {
    let (command, sequence) = record.decode_for(lease)?;
    if acknowledgement[..8] != CONTROL_ACK_MAGIC
        || read_u16(acknowledgement, 8) != VERSION
        || usize::from(read_u16(acknowledgement, 10)) != DISPLAY_WORKER_CONTROL_ACK_BYTES
        || read_u16(acknowledgement, 12) != command as u16
        || acknowledgement[14..16] != [0; 2]
        || read_u64(acknowledgement, 16) != lease.request_id
        || read_u32(acknowledgement, 24) != lease.windows_session_id.get()
        || read_u32(acknowledgement, 28) != sequence
        || acknowledgement[32..64] != Sha256::digest(record.as_bytes())[..]
    {
        return Err(DisplayWorkerProtocolError::InvalidControlAcknowledgement);
    }
    Ok(())
}

fn volatile_zero<const N: usize>(bytes: &mut [u8; N]) {
    // SAFETY: every write stays within the exclusively borrowed byte array.
    unsafe {
        for byte in bytes {
            ptr::write_volatile(byte, 0);
        }
    }
    compiler_fence(Ordering::SeqCst);
}

fn copy_identity<const N: usize>(bytes: &[u8], offset: usize) -> [u8; N] {
    bytes[offset..offset + N]
        .try_into()
        .expect("fixed record slice")
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
            .expect("fixed record slice"),
    )
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("fixed record slice"),
    )
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("fixed record slice"),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayWorkerProtocolError {
    InvalidLayout,
    InvalidLease,
    InvalidAcknowledgement,
    InvalidStartLayout,
    StartLeaseMismatch,
    InvalidStartSecret,
    InvalidStartAcknowledgement,
    InvalidControlLayout,
    InvalidControlCommand,
    InvalidControlSequence,
    ControlLeaseMismatch,
    InvalidControlAcknowledgement,
}

impl fmt::Display for DisplayWorkerProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLayout => "display worker lease layout is invalid",
            Self::InvalidLease => "display worker lease is invalid",
            Self::InvalidAcknowledgement => "display worker lease acknowledgement is invalid",
            Self::InvalidStartLayout => "display worker start layout is invalid",
            Self::StartLeaseMismatch => "display worker start does not match acknowledged lease",
            Self::InvalidStartSecret => "display worker start secret is invalid",
            Self::InvalidStartAcknowledgement => "display worker start acknowledgement is invalid",
            Self::InvalidControlLayout => "display worker control layout is invalid",
            Self::InvalidControlCommand => "display worker control command is invalid",
            Self::InvalidControlSequence => "display worker control sequence is invalid",
            Self::ControlLeaseMismatch => "display worker control does not match the active lease",
            Self::InvalidControlAcknowledgement => {
                "display worker control acknowledgement is invalid"
            }
        })
    }
}

impl std::error::Error for DisplayWorkerProtocolError {}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_display_stream_adapter::{
        DisplayMode, DisplayOnlyPolicy, DisplayStreamSessionRequest, DynamicRange, VideoCodec,
    };
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
                session_id: [4; 16],
                route_id: [5; 16],
                stream_id: [6; 16],
                stream_epoch: 8,
                host_ipv4: Ipv4Addr::LOCALHOST,
            },
        )
        .unwrap()
    }

    #[test]
    fn lease_layout_is_exact_closed_and_round_trips() {
        let lease = lease();
        let record = lease.encode();
        assert_eq!(record.len(), DISPLAY_WORKER_LEASE_BYTES);
        assert_eq!(&record[..12], b"CPYDW001\0\x01\0x");
        assert_eq!(&record[12..16], &[0; 4]);
        assert_eq!(&record[16..24], &7_u64.to_be_bytes());
        assert_eq!(&record[24..28], &3_u32.to_be_bytes());
        assert_eq!(&record[112..120], &8_u64.to_be_bytes());
        assert_eq!(DisplayWorkerLease::decode(&record).unwrap(), lease);
    }

    #[test]
    fn lease_rejects_unknown_layout_nil_identity_and_session_zero() {
        let mut record = lease().encode();
        record[12] = 1;
        assert_eq!(
            DisplayWorkerLease::decode(&record),
            Err(DisplayWorkerProtocolError::InvalidLayout)
        );
        record = lease().encode();
        record[32..48].fill(0);
        assert_eq!(
            DisplayWorkerLease::decode(&record),
            Err(DisplayWorkerProtocolError::InvalidLease)
        );
        record = lease().encode();
        record[24..28].fill(0);
        assert_eq!(
            DisplayWorkerLease::decode(&record),
            Err(DisplayWorkerProtocolError::InvalidLease)
        );
    }

    #[test]
    fn acknowledgement_is_bound_to_every_lease_byte() {
        let record = lease().encode();
        let acknowledgement = encode_display_worker_lease_ack(&record).unwrap();
        verify_display_worker_lease_ack(&record, &acknowledgement).unwrap();

        let mut changed_record = record;
        changed_record[111] ^= 1;
        assert_eq!(
            verify_display_worker_lease_ack(&changed_record, &acknowledgement),
            Err(DisplayWorkerProtocolError::InvalidAcknowledgement)
        );
        let mut changed_acknowledgement = acknowledgement;
        changed_acknowledgement[63] ^= 1;
        assert_eq!(
            verify_display_worker_lease_ack(&record, &changed_acknowledgement),
            Err(DisplayWorkerProtocolError::InvalidAcknowledgement)
        );
    }

    fn start_record() -> DisplayWorkerStartRecord {
        crate::AuthenticatedEngineLaunch::new([1; 16], [2; 16], 17, [0x42; 16], 18, [0x24; 8], 19)
            .unwrap()
            .into_worker_start_record(&lease())
            .unwrap()
    }

    #[test]
    fn secret_start_is_fixed_lease_bound_and_round_trips_once() {
        let lease = lease();
        let record = start_record();
        assert_eq!(record.0.len(), DISPLAY_WORKER_START_BYTES);
        assert_eq!(&record.0[..12], b"CPYDWS01\0\x01\0\xa0");
        assert_eq!(&record.0[16..24], &lease.request_id.to_be_bytes());
        assert_eq!(
            &record.0[24..28],
            &lease.windows_session_id.get().to_be_bytes()
        );
        assert_eq!(&record.0[64..80], &[1; 16]);
        assert_eq!(&record.0[80..96], &[2; 16]);
        assert_eq!(&record.0[96..100], &17_u32.to_be_bytes());
        assert_eq!(&record.0[100..116], &[0x42; 16]);
        assert_eq!(&record.0[132..], &[0; 28]);

        let record_for_ack = start_record();
        let (launch, acknowledgement) = acknowledge_display_worker_start(record, &lease).unwrap();
        launch.validate_for(&session()).unwrap();
        verify_display_worker_start_ack(&record_for_ack, &lease, &acknowledgement).unwrap();

        let mut changed_lease = lease;
        changed_lease.stream_epoch += 1;
        assert_eq!(
            record_for_ack.validate_for(&changed_lease),
            Err(DisplayWorkerProtocolError::StartLeaseMismatch)
        );
    }

    #[test]
    fn secret_start_rejects_layout_identity_secret_and_ack_changes() {
        let lease = lease();
        assert_eq!(
            DisplayWorkerStartRecord::new(
                &lease,
                [9; 16],
                lease.display_capability_id,
                &EngineLaunchMaterialV1::new(1, [2; 16], 3, [4; 8], 5),
            )
            .err(),
            Some(DisplayWorkerProtocolError::StartLeaseMismatch)
        );
        assert_eq!(
            DisplayWorkerStartRecord::new(
                &lease,
                lease.adapter_instance_id,
                lease.display_capability_id,
                &EngineLaunchMaterialV1::new(1, [0; 16], 3, [4; 8], 5),
            )
            .err(),
            Some(DisplayWorkerProtocolError::InvalidStartSecret)
        );

        let mut bad_layout = start_record();
        bad_layout.0[159] = 1;
        assert_eq!(
            bad_layout.validate_for(&lease),
            Err(DisplayWorkerProtocolError::InvalidStartLayout)
        );

        let record = start_record();
        let (_, acknowledgement) =
            acknowledge_display_worker_start(start_record(), &lease).unwrap();
        let mut changed_ack =
            DisplayWorkerStartAcknowledgement::from_bytes(*acknowledgement.as_bytes());
        changed_ack.0[63] ^= 1;
        assert_eq!(
            verify_display_worker_start_ack(&record, &lease, &changed_ack),
            Err(DisplayWorkerProtocolError::InvalidStartAcknowledgement)
        );
    }

    #[test]
    fn secret_start_and_ack_storage_have_explicit_zeroization() {
        let lease = lease();
        let mut record = start_record();
        assert_ne!(record.0, [0; DISPLAY_WORKER_START_BYTES]);
        record.wipe();
        assert_eq!(record.0, [0; DISPLAY_WORKER_START_BYTES]);

        let (_, mut acknowledgement) =
            acknowledge_display_worker_start(start_record(), &lease).unwrap();
        assert_ne!(acknowledgement.0, [0; DISPLAY_WORKER_START_ACK_BYTES]);
        acknowledgement.wipe();
        assert_eq!(acknowledgement.0, [0; DISPLAY_WORKER_START_ACK_BYTES]);

        let mut raw_start = *start_record().as_bytes();
        let owned_start = DisplayWorkerStartRecord::take_from(&mut raw_start);
        assert_eq!(raw_start, [0; DISPLAY_WORKER_START_BYTES]);
        assert_ne!(owned_start.0, [0; DISPLAY_WORKER_START_BYTES]);

        let (_, acknowledgement) =
            acknowledge_display_worker_start(start_record(), &lease).unwrap();
        let mut raw_ack = *acknowledgement.as_bytes();
        let owned_ack = DisplayWorkerStartAcknowledgement::take_from(&mut raw_ack);
        assert_eq!(raw_ack, [0; DISPLAY_WORKER_START_ACK_BYTES]);
        assert_ne!(owned_ack.0, [0; DISPLAY_WORKER_START_ACK_BYTES]);
    }

    #[test]
    fn control_records_are_fixed_lease_bound_and_acknowledged() {
        let lease = lease();
        for (command, sequence) in [
            (DisplayWorkerControlCommand::RequestKeyFrame, 1),
            (DisplayWorkerControlCommand::GracefulStop, 2),
        ] {
            let record = DisplayWorkerControlRecord::new(&lease, command, sequence).unwrap();
            assert_eq!(record.as_bytes().len(), DISPLAY_WORKER_CONTROL_BYTES);
            assert_eq!(&record.as_bytes()[..8], b"CPYDWC01");
            assert_eq!(record.decode_for(&lease).unwrap(), (command, sequence));
            let acknowledgement = encode_display_worker_control_ack(&record, &lease).unwrap();
            verify_display_worker_control_ack(&record, &lease, &acknowledgement).unwrap();
        }
    }

    #[test]
    fn control_records_reject_replay_unknown_commands_and_mutation() {
        let lease = lease();
        assert_eq!(
            DisplayWorkerControlRecord::new(
                &lease,
                DisplayWorkerControlCommand::RequestKeyFrame,
                0,
            ),
            Err(DisplayWorkerProtocolError::InvalidControlSequence)
        );
        let record = DisplayWorkerControlRecord::new(
            &lease,
            DisplayWorkerControlCommand::RequestKeyFrame,
            1,
        )
        .unwrap();
        let acknowledgement = encode_display_worker_control_ack(&record, &lease).unwrap();

        let mut changed_lease = lease;
        changed_lease.stream_epoch += 1;
        assert_eq!(
            record.decode_for(&changed_lease),
            Err(DisplayWorkerProtocolError::ControlLeaseMismatch)
        );
        let mut unknown = *record.as_bytes();
        unknown[12..14].copy_from_slice(&99_u16.to_be_bytes());
        assert_eq!(
            DisplayWorkerControlRecord::from_bytes(unknown).decode_for(&lease),
            Err(DisplayWorkerProtocolError::InvalidControlCommand)
        );
        let mut changed_ack = acknowledgement;
        changed_ack[63] ^= 1;
        assert_eq!(
            verify_display_worker_control_ack(&record, &lease, &changed_ack),
            Err(DisplayWorkerProtocolError::InvalidControlAcknowledgement)
        );
    }
}
