use std::net::SocketAddr;

use capyio_process_presence::{
    ProcessPresenceError, TcpPeerPresence, process_owned_tcp_peer_presence,
};

use crate::MicYouError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerTcpPresence {
    SupervisorNotRunning,
    UnsupportedPlatform,
    Disconnected,
    Established { connection_count: usize },
}

pub(crate) fn peer_tcp_presence(
    process_id: u32,
    bind_address: SocketAddr,
) -> Result<PeerTcpPresence, MicYouError> {
    process_owned_tcp_peer_presence(process_id, bind_address)
        .map(|presence| match presence {
            TcpPeerPresence::UnsupportedPlatform => PeerTcpPresence::UnsupportedPlatform,
            TcpPeerPresence::Disconnected => PeerTcpPresence::Disconnected,
            TcpPeerPresence::Established { connection_count } => {
                PeerTcpPresence::Established { connection_count }
            }
        })
        .map_err(map_error)
}

fn map_error(error: ProcessPresenceError) -> MicYouError {
    match error {
        ProcessPresenceError::TableQueryFailed { code } => {
            MicYouError::PeerTableQueryFailed { code }
        }
        ProcessPresenceError::TableTooLarge { limit } => MicYouError::PeerTableTooLarge { limit },
        ProcessPresenceError::InvalidTableLayout => MicYouError::InvalidPeerTableLayout,
    }
}
