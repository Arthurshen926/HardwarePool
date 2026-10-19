//! Authenticated control-owner orchestration; the UI never owns data lifetime.
use super::*;
use controller_workflow::ControllerMode;
use std::sync::OnceLock;

fn normal_stop(state: RouteState, authorization: &AuthorizationState) -> bool {
    state == RouteState::Stopped && *authorization != AuthorizationState::Denied
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ResourceRequest {
    pub route_id: RouteId,
    pub peer_node_id: NodeId,
    pub(super) peer_name: String,
    pub(super) capability_name: String,
    pub(super) resource_kind: &'static str,
    pub(super) capability_id: CapabilityId,
    pub(super) expires_at_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_peer_stop_is_not_reported_as_a_failure() {
        assert!(normal_stop(
            RouteState::Stopped,
            &AuthorizationState::Authorized {
                expires_at_ms: Some(100)
            }
        ));
        assert!(normal_stop(
            RouteState::Stopped,
            &AuthorizationState::Pending
        ));
        assert!(!normal_stop(
            RouteState::Stopped,
            &AuthorizationState::Denied
        ));
        assert!(!normal_stop(
            RouteState::Offline,
            &AuthorizationState::Authorized {
                expires_at_ms: Some(100)
            }
        ));
        assert!(!normal_stop(
            RouteState::Failed,
            &AuthorizationState::Pending
        ));
    }
    #[test]
    fn cancelled_consent_does_not_stop_existing_manual_receiver() {
        let node = Arc::new(Mutex::new(DesktopNodeState::demo().unwrap()));
        let gamepad = Arc::new(Mutex::new(GamepadLab::new().unwrap()));
        let port = {
            let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            socket.local_addr().unwrap().port()
        };
        gamepad.lock().unwrap().start_android(port).unwrap();
        let before = gamepad.lock().unwrap().controller_receiver_endpoint();
        let route = node.lock().unwrap().demo_routes.phone_microphone_to_windows;
        let peer = node
            .lock()
            .unwrap()
            .runtime
            .route(route)
            .unwrap()
            .source
            .node_id;
        gamepad
            .lock()
            .unwrap()
            .workflow
            .begin(peer, ControllerMode::Dsu, route)
            .unwrap();
        let mut coordinator = Coordinator {
            route: Some(route),
            ..Coordinator::default()
        };
        coordinator.local_stop(&node, &gamepad).unwrap();
        assert_eq!(
            before,
            gamepad.lock().unwrap().controller_receiver_endpoint()
        );
        assert_eq!(
            gamepad.lock().unwrap().workflow.snapshot().unwrap().status,
            "idle"
        );
        gamepad.lock().unwrap().stop_android().unwrap();
    }
}
pub(super) fn requests() -> &'static Mutex<BTreeMap<RouteId, ResourceRequest>> {
    static REQUESTS: OnceLock<Mutex<BTreeMap<RouteId, ResourceRequest>>> = OnceLock::new();
    REQUESTS.get_or_init(Mutex::default)
}
pub(super) fn pending() -> Result<Vec<ResourceRequest>, String> {
    Ok(requests()
        .lock()
        .map_err(|_| "controller requests lock poisoned")?
        .values()
        .cloned()
        .collect())
}

#[derive(Default)]
pub(super) struct Coordinator {
    route: Option<RouteId>,
    incoming: bool,
    started: bool,
    receiver_owned: bool,
    projected: bool,
    deadline: Option<Instant>,
    renew_at: Option<Instant>,
    input_lost_at: Option<Instant>,
    pub feedback_requested: bool,
}
impl Coordinator {
    pub fn owns(&self, id: RouteId) -> bool {
        self.route == Some(id)
    }
    pub fn begin(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        gamepad: &Arc<Mutex<GamepadLab>>,
        mode: ControllerMode,
    ) -> Result<(), String> {
        let route = {
            let mut node = node.lock().map_err(|_| "Node lock poisoned")?;
            let snapshot = node.runtime.snapshot();
            let peer = snapshot
                .peers
                .iter()
                .find(|p| p.id == link.peer_node_id())
                .ok_or("peer catalog unavailable")?;
            let sources: Vec<_> = peer
                .capabilities
                .values()
                .flat_map(|c| {
                    c.ports
                        .values()
                        .filter(|p| {
                            p.direction == PortDirection::Source
                                && p.profile == capyio_core::ProfileId::gamepad_state_v1()
                        })
                        .map(move |p| PortRef {
                            node_id: peer.id,
                            capability_id: c.id,
                            port_id: p.id,
                        })
                })
                .collect();
            if sources.len() != 1 {
                return Err("请在平板共享唯一的触控手柄资源后重试".into());
            }
            let (capability_id, port_id) = controller_workflow::sink(mode);
            let sink = PortRef {
                node_id: snapshot.local_node.id,
                capability_id,
                port_id,
            };
            let mut lab = gamepad.lock().map_err(|_| "gamepad lock poisoned")?;
            if lab
                .workflow
                .snapshot()
                .is_some_and(|s| !matches!(s.status, "idle" | "failed"))
            {
                return Err("请先停止当前手柄".into());
            }
            let id = node
                .runtime
                .create_route(
                    link.session_id(),
                    sources[0],
                    sink,
                    RouteBackend::AdapterManaged,
                )
                .map_err(|e| e.to_string())?;
            lab.workflow.begin(peer.id, mode, id)?;
            node.runtime.route(id).map_err(|e| e.to_string())?.clone()
        };
        *self = Self {
            route: Some(route.id),
            deadline: Some(Instant::now() + Duration::from_secs(60)),
            ..Self::default()
        };
        link.send_route_status(&route).map_err(|e| e.to_string())?;
        receive_route_status_for(link, node, route.id)?;
        Ok(())
    }
    pub fn receive(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        gamepad: &Arc<Mutex<GamepadLab>>,
        dial: &ResolvedControlDial,
        status: &capyio_protocol::ValidatedRouteStatus,
    ) -> Result<bool, String> {
        let Some(mode) = controller_workflow::mode_for_sink(status.route.sink.port_id) else {
            return Ok(false);
        };
        let mut state = node.lock().map_err(|_| "Node lock poisoned")?;
        if status.route.source.node_id != link.peer_node_id()
            || status.route.sink.node_id != state.runtime.snapshot().local_node.id
        {
            return Err("controller peer binding mismatch".into());
        }
        if !self.owns(status.route.id)
            && (state.runtime.route(status.route.id).is_ok()
                || status.route.state != RouteState::Draft
                || status.route.authorization != AuthorizationState::Pending)
        {
            return Ok(true);
        }
        link.apply_route_status(&mut state.runtime, status, false)
            .map_err(|e| e.to_string())?;
        let new_request = !self.owns(status.route.id);
        if new_request {
            let mut lab = gamepad.lock().map_err(|_| "gamepad lock poisoned")?;
            if lab
                .workflow
                .begin(link.peer_node_id(), mode, status.route.id)
                .is_err()
            {
                state
                    .runtime
                    .deny_pending_route(status.route.id)
                    .map_err(|e| e.to_string())?;
                let denied = state
                    .runtime
                    .route(status.route.id)
                    .map_err(|e| e.to_string())?
                    .clone();
                drop(lab);
                drop(state);
                link.send_route_status(&denied).map_err(|e| e.to_string())?;
                return Ok(true);
            }
            *self = Self {
                route: Some(status.route.id),
                incoming: true,
                deadline: Some(Instant::now() + Duration::from_secs(60)),
                ..Self::default()
            };
            if controller_permission::allows(dial, status.route.sink.capability_id) {
                state
                    .runtime
                    .authorize_route(status.route.id, Some(unix_now_ms()? + 3_600_000))
                    .map_err(|e| e.to_string())?;
            } else {
                let name = state
                    .runtime
                    .snapshot()
                    .peers
                    .iter()
                    .find(|p| p.id == link.peer_node_id())
                    .map(|p| p.display_name.clone())
                    .unwrap_or_else(|| "已认证设备".into());
                requests()
                    .lock()
                    .map_err(|_| "controller requests lock poisoned")?
                    .insert(
                        status.route.id,
                        ResourceRequest {
                            route_id: status.route.id,
                            peer_node_id: link.peer_node_id(),
                            peer_name: name,
                            capability_name: format!("Windows {:?} 手柄输出", mode),
                            resource_kind: "gamepad",
                            capability_id: status.route.sink.capability_id,
                            expires_at_ms: unix_now_ms()? + 60_000,
                        },
                    );
            }
        }
        let local = state
            .runtime
            .route(status.route.id)
            .map_err(|e| e.to_string())?
            .clone();
        drop(state);
        if matches!(
            status.route.state,
            RouteState::Stopped | RouteState::Failed | RouteState::Offline
        ) || status.route.authorization == AuthorizationState::Denied
        {
            self.local_stop(node, gamepad)?;
            if matches!(status.route.state, RouteState::Failed | RouteState::Offline)
                || status.route.authorization == AuthorizationState::Denied
            {
                gamepad
                    .lock()
                    .map_err(|_| "gamepad lock poisoned")?
                    .workflow
                    .fail(
                        if status.route.authorization == AuthorizationState::Denied {
                            "对端拒绝了本次手柄使用"
                        } else {
                            "平板手柄发送已失败或离线，请检查平板的运行状态"
                        }
                        .into(),
                    );
            }
        }
        if new_request {
            link.send_route_status(&local).map_err(|e| e.to_string())?;
        }
        Ok(true)
    }
    pub fn resolve(
        &mut self,
        node: &Arc<Mutex<DesktopNodeState>>,
        gamepad: &Arc<Mutex<GamepadLab>>,
        dial: &ResolvedControlDial,
        id: RouteId,
        allow: bool,
        remember: bool,
    ) -> Result<(), String> {
        if !self.owns(id) || !self.incoming {
            return Err("资源请求已结束".into());
        }
        let request = requests()
            .lock()
            .map_err(|_| "controller requests lock poisoned")?
            .remove(&id)
            .ok_or("资源请求已处理")?;
        let now = unix_now_ms()?;
        let mut node = node.lock().map_err(|_| "Node lock poisoned")?;
        if allow && request.expires_at_ms > now {
            if remember {
                controller_permission::remember(dial, request.capability_id)?;
            }
            node.runtime
                .authorize_route(id, Some(now + 3_600_000))
                .map_err(|e| e.to_string())?;
        } else {
            node.runtime
                .deny_pending_route(id)
                .map_err(|e| e.to_string())?;
            gamepad
                .lock()
                .map_err(|_| "gamepad lock poisoned")?
                .workflow
                .fail("本机拒绝或请求已过期".into());
        }
        Ok(())
    }
    pub fn local_stop(
        &mut self,
        node: &Arc<Mutex<DesktopNodeState>>,
        gamepad: &Arc<Mutex<GamepadLab>>,
    ) -> Result<(), String> {
        let Some(id) = self.route.take() else {
            return Ok(());
        };
        requests()
            .lock()
            .map_err(|_| "controller requests lock poisoned")?
            .remove(&id);
        let mut node = node.lock().map_err(|_| "Node lock poisoned")?;
        let mut lab = gamepad.lock().map_err(|_| "gamepad lock poisoned")?;
        lab.workflow.stopping();
        let result = if self.receiver_owned {
            lab.stop_authorized_controller(&mut node.runtime)
        } else {
            Ok(())
        };
        if node.runtime.route(id).is_ok_and(|r| {
            r.state == RouteState::Draft && r.authorization == AuthorizationState::Pending
        }) {
            let _ = node.runtime.deny_pending_route(id);
        }
        if let Ok(route) = node.runtime.route(id).cloned() {
            let _ = node
                .runtime
                .release_route_port_lease(id, route.sink, route.session_id);
        }
        lab.workflow.clear();
        *self = Self::default();
        result
    }
    pub fn stop(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        gamepad: &Arc<Mutex<GamepadLab>>,
    ) -> Result<(), String> {
        let route = self
            .route
            .and_then(|id| node.lock().ok()?.runtime.route(id).ok().cloned());
        let local_result = self.local_stop(node, gamepad);
        let remote_result = (|| -> Result<(), String> {
            if let Some(route) = route {
                link.send_route_command(route.id, RouteCommandAction::Stop)
                    .map_err(|e| e.to_string())?;
                receive_route_status_for(link, node, route.id)?;
                let request = link
                    .send_port_lease_request(route.id, route.source, PortLeaseAction::Release, 0)
                    .map_err(|e| e.to_string())?;
                control_exchange::receive_lease(link, node, request, route.id, route.source)?;
            }
            Ok(())
        })();
        local_result.and(remote_result)
    }
    pub fn tick(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        gamepad: &Arc<Mutex<GamepadLab>>,
        dial: &ResolvedControlDial,
    ) -> Result<(), String> {
        let Some(id) = self.route else { return Ok(()) };
        if self.deadline.is_some_and(|d| Instant::now() >= d) {
            return Err("手柄启动等待超时".into());
        }
        let route = node
            .lock()
            .map_err(|_| "Node lock poisoned")?
            .runtime
            .route(id)
            .map_err(|e| e.to_string())?
            .clone();
        if normal_stop(route.state, &route.authorization) {
            self.local_stop(node, gamepad)?;
            return Ok(());
        }
        if matches!(route.state, RouteState::Failed | RouteState::Offline)
            || route.authorization == AuthorizationState::Denied
        {
            link.send_route_status(&route).map_err(|e| e.to_string())?;
            return Err("手柄连接已停止或授权被拒绝".into());
        }
        if self.incoming && route.authorization == AuthorizationState::Pending {
            return Ok(());
        }
        link.send_route_status(&route).map_err(|e| e.to_string())?;
        let remote = receive_route_status_for(link, node, id)?;
        if remote.authorization == AuthorizationState::Pending {
            return Ok(());
        }
        if normal_stop(remote.state, &remote.authorization) {
            self.local_stop(node, gamepad)?;
            return Ok(());
        }
        if remote.authorization == AuthorizationState::Denied
            || matches!(remote.state, RouteState::Offline | RouteState::Failed)
        {
            return Err("对端已拒绝或停止手柄".into());
        }
        let now = unix_now_ms()?;
        let ttl = touchpad_lease::ttl(&remote.authorization, now)?;
        if !self.started {
            let request = link
                .send_port_lease_request(id, route.source, PortLeaseAction::Acquire, ttl)
                .map_err(|e| e.to_string())?;
            let response = control_exchange::receive_lease(link, node, request, id, route.source)?;
            if !matches!(
                response.outcome,
                PortLeaseOutcome::Acquired | PortLeaseOutcome::Renewed
            ) {
                return Err("平板手柄被占用或租约被拒绝".into());
            }
            {
                let mut node = node.lock().map_err(|_| "Node lock poisoned")?;
                node.runtime
                    .set_local_port_lease_policy(
                        route.sink,
                        capyio_runtime::PortLeasePolicy::Exclusive,
                    )
                    .map_err(|e| e.to_string())?;
                node.runtime
                    .acquire_route_port_lease(id, route.sink, now + ttl, now)
                    .map_err(|e| e.to_string())?;
            }
            let (port, token) = gamepad
                .lock()
                .map_err(|_| "gamepad lock poisoned")?
                .prepare_controller_receiver(31580)?;
            self.receiver_owned = true;
            let socket = std::net::UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
            socket
                .connect(dial.candidate.endpoint)
                .map_err(|e| e.to_string())?;
            link.send_controller_receiver_offer(capyio_protocol::v1::ControllerReceiverOffer {
                host: socket
                    .local_addr()
                    .map_err(|e| e.to_string())?
                    .ip()
                    .to_string(),
                port: port.into(),
                token,
            })
            .map_err(|e| e.to_string())?;
            gamepad
                .lock()
                .map_err(|_| "gamepad lock poisoned")?
                .workflow
                .starting();
            link.send_route_command(id, RouteCommandAction::Start)
                .map_err(|e| e.to_string())?;
            receive_route_status_for(link, node, id)?;
            self.started = true;
            self.renew_at = Some(Instant::now() + touchpad_lease::RENEW_INTERVAL);
            return Ok(());
        }
        if !self.projected
            && gamepad
                .lock()
                .map_err(|_| "gamepad lock poisoned")?
                .controller_input_live()
        {
            let mut node = node.lock().map_err(|_| "Node lock poisoned")?;
            let mut lab = gamepad.lock().map_err(|_| "gamepad lock poisoned")?;
            let mode = lab.workflow.mode().ok_or("controller mode missing")?;
            lab.activate_authorized_controller(&mut node.runtime, id, mode, now)?;
            self.projected = true;
        }
        if self.projected && remote.state == RouteState::Active {
            gamepad
                .lock()
                .map_err(|_| "gamepad lock poisoned")?
                .workflow
                .active();
            self.deadline = None;
        }
        if self.projected {
            let lab = gamepad.lock().map_err(|_| "gamepad lock poisoned")?;
            if !lab
                .workflow
                .mode()
                .is_some_and(|mode| lab.controller_projection_live(mode))
            {
                return Err("Windows 手柄输出已停止".into());
            }
            drop(lab);
            if gamepad
                .lock()
                .map_err(|_| "gamepad lock poisoned")?
                .controller_input_live()
            {
                self.input_lost_at = None;
            } else if self
                .input_lost_at
                .get_or_insert_with(Instant::now)
                .elapsed()
                > Duration::from_secs(3)
            {
                return Err("手柄输入已断开，已停止两端".into());
            }
        }
        if self.renew_at.is_some_and(|d| Instant::now() >= d) {
            touchpad_lease::renew(link, node, id, route.source).map_err(String::from)?;
            self.renew_at = Some(Instant::now() + touchpad_lease::RENEW_INTERVAL);
        }
        Ok(())
    }
}
