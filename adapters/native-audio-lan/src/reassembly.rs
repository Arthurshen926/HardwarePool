use std::collections::BTreeMap;

use capyio_audio::{AudioMediaPacket, AudioMediaStreamBinding};

use crate::{NativeLanError, NativeLanFragment, decode_native_lan_fragment};

pub const MAX_NATIVE_LAN_INFLIGHT_PACKETS: usize = 8;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeLanReassemblyStats {
    pub accepted_fragments: u64,
    pub completed_packets: u64,
    pub duplicate_fragments: u64,
    pub wrong_binding: u64,
    pub partial_evictions: u64,
    pub malformed_fragments: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeLanReassemblyOutcome {
    Pending,
    Complete(AudioMediaPacket),
    DuplicateFragment,
    WrongBinding,
}

#[derive(Clone, Debug)]
struct PartialPacket {
    source_timestamp_micros: u64,
    first_sample_index: u64,
    sample_count: u32,
    discontinuity: bool,
    fragment_count: u16,
    received_fragments: u64,
    payload: Vec<u8>,
}

impl PartialPacket {
    fn new(fragment: &NativeLanFragment<'_>) -> Self {
        Self {
            source_timestamp_micros: fragment.source_timestamp_micros,
            first_sample_index: fragment.first_sample_index,
            sample_count: fragment.sample_count,
            discontinuity: fragment.discontinuity,
            fragment_count: fragment.fragment_count,
            received_fragments: 0,
            payload: vec![0; fragment.total_payload_bytes],
        }
    }

    fn metadata_matches(&self, fragment: &NativeLanFragment<'_>) -> bool {
        self.source_timestamp_micros == fragment.source_timestamp_micros
            && self.first_sample_index == fragment.first_sample_index
            && self.sample_count == fragment.sample_count
            && self.discontinuity == fragment.discontinuity
            && self.fragment_count == fragment.fragment_count
            && self.payload.len() == fragment.total_payload_bytes
    }

    fn complete(&self) -> bool {
        let expected = if self.fragment_count == 64 {
            u64::MAX
        } else {
            (1_u64 << self.fragment_count) - 1
        };
        self.received_fragments == expected
    }
}

#[derive(Clone, Debug)]
pub struct NativeLanReassembler {
    binding: AudioMediaStreamBinding,
    inflight_capacity: usize,
    partial: BTreeMap<u64, PartialPacket>,
    stats: NativeLanReassemblyStats,
}

impl NativeLanReassembler {
    pub fn new(
        binding: AudioMediaStreamBinding,
        inflight_capacity: usize,
    ) -> Result<Self, NativeLanError> {
        binding.validate()?;
        if inflight_capacity == 0 || inflight_capacity > MAX_NATIVE_LAN_INFLIGHT_PACKETS {
            return Err(NativeLanError::InvalidConfiguration(
                "in-flight packet capacity is outside 1..=8",
            ));
        }
        Ok(Self {
            binding,
            inflight_capacity,
            partial: BTreeMap::new(),
            stats: NativeLanReassemblyStats::default(),
        })
    }

    pub fn push_datagram(
        &mut self,
        datagram: &[u8],
    ) -> Result<NativeLanReassemblyOutcome, NativeLanError> {
        let fragment = match decode_native_lan_fragment(datagram) {
            Ok(fragment) => fragment,
            Err(error) => {
                self.stats.malformed_fragments = self.stats.malformed_fragments.saturating_add(1);
                return Err(error);
            }
        };
        self.push_fragment(fragment)
    }

    pub fn push_fragment(
        &mut self,
        fragment: NativeLanFragment<'_>,
    ) -> Result<NativeLanReassemblyOutcome, NativeLanError> {
        if !fragment.matches_binding(&self.binding) {
            self.stats.wrong_binding = self.stats.wrong_binding.saturating_add(1);
            return Ok(NativeLanReassemblyOutcome::WrongBinding);
        }

        if !self.partial.contains_key(&fragment.sequence)
            && self.partial.len() == self.inflight_capacity
        {
            let oldest = *self.partial.keys().next().expect("non-empty bounded map");
            self.partial.remove(&oldest);
            self.stats.partial_evictions = self.stats.partial_evictions.saturating_add(1);
        }

        let partial = self
            .partial
            .entry(fragment.sequence)
            .or_insert_with(|| PartialPacket::new(&fragment));
        if !partial.metadata_matches(&fragment) {
            self.stats.malformed_fragments = self.stats.malformed_fragments.saturating_add(1);
            self.partial.remove(&fragment.sequence);
            return Err(NativeLanError::InvalidDatagram(
                "fragments for one sequence disagree on packet metadata",
            ));
        }

        let fragment_bit = 1_u64 << fragment.fragment_index;
        let range = fragment.fragment_offset..fragment.fragment_offset + fragment.payload.len();
        if partial.received_fragments & fragment_bit != 0 {
            if partial.payload[range] != *fragment.payload {
                self.stats.malformed_fragments = self.stats.malformed_fragments.saturating_add(1);
                self.partial.remove(&fragment.sequence);
                return Err(NativeLanError::InvalidDatagram(
                    "duplicate fragment changed its payload",
                ));
            }
            self.stats.duplicate_fragments = self.stats.duplicate_fragments.saturating_add(1);
            return Ok(NativeLanReassemblyOutcome::DuplicateFragment);
        }

        partial.payload[range].copy_from_slice(fragment.payload);
        partial.received_fragments |= fragment_bit;
        self.stats.accepted_fragments = self.stats.accepted_fragments.saturating_add(1);
        if !partial.complete() {
            return Ok(NativeLanReassemblyOutcome::Pending);
        }

        let complete = self
            .partial
            .remove(&fragment.sequence)
            .expect("completed partial packet exists");
        let packet = AudioMediaPacket {
            stream_id: self.binding.stream_id,
            stream_epoch: self.binding.stream_epoch,
            sequence: fragment.sequence,
            source_timestamp_micros: complete.source_timestamp_micros,
            first_sample_index: complete.first_sample_index,
            sample_count: complete.sample_count,
            discontinuity: complete.discontinuity,
            payload: complete.payload,
        };
        packet.validate_against(&self.binding)?;
        self.stats.completed_packets = self.stats.completed_packets.saturating_add(1);
        Ok(NativeLanReassemblyOutcome::Complete(packet))
    }

    #[must_use]
    pub const fn binding(&self) -> &AudioMediaStreamBinding {
        &self.binding
    }

    #[must_use]
    pub fn inflight_packets(&self) -> usize {
        self.partial.len()
    }

    #[must_use]
    pub const fn stats(&self) -> NativeLanReassemblyStats {
        self.stats
    }
}
