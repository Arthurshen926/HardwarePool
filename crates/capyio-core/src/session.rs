use serde::{Deserialize, Serialize};

use crate::{CoreError, NodeId, SessionId};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Opening,
    Ready,
    Suspended,
    Closing,
    Closed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub local_node_id: NodeId,
    pub remote_node_id: NodeId,
    pub state: SessionState,
}

impl Session {
    #[must_use]
    pub fn new(local_node_id: NodeId, remote_node_id: NodeId) -> Self {
        Self::with_id(SessionId::new(), local_node_id, remote_node_id)
    }

    #[must_use]
    pub const fn with_id(id: SessionId, local_node_id: NodeId, remote_node_id: NodeId) -> Self {
        Self {
            id,
            local_node_id,
            remote_node_id,
            state: SessionState::Ready,
        }
    }

    pub fn mark_remote_offline(&mut self) -> Result<(), CoreError> {
        if self.state != SessionState::Ready {
            return Err(CoreError::InvalidSessionTransition {
                from: self.state,
                action: "mark_remote_offline",
            });
        }
        self.state = SessionState::Suspended;
        Ok(())
    }

    pub fn mark_remote_online(&mut self) -> Result<(), CoreError> {
        if self.state != SessionState::Suspended {
            return Err(CoreError::InvalidSessionTransition {
                from: self.state,
                action: "mark_remote_online",
            });
        }
        self.state = SessionState::Ready;
        Ok(())
    }

    pub fn begin_close(&mut self) -> Result<(), CoreError> {
        if !matches!(
            self.state,
            SessionState::Ready | SessionState::Suspended | SessionState::Failed
        ) {
            return Err(CoreError::InvalidSessionTransition {
                from: self.state,
                action: "begin_close",
            });
        }
        self.state = SessionState::Closing;
        Ok(())
    }

    pub fn mark_closed(&mut self) -> Result<(), CoreError> {
        if self.state != SessionState::Closing {
            return Err(CoreError::InvalidSessionTransition {
                from: self.state,
                action: "mark_closed",
            });
        }
        self.state = SessionState::Closed;
        Ok(())
    }
}
