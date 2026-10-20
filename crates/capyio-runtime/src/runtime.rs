use std::collections::{BTreeMap, VecDeque};

use capyio_core::{
    AdapterHealth, AdapterInstanceDescriptor, AdapterInstanceId, AdapterState, AuthorizationState,
    CapabilityDescriptor, CoreError, NodeDescriptor, NodeId, OnlineState, PortDescriptor, PortRef,
    Problem, ProblemCategory, ProblemId, ProblemSeverity, QosMode, Route, RouteBackend,
    RouteEndpoint, RouteId, RouteState, Session, SessionId, SessionState,
};

use crate::{
    HostOperation, HostOperationCompletion, LeaseClaimOutcome, OperationId, OperationRecord,
    OperationRegistry, OperationStatus, OperationUpdate, PortLeaseClaim, PortLeasePolicy,
    PortLeaseRegistry, RuntimeError, RuntimeEvent, RuntimeEventKind, RuntimeSnapshot,
};

const MAX_RETAINED_EVENTS: usize = 256;
const MAX_RETAINED_PROBLEMS: usize = 128;

#[derive(Clone, Debug)]
pub struct NodeRuntime {
    local_node: NodeDescriptor,
    peers: BTreeMap<NodeId, NodeDescriptor>,
    sessions: BTreeMap<SessionId, Session>,
    routes: BTreeMap<RouteId, Route>,
    port_leases: PortLeaseRegistry,
    operations: OperationRegistry,
    problems: VecDeque<Problem>,
    events: VecDeque<RuntimeEvent>,
    next_event_sequence: u64,
}

impl NodeRuntime {
    pub fn new(mut local_node: NodeDescriptor) -> Result<Self, RuntimeError> {
        local_node.online_state = OnlineState::Online;
        local_node.validate()?;
        let local_node_id = local_node.id;
        Ok(Self {
            local_node,
            peers: BTreeMap::new(),
            sessions: BTreeMap::new(),
            routes: BTreeMap::new(),
            port_leases: PortLeaseRegistry::new(local_node_id),
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

    /// Atomically installs one Adapter and its initial Capability catalog on a
    /// Node already owned by this Runtime.
    pub fn register_adapter_catalog(
        &mut self,
        node_id: NodeId,
        adapter: AdapterInstanceDescriptor,
        capabilities: Vec<CapabilityDescriptor>,
    ) -> Result<(), RuntimeError> {
        let adapter_id = adapter.id;
        let adapter_state = adapter.state;
        let adapter_health = adapter.health;
        let mut candidate = self.node(node_id)?.clone();
        candidate.add_adapter(adapter)?;
        candidate.replace_adapter_catalog(adapter_id, capabilities)?;
        candidate.validate()?;
        *self.node_mut(node_id)? = candidate;
        self.emit(RuntimeEventKind::AdapterChanged {
            node_id,
            adapter_id,
            state: adapter_state,
            health: adapter_health,
        });
        self.emit(RuntimeEventKind::CatalogChanged {
            node_id,
            adapter_id,
        });
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

    /// Atomically closes one control Session, invalidates its live Routes and
    /// releases every lease they own. Repeated close of an already-closed
    /// Session is idempotent.
    pub fn close_session(&mut self, session_id: SessionId) -> Result<(), RuntimeError> {
        let mut candidate = self.clone();
        candidate.close_session_inner(session_id)?;
        *self = candidate;
        Ok(())
    }

    /// Atomically accepts a complete peer catalog through one already-ready
    /// Session. Transport authentication and envelope decoding remain caller
    /// responsibilities; this boundary prevents a valid catalog from being
    /// applied to a different peer or a suspended Session.
    pub fn accept_peer_catalog(
        &mut self,
        session_id: SessionId,
        descriptor: NodeDescriptor,
    ) -> Result<(), RuntimeError> {
        let mut candidate = self.clone();
        candidate.accept_peer_catalog_inner(session_id, descriptor)?;
        *self = candidate;
        Ok(())
    }

    /// Atomically replaces the host-owned local inventory without discarding
    /// peers, Sessions or Routes. Only incompatible dependent Routes go Offline.
    /// An identical replacement is idempotent and emits no duplicate event.
    pub fn replace_local_catalog(
        &mut self,
        mut descriptor: NodeDescriptor,
    ) -> Result<(), RuntimeError> {
        descriptor.validate()?;
        if descriptor.id != self.local_node.id {
            return Err(RuntimeError::LocalCatalogNodeMismatch);
        }
        descriptor.online_state = self.local_node.online_state;
        if descriptor == self.local_node {
            return Ok(());
        }
        let mut candidate = self.clone();
        let previous = std::mem::replace(&mut candidate.local_node, descriptor);
        candidate.reconcile_node_catalog_change(previous.id, &previous)?;
        candidate.emit(RuntimeEventKind::LocalCatalogReplaced {
            node_id: previous.id,
        });
        *self = candidate;
        Ok(())
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
        if !active {
            self.release_route_leases(route_id);
        }
        Ok(())
    }

    pub fn authorize_route(
        &mut self,
        route_id: RouteId,
        expires_at_ms: Option<u64>,
    ) -> Result<(), RuntimeError> {
        let state = {
            let route = self
                .routes
                .get_mut(&route_id)
                .ok_or(RuntimeError::UnknownRoute(route_id))?;
            route.authorize(expires_at_ms)?;
            route.state
        };
        self.emit(RuntimeEventKind::RouteChanged { route_id, state });
        Ok(())
    }

    /// Local consent owner restricts an already authorized finite Draft grant.
    /// No retained lease may exist for this Route, even if expired. This never
    /// creates authority from peer input; caller policy must check the exact
    /// authenticated binding and absence of a prepared external worker.
    /// An unchanged deadline is idempotent and emits no event.
    pub fn restrict_draft_authorization(
        &mut self,
        route_id: RouteId,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<(), RuntimeError> {
        let route = self
            .routes
            .get_mut(&route_id)
            .ok_or(RuntimeError::UnknownRoute(route_id))?;
        if self
            .port_leases
            .claims()
            .any(|claim| claim.route_id == route_id)
        {
            return Err(RuntimeError::DraftAuthorizationLeasePresent(route_id));
        }
        let before = route.authorization;
        route.restrict_draft_authorization(expires_at_ms, now_ms)?;
        if route.authorization != before {
            self.emit(RuntimeEventKind::RouteChanged {
                route_id,
                state: RouteState::Draft,
            });
        }
        Ok(())
    }

    /// Local consent owner renews an ongoing finite authorization. This does
    /// not renew a port lease or authorize another Route, and keeps the epoch.
    pub fn renew_route_authorization(
        &mut self,
        route_id: RouteId,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<(), RuntimeError> {
        let route = self
            .routes
            .get_mut(&route_id)
            .ok_or(RuntimeError::UnknownRoute(route_id))?;
        route.renew_authorization(expires_at_ms, now_ms)?;
        let state = route.state;
        self.emit(RuntimeEventKind::RouteChanged { route_id, state });
        Ok(())
    }

    /// Reject a still-pending request. Repeated rejection is idempotent; an
    /// active/previously authorized Route requires a separate revocation path.
    pub fn deny_pending_route(&mut self, route_id: RouteId) -> Result<(), RuntimeError> {
        let route = self
            .routes
            .get_mut(&route_id)
            .ok_or(RuntimeError::UnknownRoute(route_id))?;
        if route.authorization == AuthorizationState::Denied {
            return Ok(());
        }
        if route.authorization != AuthorizationState::Pending || route.state != RouteState::Draft {
            return Err(CoreError::InvalidAuthorizationTransition.into());
        }
        route.authorization = AuthorizationState::Denied;
        self.emit(RuntimeEventKind::RouteChanged {
            route_id,
            state: RouteState::Draft,
        });
        Ok(())
    }

    pub fn set_local_port_lease_policy(
        &mut self,
        port: PortRef,
        policy: PortLeasePolicy,
    ) -> Result<(), RuntimeError> {
        if port.node_id != self.local_node.id {
            return Err(crate::PortLeaseError::NotLocalPort(port).into());
        }
        self.resolve_route_endpoint(port)?;
        self.port_leases.set_policy(port, policy)?;
        Ok(())
    }

    pub fn acquire_route_port_lease(
        &mut self,
        route_id: RouteId,
        port: PortRef,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<LeaseClaimOutcome, RuntimeError> {
        let route = self
            .routes
            .get(&route_id)
            .ok_or(RuntimeError::UnknownRoute(route_id))?;
        if port.node_id != self.local_node.id || (route.source != port && route.sink != port) {
            return Err(RuntimeError::LeasePortNotOnRoute { route_id, port });
        }
        let session = self
            .sessions
            .get(&route.session_id)
            .ok_or(RuntimeError::UnknownSession(route.session_id))?;
        if session.state != SessionState::Ready {
            return Err(RuntimeError::LeaseAuthorizationRequired(route_id));
        }
        match route.authorization {
            AuthorizationState::Authorized {
                expires_at_ms: Some(authorization_expiry),
            } => {
                if authorization_expiry <= now_ms {
                    return Err(RuntimeError::LeaseAuthorizationRequired(route_id));
                }
                if expires_at_ms > authorization_expiry {
                    return Err(RuntimeError::LeaseExceedsAuthorization { route_id });
                }
            }
            AuthorizationState::Authorized {
                expires_at_ms: None,
            } => {}
            _ => return Err(RuntimeError::LeaseAuthorizationRequired(route_id)),
        }
        let outcome = self.port_leases.claim(
            PortLeaseClaim {
                port,
                route_id,
                session_id: route.session_id,
                expires_at_ms,
            },
            now_ms,
        )?;
        self.emit(RuntimeEventKind::PortLeaseChanged {
            port,
            route_id,
            active: true,
        });
        Ok(outcome)
    }

    #[must_use]
    pub fn route_port_lease_claim(
        &self,
        route_id: RouteId,
        port: PortRef,
    ) -> Option<&PortLeaseClaim> {
        self.port_leases.claim_for(port, route_id)
    }

    pub fn release_route_port_lease(
        &mut self,
        route_id: RouteId,
        port: PortRef,
        session_id: SessionId,
    ) -> Result<bool, RuntimeError> {
        let route = self
            .routes
            .get(&route_id)
            .ok_or(RuntimeError::UnknownRoute(route_id))?;
        if route.session_id != session_id {
            return Err(RuntimeError::LeaseSessionMismatch {
                route_id,
                expected: route.session_id,
                actual: session_id,
            });
        }
        if port.node_id != self.local_node.id || (route.source != port && route.sink != port) {
            return Err(RuntimeError::LeasePortNotOnRoute { route_id, port });
        }
        let released = self.port_leases.release(port, route_id, session_id)?;
        if released {
            self.emit(RuntimeEventKind::PortLeaseChanged {
                port,
                route_id,
                active: false,
            });
        }
        Ok(released)
    }

    pub fn expire_port_leases(&mut self, now_ms: u64) -> usize {
        let expired = self
            .port_leases
            .claims()
            .filter(|claim| claim.expires_at_ms <= now_ms)
            .copied()
            .collect::<Vec<_>>();
        let count = self.port_leases.expire(now_ms);
        for claim in expired {
            self.emit(RuntimeEventKind::PortLeaseChanged {
                port: claim.port,
                route_id: claim.route_id,
                active: false,
            });
            if let Some(route) = self.routes.get_mut(&claim.route_id)
                && !matches!(
                    route.state,
                    RouteState::Stopped | RouteState::Failed | RouteState::Offline
                )
            {
                route
                    .mark_offline()
                    .expect("eligible Route can become Offline");
                self.emit(RuntimeEventKind::RouteChanged {
                    route_id: claim.route_id,
                    state: RouteState::Offline,
                });
            }
        }
        count
    }

    pub fn prepare_route(
        &mut self,
        route_id: RouteId,
        selected_format: Option<capyio_core::FormatDescriptor>,
        selected_qos_mode: QosMode,
        now_ms: u64,
    ) -> Result<(), RuntimeError> {
        self.reconcile_route_before_activation(route_id)?;
        let route = self
            .routes
            .get_mut(&route_id)
            .ok_or(RuntimeError::UnknownRoute(route_id))?;
        route.prepare(selected_format, selected_qos_mode, now_ms)?;
        self.emit(RuntimeEventKind::RouteChanged {
            route_id,
            state: RouteState::Prepared,
        });
        Ok(())
    }

    pub fn begin_route_start(
        &mut self,
        route_id: RouteId,
        now_ms: u64,
    ) -> Result<(), RuntimeError> {
        self.reconcile_route_before_activation(route_id)?;
        self.require_route_leases(route_id, now_ms)?;
        let route = self
            .routes
            .get_mut(&route_id)
            .ok_or(RuntimeError::UnknownRoute(route_id))?;
        route.begin_start(now_ms)?;
        self.emit(RuntimeEventKind::RouteChanged {
            route_id,
            state: RouteState::Starting,
        });
        Ok(())
    }

    pub fn activate_route(&mut self, route_id: RouteId) -> Result<(), RuntimeError> {
        let route = self
            .routes
            .get_mut(&route_id)
            .ok_or(RuntimeError::UnknownRoute(route_id))?;
        route.mark_active()?;
        self.emit(RuntimeEventKind::RouteChanged {
            route_id,
            state: RouteState::Active,
        });
        Ok(())
    }

    pub fn begin_route_stop(&mut self, route_id: RouteId) -> Result<(), RuntimeError> {
        let route = self
            .routes
            .get_mut(&route_id)
            .ok_or(RuntimeError::UnknownRoute(route_id))?;
        route.begin_stop()?;
        self.emit(RuntimeEventKind::RouteChanged {
            route_id,
            state: RouteState::Stopping,
        });
        Ok(())
    }

    pub fn stop_route(&mut self, route_id: RouteId) -> Result<(), RuntimeError> {
        let route = self
            .routes
            .get_mut(&route_id)
            .ok_or(RuntimeError::UnknownRoute(route_id))?;
        route.mark_stopped()?;
        self.emit(RuntimeEventKind::RouteChanged {
            route_id,
            state: RouteState::Stopped,
        });
        self.release_route_leases(route_id);
        Ok(())
    }

    pub fn recover_route(&mut self, route_id: RouteId, now_ms: u64) -> Result<(), RuntimeError> {
        self.reconcile_route_before_activation(route_id)?;
        let route = self
            .routes
            .get_mut(&route_id)
            .ok_or(RuntimeError::UnknownRoute(route_id))?;
        route.recover(now_ms)?;
        self.emit(RuntimeEventKind::RouteChanged {
            route_id,
            state: RouteState::Prepared,
        });
        Ok(())
    }

    pub fn report_route_offline(
        &mut self,
        route_id: RouteId,
        problem: Problem,
    ) -> Result<(), RuntimeError> {
        problem.validate()?;
        if problem.related_route != Some(route_id) {
            return Err(CoreError::InvalidProblem(
                "Runtime Route Problem must reference the affected Route".to_owned(),
            )
            .into());
        }
        let problem_id = problem.id;
        self.routes
            .get_mut(&route_id)
            .ok_or(RuntimeError::UnknownRoute(route_id))?
            .mark_offline_with_problem(problem_id)?;
        self.push_problem(problem)?;
        self.emit(RuntimeEventKind::RouteChanged {
            route_id,
            state: RouteState::Offline,
        });
        self.release_route_leases(route_id);
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
                self.release_route_leases(route_id);
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
            port_leases: self.port_leases.claims().copied().collect(),
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
                Some(changed_adapter_id),
                issue,
            )?;
        }
        Ok(())
    }

    fn accept_peer_catalog_inner(
        &mut self,
        session_id: SessionId,
        mut descriptor: NodeDescriptor,
    ) -> Result<(), RuntimeError> {
        descriptor.validate()?;
        let session = self
            .sessions
            .get(&session_id)
            .ok_or(RuntimeError::UnknownSession(session_id))?;
        if session.state != SessionState::Ready {
            return Err(CoreError::InvalidSessionTransition {
                from: session.state,
                action: "accept_peer_catalog",
            }
            .into());
        }
        let peer_id = session.remote_node_id;
        if descriptor.id != peer_id {
            return Err(RuntimeError::PeerCatalogNodeMismatch {
                session_id,
                expected_peer_id: peer_id,
                catalog_node_id: descriptor.id,
            });
        }

        let previous = self
            .peers
            .get(&peer_id)
            .ok_or(RuntimeError::UnknownPeer(peer_id))?
            .clone();
        if previous.online_state != OnlineState::Online {
            return Err(RuntimeError::PeerOffline(peer_id));
        }

        // Peer-reported availability cannot override transport-owned presence.
        descriptor.online_state = previous.online_state;
        descriptor.validate()?;
        self.peers.insert(peer_id, descriptor);

        self.reconcile_node_catalog_change(peer_id, &previous)?;
        self.emit(RuntimeEventKind::PeerCatalogAccepted {
            session_id,
            peer_id,
        });
        Ok(())
    }

    fn close_session_inner(&mut self, session_id: SessionId) -> Result<(), RuntimeError> {
        let (peer_id, state) = self
            .sessions
            .get(&session_id)
            .map(|session| (session.remote_node_id, session.state))
            .ok_or(RuntimeError::UnknownSession(session_id))?;
        if state == SessionState::Closed {
            return Ok(());
        }
        self.sessions
            .get_mut(&session_id)
            .expect("checked Session")
            .begin_close()?;
        self.emit(RuntimeEventKind::SessionStateChanged {
            session_id,
            state: SessionState::Closing,
        });

        let route_ids = self
            .routes
            .values()
            .filter(|route| route.session_id == session_id)
            .map(|route| route.id)
            .collect::<Vec<_>>();
        for route_id in route_ids {
            let state = self.routes.get(&route_id).expect("collected Route").state;
            if !matches!(
                state,
                RouteState::Stopped | RouteState::Failed | RouteState::Offline
            ) {
                self.routes
                    .get_mut(&route_id)
                    .expect("collected Route")
                    .mark_offline()?;
                self.emit(RuntimeEventKind::RouteChanged {
                    route_id,
                    state: RouteState::Offline,
                });
            }
            self.release_route_leases(route_id);
        }

        self.sessions
            .get_mut(&session_id)
            .expect("checked Session")
            .mark_closed()?;
        self.emit(RuntimeEventKind::SessionStateChanged {
            session_id,
            state: SessionState::Closed,
        });

        let peer_has_live_session = self.sessions.values().any(|session| {
            session.id != session_id
                && session.remote_node_id == peer_id
                && matches!(
                    session.state,
                    SessionState::Opening | SessionState::Ready | SessionState::Suspended
                )
        });
        if !peer_has_live_session {
            self.set_peer_online(peer_id, false)?;
        }
        Ok(())
    }

    fn reconcile_node_catalog_change(
        &mut self,
        peer_id: NodeId,
        previous: &NodeDescriptor,
    ) -> Result<(), RuntimeError> {
        let affected = self
            .routes
            .values()
            .filter(|route| route.source.node_id == peer_id || route.sink.node_id == peer_id)
            .map(|route| {
                let endpoint = if route.source.node_id == peer_id {
                    route.source
                } else {
                    route.sink
                };
                let adapter_id = previous
                    .capabilities
                    .get(&endpoint.capability_id)
                    .map(|capability| capability.adapter_instance_id);
                (route.id, adapter_id)
            })
            .collect::<Vec<_>>();
        for (route_id, adapter_id) in affected {
            let (source, sink) = {
                let route = self.routes.get(&route_id).expect("collected Route");
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
                    .expect("collected Route")
                    .reconcile_endpoints(&source_port, &source_adapter, &sink_port, &sink_adapter)
                    .err()
                    .map(CatalogRouteIssue::Incompatible),
                Err(issue) => Some(issue),
            };
            if let Some(issue) = issue {
                self.invalidate_route_for_catalog(route_id, peer_id, adapter_id, issue)?;
            }
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
        adapter_id: Option<AdapterInstanceId>,
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
        self.release_route_leases(route_id);
        Ok(())
    }

    fn require_route_leases(&self, route_id: RouteId, now_ms: u64) -> Result<(), RuntimeError> {
        let route = self
            .routes
            .get(&route_id)
            .ok_or(RuntimeError::UnknownRoute(route_id))?;
        for port in [route.source, route.sink] {
            if port.node_id == self.local_node.id
                && self.port_leases.policy(port).is_some()
                && !self.port_leases.has_current_claim(port, route_id, now_ms)
            {
                return Err(RuntimeError::LeaseClaimRequired { route_id, port });
            }
        }
        Ok(())
    }

    fn release_route_leases(&mut self, route_id: RouteId) {
        let released = self
            .port_leases
            .claims()
            .filter(|claim| claim.route_id == route_id)
            .copied()
            .collect::<Vec<_>>();
        self.port_leases.release_route(route_id);
        for claim in released {
            self.emit(RuntimeEventKind::PortLeaseChanged {
                port: claim.port,
                route_id,
                active: false,
            });
        }
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
    adapter_id: Option<AdapterInstanceId>,
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
        related_adapter: adapter_id,
        related_route: Some(route_id),
        human_message: "A Route endpoint is no longer compatible with its catalog".to_owned(),
        technical_detail: Some(issue.detail()),
    }
}

#[cfg(test)]
mod draft_authorization_tests {
    use super::*;
    use capyio_core::{CapabilityId, Platform, PortId, ProfileId, ProtocolVersion};

    fn fixture() -> (NodeRuntime, RouteId, PortRef) {
        let node = NodeDescriptor::new(
            NodeId::new(),
            "test",
            Platform::Linux,
            "test",
            "test",
            [ProtocolVersion::new(1, 0)],
        );
        let port = PortRef {
            node_id: node.id,
            capability_id: CapabilityId::new(),
            port_id: PortId::new(),
        };
        let mut runtime = NodeRuntime::new(node).unwrap();
        // A narrowly scoped deterministic state fixture, not an admitted peer.
        let route = Route {
            id: RouteId::new(),
            session_id: SessionId::new(),
            source: port,
            sink: PortRef {
                node_id: NodeId::new(),
                capability_id: CapabilityId::new(),
                port_id: PortId::new(),
            },
            profile: ProfileId::audio_frames_v1(),
            backend: RouteBackend::AdapterManaged,
            compatible_formats: vec![],
            compatible_qos_modes: Default::default(),
            selected_format: None,
            selected_qos_mode: None,
            state: RouteState::Draft,
            authorization: AuthorizationState::Authorized {
                expires_at_ms: Some(300),
            },
            epoch: 0,
            diagnostic_ids: vec![],
        };
        let id = route.id;
        runtime.routes.insert(id, route);
        (runtime, id, port)
    }

    #[test]
    fn restriction_emits_only_on_reduction_and_preserves_other_routes() {
        let (mut runtime, id, _) = fixture();
        let mut other = runtime.routes[&id].clone();
        other.id = RouteId::new();
        runtime.routes.insert(other.id, other.clone());
        runtime.restrict_draft_authorization(id, 200, 100).unwrap();
        assert_eq!(runtime.routes[&other.id], other);
        let after = runtime.snapshot();
        assert_eq!(after.events.len(), 1);
        assert!(
            matches!(after.events[0].kind, RuntimeEventKind::RouteChanged { route_id, state: RouteState::Draft } if route_id == id)
        );
        runtime.restrict_draft_authorization(id, 200, 100).unwrap();
        assert_eq!(runtime.snapshot(), after);
        assert!(runtime.restrict_draft_authorization(id, 201, 100).is_err());
        assert_eq!(runtime.snapshot(), after);
        assert!(
            runtime
                .restrict_draft_authorization(RouteId::new(), 150, 100)
                .is_err()
        );
        assert_eq!(runtime.snapshot(), after);
    }

    #[test]
    fn restriction_rejects_even_expired_retained_route_lease_without_mutation() {
        let (mut runtime, id, port) = fixture();
        runtime
            .port_leases
            .set_policy(port, PortLeasePolicy::Exclusive)
            .unwrap();
        runtime
            .port_leases
            .claim(
                PortLeaseClaim {
                    port,
                    route_id: id,
                    session_id: runtime.routes[&id].session_id,
                    expires_at_ms: 50,
                },
                1,
            )
            .unwrap();
        let before = runtime.snapshot();
        assert!(
            matches!(runtime.restrict_draft_authorization(id, 200, 100), Err(RuntimeError::DraftAuthorizationLeasePresent(found)) if found == id)
        );
        assert_eq!(runtime.snapshot(), before);
    }
}
