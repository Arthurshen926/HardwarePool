use std::{
    str::FromStr,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use hardwarepool_core::{
    AudioQosMode, Availability, BindingState, CapabilityDetails, CapabilityId, CapabilityKind,
    PermissionRequirement, ProjectionKind,
};
use hardwarepool_testkit::DemoLab;
use serde::{Deserialize, Serialize};
use tauri::State;

struct AppState {
    lab: Mutex<DemoLab>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetProjectionRequest {
    capability_id: String,
    active: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiSnapshot {
    backend_mode: &'static str,
    schema_version: u8,
    project_version: &'static str,
    local_node_name: String,
    peers: Vec<UiPeer>,
    capabilities: Vec<UiCapability>,
    events: Vec<UiEvent>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiPeer {
    id: String,
    display_name: String,
    platform: String,
    platform_version: String,
    online: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiCapability {
    id: String,
    display_name: String,
    kind: &'static str,
    profile: String,
    permission_requirement: String,
    availability: String,
    projection_kind: Option<String>,
    binding_state: &'static str,
    active: bool,
    format_summary: Option<String>,
    qos_modes: Vec<String>,
    metrics: UiMetricSet,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UiMetricSet {
    estimated_latency_ms: Option<f64>,
    packet_loss_percent: Option<f64>,
    buffer_fill_ms: Option<f64>,
    underruns: u64,
    overruns: u64,
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
fn set_projection(
    request: SetProjectionRequest,
    state: State<'_, AppState>,
) -> Result<UiSnapshot, String> {
    let capability_id = CapabilityId::from_str(&request.capability_id)
        .map_err(|_| "invalid capability ID".to_owned())?;
    let mut lab = state.lab.lock().map_err(|_| "demo state lock poisoned")?;
    let now_ms = unix_time_ms()?;

    if capability_id == lab.microphone_capability_id {
        lab.set_microphone_active(request.active, now_ms)
            .map_err(|error| error.to_string())?;
    } else if capability_id == lab.speaker_capability_id {
        lab.set_speaker_active(request.active, now_ms)
            .map_err(|error| error.to_string())?;
    } else {
        return Err("the bootstrap UI only controls microphone and speaker".to_owned());
    }

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
            set_projection,
            reset_demo
        ])
        .run(tauri::generate_context!())
        .expect("run HardwarePool Tauri application");
}

fn to_ui_snapshot(lab: &DemoLab) -> UiSnapshot {
    let snapshot = lab.runtime.snapshot();
    let session = snapshot
        .sessions
        .iter()
        .find(|session| session.id == lab.session_id);

    let peers = snapshot
        .peers
        .iter()
        .map(|peer| UiPeer {
            id: peer.descriptor.id.to_string(),
            display_name: peer.descriptor.display_name.clone(),
            platform: platform_label(&peer.descriptor.platform),
            platform_version: peer.descriptor.platform_version.clone(),
            online: peer.online,
        })
        .collect();

    let capabilities = snapshot
        .peers
        .iter()
        .flat_map(|peer| peer.descriptor.capabilities.values())
        .filter(|capability| {
            matches!(
                &capability.kind,
                CapabilityKind::AudioCapture | CapabilityKind::AudioRender
            )
        })
        .map(|capability| {
            let binding = session.and_then(|session| session.bindings.get(&capability.id));
            let state = binding.map_or(BindingStateView::NotMapped, |binding| {
                BindingStateView::Core(binding.state)
            });
            let active = matches!(state, BindingStateView::Core(BindingState::Active));
            let projection_kind = binding
                .map(|binding| binding.projection_kind)
                .or_else(|| default_projection(&capability.kind));
            let (format_summary, qos_modes) = match &capability.details {
                CapabilityDetails::Audio(spec) => {
                    let format = spec.formats.first().map(format_summary);
                    let qos = spec
                        .qos_modes
                        .iter()
                        .copied()
                        .map(audio_qos_label)
                        .map(str::to_owned)
                        .collect();
                    (format, qos)
                }
                _ => (None, Vec::new()),
            };

            UiCapability {
                id: capability.id.to_string(),
                display_name: capability.display_name.clone(),
                kind: match &capability.kind {
                    CapabilityKind::AudioCapture => "audio_capture",
                    CapabilityKind::AudioRender => "audio_render",
                    _ => "audio_duplex_bundle",
                },
                profile: format!("{}/{}", capability.profile.name, capability.profile.major),
                permission_requirement: permission_label(capability.permission_requirement)
                    .to_owned(),
                availability: availability_label(capability.availability).to_owned(),
                projection_kind: projection_kind.map(projection_label).map(str::to_owned),
                binding_state: state.label(),
                active,
                format_summary,
                qos_modes,
                metrics: demo_metrics(&capability.kind, active),
            }
        })
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
        schema_version: 1,
        project_version: "0.1.0-bootstrap",
        local_node_name: snapshot.local_node.display_name,
        peers,
        capabilities,
        events,
        warnings: vec![
            "Tauri Demo 模式仍使用确定性 Rust Runtime；没有真实网络、音频或驱动。".to_owned(),
            "界面中的延迟、丢包和缓冲指标是模拟值。".to_owned(),
        ],
    }
}

#[derive(Clone, Copy)]
enum BindingStateView {
    NotMapped,
    Core(BindingState),
}

impl BindingStateView {
    const fn label(self) -> &'static str {
        match self {
            Self::NotMapped => "not_mapped",
            Self::Core(BindingState::Requested) => "requested",
            Self::Core(BindingState::Authorized) => "authorized",
            Self::Core(BindingState::Negotiated) => "negotiated",
            Self::Core(BindingState::Starting) => "starting",
            Self::Core(BindingState::Active) => "active",
            Self::Core(BindingState::Suspended) => "suspended",
            Self::Core(BindingState::Stopping) => "stopping",
            Self::Core(BindingState::Stopped) => "stopped",
            Self::Core(BindingState::Rejected) => "rejected",
            Self::Core(BindingState::Offline) => "offline",
            Self::Core(BindingState::Failed) => "failed",
        }
    }
}

fn default_projection(kind: &CapabilityKind) -> Option<ProjectionKind> {
    match kind {
        CapabilityKind::AudioCapture => Some(ProjectionKind::SystemCaptureEndpoint),
        CapabilityKind::AudioRender => Some(ProjectionKind::SystemRenderEndpoint),
        _ => None,
    }
}

fn permission_label(value: PermissionRequirement) -> &'static str {
    match value {
        PermissionRequirement::None => "none",
        PermissionRequirement::UserConfirmation => "user_confirmation",
        PermissionRequirement::ForegroundService => "foreground_service",
        PermissionRequirement::Privileged => "privileged",
    }
}

fn availability_label(value: Availability) -> &'static str {
    match value {
        Availability::Available => "available",
        Availability::Busy => "busy",
        Availability::PermissionRequired => "permission_required",
        Availability::Offline => "offline",
    }
}

fn projection_label(value: ProjectionKind) -> &'static str {
    match value {
        ProjectionKind::ApplicationStream => "application_stream",
        ProjectionKind::SystemCaptureEndpoint => "system_capture_endpoint",
        ProjectionKind::SystemRenderEndpoint => "system_render_endpoint",
        ProjectionKind::VirtualInputDevice => "virtual_input_device",
        ProjectionKind::VirtualDisplay => "virtual_display",
        ProjectionKind::RemoteComputeService => "remote_compute_service",
    }
}

fn audio_qos_label(value: AudioQosMode) -> &'static str {
    match value {
        AudioQosMode::MediaPlayback => "media_playback",
        AudioQosMode::VoiceInteractive => "voice_interactive",
        AudioQosMode::RawLan => "raw_lan",
        AudioQosMode::RawDuplex => "raw_duplex",
    }
}

fn platform_label(platform: &hardwarepool_core::Platform) -> String {
    match platform {
        hardwarepool_core::Platform::Unknown(detail) => detail.clone(),
        other => format!("{other:?}").to_lowercase(),
    }
}

fn format_summary(format: &hardwarepool_core::AudioFormat) -> String {
    format!(
        "{} kHz · {:?} · {} ch · {} ms",
        format.sample_rate_hz / 1_000,
        format.sample_format,
        format.channels,
        format.frame_duration_micros / 1_000
    )
}

fn demo_metrics(kind: &CapabilityKind, active: bool) -> UiMetricSet {
    if !active {
        return UiMetricSet {
            estimated_latency_ms: None,
            packet_loss_percent: None,
            buffer_fill_ms: None,
            underruns: 0,
            overruns: 0,
            simulated: true,
        };
    }

    let (latency, loss, buffer) = match kind {
        CapabilityKind::AudioCapture => (47.3, 0.03, 30.0),
        CapabilityKind::AudioRender => (63.8, 0.01, 45.0),
        _ => (0.0, 0.0, 0.0),
    };
    UiMetricSet {
        estimated_latency_ms: Some(latency),
        packet_loss_percent: Some(loss),
        buffer_fill_ms: Some(buffer),
        underruns: 0,
        overruns: 0,
        simulated: true,
    }
}

fn unix_time_ms() -> Result<u64, String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before UNIX epoch".to_owned())?;
    u64::try_from(elapsed.as_millis()).map_err(|_| "system time is out of range".to_owned())
}
