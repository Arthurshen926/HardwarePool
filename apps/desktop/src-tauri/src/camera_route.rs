//! One real CAVC receiver/projection owner; protocol readiness is not a frame.
use super::route_readiness::{FrameProgress, MediaRequirement, RouteMediaSnapshot, RouteReadiness};
use super::*;

use capyio_core::*;
use capyio_protocol::{CameraDataOffer, ValidatedRouteStatus};
use std::net::Ipv4Addr;
type Receivers = Arc<Mutex<android_receivers::AndroidReceiverActions>>;
pub(super) fn sink() -> (CapabilityId, PortId) {
    (
        "d41251b3-63ed-4e83-807e-4c4ab9b20e21".parse().unwrap(),
        "d41251b3-63ed-4e83-807e-4c4ab9b20e31".parse().unwrap(),
    )
}
fn format() -> FormatDescriptor {
    FormatDescriptor::new("cavc-avc-access-unit-v1")
}
pub(super) fn install_catalog(runtime: &mut capyio_runtime::NodeRuntime) -> Result<(), String> {
    let adapter: AdapterInstanceId = "d41251b3-63ed-4e83-807e-4c4ab9b20e01".parse().unwrap();
    if runtime
        .snapshot()
        .local_node
        .adapter_instances
        .contains_key(&adapter)
    {
        return Ok(());
    }
    let (id, port) = sink();
    runtime
        .register_adapter_catalog(
            runtime.snapshot().local_node.id,
            AdapterInstanceDescriptor {
                id: adapter,
                adapter_type: "dev.capyio.windows.cavc-camera".into(),
                display_name: "Windows system camera projection".into(),
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
                display_name: "Windows 系统摄像头接收".into(),
                class: CapabilityClass::Camera,
                availability: Availability::Available,
                permission_requirement: PermissionRequirement::UserConfirmation,
                metadata: BTreeMap::from([("transport".into(), "cavc_trusted_lab_tcp".into())]),
                ports: BTreeMap::from([(
                    port,
                    PortDescriptor {
                        id: port,
                        capability_id: id,
                        display_name: "系统摄像头".into(),
                        direction: PortDirection::Sink,
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
fn addresses(dial: &ResolvedControlDial) -> Result<(Ipv4Addr, Ipv4Addr), String> {
    super::media_lab_endpoints::resolve(
        dial.candidate.endpoint,
        "相机 CAVC 尚未加密，只在已批准的 Windows/Y700 Tailscale 实验节点启用",
    )
}
#[derive(Default)]
pub(super) struct Coordinator {
    route: Option<RouteId>,
    initiated: bool,
    started: bool,
    observation: Option<RouteMediaSnapshot>,
    deadline: Option<Instant>,
    renew: Option<Instant>,
}
static INBOX: super::route_status_inbox::RouteStatusInbox =
    super::route_status_inbox::RouteStatusInbox::new();
pub(super) fn defer(status: &ValidatedRouteStatus) -> bool {
    (status.route.sink.capability_id, status.route.sink.port_id) == sink() && INBOX.defer(status)
}
impl Coordinator {
    pub fn observation(&self) -> Option<RouteMediaSnapshot> {
        self.observation
    }

    pub fn owns(&self, id: RouteId) -> bool {
        self.route == Some(id)
    }
    pub fn handles(
        &self,
        node: &Arc<Mutex<DesktopNodeState>>,
        peer: NodeId,
        cap: CapabilityId,
    ) -> bool {
        node.lock().ok().is_some_and(|state| {
            state
                .runtime
                .snapshot()
                .peers
                .iter()
                .filter(|p| p.id == peer)
                .filter_map(|p| p.capabilities.get(&cap))
                .any(|c| {
                    remote_resource_contract::kind(c)
                        == Some(remote_resource_contract::Kind::Camera)
                })
        })
    }
    pub fn begin(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        dial: &ResolvedControlDial,
        cap: CapabilityId,
    ) -> Result<(), String> {
        addresses(dial)?;
        if self.route.is_some() {
            return Err("请先停止当前相机 Route".into());
        }
        let mut node = node.lock().map_err(|_| "Node lock poisoned")?;
        install_catalog(&mut node.runtime)?;
        let snapshot = node.runtime.snapshot();
        let capability = snapshot
            .peers
            .iter()
            .find(|p| p.id == link.peer_node_id())
            .and_then(|p| p.capabilities.get(&cap))
            .ok_or("相机资源已离线")?;
        let sources: Vec<_> = capability
            .ports
            .values()
            .filter(|p| {
                p.direction == PortDirection::Source
                    && p.profile == ProfileId::video_frames_v1()
                    && p.interoperability_mode == InteroperabilityMode::AdapterManaged
                    && p.formats.contains(&format())
            })
            .collect();
        if sources.len() != 1 {
            return Err("需要唯一 CAVC 相机 Source".into());
        }
        let (capability_id, port_id) = sink();
        let id = node
            .runtime
            .create_route(
                link.session_id(),
                PortRef {
                    node_id: link.peer_node_id(),
                    capability_id: cap,
                    port_id: sources[0].id,
                },
                PortRef {
                    node_id: snapshot.local_node.id,
                    capability_id,
                    port_id,
                },
                RouteBackend::AdapterManaged,
            )
            .map_err(|e| e.to_string())?;
        self.route = Some(id);
        self.initiated = true;
        self.deadline = Some(Instant::now() + Duration::from_secs(60));
        link.send_route_status(node.runtime.route(id).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
    pub fn receive(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        receivers: &Receivers,
        dial: &ResolvedControlDial,
        status: &ValidatedRouteStatus,
    ) -> Result<bool, String> {
        if (status.route.sink.capability_id, status.route.sink.port_id) != sink() {
            return Ok(false);
        }
        addresses(dial)?;
        let mut state = node.lock().map_err(|_| "Node lock poisoned")?;
        if status.route.source.node_id != link.peer_node_id()
            || status.route.sink.node_id != state.runtime.snapshot().local_node.id
        {
            return Err("相机节点绑定不匹配".into());
        }
        let incoming = !self.owns(status.route.id);
        if incoming
            && (state.runtime.route(status.route.id).is_ok()
                || status.route.state != RouteState::Draft
                || status.route.authorization != AuthorizationState::Pending)
        {
            return Ok(true);
        }
        link.apply_route_status(&mut state.runtime, status, false)
            .map_err(|e| e.to_string())?;
        if incoming {
            if self.route.is_some() {
                state
                    .runtime
                    .deny_pending_route(status.route.id)
                    .map_err(|e| e.to_string())?;
                link.send_route_status(state.runtime.route(status.route.id).unwrap())
                    .map_err(|e| e.to_string())?;
                return Ok(true);
            }
            self.route = Some(status.route.id);
            self.deadline = Some(Instant::now() + Duration::from_secs(60));
            if controller_permission::allows(dial, sink().0) {
                state
                    .runtime
                    .authorize_route(status.route.id, Some(unix_now_ms()? + 3_600_000))
                    .map_err(|e| e.to_string())?;
            } else {
                controller_control::requests()
                    .lock()
                    .map_err(|_| "request lock poisoned")?
                    .insert(
                        status.route.id,
                        controller_control::ResourceRequest {
                            route_id: status.route.id,
                            peer_node_id: link.peer_node_id(),
                            peer_name: "已认证相机节点".into(),
                            capability_name: "系统摄像头（CAVC 受控实验，未加密）".into(),
                            resource_kind: "camera",
                            capability_id: sink().0,
                            expires_at_ms: unix_now_ms()? + 60_000,
                        },
                    );
            }
            link.send_route_status(state.runtime.route(status.route.id).unwrap())
                .map_err(|e| e.to_string())?;
        } else if self.initiated
            && state.runtime.route(status.route.id).unwrap().authorization
                == AuthorizationState::Pending
            && let AuthorizationState::Authorized {
                expires_at_ms: Some(expiry),
            } = status.route.authorization
            && expiry > unix_now_ms()?
        {
            state
                .runtime
                .authorize_route(status.route.id, Some(expiry))
                .map_err(|e| e.to_string())?;
        }
        drop(state);
        if matches!(
            status.route.state,
            RouteState::Stopped | RouteState::Failed | RouteState::Offline
        ) || matches!(
            status.route.authorization,
            AuthorizationState::Denied | AuthorizationState::Revoked
        ) {
            self.stop(link, node, receivers, false)?;
        }
        Ok(true)
    }
    pub fn resolve(
        &mut self,
        node: &Arc<Mutex<DesktopNodeState>>,
        dial: &ResolvedControlDial,
        id: RouteId,
        allow: bool,
        remember: bool,
    ) -> Result<(), String> {
        if !self.owns(id) || self.initiated {
            return Err("相机授权请求已结束".into());
        }
        let request = controller_control::requests()
            .lock()
            .map_err(|_| "request lock poisoned")?
            .remove(&id)
            .ok_or("相机请求已处理")?;
        let now = unix_now_ms()?;
        let mut node = node.lock().map_err(|_| "Node lock poisoned")?;
        if allow && request.expires_at_ms > now {
            if remember {
                controller_permission::remember(dial, sink().0)?;
            }
            node.runtime
                .authorize_route(id, Some(now + 3_600_000))
                .map_err(|e| e.to_string())
        } else {
            node.runtime
                .deny_pending_route(id)
                .map_err(|e| e.to_string())
        }
    }
    pub fn tick(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        receivers: &Receivers,
        dial: &ResolvedControlDial,
    ) {
        let queued = INBOX.drain(link.session_id());
        for status in queued {
            if let Err(error) = self.receive(link, node, receivers, dial, &status) {
                eprintln!("camera status failed: {error}");
                let _ = self.stop(link, node, receivers, true);
            }
        }
        if let Err(error) = self.advance(link, node, receivers, dial) {
            eprintln!("camera Route stopped: {error}");
            let _ = self.stop(link, node, receivers, true);
        }
    }
    fn advance(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        receivers: &Receivers,
        dial: &ResolvedControlDial,
    ) -> Result<(), String> {
        let Some(id) = self.route else {
            return Ok(());
        };
        let now = unix_now_ms()?;
        let mut route = node
            .lock()
            .map_err(|_| "Node lock poisoned")?
            .runtime
            .route(id)
            .map_err(|e| e.to_string())?
            .clone();
        if matches!(
            route.state,
            RouteState::Stopped | RouteState::Failed | RouteState::Offline
        ) || matches!(
            route.authorization,
            AuthorizationState::Denied | AuthorizationState::Revoked
        ) {
            return Err("相机已停止或授权已失效".into());
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err("相机实际画面就绪超时".into());
        }
        if route.authorization == AuthorizationState::Pending {
            return Ok(());
        }
        let ttl = touchpad_lease::ttl(&route.authorization, now)?;
        let leases = super::route_port_leases::RoutePortLeases {
            route: id,
            local: route.sink,
            remote: route.source,
        };
        if !self.started {
            let (bind, peer) = addresses(dial)?;
            link.send_route_status(&route).map_err(|e| e.to_string())?;
            leases.acquire_remote(link, node, ttl, "远端摄像头端口占用或授权被拒绝")?;
            let mut state = node.lock().map_err(|_| "Node lock poisoned")?;
            leases.acquire_local(&mut state.runtime, now, ttl)?;
            state
                .runtime
                .prepare_route(id, Some(format()), QosMode::Interactive, now)
                .map_err(|e| e.to_string())?;
            state
                .runtime
                .begin_route_start(id, now)
                .map_err(|e| e.to_string())?;
            route = state.runtime.route(id).unwrap().clone();
            let AuthorizationState::Authorized {
                expires_at_ms: Some(expiry),
            } = route.authorization
            else {
                return Err("相机需要有限授权".into());
            };
            let offer = CameraDataOffer {
                port: 38173,
                stream_id: StreamId::new(),
            };
            offer.validate(&route).map_err(|e| e.to_string())?;
            drop(state);
            receivers
                .lock()
                .map_err(|_| "camera receiver lock poisoned")?
                .start_camera_route(id, offer.stream_id, route.epoch, expiry, bind, peer)?;
            self.started = true;
            self.renew = Some(Instant::now() + Duration::from_secs(60));
            link.send_camera_data_offer(&route, offer, now)
                .map_err(|e| e.to_string())?;
            link.send_route_command(id, RouteCommandAction::Start)
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        let frames = receivers
            .lock()
            .map_err(|_| "camera receiver lock poisoned")?
            .camera_route_progress(id)?;
        let readiness = RouteReadiness {
            worker_ready: self.started,
            local_frames: Some(FrameProgress {
                baseline: 0,
                observed: frames,
            }),
            ..Default::default()
        };
        self.observation = Some(RouteMediaSnapshot {
            session_id: route.session_id,
            route_id: id,
            epoch: route.epoch,
            sampled_at_ms: now,
            readiness,
            peer_frames: None,
        });
        if readiness.media_ready(MediaRequirement::LocalFrames)
            && route.state == RouteState::Starting
        {
            let mut node = node.lock().map_err(|_| "Node lock poisoned")?;
            node.runtime.activate_route(id).map_err(|e| e.to_string())?;
            self.deadline = None;
        }
        let state = node.lock().map_err(|_| "Node lock poisoned")?;
        if !state
            .runtime
            .session(link.session_id())
            .is_ok_and(|s| s.state == SessionState::Ready)
            || state
                .runtime
                .route_port_lease_claim(id, route.sink)
                .is_none_or(|claim| claim.expires_at_ms <= now)
        {
            return Err("相机连接或本机租约失效".into());
        }
        drop(state);
        if self
            .renew
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            leases.renew(link, node, now, ttl, "相机远端租约失效")?;
            self.renew = Some(Instant::now() + Duration::from_secs(60));
        }
        Ok(())
    }
    pub fn stop_capability(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        receivers: &Receivers,
        cap: CapabilityId,
    ) -> Result<bool, String> {
        let matches = self.route.is_some_and(|id| {
            node.lock().ok().is_some_and(|state| {
                state
                    .runtime
                    .route(id)
                    .is_ok_and(|route| route.source.capability_id == cap)
            })
        });
        if !matches {
            return Ok(false);
        }
        self.stop(link, node, receivers, true)?;
        Ok(true)
    }
    pub fn stop(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        receivers: &Receivers,
        notify: bool,
    ) -> Result<(), String> {
        let Some(id) = self.route.take() else {
            return Ok(());
        };
        let local = receivers
            .lock()
            .map_err(|_| "camera receiver lock poisoned")?
            .stop_camera_route(id);
        let _ = controller_control::requests()
            .lock()
            .map(|mut requests| requests.remove(&id));
        if let Ok(mut state) = node.lock()
            && let Ok(route) = state.runtime.route(id).cloned()
        {
            let _ = state
                .runtime
                .release_route_port_lease(id, route.sink, route.session_id);
            if route.state == RouteState::Draft
                && route.authorization == AuthorizationState::Pending
            {
                let _ = state.runtime.deny_pending_route(id);
            } else {
                if route.state == RouteState::Draft {
                    let _ = state.runtime.prepare_route(
                        id,
                        Some(format()),
                        QosMode::Interactive,
                        unix_now_ms()?,
                    );
                }
                let _ = state.runtime.set_route_active(id, false, unix_now_ms()?);
            }
            if notify {
                // Before Start the Android sender remains Draft; terminal
                // status also cancels that preparation without an invalid
                // Stop command closing the authenticated control connection.
                if let Ok(route) = state.runtime.route(id) {
                    let _ = link.send_route_status(route);
                }
            }
        }
        INBOX.remove(link.session_id(), id);
        *self = Self::default();
        local
    }
}
