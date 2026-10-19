//! Process-owned Runtime and platform workers, independent of the Tauri builder.
//! Windows creates this only in --node-host; the UI uses the versioned local API.
use super::*;

mod connections;
mod display;
mod enrollment;
mod input;
pub(super) use enrollment::EnrollmentRequest;

pub(super) struct DesktopHost {
    enrollment: Arc<Mutex<capyio_enrollment::EnrollmentManager>>,
    node: Arc<Mutex<DesktopNodeState>>,
    control_plane: Arc<Mutex<DesktopControlPlaneState>>,
    control_links: Arc<Mutex<BTreeMap<capyio_core::NodeId, ControlLinkController>>>,
    paired_listener: Mutex<Option<PairedListenerController>>,
    discovery_controller: Mutex<Option<DiscoveryController>>,
    local_shared_capabilities: Arc<Mutex<BTreeSet<CapabilityId>>>,
    physical_imu: PhysicalImuRoute,
    live_imu: Arc<Mutex<UiLiveImu>>,
    live_imu_controller: Mutex<Option<LiveImuController>>,
    gamepad_lab: Arc<Mutex<GamepadLab>>,
    gamepad_worker: GamepadProjectionWorker,
    windows_gamepad_preflight_running: AtomicBool,
    audio_share: Arc<Mutex<AudioShareQuickAction>>,
    microphone: Arc<Mutex<MicrophoneQuickAction>>,
    audio_worker: AudioQuickActionWorker,
    android_receivers: Arc<Mutex<android_receivers::AndroidReceiverActions>>,
}

impl DesktopHost {
    pub fn media_observations(&self) -> Result<Vec<route_readiness::RouteMediaSnapshot>, String> {
        let links = self
            .control_links
            .lock()
            .map_err(|_| "control links lock poisoned")?;
        let commands: Vec<_> = links
            .values()
            .filter(|link| link.is_active())
            .map(|link| link.commands.clone())
            .collect();
        drop(links);
        if commands.len() > 16 {
            return Err("媒体诊断暂不可用：活动连接超过 16 个".into());
        }
        let mut observations = Vec::new();
        for command in commands {
            let (reply, receive) = mpsc::channel();
            command
                .try_send(ControlLinkCommand::ObserveMedia { reply })
                .map_err(|_| "媒体诊断暂不可用：控制连接正忙")?;
            observations.extend(
                receive
                    .recv_timeout(Duration::from_secs(1))
                    .map_err(|_| "媒体诊断暂不可用：采样超时")?,
            );
        }
        Ok(observations)
    }

    pub fn set_route(&self, route_id: RouteId, active: bool) -> Result<UiSnapshot, String> {
        if self
            .audio_share
            .lock()
            .map_err(|_| "Audio Share state lock poisoned")?
            .owns_route(route_id)
        {
            return Err(
                "use the versioned Quick Action to control the physical speaker Route".to_owned(),
            );
        }
        if self
            .microphone
            .lock()
            .map_err(|_| "MicYou state lock poisoned")?
            .owns_route(route_id)
        {
            return Err(
                "use the versioned Quick Action to control the physical microphone Route"
                    .to_owned(),
            );
        }
        let mut node = self.node.lock().map_err(|_| "Node state lock poisoned")?;
        node.set_demo_route_active(route_id, active, unix_time_ms()?)?;
        let selected = self
            .local_shared_capabilities
            .lock()
            .map_err(|_| "local Capability selection lock poisoned")?;
        let control_plane = self
            .control_plane
            .lock()
            .map_err(|_| "control-plane state lock poisoned")?;
        Ok(to_ui_snapshot(&node, &selected, &control_plane))
    }

    pub fn set_local_capability_shared(
        &self,
        capability_id: CapabilityId,
        is_selected: bool,
    ) -> Result<UiSnapshot, String> {
        let node = self.node.lock().map_err(|_| "Node state lock poisoned")?;
        let mut selected = self
            .local_shared_capabilities
            .lock()
            .map_err(|_| "local Capability selection lock poisoned")?;
        update_local_capability_selection(
            &node.runtime,
            &mut selected,
            capability_id,
            is_selected,
        )?;
        drop(selected);
        drop(node);
        if let Ok(links) = self.control_links.lock() {
            for controller in links.values().filter(|controller| controller.is_active()) {
                controller.publish_latest();
            }
        }
        self.snapshot()
    }

    pub fn set_remote_capability_use(
        &self,
        capability_id: CapabilityId,
        active: bool,
    ) -> Result<UiSnapshot, String> {
        let peer_id = {
            let node = self.node.lock().map_err(|_| "Node state lock poisoned")?;
            let snapshot = node.runtime.snapshot();
            let peer = snapshot
                .peers
                .iter()
                .find(|peer| peer.capabilities.contains_key(&capability_id))
                .ok_or("remote Capability is absent from the authenticated catalog")?;
            let capability = &peer.capabilities[&capability_id];
            if active && remote_resource_contract::kind(capability).is_none() {
                return Err(
                    "此资源尚未接入自动使用流程，请通过对应的实验功能入口配置；未启动硬件。".into(),
                );
            }
            peer.id
        };
        let links = self
            .control_links
            .lock()
            .map_err(|_| "control-link owner lock poisoned".to_owned())?;
        let controller = links
            .get(&peer_id)
            .filter(|controller| controller.is_active())
            .ok_or_else(|| {
                "remote Node has no active authenticated control connection".to_owned()
            })?;
        controller.set_remote_capability_use(capability_id, active)?;
        drop(links);
        self.snapshot()
    }

    pub fn get_quick_actions(&self) -> Result<Vec<UiQuickAction>, String> {
        let speaker = {
            let audio_share = self
                .audio_share
                .lock()
                .map_err(|_| "Audio Share state lock poisoned")?;
            let node = self.node.lock().map_err(|_| "Node state lock poisoned")?;
            audio_share.dto(&node.runtime)?
        };
        let microphone = {
            let microphone = self
                .microphone
                .lock()
                .map_err(|_| "MicYou state lock poisoned")?;
            let node = self.node.lock().map_err(|_| "Node state lock poisoned")?;
            microphone.dto(&node.runtime)?
        };
        let mut actions = vec![speaker, microphone];
        actions.extend(
            self.android_receivers
                .lock()
                .map_err(|_| "Android receiver state lock poisoned")?
                .actions(),
        );
        Ok(actions)
    }

    pub fn invoke_quick_action(
        &self,
        request: InvokeQuickActionRequest,
    ) -> Result<UiQuickAction, String> {
        if android_receivers::AndroidReceiverActions::owns(&request.action_id) {
            return self
                .android_receivers
                .lock()
                .map_err(|_| "Android receiver state lock poisoned".to_owned())?
                .invoke(&request.action_id, request.operation);
        }
        if request.action_id == MICYOU_ACTION_ID {
            let mut microphone = self
                .microphone
                .lock()
                .map_err(|_| "MicYou state lock poisoned")?;
            let mut node = self.node.lock().map_err(|_| "Node state lock poisoned")?;
            microphone.invoke(&mut node.runtime, request, unix_time_ms()?)
        } else {
            let mut audio_share = self
                .audio_share
                .lock()
                .map_err(|_| "Audio Share state lock poisoned")?;
            let mut node = self.node.lock().map_err(|_| "Node state lock poisoned")?;
            audio_share.invoke(&mut node.runtime, request, unix_time_ms()?)
        }
    }

    pub fn get_audio_endpoints(&self) -> Result<UiAudioEndpointCatalog, String> {
        let mut audio_share = self
            .audio_share
            .lock()
            .map_err(|_| "Audio Share state lock poisoned")?;
        let can_select = {
            let node = self.node.lock().map_err(|_| "Node state lock poisoned")?;
            audio_share.endpoint_selection_available(&node.runtime)
        };
        Ok(audio_share.endpoint_catalog(can_select))
    }

    pub fn select_audio_endpoint(
        &self,
        request: SelectAudioEndpointRequest,
    ) -> Result<UiQuickAction, String> {
        let mut audio_share = self
            .audio_share
            .lock()
            .map_err(|_| "Audio Share state lock poisoned")?;
        let node = self.node.lock().map_err(|_| "Node state lock poisoned")?;
        audio_share.select_endpoint(&node.runtime, request)
    }

    pub fn snapshot(&self) -> Result<UiSnapshot, String> {
        let node = self.node.lock().map_err(|_| "Node state lock poisoned")?;
        let selected = self
            .local_shared_capabilities
            .lock()
            .map_err(|_| "local Capability selection lock poisoned")?;
        let control_plane = self
            .control_plane
            .lock()
            .map_err(|_| "control-plane state lock poisoned")?;
        Ok(to_ui_snapshot(&node, &selected, &control_plane))
    }

    pub fn stop_paired_listener(&self) -> Result<(), String> {
        let current = self
            .paired_listener
            .lock()
            .map_err(|_| "paired listener owner lock poisoned".to_owned())?
            .take();
        if let Some(current) = current {
            current.stop_and_join();
        }
        Ok(())
    }

    pub fn stop_discovery(&self) -> Result<UiSnapshot, String> {
        let controller = self
            .discovery_controller
            .lock()
            .map_err(|_| "discovery controller lock poisoned")?
            .take();
        if let Some(controller) = controller {
            controller.stop.store(true, Ordering::Release);
            controller
                .worker
                .join()
                .map_err(|_| "discovery worker panicked".to_owned())?;
        }
        self.snapshot()
    }

    pub fn reset_demo(&self) -> Result<UiSnapshot, String> {
        if self
            .discovery_controller
            .lock()
            .map_err(|_| "discovery controller lock poisoned")?
            .is_some()
        {
            return Err("stop discovery before resetting the demo".to_owned());
        }
        if self
            .audio_share
            .lock()
            .map_err(|_| "Audio Share state lock poisoned")?
            .is_configured()
        {
            return Err(
                "restart the desktop host to reset a configured physical speaker Route".to_owned(),
            );
        }
        if self
            .microphone
            .lock()
            .map_err(|_| "MicYou state lock poisoned")?
            .is_configured()
        {
            return Err(
                "restart the desktop host to reset a configured physical microphone Route"
                    .to_owned(),
            );
        }
        if self
            .live_imu_controller
            .lock()
            .map_err(|_| "live IMU controller lock poisoned")?
            .is_some()
        {
            return Err("stop the physical IMU Route before resetting the demo".to_owned());
        }
        let mut node = self.node.lock().map_err(|_| "Node state lock poisoned")?;
        let local_node_id = node.runtime.snapshot().local_node.id;
        *node = DesktopNodeState::with_local_identity(local_node_id)?;
        let mut control_plane = DesktopControlPlaneState::new(local_node_id)?;
        control_plane.attach_default_pairing_store(local_node_id);
        *self
            .control_plane
            .lock()
            .map_err(|_| "control-plane state lock poisoned")? = control_plane;
        let session_id = node.bootstrap_session_id;
        PhysicalImuRoute::install(&mut node.runtime, session_id)?;
        *self
            .gamepad_lab
            .lock()
            .map_err(|_| "gamepad lab state lock poisoned".to_owned())? = GamepadLab::new()?;
        let mut selected = self
            .local_shared_capabilities
            .lock()
            .map_err(|_| "local Capability selection lock poisoned")?;
        selected.clear();
        let control_plane = self
            .control_plane
            .lock()
            .map_err(|_| "control-plane state lock poisoned")?;
        Ok(to_ui_snapshot(&node, &selected, &control_plane))
    }

    pub fn start_discovery(&self, interface_ipv4: &str) -> Result<UiSnapshot, String> {
        let interface = interface_ipv4
            .parse::<std::net::Ipv4Addr>()
            .map_err(|_| "discovery interface must be a canonical IPv4 address".to_owned())?;
        if !network_interfaces::enumerate()?
            .iter()
            .any(|item| item.ipv4 == interface_ipv4)
        {
            return Err(
                "selected interface is no longer available; refresh the interface list".into(),
            );
        }
        let mut controller = self
            .discovery_controller
            .lock()
            .map_err(|_| "discovery controller lock poisoned")?;
        if controller
            .as_ref()
            .is_some_and(|item| item.worker.is_finished())
            && let Some(finished) = controller.take()
        {
            let _ = finished.worker.join();
        }
        if controller.is_some() {
            return Err("discovery listener is already running".to_owned());
        }
        let socket = DiscoveryUdpSocket::bind(interface).map_err(|error| error.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker_control_plane = Arc::clone(&self.control_plane);
        {
            let mut control_plane = worker_control_plane
                .lock()
                .map_err(|_| "control-plane state lock poisoned")?;
            control_plane.discovery_listening = true;
        }
        let worker_result = thread::Builder::new()
            .name("capyio-discovery".to_owned())
            .spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    let received = socket.receive();
                    let Ok(mut control_plane) = worker_control_plane.lock() else {
                        break;
                    };
                    let now_ms = u64::try_from(control_plane.clock.elapsed().as_millis())
                        .unwrap_or(u64::MAX);
                    control_plane.discovery_ingress.expire(now_ms);
                    match received {
                        Ok(DiscoveryReceive::Idle) => {}
                        Ok(DiscoveryReceive::Datagram(datagram)) => {
                            let _ = control_plane.discovery_ingress.ingest(
                                datagram.source.ip(),
                                datagram.payload(),
                                now_ms,
                            );
                        }
                        Err(_) => break,
                    }
                }
                if let Ok(mut control_plane) = worker_control_plane.lock() {
                    control_plane.discovery_listening = false;
                }
            });
        let worker = match worker_result {
            Ok(worker) => worker,
            Err(error) => {
                if let Ok(mut control_plane) = self.control_plane.lock() {
                    control_plane.discovery_listening = false;
                }
                return Err(format!("failed to start discovery worker: {error}"));
            }
        };
        *controller = Some(DiscoveryController { stop, worker });
        drop(controller);
        self.snapshot()
    }

    pub fn disconnect_peer(&self, peer_id: NodeId) -> Result<(), String> {
        let mut links = self
            .control_links
            .lock()
            .map_err(|_| "control-link owner lock poisoned".to_owned())?;
        let controller = links
            .remove(&peer_id)
            .ok_or_else(|| "peer has no authenticated control connection".to_owned())?;
        drop(links);
        controller.stop_and_join()
    }

    pub fn start() -> Self {
        let mut node = DesktopNodeState::production().unwrap_or_else(|error| {
            eprintln!("CAPYIO_NODE_IDENTITY_STARTUP_FAILED: {error}");
            std::process::exit(2);
        });
        let session_id = node.bootstrap_session_id;
        let physical_imu = PhysicalImuRoute::install(&mut node.runtime, session_id)
            .expect("valid physical IMU Runtime catalog");
        let live_imu =
            UiLiveImu::idle(physical_imu, &node.runtime).expect("valid physical IMU Route");
        let audio_share = Arc::new(Mutex::new(AudioShareQuickAction::install(
            &mut node.runtime,
            session_id,
        )));
        let microphone = Arc::new(Mutex::new(MicrophoneQuickAction::install(
            &mut node.runtime,
            session_id,
        )));
        let windows_host = WindowsGamepadHost::install(&mut node.runtime, session_id)
            .expect("valid fixed Windows gamepad host configuration");
        let gamepad_lab = Arc::new(Mutex::new(
            GamepadLab::new_with_windows_host(windows_host).expect("valid gamepad simulator"),
        ));
        let local_node_id = node.runtime.snapshot().local_node.id;
        let mut desktop_control_plane = DesktopControlPlaneState::new(local_node_id)
            .expect("valid desktop control-plane owner");
        desktop_control_plane.attach_default_pairing_store(local_node_id);
        let control_plane = Arc::new(Mutex::new(desktop_control_plane));
        let node = Arc::new(Mutex::new(node));
        let audio_stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&audio_stop);
        let worker_audio_share = Arc::clone(&audio_share);
        let worker_microphone = Arc::clone(&microphone);
        let worker_node = Arc::clone(&node);
        let audio_worker = thread::spawn(move || {
            while !worker_stop.load(Ordering::Acquire) {
                if let (Ok(mut audio_share), Ok(mut node)) =
                    (worker_audio_share.lock(), worker_node.lock())
                {
                    audio_share.poll(&mut node.runtime);
                }
                if let (Ok(mut microphone), Ok(mut node)) =
                    (worker_microphone.lock(), worker_node.lock())
                {
                    microphone.poll(&mut node.runtime);
                }
                thread::sleep(Duration::from_millis(250));
            }
        });
        let gamepad_stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&gamepad_stop);
        let worker_gamepad = Arc::clone(&gamepad_lab);
        let worker_node = Arc::clone(&node);
        let gamepad_worker = thread::Builder::new()
            .name("capyio-windows-gamepad".to_owned())
            .spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    if let (Ok(mut node), Ok(mut gamepad)) =
                        (worker_node.lock(), worker_gamepad.lock())
                    {
                        gamepad.poll_windows_projection(&mut node.runtime);
                    }
                    thread::sleep(Duration::from_millis(4));
                }
            })
            .expect("start Windows gamepad projection worker");
        let enrollment = Arc::new(Mutex::new(capyio_enrollment::EnrollmentManager::default()));
        fileless_pairing::start_store_worker(
            Arc::downgrade(&enrollment),
            control_plane.clone(),
            local_node_id,
        );
        Self {
            enrollment,
            node,
            control_plane,
            control_links: Arc::new(Mutex::new(BTreeMap::new())),
            paired_listener: Mutex::new(None),
            discovery_controller: Mutex::new(None),
            local_shared_capabilities: Arc::new(Mutex::new(BTreeSet::new())),
            physical_imu,
            live_imu: Arc::new(Mutex::new(live_imu)),
            live_imu_controller: Mutex::new(None),
            gamepad_lab,
            gamepad_worker: GamepadProjectionWorker {
                stop: gamepad_stop,
                worker: Some(gamepad_worker),
            },
            windows_gamepad_preflight_running: AtomicBool::new(false),
            audio_share,
            microphone,
            audio_worker: AudioQuickActionWorker {
                stop: audio_stop,
                worker: Some(audio_worker),
            },
            android_receivers: Arc::new(Mutex::new(
                android_receivers::AndroidReceiverActions::new(),
            )),
        }
    }
}

impl Drop for DesktopHost {
    fn drop(&mut self) {
        if let Ok(listener) = self.paired_listener.get_mut()
            && let Some(listener) = listener.take()
        {
            listener.stop_and_join();
        }
        if let Ok(controller) = self.discovery_controller.get_mut()
            && let Some(controller) = controller.take()
        {
            controller.stop.store(true, Ordering::Release);
            let _ = controller.worker.join();
        }
        if let Ok(controller) = self.live_imu_controller.get_mut()
            && let Some(controller) = controller.take()
        {
            controller.stop.store(true, Ordering::Release);
            let _ = controller.worker.join();
        }
        self.audio_worker.stop.store(true, Ordering::Release);
        if let Some(worker) = self.audio_worker.worker.take() {
            let _ = worker.join();
        }
        self.gamepad_worker.stop.store(true, Ordering::Release);
        if let Some(worker) = self.gamepad_worker.worker.take() {
            let _ = worker.join();
        }
        if let Ok(mut links) = self.control_links.lock() {
            for (_, controller) in std::mem::take(&mut *links) {
                let _ = controller.stop_and_join();
            }
        }
        if let (Ok(mut node), Ok(mut gamepad)) = (self.node.lock(), self.gamepad_lab.lock()) {
            let _ = gamepad.stop_windows_projection(&mut node.runtime);
        }
        if let Ok(mut audio_share) = self.audio_share.lock()
            && let Ok(mut node) = self.node.lock()
        {
            audio_share.shutdown(&mut node.runtime);
        }
        if let Ok(mut microphone) = self.microphone.lock()
            && let Ok(mut node) = self.node.lock()
        {
            microphone.shutdown(&mut node.runtime);
        }
        if let Ok(mut receivers) = self.android_receivers.lock() {
            receivers.shutdown();
        }
    }
}
