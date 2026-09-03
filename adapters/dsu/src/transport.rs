use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};

use capyio_input::GamepadControls;

use crate::{
    DSU_PAD_DATA_PACKET_BYTES, DsuControlsMapping, DsuMotionSample, DsuPacketError, DsuRequest,
    encode_pad_data, encode_port_info_response, encode_version_response, parse_client_request,
    protocol::validate_dsu_controls,
};

pub const DSU_CONVENTIONAL_PORT: u16 = 26_760;
pub const MAX_DSU_SUBSCRIBERS: usize = 16;
pub const DEFAULT_DSU_SUBSCRIBER_CAPACITY: usize = 8;
pub const MIN_DSU_SUBSCRIPTION_TTL_MILLIS: u64 = 250;
pub const MAX_DSU_SUBSCRIPTION_TTL_MILLIS: u64 = 30_000;
pub const DEFAULT_DSU_SUBSCRIPTION_TTL_MILLIS: u64 = 5_000;
pub const MAX_DSU_DATAGRAMS_PER_POLL: usize = 64;
pub const DEFAULT_DSU_DATAGRAMS_PER_POLL: usize = 16;

const MAX_UDP_DATAGRAM_BYTES: usize = 65_535;
const PROJECTED_SLOT: u8 = 0;
const PROJECTED_MAC: Option<[u8; 6]> = None;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DsuLoopbackConfig {
    /// `0` asks the operating system for an ephemeral test port.
    pub port: u16,
    /// The caller owns generation and must keep this stable for one server run.
    pub server_id: u32,
    pub subscriber_capacity: usize,
    pub subscription_ttl_millis: u64,
    pub datagrams_per_poll: usize,
}

impl DsuLoopbackConfig {
    #[must_use]
    pub const fn local_lab(port: u16, server_id: u32) -> Self {
        Self {
            port,
            server_id,
            subscriber_capacity: DEFAULT_DSU_SUBSCRIBER_CAPACITY,
            subscription_ttl_millis: DEFAULT_DSU_SUBSCRIPTION_TTL_MILLIS,
            datagrams_per_poll: DEFAULT_DSU_DATAGRAMS_PER_POLL,
        }
    }

    fn validate(self) -> Result<Self, DsuTransportError> {
        if self.subscriber_capacity == 0 || self.subscriber_capacity > MAX_DSU_SUBSCRIBERS {
            return Err(DsuTransportError::InvalidSubscriberCapacity {
                actual: self.subscriber_capacity,
                maximum: MAX_DSU_SUBSCRIBERS,
            });
        }
        if !(MIN_DSU_SUBSCRIPTION_TTL_MILLIS..=MAX_DSU_SUBSCRIPTION_TTL_MILLIS)
            .contains(&self.subscription_ttl_millis)
        {
            return Err(DsuTransportError::InvalidSubscriptionTtl {
                actual_millis: self.subscription_ttl_millis,
                minimum_millis: MIN_DSU_SUBSCRIPTION_TTL_MILLIS,
                maximum_millis: MAX_DSU_SUBSCRIPTION_TTL_MILLIS,
            });
        }
        if self.datagrams_per_poll == 0 || self.datagrams_per_poll > MAX_DSU_DATAGRAMS_PER_POLL {
            return Err(DsuTransportError::InvalidPollBudget {
                actual: self.datagrams_per_poll,
                maximum: MAX_DSU_DATAGRAMS_PER_POLL,
            });
        }
        Ok(self)
    }
}

#[derive(Debug)]
pub enum DsuTransportError {
    InvalidSubscriberCapacity {
        actual: usize,
        maximum: usize,
    },
    InvalidSubscriptionTtl {
        actual_millis: u64,
        minimum_millis: u64,
        maximum_millis: u64,
    },
    InvalidPollBudget {
        actual: usize,
        maximum: usize,
    },
    Bind(io::Error),
    Configure(io::Error),
    LocalAddress(io::Error),
    UnexpectedLocalAddress(SocketAddr),
    Receive(io::Error),
    MonotonicTimeRegressed {
        previous_millis: u64,
        actual_millis: u64,
    },
    MonotonicTimeOverflow,
    Packet(DsuPacketError),
}

impl Display for DsuTransportError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSubscriberCapacity { actual, maximum } => write!(
                formatter,
                "DSU subscriber capacity {actual} is outside 1..={maximum}"
            ),
            Self::InvalidSubscriptionTtl {
                actual_millis,
                minimum_millis,
                maximum_millis,
            } => write!(
                formatter,
                "DSU subscription TTL {actual_millis}ms is outside {minimum_millis}..={maximum_millis}ms"
            ),
            Self::InvalidPollBudget { actual, maximum } => write!(
                formatter,
                "DSU poll budget {actual} is outside 1..={maximum} datagrams"
            ),
            Self::Bind(error) => write!(formatter, "DSU loopback bind failed: {error}"),
            Self::Configure(error) => {
                write!(
                    formatter,
                    "DSU loopback socket configuration failed: {error}"
                )
            }
            Self::LocalAddress(error) => {
                write!(
                    formatter,
                    "DSU loopback local-address query failed: {error}"
                )
            }
            Self::UnexpectedLocalAddress(address) => {
                write!(
                    formatter,
                    "DSU socket returned unexpected local address {address}"
                )
            }
            Self::Receive(error) => write!(formatter, "DSU loopback receive failed: {error}"),
            Self::MonotonicTimeRegressed {
                previous_millis,
                actual_millis,
            } => write!(
                formatter,
                "DSU monotonic time regressed from {previous_millis}ms to {actual_millis}ms"
            ),
            Self::MonotonicTimeOverflow => {
                formatter.write_str("DSU subscription expiry overflowed monotonic time")
            }
            Self::Packet(error) => write!(formatter, "DSU packet encoding failed: {error}"),
        }
    }
}

impl Error for DsuTransportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Bind(error)
            | Self::Configure(error)
            | Self::LocalAddress(error)
            | Self::Receive(error) => Some(error),
            Self::Packet(error) => Some(error),
            Self::InvalidSubscriberCapacity { .. }
            | Self::InvalidSubscriptionTtl { .. }
            | Self::InvalidPollBudget { .. }
            | Self::UnexpectedLocalAddress(_)
            | Self::MonotonicTimeRegressed { .. }
            | Self::MonotonicTimeOverflow => None,
        }
    }
}

impl From<DsuPacketError> for DsuTransportError {
    fn from(error: DsuPacketError) -> Self {
        Self::Packet(error)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DsuPollStats {
    pub datagrams_received: usize,
    pub malformed_datagrams: usize,
    pub non_loopback_datagrams: usize,
    pub responses_sent: usize,
    pub responses_would_block: usize,
    pub response_send_errors: usize,
    pub subscriptions_added: usize,
    pub subscriptions_renewed: usize,
    pub subscriptions_replaced: usize,
    pub subscriptions_rejected_full: usize,
    pub selectors_without_projected_slot: usize,
    pub subscriptions_expired: usize,
    pub poll_budget_exhausted: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DsuPublishStats {
    pub active_subscribers: usize,
    pub subscriptions_expired: usize,
    pub packets_sent: usize,
    pub packets_would_block: usize,
    pub packet_send_errors: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Subscriber {
    endpoint: SocketAddrV4,
    client_id: u32,
    expires_at_millis: u64,
    next_packet_number: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RegistrationOutcome {
    Added,
    Renewed,
    Replaced,
    Full,
}

struct SubscriberRegistry {
    entries: [Option<Subscriber>; MAX_DSU_SUBSCRIBERS],
    capacity: usize,
    ttl_millis: u64,
    last_now_millis: Option<u64>,
}

impl SubscriberRegistry {
    fn new(capacity: usize, ttl_millis: u64) -> Self {
        Self {
            entries: [None; MAX_DSU_SUBSCRIBERS],
            capacity,
            ttl_millis,
            last_now_millis: None,
        }
    }

    fn advance_time(&mut self, now_millis: u64) -> Result<usize, DsuTransportError> {
        if self
            .last_now_millis
            .is_some_and(|previous| now_millis < previous)
        {
            return Err(DsuTransportError::MonotonicTimeRegressed {
                previous_millis: self.last_now_millis.unwrap_or_default(),
                actual_millis: now_millis,
            });
        }
        self.last_now_millis = Some(now_millis);
        let mut expired = 0;
        for entry in &mut self.entries[..self.capacity] {
            if entry.is_some_and(|subscriber| subscriber.expires_at_millis <= now_millis) {
                *entry = None;
                expired += 1;
            }
        }
        Ok(expired)
    }

    fn register(
        &mut self,
        endpoint: SocketAddrV4,
        client_id: u32,
        now_millis: u64,
    ) -> Result<RegistrationOutcome, DsuTransportError> {
        let expires_at_millis = now_millis
            .checked_add(self.ttl_millis)
            .ok_or(DsuTransportError::MonotonicTimeOverflow)?;
        if let Some(existing) = self.entries[..self.capacity]
            .iter_mut()
            .flatten()
            .find(|subscriber| subscriber.endpoint == endpoint)
        {
            let outcome = if existing.client_id == client_id {
                RegistrationOutcome::Renewed
            } else {
                existing.client_id = client_id;
                existing.next_packet_number = 0;
                RegistrationOutcome::Replaced
            };
            existing.expires_at_millis = expires_at_millis;
            return Ok(outcome);
        }
        let Some(empty) = self.entries[..self.capacity]
            .iter_mut()
            .find(|entry| entry.is_none())
        else {
            return Ok(RegistrationOutcome::Full);
        };
        *empty = Some(Subscriber {
            endpoint,
            client_id,
            expires_at_millis,
            next_packet_number: 0,
        });
        Ok(RegistrationOutcome::Added)
    }

    fn len(&self) -> usize {
        self.entries[..self.capacity]
            .iter()
            .filter(|entry| entry.is_some())
            .count()
    }
}

/// Caller-polled DSU endpoint restricted to the IPv4 loopback interface.
///
/// The server owns no thread or wall clock. Its owner supplies a nondecreasing
/// monotonic millisecond value to `poll` and `publish_motion`, so Route/session
/// lifecycle remains outside the protocol Adapter and expiry is deterministic.
pub struct DsuLoopbackServer {
    socket: UdpSocket,
    local_address: SocketAddrV4,
    receive_buffer: Box<[u8]>,
    server_id: u32,
    datagrams_per_poll: usize,
    subscribers: SubscriberRegistry,
}

impl DsuLoopbackServer {
    pub fn bind(config: DsuLoopbackConfig) -> Result<Self, DsuTransportError> {
        let config = config.validate()?;
        let bind_address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, config.port);
        let socket = UdpSocket::bind(bind_address).map_err(DsuTransportError::Bind)?;
        socket
            .set_nonblocking(true)
            .map_err(DsuTransportError::Configure)?;
        let local_address = match socket
            .local_addr()
            .map_err(DsuTransportError::LocalAddress)?
        {
            SocketAddr::V4(address) if address.ip().is_loopback() => address,
            address => return Err(DsuTransportError::UnexpectedLocalAddress(address)),
        };
        Ok(Self {
            socket,
            local_address,
            receive_buffer: vec![0_u8; MAX_UDP_DATAGRAM_BYTES].into_boxed_slice(),
            server_id: config.server_id,
            datagrams_per_poll: config.datagrams_per_poll,
            subscribers: SubscriberRegistry::new(
                config.subscriber_capacity,
                config.subscription_ttl_millis,
            ),
        })
    }

    #[must_use]
    pub const fn local_address(&self) -> SocketAddrV4 {
        self.local_address
    }

    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }

    pub fn poll(&mut self, now_millis: u64) -> Result<DsuPollStats, DsuTransportError> {
        let mut stats = DsuPollStats {
            subscriptions_expired: self.subscribers.advance_time(now_millis)?,
            ..DsuPollStats::default()
        };
        for _ in 0..self.datagrams_per_poll {
            let (received, endpoint) = match self.socket.recv_from(&mut self.receive_buffer) {
                Ok(value) => value,
                Err(error) if is_idle_udp_receive(error.kind()) => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(DsuTransportError::Receive(error)),
            };
            stats.datagrams_received += 1;
            let SocketAddr::V4(endpoint) = endpoint else {
                stats.non_loopback_datagrams += 1;
                continue;
            };
            if !endpoint.ip().is_loopback() {
                stats.non_loopback_datagrams += 1;
                continue;
            }
            let request = match parse_client_request(&self.receive_buffer[..received]) {
                Ok(request) => request,
                Err(_) => {
                    stats.malformed_datagrams += 1;
                    continue;
                }
            };
            match request {
                DsuRequest::Version { .. } => {
                    let packet = encode_version_response(self.server_id);
                    self.send_response(&packet, endpoint, &mut stats);
                }
                DsuRequest::PortInfo {
                    requested_slots, ..
                } => {
                    for slot in requested_slots.as_slice() {
                        let packet = encode_port_info_response(self.server_id, *slot)?;
                        self.send_response(&packet, endpoint, &mut stats);
                    }
                }
                DsuRequest::PadData {
                    client_id,
                    selector,
                } => {
                    if !selector.selects(PROJECTED_SLOT, PROJECTED_MAC) {
                        stats.selectors_without_projected_slot += 1;
                        continue;
                    }
                    match self.subscribers.register(endpoint, client_id, now_millis)? {
                        RegistrationOutcome::Added => stats.subscriptions_added += 1,
                        RegistrationOutcome::Renewed => stats.subscriptions_renewed += 1,
                        RegistrationOutcome::Replaced => stats.subscriptions_replaced += 1,
                        RegistrationOutcome::Full => stats.subscriptions_rejected_full += 1,
                    }
                }
            }
        }
        stats.poll_budget_exhausted = stats.datagrams_received == self.datagrams_per_poll;
        Ok(stats)
    }

    pub fn publish_motion(
        &mut self,
        now_millis: u64,
        motion: DsuMotionSample,
    ) -> Result<DsuPublishStats, DsuTransportError> {
        self.publish_state(
            now_millis,
            motion,
            GamepadControls::neutral(),
            DsuControlsMapping::identity(),
        )
    }

    /// Publishes one combined normalized-control and motion snapshot.
    ///
    /// Subscription expiry and packet numbers remain owned by this endpoint;
    /// validation and semantic-to-DSU field mapping remain in the codec.
    pub fn publish_state(
        &mut self,
        now_millis: u64,
        motion: DsuMotionSample,
        controls: GamepadControls,
        mapping: DsuControlsMapping,
    ) -> Result<DsuPublishStats, DsuTransportError> {
        validate_dsu_controls(controls)?;
        let mut stats = DsuPublishStats {
            subscriptions_expired: self.subscribers.advance_time(now_millis)?,
            active_subscribers: self.subscribers.len(),
            ..DsuPublishStats::default()
        };
        let socket = &self.socket;
        for subscriber in self.subscribers.entries[..self.subscribers.capacity]
            .iter_mut()
            .flatten()
        {
            let packet: [u8; DSU_PAD_DATA_PACKET_BYTES] = encode_pad_data(
                self.server_id,
                PROJECTED_SLOT,
                subscriber.next_packet_number,
                motion,
                controls,
                mapping,
            )?;
            match socket.send_to(&packet, subscriber.endpoint) {
                Ok(sent) if sent == packet.len() => {
                    stats.packets_sent += 1;
                    subscriber.next_packet_number = subscriber.next_packet_number.wrapping_add(1);
                }
                Ok(_) => stats.packet_send_errors += 1,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    stats.packets_would_block += 1;
                }
                Err(_) => stats.packet_send_errors += 1,
            }
        }
        Ok(stats)
    }

    fn send_response(&self, packet: &[u8], endpoint: SocketAddrV4, stats: &mut DsuPollStats) {
        match self.socket.send_to(packet, endpoint) {
            Ok(sent) if sent == packet.len() => stats.responses_sent += 1,
            Ok(_) => stats.response_send_errors += 1,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                stats.responses_would_block += 1;
            }
            Err(_) => stats.response_send_errors += 1,
        }
    }
}

/// A Windows UDP socket reports `ConnectionReset` after an earlier send reaches
/// a subscriber that has already closed its port. UDP has no connection to
/// reset, so this is an idle poll condition; the bounded subscriber TTL remains
/// responsible for removing that stale endpoint.
const fn is_idle_udp_receive(kind: io::ErrorKind) -> bool {
    matches!(
        kind,
        io::ErrorKind::WouldBlock | io::ErrorKind::ConnectionReset
    )
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::is_idle_udp_receive;

    #[test]
    fn udp_connection_reset_is_treated_as_an_idle_poll() {
        assert!(is_idle_udp_receive(io::ErrorKind::WouldBlock));
        assert!(is_idle_udp_receive(io::ErrorKind::ConnectionReset));
        assert!(!is_idle_udp_receive(io::ErrorKind::PermissionDenied));
        assert!(!is_idle_udp_receive(io::ErrorKind::InvalidData));
    }
}
