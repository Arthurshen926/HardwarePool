use capyio_core::{NodeDescriptor, Session};
use serde::{Deserialize, Serialize};

use crate::{OperationRecord, RuntimeEvent};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerSnapshot {
    pub descriptor: NodeDescriptor,
    pub online: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeSnapshot {
    pub local_node: NodeDescriptor,
    pub peers: Vec<PeerSnapshot>,
    pub sessions: Vec<Session>,
    pub operations: Vec<OperationRecord>,
    pub events: Vec<RuntimeEvent>,
}
