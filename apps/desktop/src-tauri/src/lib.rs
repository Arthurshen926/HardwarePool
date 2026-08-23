use std::{
    str::FromStr,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use capyio_core::{
    AdapterDeploymentMode, CapabilityClass, NodeDescriptor, OnlineState, PortDirection, PortRef,
    QosMode, Route, RouteId, RouteState,
};
use capyio_testkit::DemoLab;
use serde::{Deserialize, Serialize};
use tauri::State;

struct AppState {
    lab: Mutex<DemoLab>,
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

pub fn run() {
    let lab = DemoLab::new().expect("valid deterministic demo lab");
    tauri::Builder::default()
        .manage(AppState {
            lab: Mutex::new(lab),
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            set_route,
            reset_demo
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
        schema_version: 2,
        project_version: env!("CARGO_PKG_VERSION"),
        nodes,
        routes,
        adapters,
        events,
        warnings: vec![
            "Tauri Demo 使用确定性 Rust Runtime；没有访问真实摄像头、麦克风、传感器、网络或驱动。"
                .to_owned(),
            "四条 Route 的授权、指标与系统投影状态均为模拟数据。".to_owned(),
        ],
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
