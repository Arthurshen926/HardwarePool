use std::{
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    time::Duration,
};

use capyio_audio::{AudioMediaPacket, AudioMediaStreamBinding};

use crate::{
    MAX_NATIVE_LAN_DATAGRAM_BYTES, NativeLanError, NativeLanReassembler,
    NativeLanReassemblyOutcome, encode_native_lan_fragment, native_lan_fragment_count,
};

const MIN_SOCKET_TIMEOUT: Duration = Duration::from_millis(1);
const MAX_SOCKET_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeLanEndpointConfig {
    pub peer: SocketAddr,
    pub read_timeout: Duration,
    pub inflight_packet_capacity: usize,
}

impl NativeLanEndpointConfig {
    pub fn validate(self) -> Result<Self, NativeLanError> {
        if self.peer.port() == 0
            || self.peer.ip().is_unspecified()
            || self.peer.ip().is_multicast()
            || self.peer.ip() == IpAddr::V4(Ipv4Addr::BROADCAST)
        {
            return Err(NativeLanError::InvalidConfiguration(
                "peer must be a concrete unicast IP and non-zero port",
            ));
        }
        if !(MIN_SOCKET_TIMEOUT..=MAX_SOCKET_TIMEOUT).contains(&self.read_timeout) {
            return Err(NativeLanError::InvalidConfiguration(
                "socket timeout is outside 1 ms..=2 s",
            ));
        }
        if self.inflight_packet_capacity == 0
            || self.inflight_packet_capacity > crate::MAX_NATIVE_LAN_INFLIGHT_PACKETS
        {
            return Err(NativeLanError::InvalidConfiguration(
                "in-flight packet capacity is outside 1..=8",
            ));
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeLanEndpointMetrics {
    pub packets_sent: u64,
    pub datagrams_sent: u64,
    pub bytes_sent: u64,
    pub datagrams_received: u64,
    pub bytes_received: u64,
    pub packets_received: u64,
    pub wrong_peer_datagrams: u64,
    pub malformed_datagrams: u64,
    pub duplicate_fragments: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeLanReceiveOutcome {
    Pending,
    Packet(AudioMediaPacket),
    DuplicateFragment,
    DroppedWrongPeer,
    DroppedMalformed,
}

#[derive(Debug)]
pub struct NativeLanUdpEndpoint {
    socket: UdpSocket,
    config: NativeLanEndpointConfig,
    binding: AudioMediaStreamBinding,
    reassembler: NativeLanReassembler,
    metrics: NativeLanEndpointMetrics,
}

impl NativeLanUdpEndpoint {
    pub fn bind(
        local: SocketAddr,
        config: NativeLanEndpointConfig,
        binding: AudioMediaStreamBinding,
    ) -> Result<Self, NativeLanError> {
        Self::from_socket(bind_udp_socket(local)?, config, binding)
    }

    pub fn from_socket(
        socket: UdpSocket,
        config: NativeLanEndpointConfig,
        binding: AudioMediaStreamBinding,
    ) -> Result<Self, NativeLanError> {
        let config = config.validate()?;
        binding.validate()?;
        if socket.local_addr()?.is_ipv4() != config.peer.is_ipv4() {
            return Err(NativeLanError::InvalidConfiguration(
                "local socket and peer use different IP families",
            ));
        }
        socket.set_read_timeout(Some(config.read_timeout))?;
        socket.set_write_timeout(Some(config.read_timeout))?;
        let reassembler =
            NativeLanReassembler::new(binding.clone(), config.inflight_packet_capacity)?;
        Ok(Self {
            socket,
            config,
            binding,
            reassembler,
            metrics: NativeLanEndpointMetrics::default(),
        })
    }

    pub fn send_packet(&mut self, packet: &AudioMediaPacket) -> Result<(), NativeLanError> {
        packet.validate_against(&self.binding)?;
        let fragment_count = native_lan_fragment_count(packet.payload.len())?;
        let mut datagram = [0_u8; MAX_NATIVE_LAN_DATAGRAM_BYTES];
        for fragment_index in 0..fragment_count {
            let bytes =
                encode_native_lan_fragment(&self.binding, packet, fragment_index, &mut datagram)?;
            let sent = self.socket.send_to(&datagram[..bytes], self.config.peer)?;
            if sent != bytes {
                return Err(NativeLanError::Io(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "UDP send returned a partial datagram",
                )));
            }
            self.metrics.datagrams_sent = self.metrics.datagrams_sent.saturating_add(1);
            self.metrics.bytes_sent = self.metrics.bytes_sent.saturating_add(sent as u64);
        }
        self.metrics.packets_sent = self.metrics.packets_sent.saturating_add(1);
        Ok(())
    }

    pub fn receive(&mut self) -> Result<NativeLanReceiveOutcome, NativeLanError> {
        let mut datagram = [0_u8; MAX_NATIVE_LAN_DATAGRAM_BYTES + 1];
        let (bytes, peer) = match self.socket.recv_from(&mut datagram) {
            Ok(value) => value,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Err(NativeLanError::ReceiveTimeout);
            }
            Err(error) => return Err(error.into()),
        };
        self.metrics.datagrams_received = self.metrics.datagrams_received.saturating_add(1);
        self.metrics.bytes_received = self.metrics.bytes_received.saturating_add(bytes as u64);
        if peer != self.config.peer {
            self.metrics.wrong_peer_datagrams = self.metrics.wrong_peer_datagrams.saturating_add(1);
            return Ok(NativeLanReceiveOutcome::DroppedWrongPeer);
        }
        if bytes > MAX_NATIVE_LAN_DATAGRAM_BYTES {
            self.metrics.malformed_datagrams = self.metrics.malformed_datagrams.saturating_add(1);
            return Ok(NativeLanReceiveOutcome::DroppedMalformed);
        }

        match self.reassembler.push_datagram(&datagram[..bytes]) {
            Ok(NativeLanReassemblyOutcome::Pending) => Ok(NativeLanReceiveOutcome::Pending),
            Ok(NativeLanReassemblyOutcome::Complete(packet)) => {
                self.metrics.packets_received = self.metrics.packets_received.saturating_add(1);
                Ok(NativeLanReceiveOutcome::Packet(packet))
            }
            Ok(NativeLanReassemblyOutcome::DuplicateFragment) => {
                self.metrics.duplicate_fragments =
                    self.metrics.duplicate_fragments.saturating_add(1);
                Ok(NativeLanReceiveOutcome::DuplicateFragment)
            }
            Ok(NativeLanReassemblyOutcome::WrongBinding) | Err(_) => {
                self.metrics.malformed_datagrams =
                    self.metrics.malformed_datagrams.saturating_add(1);
                Ok(NativeLanReceiveOutcome::DroppedMalformed)
            }
        }
    }

    pub fn local_addr(&self) -> Result<SocketAddr, NativeLanError> {
        Ok(self.socket.local_addr()?)
    }

    #[must_use]
    pub const fn binding(&self) -> &AudioMediaStreamBinding {
        &self.binding
    }

    #[must_use]
    pub const fn metrics(&self) -> NativeLanEndpointMetrics {
        self.metrics
    }

    #[must_use]
    pub const fn reassembler(&self) -> &NativeLanReassembler {
        &self.reassembler
    }
}

fn bind_udp_socket(local: SocketAddr) -> io::Result<UdpSocket> {
    UdpSocket::bind(local)
}
