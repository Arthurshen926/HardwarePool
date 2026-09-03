use std::{error::Error, fmt};

use crate::{
    PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES, PRIVATE_TOUCHPAD_PACKET_MAGIC,
    PRIVATE_TOUCHPAD_PACKET_MAX_BYTES, PRIVATE_TOUCHPAD_PACKET_RECORD_BYTES,
    PRIVATE_TOUCHPAD_PACKET_VERSION, PrivateTouchpadPacketV1, PrivateTouchpadRouteBinding,
};

pub const PRIVATE_TOUCHPAD_TRANSPORT_MAGIC: [u8; 4] = *b"CPTR";
pub const PRIVATE_TOUCHPAD_TRANSPORT_VERSION: u8 = 1;
pub const PRIVATE_TOUCHPAD_TRANSPORT_HEADER_BYTES: usize = 24;
pub const PRIVATE_TOUCHPAD_TRANSPORT_HELLO_BYTES: usize = 160;
pub const PRIVATE_TOUCHPAD_TRANSPORT_ACK_BYTES: usize = PRIVATE_TOUCHPAD_TRANSPORT_HEADER_BYTES;
pub const PRIVATE_TOUCHPAD_TRANSPORT_CLOSE_BYTES: usize = PRIVATE_TOUCHPAD_TRANSPORT_HEADER_BYTES;
pub const PRIVATE_TOUCHPAD_TRANSPORT_MAX_BYTES: usize =
    PRIVATE_TOUCHPAD_TRANSPORT_HEADER_BYTES + PRIVATE_TOUCHPAD_PACKET_MAX_BYTES;

const RECORD_HELLO: u8 = 1;
const RECORD_DATA: u8 = 2;
const RECORD_ACK: u8 = 3;
const RECORD_CLOSE: u8 = 4;
const HELLO_FLAG_EXPIRY_PRESENT: u8 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateTouchpadTransportRecordV1 {
    bytes: [u8; PRIVATE_TOUCHPAD_TRANSPORT_MAX_BYTES],
    len: u16,
}

impl PrivateTouchpadTransportRecordV1 {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateTouchpadTransportPacketV1 {
    bytes: [u8; PRIVATE_TOUCHPAD_PACKET_MAX_BYTES],
    len: u16,
}

impl PrivateTouchpadTransportPacketV1 {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadTransportRecordError {
    TooShort { actual: usize, minimum: usize },
    TooLong { actual: usize, maximum: usize },
    InvalidMagic([u8; 4]),
    UnsupportedVersion(u8),
    WrongKind { expected: u8, actual: u8 },
    UnknownFlags(u8),
    NonZeroReserved(u8),
    InvalidLength { actual: usize, expected: usize },
    BindingMismatch,
    EpochMismatch { expected: u64, actual: u64 },
    SequenceMismatch { expected: u64, actual: u64 },
    InvalidPacketMagic([u8; 4]),
    UnsupportedPacketVersion(u8),
    InvalidPacketLength { actual: usize, expected: usize },
}

impl fmt::Display for PrivateTouchpadTransportRecordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { actual, minimum } => {
                write!(
                    formatter,
                    "transport record is {actual} bytes; minimum is {minimum}"
                )
            }
            Self::TooLong { actual, maximum } => {
                write!(
                    formatter,
                    "transport record is {actual} bytes; maximum is {maximum}"
                )
            }
            Self::InvalidMagic(actual) => {
                write!(formatter, "invalid transport magic {actual:02x?}")
            }
            Self::UnsupportedVersion(actual) => {
                write!(formatter, "unsupported transport record version {actual}")
            }
            Self::WrongKind { expected, actual } => {
                write!(
                    formatter,
                    "expected transport record kind {expected}, got {actual}"
                )
            }
            Self::UnknownFlags(actual) => {
                write!(formatter, "unknown transport flags 0x{actual:02x}")
            }
            Self::NonZeroReserved(actual) => {
                write!(formatter, "transport reserved byte is 0x{actual:02x}")
            }
            Self::InvalidLength { actual, expected } => {
                write!(
                    formatter,
                    "transport record is {actual} bytes; expected {expected}"
                )
            }
            Self::BindingMismatch => formatter.write_str("transport Hello binding mismatch"),
            Self::EpochMismatch { expected, actual } => {
                write!(
                    formatter,
                    "transport epoch {actual} does not match {expected}"
                )
            }
            Self::SequenceMismatch { expected, actual } => {
                write!(
                    formatter,
                    "transport sequence {actual} does not match {expected}"
                )
            }
            Self::InvalidPacketMagic(actual) => {
                write!(formatter, "invalid embedded packet magic {actual:02x?}")
            }
            Self::UnsupportedPacketVersion(actual) => {
                write!(formatter, "unsupported embedded packet version {actual}")
            }
            Self::InvalidPacketLength { actual, expected } => {
                write!(
                    formatter,
                    "embedded packet is {actual} bytes; expected {expected}"
                )
            }
        }
    }
}

impl Error for PrivateTouchpadTransportRecordError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateTouchpadTransportCodecV1 {
    binding: PrivateTouchpadRouteBinding,
}

impl PrivateTouchpadTransportCodecV1 {
    #[must_use]
    pub const fn new(binding: PrivateTouchpadRouteBinding) -> Self {
        Self { binding }
    }

    #[must_use]
    pub const fn binding(&self) -> &PrivateTouchpadRouteBinding {
        &self.binding
    }

    #[must_use]
    pub fn encode_hello(&self) -> PrivateTouchpadTransportRecordV1 {
        let mut record = self.record(
            RECORD_HELLO,
            self.binding.route_epoch,
            0,
            PRIVATE_TOUCHPAD_TRANSPORT_HELLO_BYTES,
        );
        record.bytes[6] = if self.binding.authorization_expires_at_ms.is_some() {
            HELLO_FLAG_EXPIRY_PRESENT
        } else {
            0
        };
        write_uuid(
            &mut record.bytes,
            24,
            self.binding.route_id.as_uuid().as_bytes(),
        );
        write_uuid(
            &mut record.bytes,
            40,
            self.binding.session_id.as_uuid().as_bytes(),
        );
        write_port(&mut record.bytes, 56, self.binding.source);
        write_port(&mut record.bytes, 104, self.binding.sink);
        write_u64(
            &mut record.bytes,
            152,
            self.binding.authorization_expires_at_ms.unwrap_or(0),
        );
        record
    }

    pub fn validate_hello(&self, bytes: &[u8]) -> Result<(), PrivateTouchpadTransportRecordError> {
        self.validate_header(bytes, RECORD_HELLO, HELLO_FLAG_EXPIRY_PRESENT)?;
        require_len(bytes, PRIVATE_TOUCHPAD_TRANSPORT_HELLO_BYTES)?;
        if bytes != self.encode_hello().as_bytes() {
            return Err(PrivateTouchpadTransportRecordError::BindingMismatch);
        }
        Ok(())
    }

    pub fn encode_data(
        &self,
        packet: &PrivateTouchpadPacketV1,
    ) -> Result<PrivateTouchpadTransportRecordV1, PrivateTouchpadTransportRecordError> {
        if packet.stream_epoch() != self.binding.route_epoch {
            return Err(PrivateTouchpadTransportRecordError::EpochMismatch {
                expected: self.binding.route_epoch,
                actual: packet.stream_epoch(),
            });
        }
        let mut record = self.record(
            RECORD_DATA,
            packet.stream_epoch(),
            packet.sequence(),
            PRIVATE_TOUCHPAD_TRANSPORT_HEADER_BYTES + packet.len(),
        );
        let record_len = record.len();
        record.bytes[PRIVATE_TOUCHPAD_TRANSPORT_HEADER_BYTES..record_len]
            .copy_from_slice(packet.as_bytes());
        Ok(record)
    }

    pub fn decode_data(
        &self,
        bytes: &[u8],
    ) -> Result<PrivateTouchpadTransportPacketV1, PrivateTouchpadTransportRecordError> {
        let (epoch, sequence) = self.validate_header(bytes, RECORD_DATA, 0)?;
        if epoch != self.binding.route_epoch {
            return Err(PrivateTouchpadTransportRecordError::EpochMismatch {
                expected: self.binding.route_epoch,
                actual: epoch,
            });
        }
        let minimum =
            PRIVATE_TOUCHPAD_TRANSPORT_HEADER_BYTES + PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES;
        if bytes.len() < minimum {
            return Err(PrivateTouchpadTransportRecordError::TooShort {
                actual: bytes.len(),
                minimum,
            });
        }
        let packet = &bytes[PRIVATE_TOUCHPAD_TRANSPORT_HEADER_BYTES..];
        if packet.len() > PRIVATE_TOUCHPAD_PACKET_MAX_BYTES {
            return Err(PrivateTouchpadTransportRecordError::TooLong {
                actual: bytes.len(),
                maximum: PRIVATE_TOUCHPAD_TRANSPORT_MAX_BYTES,
            });
        }
        let magic = [packet[0], packet[1], packet[2], packet[3]];
        if magic != PRIVATE_TOUCHPAD_PACKET_MAGIC {
            return Err(PrivateTouchpadTransportRecordError::InvalidPacketMagic(
                magic,
            ));
        }
        if packet[4] != PRIVATE_TOUCHPAD_PACKET_VERSION {
            return Err(PrivateTouchpadTransportRecordError::UnsupportedPacketVersion(packet[4]));
        }
        let expected_packet_len = PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES
            + usize::from(packet[7]) * PRIVATE_TOUCHPAD_PACKET_RECORD_BYTES;
        if packet.len() != expected_packet_len {
            return Err(PrivateTouchpadTransportRecordError::InvalidPacketLength {
                actual: packet.len(),
                expected: expected_packet_len,
            });
        }
        let packet_epoch = read_u64(packet, 8);
        if packet_epoch != epoch {
            return Err(PrivateTouchpadTransportRecordError::EpochMismatch {
                expected: epoch,
                actual: packet_epoch,
            });
        }
        let packet_sequence = read_u64(packet, 16);
        if packet_sequence != sequence {
            return Err(PrivateTouchpadTransportRecordError::SequenceMismatch {
                expected: sequence,
                actual: packet_sequence,
            });
        }
        let mut decoded = PrivateTouchpadTransportPacketV1 {
            bytes: [0; PRIVATE_TOUCHPAD_PACKET_MAX_BYTES],
            len: packet.len() as u16,
        };
        decoded.bytes[..packet.len()].copy_from_slice(packet);
        Ok(decoded)
    }

    #[must_use]
    pub fn encode_ack(&self, sequence: u64) -> PrivateTouchpadTransportRecordV1 {
        self.record(
            RECORD_ACK,
            self.binding.route_epoch,
            sequence,
            PRIVATE_TOUCHPAD_TRANSPORT_ACK_BYTES,
        )
    }

    pub fn validate_ack(
        &self,
        bytes: &[u8],
        expected_sequence: u64,
    ) -> Result<(), PrivateTouchpadTransportRecordError> {
        let (epoch, sequence) = self.validate_header(bytes, RECORD_ACK, 0)?;
        require_len(bytes, PRIVATE_TOUCHPAD_TRANSPORT_ACK_BYTES)?;
        if epoch != self.binding.route_epoch {
            return Err(PrivateTouchpadTransportRecordError::EpochMismatch {
                expected: self.binding.route_epoch,
                actual: epoch,
            });
        }
        if sequence != expected_sequence {
            return Err(PrivateTouchpadTransportRecordError::SequenceMismatch {
                expected: expected_sequence,
                actual: sequence,
            });
        }
        Ok(())
    }

    #[must_use]
    pub fn encode_close(&self) -> PrivateTouchpadTransportRecordV1 {
        self.record(
            RECORD_CLOSE,
            self.binding.route_epoch,
            0,
            PRIVATE_TOUCHPAD_TRANSPORT_CLOSE_BYTES,
        )
    }

    pub fn validate_close(&self, bytes: &[u8]) -> Result<(), PrivateTouchpadTransportRecordError> {
        let (epoch, sequence) = self.validate_header(bytes, RECORD_CLOSE, 0)?;
        require_len(bytes, PRIVATE_TOUCHPAD_TRANSPORT_CLOSE_BYTES)?;
        if epoch != self.binding.route_epoch {
            return Err(PrivateTouchpadTransportRecordError::EpochMismatch {
                expected: self.binding.route_epoch,
                actual: epoch,
            });
        }
        if sequence != 0 {
            return Err(PrivateTouchpadTransportRecordError::SequenceMismatch {
                expected: 0,
                actual: sequence,
            });
        }
        Ok(())
    }

    fn record(
        &self,
        kind: u8,
        epoch: u64,
        sequence: u64,
        len: usize,
    ) -> PrivateTouchpadTransportRecordV1 {
        let mut record = PrivateTouchpadTransportRecordV1 {
            bytes: [0; PRIVATE_TOUCHPAD_TRANSPORT_MAX_BYTES],
            len: len as u16,
        };
        record.bytes[..4].copy_from_slice(&PRIVATE_TOUCHPAD_TRANSPORT_MAGIC);
        record.bytes[4] = PRIVATE_TOUCHPAD_TRANSPORT_VERSION;
        record.bytes[5] = kind;
        write_u64(&mut record.bytes, 8, epoch);
        write_u64(&mut record.bytes, 16, sequence);
        record
    }

    fn validate_header(
        &self,
        bytes: &[u8],
        expected_kind: u8,
        allowed_flags: u8,
    ) -> Result<(u64, u64), PrivateTouchpadTransportRecordError> {
        if bytes.len() < PRIVATE_TOUCHPAD_TRANSPORT_HEADER_BYTES {
            return Err(PrivateTouchpadTransportRecordError::TooShort {
                actual: bytes.len(),
                minimum: PRIVATE_TOUCHPAD_TRANSPORT_HEADER_BYTES,
            });
        }
        if bytes.len() > PRIVATE_TOUCHPAD_TRANSPORT_MAX_BYTES {
            return Err(PrivateTouchpadTransportRecordError::TooLong {
                actual: bytes.len(),
                maximum: PRIVATE_TOUCHPAD_TRANSPORT_MAX_BYTES,
            });
        }
        let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
        if magic != PRIVATE_TOUCHPAD_TRANSPORT_MAGIC {
            return Err(PrivateTouchpadTransportRecordError::InvalidMagic(magic));
        }
        if bytes[4] != PRIVATE_TOUCHPAD_TRANSPORT_VERSION {
            return Err(PrivateTouchpadTransportRecordError::UnsupportedVersion(
                bytes[4],
            ));
        }
        if bytes[5] != expected_kind {
            return Err(PrivateTouchpadTransportRecordError::WrongKind {
                expected: expected_kind,
                actual: bytes[5],
            });
        }
        if bytes[6] & !allowed_flags != 0 {
            return Err(PrivateTouchpadTransportRecordError::UnknownFlags(bytes[6]));
        }
        if bytes[7] != 0 {
            return Err(PrivateTouchpadTransportRecordError::NonZeroReserved(
                bytes[7],
            ));
        }
        Ok((read_u64(bytes, 8), read_u64(bytes, 16)))
    }
}

fn require_len(bytes: &[u8], expected: usize) -> Result<(), PrivateTouchpadTransportRecordError> {
    if bytes.len() != expected {
        return Err(PrivateTouchpadTransportRecordError::InvalidLength {
            actual: bytes.len(),
            expected,
        });
    }
    Ok(())
}

fn write_port(bytes: &mut [u8], offset: usize, port: capyio_core::PortRef) {
    write_uuid(bytes, offset, port.node_id.as_uuid().as_bytes());
    write_uuid(bytes, offset + 16, port.capability_id.as_uuid().as_bytes());
    write_uuid(bytes, offset + 32, port.port_id.as_uuid().as_bytes());
}

fn write_uuid(bytes: &mut [u8], offset: usize, value: &[u8; 16]) {
    bytes[offset..offset + 16].copy_from_slice(value);
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("fixed u64 field"),
    )
}
