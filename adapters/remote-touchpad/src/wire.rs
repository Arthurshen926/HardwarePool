use std::{error::Error, fmt};

use capyio_input::{
    InputContractError, InputFrameHeader, InputStreamDescriptor, NormalizedMagnitude,
    TouchpadButtonState, TouchpadContact, TouchpadContactSize, TouchpadDescriptor, TouchpadFrame,
    TouchpadFrameKind, TouchpadPosition,
};

pub const PRIVATE_TOUCHPAD_PACKET_MAGIC: [u8; 4] = *b"CPTP";
pub const PRIVATE_TOUCHPAD_PACKET_VERSION: u8 = 1;
pub const PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES: usize = 32;
pub const PRIVATE_TOUCHPAD_PACKET_RECORD_BYTES: usize = 24;
pub const PRIVATE_TOUCHPAD_PACKET_MAX_BYTES: usize =
    PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES + 5 * PRIVATE_TOUCHPAD_PACKET_RECORD_BYTES;

const CONTACT_FLAG_CONFIDENCE: u8 = 1 << 0;
const CONTACT_FLAG_SIZE: u8 = 1 << 1;
const CONTACT_FLAG_PRESSURE: u8 = 1 << 2;
const CONTACT_FLAGS_KNOWN: u8 = CONTACT_FLAG_CONFIDENCE | CONTACT_FLAG_SIZE | CONTACT_FLAG_PRESSURE;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateTouchpadPacketV1 {
    bytes: [u8; PRIVATE_TOUCHPAD_PACKET_MAX_BYTES],
    len: u16,
}

impl PrivateTouchpadPacketV1 {
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

    #[must_use]
    pub fn stream_epoch(&self) -> u64 {
        read_u64(&self.bytes, 8)
    }

    #[must_use]
    pub fn sequence(&self) -> u64 {
        read_u64(&self.bytes, 16)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadPacketError {
    Contract(InputContractError),
    PacketTooShort { actual: usize, minimum: usize },
    PacketTooLong { actual: usize, maximum: usize },
    InvalidPacketLength { actual: usize, expected: usize },
    InvalidMagic { actual: [u8; 4] },
    UnsupportedVersion(u8),
    InvalidFrameKind(u8),
    InvalidButtonState(u8),
    ContactCountExceedsDescriptor { actual: u8, maximum: u8 },
    UnknownContactFlags { index: u8, flags: u8 },
    NonZeroReservedContactByte { index: u8, value: u8 },
    NonCanonicalContact { index: u8, field: &'static str },
}

impl fmt::Display for PrivateTouchpadPacketError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Contract(error) => error.fmt(formatter),
            Self::PacketTooShort { actual, minimum } => {
                write!(
                    formatter,
                    "touchpad packet is {actual} bytes; minimum is {minimum}"
                )
            }
            Self::PacketTooLong { actual, maximum } => {
                write!(
                    formatter,
                    "touchpad packet is {actual} bytes; maximum is {maximum}"
                )
            }
            Self::InvalidPacketLength { actual, expected } => write!(
                formatter,
                "touchpad packet is {actual} bytes; contact count requires exactly {expected}"
            ),
            Self::InvalidMagic { actual } => {
                write!(formatter, "invalid touchpad packet magic: {actual:02x?}")
            }
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported touchpad packet version: {version}")
            }
            Self::InvalidFrameKind(kind) => {
                write!(formatter, "invalid touchpad packet frame kind: {kind}")
            }
            Self::InvalidButtonState(button) => {
                write!(formatter, "invalid touchpad packet button state: {button}")
            }
            Self::ContactCountExceedsDescriptor { actual, maximum } => write!(
                formatter,
                "touchpad packet contains {actual} contacts; descriptor maximum is {maximum}"
            ),
            Self::UnknownContactFlags { index, flags } => write!(
                formatter,
                "touchpad packet contact {index} contains unknown flags 0x{flags:02x}"
            ),
            Self::NonZeroReservedContactByte { index, value } => write!(
                formatter,
                "touchpad packet contact {index} reserved byte is 0x{value:02x}"
            ),
            Self::NonCanonicalContact { index, field } => write!(
                formatter,
                "touchpad packet contact {index} has non-canonical absent {field} data"
            ),
        }
    }
}

impl Error for PrivateTouchpadPacketError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Contract(error) => Some(error),
            _ => None,
        }
    }
}

impl From<InputContractError> for PrivateTouchpadPacketError {
    fn from(error: InputContractError) -> Self {
        Self::Contract(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateTouchpadPacketCodecV1 {
    stream: InputStreamDescriptor,
    descriptor: TouchpadDescriptor,
}

impl PrivateTouchpadPacketCodecV1 {
    pub fn new(
        stream: InputStreamDescriptor,
        descriptor: TouchpadDescriptor,
    ) -> Result<Self, PrivateTouchpadPacketError> {
        stream.validate()?;
        descriptor.validate()?;
        Ok(Self { stream, descriptor })
    }

    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.stream.stream_epoch
    }

    pub fn advance_epoch(&mut self, new_epoch: u64) -> Result<(), PrivateTouchpadPacketError> {
        if new_epoch <= self.stream.stream_epoch {
            return Err(InputContractError::NonAdvancingEpoch {
                current_epoch: self.stream.stream_epoch,
                new_epoch,
            }
            .into());
        }
        self.stream.stream_epoch = new_epoch;
        Ok(())
    }

    pub fn encode(
        &self,
        frame: &TouchpadFrame,
    ) -> Result<PrivateTouchpadPacketV1, PrivateTouchpadPacketError> {
        self.validate_binding(frame.header)?;
        frame.validate(&self.descriptor)?;

        let mut packet = PrivateTouchpadPacketV1 {
            bytes: [0; PRIVATE_TOUCHPAD_PACKET_MAX_BYTES],
            len: packet_len(frame.contacts.len()) as u16,
        };
        packet.bytes[..4].copy_from_slice(&PRIVATE_TOUCHPAD_PACKET_MAGIC);
        packet.bytes[4] = PRIVATE_TOUCHPAD_PACKET_VERSION;
        packet.bytes[5] = match frame.kind {
            TouchpadFrameKind::Update => 0,
            TouchpadFrameKind::CancelAll => 1,
        };
        packet.bytes[6] = match frame.button {
            TouchpadButtonState::Released => 0,
            TouchpadButtonState::Pressed => 1,
        };
        packet.bytes[7] = frame.contacts.len() as u8;
        write_u64(&mut packet.bytes, 8, frame.header.stream_epoch);
        write_u64(&mut packet.bytes, 16, frame.header.sequence);
        write_u64(&mut packet.bytes, 24, frame.header.source_timestamp_nanos);

        for (index, contact) in frame.contacts.iter().copied().enumerate() {
            encode_contact(&mut packet.bytes, index, contact);
        }
        Ok(packet)
    }

    pub fn decode(&self, packet: &[u8]) -> Result<TouchpadFrame, PrivateTouchpadPacketError> {
        if packet.len() < PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES {
            return Err(PrivateTouchpadPacketError::PacketTooShort {
                actual: packet.len(),
                minimum: PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES,
            });
        }
        if packet.len() > PRIVATE_TOUCHPAD_PACKET_MAX_BYTES {
            return Err(PrivateTouchpadPacketError::PacketTooLong {
                actual: packet.len(),
                maximum: PRIVATE_TOUCHPAD_PACKET_MAX_BYTES,
            });
        }

        let actual_magic = [packet[0], packet[1], packet[2], packet[3]];
        if actual_magic != PRIVATE_TOUCHPAD_PACKET_MAGIC {
            return Err(PrivateTouchpadPacketError::InvalidMagic {
                actual: actual_magic,
            });
        }
        if packet[4] != PRIVATE_TOUCHPAD_PACKET_VERSION {
            return Err(PrivateTouchpadPacketError::UnsupportedVersion(packet[4]));
        }
        let kind = match packet[5] {
            0 => TouchpadFrameKind::Update,
            1 => TouchpadFrameKind::CancelAll,
            actual => return Err(PrivateTouchpadPacketError::InvalidFrameKind(actual)),
        };
        let button = match packet[6] {
            0 => TouchpadButtonState::Released,
            1 => TouchpadButtonState::Pressed,
            actual => return Err(PrivateTouchpadPacketError::InvalidButtonState(actual)),
        };
        let contact_count = packet[7];
        if contact_count > self.descriptor.max_contacts {
            return Err(PrivateTouchpadPacketError::ContactCountExceedsDescriptor {
                actual: contact_count,
                maximum: self.descriptor.max_contacts,
            });
        }
        let expected_len = packet_len(usize::from(contact_count));
        if packet.len() != expected_len {
            return Err(PrivateTouchpadPacketError::InvalidPacketLength {
                actual: packet.len(),
                expected: expected_len,
            });
        }

        let epoch = read_u64(packet, 8);
        self.validate_epoch(epoch)?;
        let mut contacts = Vec::with_capacity(usize::from(contact_count));
        for index in 0..contact_count {
            contacts.push(decode_contact(packet, index)?);
        }
        let frame = TouchpadFrame {
            header: InputFrameHeader {
                stream_id: self.stream.stream_id,
                stream_epoch: epoch,
                sequence: read_u64(packet, 16),
                source_timestamp_nanos: read_u64(packet, 24),
            },
            kind,
            button,
            contacts,
        };
        frame.validate(&self.descriptor)?;
        Ok(frame)
    }

    fn validate_binding(&self, header: InputFrameHeader) -> Result<(), PrivateTouchpadPacketError> {
        if header.stream_id != self.stream.stream_id {
            return Err(InputContractError::WrongStream {
                expected: self.stream.stream_id,
                actual: header.stream_id,
            }
            .into());
        }
        self.validate_epoch(header.stream_epoch)
    }

    fn validate_epoch(&self, actual: u64) -> Result<(), PrivateTouchpadPacketError> {
        if actual < self.stream.stream_epoch {
            return Err(InputContractError::StaleEpoch {
                current: self.stream.stream_epoch,
                actual,
            }
            .into());
        }
        if actual > self.stream.stream_epoch {
            return Err(InputContractError::FutureEpoch {
                current: self.stream.stream_epoch,
                actual,
            }
            .into());
        }
        Ok(())
    }
}

fn packet_len(contact_count: usize) -> usize {
    PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES + contact_count * PRIVATE_TOUCHPAD_PACKET_RECORD_BYTES
}

fn contact_offset(index: usize) -> usize {
    PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES + index * PRIVATE_TOUCHPAD_PACKET_RECORD_BYTES
}

fn encode_contact(bytes: &mut [u8], index: usize, contact: TouchpadContact) {
    let offset = contact_offset(index);
    write_u32(bytes, offset, contact.contact_id);
    write_u32(bytes, offset + 4, contact.position.x_himetric);
    write_u32(bytes, offset + 8, contact.position.y_himetric);
    let mut flags = if contact.confidence {
        CONTACT_FLAG_CONFIDENCE
    } else {
        0
    };
    if contact.size.is_some() {
        flags |= CONTACT_FLAG_SIZE;
    }
    if contact.pressure.is_some() {
        flags |= CONTACT_FLAG_PRESSURE;
    }
    bytes[offset + 12] = flags;
    bytes[offset + 13] = 0;
    write_u16(
        bytes,
        offset + 14,
        contact.pressure.map_or(0, NormalizedMagnitude::get),
    );
    let size = contact.size.unwrap_or(TouchpadContactSize {
        width_himetric: 0,
        height_himetric: 0,
    });
    write_u32(bytes, offset + 16, size.width_himetric);
    write_u32(bytes, offset + 20, size.height_himetric);
}

fn decode_contact(bytes: &[u8], index: u8) -> Result<TouchpadContact, PrivateTouchpadPacketError> {
    let offset = contact_offset(usize::from(index));
    let flags = bytes[offset + 12];
    if flags & !CONTACT_FLAGS_KNOWN != 0 {
        return Err(PrivateTouchpadPacketError::UnknownContactFlags { index, flags });
    }
    let reserved = bytes[offset + 13];
    if reserved != 0 {
        return Err(PrivateTouchpadPacketError::NonZeroReservedContactByte {
            index,
            value: reserved,
        });
    }
    let pressure_value = read_u16(bytes, offset + 14);
    let width_himetric = read_u32(bytes, offset + 16);
    let height_himetric = read_u32(bytes, offset + 20);
    let size = if flags & CONTACT_FLAG_SIZE != 0 {
        Some(TouchpadContactSize {
            width_himetric,
            height_himetric,
        })
    } else {
        if width_himetric != 0 || height_himetric != 0 {
            return Err(PrivateTouchpadPacketError::NonCanonicalContact {
                index,
                field: "contact-size",
            });
        }
        None
    };
    let pressure = if flags & CONTACT_FLAG_PRESSURE != 0 {
        Some(NormalizedMagnitude::new(pressure_value))
    } else {
        if pressure_value != 0 {
            return Err(PrivateTouchpadPacketError::NonCanonicalContact {
                index,
                field: "pressure",
            });
        }
        None
    };
    Ok(TouchpadContact {
        contact_id: read_u32(bytes, offset),
        position: TouchpadPosition {
            x_himetric: read_u32(bytes, offset + 4),
            y_himetric: read_u32(bytes, offset + 8),
        },
        confidence: flags & CONTACT_FLAG_CONFIDENCE != 0,
        size,
        pressure,
    })
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}
