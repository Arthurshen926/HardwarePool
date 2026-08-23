use serde::{Deserialize, Serialize};

use crate::{AdapterInstanceId, CoreError, NodeId, ProblemId, RouteId};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProblemCategory {
    Protocol,
    Identity,
    Authorization,
    Capability,
    Route,
    Adapter,
    Transport,
    Platform,
    Data,
    Driver,
    Internal,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProblemSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Problem {
    pub id: ProblemId,
    pub code: String,
    pub category: ProblemCategory,
    pub severity: ProblemSeverity,
    pub retryable: bool,
    pub related_node: Option<NodeId>,
    pub related_adapter: Option<AdapterInstanceId>,
    pub related_route: Option<RouteId>,
    pub human_message: String,
    pub technical_detail: Option<String>,
}

impl Problem {
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.code.trim().is_empty() || self.human_message.trim().is_empty() {
            return Err(CoreError::InvalidProblem(
                "Problem code and human message cannot be empty".to_owned(),
            ));
        }
        if self
            .technical_detail
            .as_ref()
            .is_some_and(|detail| detail.len() > 4096)
        {
            return Err(CoreError::InvalidProblem(
                "technical detail exceeds 4096 characters".to_owned(),
            ));
        }
        Ok(())
    }
}
