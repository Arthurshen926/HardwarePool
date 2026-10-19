//! Node-authenticated display orchestration. The service owns capture and restoration.
use super::route_readiness::{MediaRequirement, RouteMediaSnapshot, RouteReadiness};
use super::*;
use capyio_core::*;
use capyio_protocol::v1::{
    DisplaySessionAction as Action, DisplaySessionControl as Message, DisplayStreamCapabilities,
    DisplayStreamMode, DisplayStreamSelection,
};

pub(super) fn source() -> (CapabilityId, PortId) {
    (
        "7d5ad480-23d8-4aac-9ea9-8e18672a7221".parse().unwrap(),
        "7d5ad480-23d8-4aac-9ea9-8e18672a7222".parse().unwrap(),
    )
}
pub(super) fn format() -> FormatDescriptor {
    FormatDescriptor::new("gamestream-h264-sdr-v1")
}
fn policy() -> DisplayStreamCapabilities {
    DisplayStreamCapabilities {
        modes: [(1280, 720), (1920, 1080), (2560, 1440)]
            .into_iter()
            .flat_map(|(width, height)| {
                [30_000, 60_000]
                    .into_iter()
                    .map(move |refresh_millihertz| DisplayStreamMode {
                        width,
                        height,
                        refresh_millihertz,
                    })
            })
            .collect(),
        minimum_bitrate_kbps: 2_000,
        maximum_bitrate_kbps: 50_000,
        h264_sdr: true,
    }
}
fn automatic_selection_policy(
    selection: &DisplayStreamSelection,
) -> Result<DisplayStreamCapabilities, String> {
    let mode = selection.mode.as_ref().ok_or("接收端未返回自动画质")?;
    if mode.width < 2
        || mode.height < 2
        || mode.width % 2 != 0
        || mode.height % 2 != 0
        || mode.width.max(mode.height) > 2560
        || mode.width.min(mode.height) > 1600
        || ![30_000, 60_000].contains(&mode.refresh_millihertz)
        || !(5_000..=40_000).contains(&selection.bitrate_kbps)
    {
        return Err("接收端自动画质超出当前显示适配器策略".into());
    }
    // This is the adapter's bounded dynamic-mode policy, not a GPU probe.
    Ok(DisplayStreamCapabilities {
        modes: vec![*mode],
        minimum_bitrate_kbps: 5_000,
        maximum_bitrate_kbps: 40_000,
        h264_sdr: true,
    })
}
enum Deferred {
    Message(Message),
    Status(Box<capyio_protocol::ValidatedRouteStatus>),
}
fn mailbox() -> &'static Mutex<BTreeMap<SessionId, std::collections::VecDeque<Deferred>>> {
    static QUEUE: std::sync::OnceLock<
        Mutex<BTreeMap<SessionId, std::collections::VecDeque<Deferred>>>,
    > = std::sync::OnceLock::new();
    QUEUE.get_or_init(Mutex::default)
}
pub(super) fn defer_message(session: SessionId, value: Message) -> Result<(), String> {
    let mut queue = mailbox().lock().map_err(|_| "display queue poisoned")?;
    let q = queue.entry(session).or_default();
    if q.len() >= 32 {
        return Err("display queue full".into());
    }
    q.push_back(Deferred::Message(value));
    Ok(())
}
pub(super) fn defer_status(value: &capyio_protocol::ValidatedRouteStatus) -> bool {
    if (value.route.source.capability_id, value.route.source.port_id) != source() {
        return false;
    }
    if let Ok(mut queue) = mailbox().lock() {
        let q = queue.entry(value.session_id).or_default();
        if q.len() < 32 {
            q.push_back(Deferred::Status(Box::new(value.clone())));
            return true;
        }
    }
    false
}
pub(super) fn install_catalog(runtime: &mut NodeRuntime) -> Result<(), String> {
    let adapter: AdapterInstanceId = "7d5ad480-23d8-4aac-9ea9-8e18672a7220".parse().unwrap();
    if runtime
        .snapshot()
        .local_node
        .adapter_instances
        .contains_key(&adapter)
    {
        return Ok(());
    }
    let (id, port) = source();
    runtime
        .register_adapter_catalog(
            runtime.snapshot().local_node.id,
            AdapterInstanceDescriptor {
                id: adapter,
                adapter_type: "dev.capyio.windows.display".into(),
                display_name: "Windows 扩展屏幕".into(),
                deployment_mode: AdapterDeploymentMode::ExternalService,
                version: env!("CARGO_PKG_VERSION").into(),
                state: AdapterState::Ready,
                health: AdapterHealth::Unknown,
                owned_capabilities: BTreeSet::new(),
                supported_route_modes: BTreeSet::from([RouteBackend::AdapterManaged]),
            },
            vec![CapabilityDescriptor {
                id,
                adapter_instance_id: adapter,
                display_name: "Windows 扩展屏幕".into(),
                class: CapabilityClass::Display,
                availability: Availability::Available,
                permission_requirement: PermissionRequirement::UserConfirmation,
                metadata: BTreeMap::new(),
                ports: BTreeMap::from([(
                    port,
                    PortDescriptor {
                        id: port,
                        capability_id: id,
                        display_name: "扩展屏幕画面".into(),
                        direction: PortDirection::Source,
                        profile: ProfileId::video_frames_v1(),
                        schema_id: None,
                        formats: vec![format()],
                        qos_modes: BTreeSet::from([QosMode::Interactive]),
                        clock_domain: None,
                        availability: Availability::Available,
                        permission_requirement: PermissionRequirement::UserConfirmation,
                        interoperability_mode: InteroperabilityMode::AdapterManaged,
                    },
                )]),
            }],
        )
        .map_err(|e| e.to_string())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UiDisplayUseState {
    pub automatic: bool,
    pub peer_node_id: String,
    pub operation_id: String,
    pub route_id: String,
    pub status: String,
    pub error: Option<String>,
    pub width: u32,
    pub height: u32,
    pub refresh_millihertz: u32,
    pub bitrate_kbps: u32,
    pub presented_frames: u64,
}
fn view() -> &'static Mutex<Option<UiDisplayUseState>> {
    static VALUE: std::sync::OnceLock<Mutex<Option<UiDisplayUseState>>> =
        std::sync::OnceLock::new();
    VALUE.get_or_init(Mutex::default)
}
pub(super) fn get_display_use_state() -> Result<Option<UiDisplayUseState>, String> {
    view()
        .lock()
        .map(|v| v.clone())
        .map_err(|_| "display state lock poisoned".into())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DisplayPeer {
    pub(super) node_id: String,
    pub(super) display_name: String,
}
pub(super) fn get_display_peers(state: &HostState) -> Result<Vec<DisplayPeer>, String> {
    state.0.get_display_peers()
}
pub(super) fn start_display_use(
    state: &HostState,
    peer_node_id: String,
    width: u32,
    height: u32,
    refresh_millihertz: u32,
    bitrate_kbps: u32,
    automatic: Option<bool>,
) -> Result<(), String> {
    state.0.start_display_use(
        peer_node_id,
        width,
        height,
        refresh_millihertz,
        bitrate_kbps,
        automatic,
    )
}
pub(super) fn stop_display_use(state: &HostState) -> Result<(), String> {
    state.0.stop_display_use()
}

#[derive(Default)]
pub(super) struct Coordinator {
    observation: Option<RouteMediaSnapshot>,
    operation: Option<String>,
    route: Option<RouteId>,
    selection: Option<DisplayStreamSelection>,
    caps: Option<DisplayStreamCapabilities>,
    automatic: bool,
    local_allowed: bool,
    remote_allowed: bool,
    generation: Option<u64>,
    started: bool,
    deadline: Option<Instant>,
    expires: u64,
    stopping: bool,
    stop_deadline: Option<Instant>,
    failure: Option<String>,
    renew: Option<Instant>,
    ready: bool,
    proposed_expiry: Option<u64>,
    poll_after: Option<Instant>,
}
impl Coordinator {
    pub fn observation(&self) -> Option<RouteMediaSnapshot> {
        self.observation
    }

    pub fn owns(&self, id: RouteId) -> bool {
        self.route == Some(id)
    }
    fn message(&self, action: Action) -> Message {
        Message {
            operation_id: self.operation.clone().unwrap_or_default(),
            route_id: self.route.map(|r| r.to_string()).unwrap_or_default(),
            action: action as i32,
            ..Default::default()
        }
    }
    fn state(&self, status: &str, error: Option<String>) {
        if let Ok(mut v) = view().lock()
            && let Some(v) = v.as_mut()
            && Some(&v.operation_id) == self.operation.as_ref()
        {
            v.status = status.into();
            v.error = error;
        }
    }
    pub fn begin(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        dial: &ResolvedControlDial,
        selection: DisplayStreamSelection,
        automatic: bool,
        remote: Option<String>,
    ) -> Result<(), String> {
        if self.route.is_some() || get_display_use_state()?.is_some_and(|s| s.status != "failed") {
            return Err("请先停止当前屏幕共享".into());
        }
        let operation = remote.clone().unwrap_or_else(|| RouteId::new().to_string());
        let mode = selection.mode.ok_or("缺少画质参数")?;
        capyio_protocol::validate_display_session_control(&Message {
            operation_id: operation.clone(),
            action: Action::Start as i32,
            selection: Some(selection),
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
        let mut n = node.lock().map_err(|_| "node lock poisoned")?;
        install_catalog(&mut n.runtime)?;
        let snap = n.runtime.snapshot();
        let sinks: Vec<_> = snap
            .peers
            .iter()
            .filter(|p| p.id == link.peer_node_id())
            .flat_map(|p| p.capabilities.values())
            .filter(|c| c.class == CapabilityClass::Display)
            .flat_map(|c| {
                c.ports
                    .values()
                    .filter(|p| p.direction == PortDirection::Sink && p.formats.contains(&format()))
                    .map(|p| PortRef {
                        node_id: link.peer_node_id(),
                        capability_id: c.id,
                        port_id: p.id,
                    })
            })
            .collect();
        if sinks.len() != 1 {
            return Err("对端尚未公布唯一的屏幕接收资源，请更新并重新连接".into());
        }
        let (capability_id, port_id) = source();
        let id = n
            .runtime
            .create_route(
                link.session_id(),
                PortRef {
                    node_id: snap.local_node.id,
                    capability_id,
                    port_id,
                },
                sinks[0],
                RouteBackend::AdapterManaged,
            )
            .map_err(|e| e.to_string())?;
        self.operation = Some(operation.clone());
        self.route = Some(id);
        self.selection = Some(selection);
        self.automatic = automatic;
        self.deadline = Some(Instant::now() + Duration::from_secs(90));
        self.local_allowed = remote.is_none() || controller_permission::allows(dial, source().0);
        self.expires = unix_now_ms()? + 600_000;
        if !self.local_allowed {
            controller_control::requests()
                .lock()
                .map_err(|_| "request lock poisoned")?
                .insert(
                    id,
                    controller_control::ResourceRequest {
                        route_id: id,
                        peer_node_id: link.peer_node_id(),
                        peer_name: "已认证屏幕节点".into(),
                        capability_name: "Windows 扩展屏幕画面".into(),
                        resource_kind: "display",
                        capability_id: source().0,
                        expires_at_ms: unix_now_ms()? + 90_000,
                    },
                );
        }
        *view().lock().map_err(|_| "display lock poisoned")? = Some(UiDisplayUseState {
            automatic,
            peer_node_id: link.peer_node_id().to_string(),
            operation_id: operation,
            route_id: id.to_string(),
            status: "awaiting_consent".into(),
            error: None,
            width: mode.width,
            height: mode.height,
            refresh_millihertz: mode.refresh_millihertz,
            bitrate_kbps: selection.bitrate_kbps,
            presented_frames: 0,
        });
        let mut start = self.message(Action::Start);
        start.selection = Some(selection);
        start.automatic = automatic;
        link.send_display_session_control(start)
            .map_err(|e| e.to_string())?;
        link.send_route_status(n.runtime.route(id).unwrap())
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn resolve(
        &mut self,
        node: &Arc<Mutex<DesktopNodeState>>,
        dial: &ResolvedControlDial,
        id: RouteId,
        allow: bool,
        remember: bool,
    ) -> Result<(), String> {
        if !self.owns(id) {
            return Err("屏幕请求已结束".into());
        }
        let request = controller_control::requests()
            .lock()
            .map_err(|_| "request lock poisoned")?
            .remove(&id)
            .ok_or("屏幕请求已处理")?;
        if allow && request.expires_at_ms > unix_now_ms()? {
            if remember {
                controller_permission::remember(dial, source().0)?;
            }
            self.local_allowed = true;
            Ok(())
        } else {
            node.lock()
                .map_err(|_| "node lock poisoned")?
                .runtime
                .deny_pending_route(id)
                .map_err(|e| e.to_string())
        }
    }
    pub fn receive_status(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        status: &capyio_protocol::ValidatedRouteStatus,
    ) -> Result<bool, String> {
        if !self.owns(status.route.id) {
            return Ok(false);
        }
        let n = node.lock().map_err(|_| "node lock poisoned")?;
        let own = n
            .runtime
            .route(status.route.id)
            .map_err(|e| e.to_string())?;
        if own.source != status.route.source
            || own.sink != status.route.sink
            || status.session_id != link.session_id()
        {
            return Err("屏幕Route绑定不一致".into());
        }
        if matches!(
            status.route.authorization,
            AuthorizationState::Denied | AuthorizationState::Revoked
        ) || matches!(
            status.route.state,
            RouteState::Stopped | RouteState::Failed | RouteState::Offline
        ) {
            drop(n);
            self.stop(link, node, false)?;
            return Ok(true);
        }
        if let AuthorizationState::Authorized {
            expires_at_ms: Some(expiry),
        } = status.route.authorization
            && expiry > unix_now_ms()?
        {
            self.remote_allowed = true;
            if self.proposed_expiry == Some(expiry) {
                #[cfg(windows)]
                if let Some(generation) = self.generation {
                    capyio_windows_service::DisplayServiceClient::default()
                        .renew_peer(generation, expiry)?;
                }
                self.expires = expiry;
                self.proposed_expiry = None;
            } else if self.generation.is_none() {
                self.expires = self.expires.min(expiry);
            }
        }
        // Local permission is never replaced by a peer's Authorized status.
        Ok(true)
    }
    pub fn receive(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        dial: &ResolvedControlDial,
        value: Message,
    ) -> Result<(), String> {
        let action = Action::try_from(value.action).map_err(|_| "unknown display action")?;
        if action == Action::Start
            && self
                .operation
                .as_ref()
                .is_some_and(|id| id != &value.operation_id)
        {
            let failed = Message {
                action: Action::Failed as i32,
                operation_id: value.operation_id,
                route_id: value.route_id,
                problem_code: "CAPY.DISPLAY.HOST_BUSY".into(),
                ..Default::default()
            };
            link.send_display_session_control(failed)
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        if action == Action::Start && self.operation.is_none() {
            let operation = value.operation_id.clone();
            let result = self.begin(
                link,
                node,
                dial,
                value.selection.ok_or("缺少画质参数")?,
                value.automatic,
                Some(value.operation_id),
            );
            if result.is_err() && self.operation.is_none() {
                let _ = link.send_display_session_control(Message {
                    operation_id: operation,
                    action: Action::Failed as i32,
                    problem_code: "CAPY.DISPLAY.START_REJECTED".into(),
                    ..Default::default()
                });
            }
            return result;
        }
        if self.operation.as_deref() != Some(&value.operation_id)
            || self.route.map(|r| r.to_string()).as_deref() != Some(value.route_id.as_str())
        {
            return Ok(());
        }
        match action {
            Action::Capabilities => {
                if self.generation.is_some() {
                    return Ok(());
                }
                if self.automatic {
                    let selection = value
                        .selection
                        .ok_or("接收端未返回自动画质，请更新接收端")?;
                    let sender = automatic_selection_policy(&selection)?;
                    let caps = value.capabilities.as_ref().ok_or("接收端未返回解码能力")?;
                    capyio_protocol::negotiate_display_settings(&sender, caps, &selection)
                        .map_err(|e| e.to_string())?;
                    if let Ok(mut state) = view().lock()
                        && let Some(state) = state.as_mut()
                        && self.operation.as_deref() == Some(&state.operation_id)
                    {
                        let mode = selection.mode.as_ref().unwrap();
                        state.width = mode.width;
                        state.height = mode.height;
                        state.refresh_millihertz = mode.refresh_millihertz;
                        state.bitrate_kbps = selection.bitrate_kbps;
                    }
                    self.selection = Some(selection);
                }
                self.caps = value.capabilities;
            }
            Action::Ack => {
                let route = node
                    .lock()
                    .map_err(|_| "node lock poisoned")?
                    .runtime
                    .route(self.route.unwrap())
                    .map_err(|e| e.to_string())?
                    .clone();
                if route.epoch != value.epoch
                    || self.started
                    || !self.local_allowed
                    || !self.remote_allowed
                {
                    return Err("屏幕启动ACK状态不匹配".into());
                }
                #[cfg(windows)]
                capyio_windows_service::DisplayServiceClient::default().commit_peer(
                    self.generation.ok_or("屏幕尚未准备")?,
                    value.acknowledgement,
                )?;
                self.started = true;
                self.state("starting", None);
            }
            Action::Presented => {
                let mut n = node.lock().map_err(|_| "node lock poisoned")?;
                let id = self.route.unwrap();
                let readiness = RouteReadiness {
                    worker_ready: self.started,
                    // Presented is validated by the protocol to contain positive frame evidence.
                    peer_media_observed: value.presented_frames > 0,
                    ..Default::default()
                };
                if readiness.media_ready(MediaRequirement::PeerMedia)
                    && n.runtime.route(id).is_ok_and(|r| r.epoch == value.epoch)
                {
                    if n.runtime.route(id).unwrap().state == RouteState::Starting {
                        n.runtime.activate_route(id).map_err(|e| e.to_string())?;
                        link.send_route_status(n.runtime.route(id).unwrap())
                            .map_err(|e| e.to_string())?;
                    }
                    self.observation = Some(RouteMediaSnapshot {
                        session_id: link.session_id(),
                        route_id: id,
                        epoch: value.epoch,
                        sampled_at_ms: unix_now_ms()?,
                        readiness,
                        peer_frames: Some(value.presented_frames),
                    });
                    self.deadline = None;
                    self.state("active", None);
                    if let Ok(mut v) = view().lock()
                        && let Some(v) = v.as_mut()
                    {
                        v.presented_frames = value.presented_frames;
                    }
                }
            }
            Action::Stop => {
                self.stop(link, node, false)?;
            }
            Action::Failed => {
                self.failure = Some(value.problem_code);
                self.stop(link, node, false)?;
            }
            _ => {}
        }
        Ok(())
    }
    pub fn tick(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        dial: &ResolvedControlDial,
    ) {
        let queued = mailbox()
            .lock()
            .ok()
            .and_then(|mut q| q.remove(&link.session_id()))
            .unwrap_or_default();
        let mut result = Ok(());
        for value in queued {
            result = match value {
                Deferred::Message(value) => self.receive(link, node, dial, value),
                Deferred::Status(value) => self.receive_status(link, node, &value).map(|_| ()),
            };
            if result.is_err() {
                break;
            }
        }
        if let Err(error) = result.and_then(|_| self.advance(link, node, dial)) {
            self.fail(link, node, error);
        }
    }
    pub fn fail(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        error: String,
    ) {
        self.failure = Some(error);
        if self.stopping {
            self.state("stopping", self.failure.clone());
            return;
        }
        if self.operation.is_some() {
            let mut message = self.message(Action::Failed);
            message.problem_code = "CAPY.DISPLAY.WORKFLOW_FAILED".into();
            let _ = link.send_display_session_control(message);
        }
        let _ = self.stop(link, node, false);
    }
    fn advance(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        dial: &ResolvedControlDial,
    ) -> Result<(), String> {
        let Some(id) = self.route else { return Ok(()) };
        if self.finish_expired_stop(Instant::now()) {
            return Ok(());
        }
        let now = unix_now_ms()?;
        if let Some(generation) = self.generation {
            if self.poll_after.is_some_and(|t| Instant::now() < t) {
                return Ok(());
            }
            self.poll_after = Some(Instant::now() + Duration::from_secs(1));
            #[cfg(windows)]
            {
                let snapshot = capyio_windows_service::DisplayServiceClient::default()
                    .invoke(capyio_windows_service::DisplayControlOperation::Status)?;
                if snapshot.generation != generation {
                    // The previous owner no longer owns this service generation.
                    // In particular, never issue global Stop against its successor.
                    self.generation = None;
                    return Err("屏幕服务会话已被替换".into());
                }
                use capyio_windows_service::DisplayExecutionState as S;
                if self.stopping {
                    if matches!(snapshot.state, S::Stopped | S::Failed) {
                        self.finish();
                    }
                    return Ok(());
                }
                if matches!(snapshot.state, S::Stopped | S::Failed) {
                    return Err(snapshot
                        .problem_code
                        .unwrap_or_else(|| "CAPY.DISPLAY.WORKER_STOPPED".into()));
                }
                if self.started && !self.ready && snapshot.state == S::Running {
                    let route = node
                        .lock()
                        .map_err(|_| "node lock poisoned")?
                        .runtime
                        .route(id)
                        .map_err(|e| e.to_string())?
                        .clone();
                    let mut ready = self.message(Action::Ready);
                    ready.epoch = route.epoch;
                    link.send_display_session_control(ready)
                        .map_err(|e| e.to_string())?;
                    self.ready = true;
                }
            }
        }
        if self.stopping {
            self.finish();
            return Ok(());
        }
        if now >= self.expires || self.deadline.is_some_and(|d| Instant::now() >= d) {
            return Err("屏幕授权或启动等待已超时，请重新启动".into());
        }
        let route = node
            .lock()
            .map_err(|_| "node lock poisoned")?
            .runtime
            .route(id)
            .map_err(|e| e.to_string())?
            .clone();
        if matches!(
            route.state,
            RouteState::Stopped | RouteState::Failed | RouteState::Offline
        ) {
            self.stop(link, node, true)?;
            return Ok(());
        }
        if matches!(
            route.authorization,
            AuthorizationState::Denied | AuthorizationState::Revoked
        ) {
            return Err("屏幕请求未获允许".into());
        }
        if !self.local_allowed {
            return Ok(());
        }
        if route.authorization == AuthorizationState::Pending {
            let mut n = node.lock().map_err(|_| "node lock poisoned")?;
            n.runtime
                .authorize_route(id, Some(self.expires))
                .map_err(|e| e.to_string())?;
            link.send_route_status(n.runtime.route(id).unwrap())
                .map_err(|e| e.to_string())?;
        }
        if self.generation.is_some() {
            if self.renew.is_some_and(|d| Instant::now() >= d) {
                if self.proposed_expiry.is_none() {
                    let expiry = now + 600_000;
                    let mut n = node.lock().map_err(|_| "node lock poisoned")?;
                    n.runtime
                        .renew_route_authorization(id, expiry, now)
                        .map_err(|e| e.to_string())?;
                    self.proposed_expiry = Some(expiry);
                    link.send_route_status(n.runtime.route(id).unwrap())
                        .map_err(|e| e.to_string())?;
                }
                let ttl = touchpad_lease::ttl(&route.authorization, now)?;
                let request = link
                    .send_port_lease_request(id, route.sink, PortLeaseAction::Renew, ttl)
                    .map_err(|e| e.to_string())?;
                let response =
                    control_exchange::receive_lease(link, node, request, id, route.sink)?;
                if !matches!(
                    response.outcome,
                    PortLeaseOutcome::Acquired | PortLeaseOutcome::Renewed
                ) {
                    return Err("屏幕接收资源租约续期失败".into());
                }
                node.lock()
                    .map_err(|_| "node lock poisoned")?
                    .runtime
                    .acquire_route_port_lease(id, route.source, now + ttl, now)
                    .map_err(|e| e.to_string())?;
                self.renew = Some(Instant::now() + Duration::from_secs(60));
            }
            return Ok(());
        }
        if !self.remote_allowed {
            return Ok(());
        }
        let Some(caps) = self.caps.as_ref() else {
            return Ok(());
        };
        let selected = self.selection.as_ref().ok_or("display selection missing")?;
        // The engine's explicit supported-mode policy is bounded; no silent downgrade.
        let sender = if self.automatic {
            automatic_selection_policy(selected)?
        } else {
            policy()
        };
        let selected = capyio_protocol::negotiate_display_settings(&sender, caps, selected)
            .map_err(|e| e.to_string())?;
        let route = node
            .lock()
            .map_err(|_| "node lock poisoned")?
            .runtime
            .route(id)
            .map_err(|e| e.to_string())?
            .clone();
        let ttl = touchpad_lease::ttl(&route.authorization, now)?;
        let request = link
            .send_port_lease_request(id, route.sink, PortLeaseAction::Acquire, ttl)
            .map_err(|e| e.to_string())?;
        let response = control_exchange::receive_lease(link, node, request, id, route.sink)?;
        if !matches!(
            response.outcome,
            PortLeaseOutcome::Acquired | PortLeaseOutcome::Renewed
        ) {
            return Err("屏幕接收资源正在使用".into());
        }
        let mut n = node.lock().map_err(|_| "node lock poisoned")?;
        n.runtime
            .set_local_port_lease_policy(route.source, capyio_runtime::PortLeasePolicy::Exclusive)
            .map_err(|e| e.to_string())?;
        n.runtime
            .acquire_route_port_lease(id, route.source, now + ttl, now)
            .map_err(|e| e.to_string())?;
        n.runtime
            .prepare_route(id, Some(format()), QosMode::Interactive, now)
            .map_err(|e| e.to_string())?;
        n.runtime
            .begin_route_start(id, now)
            .map_err(|e| e.to_string())?;
        let route = n.runtime.route(id).unwrap().clone();
        link.send_route_status(&route).map_err(|e| e.to_string())?;
        drop(n);
        let socket = std::net::UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        socket
            .connect(dial.candidate.endpoint)
            .map_err(|e| e.to_string())?;
        let host = socket.local_addr().map_err(|e| e.to_string())?.ip();
        if !host.is_ipv4() {
            return Err("显示共享当前需要IPv4节点地址".into());
        }
        #[cfg(windows)]
        {
            let mode = selected.mode.as_ref().unwrap();
            let prepared = capyio_windows_service::DisplayServiceClient::default().prepare_peer(
                capyio_windows_service::DisplayPeerLaunchRequest {
                    session_id: link.session_id().to_string(),
                    route_id: id.to_string(),
                    stream_id: StreamId::new().to_string(),
                    stream_epoch: route.epoch,
                    sink_capability_id: route.sink.capability_id.to_string(),
                    host_ipv4: host.to_string(),
                    settings: capyio_windows_service::DisplaySessionSettings {
                        width: mode.width,
                        height: mode.height,
                        refresh_millihertz: mode.refresh_millihertz,
                        bitrate_kbps: selected.bitrate_kbps,
                    },
                    expires_at_ms: self.expires,
                },
            )?;
            self.generation = Some(prepared.generation);
            let mut launch = self.message(Action::Launch);
            launch.epoch = route.epoch;
            launch.selection = Some(selected);
            launch.launch_record = prepared.launch_record;
            link.send_display_session_control(launch)
                .map_err(|e| e.to_string())?;
        }
        self.renew = Some(Instant::now() + Duration::from_secs(60));
        self.state("starting", None);
        Ok(())
    }
    pub fn stop(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        notify: bool,
    ) -> Result<(), String> {
        let Some(id) = self.route else { return Ok(()) };
        if self.stopping {
            return Ok(());
        }
        self.stopping = true;
        self.stop_deadline = Some(Instant::now() + Duration::from_secs(50));
        self.poll_after = None;
        self.state("stopping", self.failure.clone());
        let mut result = Ok(());
        #[cfg(windows)]
        if let Some(generation) = self.generation {
            result = capyio_windows_service::DisplayServiceClient::default()
                .stop_peer(generation)
                .map(|_| ());
            if let Err(error) = &result {
                self.failure = Some(format!("停止请求未获确认：{error}；将继续检查后台恢复状态"));
                self.state("stopping", self.failure.clone());
            }
        }
        if notify {
            let _ = link.send_display_session_control(self.message(Action::Stop));
        }
        let _ = controller_control::requests()
            .lock()
            .map(|mut q| q.remove(&id));
        if let Ok(mut n) = node.lock()
            && let Ok(route) = n.runtime.route(id).cloned()
        {
            let _ = n
                .runtime
                .release_route_port_lease(id, route.source, link.session_id());
            if route.state == RouteState::Draft {
                if route.authorization == AuthorizationState::Pending {
                    let _ = n.runtime.deny_pending_route(id);
                } else {
                    let _ = n.runtime.prepare_route(
                        id,
                        Some(format()),
                        QosMode::Interactive,
                        unix_now_ms()?,
                    );
                }
            }
            let _ = n.runtime.set_route_active(id, false, unix_now_ms()?);
        }
        if self.generation.is_none() {
            self.finish();
        }
        result
    }
    fn finish(&mut self) {
        if let Ok(mut v) = view().lock()
            && v.as_ref()
                .is_some_and(|v| Some(&v.operation_id) == self.operation.as_ref())
        {
            if let Some(error) = self.failure.take() {
                if let Some(v) = v.as_mut() {
                    v.status = "failed".into();
                    v.error = Some(error);
                }
            } else {
                *v = None;
            }
        }
        *self = Self::default();
    }
    fn finish_expired_stop(&mut self, now: Instant) -> bool {
        if self.stopping && self.stop_deadline.is_some_and(|deadline| now >= deadline) {
            self.failure = Some("后台停止或桌面恢复尚未确认。请检查显示服务状态；连接恢复后可以重新申请，不会自动重启共享。".into());
            self.finish();
            return true;
        }
        false
    }
    pub fn shutdown(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        dial: &ResolvedControlDial,
    ) {
        let _ = self.stop(link, node, true);
        let deadline = Instant::now() + Duration::from_secs(50);
        while self.route.is_some() && Instant::now() < deadline {
            if self.advance(link, node, dial).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if self.route.is_some() {
            self.failure = Some("屏幕连接已断开；后台停止尚未确认，请检查屏幕服务状态".into());
            self.finish();
        }
        let _ = mailbox().lock().map(|mut q| q.remove(&link.session_id()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unreachable_service_stop_eventually_releases_old_coordinator() {
        let now = Instant::now();
        let mut coordinator = Coordinator {
            operation: Some(RouteId::new().to_string()),
            route: Some(RouteId::new()),
            generation: Some(12),
            stopping: true,
            stop_deadline: Some(now),
            ..Default::default()
        };
        assert!(coordinator.finish_expired_stop(now));
        assert!(coordinator.route.is_none());
        assert!(coordinator.operation.is_none());
        assert!(coordinator.generation.is_none());
        assert!(!coordinator.started);
    }
    #[test]
    fn stopping_keeps_ownership_while_recovery_is_still_within_bound() {
        let now = Instant::now();
        let route = RouteId::new();
        let mut coordinator = Coordinator {
            route: Some(route),
            generation: Some(12),
            stopping: true,
            stop_deadline: Some(now + Duration::from_secs(50)),
            ..Default::default()
        };
        assert!(!coordinator.finish_expired_stop(now));
        assert!(coordinator.owns(route));
        assert_eq!(coordinator.generation, Some(12));
    }
    #[test]
    fn automatic_native_aspect_ratio_is_bounded_and_exact() {
        let chosen = DisplayStreamSelection {
            mode: Some(DisplayStreamMode {
                width: 2560,
                height: 1600,
                refresh_millihertz: 60_000,
            }),
            bitrate_kbps: 24_000,
        };
        let caps = automatic_selection_policy(&chosen).unwrap();
        assert_eq!(
            capyio_protocol::negotiate_display_settings(&caps, &caps, &chosen).unwrap(),
            chosen
        );
        assert!(capyio_protocol::negotiate_display_settings(&policy(), &caps, &chosen).is_err());
        for (width, height, fps, bitrate) in [
            (2562, 1600, 60_000, 24_000),
            (1602, 2560, 60_000, 24_000),
            (2559, 1600, 60_000, 24_000),
            (1920, 1080, 120_000, 24_000),
            (1920, 1080, 60_000, 40_001),
        ] {
            assert!(
                automatic_selection_policy(&DisplayStreamSelection {
                    mode: Some(DisplayStreamMode {
                        width,
                        height,
                        refresh_millihertz: fps
                    }),
                    bitrate_kbps: bitrate
                })
                .is_err()
            );
        }
    }
    #[test]
    fn every_visible_quality_choice_negotiates_without_substitution() {
        let caps = policy();
        assert_eq!(caps.modes.len(), 6);
        for mode in &caps.modes {
            for bitrate_kbps in [2_000, 15_000, 50_000] {
                let selected = DisplayStreamSelection {
                    mode: Some(*mode),
                    bitrate_kbps,
                };
                assert_eq!(
                    capyio_protocol::negotiate_display_settings(&caps, &caps, &selected).unwrap(),
                    selected
                );
            }
        }
        let unsupported = DisplayStreamSelection {
            mode: Some(DisplayStreamMode {
                width: 3840,
                height: 2160,
                refresh_millihertz: 120_000,
            }),
            bitrate_kbps: 15_000,
        };
        assert!(capyio_protocol::negotiate_display_settings(&caps, &caps, &unsupported).is_err());
    }
    #[test]
    fn catalog_is_explicit_source_and_requires_consent() {
        let mut node = DesktopNodeState::demo().unwrap();
        install_catalog(&mut node.runtime).unwrap();
        install_catalog(&mut node.runtime).unwrap();
        let snapshot = node.runtime.snapshot();
        let (cap, port) = source();
        let capability = &snapshot.local_node.capabilities[&cap];
        assert_eq!(capability.class, CapabilityClass::Display);
        assert_eq!(
            capability.permission_requirement,
            PermissionRequirement::UserConfirmation
        );
        let port = &capability.ports[&port];
        assert_eq!(port.direction, PortDirection::Source);
        assert_eq!(port.formats, vec![format()]);
    }
    #[test]
    fn launch_control_does_not_carry_unrelated_keys_or_media() {
        let coordinator = Coordinator {
            operation: Some(RouteId::new().to_string()),
            route: Some(RouteId::new()),
            ..Default::default()
        };
        let stop = coordinator.message(Action::Stop);
        assert!(stop.launch_record.is_empty());
        assert!(stop.acknowledgement.is_empty());
        assert!(stop.selection.is_none());
        capyio_protocol::validate_display_session_control(&stop).unwrap();
    }
}
