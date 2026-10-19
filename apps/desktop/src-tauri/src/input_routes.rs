//! Connection-owned touchpad/haptics Routes, pending consent, leases and data workers.
use super::*;

pub(super) struct InputRoutes<'a> {
    node: &'a Arc<Mutex<DesktopNodeState>>,
    dial: &'a ResolvedControlDial,
    gamepad: &'a Arc<Mutex<GamepadLab>>,
    selected: &'a Arc<Mutex<BTreeSet<CapabilityId>>>,
    managed_routes: BTreeMap<CapabilityId, (RouteId, PortRef)>,
    data_workers: BTreeMap<CapabilityId, ResourceDataWorker>,
    pending_routes: BTreeMap<CapabilityId, (RouteId, Instant)>,
    next_renewal: Instant,
    controller_feedback: Option<CapabilityId>,
}

impl<'a> InputRoutes<'a> {
    pub fn new(
        node: &'a Arc<Mutex<DesktopNodeState>>,
        dial: &'a ResolvedControlDial,
        gamepad: &'a Arc<Mutex<GamepadLab>>,
        selected: &'a Arc<Mutex<BTreeSet<CapabilityId>>>,
    ) -> Self {
        Self {
            node,
            dial,
            gamepad,
            selected,
            managed_routes: BTreeMap::new(),
            data_workers: BTreeMap::new(),
            pending_routes: BTreeMap::new(),
            next_renewal: Instant::now() + touchpad_lease::RENEW_INTERVAL,
            controller_feedback: None,
        }
    }

    pub fn cleanup_feedback(&mut self, link: &mut CatalogControlLink) {
        if self.controller_feedback.is_some()
            && self.gamepad.lock().is_ok_and(|lab| {
                lab.workflow.peer() != Some(link.peer_node_id())
                    || lab
                        .workflow
                        .snapshot()
                        .is_none_or(|s| matches!(s.status, "idle" | "failed" | "stopping"))
            })
            && let Some(capability) = self.controller_feedback.take()
        {
            if let Some((route, _)) = self.pending_routes.remove(&capability) {
                let _ = cancel_pending_remote_route(link, self.node, route);
            }
            if let Some((route, port)) = self.managed_routes.remove(&capability) {
                let _ = stop_remote_capability_route(link, self.node, route, port);
            }
            self.data_workers.remove(&capability);
        }
    }

    pub fn begin_feedback(&mut self, link: &mut CatalogControlLink) {
        let capability = self.node.lock().ok().and_then(|state| {
            let snapshot = state.runtime.snapshot();
            let candidates: Vec<_> = snapshot
                .peers
                .iter()
                .filter(|p| p.id == link.peer_node_id())
                .flat_map(|p| p.capabilities.values())
                .filter(|c| {
                    c.class == CapabilityClass::Haptics
                        && c.ports.values().any(|p| {
                            p.direction == PortDirection::Sink
                                && p.profile == capyio_core::ProfileId::haptics_feedback_v1()
                        })
                })
                .map(|c| c.id)
                .collect();
            (candidates.len() == 1).then(|| candidates[0])
        });
        if let Some(capability) = capability.filter(|capability| {
            !self.managed_routes.contains_key(capability)
                && !self.pending_routes.contains_key(capability)
        }) {
            match start_remote_capability_route_with_data(
                link,
                self.node,
                capability,
                Some(self.dial),
                Some(self.gamepad),
                Some(self.selected),
            ) {
                Ok(Some((route, port, worker))) => {
                    self.controller_feedback = Some(capability);
                    self.managed_routes.insert(capability, (route, port));
                    if let Some(worker) = worker {
                        self.data_workers.insert(capability, worker);
                    }
                }
                Ok(None) => {
                    if let Some(route) = self.node.lock().ok().and_then(|state| {
                        state.runtime.snapshot().routes.into_iter().find(|r| {
                            r.session_id == link.session_id()
                                && r.sink.capability_id == capability
                                && r.authorization == AuthorizationState::Pending
                                && r.state == RouteState::Draft
                        })
                    }) {
                        self.pending_routes.insert(
                            capability,
                            (route.id, Instant::now() + Duration::from_secs(60)),
                        );
                        self.controller_feedback = Some(capability);
                    }
                }
                Err(_) => {} // Optional feedback never grants itself or breaks working input.
            }
        }
    }

    pub fn tick(&mut self, link: &mut CatalogControlLink) -> Result<(), String> {
        let finished: Vec<_> = self
            .data_workers
            .iter()
            .filter(|(_, worker)| worker.is_finished())
            .map(|(capability, _)| *capability)
            .collect();
        for capability in finished {
            self.data_workers.remove(&capability);
            if let Some((route, port)) = self.managed_routes.remove(&capability) {
                let _ = stop_remote_capability_route(link, self.node, route, port);
            }
        }
        if Instant::now() >= self.next_renewal {
            for capability in self.data_workers.keys().copied().collect::<Vec<_>>() {
                if let Some((route, port)) = self.managed_routes.get(&capability).copied()
                    && let Err(error) = touchpad_lease::renew(link, self.node, route, port)
                {
                    self.data_workers.remove(&capability);
                    self.managed_routes.remove(&capability);
                    match error {
                        touchpad_lease::RenewError::Link(error) => {
                            return Err(error);
                        }
                        touchpad_lease::RenewError::Lease(error) => {
                            stop_remote_capability_route(link, self.node, route, port)?;
                            touchpad_data::report_lease_failure(self.node, route, port, error);
                        }
                    }
                }
            }
            self.next_renewal = Instant::now() + touchpad_lease::RENEW_INTERVAL;
        }
        // Consent polling runs on the existing control owner, not the UI
        // thread. Reuse the exact proposal; denial never creates a retry.
        let pending: Vec<_> = self
            .pending_routes
            .iter()
            .map(|(capability, value)| (*capability, *value))
            .collect();
        for (capability, (route, deadline)) in pending {
            // Pending feedback belongs to the projection, not the WebView.
            // A late consent must not revive feedback after projection stop.
            let is_gamepad_feedback = self.node.lock().is_ok_and(|node| {
                node.runtime.route(route).is_ok_and(|route| {
                    route.source.capability_id.to_string() == haptics_data::CAPABILITY
                })
            });
            let projection_stopped = is_gamepad_feedback
                && self
                    .gamepad
                    .lock()
                    .map_or(true, |lab| lab.feedback_motors().is_none());
            if Instant::now() >= deadline || projection_stopped {
                self.pending_routes.remove(&capability);
                let _ = cancel_pending_remote_route(link, self.node, route);
                continue;
            }
            match start_remote_capability_route_with_data(
                link,
                self.node,
                capability,
                Some(self.dial),
                Some(self.gamepad),
                Some(self.selected),
            ) {
                Ok(None) => {}
                Ok(Some((route, port, worker))) => {
                    self.pending_routes.remove(&capability);
                    self.managed_routes.insert(capability, (route, port));
                    if let Some(worker) = worker {
                        self.data_workers.insert(capability, worker);
                    }
                }
                Err(_) => {
                    self.pending_routes.remove(&capability);
                    let _ = cancel_pending_remote_route(link, self.node, route);
                }
            }
        }

        Ok(())
    }

    pub fn begin(
        &mut self,
        link: &mut CatalogControlLink,
        capability_id: CapabilityId,
    ) -> Result<(), String> {
        if let std::collections::btree_map::Entry::Vacant(pending_entry) =
            self.pending_routes.entry(capability_id)
            && let std::collections::btree_map::Entry::Vacant(entry) =
                self.managed_routes.entry(capability_id)
        {
            start_remote_capability_route_with_data(
                link,
                self.node,
                capability_id,
                Some(self.dial),
                Some(self.gamepad),
                Some(self.selected),
            )
            .and_then(|started| {
                if let Some((route, port, worker)) = started {
                    entry.insert((route, port));
                    if let Some(worker) = worker {
                        self.data_workers.insert(capability_id, worker);
                    }
                } else {
                    let route = self
                        .node
                        .lock()
                        .map_err(|_| "Node state lock poisoned")?
                        .runtime
                        .snapshot()
                        .routes
                        .into_iter()
                        .find(|route| {
                            route.session_id == link.session_id()
                                && (route.source.capability_id == capability_id
                                    || route.sink.capability_id == capability_id)
                                && route.state == RouteState::Draft
                                && route.authorization == AuthorizationState::Pending
                        })
                        .ok_or("pending resource request disappeared")?;
                    pending_entry.insert((route.id, Instant::now() + Duration::from_secs(60)));
                }
                Ok(())
            })
        } else {
            Ok(())
        }
    }

    pub fn stop(
        &mut self,
        link: &mut CatalogControlLink,
        capability_id: CapabilityId,
    ) -> Result<(), String> {
        let result = if let Some((route, _)) = self.pending_routes.remove(&capability_id) {
            cancel_pending_remote_route(link, self.node, route)
        } else {
            match self.managed_routes.get(&capability_id).copied() {
                Some((route_id, port)) => {
                    stop_remote_capability_route(link, self.node, route_id, port)
                }
                None => Ok(()),
            }
        };
        // Even a failed control exchange must terminate local input.
        self.data_workers.remove(&capability_id);
        self.managed_routes.remove(&capability_id);
        result
    }
}
