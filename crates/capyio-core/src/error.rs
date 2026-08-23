use thiserror::Error;

use crate::{BindingState, CapabilityId, ProjectionKind, SessionId, SessionPhase};

/// Domain validation and lifecycle errors.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("invalid profile: {0}")]
    InvalidProfile(String),

    #[error("invalid audio format: {0}")]
    InvalidAudioFormat(String),

    #[error("invalid capability {capability_id}: {reason}")]
    InvalidCapability {
        capability_id: CapabilityId,
        reason: String,
    },

    #[error("capability {capability_id} does not support projection {mapping:?}")]
    UnsupportedProjection {
        capability_id: CapabilityId,
        mapping: ProjectionKind,
    },

    #[error("capability {0} is not present in the session")]
    UnknownCapability(CapabilityId),

    #[error("node already contains capability {0}")]
    DuplicateCapability(CapabilityId),

    #[error("session {0} is unknown")]
    UnknownSession(SessionId),

    #[error("binding transition '{action}' is not valid from {from:?}")]
    InvalidBindingTransition {
        from: BindingState,
        action: &'static str,
    },

    #[error("session transition '{action}' is not valid from {from:?}")]
    InvalidSessionTransition {
        from: SessionPhase,
        action: &'static str,
    },

    #[error("authorization lease must expire after it is issued")]
    InvalidLease,

    #[error("authorization lease has expired")]
    LeaseExpired,

    #[error("audio format was not advertised by capability {0}")]
    UnsupportedAudioFormat(CapabilityId),

    #[error("capability {0} is not an audio capability")]
    NotAudioCapability(CapabilityId),

    #[error("a live binding already exists for capability {0}")]
    BindingAlreadyExists(CapabilityId),
}
