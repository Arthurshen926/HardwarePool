//! Existing controller and IMU commands executed by the process host.
use super::*;

impl DesktopHost {
    pub fn get_controller_use(
        &self,
    ) -> Result<Option<controller_workflow::UiControllerUse>, String> {
        Ok(self
            .gamepad_lab
            .lock()
            .map_err(|_| "gamepad lock poisoned")?
            .workflow
            .snapshot())
    }

    pub fn start_controller_use(
        &self,
        peer_node_id: String,
        mode: controller_workflow::ControllerMode,
    ) -> Result<(), String> {
        let peer = NodeId::from_str(&peer_node_id).map_err(|e| e.to_string())?;
        let (reply, receive) = mpsc::channel();
        {
            let links = self
                .control_links
                .lock()
                .map_err(|_| "control links lock poisoned")?;
            let link = links
                .get(&peer)
                .filter(|link| link.active.load(Ordering::Acquire))
                .ok_or("请先连接已信任节点")?;
            link.commands
                .try_send(ControlLinkCommand::StartController { mode, reply })
                .map_err(|_| "控制连接正忙，请稍后重试")?;
        }
        receive
            .recv_timeout(Duration::from_secs(20))
            .map_err(|_| "手柄启动请求超时")?
    }

    pub fn stop_controller_use(&self) -> Result<(), String> {
        let peer = self
            .gamepad_lab
            .lock()
            .map_err(|_| "gamepad lock poisoned")?
            .workflow
            .peer();
        let Some(peer) = peer else { return Ok(()) };
        let (reply, receive) = mpsc::channel();
        {
            let links = self
                .control_links
                .lock()
                .map_err(|_| "control links lock poisoned")?;
            let link = links
                .get(&peer)
                .filter(|link| link.active.load(Ordering::Acquire))
                .ok_or("节点已断开，后台会清理手柄")?;
            link.commands
                .try_send(ControlLinkCommand::StopController { reply })
                .map_err(|_| "控制连接正忙，请稍后重试")?;
        }
        receive
            .recv_timeout(Duration::from_secs(20))
            .map_err(|_| "手柄停止请求超时")?
    }

    pub fn begin_windows_preflight(&self) -> Result<AtomicSingleFlight<'_>, String> {
        AtomicSingleFlight::acquire(
            &self.windows_gamepad_preflight_running,
            "Windows gamepad preflight is already running",
        )
    }

    pub fn apply_windows_preflight(
        &self,
        projection: windows_gamepad_host::UiWindowsGamepadProjection,
    ) -> Result<UiGamepadState, String> {
        Ok(self
            .gamepad_lab
            .lock()
            .map_err(|_| "gamepad lab state lock poisoned".to_owned())?
            .apply_windows_preflight(projection))
    }

    pub fn get_live_imu(&self) -> Result<UiLiveImu, String> {
        self.live_imu
            .lock()
            .map_err(|_| "live IMU state lock poisoned".to_owned())
            .map(|snapshot| snapshot.clone())
    }

    pub fn get_gamepad_state(&self) -> Result<UiGamepadState, String> {
        self.gamepad_lab
            .lock()
            .map_err(|_| "gamepad lab state lock poisoned".to_owned())
            .map(|lab| lab.snapshot())
    }

    pub fn update_gamepad_state(
        &self,
        request: UpdateGamepadRequest,
    ) -> Result<UiGamepadState, String> {
        self.gamepad_lab
            .lock()
            .map_err(|_| "gamepad lab state lock poisoned".to_owned())?
            .apply(request)
    }

    pub fn start_gamepad_dsu(
        &self,
        request: StartGamepadDsuRequest,
    ) -> Result<UiGamepadState, String> {
        if request.port == 0 {
            return Err("DSU port must be non-zero".to_owned());
        }
        self.gamepad_lab
            .lock()
            .map_err(|_| "gamepad lab state lock poisoned".to_owned())?
            .start_dsu_mode(request.port, request.mode)
    }

    pub fn stop_gamepad_dsu(&self) -> Result<UiGamepadState, String> {
        self.gamepad_lab
            .lock()
            .map_err(|_| "gamepad lab state lock poisoned".to_owned())?
            .stop_dsu()
    }

    pub fn start_android_gamepad(
        &self,
        request: StartAndroidGamepadRequest,
    ) -> Result<UiGamepadState, String> {
        self.gamepad_lab
            .lock()
            .map_err(|_| "gamepad lab state lock poisoned".to_owned())?
            .start_android(request.port)
    }

    pub fn stop_android_gamepad(&self) -> Result<UiGamepadState, String> {
        self.gamepad_lab
            .lock()
            .map_err(|_| "gamepad lab state lock poisoned".to_owned())?
            .stop_android()
    }

    pub fn start_windows_gamepad_projection(
        &self,
        request: StartWindowsGamepadProjectionRequest,
    ) -> Result<UiGamepadState, String> {
        let mut node = self.node.lock().map_err(|_| "Node state lock poisoned")?;
        self.gamepad_lab
            .lock()
            .map_err(|_| "gamepad lab state lock poisoned".to_owned())?
            .select_windows_projection(&mut node.runtime, request.controller_kind)
    }

    pub fn stop_windows_gamepad_projection(&self) -> Result<UiGamepadState, String> {
        let mut node = self.node.lock().map_err(|_| "Node state lock poisoned")?;
        self.gamepad_lab
            .lock()
            .map_err(|_| "gamepad lab state lock poisoned".to_owned())?
            .stop_windows_projection(&mut node.runtime)
    }

    pub fn start_controller_host(
        &self,
        request: StartControllerHostRequest,
    ) -> Result<UiGamepadState, String> {
        let mut node = self.node.lock().map_err(|_| "Node state lock poisoned")?;
        self.gamepad_lab
            .lock()
            .map_err(|_| "gamepad lab state lock poisoned")?
            .start_controller_host(&mut node.runtime, request.port, request.controller_kind)
    }

    pub fn stop_controller_host(&self) -> Result<UiGamepadState, String> {
        let mut node = self.node.lock().map_err(|_| "Node state lock poisoned")?;
        self.gamepad_lab
            .lock()
            .map_err(|_| "gamepad lab state lock poisoned")?
            .stop_controller_host(&mut node.runtime)
    }

    pub fn start_live_imu(&self, request: StartLiveImuRequest) -> Result<UiLiveImu, String> {
        let ip = request
            .ip
            .parse::<IpAddr>()
            .map_err(|_| "live IMU endpoint must be an IP literal".to_owned())?;
        let endpoint =
            SensorServerEndpoint::new(ip, request.port).map_err(|error| error.to_string())?;
        let mut controller = self
            .live_imu_controller
            .lock()
            .map_err(|_| "live IMU controller lock poisoned".to_owned())?;
        if controller
            .as_ref()
            .is_some_and(|controller| controller.worker.is_finished())
        {
            let finished = controller.take().expect("checked as present");
            finished
                .worker
                .join()
                .map_err(|_| "live IMU worker panicked".to_owned())?;
        }
        if controller.is_some() {
            return Err("live IMU lab is already running".to_owned());
        }
        let (epoch, route_id) = {
            let mut node = self
                .node
                .lock()
                .map_err(|_| "physical IMU Runtime lock poisoned".to_owned())?;
            let epoch = self
                .physical_imu
                .begin_start(&mut node.runtime, unix_time_ms()?)?;
            (epoch, self.physical_imu.route_id().to_string())
        };
        {
            let mut snapshot = self
                .live_imu
                .lock()
                .map_err(|_| "live IMU state lock poisoned".to_owned())?;
            *snapshot = UiLiveImu {
                status: "connecting",
                simulated: false,
                route_id,
                route_state: "starting",
                endpoint: Some(endpoint.address().to_string()),
                profile: "capyio.motion.imu-samples/1",
                stream_epoch: epoch,
                sequence: None,
                source_timestamp_nanos: None,
                clock_domain_id: None,
                acceleration: None,
                angular_velocity: None,
                received_samples: 0,
                problem_code: None,
                problem: None,
            };
        }
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let shared = Arc::clone(&self.live_imu);
        let runtime = Arc::clone(&self.node);
        let route = self.physical_imu;
        let worker = thread::spawn(move || {
            run_live_imu(endpoint, epoch, &worker_stop, &shared, &runtime, route)
        });
        *controller = Some(LiveImuController { stop, worker });
        self.live_imu
            .lock()
            .map_err(|_| "live IMU state lock poisoned".to_owned())
            .map(|snapshot| snapshot.clone())
    }

    pub fn stop_live_imu(&self) -> Result<UiLiveImu, String> {
        let controller = self
            .live_imu_controller
            .lock()
            .map_err(|_| "live IMU controller lock poisoned".to_owned())?
            .take();
        if let Some(controller) = controller {
            controller.stop.store(true, Ordering::Release);
            controller
                .worker
                .join()
                .map_err(|_| "live IMU worker panicked".to_owned())?;
        }
        let (route_state, route_epoch) = {
            let mut node = self
                .node
                .lock()
                .map_err(|_| "physical IMU Runtime lock poisoned".to_owned())?;
            self.physical_imu.stop(&mut node.runtime)?;
            (
                self.physical_imu.route_state(&node.runtime)?,
                self.physical_imu.route_epoch(&node.runtime)?,
            )
        };
        let mut snapshot = self
            .live_imu
            .lock()
            .map_err(|_| "live IMU state lock poisoned".to_owned())?;
        snapshot.status = "stopped";
        snapshot.route_state = route_state_label(route_state);
        snapshot.stream_epoch = route_epoch;
        Ok(snapshot.clone())
    }
}
