use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    CapabilityId, CoreError, FormatDescriptor, NodeId, PortDescriptor, PortDirection, PortId,
    ProblemId, ProfileId, QosMode, RouteId, SessionId,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteBackend {
    CapyDataPlane,
    AdapterManaged,
    LocalPipeline,
    ExternalProtocol,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteState {
    Draft,
    Prepared,
    Starting,
    Active,
    Stopping,
    Stopped,
    Failed,
    Offline,
}

impl RouteState {
    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(
            self,
            Self::Prepared | Self::Starting | Self::Active | Self::Stopping
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AuthorizationState {
    Pending,
    Authorized { expires_at_ms: Option<u64> },
    Denied,
    Revoked,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct PortRef {
    pub node_id: NodeId,
    pub capability_id: CapabilityId,
    pub port_id: PortId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub id: RouteId,
    pub session_id: SessionId,
    pub source: PortRef,
    pub sink: PortRef,
    pub profile: ProfileId,
    pub backend: RouteBackend,
    pub compatible_formats: Vec<FormatDescriptor>,
    pub compatible_qos_modes: BTreeSet<QosMode>,
    pub selected_format: Option<FormatDescriptor>,
    pub selected_qos_mode: Option<QosMode>,
    pub state: RouteState,
    pub authorization: AuthorizationState,
    pub epoch: u64,
    pub diagnostic_ids: Vec<ProblemId>,
}

impl Route {
    pub fn new(
        id: RouteId,
        session_id: SessionId,
        source: PortRef,
        source_port: &PortDescriptor,
        sink: PortRef,
        sink_port: &PortDescriptor,
        backend: RouteBackend,
    ) -> Result<Self, CoreError> {
        validate_ref(source, source_port)?;
        validate_ref(sink, sink_port)?;
        if source_port.direction != PortDirection::Source {
            return Err(CoreError::InvalidRouteEndpoint {
                port_id: source_port.id,
                expected: PortDirection::Source,
                actual: source_port.direction,
            });
        }
        if sink_port.direction != PortDirection::Sink {
            return Err(CoreError::InvalidRouteEndpoint {
                port_id: sink_port.id,
                expected: PortDirection::Sink,
                actual: sink_port.direction,
            });
        }
        if source_port.profile != sink_port.profile {
            return Err(CoreError::IncompatibleProfiles {
                source_profile: source_port.profile.clone(),
                sink_profile: sink_port.profile.clone(),
            });
        }
        if source_port.interoperability_mode != sink_port.interoperability_mode {
            return Err(CoreError::IncompatibleInteroperabilityModes);
        }
        let compatible_formats = source_port
            .formats
            .iter()
            .filter(|format| sink_port.formats.contains(format))
            .cloned()
            .collect::<Vec<_>>();
        if !source_port.formats.is_empty()
            && !sink_port.formats.is_empty()
            && compatible_formats.is_empty()
        {
            return Err(CoreError::NoCompatibleFormat);
        }
        let compatible_qos_modes = source_port
            .qos_modes
            .intersection(&sink_port.qos_modes)
            .cloned()
            .collect::<BTreeSet<_>>();
        if compatible_qos_modes.is_empty() {
            return Err(CoreError::NoCompatibleQos);
        }

        Ok(Self {
            id,
            session_id,
            source,
            sink,
            profile: source_port.profile.clone(),
            backend,
            compatible_formats,
            compatible_qos_modes,
            selected_format: None,
            selected_qos_mode: None,
            state: RouteState::Draft,
            authorization: AuthorizationState::Pending,
            epoch: 0,
            diagnostic_ids: Vec::new(),
        })
    }

    pub fn authorize(&mut self, expires_at_ms: Option<u64>) -> Result<(), CoreError> {
        if !matches!(
            self.authorization,
            AuthorizationState::Pending | AuthorizationState::Revoked
        ) {
            return Err(CoreError::InvalidAuthorizationTransition);
        }
        self.authorization = AuthorizationState::Authorized { expires_at_ms };
        Ok(())
    }

    pub fn prepare(
        &mut self,
        selected_format: Option<FormatDescriptor>,
        selected_qos_mode: QosMode,
        now_ms: u64,
    ) -> Result<(), CoreError> {
        require_state(
            self.state,
            &[RouteState::Draft, RouteState::Stopped],
            "prepare",
        )?;
        ensure_authorized(self.authorization, now_ms)?;
        if self.compatible_formats.is_empty() {
            if selected_format.is_some() {
                return Err(CoreError::UnsupportedRouteFormat);
            }
        } else if selected_format
            .as_ref()
            .is_none_or(|format| !self.compatible_formats.contains(format))
        {
            return Err(CoreError::UnsupportedRouteFormat);
        }
        if !self.compatible_qos_modes.contains(&selected_qos_mode) {
            return Err(CoreError::UnsupportedRouteQos);
        }
        self.selected_format = selected_format;
        self.selected_qos_mode = Some(selected_qos_mode);
        self.state = RouteState::Prepared;
        Ok(())
    }

    pub fn begin_start(&mut self, now_ms: u64) -> Result<(), CoreError> {
        require_state(self.state, &[RouteState::Prepared], "begin_start")?;
        ensure_authorized(self.authorization, now_ms)?;
        self.epoch = self.epoch.saturating_add(1);
        self.state = RouteState::Starting;
        Ok(())
    }

    pub fn mark_active(&mut self) -> Result<(), CoreError> {
        require_state(self.state, &[RouteState::Starting], "mark_active")?;
        self.state = RouteState::Active;
        Ok(())
    }

    pub fn begin_stop(&mut self) -> Result<(), CoreError> {
        require_state(
            self.state,
            &[
                RouteState::Prepared,
                RouteState::Starting,
                RouteState::Active,
                RouteState::Offline,
            ],
            "begin_stop",
        )?;
        self.state = RouteState::Stopping;
        Ok(())
    }

    pub fn mark_stopped(&mut self) -> Result<(), CoreError> {
        require_state(self.state, &[RouteState::Stopping], "mark_stopped")?;
        self.state = RouteState::Stopped;
        Ok(())
    }

    pub fn mark_failed(&mut self, problem_id: ProblemId) -> Result<(), CoreError> {
        if matches!(self.state, RouteState::Stopped | RouteState::Failed) {
            return Err(CoreError::InvalidRouteTransition {
                from: self.state,
                action: "mark_failed",
            });
        }
        self.state = RouteState::Failed;
        self.diagnostic_ids.push(problem_id);
        Ok(())
    }

    pub fn mark_offline(&mut self) -> Result<(), CoreError> {
        if matches!(
            self.state,
            RouteState::Stopped | RouteState::Failed | RouteState::Offline
        ) {
            return Err(CoreError::InvalidRouteTransition {
                from: self.state,
                action: "mark_offline",
            });
        }
        self.state = RouteState::Offline;
        Ok(())
    }

    pub fn recover(&mut self, now_ms: u64) -> Result<(), CoreError> {
        require_state(self.state, &[RouteState::Offline], "recover")?;
        ensure_authorized(self.authorization, now_ms)?;
        self.state = RouteState::Prepared;
        Ok(())
    }
}

fn validate_ref(reference: PortRef, port: &PortDescriptor) -> Result<(), CoreError> {
    if reference.capability_id != port.capability_id || reference.port_id != port.id {
        return Err(CoreError::InvalidPortRef(reference));
    }
    Ok(())
}

fn require_state(
    actual: RouteState,
    allowed: &[RouteState],
    action: &'static str,
) -> Result<(), CoreError> {
    if allowed.contains(&actual) {
        Ok(())
    } else {
        Err(CoreError::InvalidRouteTransition {
            from: actual,
            action,
        })
    }
}

fn ensure_authorized(state: AuthorizationState, now_ms: u64) -> Result<(), CoreError> {
    match state {
        AuthorizationState::Authorized {
            expires_at_ms: Some(expiry),
        } if expiry <= now_ms => Err(CoreError::AuthorizationExpired),
        AuthorizationState::Authorized { .. } => Ok(()),
        _ => Err(CoreError::RouteNotAuthorized),
    }
}
