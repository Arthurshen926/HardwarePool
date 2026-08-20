use hardwarepool_core::{CapabilityId, CoreError, NodeId, SessionId};
use thiserror::Error;

/// Runtime lookup, availability and domain errors.
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("peer {0} is unknown")]
    UnknownPeer(NodeId),

    #[error("peer {0} is offline")]
    PeerOffline(NodeId),

    #[error("session {0} is unknown")]
    UnknownSession(SessionId),

    #[error("capability {capability_id} is not advertised by peer {peer_id}")]
    CapabilityNotAdvertised {
        peer_id: NodeId,
        capability_id: CapabilityId,
    },

    #[error(transparent)]
    Core(#[from] CoreError),
}
