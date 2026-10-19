use super::route_readiness::{FrameProgress, MediaRequirement, RouteMediaSnapshot, RouteReadiness};
use super::*;

use capyio_audio::{AudioMediaStreamBinding, AudioStreamSpec};
use capyio_core::*;
use capyio_protocol::{
    NativeAudioDataOffer, NativeAudioEndpoint, NativeAudioPreset, ValidatedRouteStatus,
};
use capyio_windows_service::{
    AudioRouteDirection as Direction, AudioRouteOperation, AudioRouteServiceClient, AudioRouteStart,
};
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};

pub(crate) fn endpoint(direction: Direction) -> (CapabilityId, PortId) {
    let (cap, port) = match direction {
        Direction::Microphone => ("21", "31"),
        Direction::Speaker => ("22", "32"),
    };
    (
        format!("d41251b3-63ed-4e83-807e-4c4ab9b20d{cap}")
            .parse()
            .unwrap(),
        format!("d41251b3-63ed-4e83-807e-4c4ab9b20d{port}")
            .parse()
            .unwrap(),
    )
}
fn preset(direction: Direction) -> NativeAudioPreset {
    match direction {
        Direction::Microphone => NativeAudioPreset::PcmS16le48000Mono,
        Direction::Speaker => NativeAudioPreset::PcmS16le48000Stereo,
    }
}
fn format(direction: Direction) -> FormatDescriptor {
    FormatDescriptor::new(match direction {
        Direction::Microphone => "pcm-s16le-48000-mono",
        Direction::Speaker => "pcm-s16le-48000-stereo",
    })
}
fn local_port(route: &Route, direction: Direction) -> PortRef {
    match direction {
        Direction::Microphone => route.sink,
        Direction::Speaker => route.source,
    }
}
fn remote_port(route: &Route, direction: Direction) -> PortRef {
    match direction {
        Direction::Microphone => route.source,
        Direction::Speaker => route.sink,
    }
}
fn direction(route: &Route) -> Option<Direction> {
    [Direction::Microphone, Direction::Speaker]
        .into_iter()
        .find(|direction| {
            let local = local_port(route, *direction);
            (local.capability_id, local.port_id) == endpoint(*direction)
        })
}

pub(crate) fn install_catalog(runtime: &mut capyio_runtime::NodeRuntime) -> Result<(), String> {
    let adapter: AdapterInstanceId = "d41251b3-63ed-4e83-807e-4c4ab9b20d01".parse().unwrap();
    if runtime
        .snapshot()
        .local_node
        .adapter_instances
        .contains_key(&adapter)
    {
        return Ok(());
    }
    let caps = [Direction::Microphone, Direction::Speaker]
        .into_iter()
        .map(|direction| {
            let (id, port) = endpoint(direction);
            let (name, port_direction, class) = match direction {
                Direction::Microphone => (
                    "Windows 系统麦克风接收",
                    PortDirection::Sink,
                    CapabilityClass::Microphone,
                ),
                Direction::Speaker => (
                    "Windows 系统扬声器发送",
                    PortDirection::Source,
                    CapabilityClass::Speaker,
                ),
            };
            CapabilityDescriptor {
                id,
                adapter_instance_id: adapter,
                display_name: name.into(),
                class,
                availability: Availability::Available,
                permission_requirement: PermissionRequirement::UserConfirmation,
                metadata: BTreeMap::from([("transport".into(), "native_udp_insecure_lab".into())]),
                ports: BTreeMap::from([(
                    port,
                    PortDescriptor {
                        id: port,
                        capability_id: id,
                        display_name: name.into(),
                        direction: port_direction,
                        profile: ProfileId::audio_frames_v1(),
                        schema_id: None,
                        formats: vec![format(direction)],
                        qos_modes: BTreeSet::from([QosMode::Interactive]),
                        clock_domain: None,
                        availability: Availability::Available,
                        permission_requirement: PermissionRequirement::UserConfirmation,
                        interoperability_mode: InteroperabilityMode::AdapterManaged,
                    },
                )]),
            }
        })
        .collect();
    runtime
        .register_adapter_catalog(
            runtime.snapshot().local_node.id,
            AdapterInstanceDescriptor {
                id: adapter,
                adapter_type: "dev.capyio.windows.native-audio-route".into(),
                display_name: "Native audio controlled-lab service".into(),
                deployment_mode: AdapterDeploymentMode::ExternalService,
                version: env!("CARGO_PKG_VERSION").into(),
                state: AdapterState::Ready,
                health: AdapterHealth::Unknown,
                owned_capabilities: BTreeSet::new(),
                supported_route_modes: BTreeSet::from([RouteBackend::AdapterManaged]),
            },
            caps,
        )
        .map_err(|e| e.to_string())
}
fn lab_endpoints(dial: &ResolvedControlDial) -> Result<(Ipv4Addr, Ipv4Addr), String> {
    super::media_lab_endpoints::resolve(
        dial.candidate.endpoint,
        "原生音频 UDP 尚未加密；当前只获准在指定 Windows/Y700 Tailscale 实验节点使用",
    )
}
struct Operation {
    initiated: bool,
    direction: Direction,
    remote_capability: CapabilityId,
    deadline: Instant,
    offer: Option<NativeAudioDataOffer>,
    remote_offer: Option<ValidatedRouteStatus>,
    launch: Option<AudioRouteStart>,
    renew: Instant,
    lease_renew: Instant,
    capture_baseline: Option<u64>,
    observation: Option<RouteMediaSnapshot>,
}
static INBOX: super::route_status_inbox::RouteStatusInbox =
    super::route_status_inbox::RouteStatusInbox::new();
pub(super) fn defer(status: &ValidatedRouteStatus) -> bool {
    direction(&status.route).is_some() && INBOX.defer(status)
}
#[derive(Default)]
pub(super) struct Coordinator {
    routes: BTreeMap<RouteId, Operation>,
}
impl Coordinator {
    pub fn observations(&self) -> impl Iterator<Item = RouteMediaSnapshot> + '_ {
        self.routes.values().filter_map(|op| op.observation)
    }

    pub fn owns(&self, id: RouteId) -> bool {
        self.routes.contains_key(&id)
    }
    pub fn handles(
        &self,
        node: &Arc<Mutex<DesktopNodeState>>,
        peer: NodeId,
        cap: CapabilityId,
    ) -> bool {
        node.lock().ok().is_some_and(|node| {
            node.runtime
                .snapshot()
                .peers
                .iter()
                .filter(|p| p.id == peer)
                .filter_map(|p| p.capabilities.get(&cap))
                .any(|c| {
                    matches!(
                        remote_resource_contract::kind(c),
                        Some(
                            remote_resource_contract::Kind::Microphone
                                | remote_resource_contract::Kind::Speaker
                        )
                    )
                })
        })
    }
    pub fn begin(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        dial: &ResolvedControlDial,
        selected: &Arc<Mutex<BTreeSet<CapabilityId>>>,
        cap: CapabilityId,
    ) -> Result<(), String> {
        lab_endpoints(dial)?;
        if self.routes.values().any(|op| op.remote_capability == cap) {
            return Ok(());
        }
        let mut node = node.lock().map_err(|_| "Node lock poisoned")?;
        install_catalog(&mut node.runtime)?;
        let snapshot = node.runtime.snapshot();
        let remote = snapshot
            .peers
            .iter()
            .find(|p| p.id == link.peer_node_id())
            .and_then(|p| p.capabilities.get(&cap))
            .ok_or("远端音频资源已离线")?;
        let matches: Vec<_> = remote
            .ports
            .values()
            .filter_map(|port| {
                let direction = match port.direction {
                    PortDirection::Source => Direction::Microphone,
                    PortDirection::Sink => Direction::Speaker,
                    _ => return None,
                };
                (port.profile == ProfileId::audio_frames_v1()
                    && port.interoperability_mode == InteroperabilityMode::AdapterManaged
                    && port.formats.contains(&format(direction)))
                .then_some((direction, port.id))
            })
            .collect();
        if matches.len() != 1 {
            return Err("远端尚未公布唯一兼容的原生音频端口，请更新两端".into());
        }
        let (direction, port) = matches[0];
        let remote = PortRef {
            node_id: link.peer_node_id(),
            capability_id: cap,
            port_id: port,
        };
        let (capability_id, port_id) = endpoint(direction);
        let local = PortRef {
            node_id: snapshot.local_node.id,
            capability_id,
            port_id,
        };
        if direction == Direction::Speaker {
            let mut selected = selected.lock().map_err(|_| "publication lock poisoned")?;
            selected.insert(capability_id);
            let publication = link
                .prepare_catalog(&node.runtime, &selected)
                .map_err(|e| e.to_string())?;
            link.send_catalog(publication).map_err(|e| e.to_string())?;
        }
        let (source, sink) = match direction {
            Direction::Microphone => (remote, local),
            Direction::Speaker => (local, remote),
        };
        let id = node
            .runtime
            .create_route(
                link.session_id(),
                source,
                sink,
                RouteBackend::AdapterManaged,
            )
            .map_err(|e| e.to_string())?;
        let route = node.runtime.route(id).map_err(|e| e.to_string())?.clone();
        self.insert(&route, direction);
        self.routes.get_mut(&id).unwrap().initiated = true;
        drop(node);
        link.send_route_status(&route).map_err(|e| e.to_string())
    }
    fn insert(&mut self, route: &Route, direction: Direction) {
        self.routes.insert(
            route.id,
            Operation {
                initiated: false,
                direction,
                remote_capability: remote_port(route, direction).capability_id,
                deadline: Instant::now() + Duration::from_secs(60),
                offer: None,
                remote_offer: None,
                launch: None,
                renew: Instant::now(),
                lease_renew: Instant::now() + Duration::from_secs(60),
                capture_baseline: None,
                observation: None,
            },
        );
    }
    pub fn receive(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        dial: &ResolvedControlDial,
        status: &ValidatedRouteStatus,
    ) -> Result<bool, String> {
        let Some(direction) = direction(&status.route) else {
            return Ok(false);
        };
        lab_endpoints(dial)?;
        let mut state = node.lock().map_err(|_| "Node lock poisoned")?;
        if local_port(&status.route, direction).node_id != state.runtime.snapshot().local_node.id
            || remote_port(&status.route, direction).node_id != link.peer_node_id()
        {
            return Err("音频节点绑定不匹配".into());
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
        if !incoming
            && self
                .routes
                .get(&status.route.id)
                .is_some_and(|op| op.initiated)
            && state
                .runtime
                .route(status.route.id)
                .is_ok_and(|route| route.authorization == AuthorizationState::Pending)
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
        if incoming {
            self.insert(&status.route, direction);
            let local_cap = local_port(&status.route, direction).capability_id;
            if controller_permission::allows(dial, local_cap) {
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
                            peer_name: "已认证音频节点".into(),
                            capability_name: "原生音频（受控实验 UDP，未加密）".into(),
                            resource_kind: match direction {
                                Direction::Microphone => "microphone",
                                Direction::Speaker => "speaker",
                            },
                            capability_id: local_cap,
                            expires_at_ms: unix_now_ms()? + 60_000,
                        },
                    );
            }
            link.send_route_status(
                state
                    .runtime
                    .route(status.route.id)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        }
        if status.native_audio_data.is_some() {
            self.routes.get_mut(&status.route.id).unwrap().remote_offer = Some(status.clone());
        } else if let Some(previous) = self
            .routes
            .get_mut(&status.route.id)
            .and_then(|op| op.remote_offer.as_mut())
        {
            // Draft consent acknowledgements describe the peer's not-yet-started
            // local lifecycle (epoch 0), not the negotiated media offer. They can
            // arrive after the reciprocal offer; never replace its epoch/format.
            // Actual media readiness still updates the snapshot and is checked
            // against the exact binding before starting/renewing a worker.
            if matches!(
                status.route.state,
                RouteState::Starting | RouteState::Active
            ) {
                previous.route = status.route.clone();
            }
        }
        drop(state);
        if matches!(
            status.route.state,
            RouteState::Stopped | RouteState::Failed | RouteState::Offline
        ) || matches!(
            status.route.authorization,
            AuthorizationState::Denied | AuthorizationState::Revoked
        ) {
            self.stop_id(link, node, status.route.id, false)?;
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
        let request = controller_control::requests()
            .lock()
            .map_err(|_| "request lock poisoned")?
            .remove(&id)
            .ok_or("请求已结束")?;
        let now = unix_now_ms()?;
        let mut node = node.lock().map_err(|_| "Node lock poisoned")?;
        if allow && now < request.expires_at_ms {
            if remember {
                controller_permission::remember(dial, request.capability_id)?;
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
        dial: &ResolvedControlDial,
    ) {
        let queued = INBOX.drain(link.session_id());
        for status in queued {
            if let Err(error) = self.receive(link, node, dial, &status) {
                eprintln!("audio status rejected: {error}");
                self.fail(link, node, status.route.id, error);
            }
        }
        for id in self.routes.keys().copied().collect::<Vec<_>>() {
            if let Err(error) = self.tick_one(link, node, dial, id) {
                eprintln!("native audio Route stopped: {error}");
                self.fail(link, node, id, error);
            }
        }
    }
    fn fail(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        id: RouteId,
        error: String,
    ) {
        let phase = self
            .routes
            .get(&id)
            .map(|op| {
                if op.launch.is_some() {
                    "service/start/media"
                } else if op.offer.is_some() {
                    "reciprocal-offer/worker-validation"
                } else {
                    "authorization/lease/offer"
                }
            })
            .unwrap_or("remote-status");
        let cleanup = self.stop_id(link, node, id, true).err();
        if let Ok(mut node) = node.lock() {
            let detail = format!(
                "phase={phase}; {error}{}",
                cleanup
                    .map(|e| format!("; cleanup={e}"))
                    .unwrap_or_default()
            );
            let _ = node.runtime.report_route_offline(
                id,
                Problem {
                    id: ProblemId::new(),
                    code: "CAPY.AUDIO_ROUTE.FAILED".into(),
                    category: ProblemCategory::Transport,
                    severity: ProblemSeverity::Warning,
                    retryable: true,
                    related_node: None,
                    related_adapter: None,
                    related_route: Some(id),
                    human_message: "音频启动或运行失败；已清理本次共享，可重新申请".into(),
                    technical_detail: Some(detail.chars().take(1000).collect()),
                },
            );
        }
    }
    fn tick_one(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        dial: &ResolvedControlDial,
        id: RouteId,
    ) -> Result<(), String> {
        let (local_ip, peer_ip) = lab_endpoints(dial)?;
        let now = unix_now_ms()?;
        let op = self.routes.get_mut(&id).ok_or("audio owner missing")?;
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
            return Err("对端已拒绝或停止音频".into());
        }
        if route.authorization == AuthorizationState::Pending {
            op.deadline = Instant::now() + Duration::from_secs(60);
            return Ok(());
        }
        // A ready speaker may legitimately be silent until an application opens
        // the virtual endpoint. Keep Starting (not Active) while its finite
        // grant, leases and worker supervision remain valid.
        let waiting_for_playback = op.direction == Direction::Speaker && op.launch.is_some();
        if route.state != RouteState::Active
            && !waiting_for_playback
            && Instant::now() >= op.deadline
        {
            return Err(match op.direction {
                Direction::Microphone => "麦克风启动超时，未确认本机新媒体；请在 Windows 录音应用中选择 CapyIO 虚拟麦克风后重试",
                Direction::Speaker => "扬声器启动超时，未确认对端播放；请向 CapyIO 虚拟扬声器播放音频后重试",
            }.into());
        }
        let ttl = touchpad_lease::ttl(&route.authorization, now)?;
        let local = local_port(&route, op.direction);
        let remote = remote_port(&route, op.direction);
        let leases = super::route_port_leases::RoutePortLeases {
            route: id,
            local,
            remote,
        };
        if op.offer.is_none() {
            link.send_route_status(&route).map_err(|e| e.to_string())?;
            leases.acquire_remote(link, node, ttl, "音频端口被占用或授权拒绝")?;
            let mut state = node.lock().map_err(|_| "Node lock poisoned")?;
            leases.acquire_local(&mut state.runtime, now, ttl)?;
            if route.state == RouteState::Draft {
                state
                    .runtime
                    .prepare_route(id, Some(format(op.direction)), QosMode::Interactive, now)
                    .map_err(|e| e.to_string())?;
            }
            state
                .runtime
                .begin_route_start(id, now)
                .map_err(|e| e.to_string())?;
            route = state.runtime.route(id).map_err(|e| e.to_string())?.clone();
            drop(state);
            let socket =
                UdpSocket::bind(SocketAddrV4::new(local_ip, 0)).map_err(|e| e.to_string())?;
            let offer = NativeAudioDataOffer {
                port: socket.local_addr().map_err(|e| e.to_string())?.port(),
                stream_id: StreamId::new(),
                endpoint: match op.direction {
                    Direction::Microphone => NativeAudioEndpoint::Sink,
                    Direction::Speaker => NativeAudioEndpoint::Source,
                },
                preset: preset(op.direction),
            };
            drop(socket);
            op.offer = Some(offer);
            link.send_native_audio_data_offer(&route, offer, now)
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        if op.launch.is_none() {
            let Some(remote_offer) = &op.remote_offer else {
                return Ok(());
            };
            let state = node.lock().map_err(|_| "Node lock poisoned")?;
            let prepared = link
                .prepare_native_audio_worker(
                    &state.runtime,
                    id,
                    op.offer.unwrap(),
                    remote_offer,
                    true,
                    now,
                )
                .map_err(|e| e.to_string())?;
            let launch = AudioRouteStart {
                direction: op.direction,
                route: state.runtime.route(id).map_err(|e| e.to_string())?.clone(),
                session: state
                    .runtime
                    .session(link.session_id())
                    .map_err(|e| e.to_string())?
                    .clone(),
                media: AudioMediaStreamBinding {
                    session_id: prepared.session_id,
                    route_id: id,
                    stream_id: prepared.stream_id,
                    stream_epoch: prepared.epoch,
                    selected_spec: match op.direction {
                        Direction::Microphone => AudioStreamSpec::voice_interactive(),
                        Direction::Speaker => AudioStreamSpec::media_balanced(),
                    },
                },
                local: SocketAddrV4::new(local_ip, prepared.local_port),
                peer: SocketAddrV4::new(peer_ip, prepared.peer_port),
            };
            drop(state);
            op.launch = Some(launch.clone());
            let ready = AudioRouteServiceClient::default()
                .invoke(AudioRouteOperation::Start(Box::new(launch)))?;
            if !ready.worker_ready {
                return Err("Windows 音频服务未就绪".into());
            }
            if op.direction == Direction::Microphone {
                // The Route-owned worker has now attached the same capture ring
                // observed by this read-only service status. Snapshot after ready,
                // before Android Start, rather than retaining a missing pre-start
                // observation forever and timing out despite successful capture.
                op.capture_baseline = Some(
                    capyio_windows_service::BrokerServiceClient::default()
                        .native_microphone_status()?
                        .produced_frames
                        .ok_or("麦克风捕获环计数不可读，无法确认实际媒体")?,
                );
            }
            op.renew = Instant::now() + Duration::from_secs(5);
            link.send_route_command(id, RouteCommandAction::Start)
                .map_err(|e| e.to_string())?;
        }
        if route.state == RouteState::Starting
            && op
                .remote_offer
                .as_ref()
                .is_some_and(|status| status.route.state == RouteState::Active)
        {
            let local_frames = if op.direction == Direction::Microphone {
                op.capture_baseline.and_then(|baseline| {
                    capyio_windows_service::BrokerServiceClient::default()
                        .native_microphone_status()
                        .ok()
                        .and_then(|status| status.produced_frames)
                        .map(|observed| FrameProgress { baseline, observed })
                })
            } else {
                None
            };
            let readiness = RouteReadiness {
                worker_ready: op.launch.is_some(),
                peer_media_observed: true, // Validated Active status in the condition above.
                local_frames,
            };
            op.observation = Some(RouteMediaSnapshot {
                session_id: route.session_id,
                route_id: id,
                epoch: route.epoch,
                sampled_at_ms: now,
                readiness,
                peer_frames: None,
            });
            let requirement = match op.direction {
                Direction::Speaker => MediaRequirement::PeerMedia,
                Direction::Microphone => MediaRequirement::PeerAndLocalFrames,
            };
            if readiness.media_ready(requirement) {
                node.lock()
                    .map_err(|_| "Node lock poisoned")?
                    .runtime
                    .activate_route(id)
                    .map_err(|e| e.to_string())?;
            }
        }
        if Instant::now() >= op.lease_renew {
            leases.renew(link, node, now, ttl, "音频远端租约已失效")?;
            op.lease_renew = Instant::now() + Duration::from_secs(60);
        }
        if Instant::now() >= op.renew {
            let state = node.lock().map_err(|_| "Node lock poisoned")?;
            let launch = op.launch.as_ref().unwrap();
            launch
                .media
                .validate_route_context(
                    state.runtime.route(id).map_err(|e| e.to_string())?,
                    state
                        .runtime
                        .session(link.session_id())
                        .map_err(|e| e.to_string())?,
                    now,
                )
                .map_err(|e| e.to_string())?;
            if state
                .runtime
                .route_port_lease_claim(id, local)
                .is_none_or(|claim| claim.expires_at_ms <= now)
            {
                return Err("本机音频租约失效".into());
            }
            drop(state);
            if !AudioRouteServiceClient::default()
                .invoke(AudioRouteOperation::Start(Box::new(launch.clone())))?
                .worker_ready
            {
                return Err("Windows 音频进程已退出".into());
            }
            op.renew = Instant::now() + Duration::from_secs(5);
        }
        Ok(())
    }
    pub fn stop_capability(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        cap: CapabilityId,
    ) -> Result<bool, String> {
        let Some(id) = self
            .routes
            .iter()
            .find_map(|(id, op)| (op.remote_capability == cap).then_some(*id))
        else {
            return Ok(false);
        };
        self.stop_id(link, node, id, true)?;
        Ok(true)
    }
    pub fn stop_id(
        &mut self,
        link: &mut CatalogControlLink,
        node: &Arc<Mutex<DesktopNodeState>>,
        id: RouteId,
        notify: bool,
    ) -> Result<(), String> {
        let Some(op) = self.routes.remove(&id) else {
            return Ok(());
        };
        controller_control::requests()
            .lock()
            .map_err(|_| "request lock poisoned")?
            .remove(&id);
        let stopped = op
            .launch
            .map(|launch| {
                AudioRouteServiceClient::default().invoke(AudioRouteOperation::Stop(launch.key()))
            })
            .transpose();
        let mut node = node.lock().map_err(|_| "Node lock poisoned")?;
        if let Ok(route) = node.runtime.route(id).cloned() {
            let _ = node.runtime.release_route_port_lease(
                id,
                local_port(&route, op.direction),
                route.session_id,
            );
            if route.state == RouteState::Draft
                && route.authorization == AuthorizationState::Pending
            {
                let _ = node.runtime.deny_pending_route(id);
            } else {
                if route.state == RouteState::Draft {
                    // Closing an authorized proposal must not leave it looking
                    // restartable; prepare/stop does not open hardware.
                    let _ = node.runtime.prepare_route(
                        id,
                        Some(format(op.direction)),
                        QosMode::Interactive,
                        unix_now_ms()?,
                    );
                }
                let _ = node.runtime.set_route_active(id, false, unix_now_ms()?);
            }
        }
        if notify {
            // The peer may only have prepared its socket and still be Draft;
            // Stop commands are invalid there. Terminal status closes both
            // pre-start preparation and an already running audio owner.
            if let Ok(route) = node.runtime.route(id) {
                let _ = link.send_route_status(route);
            }
        }
        stopped.map(|_| ())
    }
    pub fn shutdown(&mut self, link: &mut CatalogControlLink, node: &Arc<Mutex<DesktopNodeState>>) {
        for id in self.routes.keys().copied().collect::<Vec<_>>() {
            let _ = self.stop_id(link, node, id, true);
        }
        INBOX.remove_session(link.session_id());
    }
}

/// Do not let the generic control-only path report a usable audio Route before
/// the product owns its negotiated worker and media-readiness evidence.
pub(crate) fn require_product_data_path(profile: &ProfileId) -> Result<(), String> {
    if *profile == ProfileId::audio_frames_v1() {
        return Err(
            "音频必须经过专用 Route 协调器及有限授权，不能走仅控制层的兼容启动入口。".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_cannot_fall_through_to_control_only_success() {
        assert!(require_product_data_path(&ProfileId::audio_frames_v1()).is_err());
        assert!(require_product_data_path(&ProfileId::touchpad_frames_v1()).is_ok());
    }

    #[test]
    fn native_service_catalog_has_two_real_directional_contracts() {
        let mut runtime = capyio_runtime::NodeRuntime::new(capyio_testkit::windows_node()).unwrap();
        install_catalog(&mut runtime).unwrap();
        install_catalog(&mut runtime).unwrap();
        let snapshot = runtime.snapshot();
        for direction in [Direction::Microphone, Direction::Speaker] {
            let (cap, port) = endpoint(direction);
            let capability = &snapshot.local_node.capabilities[&cap];
            let endpoint = &capability.ports[&port];
            assert_eq!(endpoint.profile, ProfileId::audio_frames_v1());
            assert_eq!(
                endpoint.interoperability_mode,
                InteroperabilityMode::AdapterManaged
            );
            assert_eq!(endpoint.formats, vec![format(direction)]);
            assert_eq!(
                endpoint.permission_requirement,
                PermissionRequirement::UserConfirmation
            );
            assert_eq!(
                endpoint.direction,
                match direction {
                    Direction::Microphone => PortDirection::Sink,
                    Direction::Speaker => PortDirection::Source,
                }
            );
            assert_eq!(capability.metadata["transport"], "native_udp_insecure_lab");
        }
        assert_ne!(
            endpoint(Direction::Microphone),
            endpoint(Direction::Speaker)
        );
    }

    #[test]
    fn opposite_directions_keep_independent_operation_owners() {
        let node = DesktopNodeState::demo().unwrap();
        let mut route = node
            .runtime
            .route(node.demo_routes.phone_microphone_to_windows)
            .unwrap()
            .clone();
        let first = route.id;
        let mut owner = Coordinator::default();
        owner.insert(&route, Direction::Microphone);
        route.id = RouteId::new();
        let second = route.id;
        owner.insert(&route, Direction::Speaker);
        assert_eq!(owner.routes.len(), 2);
        owner.routes.remove(&first);
        assert!(owner.owns(second));
        assert_eq!(owner.routes[&second].direction, Direction::Speaker);
        assert!(owner.routes[&second].launch.is_none());
    }
}
