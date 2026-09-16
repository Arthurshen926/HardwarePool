use std::{
    collections::BTreeSet,
    net::IpAddr,
    str::FromStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use capyio_control_host::project_publication_catalog;
use capyio_core::{
    AdapterDeploymentMode, Availability, CapabilityClass, CapabilityId, NodeDescriptor,
    OnlineState, PermissionRequirement, PortDirection, PortRef, QosMode, Route, RouteId,
    RouteState, SessionId,
};
use capyio_data_plane::{
    BoundedFanout, BoundedJsonlRecorder, ImuSampleV1, NumericImuPanel, RecorderOutcome,
    parse_imu_fixture_jsonl,
};
use capyio_runtime::NodeRuntime;
use capyio_sensor_server_adapter::{
    AssembleOutcome, SensorKind, SensorServerConnectionConfig, SensorServerEndpoint,
    SensorServerImuAssembler, SensorServerReadOutcome, SensorServerReading,
    SensorServerWebSocketClient,
};
use capyio_testkit::{DemoLab, DemoRoutes};
use serde::{Deserialize, Serialize};
use tauri::State;

mod android_gamepad;
mod android_receivers;
pub mod audio_share_runtime;
mod gamepad_lab;
mod microphone_quick_action;
pub mod micyou_runtime;
mod physical_imu_runtime;
mod quick_actions;
mod windows_gamepad_host;

use gamepad_lab::{GamepadLab, UiGamepadState, UpdateGamepadRequest};
use microphone_quick_action::{MICYOU_ACTION_ID, MicrophoneQuickAction};
use physical_imu_runtime::PhysicalImuRoute;
use quick_actions::{
    AudioShareQuickAction, InvokeQuickActionRequest, SelectAudioEndpointRequest,
    UiAudioEndpointCatalog, UiQuickAction,
};
use windows_gamepad_host::WindowsGamepadHost;

struct AppState {
    node: Arc<Mutex<DesktopNodeState>>,
    local_shared_capabilities: Mutex<BTreeSet<CapabilityId>>,
    physical_imu: PhysicalImuRoute,
    live_imu: Arc<Mutex<UiLiveImu>>,
    live_imu_controller: Mutex<Option<LiveImuController>>,
    gamepad_lab: Arc<Mutex<GamepadLab>>,
    gamepad_worker: GamepadProjectionWorker,
    windows_gamepad_preflight_running: AtomicBool,
    audio_share: Arc<Mutex<AudioShareQuickAction>>,
    microphone: Arc<Mutex<MicrophoneQuickAction>>,
    audio_worker: AudioQuickActionWorker,
    android_receivers: Mutex<android_receivers::AndroidReceiverActions>,
}

/// Process-owned Node state. `DemoLab` is only a deterministic bootstrap source;
/// long-lived platform workers and future authenticated control connections share
/// this Runtime directly instead of depending on the test fixture wrapper.
struct DesktopNodeState {
    runtime: NodeRuntime,
    bootstrap_session_id: SessionId,
    demo_routes: DemoRoutes,
}

impl DesktopNodeState {
    fn demo() -> Result<Self, String> {
        let lab = DemoLab::new().map_err(|error| error.to_string())?;
        Ok(Self {
            runtime: lab.runtime,
            bootstrap_session_id: lab.session_id,
            demo_routes: lab.routes,
        })
    }

    fn set_demo_route_active(
        &mut self,
        route_id: RouteId,
        active: bool,
        now_ms: u64,
    ) -> Result<(), String> {
        if !self.demo_routes.all().contains(&route_id) {
            return Err("Route is not controlled by the deterministic demo".to_owned());
        }
        self.runtime
            .set_route_active(route_id, active, now_ms)
            .map_err(|error| error.to_string())
    }
}

struct AtomicSingleFlight<'a> {
    flag: &'a AtomicBool,
}

impl<'a> AtomicSingleFlight<'a> {
    fn acquire(flag: &'a AtomicBool, busy_message: &'static str) -> Result<Self, String> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| busy_message.to_owned())?;
        Ok(Self { flag })
    }
}

impl Drop for AtomicSingleFlight<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
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
        if let Ok(receivers) = self.android_receivers.get_mut() {
            receivers.shutdown();
        }
    }
}

struct LiveImuController {
    stop: Arc<AtomicBool>,
    worker: JoinHandle<()>,
}

struct AudioQuickActionWorker {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

struct GamepadProjectionWorker {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartLiveImuRequest {
    ip: String,
    port: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartGamepadDsuRequest {
    port: u16,
    mode: gamepad_lab::UiDsuMode,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartAndroidGamepadRequest {
    port: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RefreshWindowsGamepadPreflightRequest {
    controller_kind: windows_gamepad_host::UiWindowsControllerKind,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartWindowsGamepadProjectionRequest {
    enable_xinput_companion: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiLiveImu {
    status: &'static str,
    simulated: bool,
    route_id: String,
    route_state: &'static str,
    endpoint: Option<String>,
    profile: &'static str,
    stream_epoch: u64,
    sequence: Option<u64>,
    source_timestamp_nanos: Option<u64>,
    clock_domain_id: Option<String>,
    acceleration: Option<UiVector3>,
    angular_velocity: Option<UiVector3>,
    received_samples: u64,
    problem_code: Option<String>,
    problem: Option<String>,
}

impl UiLiveImu {
    fn idle(
        route: PhysicalImuRoute,
        runtime: &capyio_runtime::NodeRuntime,
    ) -> Result<Self, String> {
        Ok(Self {
            status: "idle",
            simulated: false,
            route_id: route.route_id().to_string(),
            route_state: route_state_label(route.route_state(runtime)?),
            endpoint: None,
            profile: "capyio.motion.imu-samples/1",
            stream_epoch: 0,
            sequence: None,
            source_timestamp_nanos: None,
            clock_domain_id: None,
            acceleration: None,
            angular_velocity: None,
            received_samples: 0,
            problem_code: None,
            problem: None,
        })
    }
}

#[derive(Debug)]
enum LiveWorkerEvent {
    Connected(SensorKind),
    Reading(SensorServerReading),
    Failed { kind: SensorKind, detail: String },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetRouteRequest {
    route_id: String,
    active: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SetLocalCapabilitySharedRequest {
    capability_id: String,
    selected: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiSnapshot {
    backend_mode: &'static str,
    schema_version: u8,
    project_version: &'static str,
    nodes: Vec<UiNode>,
    capabilities: Vec<UiCapability>,
    routes: Vec<UiRoute>,
    adapters: Vec<UiAdapter>,
    events: Vec<UiEvent>,
    warnings: Vec<String>,
    imu_fixture: UiImuFixture,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiNode {
    id: String,
    display_name: String,
    platform: String,
    platform_version: String,
    online: bool,
    local: bool,
    capability_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiCapability {
    id: String,
    node_id: String,
    node_name: String,
    display_name: String,
    capability_class: String,
    local: bool,
    availability: &'static str,
    permission_requirement: &'static str,
    inventory_source: &'static str,
    simulated: bool,
    selectable_for_sharing: bool,
    selected_for_sharing: bool,
    ports: Vec<UiCapabilityPort>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiCapabilityPort {
    id: String,
    display_name: String,
    direction: &'static str,
    profile: String,
    formats: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiRoute {
    id: String,
    title: String,
    summary: String,
    profile: String,
    backend: String,
    state: String,
    active: bool,
    source: UiPort,
    sink: UiPort,
    format_summary: Option<String>,
    qos_modes: Vec<String>,
    projection_note: String,
    metrics: UiMetricSet,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiPort {
    node_name: String,
    capability_name: String,
    capability_class: String,
    port_name: String,
    direction: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiAdapter {
    id: String,
    node_name: String,
    display_name: String,
    adapter_type: String,
    deployment_mode: String,
    state: String,
    health: String,
    capability_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiMetricSet {
    estimated_latency_ms: Option<f64>,
    packet_loss_percent: Option<f64>,
    buffer_fill_ms: Option<f64>,
    simulated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiEvent {
    sequence: u64,
    summary: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiImuFixture {
    mode: &'static str,
    simulated: bool,
    profile: &'static str,
    sequence: u64,
    source_timestamp_nanos: u64,
    clock_domain_id: String,
    acceleration: UiVector3,
    angular_velocity: UiVector3,
    panel_received: u64,
    panel_missing_sequences: u64,
    recorder_records: usize,
    panel_route_state: &'static str,
    recorder_route_state: &'static str,
}

#[derive(Clone, Debug, Serialize)]
struct UiVector3 {
    x: f64,
    y: f64,
    z: f64,
}

#[tauri::command]
fn get_snapshot(state: State<'_, AppState>) -> Result<UiSnapshot, String> {
    let node = state.node.lock().map_err(|_| "Node state lock poisoned")?;
    let selected = state
        .local_shared_capabilities
        .lock()
        .map_err(|_| "local Capability selection lock poisoned")?;
    Ok(to_ui_snapshot(&node, &selected))
}

#[tauri::command]
fn set_route(request: SetRouteRequest, state: State<'_, AppState>) -> Result<UiSnapshot, String> {
    let route_id = RouteId::from_str(&request.route_id).map_err(|_| "invalid Route ID")?;
    if state
        .audio_share
        .lock()
        .map_err(|_| "Audio Share state lock poisoned")?
        .owns_route(route_id)
    {
        return Err(
            "use the versioned Quick Action to control the physical speaker Route".to_owned(),
        );
    }
    if state
        .microphone
        .lock()
        .map_err(|_| "MicYou state lock poisoned")?
        .owns_route(route_id)
    {
        return Err(
            "use the versioned Quick Action to control the physical microphone Route".to_owned(),
        );
    }
    let mut node = state.node.lock().map_err(|_| "Node state lock poisoned")?;
    node.set_demo_route_active(route_id, request.active, unix_time_ms()?)?;
    let selected = state
        .local_shared_capabilities
        .lock()
        .map_err(|_| "local Capability selection lock poisoned")?;
    Ok(to_ui_snapshot(&node, &selected))
}

#[tauri::command]
fn set_local_capability_shared(
    request: SetLocalCapabilitySharedRequest,
    state: State<'_, AppState>,
) -> Result<UiSnapshot, String> {
    let capability_id = CapabilityId::from_str(&request.capability_id)
        .map_err(|_| "invalid Capability ID".to_owned())?;
    let node = state.node.lock().map_err(|_| "Node state lock poisoned")?;
    let mut selected = state
        .local_shared_capabilities
        .lock()
        .map_err(|_| "local Capability selection lock poisoned")?;
    update_local_capability_selection(
        &node.runtime,
        &mut selected,
        capability_id,
        request.selected,
    )?;
    Ok(to_ui_snapshot(&node, &selected))
}

#[tauri::command]
fn reset_demo(state: State<'_, AppState>) -> Result<UiSnapshot, String> {
    if state
        .audio_share
        .lock()
        .map_err(|_| "Audio Share state lock poisoned")?
        .is_configured()
    {
        return Err(
            "restart the desktop host to reset a configured physical speaker Route".to_owned(),
        );
    }
    if state
        .microphone
        .lock()
        .map_err(|_| "MicYou state lock poisoned")?
        .is_configured()
    {
        return Err(
            "restart the desktop host to reset a configured physical microphone Route".to_owned(),
        );
    }
    if state
        .live_imu_controller
        .lock()
        .map_err(|_| "live IMU controller lock poisoned")?
        .is_some()
    {
        return Err("stop the physical IMU Route before resetting the demo".to_owned());
    }
    let mut node = state.node.lock().map_err(|_| "Node state lock poisoned")?;
    *node = DesktopNodeState::demo()?;
    let session_id = node.bootstrap_session_id;
    PhysicalImuRoute::install(&mut node.runtime, session_id)?;
    *state
        .gamepad_lab
        .lock()
        .map_err(|_| "gamepad lab state lock poisoned".to_owned())? = GamepadLab::new()?;
    let mut selected = state
        .local_shared_capabilities
        .lock()
        .map_err(|_| "local Capability selection lock poisoned")?;
    selected.clear();
    Ok(to_ui_snapshot(&node, &selected))
}

#[tauri::command]
fn get_quick_actions(state: State<'_, AppState>) -> Result<Vec<UiQuickAction>, String> {
    let speaker = {
        let audio_share = state
            .audio_share
            .lock()
            .map_err(|_| "Audio Share state lock poisoned")?;
        let node = state.node.lock().map_err(|_| "Node state lock poisoned")?;
        audio_share.dto(&node.runtime)?
    };
    let microphone = {
        let microphone = state
            .microphone
            .lock()
            .map_err(|_| "MicYou state lock poisoned")?;
        let node = state.node.lock().map_err(|_| "Node state lock poisoned")?;
        microphone.dto(&node.runtime)?
    };
    let mut actions = vec![speaker, microphone];
    actions.extend(
        state
            .android_receivers
            .lock()
            .map_err(|_| "Android receiver state lock poisoned")?
            .actions(),
    );
    Ok(actions)
}

#[tauri::command]
fn invoke_quick_action(
    request: InvokeQuickActionRequest,
    state: State<'_, AppState>,
) -> Result<UiQuickAction, String> {
    if android_receivers::AndroidReceiverActions::owns(&request.action_id) {
        return state
            .android_receivers
            .lock()
            .map_err(|_| "Android receiver state lock poisoned".to_owned())?
            .invoke(&request.action_id, request.operation);
    }
    if request.action_id == MICYOU_ACTION_ID {
        let mut microphone = state
            .microphone
            .lock()
            .map_err(|_| "MicYou state lock poisoned")?;
        let mut node = state.node.lock().map_err(|_| "Node state lock poisoned")?;
        microphone.invoke(&mut node.runtime, request, unix_time_ms()?)
    } else {
        let mut audio_share = state
            .audio_share
            .lock()
            .map_err(|_| "Audio Share state lock poisoned")?;
        let mut node = state.node.lock().map_err(|_| "Node state lock poisoned")?;
        audio_share.invoke(&mut node.runtime, request, unix_time_ms()?)
    }
}

#[tauri::command]
fn get_audio_endpoints(state: State<'_, AppState>) -> Result<UiAudioEndpointCatalog, String> {
    let mut audio_share = state
        .audio_share
        .lock()
        .map_err(|_| "Audio Share state lock poisoned")?;
    let can_select = {
        let node = state.node.lock().map_err(|_| "Node state lock poisoned")?;
        audio_share.endpoint_selection_available(&node.runtime)
    };
    Ok(audio_share.endpoint_catalog(can_select))
}

#[tauri::command]
fn select_audio_endpoint(
    request: SelectAudioEndpointRequest,
    state: State<'_, AppState>,
) -> Result<UiQuickAction, String> {
    let mut audio_share = state
        .audio_share
        .lock()
        .map_err(|_| "Audio Share state lock poisoned")?;
    let node = state.node.lock().map_err(|_| "Node state lock poisoned")?;
    audio_share.select_endpoint(&node.runtime, request)
}

#[tauri::command]
fn get_live_imu(state: State<'_, AppState>) -> Result<UiLiveImu, String> {
    state
        .live_imu
        .lock()
        .map_err(|_| "live IMU state lock poisoned".to_owned())
        .map(|snapshot| snapshot.clone())
}

#[tauri::command]
fn get_gamepad_state(state: State<'_, AppState>) -> Result<UiGamepadState, String> {
    state
        .gamepad_lab
        .lock()
        .map_err(|_| "gamepad lab state lock poisoned".to_owned())
        .map(|lab| lab.snapshot())
}

#[tauri::command]
async fn refresh_windows_gamepad_preflight(
    request: RefreshWindowsGamepadPreflightRequest,
    state: State<'_, AppState>,
) -> Result<UiGamepadState, String> {
    let _single_flight = AtomicSingleFlight::acquire(
        &state.windows_gamepad_preflight_running,
        "Windows gamepad preflight is already running",
    )?;
    let projection = tauri::async_runtime::spawn_blocking(move || {
        windows_gamepad_host::probe_read_only(request.controller_kind)
    })
    .await
    .map_err(|_| "Windows gamepad preflight worker panicked".to_owned())?;
    Ok(state
        .gamepad_lab
        .lock()
        .map_err(|_| "gamepad lab state lock poisoned".to_owned())?
        .apply_windows_preflight(projection))
}

#[tauri::command]
fn update_gamepad_state(
    request: UpdateGamepadRequest,
    state: State<'_, AppState>,
) -> Result<UiGamepadState, String> {
    state
        .gamepad_lab
        .lock()
        .map_err(|_| "gamepad lab state lock poisoned".to_owned())?
        .apply(request)
}

#[tauri::command]
fn start_gamepad_dsu(
    request: StartGamepadDsuRequest,
    state: State<'_, AppState>,
) -> Result<UiGamepadState, String> {
    if request.port == 0 {
        return Err("DSU port must be non-zero".to_owned());
    }
    state
        .gamepad_lab
        .lock()
        .map_err(|_| "gamepad lab state lock poisoned".to_owned())?
        .start_dsu_mode(request.port, request.mode)
}

#[tauri::command]
fn stop_gamepad_dsu(state: State<'_, AppState>) -> Result<UiGamepadState, String> {
    state
        .gamepad_lab
        .lock()
        .map_err(|_| "gamepad lab state lock poisoned".to_owned())?
        .stop_dsu()
}

#[tauri::command]
fn start_android_gamepad(
    request: StartAndroidGamepadRequest,
    state: State<'_, AppState>,
) -> Result<UiGamepadState, String> {
    state
        .gamepad_lab
        .lock()
        .map_err(|_| "gamepad lab state lock poisoned".to_owned())?
        .start_android(request.port)
}

#[tauri::command]
fn stop_android_gamepad(state: State<'_, AppState>) -> Result<UiGamepadState, String> {
    state
        .gamepad_lab
        .lock()
        .map_err(|_| "gamepad lab state lock poisoned".to_owned())?
        .stop_android()
}

#[tauri::command]
fn start_windows_gamepad_projection(
    request: StartWindowsGamepadProjectionRequest,
    state: State<'_, AppState>,
) -> Result<UiGamepadState, String> {
    let mut node = state.node.lock().map_err(|_| "Node state lock poisoned")?;
    state
        .gamepad_lab
        .lock()
        .map_err(|_| "gamepad lab state lock poisoned".to_owned())?
        .start_windows_projection(&mut node.runtime, request.enable_xinput_companion)
}

#[tauri::command]
fn stop_windows_gamepad_projection(state: State<'_, AppState>) -> Result<UiGamepadState, String> {
    let mut node = state.node.lock().map_err(|_| "Node state lock poisoned")?;
    state
        .gamepad_lab
        .lock()
        .map_err(|_| "gamepad lab state lock poisoned".to_owned())?
        .stop_windows_projection(&mut node.runtime)
}

#[tauri::command]
fn start_live_imu(
    request: StartLiveImuRequest,
    state: State<'_, AppState>,
) -> Result<UiLiveImu, String> {
    let ip = request
        .ip
        .parse::<IpAddr>()
        .map_err(|_| "live IMU endpoint must be an IP literal".to_owned())?;
    let endpoint =
        SensorServerEndpoint::new(ip, request.port).map_err(|error| error.to_string())?;
    let mut controller = state
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
        let mut node = state
            .node
            .lock()
            .map_err(|_| "physical IMU Runtime lock poisoned".to_owned())?;
        let epoch = state
            .physical_imu
            .begin_start(&mut node.runtime, unix_time_ms()?)?;
        (epoch, state.physical_imu.route_id().to_string())
    };
    {
        let mut snapshot = state
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
    let shared = Arc::clone(&state.live_imu);
    let runtime = Arc::clone(&state.node);
    let route = state.physical_imu;
    let worker = thread::spawn(move || {
        run_live_imu(endpoint, epoch, &worker_stop, &shared, &runtime, route)
    });
    *controller = Some(LiveImuController { stop, worker });
    state
        .live_imu
        .lock()
        .map_err(|_| "live IMU state lock poisoned".to_owned())
        .map(|snapshot| snapshot.clone())
}

#[tauri::command]
fn stop_live_imu(state: State<'_, AppState>) -> Result<UiLiveImu, String> {
    let controller = state
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
        let mut node = state
            .node
            .lock()
            .map_err(|_| "physical IMU Runtime lock poisoned".to_owned())?;
        state.physical_imu.stop(&mut node.runtime)?;
        (
            state.physical_imu.route_state(&node.runtime)?,
            state.physical_imu.route_epoch(&node.runtime)?,
        )
    };
    let mut snapshot = state
        .live_imu
        .lock()
        .map_err(|_| "live IMU state lock poisoned".to_owned())?;
    snapshot.status = "stopped";
    snapshot.route_state = route_state_label(route_state);
    snapshot.stream_epoch = route_epoch;
    Ok(snapshot.clone())
}

fn run_live_imu(
    endpoint: SensorServerEndpoint,
    epoch: u64,
    stop: &Arc<AtomicBool>,
    shared: &Arc<Mutex<UiLiveImu>>,
    runtime: &Arc<Mutex<DesktopNodeState>>,
    route: PhysicalImuRoute,
) {
    if let Err(problem) = run_live_imu_inner(endpoint, epoch, stop, shared, runtime, route) {
        stop.store(true, Ordering::Release);
        let runtime_update = runtime
            .lock()
            .map_err(|_| "physical IMU Runtime lock poisoned".to_owned())
            .and_then(|mut node| {
                route.report_offline(&mut node.runtime, problem.clone())?;
                let retained = route.latest_problem(&node.runtime);
                Ok((
                    route.route_state(&node.runtime)?,
                    route.route_epoch(&node.runtime)?,
                    retained,
                ))
            });
        if let Ok(mut snapshot) = shared.lock() {
            match runtime_update {
                Ok((route_state, route_epoch, retained)) => {
                    snapshot.status = "offline";
                    snapshot.route_state = route_state_label(route_state);
                    snapshot.stream_epoch = route_epoch;
                    snapshot.problem_code = retained.as_ref().map(|item| item.code.clone());
                    snapshot.problem = retained
                        .and_then(|item| item.technical_detail)
                        .or(Some(problem));
                }
                Err(runtime_error) => {
                    snapshot.status = "failed";
                    snapshot.route_state = "failed";
                    snapshot.problem_code = Some("CAPY.RUNTIME.IMU_LIFECYCLE".to_owned());
                    snapshot.problem = Some(runtime_error);
                }
            }
        }
    }
}

fn run_live_imu_inner(
    endpoint: SensorServerEndpoint,
    epoch: u64,
    stop: &Arc<AtomicBool>,
    shared: &Arc<Mutex<UiLiveImu>>,
    runtime: &Arc<Mutex<DesktopNodeState>>,
    route: PhysicalImuRoute,
) -> Result<(), String> {
    let config =
        SensorServerConnectionConfig::new(Duration::from_secs(5), Duration::from_millis(500))
            .map_err(|error| error.to_string())?;
    let (sender, receiver) = mpsc::sync_channel(256);
    let accelerometer = spawn_live_reader(
        endpoint,
        SensorKind::Accelerometer,
        config,
        sender.clone(),
        Arc::clone(stop),
    );
    match receiver
        .recv_timeout(Duration::from_secs(10))
        .map_err(|error| format!("accelerometer connection timed out: {error}"))?
    {
        LiveWorkerEvent::Connected(SensorKind::Accelerometer) => {}
        LiveWorkerEvent::Failed { kind, detail } => {
            return Err(format!("{kind:?} reader failed: {detail}"));
        }
        event => return Err(format!("unexpected initial live IMU event: {event:?}")),
    }
    let gyroscope = spawn_live_reader(
        endpoint,
        SensorKind::Gyroscope,
        config,
        sender,
        Arc::clone(stop),
    );
    let mut assembler =
        SensorServerImuAssembler::new(capyio_core::StreamId::new(), epoch, 1_000_000_000, 0)
            .map_err(|error| error.to_string())?;
    let started = Instant::now();
    while !stop.load(Ordering::Acquire) {
        let event = match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(event) => event,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(error) => return Err(format!("live IMU event channel failed: {error}")),
        };
        match event {
            LiveWorkerEvent::Connected(SensorKind::Gyroscope) => {
                let (route_state, route_epoch) = {
                    let mut node = runtime
                        .lock()
                        .map_err(|_| "physical IMU Runtime lock poisoned".to_owned())?;
                    route.activate(&mut node.runtime)?;
                    (
                        route.route_state(&node.runtime)?,
                        route.route_epoch(&node.runtime)?,
                    )
                };
                let mut snapshot = shared
                    .lock()
                    .map_err(|_| "live IMU state lock poisoned".to_owned())?;
                snapshot.status = "active";
                snapshot.route_state = route_state_label(route_state);
                snapshot.stream_epoch = route_epoch;
            }
            LiveWorkerEvent::Connected(_) => {}
            LiveWorkerEvent::Reading(reading) => {
                let receive_timestamp_nanos = u64::try_from(started.elapsed().as_nanos())
                    .unwrap_or(u64::MAX)
                    .saturating_add(1);
                if let AssembleOutcome::Emitted { envelope, .. } = assembler
                    .ingest(reading, receive_timestamp_nanos)
                    .map_err(|error| error.to_string())?
                {
                    let mut snapshot = shared
                        .lock()
                        .map_err(|_| "live IMU state lock poisoned".to_owned())?;
                    snapshot.sequence = Some(envelope.sequence);
                    snapshot.source_timestamp_nanos = Some(envelope.source_timestamp_nanos);
                    snapshot.clock_domain_id = Some(envelope.clock_domain_id.clone());
                    snapshot.acceleration = Some(vector3(envelope.payload.acceleration));
                    snapshot.angular_velocity = Some(vector3(envelope.payload.angular_velocity));
                    snapshot.received_samples = snapshot.received_samples.saturating_add(1);
                }
            }
            LiveWorkerEvent::Failed { kind, detail } => {
                return Err(format!("{kind:?} reader failed: {detail}"));
            }
        }
    }
    stop.store(true, Ordering::Release);
    accelerometer
        .join()
        .map_err(|_| "accelerometer reader thread panicked".to_owned())?;
    gyroscope
        .join()
        .map_err(|_| "gyroscope reader thread panicked".to_owned())?;
    Ok(())
}

fn spawn_live_reader(
    endpoint: SensorServerEndpoint,
    kind: SensorKind,
    config: SensorServerConnectionConfig,
    sender: SyncSender<LiveWorkerEvent>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        if let Err(detail) = read_live_sensor(endpoint, kind, config, &sender, &stop) {
            let _ = sender.try_send(LiveWorkerEvent::Failed { kind, detail });
        }
    })
}

fn read_live_sensor(
    endpoint: SensorServerEndpoint,
    kind: SensorKind,
    config: SensorServerConnectionConfig,
    sender: &SyncSender<LiveWorkerEvent>,
    stop: &AtomicBool,
) -> Result<(), String> {
    let mut client = SensorServerWebSocketClient::connect(endpoint, kind, config)
        .map_err(|error| error.to_string())?;
    sender
        .try_send(LiveWorkerEvent::Connected(kind))
        .map_err(|error| error.to_string())?;
    while !stop.load(Ordering::Acquire) {
        match client.read().map_err(|error| error.to_string())? {
            SensorServerReadOutcome::Reading(reading) => sender
                .try_send(LiveWorkerEvent::Reading(reading))
                .map_err(|error| error.to_string())?,
            SensorServerReadOutcome::TimedOut | SensorServerReadOutcome::ControlHandled(_) => {}
            SensorServerReadOutcome::Closed { code } => {
                return Err(format!("connection closed with code {code:?}"));
            }
        }
    }
    client.close().map_err(|error| error.to_string())
}

/// Debug-only operator-assisted physical Android -> DSU gate.
#[cfg(debug_assertions)]
pub fn run_gamepad_physical_gate(android_port: u16, dsu_port: u16) -> Result<(), String> {
    gamepad_lab::run_physical_gamepad_gate(android_port, dsu_port)
}

/// Debug-only Runtime-owned physical Android -> Windows DS4 Gate.
#[cfg(debug_assertions)]
pub fn run_windows_ds4_runtime_gate(android_port: u16, hold_seconds: u64) -> Result<(), String> {
    gamepad_lab::run_windows_ds4_runtime_gate(android_port, hold_seconds)
}

/// Debug-only Runtime-owned physical Android -> Windows DS4-only Gate.
#[cfg(debug_assertions)]
pub fn run_windows_ds4_only_runtime_gate(
    android_port: u16,
    hold_seconds: u64,
) -> Result<(), String> {
    gamepad_lab::run_windows_ds4_only_runtime_gate(android_port, hold_seconds)
}

/// Debug-only operator-assisted physical Android -> DSU + VIIPER gate.
#[cfg(debug_assertions)]
pub fn run_gamepad_viiper_physical_gate(
    android_port: u16,
    dsu_port: u16,
    viiper_port: u16,
    hold_seconds: u64,
) -> Result<(), String> {
    gamepad_lab::run_physical_viiper_gamepad_gate(android_port, dsu_port, viiper_port, hold_seconds)
}

/// Debug-only operator-assisted physical Android touch+IMU -> native DS4 gate.
#[cfg(debug_assertions)]
pub fn run_gamepad_ds4_physical_gate(
    android_port: u16,
    viiper_port: u16,
    hold_seconds: u64,
) -> Result<(), String> {
    gamepad_lab::run_physical_ds4_gamepad_gate(android_port, viiper_port, hold_seconds)
}

/// Debug-only fixed-config read-only Windows gamepad host preflight.
#[cfg(debug_assertions)]
pub fn run_windows_gamepad_read_only_preflight() -> Result<(), String> {
    windows_gamepad_host::run_read_only_preflight_gate()
}

pub fn run() {
    let mut node = DesktopNodeState::demo().expect("valid deterministic demo bootstrap");
    let session_id = node.bootstrap_session_id;
    let physical_imu = PhysicalImuRoute::install(&mut node.runtime, session_id)
        .expect("valid physical IMU Runtime catalog");
    let live_imu = UiLiveImu::idle(physical_imu, &node.runtime).expect("valid physical IMU Route");
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
                if let (Ok(mut node), Ok(mut gamepad)) = (worker_node.lock(), worker_gamepad.lock())
                {
                    gamepad.poll_windows_projection(&mut node.runtime);
                }
                thread::sleep(Duration::from_millis(4));
            }
        })
        .expect("start Windows gamepad projection worker");
    tauri::Builder::default()
        .manage(AppState {
            node,
            local_shared_capabilities: Mutex::new(BTreeSet::new()),
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
            android_receivers: Mutex::new(android_receivers::AndroidReceiverActions::new()),
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            set_route,
            set_local_capability_shared,
            reset_demo,
            get_live_imu,
            start_live_imu,
            stop_live_imu,
            get_gamepad_state,
            refresh_windows_gamepad_preflight,
            update_gamepad_state,
            start_gamepad_dsu,
            stop_gamepad_dsu,
            start_android_gamepad,
            stop_android_gamepad,
            start_windows_gamepad_projection,
            stop_windows_gamepad_projection,
            get_quick_actions,
            invoke_quick_action,
            get_audio_endpoints,
            select_audio_endpoint
        ])
        .run(tauri::generate_context!())
        .expect("run CapyIO Tauri application");
}

fn to_ui_snapshot(node: &DesktopNodeState, selected: &BTreeSet<CapabilityId>) -> UiSnapshot {
    let snapshot = node.runtime.snapshot();
    let local_node_id = snapshot.local_node.id;
    let all_nodes = std::iter::once(&snapshot.local_node).chain(snapshot.peers.iter());
    let nodes = all_nodes
        .clone()
        .map(|node| UiNode {
            id: node.id.to_string(),
            display_name: node.display_name.clone(),
            platform: platform_label(&node.platform),
            platform_version: node.platform_version.clone(),
            online: node.online_state == OnlineState::Online,
            local: node.id == local_node_id,
            capability_count: node.capabilities.len(),
        })
        .collect();
    let capabilities = all_nodes
        .clone()
        .flat_map(|node| {
            let local = node.id == local_node_id;
            node.capabilities.values().map(move |capability| {
                let selectable_for_sharing = local
                    && capability.availability != Availability::Offline
                    && capability
                        .ports
                        .values()
                        .any(|port| port.direction == PortDirection::Source);
                UiCapability {
                    id: capability.id.to_string(),
                    node_id: node.id.to_string(),
                    node_name: node.display_name.clone(),
                    display_name: capability.display_name.clone(),
                    capability_class: capability_class_label(&capability.class),
                    local,
                    availability: availability_label(capability.availability),
                    permission_requirement: permission_requirement_label(
                        capability.permission_requirement,
                    ),
                    inventory_source: "runtime_catalog",
                    simulated: true,
                    selectable_for_sharing,
                    selected_for_sharing: selectable_for_sharing
                        && selected.contains(&capability.id),
                    ports: capability
                        .ports
                        .values()
                        .map(|port| UiCapabilityPort {
                            id: port.id.to_string(),
                            display_name: port.display_name.clone(),
                            direction: port_direction_label(port.direction),
                            profile: format!("{}/{}", port.profile.name, port.profile.major),
                            formats: port
                                .formats
                                .iter()
                                .map(|format| format.id.clone())
                                .collect(),
                        })
                        .collect(),
                }
            })
        })
        .collect();
    let adapters = all_nodes
        .clone()
        .flat_map(|node| {
            node.adapter_instances
                .values()
                .map(move |adapter| UiAdapter {
                    id: adapter.id.to_string(),
                    node_name: node.display_name.clone(),
                    display_name: adapter.display_name.clone(),
                    adapter_type: adapter.adapter_type.clone(),
                    deployment_mode: deployment_label(adapter.deployment_mode).to_owned(),
                    state: format!("{:?}", adapter.state).to_lowercase(),
                    health: format!("{:?}", adapter.health).to_lowercase(),
                    capability_count: adapter.owned_capabilities.len(),
                })
        })
        .collect();
    let routes = snapshot
        .routes
        .iter()
        .filter(|route| node.demo_routes.all().contains(&route.id))
        .map(|route| route_to_ui(route, &snapshot.local_node, &snapshot.peers))
        .collect();
    let events = snapshot
        .events
        .iter()
        .map(|event| UiEvent {
            sequence: event.sequence,
            summary: format!("{:?}", event.kind),
        })
        .collect();

    UiSnapshot {
        backend_mode: "tauri_demo",
        schema_version: 4,
        project_version: env!("CARGO_PKG_VERSION"),
        nodes,
        capabilities,
        routes,
        adapters,
        events,
        warnings: vec![
            "Tauri Demo 的 Route/授权/指标仍为确定性数据；只有显式启动的 Physical IMU Lab 会访问一个经校验的 IP 端点。".to_owned(),
            "能力目录当前由确定性 bootstrap 初始化；选择仅保存在当前进程，不代表真实硬件枚举、授权或网络发布。".to_owned(),
        ],
        imu_fixture: build_imu_fixture(),
    }
}

fn update_local_capability_selection(
    runtime: &NodeRuntime,
    selected: &mut BTreeSet<CapabilityId>,
    capability_id: CapabilityId,
    enabled: bool,
) -> Result<(), String> {
    let snapshot = runtime.snapshot();
    snapshot
        .local_node
        .capabilities
        .get(&capability_id)
        .ok_or_else(|| "Capability is not advertised by the local Node".to_owned())?;
    let mut candidate = selected.clone();
    if enabled {
        candidate.insert(capability_id);
    } else {
        candidate.remove(&capability_id);
    }
    project_publication_catalog(&snapshot.local_node, &candidate)
        .map_err(|error| error.to_string())?;
    *selected = candidate;
    Ok(())
}

fn build_imu_fixture() -> UiImuFixture {
    const FIXTURE: &str = include_str!("../../../../fixtures/imu/imu_samples_v1.jsonl");
    let envelopes = parse_imu_fixture_jsonl(FIXTURE, 64).expect("valid compiled IMU fixture");
    let first = envelopes.first().expect("non-empty compiled IMU fixture");
    let mut fanout =
        BoundedFanout::new(ImuSampleV1::profile(), first.stream_id, first.stream_epoch);
    fanout
        .register_consumer("numeric-panel", 64)
        .expect("valid Panel queue");
    fanout
        .register_consumer("jsonl-recorder", 64)
        .expect("valid Recorder queue");
    for envelope in envelopes.iter().cloned() {
        let outcomes = fanout.publish(envelope);
        assert!(outcomes.values().all(Result::is_ok));
    }
    let mut panel = NumericImuPanel::default();
    let mut recorder = BoundedJsonlRecorder::new(64, 4096).expect("valid Recorder bounds");
    while let Some(delivery) = fanout.pop("numeric-panel").expect("registered Panel") {
        panel.consume(delivery);
    }
    while let Some(delivery) = fanout.pop("jsonl-recorder").expect("registered Recorder") {
        assert_eq!(
            recorder.record(&delivery).expect("serializable delivery"),
            RecorderOutcome::Recorded
        );
    }
    let last_envelope = envelopes.last().expect("non-empty compiled IMU fixture");
    let sample = panel
        .last_sample
        .expect("numeric Panel consumed compiled IMU fixture");
    UiImuFixture {
        mode: "deterministic_fixture",
        simulated: true,
        profile: "capyio.motion.imu-samples/1",
        sequence: last_envelope.sequence,
        source_timestamp_nanos: last_envelope.source_timestamp_nanos,
        clock_domain_id: last_envelope.clock_domain_id.clone(),
        acceleration: vector3(sample.acceleration),
        angular_velocity: vector3(sample.angular_velocity),
        panel_received: panel.received,
        panel_missing_sequences: panel.missing_sequences,
        recorder_records: recorder.len(),
        panel_route_state: "active",
        recorder_route_state: "active",
    }
}

fn vector3(value: [f64; 3]) -> UiVector3 {
    UiVector3 {
        x: value[0],
        y: value[1],
        z: value[2],
    }
}

fn route_to_ui(route: &Route, local: &NodeDescriptor, peers: &[NodeDescriptor]) -> UiRoute {
    let source = resolve_port(route.source, local, peers);
    let sink = resolve_port(route.sink, local, peers);
    let active = route.state == RouteState::Active;
    let title = format!("{} → {}", source.capability_name, sink.capability_name);
    let summary = format!("{} → {}", source.node_name, sink.node_name);
    let format_summary = route
        .selected_format
        .as_ref()
        .or_else(|| route.compatible_formats.first())
        .map(|format| format.id.clone());
    let qos_modes = route
        .compatible_qos_modes
        .iter()
        .map(qos_label)
        .map(str::to_owned)
        .collect();
    let projection_note = match sink.capability_class.as_str() {
        "panel" => "CapyIO 应用内 Panel（模拟）",
        "gamepad" => "本地游戏手柄投影（best-effort，模拟）",
        "microphone" => "Windows 系统端点投影（驱动尚未实现）",
        "speaker" => "Android 应用内播放端（模拟）",
        _ => "标准 Port Sink",
    }
    .to_owned();

    UiRoute {
        id: route.id.to_string(),
        title,
        summary,
        profile: format!("{}/{}", route.profile.name, route.profile.major),
        backend: format!("{:?}", route.backend).to_lowercase(),
        state: route_state_label(route.state).to_owned(),
        active,
        source,
        sink,
        format_summary,
        qos_modes,
        projection_note,
        metrics: demo_metrics(&route.profile.name, active),
    }
}

fn resolve_port(reference: PortRef, local: &NodeDescriptor, peers: &[NodeDescriptor]) -> UiPort {
    let node = std::iter::once(local)
        .chain(peers.iter())
        .find(|node| node.id == reference.node_id)
        .expect("demo Route references advertised Node");
    let capability = node
        .capabilities
        .get(&reference.capability_id)
        .expect("demo Route references advertised Capability");
    let port = capability
        .ports
        .get(&reference.port_id)
        .expect("demo Route references advertised Port");
    UiPort {
        node_name: node.display_name.clone(),
        capability_name: capability.display_name.clone(),
        capability_class: capability_class_label(&capability.class),
        port_name: port.display_name.clone(),
        direction: port_direction_label(port.direction).to_owned(),
    }
}

fn capability_class_label(value: &CapabilityClass) -> String {
    match value {
        CapabilityClass::Custom(custom) => custom.clone(),
        other => format!("{other:?}").to_lowercase(),
    }
}

fn platform_label(value: &capyio_core::Platform) -> String {
    match value {
        capyio_core::Platform::Unknown(detail) => detail.clone(),
        other => format!("{other:?}").to_lowercase(),
    }
}

const fn deployment_label(value: AdapterDeploymentMode) -> &'static str {
    match value {
        AdapterDeploymentMode::InProcess => "in_process",
        AdapterDeploymentMode::Sidecar => "sidecar",
        AdapterDeploymentMode::ExternalService => "external_service",
        AdapterDeploymentMode::DriverBacked => "driver_backed",
    }
}

const fn port_direction_label(value: PortDirection) -> &'static str {
    match value {
        PortDirection::Source => "source",
        PortDirection::Sink => "sink",
        PortDirection::Control => "control",
    }
}

const fn availability_label(value: Availability) -> &'static str {
    match value {
        Availability::Available => "available",
        Availability::Busy => "busy",
        Availability::PermissionRequired => "permission_required",
        Availability::Offline => "offline",
    }
}

const fn permission_requirement_label(value: PermissionRequirement) -> &'static str {
    match value {
        PermissionRequirement::None => "none",
        PermissionRequirement::UserConfirmation => "user_confirmation",
        PermissionRequirement::ForegroundService => "foreground_service",
        PermissionRequirement::Privileged => "privileged",
    }
}

fn qos_label(value: &QosMode) -> &str {
    match value {
        QosMode::Basic => "basic",
        QosMode::Interactive => "interactive",
        QosMode::Measurement => "measurement",
        QosMode::Custom(custom) => custom,
    }
}

const fn route_state_label(value: RouteState) -> &'static str {
    match value {
        RouteState::Draft => "draft",
        RouteState::Prepared => "prepared",
        RouteState::Starting => "starting",
        RouteState::Active => "active",
        RouteState::Stopping => "stopping",
        RouteState::Stopped => "stopped",
        RouteState::Failed => "failed",
        RouteState::Offline => "offline",
    }
}

fn demo_metrics(profile: &str, active: bool) -> UiMetricSet {
    if !active {
        return UiMetricSet {
            estimated_latency_ms: None,
            packet_loss_percent: None,
            buffer_fill_ms: None,
            simulated: true,
        };
    }
    let (latency, loss, buffer) = if profile.contains("audio") {
        (47.3, 0.03, 30.0)
    } else if profile.contains("video") {
        (81.2, 0.07, 66.0)
    } else {
        (18.6, 0.01, 12.0)
    };
    UiMetricSet {
        estimated_latency_ms: Some(latency),
        packet_loss_percent: Some(loss),
        buffer_fill_ms: Some(buffer),
        simulated: true,
    }
}

fn unix_time_ms() -> Result<u64, String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before UNIX epoch".to_owned())?;
    u64::try_from(elapsed.as_millis()).map_err(|_| "system time is out of range".to_owned())
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, TcpListener};

    use tungstenite::{Message, accept};

    use super::*;

    fn physical_node() -> (DesktopNodeState, PhysicalImuRoute) {
        let mut node = DesktopNodeState::demo().expect("demo Runtime bootstrap");
        let session_id = node.bootstrap_session_id;
        let route = PhysicalImuRoute::install(&mut node.runtime, session_id)
            .expect("physical Route fixture");
        (node, route)
    }

    #[test]
    fn idle_live_imu_dto_is_explicitly_real_but_inactive() {
        let (node, route) = physical_node();
        let snapshot = UiLiveImu::idle(route, &node.runtime).expect("idle DTO");
        assert_eq!(snapshot.status, "idle");
        assert!(!snapshot.simulated);
        assert_eq!(snapshot.route_state, "draft");
        assert_eq!(snapshot.stream_epoch, 0);
        assert!(snapshot.acceleration.is_none());
        assert!(snapshot.problem.is_none());
    }

    #[test]
    fn capability_catalog_separates_local_sharing_from_peer_inventory() {
        let node = DesktopNodeState::demo().expect("demo Runtime bootstrap");
        let runtime = node.runtime.snapshot();
        let local_source = runtime
            .local_node
            .capabilities
            .values()
            .find(|capability| {
                capability
                    .ports
                    .values()
                    .any(|port| port.direction == PortDirection::Source)
            })
            .expect("local Source Capability")
            .id;
        let local_sink = runtime
            .local_node
            .capabilities
            .values()
            .find(|capability| {
                capability
                    .ports
                    .values()
                    .all(|port| port.direction == PortDirection::Sink)
            })
            .expect("local Sink-only Capability")
            .id;
        let peer_source = runtime
            .peers
            .first()
            .expect("peer Node")
            .capabilities
            .values()
            .find(|capability| {
                capability
                    .ports
                    .values()
                    .any(|port| port.direction == PortDirection::Source)
            })
            .expect("peer Source Capability")
            .id;
        let mut selected = BTreeSet::new();

        update_local_capability_selection(&node.runtime, &mut selected, local_source, true)
            .expect("select local Source");
        assert!(
            update_local_capability_selection(&node.runtime, &mut selected, local_sink, true)
                .is_err()
        );
        assert!(
            update_local_capability_selection(&node.runtime, &mut selected, peer_source, true)
                .is_err()
        );

        let dto = to_ui_snapshot(&node, &selected);
        assert_eq!(dto.schema_version, 4);
        assert!(dto.capabilities.iter().any(|capability| {
            capability.local
                && capability.id == local_source.to_string()
                && capability.selectable_for_sharing
                && capability.selected_for_sharing
        }));
        assert!(dto.capabilities.iter().any(|capability| {
            !capability.local
                && capability.id == peer_source.to_string()
                && !capability.selectable_for_sharing
                && !capability.selected_for_sharing
        }));
        assert_eq!(
            dto.capabilities
                .iter()
                .filter(|capability| capability.capability_class == "touchpad")
                .count(),
            2
        );
    }

    #[test]
    fn desktop_node_only_exposes_fixture_routes_to_demo_mutation() {
        let (mut node, physical_imu) = physical_node();
        let before = node.runtime.snapshot();

        let error = node
            .set_demo_route_active(physical_imu.route_id(), true, 1)
            .expect_err("physical Route must use its own lifecycle boundary");

        assert_eq!(error, "Route is not controlled by the deterministic demo");
        assert_eq!(node.runtime.snapshot(), before);
    }

    #[test]
    fn desktop_can_remove_stale_publication_intent_after_source_goes_offline() {
        let mut node = DesktopNodeState::demo().expect("demo Runtime bootstrap");
        let snapshot = node.runtime.snapshot();
        let source = snapshot
            .local_node
            .capabilities
            .values()
            .find(|capability| {
                capability.availability != Availability::Offline
                    && capability
                        .ports
                        .values()
                        .any(|port| port.direction == PortDirection::Source)
            })
            .expect("available Source Capability");
        let capability_id = source.id;
        let adapter_id = source.adapter_instance_id;
        let mut selected = BTreeSet::new();
        update_local_capability_selection(&node.runtime, &mut selected, capability_id, true)
            .expect("select Source");

        let mut replacement: Vec<_> = snapshot
            .local_node
            .capabilities
            .values()
            .filter(|capability| capability.adapter_instance_id == adapter_id)
            .cloned()
            .collect();
        replacement
            .iter_mut()
            .find(|capability| capability.id == capability_id)
            .expect("selected Source in Adapter catalog")
            .availability = Availability::Offline;
        node.runtime
            .replace_adapter_catalog(snapshot.local_node.id, adapter_id, replacement)
            .expect("replace local Adapter catalog");

        update_local_capability_selection(&node.runtime, &mut selected, capability_id, false)
            .expect("remove stale selection");
        assert!(selected.is_empty());
    }

    #[test]
    fn windows_preflight_single_flight_is_bounded_and_released_by_drop() {
        let flag = AtomicBool::new(false);
        let first = AtomicSingleFlight::acquire(&flag, "busy").expect("first permit");
        assert!(AtomicSingleFlight::acquire(&flag, "busy").is_err());
        drop(first);
        assert!(AtomicSingleFlight::acquire(&flag, "busy").is_ok());
    }

    #[test]
    fn loopback_worker_drives_runtime_active_and_stopped_states() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("loopback listener");
        let address = listener.local_addr().expect("loopback address");
        let server = thread::spawn(move || {
            let (accelerometer_stream, _) = listener.accept().expect("accelerometer connection");
            let mut accelerometer = accept(accelerometer_stream).expect("accelerometer handshake");
            let (gyroscope_stream, _) = listener.accept().expect("gyroscope connection");
            let mut gyroscope = accept(gyroscope_stream).expect("gyroscope handshake");
            for index in 0..12_u64 {
                let timestamp = 1_000_000_000 + index * 10_000_000;
                if accelerometer
                    .send(Message::Text(
                        serde_json::json!({
                            "accuracy": 3,
                            "timestamp": timestamp,
                            "values": [0.1 + index as f64, 0.2, 9.8]
                        })
                        .to_string()
                        .into(),
                    ))
                    .is_err()
                {
                    break;
                }
                if gyroscope
                    .send(Message::Text(
                        serde_json::json!({
                            "accuracy": 3,
                            "timestamp": timestamp + 1_000,
                            "values": [0.01, 0.02 + index as f64, 0.03]
                        })
                        .to_string()
                        .into(),
                    ))
                    .is_err()
                {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
        });
        let endpoint = SensorServerEndpoint::new(address.ip(), address.port()).expect("endpoint");
        let (mut node, route) = physical_node();
        let epoch = route
            .begin_start(&mut node.runtime, 1)
            .expect("begin physical Route");
        let route_runtime = Arc::new(Mutex::new(node));
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Mutex::new(
            UiLiveImu::idle(route, &route_runtime.lock().expect("Runtime lock").runtime)
                .expect("idle DTO"),
        ));
        let worker_stop = Arc::clone(&stop);
        let worker_shared = Arc::clone(&shared);
        let worker_runtime = Arc::clone(&route_runtime);
        let worker = thread::spawn(move || {
            run_live_imu(
                endpoint,
                epoch,
                &worker_stop,
                &worker_shared,
                &worker_runtime,
                route,
            )
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let received = shared.lock().expect("state lock").received_samples;
            if received >= 4 {
                break;
            }
            assert!(Instant::now() < deadline, "loopback worker timed out");
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            route.route_state(&route_runtime.lock().expect("Runtime lock").runtime),
            Ok(RouteState::Active)
        );
        stop.store(true, Ordering::Release);
        worker.join().expect("worker joins");
        route
            .stop(&mut route_runtime.lock().expect("Runtime lock").runtime)
            .expect("stop Route");
        assert_eq!(
            route.route_state(&route_runtime.lock().expect("Runtime lock").runtime),
            Ok(RouteState::Stopped)
        );
        server.join().expect("server joins");
    }

    #[test]
    fn loopback_connect_failure_offlines_route_and_retry_advances_epoch() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("loopback listener");
        let address = listener.local_addr().expect("loopback address");
        drop(listener);
        let endpoint = SensorServerEndpoint::new(address.ip(), address.port()).expect("endpoint");
        let (mut node, route) = physical_node();
        let first_epoch = route
            .begin_start(&mut node.runtime, 1)
            .expect("begin physical Route");
        let route_runtime = Arc::new(Mutex::new(node));
        let shared = Arc::new(Mutex::new(
            UiLiveImu::idle(route, &route_runtime.lock().expect("Runtime lock").runtime)
                .expect("idle DTO"),
        ));
        run_live_imu(
            endpoint,
            first_epoch,
            &Arc::new(AtomicBool::new(false)),
            &shared,
            &route_runtime,
            route,
        );
        let snapshot = shared.lock().expect("state lock").clone();
        assert_eq!(snapshot.status, "offline");
        assert_eq!(snapshot.route_state, "offline");
        assert_eq!(
            snapshot.problem_code.as_deref(),
            Some("CAPY.IMU.SENSORSERVER_DISCONNECTED")
        );
        let offline_epoch = route
            .route_epoch(&route_runtime.lock().expect("Runtime lock").runtime)
            .expect("offline epoch");
        assert!(offline_epoch > first_epoch);
        let retry_epoch = route
            .begin_start(&mut route_runtime.lock().expect("Runtime lock").runtime, 2)
            .expect("retry Route");
        assert!(retry_epoch > offline_epoch);
    }

    #[test]
    #[ignore = "requires explicitly configured physical SensorServer lab"]
    fn physical_live_imu_worker_updates_the_tauri_dto_and_stops_cleanly() {
        let ip = std::env::var("CAPYIO_LIVE_IMU_IP")
            .expect("CAPYIO_LIVE_IMU_IP must name the authorized physical lab")
            .parse::<IpAddr>()
            .expect("CAPYIO_LIVE_IMU_IP must be an IP literal");
        let port = std::env::var("CAPYIO_LIVE_IMU_PORT")
            .expect("CAPYIO_LIVE_IMU_PORT must name the authorized physical lab")
            .parse::<u16>()
            .expect("CAPYIO_LIVE_IMU_PORT must be a valid port");
        let endpoint = SensorServerEndpoint::new(ip, port).expect("valid physical endpoint");
        let (mut node, route) = physical_node();
        let epoch = route
            .begin_start(&mut node.runtime, 1)
            .expect("begin physical Route");
        let route_runtime = Arc::new(Mutex::new(node));
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Mutex::new(
            UiLiveImu::idle(route, &route_runtime.lock().expect("Runtime lock").runtime)
                .expect("idle DTO"),
        ));
        shared.lock().expect("state lock").stream_epoch = epoch;
        let worker_stop = Arc::clone(&stop);
        let worker_shared = Arc::clone(&shared);
        let worker_runtime = Arc::clone(&route_runtime);
        let worker = thread::spawn(move || {
            run_live_imu(
                endpoint,
                epoch,
                &worker_stop,
                &worker_shared,
                &worker_runtime,
                route,
            )
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = shared.lock().expect("state lock").clone();
            if snapshot.received_samples >= 4 {
                assert_eq!(snapshot.status, "active");
                assert!(snapshot.acceleration.is_some());
                assert!(snapshot.angular_velocity.is_some());
                assert!(snapshot.problem.is_none());
                break;
            }
            assert_ne!(snapshot.status, "offline", "{:?}", snapshot.problem);
            assert!(
                Instant::now() < deadline,
                "physical DTO did not receive four samples"
            );
            thread::sleep(Duration::from_millis(100));
        }
        stop.store(true, Ordering::Release);
        worker.join().expect("live worker joins");
        route
            .stop(&mut route_runtime.lock().expect("Runtime lock").runtime)
            .expect("stop physical Route");
        assert_eq!(
            route.route_state(&route_runtime.lock().expect("Runtime lock").runtime),
            Ok(RouteState::Stopped)
        );
    }
}
