use std::error::Error;
use std::fmt::{self, Display, Formatter};

use capyio_input::{GamepadButton, GamepadControls};

use crate::{AxisSign, DsuMotionSample, crc32_ieee};

pub const DSU_PROTOCOL_VERSION: u16 = 1001;
pub const MAX_DSU_DATAGRAM_BYTES: usize = 128;
pub const DSU_PAD_DATA_PACKET_BYTES: usize = 100;

const DSU_HEADER_BYTES: usize = 16;
const DSU_COMMON_BYTES: usize = 20;
const DSU_CLIENT_MAGIC: [u8; 4] = *b"DSUC";
const DSU_SERVER_MAGIC: [u8; 4] = *b"DSUS";
const MESSAGE_VERSION: u32 = 0x10_0000;
const MESSAGE_PORT_INFO: u32 = 0x10_0001;
const MESSAGE_PAD_DATA: u32 = 0x10_0002;
const MAX_SLOTS: usize = 4;
const UNSUPPORTED_DSU_BUTTON_MASK: u32 = (1 << GamepadButton::Paddle1 as u8)
    | (1 << GamepadButton::Paddle2 as u8)
    | (1 << GamepadButton::Paddle3 as u8)
    | (1 << GamepadButton::Paddle4 as u8);

/// Selects how the ambiguous four DSU face fields are interpreted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DsuFaceButtonLayout {
    /// Follows the pinned v1993 field labels Y/B/A/X at bits 7/6/5/4.
    ProtocolNamed,
    /// Follows DualShock physical positions Square/Cross/Circle/Triangle.
    DualShockPhysical,
}

/// Explicit semantic mapping from normalized controls into DSU fields.
///
/// In addition to source-axis signs, callers select a face-button layout
/// because DSU implementations disagree whether the four fields are named as
/// Y/B/A/X or interpreted as Square/Cross/Circle/Triangle. DSU defines positive
/// X as rightward and positive Y as upward; the portable CapyIO gamepad
/// contract does not impose a source-coordinate orientation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DsuControlsMapping {
    face_buttons: DsuFaceButtonLayout,
    dpad_x: AxisSign,
    dpad_y: AxisSign,
    left_stick_x: AxisSign,
    left_stick_y: AxisSign,
    right_stick_x: AxisSign,
    right_stick_y: AxisSign,
}

impl DsuControlsMapping {
    #[must_use]
    pub const fn new(
        face_buttons: DsuFaceButtonLayout,
        dpad_x: AxisSign,
        dpad_y: AxisSign,
        left_stick_x: AxisSign,
        left_stick_y: AxisSign,
        right_stick_x: AxisSign,
        right_stick_y: AxisSign,
    ) -> Self {
        Self {
            face_buttons,
            dpad_x,
            dpad_y,
            left_stick_x,
            left_stick_y,
            right_stick_x,
            right_stick_y,
        }
    }

    /// Uses the pinned protocol's face-field names and preserves every source
    /// sign. This is useful for deterministic fixtures, not a claim about a
    /// particular UI, physical controller or emulator interpretation.
    #[must_use]
    pub const fn identity() -> Self {
        Self::new(
            DsuFaceButtonLayout::ProtocolNamed,
            AxisSign::Positive,
            AxisSign::Positive,
            AxisSign::Positive,
            AxisSign::Positive,
            AxisSign::Positive,
            AxisSign::Positive,
        )
    }

    /// Uses Dolphin's current DualShock physical interpretation with source
    /// axis signs preserved.
    #[must_use]
    pub const fn dualshock_physical() -> Self {
        Self::new(
            DsuFaceButtonLayout::DualShockPhysical,
            AxisSign::Positive,
            AxisSign::Positive,
            AxisSign::Positive,
            AxisSign::Positive,
            AxisSign::Positive,
            AxisSign::Positive,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DsuRequestedSlots {
    slots: [u8; MAX_SLOTS],
    len: u8,
}

impl DsuRequestedSlots {
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.slots[..usize::from(self.len)]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DsuPadSelector {
    pub flags: u8,
    pub slot: u8,
    pub mac: [u8; 6],
}

impl DsuPadSelector {
    /// Returns whether a parsed selector requests the supplied controller.
    ///
    /// DSU treats the enabled slot and MAC criteria as alternatives. A selector
    /// with no flags requests every controller. `None` deliberately means that
    /// the projected controller has no stable MAC identity to advertise.
    #[must_use]
    pub fn selects(self, slot: u8, mac: Option<[u8; 6]>) -> bool {
        self.flags == 0
            || (self.flags & 1 != 0 && self.slot == slot)
            || (self.flags & 2 != 0 && mac.is_some_and(|value| value == self.mac))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DsuRequest {
    Version {
        client_id: u32,
    },
    PortInfo {
        client_id: u32,
        requested_slots: DsuRequestedSlots,
    },
    PadData {
        client_id: u32,
        selector: DsuPadSelector,
    },
}

impl DsuRequest {
    #[must_use]
    pub const fn client_id(self) -> u32 {
        match self {
            Self::Version { client_id }
            | Self::PortInfo { client_id, .. }
            | Self::PadData { client_id, .. } => client_id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DsuPacketError {
    DatagramTooLarge {
        actual: usize,
        maximum: usize,
    },
    DatagramTooShort {
        actual: usize,
        minimum: usize,
    },
    InvalidMagic,
    UnsupportedVersion(u16),
    DeclaredLengthTooShort {
        declared_total: usize,
        actual: usize,
    },
    InvalidCrc {
        stored: u32,
        calculated: u32,
    },
    UnsupportedMessageType(u32),
    UnexpectedPayloadLength {
        message_type: u32,
        actual: usize,
    },
    InvalidPortCount(i32),
    InvalidSlot(u8),
    UnavailableSlot(u8),
    InvalidRegistrationFlags(u8),
    NonNeutralControlsUnsupported,
    InvalidGamepadControls,
    UnsupportedGamepadButtons(u32),
}

impl Display for DsuPacketError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DatagramTooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "DSU datagram is {actual} bytes; maximum is {maximum}"
                )
            }
            Self::DatagramTooShort { actual, minimum } => {
                write!(
                    formatter,
                    "DSU datagram is {actual} bytes; minimum is {minimum}"
                )
            }
            Self::InvalidMagic => formatter.write_str("DSU client magic must be DSUC"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported DSU protocol version {version}")
            }
            Self::DeclaredLengthTooShort {
                declared_total,
                actual,
            } => write!(
                formatter,
                "DSU header declares {declared_total} bytes but datagram has {actual}"
            ),
            Self::InvalidCrc { stored, calculated } => write!(
                formatter,
                "DSU CRC mismatch: stored {stored:#010x}, calculated {calculated:#010x}"
            ),
            Self::UnsupportedMessageType(message_type) => {
                write!(
                    formatter,
                    "unsupported DSU message type {message_type:#010x}"
                )
            }
            Self::UnexpectedPayloadLength {
                message_type,
                actual,
            } => write!(
                formatter,
                "DSU message {message_type:#010x} has unexpected logical length {actual}"
            ),
            Self::InvalidPortCount(count) => write!(formatter, "invalid DSU port count {count}"),
            Self::InvalidSlot(slot) => write!(formatter, "invalid DSU slot {slot}"),
            Self::UnavailableSlot(slot) => {
                write!(
                    formatter,
                    "DSU slot {slot} is valid but unavailable in this slice"
                )
            }
            Self::InvalidRegistrationFlags(flags) => {
                write!(formatter, "invalid DSU registration flags {flags:#04x}")
            }
            Self::NonNeutralControlsUnsupported => formatter
                .write_str("CAPY-GAMEPAD-001A only emits explicit neutral gamepad controls"),
            Self::InvalidGamepadControls => {
                formatter.write_str("invalid normalized gamepad controls")
            }
            Self::UnsupportedGamepadButtons(bits) => write!(
                formatter,
                "normalized gamepad buttons {bits:#010x} have no DSU v1001 field"
            ),
        }
    }
}

impl Error for DsuPacketError {}

/// Parses one bounded DSU v1001 client request without retaining the datagram.
///
/// Per the protocol, bytes beyond the declared logical length are ignored.
/// Datagrams shorter than the declared length, or larger than the hard Adapter
/// bound, are rejected. Unofficial motor and rumble messages are outside this
/// slice and therefore rejected as unsupported.
pub fn parse_client_request(datagram: &[u8]) -> Result<DsuRequest, DsuPacketError> {
    if datagram.len() > MAX_DSU_DATAGRAM_BYTES {
        return Err(DsuPacketError::DatagramTooLarge {
            actual: datagram.len(),
            maximum: MAX_DSU_DATAGRAM_BYTES,
        });
    }
    if datagram.len() < DSU_COMMON_BYTES {
        return Err(DsuPacketError::DatagramTooShort {
            actual: datagram.len(),
            minimum: DSU_COMMON_BYTES,
        });
    }
    if datagram[..4] != DSU_CLIENT_MAGIC {
        return Err(DsuPacketError::InvalidMagic);
    }
    let version = read_u16(datagram, 4);
    if version != DSU_PROTOCOL_VERSION {
        return Err(DsuPacketError::UnsupportedVersion(version));
    }

    let declared_total = DSU_HEADER_BYTES + usize::from(read_u16(datagram, 6));
    if datagram.len() < declared_total {
        return Err(DsuPacketError::DeclaredLengthTooShort {
            declared_total,
            actual: datagram.len(),
        });
    }
    if declared_total < DSU_COMMON_BYTES {
        return Err(DsuPacketError::DatagramTooShort {
            actual: declared_total,
            minimum: DSU_COMMON_BYTES,
        });
    }

    let logical = &datagram[..declared_total];
    let stored_crc = read_u32(logical, 8);
    let mut checksum_input = [0_u8; MAX_DSU_DATAGRAM_BYTES];
    checksum_input[..declared_total].copy_from_slice(logical);
    checksum_input[8..12].fill(0);
    let calculated_crc = crc32_ieee(&checksum_input[..declared_total]);
    if stored_crc != calculated_crc {
        return Err(DsuPacketError::InvalidCrc {
            stored: stored_crc,
            calculated: calculated_crc,
        });
    }

    let client_id = read_u32(logical, 12);
    let message_type = read_u32(logical, 16);
    match message_type {
        MESSAGE_VERSION => {
            require_length(message_type, logical.len(), DSU_COMMON_BYTES)?;
            Ok(DsuRequest::Version { client_id })
        }
        MESSAGE_PORT_INFO => parse_port_info(logical, client_id),
        MESSAGE_PAD_DATA => parse_pad_data(logical, client_id),
        _ => Err(DsuPacketError::UnsupportedMessageType(message_type)),
    }
}

fn parse_port_info(logical: &[u8], client_id: u32) -> Result<DsuRequest, DsuPacketError> {
    if logical.len() < 24 {
        return Err(DsuPacketError::UnexpectedPayloadLength {
            message_type: MESSAGE_PORT_INFO,
            actual: logical.len(),
        });
    }
    let count = read_u32(logical, 20) as i32;
    if !(0..=MAX_SLOTS as i32).contains(&count) {
        return Err(DsuPacketError::InvalidPortCount(count));
    }
    let count = count as usize;
    require_length(MESSAGE_PORT_INFO, logical.len(), 24 + count)?;
    let mut slots = [0_u8; MAX_SLOTS];
    slots[..count].copy_from_slice(&logical[24..24 + count]);
    if let Some(slot) = slots[..count]
        .iter()
        .copied()
        .find(|slot| usize::from(*slot) >= MAX_SLOTS)
    {
        return Err(DsuPacketError::InvalidSlot(slot));
    }
    Ok(DsuRequest::PortInfo {
        client_id,
        requested_slots: DsuRequestedSlots {
            slots,
            len: count as u8,
        },
    })
}

fn parse_pad_data(logical: &[u8], client_id: u32) -> Result<DsuRequest, DsuPacketError> {
    require_length(MESSAGE_PAD_DATA, logical.len(), 28)?;
    let flags = logical[20];
    if flags & !0b11 != 0 {
        return Err(DsuPacketError::InvalidRegistrationFlags(flags));
    }
    let slot = logical[21];
    if flags & 1 != 0 && usize::from(slot) >= MAX_SLOTS {
        return Err(DsuPacketError::InvalidSlot(slot));
    }
    let mut mac = [0_u8; 6];
    mac.copy_from_slice(&logical[22..28]);
    Ok(DsuRequest::PadData {
        client_id,
        selector: DsuPadSelector { flags, slot, mac },
    })
}

fn require_length(message_type: u32, actual: usize, expected: usize) -> Result<(), DsuPacketError> {
    if actual == expected {
        Ok(())
    } else {
        Err(DsuPacketError::UnexpectedPayloadLength {
            message_type,
            actual,
        })
    }
}

#[must_use]
pub fn encode_version_response(server_id: u32) -> [u8; 22] {
    let mut packet = [0_u8; 22];
    write_u16(&mut packet, 20, DSU_PROTOCOL_VERSION);
    seal_server_packet(&mut packet, server_id, MESSAGE_VERSION);
    packet
}

pub fn encode_port_info_response(server_id: u32, slot: u8) -> Result<[u8; 32], DsuPacketError> {
    validate_slot(slot)?;
    let mut packet = [0_u8; 32];
    packet[20] = slot;
    if slot == 0 {
        write_connected_controller_beginning(&mut packet, slot);
    }
    // Port-info byte 11 is reserved and remains zero.
    seal_server_packet(&mut packet, server_id, MESSAGE_PORT_INFO);
    Ok(packet)
}

/// Encodes a fixed-size DSU pad-data response with explicit neutral controls.
///
/// This CAPY-GAMEPAD-001A compatibility helper retains its fail-closed
/// behavior. New callers that intentionally project controls use
/// [`encode_pad_data`].
pub fn encode_neutral_pad_data(
    server_id: u32,
    slot: u8,
    packet_number: u32,
    motion: DsuMotionSample,
    controls: GamepadControls,
) -> Result<[u8; DSU_PAD_DATA_PACKET_BYTES], DsuPacketError> {
    validate_slot(slot)?;
    if slot != 0 {
        return Err(DsuPacketError::UnavailableSlot(slot));
    }
    controls
        .validate()
        .map_err(|_| DsuPacketError::InvalidGamepadControls)?;
    if controls != GamepadControls::neutral() {
        return Err(DsuPacketError::NonNeutralControlsUnsupported);
    }

    encode_pad_data(
        server_id,
        slot,
        packet_number,
        motion,
        controls,
        DsuControlsMapping::identity(),
    )
}

/// Encodes one normalized controller snapshot and one motion sample into DSU.
///
/// The supplied mapping makes the ambiguous face-button naming and source-axis
/// orientation explicit. Signed stick axes map -32767/0/32767 to 0/128/255.
/// Any non-zero trigger sets its digital bit while its full 16-bit magnitude is
/// scaled to the analog byte. Paddle buttons fail closed because DSU v1001 has
/// no corresponding fields.
pub fn encode_pad_data(
    server_id: u32,
    slot: u8,
    packet_number: u32,
    motion: DsuMotionSample,
    controls: GamepadControls,
    mapping: DsuControlsMapping,
) -> Result<[u8; DSU_PAD_DATA_PACKET_BYTES], DsuPacketError> {
    validate_slot(slot)?;
    if slot != 0 {
        return Err(DsuPacketError::UnavailableSlot(slot));
    }
    validate_dsu_controls(controls)?;

    let mut packet = [0_u8; DSU_PAD_DATA_PACKET_BYTES];
    write_connected_controller_beginning(&mut packet, slot);
    packet[31] = 1;
    write_u32(&mut packet, 32, packet_number);
    write_controls(&mut packet, controls, mapping);
    write_u64(&mut packet, 68, motion.timestamp_micros());
    let acceleration = motion.acceleration_g();
    let gyroscope = motion.gyroscope_degrees_per_second();
    for (offset, value) in [76, 80, 84].into_iter().zip(acceleration) {
        write_f32(&mut packet, offset, value);
    }
    for (offset, value) in [88, 92, 96].into_iter().zip(gyroscope) {
        write_f32(&mut packet, offset, value);
    }
    seal_server_packet(&mut packet, server_id, MESSAGE_PAD_DATA);
    Ok(packet)
}

pub(crate) fn validate_dsu_controls(controls: GamepadControls) -> Result<(), DsuPacketError> {
    controls
        .validate()
        .map_err(|_| DsuPacketError::InvalidGamepadControls)?;
    let unsupported = controls.buttons.bits() & UNSUPPORTED_DSU_BUTTON_MASK;
    if unsupported != 0 {
        return Err(DsuPacketError::UnsupportedGamepadButtons(unsupported));
    }
    Ok(())
}

fn write_controls(
    packet: &mut [u8; DSU_PAD_DATA_PACKET_BYTES],
    controls: GamepadControls,
    mapping: DsuControlsMapping,
) {
    let dpad_x = apply_i8_sign(mapping.dpad_x, controls.dpad.x);
    let dpad_y = apply_i8_sign(mapping.dpad_y, controls.dpad.y);
    set_bit(&mut packet[36], 7, dpad_x < 0);
    set_bit(&mut packet[36], 6, dpad_y < 0);
    set_bit(&mut packet[36], 5, dpad_x > 0);
    set_bit(&mut packet[36], 4, dpad_y > 0);
    set_bit(
        &mut packet[36],
        3,
        controls.buttons.contains(GamepadButton::Start),
    );
    set_bit(
        &mut packet[36],
        2,
        controls.buttons.contains(GamepadButton::RightStick),
    );
    set_bit(
        &mut packet[36],
        1,
        controls.buttons.contains(GamepadButton::LeftStick),
    );
    set_bit(
        &mut packet[36],
        0,
        controls.buttons.contains(GamepadButton::Select),
    );

    write_face_buttons(packet, controls, mapping.face_buttons);
    set_bit(
        &mut packet[37],
        3,
        controls.buttons.contains(GamepadButton::RightShoulder),
    );
    set_bit(
        &mut packet[37],
        2,
        controls.buttons.contains(GamepadButton::LeftShoulder),
    );
    set_bit(&mut packet[37], 1, controls.right_trigger.get() != 0);
    set_bit(&mut packet[37], 0, controls.left_trigger.get() != 0);

    packet[38] = u8::from(controls.buttons.contains(GamepadButton::Guide));
    packet[39] = u8::from(controls.buttons.contains(GamepadButton::Touchpad));
    packet[40] = scale_signed_axis(apply_i16_sign(
        mapping.left_stick_x,
        controls.left_stick.x.get(),
    ));
    packet[41] = scale_signed_axis(apply_i16_sign(
        mapping.left_stick_y,
        controls.left_stick.y.get(),
    ));
    packet[42] = scale_signed_axis(apply_i16_sign(
        mapping.right_stick_x,
        controls.right_stick.x.get(),
    ));
    packet[43] = scale_signed_axis(apply_i16_sign(
        mapping.right_stick_y,
        controls.right_stick.y.get(),
    ));
    packet[44] = pressed_byte(dpad_x < 0);
    packet[45] = pressed_byte(dpad_y < 0);
    packet[46] = pressed_byte(dpad_x > 0);
    packet[47] = pressed_byte(dpad_y > 0);
    packet[52] = pressed_byte(controls.buttons.contains(GamepadButton::RightShoulder));
    packet[53] = pressed_byte(controls.buttons.contains(GamepadButton::LeftShoulder));
    packet[54] = scale_trigger(controls.right_trigger.get());
    packet[55] = scale_trigger(controls.left_trigger.get());
}

fn write_face_buttons(
    packet: &mut [u8; DSU_PAD_DATA_PACKET_BYTES],
    controls: GamepadControls,
    layout: DsuFaceButtonLayout,
) {
    let ordered = match layout {
        DsuFaceButtonLayout::ProtocolNamed => [
            GamepadButton::North,
            GamepadButton::East,
            GamepadButton::South,
            GamepadButton::West,
        ],
        DsuFaceButtonLayout::DualShockPhysical => [
            GamepadButton::West,
            GamepadButton::South,
            GamepadButton::East,
            GamepadButton::North,
        ],
    };
    for (index, button) in ordered.into_iter().enumerate() {
        let pressed = controls.buttons.contains(button);
        set_bit(
            &mut packet[37],
            7 - u8::try_from(index).expect("four face buttons fit in u8"),
            pressed,
        );
        packet[48 + index] = pressed_byte(pressed);
    }
}

fn set_bit(byte: &mut u8, bit: u8, pressed: bool) {
    if pressed {
        *byte |= 1 << bit;
    }
}

const fn apply_i8_sign(sign: AxisSign, value: i8) -> i8 {
    match sign {
        AxisSign::Positive => value,
        AxisSign::Negative => -value,
    }
}

fn apply_i16_sign(sign: AxisSign, value: i16) -> i16 {
    match sign {
        AxisSign::Positive => value,
        AxisSign::Negative => value
            .checked_neg()
            .expect("validated signed gamepad axis excludes i16::MIN"),
    }
}

const fn pressed_byte(pressed: bool) -> u8 {
    if pressed { u8::MAX } else { 0 }
}

fn scale_signed_axis(value: i16) -> u8 {
    let value = i32::from(value);
    let scaled = if value < 0 {
        128 - ((-value * 128 + 16_383) / 32_767)
    } else {
        128 + ((value * 127 + 16_383) / 32_767)
    };
    u8::try_from(scaled).expect("validated signed gamepad axis maps to u8")
}

fn scale_trigger(value: u16) -> u8 {
    let scaled = (u32::from(value) * u32::from(u8::MAX) + 32_767) / u32::from(u16::MAX);
    u8::try_from(scaled).expect("u16 trigger scaling maps to u8")
}

fn validate_slot(slot: u8) -> Result<(), DsuPacketError> {
    if usize::from(slot) < MAX_SLOTS {
        Ok(())
    } else {
        Err(DsuPacketError::InvalidSlot(slot))
    }
}

fn write_connected_controller_beginning(packet: &mut [u8], slot: u8) {
    packet[20] = slot;
    packet[21] = 2;
    packet[22] = 2;
    // Connection type, MAC and battery remain zero/not-applicable.
}

fn seal_server_packet(packet: &mut [u8], server_id: u32, message_type: u32) {
    packet[..4].copy_from_slice(&DSU_SERVER_MAGIC);
    write_u16(packet, 4, DSU_PROTOCOL_VERSION);
    write_u16(
        packet,
        6,
        u16::try_from(packet.len() - DSU_HEADER_BYTES)
            .expect("all DSU response packet sizes fit in u16"),
    );
    packet[8..12].fill(0);
    write_u32(packet, 12, server_id);
    write_u32(packet, 16, message_type);
    let checksum = crc32_ieee(packet);
    write_u32(packet, 8, checksum);
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

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn write_f32(bytes: &mut [u8], offset: usize, value: f32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::{
        DSU_CLIENT_MAGIC, DSU_COMMON_BYTES, DSU_HEADER_BYTES, DSU_PROTOCOL_VERSION, DsuPacketError,
        DsuRequest, MAX_DSU_DATAGRAM_BYTES, MESSAGE_PAD_DATA, MESSAGE_PORT_INFO, MESSAGE_VERSION,
        crc32_ieee, parse_client_request, read_u32, write_u16, write_u32,
    };

    fn client_request(message_type: u32, payload: &[u8], trailing: &[u8]) -> Vec<u8> {
        let logical_len = 20 + payload.len();
        let mut packet = vec![0_u8; logical_len + trailing.len()];
        packet[..4].copy_from_slice(&DSU_CLIENT_MAGIC);
        write_u16(&mut packet, 4, DSU_PROTOCOL_VERSION);
        write_u16(
            &mut packet,
            6,
            u16::try_from(logical_len - DSU_HEADER_BYTES).unwrap(),
        );
        write_u32(&mut packet, 12, 0x1122_3344);
        write_u32(&mut packet, 16, message_type);
        packet[20..logical_len].copy_from_slice(payload);
        packet[logical_len..].copy_from_slice(trailing);
        let checksum = crc32_ieee(&packet[..logical_len]);
        write_u32(&mut packet, 8, checksum);
        packet
    }

    #[test]
    fn parses_all_supported_request_shapes() {
        let version = client_request(MESSAGE_VERSION, &[], &[]);
        assert_eq!(
            parse_client_request(&version),
            Ok(DsuRequest::Version {
                client_id: 0x1122_3344
            })
        );

        let ports = client_request(MESSAGE_PORT_INFO, &[2, 0, 0, 0, 0, 3], &[]);
        let DsuRequest::PortInfo {
            requested_slots, ..
        } = parse_client_request(&ports).unwrap()
        else {
            panic!("expected port-info request")
        };
        assert_eq!(requested_slots.as_slice(), &[0, 3]);

        let pads = client_request(MESSAGE_PAD_DATA, &[3, 2, 1, 2, 3, 4, 5, 6], &[]);
        let DsuRequest::PadData { selector, .. } = parse_client_request(&pads).unwrap() else {
            panic!("expected pad-data request")
        };
        assert_eq!(selector.flags, 3);
        assert_eq!(selector.slot, 2);
        assert_eq!(selector.mac, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn follows_declared_length_truncation_rule() {
        let packet = client_request(MESSAGE_VERSION, &[], &[9, 8, 7]);
        assert!(matches!(
            parse_client_request(&packet),
            Ok(DsuRequest::Version { .. })
        ));
    }

    #[test]
    fn rejects_crc_corruption_and_unofficial_messages() {
        let mut corrupt = client_request(MESSAGE_PAD_DATA, &[1, 0, 0, 0, 0, 0, 0, 0], &[]);
        corrupt[21] ^= 1;
        assert!(matches!(
            parse_client_request(&corrupt),
            Err(DsuPacketError::InvalidCrc { .. })
        ));

        let unofficial = client_request(0x11_0002, &[], &[]);
        assert_eq!(
            parse_client_request(&unofficial),
            Err(DsuPacketError::UnsupportedMessageType(0x11_0002))
        );
    }

    #[test]
    fn rejects_invalid_counts_flags_slots_and_bounds() {
        let count = client_request(MESSAGE_PORT_INFO, &[5, 0, 0, 0], &[]);
        assert_eq!(
            parse_client_request(&count),
            Err(DsuPacketError::InvalidPortCount(5))
        );

        let flags = client_request(MESSAGE_PAD_DATA, &[4, 0, 0, 0, 0, 0, 0, 0], &[]);
        assert_eq!(
            parse_client_request(&flags),
            Err(DsuPacketError::InvalidRegistrationFlags(4))
        );

        let slot = client_request(MESSAGE_PAD_DATA, &[1, 4, 0, 0, 0, 0, 0, 0], &[]);
        assert_eq!(
            parse_client_request(&slot),
            Err(DsuPacketError::InvalidSlot(4))
        );

        let oversized = vec![0_u8; MAX_DSU_DATAGRAM_BYTES + 1];
        assert!(matches!(
            parse_client_request(&oversized),
            Err(DsuPacketError::DatagramTooLarge { .. })
        ));
    }

    #[test]
    fn rejects_short_magic_version_and_declared_length_errors() {
        let short = vec![0_u8; DSU_COMMON_BYTES - 1];
        assert_eq!(
            parse_client_request(&short),
            Err(DsuPacketError::DatagramTooShort {
                actual: DSU_COMMON_BYTES - 1,
                minimum: DSU_COMMON_BYTES,
            })
        );

        let mut bad_magic = client_request(MESSAGE_VERSION, &[], &[]);
        bad_magic[..4].copy_from_slice(b"NOPE");
        assert_eq!(
            parse_client_request(&bad_magic),
            Err(DsuPacketError::InvalidMagic)
        );

        let mut bad_version = client_request(MESSAGE_VERSION, &[], &[]);
        write_u16(&mut bad_version, 4, DSU_PROTOCOL_VERSION - 1);
        assert_eq!(
            parse_client_request(&bad_version),
            Err(DsuPacketError::UnsupportedVersion(DSU_PROTOCOL_VERSION - 1))
        );

        let mut truncated = client_request(MESSAGE_VERSION, &[], &[]);
        write_u16(&mut truncated, 6, 100);
        assert_eq!(
            parse_client_request(&truncated),
            Err(DsuPacketError::DeclaredLengthTooShort {
                declared_total: DSU_HEADER_BYTES + 100,
                actual: DSU_COMMON_BYTES,
            })
        );
    }

    #[test]
    fn pad_selector_validates_only_enabled_identity_fields() {
        for (flags, slot) in [(0, u8::MAX), (2, u8::MAX)] {
            let request = client_request(MESSAGE_PAD_DATA, &[flags, slot, 1, 2, 3, 4, 5, 6], &[]);
            assert!(matches!(
                parse_client_request(&request),
                Ok(DsuRequest::PadData { .. })
            ));
        }

        for flags in [1, 3] {
            let request = client_request(MESSAGE_PAD_DATA, &[flags, 4, 0, 0, 0, 0, 0, 0], &[]);
            assert_eq!(
                parse_client_request(&request),
                Err(DsuPacketError::InvalidSlot(4))
            );
        }
    }

    #[test]
    fn pad_selector_uses_alternative_slot_and_mac_criteria() {
        let all = super::DsuPadSelector {
            flags: 0,
            slot: u8::MAX,
            mac: [0xff; 6],
        };
        assert!(all.selects(0, None));

        let slot = super::DsuPadSelector {
            flags: 1,
            slot: 0,
            mac: [0xff; 6],
        };
        assert!(slot.selects(0, None));
        assert!(!slot.selects(1, None));

        let mac = [1, 2, 3, 4, 5, 6];
        let either = super::DsuPadSelector {
            flags: 3,
            slot: 1,
            mac,
        };
        assert!(either.selects(1, None));
        assert!(either.selects(0, Some(mac)));
        assert!(!either.selects(0, None));
    }

    #[test]
    fn encoded_crc_covers_packet_with_zeroed_crc_field() {
        let packet = super::encode_version_response(0xaabb_ccdd);
        let stored = read_u32(&packet, 8);
        let mut checksum_input = packet;
        checksum_input[8..12].fill(0);
        assert_eq!(stored, crc32_ieee(&checksum_input));
        assert_eq!(&packet[..4], b"DSUS");
        assert_eq!(read_u32(&packet, 12), 0xaabb_ccdd);
    }

    #[test]
    fn port_info_marks_only_slot_zero_connected() {
        let connected = super::encode_port_info_response(7, 0).unwrap();
        assert_eq!(connected[20], 0);
        assert_eq!(connected[21], 2);
        assert_eq!(connected[22], 2);

        for slot in 1..=3 {
            let disconnected = super::encode_port_info_response(7, slot).unwrap();
            assert_eq!(disconnected[20], slot);
            assert!(disconnected[21..32].iter().all(|byte| *byte == 0));
        }
    }

    #[test]
    fn pad_data_rejects_valid_but_unavailable_slots() {
        use capyio_data_plane::parse_imu_fixture_jsonl;
        use capyio_input::GamepadControls;

        use crate::{DsuMotionMapping, project_imu_envelope};

        const FIXTURE: &str = include_str!("../../../fixtures/imu/imu_samples_v1.jsonl");
        let envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
        let motion = project_imu_envelope(&envelope, DsuMotionMapping::identity()).unwrap();
        for slot in 1..=3 {
            assert_eq!(
                super::encode_neutral_pad_data(7, slot, 0, motion, GamepadControls::neutral(),),
                Err(DsuPacketError::UnavailableSlot(slot))
            );
        }
    }

    #[test]
    fn response_encoders_reject_invalid_slots_and_controls() {
        use capyio_data_plane::parse_imu_fixture_jsonl;
        use capyio_input::{DpadState, GamepadControls};

        use crate::{DsuMotionMapping, project_imu_envelope};

        assert_eq!(
            super::encode_port_info_response(7, 4),
            Err(DsuPacketError::InvalidSlot(4))
        );

        const FIXTURE: &str = include_str!("../../../fixtures/imu/imu_samples_v1.jsonl");
        let envelope = parse_imu_fixture_jsonl(FIXTURE, 6).unwrap().remove(0);
        let motion = project_imu_envelope(&envelope, DsuMotionMapping::identity()).unwrap();
        let invalid_controls = GamepadControls {
            dpad: DpadState { x: 2, y: 0 },
            ..GamepadControls::neutral()
        };
        assert_eq!(
            super::encode_neutral_pad_data(7, 0, 0, motion, invalid_controls),
            Err(DsuPacketError::InvalidGamepadControls)
        );
    }
}
