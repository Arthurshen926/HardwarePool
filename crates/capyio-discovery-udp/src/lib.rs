#![forbid(unsafe_code)]

//! Platform-hosted IPv4 multicast I/O for unauthenticated discovery hints.
//! Binding is explicit: constructing a Node or UI never opens this socket.

use std::{
    io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    time::Duration,
};

use capyio_discovery::{DISCOVERY_RECORD_BYTES, DiscoveryAnnouncement};
use thiserror::Error;

pub const DISCOVERY_MULTICAST_GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 67, 80);
pub const DISCOVERY_UDP_PORT: u16 = 54_785;
pub const DISCOVERY_MULTICAST_TTL: u32 = 1;
pub const DISCOVERY_RECEIVE_TIMEOUT: Duration = Duration::from_millis(250);

pub struct DiscoveryUdpSocket {
    socket: UdpSocket,
}

/// Send-only discovery owner for platforms which rely on the operating
/// system's active IPv4 route. It does not join a group or open the fixed
/// receive port.
pub struct DiscoveryUdpAnnouncer {
    socket: UdpSocket,
}

impl DiscoveryUdpAnnouncer {
    pub fn bind_default() -> Result<Self, DiscoveryUdpError> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_multicast_ttl_v4(DISCOVERY_MULTICAST_TTL)?;
        socket.set_multicast_loop_v4(false)?;
        Ok(Self { socket })
    }

    pub fn announce(&self, announcement: DiscoveryAnnouncement) -> Result<(), DiscoveryUdpError> {
        send_announcement(&self.socket, announcement)
    }

    pub fn local_addr(&self) -> Result<SocketAddr, DiscoveryUdpError> {
        self.socket.local_addr().map_err(Into::into)
    }
}

impl DiscoveryUdpSocket {
    /// Joins on one IPv4 interface. The caller owns interface selection and
    /// must pass a concrete non-loopback unicast address.
    pub fn bind(interface: Ipv4Addr) -> Result<Self, DiscoveryUdpError> {
        if interface.is_unspecified()
            || interface.is_loopback()
            || interface.is_multicast()
            || interface.is_broadcast()
        {
            return Err(DiscoveryUdpError::InvalidInterface);
        }
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, DISCOVERY_UDP_PORT))?;
        socket.join_multicast_v4(&DISCOVERY_MULTICAST_GROUP, &interface)?;
        socket.set_multicast_ttl_v4(DISCOVERY_MULTICAST_TTL)?;
        socket.set_multicast_loop_v4(false)?;
        socket.set_read_timeout(Some(DISCOVERY_RECEIVE_TIMEOUT))?;
        Ok(Self { socket })
    }

    pub fn announce(&self, announcement: DiscoveryAnnouncement) -> Result<(), DiscoveryUdpError> {
        send_announcement(&self.socket, announcement)
    }

    pub fn receive(&self) -> Result<DiscoveryReceive, DiscoveryUdpError> {
        let mut datagram = [0_u8; DISCOVERY_RECORD_BYTES + 1];
        match self.socket.recv_from(&mut datagram) {
            Ok((length, source)) => Ok(DiscoveryReceive::Datagram(DiscoveryDatagram {
                source,
                bytes: datagram,
                length,
            })),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                Ok(DiscoveryReceive::Idle)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn local_addr(&self) -> Result<SocketAddr, DiscoveryUdpError> {
        self.socket.local_addr().map_err(Into::into)
    }
}

fn send_announcement(
    socket: &UdpSocket,
    announcement: DiscoveryAnnouncement,
) -> Result<(), DiscoveryUdpError> {
    let sent = socket.send_to(
        &announcement.encode(),
        SocketAddrV4::new(DISCOVERY_MULTICAST_GROUP, DISCOVERY_UDP_PORT),
    )?;
    if sent != DISCOVERY_RECORD_BYTES {
        return Err(DiscoveryUdpError::PartialDatagram(sent));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryReceive {
    Idle,
    Datagram(DiscoveryDatagram),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscoveryDatagram {
    pub source: SocketAddr,
    bytes: [u8; DISCOVERY_RECORD_BYTES + 1],
    length: usize,
}

impl DiscoveryDatagram {
    pub fn payload(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}

#[derive(Debug, Error)]
pub enum DiscoveryUdpError {
    #[error("discovery interface must be a concrete non-loopback IPv4 unicast address")]
    InvalidInterface,
    #[error("discovery UDP datagram was partially sent: {0} bytes")]
    PartialDatagram(usize),
    #[error("discovery UDP I/O failed: {0}")]
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_policy_is_fixed_private_scope_and_one_hop() {
        assert!(DISCOVERY_MULTICAST_GROUP.is_multicast());
        assert_eq!(DISCOVERY_MULTICAST_GROUP.octets()[0], 239);
        assert!((49_152..=65_535).contains(&DISCOVERY_UDP_PORT));
        assert_eq!(DISCOVERY_MULTICAST_TTL, 1);
        assert!(DISCOVERY_RECEIVE_TIMEOUT <= Duration::from_secs(1));
    }

    #[test]
    fn invalid_interface_fails_before_opening_a_listener() {
        assert!(matches!(
            DiscoveryUdpSocket::bind(Ipv4Addr::UNSPECIFIED),
            Err(DiscoveryUdpError::InvalidInterface)
        ));
        assert!(matches!(
            DiscoveryUdpSocket::bind(DISCOVERY_MULTICAST_GROUP),
            Err(DiscoveryUdpError::InvalidInterface)
        ));
        assert!(matches!(
            DiscoveryUdpSocket::bind(Ipv4Addr::LOCALHOST),
            Err(DiscoveryUdpError::InvalidInterface)
        ));
        assert!(matches!(
            DiscoveryUdpSocket::bind(Ipv4Addr::BROADCAST),
            Err(DiscoveryUdpError::InvalidInterface)
        ));
    }

    #[test]
    fn receive_timeout_is_an_idle_poll() {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("loopback UDP");
        socket
            .set_read_timeout(Some(Duration::from_millis(1)))
            .expect("timeout");
        let host = DiscoveryUdpSocket { socket };
        assert_eq!(
            host.receive().expect("idle receive"),
            DiscoveryReceive::Idle
        );
    }

    #[test]
    fn default_announcer_uses_an_ephemeral_send_socket() {
        let announcer = DiscoveryUdpAnnouncer::bind_default().expect("announcer");
        assert_ne!(announcer.local_addr().expect("address").port(), 0);
    }
}
