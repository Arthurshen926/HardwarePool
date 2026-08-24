use std::{
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

use capyio_core::{
    AdapterDeploymentMode, CapabilityClass, NodeDescriptor, OnlineState, PortDirection, PortRef,
    QosMode, Route, RouteId, RouteState,
};
use capyio_data_plane::{
    BoundedFanout, BoundedJsonlRecorder, ImuSampleV1, NumericImuPanel, RecorderOutcome,
    parse_imu_fixture_jsonl,
};
use capyio_sensor_server_adapter::{
    AssembleOutcome, SensorKind, SensorServerConnectionConfig, SensorServerEndpoint,
    SensorServerImuAssembler, SensorServerReadOutcome, SensorServerReading,
    SensorServerWebSocketClient,
};
use capyio_testkit::DemoLab;
use serde::{Deserialize, Serialize};
use tauri::State;

struct AppState {
    lab: Mutex<DemoLab>,
    live_imu: Arc<Mutex<UiLiveImu>>,
    live_imu_controller: Mutex<Option<LiveImuController>>,
}

impl Drop for AppState {
    fn drop(&mut self) {
        let Ok(controller) = self.live_imu_controller.get_mut() else {
            return;
        };
        if let Some(controller) = controller.take() {
            controller.stop.store(true, Ordering::Release);
            let _ = controller.worker.join();
        }
    }
}

struct LiveImuController {
    stop: Arc<AtomicBool>,
    worker: JoinHandle<()>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartLiveImuRequest {
    ip: String,
    port: u16,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiLiveImu {
    status: &'static str,
    simulated: bool,
    endpoint: Option<String>,
    profile: &'static str,
    stream_epoch: u64,
    sequence: Option<u64>,
    source_timestamp_nanos: Option<u64>,
    clock_domain_id: Option<String>,
    acceleration: Option<UiVector3>,
    angular_velocity: Option<UiVector3>,
    received_samples: u64,
    problem: Option<String>,
}

impl UiLiveImu {
    fn idle() -> Self {
        Self {
            status: "idle",
            simulated: false,
            endpoint: None,
            profile: "capyio.motion.imu-samples/1",
            stream_epoch: 0,
            sequence: None,
            source_timestamp_nanos: None,
            clock_domain_id: None,
            acceleration: None,
            angular_velocity: None,
            received_samples: 0,
            problem: None,
        }
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiSnapshot {
    backend_mode: &'static str,
    schema_version: u8,
    project_version: &'static str,
    nodes: Vec<UiNode>,
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
    let lab = state.lab.lock().map_err(|_| "demo state lock poisoned")?;
    Ok(to_ui_snapshot(&lab))
}

#[tauri::command]
fn set_route(request: SetRouteRequest, state: State<'_, AppState>) -> Result<UiSnapshot, String> {
    let route_id = RouteId::from_str(&request.route_id).map_err(|_| "invalid Route ID")?;
    let mut lab = state.lab.lock().map_err(|_| "demo state lock poisoned")?;
    lab.set_route_active(route_id, request.active, unix_time_ms()?)
        .map_err(|error| error.to_string())?;
    Ok(to_ui_snapshot(&lab))
}

#[tauri::command]
fn reset_demo(state: State<'_, AppState>) -> Result<UiSnapshot, String> {
    let mut lab = state.lab.lock().map_err(|_| "demo state lock poisoned")?;
    *lab = DemoLab::new().map_err(|error| error.to_string())?;
    Ok(to_ui_snapshot(&lab))
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
    let epoch = {
        let mut snapshot = state
            .live_imu
            .lock()
            .map_err(|_| "live IMU state lock poisoned".to_owned())?;
        let epoch = snapshot.stream_epoch.saturating_add(1).max(1);
        *snapshot = UiLiveImu {
            status: "connecting",
            simulated: false,
            endpoint: Some(endpoint.address().to_string()),
            profile: "capyio.motion.imu-samples/1",
            stream_epoch: epoch,
            sequence: None,
            source_timestamp_nanos: None,
            clock_domain_id: None,
            acceleration: None,
            angular_velocity: None,
            received_samples: 0,
            problem: None,
        };
        epoch
    };
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let shared = Arc::clone(&state.live_imu);
    let worker = thread::spawn(move || run_live_imu(endpoint, epoch, &worker_stop, &shared));
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
    let mut snapshot = state
        .live_imu
        .lock()
        .map_err(|_| "live IMU state lock poisoned".to_owned())?;
    if snapshot.status != "failed" {
        snapshot.status = "stopped";
    }
    Ok(snapshot.clone())
}

fn run_live_imu(
    endpoint: SensorServerEndpoint,
    epoch: u64,
    stop: &Arc<AtomicBool>,
    shared: &Arc<Mutex<UiLiveImu>>,
) {
    if let Err(problem) = run_live_imu_inner(endpoint, epoch, stop, shared) {
        stop.store(true, Ordering::Release);
        if let Ok(mut snapshot) = shared.lock() {
            snapshot.status = "failed";
            snapshot.problem = Some(problem);
        }
    } else if let Ok(mut snapshot) = shared.lock() {
        snapshot.status = "stopped";
    }
}

fn run_live_imu_inner(
    endpoint: SensorServerEndpoint,
    epoch: u64,
    stop: &Arc<AtomicBool>,
    shared: &Arc<Mutex<UiLiveImu>>,
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
                shared
                    .lock()
                    .map_err(|_| "live IMU state lock poisoned".to_owned())?
                    .status = "active";
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

pub fn run() {
    let lab = DemoLab::new().expect("valid deterministic demo lab");
    tauri::Builder::default()
        .manage(AppState {
            lab: Mutex::new(lab),
            live_imu: Arc::new(Mutex::new(UiLiveImu::idle())),
            live_imu_controller: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            set_route,
            reset_demo,
            get_live_imu,
            start_live_imu,
            stop_live_imu
        ])
        .run(tauri::generate_context!())
        .expect("run CapyIO Tauri application");
}

fn to_ui_snapshot(lab: &DemoLab) -> UiSnapshot {
    let snapshot = lab.runtime.snapshot();
    let all_nodes = std::iter::once(&snapshot.local_node).chain(snapshot.peers.iter());
    let nodes = all_nodes
        .clone()
        .map(|node| UiNode {
            id: node.id.to_string(),
            display_name: node.display_name.clone(),
            platform: platform_label(&node.platform),
            platform_version: node.platform_version.clone(),
            online: node.online_state == OnlineState::Online,
            local: node.id == snapshot.local_node.id,
            capability_count: node.capabilities.len(),
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
        schema_version: 3,
        project_version: env!("CARGO_PKG_VERSION"),
        nodes,
        routes,
        adapters,
        events,
        warnings: vec![
            "Tauri Demo 的 Route/授权/指标仍为确定性数据；只有显式启动的 Physical IMU Lab 会访问一个经校验的 IP 端点。".to_owned(),
            "四条 Route 的授权、指标与系统投影状态均为模拟数据。".to_owned(),
        ],
        imu_fixture: build_imu_fixture(),
    }
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
    use super::*;

    #[test]
    fn idle_live_imu_dto_is_explicitly_real_but_inactive() {
        let snapshot = UiLiveImu::idle();
        assert_eq!(snapshot.status, "idle");
        assert!(!snapshot.simulated);
        assert_eq!(snapshot.stream_epoch, 0);
        assert!(snapshot.acceleration.is_none());
        assert!(snapshot.problem.is_none());
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
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Mutex::new(UiLiveImu::idle()));
        shared.lock().expect("state lock").stream_epoch = 1;
        let worker_stop = Arc::clone(&stop);
        let worker_shared = Arc::clone(&shared);
        let worker = thread::spawn(move || run_live_imu(endpoint, 1, &worker_stop, &worker_shared));
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
            assert_ne!(snapshot.status, "failed", "{:?}", snapshot.problem);
            assert!(
                Instant::now() < deadline,
                "physical DTO did not receive four samples"
            );
            thread::sleep(Duration::from_millis(100));
        }
        stop.store(true, Ordering::Release);
        worker.join().expect("live worker joins");
        assert_eq!(shared.lock().expect("state lock").status, "stopped");
    }
}
