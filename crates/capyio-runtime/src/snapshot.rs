use capyio_core::{NodeDescriptor, Problem, Route, Session};
use serde::{Deserialize, Serialize};

use crate::{OperationRecord, RuntimeEvent};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeSnapshot {
    pub local_node: NodeDescriptor,
    pub peers: Vec<NodeDescriptor>,
    pub sessions: Vec<Session>,
    pub routes: Vec<Route>,
    pub operations: Vec<OperationRecord>,
    pub problems: Vec<Problem>,
    pub events: Vec<RuntimeEvent>,
}
