use capyio_audio::{AudioMediaPacket, AudioMediaStreamBinding};
use capyio_core::{RouteId, SessionId, StreamId};
use uuid::Uuid;

use crate::NativeLanError;

pub const NATIVE_LAN_BACKEND_ID: &str = "dev.capyio.audio.lan-lab/1";
pub const NATIVE_LAN_WIRE_VERSION: u8 = 1;
pub const MAX_NATIVE_LAN_DATAGRAM_BYTES: usize = 1_200;
pub const NATIVE_LAN_HEADER_BYTES: usize = 104;
pub const MAX_NATIVE_LAN_FRAGMENT_PAYLOAD_BYTES: usize =
    MAX_NATIVE_LAN_DATAGRAM_BYTES - NATIVE_LAN_HEADER_BYTES;
pub const MAX_NATIVE_LAN_FRAGMENTS: usize = 64;
pub const MAX_NATIVE_LAN_PACKET_PAYLOAD_BYTES: usize =
    MAX_NATIVE_LAN_FRAGMENT_PAYLOAD_BYTES * MAX_NATIVE_LAN_FRAGMENTS;

const MAGIC: [u8; 4] = *b"CPYA";
const FLAG_DISCONTINUITY: u8 = 0x01;
const KNOWN_FLAGS: u8 = FLAG_DISCONTINUITY;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeLanFragment<'a> {
    pub session_id: SessionId,
    pub route_id: RouteId,
    pub stream_id: StreamId,
    pub stream_epoch: u32,
    pub sequence: u64,
    pub source_timestamp_micros: u64,
    pub first_sample_index: u64,
    pub sample_count: u32,
    pub discontinuity: bool,
    pub total_payload_bytes: usize,
    pub fragment_offset: usize,
    pub fragment_index: u16,
    pub fragment_count: u16,
    pub payload: &'a [u8],
}

impl NativeLanFragment<'_> {
    #[must_use]
    pub fn matches_binding(&self, binding: &AudioMediaStreamBinding) -> bool {
        self.session_id == binding.session_id
            && self.route_id == binding.route_id
            && self.stream_id == binding.stream_id
            && self.stream_epoch == binding.stream_epoch
    }
}

pub fn native_lan_fragment_count(payload_bytes: usize) -> Result<u16, NativeLanError> {
    if payload_bytes == 0 || payload_bytes > MAX_NATIVE_LAN_PACKET_PAYLOAD_BYTES {
        return Err(NativeLanError::InvalidDatagram(
            "packet payload is empty or exceeds the LAN backend bound",
        ));
    }
    let count = payload_bytes.div_ceil(MAX_NATIVE_LAN_FRAGMENT_PAYLOAD_BYTES);
    u16::try_from(count).map_err(|_| {
        NativeLanError::InvalidDatagram("fragment count exceeds the LAN backend bound")
    })
}

pub fn encode_native_lan_fragment(
    binding: &AudioMediaStreamBinding,
    packet: &AudioMediaPacket,
    fragment_index: u16,
    output: &mut [u8],
) -> Result<usize, NativeLanError> {
    binding.validate()?;
    packet.validate_against(binding)?;
    let fragment_count = native_lan_fragment_count(packet.payload.len())?;
    if fragment_index >= fragment_count {
        return Err(NativeLanError::InvalidDatagram(
            "fragment index is outside the packet fragment count",
        ));
    }

    let fragment_offset = usize::from(fragment_index) * MAX_NATIVE_LAN_FRAGMENT_PAYLOAD_BYTES;
    let fragment_bytes =
        (packet.payload.len() - fragment_offset).min(MAX_NATIVE_LAN_FRAGMENT_PAYLOAD_BYTES);
    let datagram_bytes = NATIVE_LAN_HEADER_BYTES + fragment_bytes;
    if output.len() < datagram_bytes {
        return Err(NativeLanError::InvalidConfiguration(
            "caller datagram buffer is too small",
        ));
    }

    output[..datagram_bytes].fill(0);
    output[0..4].copy_from_slice(&MAGIC);
    output[4] = NATIVE_LAN_WIRE_VERSION;
    output[5] = u8::from(packet.discontinuity) * FLAG_DISCONTINUITY;
    write_u16(output, 6, NATIVE_LAN_HEADER_BYTES as u16);
    output[8..24].copy_from_slice(binding.session_id.as_uuid().as_bytes());
    output[24..40].copy_from_slice(binding.route_id.as_uuid().as_bytes());
    output[40..56].copy_from_slice(binding.stream_id.as_uuid().as_bytes());
    write_u32(output, 56, binding.stream_epoch);
    write_u64(output, 60, packet.sequence);
    write_u64(output, 68, packet.source_timestamp_micros);
    write_u64(output, 76, packet.first_sample_index);
    write_u32(output, 84, packet.sample_count);
    write_u32(
        output,
        88,
        u32::try_from(packet.payload.len()).map_err(|_| {
            NativeLanError::InvalidDatagram("packet payload length cannot enter the wire header")
        })?,
    );
    write_u32(
        output,
        92,
        u32::try_from(fragment_offset).map_err(|_| {
            NativeLanError::InvalidDatagram("fragment offset cannot enter the wire header")
        })?,
    );
    write_u16(output, 96, fragment_index);
    write_u16(output, 98, fragment_count);
    write_u16(output, 100, fragment_bytes as u16);
    output[NATIVE_LAN_HEADER_BYTES..datagram_bytes]
        .copy_from_slice(&packet.payload[fragment_offset..fragment_offset + fragment_bytes]);
    Ok(datagram_bytes)
}

pub fn decode_native_lan_fragment(
    datagram: &[u8],
) -> Result<NativeLanFragment<'_>, NativeLanError> {
    if !(NATIVE_LAN_HEADER_BYTES..=MAX_NATIVE_LAN_DATAGRAM_BYTES).contains(&datagram.len()) {
        return Err(NativeLanError::InvalidDatagram(
            "datagram length is outside the LAN wire bounds",
        ));
    }
    if datagram[0..4] != MAGIC {
        return Err(NativeLanError::InvalidDatagram("wire magic is unknown"));
    }
    if datagram[4] != NATIVE_LAN_WIRE_VERSION {
        return Err(NativeLanError::InvalidDatagram(
            "wire version is unsupported",
        ));
    }
    if datagram[5] & !KNOWN_FLAGS != 0 {
        return Err(NativeLanError::InvalidDatagram(
            "wire flags contain unknown bits",
        ));
    }
    if read_u16(datagram, 6) as usize != NATIVE_LAN_HEADER_BYTES {
        return Err(NativeLanError::InvalidDatagram(
            "wire header length is not canonical",
        ));
    }
    if read_u16(datagram, 102) != 0 {
        return Err(NativeLanError::InvalidDatagram(
            "reserved wire field is non-zero",
        ));
    }

    let total_payload_bytes = read_u32(datagram, 88) as usize;
    let fragment_offset = read_u32(datagram, 92) as usize;
    let fragment_index = read_u16(datagram, 96);
    let fragment_count = read_u16(datagram, 98);
    let fragment_bytes = read_u16(datagram, 100) as usize;
    let expected_count = native_lan_fragment_count(total_payload_bytes)?;
    if fragment_count != expected_count || fragment_index >= fragment_count {
        return Err(NativeLanError::InvalidDatagram(
            "fragment count or index is not canonical",
        ));
    }
    let expected_offset = usize::from(fragment_index) * MAX_NATIVE_LAN_FRAGMENT_PAYLOAD_BYTES;
    if fragment_offset != expected_offset {
        return Err(NativeLanError::InvalidDatagram(
            "fragment offset is not canonical",
        ));
    }
    let expected_fragment_bytes =
        (total_payload_bytes - fragment_offset).min(MAX_NATIVE_LAN_FRAGMENT_PAYLOAD_BYTES);
    if fragment_bytes != expected_fragment_bytes
        || datagram.len() != NATIVE_LAN_HEADER_BYTES + fragment_bytes
    {
        return Err(NativeLanError::InvalidDatagram(
            "fragment payload length is inconsistent",
        ));
    }

    Ok(NativeLanFragment {
        session_id: SessionId::from_uuid(read_uuid(datagram, 8)),
        route_id: RouteId::from_uuid(read_uuid(datagram, 24)),
        stream_id: StreamId::from_uuid(read_uuid(datagram, 40)),
        stream_epoch: read_u32(datagram, 56),
        sequence: read_u64(datagram, 60),
        source_timestamp_micros: read_u64(datagram, 68),
        first_sample_index: read_u64(datagram, 76),
        sample_count: read_u32(datagram, 84),
        discontinuity: datagram[5] & FLAG_DISCONTINUITY != 0,
        total_payload_bytes,
        fragment_offset,
        fragment_index,
        fragment_count,
        payload: &datagram[NATIVE_LAN_HEADER_BYTES..],
    })
}

fn read_uuid(bytes: &[u8], offset: usize) -> Uuid {
    let mut value = [0_u8; 16];
    value.copy_from_slice(&bytes[offset..offset + 16]);
    Uuid::from_bytes(value)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("bounded header"),
    )
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("bounded header"),
    )
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_be_bytes());
}
