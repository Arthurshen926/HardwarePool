use std::net::SocketAddr;

use capyio_process_presence::{
    ProcessPresenceError, TcpPeerPresence, process_owned_tcp_peer_presence,
};

use crate::AudioShareError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiverTcpPresence {
    /// The process is not running, so receiver presence cannot be queried.
    SupervisorNotRunning,
    /// The platform has no reviewed process-owned TCP table implementation.
    UnsupportedPlatform,
    /// The server runs but owns no established TCP connection on its bind port.
    Disconnected,
    /// One or more process-owned TCP connections are established. This proves
    /// transport presence only, not Audio Share negotiation or audible playback.
    Established { connection_count: usize },
}

pub(crate) fn receiver_tcp_presence(
    process_id: u32,
    bind_address: SocketAddr,
) -> Result<ReceiverTcpPresence, AudioShareError> {
    process_owned_tcp_peer_presence(process_id, bind_address)
        .map(|presence| match presence {
            TcpPeerPresence::UnsupportedPlatform => ReceiverTcpPresence::UnsupportedPlatform,
            TcpPeerPresence::Disconnected => ReceiverTcpPresence::Disconnected,
            TcpPeerPresence::Established { connection_count } => {
                ReceiverTcpPresence::Established { connection_count }
            }
        })
        .map_err(map_error)
}

fn map_error(error: ProcessPresenceError) -> AudioShareError {
    match error {
        ProcessPresenceError::TableQueryFailed { code } => {
            AudioShareError::PeerTableQueryFailed { code }
        }
        ProcessPresenceError::TableTooLarge { limit } => {
            AudioShareError::PeerTableTooLarge { limit }
        }
        ProcessPresenceError::InvalidTableLayout => AudioShareError::InvalidPeerTableLayout,
    }
}
