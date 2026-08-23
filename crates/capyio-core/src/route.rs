use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    AdapterInstanceDescriptor, CapabilityId, CoreError, FormatDescriptor, InteroperabilityMode,
    NodeId, PortDescriptor, PortDirection, PortId, ProblemId, ProfileId, QosMode, RouteId,
    SessionId,
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

/// Borrowed catalog context required to validate one Route endpoint.
#[derive(Clone, Copy, Debug)]
pub struct RouteEndpoint<'a> {
    pub reference: PortRef,
    pub port: &'a PortDescriptor,
    pub adapter: &'a AdapterInstanceDescriptor,
}

impl<'a> RouteEndpoint<'a> {
    #[must_use]
    pub const fn new(
        reference: PortRef,
        port: &'a PortDescriptor,
        adapter: &'a AdapterInstanceDescriptor,
    ) -> Self {
        Self {
            reference,
            port,
            adapter,
        }
    }
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
        source: RouteEndpoint<'_>,
        sink: RouteEndpoint<'_>,
        backend: RouteBackend,
    ) -> Result<Self, CoreError> {
        let contract = endpoint_contract(source, sink, backend)?;

        Ok(Self {
            id,
            session_id,
            source: source.reference,
            sink: sink.reference,
            profile: contract.profile,
            backend,
            compatible_formats: contract.compatible_formats,
            compatible_qos_modes: contract.compatible_qos_modes,
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
        if matches!(self.state, RouteState::Failed | RouteState::Offline) {
            return Err(CoreError::InvalidRouteTransition {
                from: self.state,
                action: "mark_offline",
            });
        }
        self.epoch = self.epoch.saturating_add(1);
        self.state = RouteState::Offline;
        Ok(())
    }

    /// Invalidates the current data epoch and retains the structured reason.
    pub fn mark_offline_with_problem(&mut self, problem_id: ProblemId) -> Result<(), CoreError> {
        self.mark_offline()?;
        self.diagnostic_ids.push(problem_id);
        Ok(())
    }

    /// Revalidates a persisted Route against the current endpoint catalogs.
    ///
    /// On success only the mutually advertised format/QoS sets are refreshed;
    /// lifecycle state, authorization, selections and epoch are preserved.
    pub fn reconcile_endpoints(
        &mut self,
        source_port: &PortDescriptor,
        source_adapter: &AdapterInstanceDescriptor,
        sink_port: &PortDescriptor,
        sink_adapter: &AdapterInstanceDescriptor,
    ) -> Result<(), CoreError> {
        let contract = endpoint_contract(
            RouteEndpoint::new(self.source, source_port, source_adapter),
            RouteEndpoint::new(self.sink, sink_port, sink_adapter),
            self.backend,
        )?;
        if contract.profile != self.profile {
            return Err(CoreError::RouteProfileChanged {
                route_profile: self.profile.clone(),
                endpoint_profile: contract.profile,
            });
        }
        if self
            .selected_format
            .as_ref()
            .is_some_and(|selected| !contract.compatible_formats.contains(selected))
        {
            return Err(CoreError::UnsupportedRouteFormat);
        }
        if self
            .selected_qos_mode
            .as_ref()
            .is_some_and(|selected| !contract.compatible_qos_modes.contains(selected))
        {
            return Err(CoreError::UnsupportedRouteQos);
        }
        self.compatible_formats = contract.compatible_formats;
        self.compatible_qos_modes = contract.compatible_qos_modes;
        Ok(())
    }

    pub fn recover(&mut self, now_ms: u64) -> Result<(), CoreError> {
        require_state(self.state, &[RouteState::Offline], "recover")?;
        ensure_authorized(self.authorization, now_ms)?;
        self.state = RouteState::Prepared;
        Ok(())
    }
}

#[derive(Debug)]
struct EndpointContract {
    profile: ProfileId,
    compatible_formats: Vec<FormatDescriptor>,
    compatible_qos_modes: BTreeSet<QosMode>,
}

fn endpoint_contract(
    source: RouteEndpoint<'_>,
    sink: RouteEndpoint<'_>,
    backend: RouteBackend,
) -> Result<EndpointContract, CoreError> {
    validate_ref(source.reference, source.port)?;
    validate_ref(sink.reference, sink.port)?;
    validate_adapter_ownership(source.adapter, source.reference)?;
    validate_adapter_ownership(sink.adapter, sink.reference)?;
    if source.port.direction != PortDirection::Source {
        return Err(CoreError::InvalidRouteEndpoint {
            port_id: source.port.id,
            expected: PortDirection::Source,
            actual: source.port.direction,
        });
    }
    if sink.port.direction != PortDirection::Sink {
        return Err(CoreError::InvalidRouteEndpoint {
            port_id: sink.port.id,
            expected: PortDirection::Sink,
            actual: sink.port.direction,
        });
    }
    if source.port.profile != sink.port.profile {
        return Err(CoreError::IncompatibleProfiles {
            source_profile: source.port.profile.clone(),
            sink_profile: sink.port.profile.clone(),
        });
    }
    if source.port.interoperability_mode != sink.port.interoperability_mode {
        return Err(CoreError::IncompatibleInteroperabilityModes);
    }
    validate_backend_interoperability(backend, source.port.interoperability_mode)?;
    validate_adapter_backend(source.adapter, backend)?;
    validate_adapter_backend(sink.adapter, backend)?;
    if backend == RouteBackend::LocalPipeline && source.reference.node_id != sink.reference.node_id
    {
        return Err(CoreError::LocalPipelineRequiresSameNode {
            source_node: source.reference.node_id,
            sink_node: sink.reference.node_id,
        });
    }

    let compatible_formats = source
        .port
        .formats
        .iter()
        .filter(|format| sink.port.formats.contains(format))
        .cloned()
        .collect::<Vec<_>>();
    if !source.port.formats.is_empty()
        && !sink.port.formats.is_empty()
        && compatible_formats.is_empty()
    {
        return Err(CoreError::NoCompatibleFormat);
    }
    let compatible_qos_modes = source
        .port
        .qos_modes
        .intersection(&sink.port.qos_modes)
        .cloned()
        .collect::<BTreeSet<_>>();
    if compatible_qos_modes.is_empty() {
        return Err(CoreError::NoCompatibleQos);
    }

    Ok(EndpointContract {
        profile: source.port.profile.clone(),
        compatible_formats,
        compatible_qos_modes,
    })
}

fn validate_adapter_ownership(
    adapter: &AdapterInstanceDescriptor,
    endpoint: PortRef,
) -> Result<(), CoreError> {
    if adapter.owned_capabilities.contains(&endpoint.capability_id) {
        Ok(())
    } else {
        Err(CoreError::AdapterDoesNotOwnRouteEndpoint {
            adapter_id: adapter.id,
            capability_id: endpoint.capability_id,
        })
    }
}

fn validate_adapter_backend(
    adapter: &AdapterInstanceDescriptor,
    backend: RouteBackend,
) -> Result<(), CoreError> {
    if adapter.supported_route_modes.contains(&backend) {
        Ok(())
    } else {
        Err(CoreError::UnsupportedRouteBackend {
            adapter_id: adapter.id,
            backend,
        })
    }
}

fn validate_backend_interoperability(
    backend: RouteBackend,
    mode: InteroperabilityMode,
) -> Result<(), CoreError> {
    let supported = match backend {
        RouteBackend::CapyDataPlane | RouteBackend::LocalPipeline => {
            mode == InteroperabilityMode::StandardPort
        }
        RouteBackend::AdapterManaged => mode == InteroperabilityMode::AdapterManaged,
        // An explicit external bridge may either expose a StandardPort boundary
        // or retain an Adapter-managed contract. Both endpoint modes must still
        // match and both Adapters must advertise ExternalProtocol support.
        RouteBackend::ExternalProtocol => true,
    };
    if supported {
        Ok(())
    } else {
        Err(CoreError::BackendInteroperabilityMismatch { backend, mode })
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
