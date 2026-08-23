use std::collections::{BTreeMap, VecDeque};

use capyio_core::{
    AdapterHealth, AdapterInstanceDescriptor, AdapterInstanceId, AdapterState, AuthorizationState,
    CapabilityDescriptor, CoreError, NodeDescriptor, NodeId, OnlineState, PortDescriptor, PortRef,
    Problem, ProblemCategory, ProblemId, ProblemSeverity, Route, RouteBackend, RouteEndpoint,
    RouteId, RouteState, Session, SessionId, SessionState,
};

use crate::{
    HostOperation, HostOperationCompletion, OperationId, OperationRecord, OperationRegistry,
    OperationStatus, OperationUpdate, RuntimeError, RuntimeEvent, RuntimeEventKind,
    RuntimeSnapshot,
};

const MAX_RETAINED_EVENTS: usize = 256;
const MAX_RETAINED_PROBLEMS: usize = 128;

#[derive(Clone, Debug)]
pub struct NodeRuntime {
    local_node: NodeDescriptor,
    peers: BTreeMap<NodeId, NodeDescriptor>,
    sessions: BTreeMap<SessionId, Session>,
    routes: BTreeMap<RouteId, Route>,
    operations: OperationRegistry,
    problems: VecDeque<Problem>,
    events: VecDeque<RuntimeEvent>,
    next_event_sequence: u64,
}

impl NodeRuntime {
    pub fn new(mut local_node: NodeDescriptor) -> Result<Self, RuntimeError> {
        local_node.online_state = OnlineState::Online;
        local_node.validate()?;
        Ok(Self {
            local_node,
            peers: BTreeMap::new(),
            sessions: BTreeMap::new(),
            routes: BTreeMap::new(),
            operations: OperationRegistry::default(),
            problems: VecDeque::new(),
            events: VecDeque::new(),
            next_event_sequence: 1,
        })
    }

    pub fn register_peer(
        &mut self,
        mut descriptor: NodeDescriptor,
        online: bool,
    ) -> Result<(), RuntimeError> {
        descriptor.online_state = if online {
            OnlineState::Online
        } else {
            OnlineState::Offline
        };
        descriptor.validate()?;
        let peer_id = descriptor.id;
        self.peers.insert(peer_id, descriptor);
        self.emit(RuntimeEventKind::PeerRegistered { peer_id });
        Ok(())
    }

    pub fn open_session(&mut self, peer_id: NodeId) -> Result<SessionId, RuntimeError> {
        self.open_session_with_id(SessionId::new(), peer_id)
    }

    pub fn open_session_with_id(
        &mut self,
        session_id: SessionId,
        peer_id: NodeId,
    ) -> Result<SessionId, RuntimeError> {
        let peer = self
            .peers
            .get(&peer_id)
            .ok_or(RuntimeError::UnknownPeer(peer_id))?;
        if peer.online_state != OnlineState::Online {
            return Err(RuntimeError::PeerOffline(peer_id));
        }
        if self.sessions.contains_key(&session_id) {
            return Err(RuntimeError::DuplicateSession(session_id));
        }
        let session = Session::with_id(session_id, self.local_node.id, peer_id);
        self.sessions.insert(session_id, session);
        self.emit(RuntimeEventKind::SessionOpened {
            session_id,
            peer_id,
        });
        Ok(session_id)
    }

    pub fn create_route(
        &mut self,
        session_id: SessionId,
        source: PortRef,
        sink: PortRef,
        backend: RouteBackend,
    ) -> Result<RouteId, RuntimeError> {
        self.create_route_with_id(RouteId::new(), session_id, source, sink, backend)
    }

    pub fn create_route_with_id(
        &mut self,
        route_id: RouteId,
        session_id: SessionId,
        source: PortRef,
        sink: PortRef,
        backend: RouteBackend,
    ) -> Result<RouteId, RuntimeError> {
        if self.routes.contains_key(&route_id) {
            return Err(RuntimeError::DuplicateRoute(route_id));
        }
        let session = self
            .sessions
            .get(&session_id)
            .ok_or(RuntimeError::UnknownSession(session_id))?;
        if session.state != SessionState::Ready {
            return Err(capyio_core::CoreError::InvalidSessionTransition {
                from: session.state,
                action: "create_route",
            }
            .into());
        }
        let valid_nodes = [session.local_node_id, session.remote_node_id];
        if !valid_nodes.contains(&source.node_id) || !valid_nodes.contains(&sink.node_id) {
            return Err(RuntimeError::UnknownPeer(
                if !valid_nodes.contains(&source.node_id) {
                    source.node_id
                } else {
                    sink.node_id
                },
            ));
        }
        let (source_port, source_adapter) = self.resolve_route_endpoint(source)?;
        let (sink_port, sink_adapter) = self.resolve_route_endpoint(sink)?;
        let route = Route::new(
            route_id,
            session_id,
            RouteEndpoint::new(source, &source_port, &source_adapter),
            RouteEndpoint::new(sink, &sink_port, &sink_adapter),
            backend,
        )?;
        self.routes.insert(route_id, route);
        self.emit(RuntimeEventKind::RouteChanged {
            route_id,
            state: RouteState::Draft,
        });
        Ok(route_id)
    }

    /// Deterministic demo helper using synthetic authorization and immediate host completion.
    pub fn set_route_active(
        &mut self,
        route_id: RouteId,
        active: bool,
        now_ms: u64,
    ) -> Result<(), RuntimeError> {
        if active {
            self.reconcile_route_before_activation(route_id)?;
        }
        let route = self
            .routes
            .get_mut(&route_id)
            .ok_or(RuntimeError::UnknownRoute(route_id))?;
        if active {
            match route.state {
                RouteState::Draft => {
                    if !matches!(route.authorization, AuthorizationState::Authorized { .. }) {
                        route.authorize(None)?;
                    }
                    let format = route.compatible_formats.first().cloned();
                    let qos = route
                        .compatible_qos_modes
                        .first()
                        .cloned()
                        .expect("Route compatibility requires QoS");
                    route.prepare(format, qos, now_ms)?;
                    route.begin_start(now_ms)?;
                    route.mark_active()?;
                }
                RouteState::Stopped => {
                    let format = route.compatible_formats.first().cloned();
                    let qos = route
                        .compatible_qos_modes
                        .first()
                        .cloned()
                        .expect("Route compatibility requires QoS");
                    route.prepare(format, qos, now_ms)?;
                    route.begin_start(now_ms)?;
                    route.mark_active()?;
                }
                RouteState::Prepared => {
                    route.begin_start(now_ms)?;
                    route.mark_active()?;
                }
                RouteState::Starting => route.mark_active()?,
                RouteState::Offline => {
                    route.recover(now_ms)?;
                    route.begin_start(now_ms)?;
                    route.mark_active()?;
                }
                RouteState::Active => return Ok(()),
                RouteState::Stopping | RouteState::Failed => {
                    return Err(capyio_core::CoreError::InvalidRouteTransition {
                        from: route.state,
                        action: "set_route_active",
                    }
                    .into());
                }
            }
        } else {
            match route.state {
                RouteState::Prepared
                | RouteState::Starting
                | RouteState::Active
                | RouteState::Offline => {
                    route.begin_stop()?;
                    route.mark_stopped()?;
                }
                RouteState::Draft | RouteState::Stopped | RouteState::Failed => return Ok(()),
                RouteState::Stopping => route.mark_stopped()?,
            }
        }
        let state = route.state;
        self.emit(RuntimeEventKind::RouteChanged { route_id, state });
        Ok(())
    }

    pub fn set_peer_online(&mut self, peer_id: NodeId, online: bool) -> Result<(), RuntimeError> {
        let peer = self
            .peers
            .get_mut(&peer_id)
            .ok_or(RuntimeError::UnknownPeer(peer_id))?;
        let desired = if online {
            OnlineState::Online
        } else {
            OnlineState::Offline
        };
        if peer.online_state == desired {
            return Ok(());
        }
        peer.online_state = desired;

        let session_ids = self
            .sessions
            .values()
            .filter(|session| session.remote_node_id == peer_id)
            .map(|session| session.id)
            .collect::<Vec<_>>();
        for session_id in session_ids {
            let state = {
                let session = self
                    .sessions
                    .get_mut(&session_id)
                    .expect("collected Session");
                if online {
                    if session.state == SessionState::Suspended {
                        session.mark_remote_online()?;
                    }
                } else if session.state == SessionState::Ready {
                    session.mark_remote_offline()?;
                }
                session.state
            };
            self.emit(RuntimeEventKind::SessionStateChanged { session_id, state });
        }

        if !online {
            let route_ids = self
                .routes
                .values()
                .filter(|route| {
                    (route.source.node_id == peer_id || route.sink.node_id == peer_id)
                        && !matches!(
                            route.state,
                            RouteState::Stopped | RouteState::Failed | RouteState::Offline
                        )
                })
                .map(|route| route.id)
                .collect::<Vec<_>>();
            for route_id in route_ids {
                let route = self.routes.get_mut(&route_id).expect("collected Route");
                route.mark_offline()?;
                self.emit(RuntimeEventKind::RouteChanged {
                    route_id,
                    state: RouteState::Offline,
                });
            }
        }
        self.emit(RuntimeEventKind::PeerOnlineChanged { peer_id, online });
        Ok(())
    }

    pub fn replace_adapter_catalog(
        &mut self,
        node_id: NodeId,
        adapter_id: AdapterInstanceId,
        capabilities: Vec<CapabilityDescriptor>,
    ) -> Result<(), RuntimeError> {
        self.node_mut(node_id)?
            .replace_adapter_catalog(adapter_id, capabilities)?;

        // Re-evaluate every Route because a changed endpoint can be paired with
        // either a local or peer endpoint. Compatible changes only refresh the
        // negotiated candidate sets; they never restart an Offline Route.
        let route_ids = self.routes.keys().copied().collect::<Vec<_>>();
        for route_id in route_ids {
            self.reconcile_route_after_catalog_change(route_id, node_id, adapter_id)?;
        }
        self.emit(RuntimeEventKind::CatalogChanged {
            node_id,
            adapter_id,
        });
        Ok(())
    }

    pub fn fail_adapter(
        &mut self,
        node_id: NodeId,
        adapter_id: AdapterInstanceId,
        code: impl Into<String>,
    ) -> Result<(), RuntimeError> {
        let code = code.into();
        let owned = {
            let node = self.node_mut(node_id)?;
            let adapter = node.adapter_instances.get_mut(&adapter_id).ok_or(
                RuntimeError::UnknownAdapter {
                    node_id,
                    adapter_id,
                },
            )?;
            adapter.state = AdapterState::Failed;
            adapter.health = AdapterHealth::Unhealthy;
            adapter.owned_capabilities.clone()
        };
        self.emit(RuntimeEventKind::AdapterChanged {
            node_id,
            adapter_id,
            state: AdapterState::Failed,
            health: AdapterHealth::Unhealthy,
        });

        let affected = self
            .routes
            .values()
            .filter(|route| {
                ((route.source.node_id == node_id && owned.contains(&route.source.capability_id))
                    || (route.sink.node_id == node_id && owned.contains(&route.sink.capability_id)))
                    && !matches!(route.state, RouteState::Stopped | RouteState::Failed)
            })
            .map(|route| route.id)
            .collect::<Vec<_>>();

        if affected.is_empty() {
            self.push_problem(adapter_problem(node_id, adapter_id, None, code))?;
        } else {
            for route_id in affected {
                let problem = adapter_problem(node_id, adapter_id, Some(route_id), code.clone());
                let problem_id = problem.id;
                self.push_problem(problem)?;
                let route = self.routes.get_mut(&route_id).expect("affected Route");
                route.mark_failed(problem_id)?;
                self.emit(RuntimeEventKind::RouteChanged {
                    route_id,
                    state: RouteState::Failed,
                });
            }
        }
        Ok(())
    }

    pub fn begin_host_operation(
        &mut self,
        operation: HostOperation,
    ) -> Result<OperationId, RuntimeError> {
        let id = self.operations.begin(operation)?;
        self.emit(RuntimeEventKind::OperationChanged {
            operation_id: id,
            status: OperationStatus::Pending,
        });
        Ok(id)
    }

    pub fn complete_host_operation(
        &mut self,
        id: OperationId,
        completion: HostOperationCompletion,
    ) -> Result<OperationUpdate, RuntimeError> {
        let update = self.operations.complete(id, completion)?;
        self.emit_operation_update(id, update);
        Ok(update)
    }

    pub fn cancel_host_operation(
        &mut self,
        id: OperationId,
    ) -> Result<OperationUpdate, RuntimeError> {
        let update = self.operations.cancel(id)?;
        self.emit_operation_update(id, update);
        Ok(update)
    }

    pub fn dispose_host_operation(
        &mut self,
        id: OperationId,
    ) -> Result<OperationUpdate, RuntimeError> {
        let update = self.operations.dispose(id)?;
        self.emit_operation_update(id, update);
        Ok(update)
    }

    pub fn host_operation(&self, id: OperationId) -> Result<&OperationRecord, RuntimeError> {
        self.operations.record(id)
    }

    pub fn session(&self, id: SessionId) -> Result<&Session, RuntimeError> {
        self.sessions
            .get(&id)
            .ok_or(RuntimeError::UnknownSession(id))
    }

    pub fn route(&self, id: RouteId) -> Result<&Route, RuntimeError> {
        self.routes.get(&id).ok_or(RuntimeError::UnknownRoute(id))
    }

    #[must_use]
    pub fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            local_node: self.local_node.clone(),
            peers: self.peers.values().cloned().collect(),
            sessions: self.sessions.values().cloned().collect(),
            routes: self.routes.values().cloned().collect(),
            operations: self.operations.records().cloned().collect(),
            problems: self.problems.iter().cloned().collect(),
            events: self.events.iter().cloned().collect(),
        }
    }

    fn resolve_route_endpoint(
        &self,
        reference: PortRef,
    ) -> Result<(PortDescriptor, AdapterInstanceDescriptor), RuntimeError> {
        let node = self.node(reference.node_id)?;
        let capability = node.capabilities.get(&reference.capability_id).ok_or(
            RuntimeError::PortNotAdvertised {
                node_id: reference.node_id,
                port_id: reference.port_id,
            },
        )?;
        let port =
            capability
                .ports
                .get(&reference.port_id)
                .ok_or(RuntimeError::PortNotAdvertised {
                    node_id: reference.node_id,
                    port_id: reference.port_id,
                })?;
        let adapter = node
            .adapter_instances
            .get(&capability.adapter_instance_id)
            .ok_or(RuntimeError::UnknownAdapter {
                node_id: reference.node_id,
                adapter_id: capability.adapter_instance_id,
            })?;
        Ok((port.clone(), adapter.clone()))
    }

    fn reconcile_route_after_catalog_change(
        &mut self,
        route_id: RouteId,
        changed_node_id: NodeId,
        changed_adapter_id: AdapterInstanceId,
    ) -> Result<(), RuntimeError> {
        let (source, sink) = {
            let route = self
                .routes
                .get(&route_id)
                .expect("Route ID collected from map");
            (route.source, route.sink)
        };
        let endpoints = self
            .resolve_route_endpoint(source)
            .map_err(|_| CatalogRouteIssue::EndpointMissing(source))
            .and_then(|(source_port, source_adapter)| {
                self.resolve_route_endpoint(sink)
                    .map_err(|_| CatalogRouteIssue::EndpointMissing(sink))
                    .map(|(sink_port, sink_adapter)| {
                        (source_port, source_adapter, sink_port, sink_adapter)
                    })
            });

        let issue = match endpoints {
            Ok((source_port, source_adapter, sink_port, sink_adapter)) => self
                .routes
                .get_mut(&route_id)
                .expect("Route ID collected from map")
                .reconcile_endpoints(&source_port, &source_adapter, &sink_port, &sink_adapter)
                .err()
                .map(CatalogRouteIssue::Incompatible),
            Err(issue) => Some(issue),
        };

        if let Some(issue) = issue {
            self.invalidate_route_for_catalog(
                route_id,
                changed_node_id,
                changed_adapter_id,
                issue,
            )?;
        }
        Ok(())
    }

    fn reconcile_route_before_activation(&mut self, route_id: RouteId) -> Result<(), RuntimeError> {
        let (source, sink) = {
            let route = self
                .routes
                .get(&route_id)
                .ok_or(RuntimeError::UnknownRoute(route_id))?;
            (route.source, route.sink)
        };
        let (source_port, source_adapter) = self.resolve_route_endpoint(source)?;
        let (sink_port, sink_adapter) = self.resolve_route_endpoint(sink)?;
        self.routes
            .get_mut(&route_id)
            .expect("Route existence checked above")
            .reconcile_endpoints(&source_port, &source_adapter, &sink_port, &sink_adapter)?;
        Ok(())
    }

    fn invalidate_route_for_catalog(
        &mut self,
        route_id: RouteId,
        node_id: NodeId,
        adapter_id: AdapterInstanceId,
        issue: CatalogRouteIssue,
    ) -> Result<(), RuntimeError> {
        let state = self
            .routes
            .get(&route_id)
            .expect("Route ID collected from map")
            .state;
        if matches!(state, RouteState::Offline | RouteState::Failed) {
            return Ok(());
        }

        let problem = catalog_problem(node_id, adapter_id, route_id, &issue);
        problem.validate()?;
        let problem_id = problem.id;
        self.routes
            .get_mut(&route_id)
            .expect("Route ID collected from map")
            .mark_offline_with_problem(problem_id)?;
        self.push_problem(problem)?;
        self.emit(RuntimeEventKind::RouteChanged {
            route_id,
            state: RouteState::Offline,
        });
        Ok(())
    }

    fn node(&self, id: NodeId) -> Result<&NodeDescriptor, RuntimeError> {
        if self.local_node.id == id {
            Ok(&self.local_node)
        } else {
            self.peers.get(&id).ok_or(RuntimeError::UnknownPeer(id))
        }
    }

    fn node_mut(&mut self, id: NodeId) -> Result<&mut NodeDescriptor, RuntimeError> {
        if self.local_node.id == id {
            Ok(&mut self.local_node)
        } else {
            self.peers.get_mut(&id).ok_or(RuntimeError::UnknownPeer(id))
        }
    }

    fn push_problem(&mut self, problem: Problem) -> Result<(), RuntimeError> {
        problem.validate()?;
        let id = problem.id;
        self.problems.push_back(problem);
        while self.problems.len() > MAX_RETAINED_PROBLEMS {
            self.problems.pop_front();
        }
        self.emit(RuntimeEventKind::ProblemReported { problem_id: id });
        Ok(())
    }

    fn emit_operation_update(&mut self, id: OperationId, update: OperationUpdate) {
        if let OperationUpdate::Applied(status) = update {
            self.emit(RuntimeEventKind::OperationChanged {
                operation_id: id,
                status,
            });
        }
    }

    fn emit(&mut self, kind: RuntimeEventKind) {
        self.events.push_back(RuntimeEvent {
            sequence: self.next_event_sequence,
            kind,
        });
        self.next_event_sequence = self.next_event_sequence.saturating_add(1);
        while self.events.len() > MAX_RETAINED_EVENTS {
            self.events.pop_front();
        }
    }
}

fn adapter_problem(
    node_id: NodeId,
    adapter_id: AdapterInstanceId,
    route_id: Option<RouteId>,
    code: String,
) -> Problem {
    Problem {
        id: ProblemId::new(),
        code,
        category: ProblemCategory::Adapter,
        severity: ProblemSeverity::Error,
        retryable: true,
        related_node: Some(node_id),
        related_adapter: Some(adapter_id),
        related_route: route_id,
        human_message: "Adapter stopped unexpectedly".to_owned(),
        technical_detail: None,
    }
}

#[derive(Debug)]
enum CatalogRouteIssue {
    EndpointMissing(PortRef),
    Incompatible(CoreError),
}

impl CatalogRouteIssue {
    const fn code(&self) -> &'static str {
        match self {
            Self::EndpointMissing(_) => "CAPY.ROUTE.ENDPOINT_REMOVED",
            Self::Incompatible(CoreError::InvalidRouteEndpoint { .. }) => {
                "CAPY.ROUTE.DIRECTION_CHANGED"
            }
            Self::Incompatible(
                CoreError::IncompatibleProfiles { .. } | CoreError::RouteProfileChanged { .. },
            ) => "CAPY.ROUTE.PROFILE_CHANGED",
            Self::Incompatible(
                CoreError::NoCompatibleFormat | CoreError::UnsupportedRouteFormat,
            ) => "CAPY.ROUTE.FORMAT_UNAVAILABLE",
            Self::Incompatible(CoreError::NoCompatibleQos | CoreError::UnsupportedRouteQos) => {
                "CAPY.ROUTE.QOS_UNAVAILABLE"
            }
            Self::Incompatible(
                CoreError::IncompatibleInteroperabilityModes
                | CoreError::BackendInteroperabilityMismatch { .. },
            ) => "CAPY.ROUTE.INTEROPERABILITY_CHANGED",
            Self::Incompatible(
                CoreError::UnsupportedRouteBackend { .. }
                | CoreError::LocalPipelineRequiresSameNode { .. },
            ) => "CAPY.ROUTE.BACKEND_UNSUPPORTED",
            Self::Incompatible(_) => "CAPY.ROUTE.ENDPOINT_INCOMPATIBLE",
        }
    }

    fn detail(&self) -> String {
        match self {
            Self::EndpointMissing(reference) => format!(
                "Port {} on Capability {} is absent from Node {}",
                reference.port_id, reference.capability_id, reference.node_id
            ),
            Self::Incompatible(error) => error.to_string(),
        }
    }
}

fn catalog_problem(
    node_id: NodeId,
    adapter_id: AdapterInstanceId,
    route_id: RouteId,
    issue: &CatalogRouteIssue,
) -> Problem {
    Problem {
        id: ProblemId::new(),
        code: issue.code().to_owned(),
        category: ProblemCategory::Route,
        severity: ProblemSeverity::Error,
        retryable: true,
        related_node: Some(node_id),
        related_adapter: Some(adapter_id),
        related_route: Some(route_id),
        human_message: "A Route endpoint is no longer compatible with its catalog".to_owned(),
        technical_detail: Some(issue.detail()),
    }
}
