use capyio_core::{CapabilityId, CoreError, NodeId, SessionId};
use thiserror::Error;

use crate::OperationId;

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

    #[error("operation limits must be greater than zero")]
    InvalidOperationLimits,

    #[error("pending operation limit {limit} reached")]
    PendingOperationLimitReached { limit: usize },

    #[error("operation identifier space is exhausted")]
    OperationIdExhausted,

    #[error("operation {0} is unknown or no longer retained")]
    UnknownOperation(OperationId),

    #[error("invalid operation completion: {0}")]
    InvalidOperationCompletion(&'static str),

    #[error(transparent)]
    Core(#[from] CoreError),
}
