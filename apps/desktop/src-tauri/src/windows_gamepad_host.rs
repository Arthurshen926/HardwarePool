use serde::{Deserialize, Serialize};
use std::sync::{
    Mutex,
    mpsc::{Receiver, SyncSender, TryRecvError, sync_channel},
};

use crate::android_gamepad::AndroidProjectionEvent;
use capyio_data_plane::{DataEnvelope, ImuSampleV1};
use capyio_input::GamepadState;
use capyio_runtime::NodeRuntime;

#[cfg(target_os = "windows")]
use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    path::PathBuf,
    time::Duration,
};

#[cfg(target_os = "windows")]
use capyio_core::{CapabilityId, NodeId, PortId, PortRef, RouteId, RouteState, SessionId};
#[cfg(target_os = "windows")]
use capyio_viiper_adapter::{
    MAX_VIIPER_MANAGEMENT_RESPONSE_BYTES, ViiperClientError, ViiperLoopbackClient,
    ViiperLoopbackConfig,
};
#[cfg(target_os = "windows")]
use capyio_viiper_adapter::{
    ViiperAutoAttachDisabled, ViiperDs4ControlsMapping, ViiperDs4MotionMapping,
};
#[cfg(target_os = "windows")]
use capyio_windows_input::{
    MAX_USBIP_OUTPUT_BYTES, UsbipWin2Client, UsbipWin2Config, UsbipWin2DeploymentVerified,
    UsbipWin2Error, VigemX360Companion, VigemX360SidecarConfig, ViiperDs4RouteController,
};

const VIIPER_ENDPOINT: &str = "127.0.0.1:3242";
const USBIP_ENDPOINT: &str = "127.0.0.1:3241";

#[cfg(target_os = "windows")]
fn vigem_sidecar_path() -> Result<PathBuf, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not locate the desktop executable: {error}"))?;
    let directory = executable
        .parent()
        .ok_or_else(|| "desktop executable has no parent directory".to_owned())?;
    let sidecar = directory.join("CapyIO.ViGEmX360Sidecar.exe");
    if !sidecar.is_file() {
        return Err(format!(
            "ViGEm Xbox compatibility sidecar is unavailable: {}",
            sidecar.display()
        ));
    }
    let managed_client = directory.join("Nefarius.ViGEm.Client.dll");
    if !managed_client.is_file() {
        return Err(format!(
            "ViGEm Xbox compatibility runtime is unavailable: {}",
            managed_client.display()
        ));
    }
    Ok(sidecar)
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UiWindowsControllerKind {
    Xbox360,
    #[serde(rename = "dualshock4", alias = "dual_shock4")]
    DualShock4,
}

impl UiWindowsControllerKind {
    const fn identity(self) -> &'static str {
        match self {
            Self::Xbox360 => "Xbox 360 Controller · 045e:028e",
            Self::DualShock4 => "DualShock 4 · 054c:09cc · native motion",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UiWindowsGamepadProjection {
    pub(crate) supported: bool,
    pub(crate) status: &'static str,
    pub(crate) controller_kind: UiWindowsControllerKind,
    pub(crate) device_identity: &'static str,
    pub(crate) viiper_endpoint: Option<&'static str>,
    pub(crate) usbip_endpoint: Option<&'static str>,
    pub(crate) viiper_ready: bool,
    pub(crate) usbip_ready: bool,
    pub(crate) xinput_available: bool,
    pub(crate) xinput_ready: bool,
    pub(crate) export_count: usize,
    pub(crate) bus_id: Option<String>,
    pub(crate) owned_usbip_port: Option<u8>,
    pub(crate) input_packets: u64,
    pub(crate) non_neutral_packets: u64,
    pub(crate) ds4_rejected_packets: u64,
    pub(crate) xinput_packets: u64,
    pub(crate) input_offline_events: u64,
    pub(crate) last_remote_sequence: Option<u64>,
    pub(crate) last_event: &'static str,
    pub(crate) problem_code: Option<&'static str>,
    pub(crate) problem: Option<&'static str>,
}

pub(crate) struct WindowsGamepadHost {
    projection: UiWindowsGamepadProjection,
    input_sender: SyncSender<AndroidProjectionEvent>,
    input: Mutex<WindowsInputIngress>,
    #[cfg(target_os = "windows")]
    controller: Option<ViiperDs4RouteController>,
    #[cfg(target_os = "windows")]
    xinput_companion: Option<VigemX360Companion>,
}

struct WindowsInputIngress {
    receiver: Receiver<AndroidProjectionEvent>,
    packets: u64,
    non_neutral_packets: u64,
    offline_events: u64,
    last_remote_sequence: Option<u64>,
}

impl WindowsGamepadHost {
    pub(crate) fn new() -> Self {
        let (input_sender, receiver) = sync_channel(8);
        Self {
            projection: projection_from_outcome(
                UiWindowsControllerKind::DualShock4,
                initial_outcome(),
            ),
            input_sender,
            input: Mutex::new(WindowsInputIngress {
                receiver,
                packets: 0,
                non_neutral_packets: 0,
                offline_events: 0,
                last_remote_sequence: None,
            }),
            #[cfg(target_os = "windows")]
            controller: None,
            #[cfg(target_os = "windows")]
            xinput_companion: None,
        }
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn install(
        runtime: &mut NodeRuntime,
        session_id: SessionId,
    ) -> Result<Self, String> {
        use std::str::FromStr;

        let mut host = Self::new();
        let viiper = ViiperLoopbackClient::new(
            ViiperLoopbackConfig::new(
                SocketAddr::from((Ipv4Addr::LOCALHOST, 3242)),
                Duration::from_secs(2),
                Duration::from_secs(2),
                MAX_VIIPER_MANAGEMENT_RESPONSE_BYTES,
            )
            .map_err(|error| error.to_string())?,
        );
        let usbip = UsbipWin2Client::new(
            UsbipWin2Config::new(
                PathBuf::from(r"C:\Program Files\USBip\usbip.exe"),
                SocketAddrV4::new(Ipv4Addr::LOCALHOST, 3241),
                Duration::from_secs(5),
                MAX_USBIP_OUTPUT_BYTES,
            )
            .map_err(|error| error.to_string())?,
        );
        let android_node =
            NodeId::from_str(capyio_testkit::ANDROID_NODE_ID).map_err(|error| error.to_string())?;
        let source = |capability: &str, port: &str| -> Result<PortRef, String> {
            Ok(PortRef {
                node_id: android_node,
                capability_id: capability
                    .parse::<CapabilityId>()
                    .map_err(|error| error.to_string())?,
                port_id: port.parse::<PortId>().map_err(|error| error.to_string())?,
            })
        };
        host.controller = Some(ViiperDs4RouteController::install_with_usbip(
            runtime,
            session_id,
            RouteId::new(),
            source(
                "00000000-0000-4000-8000-000000000205",
                "00000000-0000-4000-8000-000000001205",
            )?,
            RouteId::new(),
            source(
                "00000000-0000-4000-8000-000000000204",
                "00000000-0000-4000-8000-000000001204",
            )?,
            viiper,
            ViiperAutoAttachDisabled::confirmed_by_caller(),
            ViiperDs4ControlsMapping::gamepad_y_up(),
            ViiperDs4MotionMapping::android_landscape_to_ds4(),
            usbip,
            UsbipWin2DeploymentVerified::confirmed_by_caller(),
        )?);
        host.projection.last_event = "host_ready";
        Ok(host)
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn install(
        _runtime: &mut NodeRuntime,
        _session_id: capyio_core::SessionId,
    ) -> Result<Self, String> {
        Ok(Self::new())
    }

    pub(crate) fn snapshot(&self) -> UiWindowsGamepadProjection {
        let mut projection = self.projection.clone();
        #[cfg(target_os = "windows")]
        let metrics_only = self.controller.is_none();
        #[cfg(not(target_os = "windows"))]
        let metrics_only = true;
        if let Ok(mut input) = self.input.lock() {
            if metrics_only {
                loop {
                    match input.receiver.try_recv() {
                        Ok(AndroidProjectionEvent::State(state)) => {
                            input.packets = input.packets.saturating_add(1);
                            if state.controls.controls != capyio_input::GamepadControls::neutral() {
                                input.non_neutral_packets =
                                    input.non_neutral_packets.saturating_add(1);
                            }
                            input.last_remote_sequence = Some(state.remote_sequence);
                        }
                        Ok(AndroidProjectionEvent::UpstreamOffline) => {
                            input.offline_events = input.offline_events.saturating_add(1);
                        }
                        Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                    }
                }
            }
            projection.input_packets = input.packets;
            projection.non_neutral_packets = input.non_neutral_packets;
            projection.input_offline_events = input.offline_events;
            projection.last_remote_sequence = input.last_remote_sequence;
        }
        projection
    }

    pub(crate) fn is_active(&self) -> bool {
        self.projection.status == "active"
    }

    fn record_ds4_rejection(&mut self) {
        self.projection.ds4_rejected_packets =
            self.projection.ds4_rejected_packets.saturating_add(1);
        self.projection.last_event = "ds4_state_rejected";
    }

    pub(crate) fn start(
        &mut self,
        runtime: &mut NodeRuntime,
        mut controls: GamepadState,
        mut motion: DataEnvelope<ImuSampleV1>,
        now_ms: u64,
        enable_xinput_companion: bool,
    ) -> Result<(), String> {
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (runtime, controls, motion, now_ms, enable_xinput_companion);
            return Err("Windows DS4 projection is unsupported on this platform".to_owned());
        }
        #[cfg(target_os = "windows")]
        {
            let controller = self
                .controller
                .as_mut()
                .ok_or_else(|| "Windows DS4 projection host was not installed".to_owned())?;
            let mut xinput = if enable_xinput_companion {
                let config = VigemX360SidecarConfig::new(vigem_sidecar_path()?)
                    .map_err(|error| error.to_string())?;
                Some(VigemX360Companion::start(config).map_err(|error| error.to_string())?)
            } else {
                None
            };
            let initial_xinput = controls.controls;
            let epochs = match controller.begin_start(runtime, now_ms) {
                Ok(epochs) => epochs,
                Err(error) => {
                    if let Some(companion) = xinput {
                        let _ = companion.stop();
                    }
                    return Err(error);
                }
            };
            controls.header.stream_epoch = epochs.controls;
            motion.stream_epoch = epochs.motion;
            if let Err(error) = controller.activate(runtime, controls, &motion) {
                if let Some(companion) = xinput {
                    let _ = companion.stop();
                }
                return Err(error);
            }
            if let Some(companion) = xinput.as_mut()
                && let Err(error) = companion.submit(initial_xinput)
            {
                if let Some(companion) = xinput {
                    let _ = companion.stop();
                }
                let _ = controller.stop(runtime);
                return Err(error.to_string());
            }
            let status = match controller.status(runtime) {
                Ok(status) => status,
                Err(error) => {
                    if let Some(companion) = xinput {
                        let _ = companion.stop();
                    }
                    let _ = controller.stop(runtime);
                    return Err(error);
                }
            };
            self.xinput_companion = xinput;
            self.projection.status = "active";
            self.projection.viiper_ready = true;
            self.projection.usbip_ready = true;
            self.projection.xinput_available = vigem_sidecar_path().is_ok();
            self.projection.xinput_ready = enable_xinput_companion;
            self.projection.bus_id = status.bus_id.map(|bus| bus.to_string());
            self.projection.owned_usbip_port = status.usbip_port;
            self.projection.last_event = if enable_xinput_companion {
                "ds4_xinput_projection_active"
            } else {
                "ds4_projection_active"
            };
            self.projection.problem_code = None;
            self.projection.problem = None;
            Ok(())
        }
    }

    pub(crate) fn stop(&mut self, runtime: &mut NodeRuntime) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        let xinput_error = self
            .xinput_companion
            .take()
            .and_then(|companion| companion.stop().err())
            .map(|error| error.to_string());
        #[cfg(target_os = "windows")]
        let controller_error = self
            .controller
            .as_mut()
            .and_then(|controller| controller.stop(runtime).err());
        self.projection.status = "stopped";
        self.projection.xinput_ready = false;
        self.projection.bus_id = None;
        self.projection.owned_usbip_port = None;
        self.projection.last_event = "ds4_xinput_projection_stopped";
        #[cfg(target_os = "windows")]
        if let Some(error) = xinput_error.or(controller_error) {
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn poll(&mut self, runtime: &mut NodeRuntime) {
        let mut events = Vec::with_capacity(8);
        if let Ok(input) = self.input.lock() {
            while let Ok(event) = input.receiver.try_recv() {
                events.push(event);
            }
        }
        for event in events {
            match event {
                AndroidProjectionEvent::State(mut state) => {
                    #[cfg(target_os = "windows")]
                    let xinput_controls = state.controls.controls;
                    if let Ok(mut input) = self.input.lock() {
                        input.packets = input.packets.saturating_add(1);
                        if state.controls.controls != capyio_input::GamepadControls::neutral() {
                            input.non_neutral_packets = input.non_neutral_packets.saturating_add(1);
                        }
                        input.last_remote_sequence = Some(state.remote_sequence);
                    }
                    #[cfg(target_os = "windows")]
                    let mut ds4_accepted = false;
                    #[cfg(target_os = "windows")]
                    if self.is_active()
                        && let Some(controller) = self.controller.as_mut()
                        && let Ok(status) = controller.status(runtime)
                    {
                        state.controls.header.stream_epoch = status.epochs.controls;
                        state.motion.stream_epoch = status.epochs.motion;
                        match controller.submit(runtime, state.controls, &state.motion) {
                            Ok(outcome) if outcome.exhausted() => {
                                if let Some(companion) = self.xinput_companion.take() {
                                    let _ = companion.stop();
                                }
                                self.projection.status = "offline";
                                self.projection.xinput_ready = false;
                                self.projection.bus_id = None;
                                self.projection.owned_usbip_port = None;
                                self.projection.last_event = "ds4_sequence_exhausted";
                                self.projection.problem_code =
                                    Some("CAPY.GAMEPAD.DS4_SEQUENCE_EXHAUSTED");
                                self.projection.problem =
                                    Some("DualShock 4 输入序列已耗尽，联合投影已安全停止。");
                            }
                            Ok(_) => ds4_accepted = true,
                            Err(_) => {
                                let still_active = controller.status(runtime).is_ok_and(|status| {
                                    status.controls_state == RouteState::Active
                                        && status.motion_state == RouteState::Active
                                });
                                if still_active {
                                    self.record_ds4_rejection();
                                } else {
                                    if let Some(companion) = self.xinput_companion.take() {
                                        let _ = companion.stop();
                                    }
                                    self.projection.status = "failed";
                                    self.projection.xinput_ready = false;
                                    self.projection.bus_id = None;
                                    self.projection.owned_usbip_port = None;
                                    self.projection.last_event = "ds4_submit_failed";
                                    self.projection.problem_code =
                                        Some("CAPY.GAMEPAD.DS4_STREAM_FAILED");
                                    self.projection.problem =
                                        Some("DualShock 4 输入流已失败，联合投影已安全停止。");
                                }
                            }
                        }
                    }
                    #[cfg(target_os = "windows")]
                    if ds4_accepted
                        && self.is_active()
                        && let Some(companion) = self.xinput_companion.as_mut()
                    {
                        match companion.submit(xinput_controls) {
                            Ok(()) => {
                                self.projection.xinput_packets =
                                    self.projection.xinput_packets.saturating_add(1);
                            }
                            Err(_) => {
                                if let Some(companion) = self.xinput_companion.take() {
                                    let _ = companion.stop();
                                }
                                if let Some(controller) = self.controller.as_mut() {
                                    let _ = controller.stop(runtime);
                                }
                                self.projection.status = "failed";
                                self.projection.xinput_ready = false;
                                self.projection.bus_id = None;
                                self.projection.owned_usbip_port = None;
                                self.projection.last_event = "xinput_submit_failed";
                                self.projection.problem_code =
                                    Some("CAPY.GAMEPAD.XINPUT_STREAM_FAILED");
                                self.projection.problem =
                                    Some("Xbox 兼容输入流已失败，联合投影已安全停止。");
                            }
                        }
                    }
                }
                AndroidProjectionEvent::UpstreamOffline => {
                    if let Ok(mut input) = self.input.lock() {
                        input.offline_events = input.offline_events.saturating_add(1);
                    }
                    #[cfg(target_os = "windows")]
                    if self.is_active()
                        && let Some(controller) = self.controller.as_mut()
                    {
                        let _ = controller.report_controls_offline(
                            runtime,
                            "Android paired controls and IMU source disconnected",
                        );
                        self.projection.status = "offline";
                        self.projection.bus_id = None;
                        self.projection.owned_usbip_port = None;
                        if let Some(companion) = self.xinput_companion.take() {
                            let _ = companion.stop();
                        }
                        self.projection.xinput_ready = false;
                        self.projection.last_event = "android_source_offline";
                    }
                }
            }
        }
    }

    pub(crate) fn input_sender(&self) -> SyncSender<AndroidProjectionEvent> {
        self.input_sender.clone()
    }

    pub(crate) fn apply_read_only_preflight(&mut self, projection: UiWindowsGamepadProjection) {
        self.projection = projection;
    }
}

/// Performs the bounded fixed-configuration probe without holding desktop
/// gamepad state. The Tauri command runs this on a blocking worker, then
/// publishes the finished immutable DTO under the short-lived state lock.
///
/// The probe checks the pinned VIIPER identity, the pinned usbip-win2 CLI and
/// the current Xbox export list. It cannot create a VIIPER bus, attach USB/IP,
/// start/reconfigure VIIPER or a persistent helper, install/remove a driver,
/// restart Windows or change boot policy.
pub(crate) fn probe_read_only(kind: UiWindowsControllerKind) -> UiWindowsGamepadProjection {
    let mut projection = projection_from_outcome(kind, run_read_only_probe(kind));
    #[cfg(target_os = "windows")]
    {
        projection.xinput_available = vigem_sidecar_path().is_ok();
    }
    projection
}

#[cfg(debug_assertions)]
pub(crate) fn run_read_only_preflight_gate() -> Result<(), String> {
    let projection = probe_read_only(UiWindowsControllerKind::DualShock4);
    println!("CAPYIO_WINDOWS_GAMEPAD_STATUS={}", projection.status);
    println!(
        "CAPYIO_WINDOWS_GAMEPAD_VIIPER_READY={}",
        projection.viiper_ready
    );
    println!(
        "CAPYIO_WINDOWS_GAMEPAD_USBIP_READY={}",
        projection.usbip_ready
    );
    println!(
        "CAPYIO_WINDOWS_GAMEPAD_XINPUT_AVAILABLE={}",
        projection.xinput_available
    );
    println!(
        "CAPYIO_WINDOWS_GAMEPAD_EXPORT_COUNT={}",
        projection.export_count
    );
    println!(
        "CAPYIO_WINDOWS_GAMEPAD_BUS_ID={}",
        projection.bus_id.as_deref().unwrap_or("none")
    );
    println!(
        "CAPYIO_WINDOWS_GAMEPAD_OWNED_PORT={}",
        projection
            .owned_usbip_port
            .map_or_else(|| "none".to_owned(), |port| port.to_string())
    );
    println!("CAPYIO_WINDOWS_GAMEPAD_LAST={}", projection.last_event);
    println!(
        "CAPYIO_WINDOWS_GAMEPAD_PROBLEM={}",
        projection.problem_code.unwrap_or("none")
    );
    if matches!(projection.status, "failed" | "offline") {
        return Err(projection
            .problem
            .unwrap_or("Windows gamepad read-only preflight failed")
            .to_owned());
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ProbeOutcome {
    Unsupported,
    NotChecked,
    NoExport,
    ExportReady(String),
    AmbiguousExports(usize),
    ViiperUnavailable,
    ViiperIncompatible,
    UsbipCliUnavailable,
    UsbipIncompatible,
    UsbipServerUnavailable,
    UsbipResponseInvalid,
    ConfigurationInvalid,
}

fn initial_outcome() -> ProbeOutcome {
    if cfg!(target_os = "windows") {
        ProbeOutcome::NotChecked
    } else {
        ProbeOutcome::Unsupported
    }
}

fn projection_from_outcome(
    kind: UiWindowsControllerKind,
    outcome: ProbeOutcome,
) -> UiWindowsGamepadProjection {
    let supported = !matches!(outcome, ProbeOutcome::Unsupported);
    let mut projection = UiWindowsGamepadProjection {
        supported,
        status: if supported {
            "host_gate_required"
        } else {
            "unsupported"
        },
        controller_kind: kind,
        device_identity: kind.identity(),
        viiper_endpoint: supported.then_some(VIIPER_ENDPOINT),
        usbip_endpoint: supported.then_some(USBIP_ENDPOINT),
        viiper_ready: false,
        usbip_ready: false,
        xinput_available: false,
        xinput_ready: false,
        export_count: 0,
        bus_id: None,
        owned_usbip_port: None,
        input_packets: 0,
        non_neutral_packets: 0,
        ds4_rejected_packets: 0,
        xinput_packets: 0,
        input_offline_events: 0,
        last_remote_sequence: None,
        last_event: "not_checked",
        problem_code: None,
        problem: None,
    };
    match outcome {
        ProbeOutcome::Unsupported => projection.last_event = "unsupported_platform",
        ProbeOutcome::NotChecked => {}
        ProbeOutcome::NoExport => {
            projection.viiper_ready = true;
            projection.usbip_ready = true;
            projection.last_event = "read_only_preflight_no_export";
        }
        ProbeOutcome::ExportReady(bus_id) => {
            projection.status = "export_ready";
            projection.viiper_ready = true;
            projection.usbip_ready = true;
            projection.export_count = 1;
            projection.bus_id = Some(bus_id);
            projection.last_event = "read_only_preflight_passed";
        }
        ProbeOutcome::AmbiguousExports(count) => {
            projection.status = "failed";
            projection.viiper_ready = true;
            projection.usbip_ready = true;
            projection.export_count = count;
            projection.last_event = match kind {
                UiWindowsControllerKind::Xbox360 => "multiple_xbox_exports_rejected",
                UiWindowsControllerKind::DualShock4 => "multiple_ds4_exports_rejected",
            };
            projection.problem_code = Some("CAPY.GAMEPAD.USBIP_EXPORT_AMBIGUOUS");
            projection.problem = Some(match kind {
                UiWindowsControllerKind::Xbox360 => {
                    "检测到多个 Xbox 360 导出；为避免选择错误设备，预检已关闭后续附加动作。"
                }
                UiWindowsControllerKind::DualShock4 => {
                    "检测到多个 DualShock 4 导出；为避免选择错误设备，预检已关闭后续附加动作。"
                }
            });
        }
        ProbeOutcome::ViiperUnavailable => {
            projection.status = "offline";
            projection.last_event = "viiper_unavailable";
            projection.problem_code = Some("CAPY.GAMEPAD.VIIPER_UNAVAILABLE");
            projection.problem = Some("VIIPER v0.7.0 未在固定回环端点响应。");
        }
        ProbeOutcome::ViiperIncompatible => {
            projection.status = "failed";
            projection.last_event = "viiper_incompatible";
            projection.problem_code = Some("CAPY.GAMEPAD.VIIPER_INCOMPATIBLE");
            projection.problem = Some("固定回环端点返回了不兼容的 VIIPER 身份或版本。");
        }
        ProbeOutcome::UsbipCliUnavailable => {
            projection.status = "offline";
            projection.viiper_ready = true;
            projection.last_event = "usbip_cli_unavailable";
            projection.problem_code = Some("CAPY.GAMEPAD.USBIP_CLI_UNAVAILABLE");
            projection.problem = Some("固定位置的 usbip-win2 v0.9.7.7 CLI 当前不可用。");
        }
        ProbeOutcome::UsbipIncompatible => {
            projection.status = "failed";
            projection.viiper_ready = true;
            projection.last_event = "usbip_incompatible";
            projection.problem_code = Some("CAPY.GAMEPAD.USBIP_INCOMPATIBLE");
            projection.problem = Some("usbip-win2 CLI 版本与固定的 v0.9.7.7 契约不兼容。");
        }
        ProbeOutcome::UsbipServerUnavailable => {
            projection.status = "offline";
            projection.viiper_ready = true;
            projection.last_event = "usbip_server_unavailable";
            projection.problem_code = Some("CAPY.GAMEPAD.USBIP_SERVER_UNAVAILABLE");
            projection.problem = Some("USB/IP 回环服务当前未返回导出列表。");
        }
        ProbeOutcome::UsbipResponseInvalid => {
            projection.status = "failed";
            projection.viiper_ready = true;
            projection.last_event = "usbip_response_invalid";
            projection.problem_code = Some("CAPY.GAMEPAD.USBIP_RESPONSE_INVALID");
            projection.problem = Some("USB/IP 回环服务返回了无法安全接受的有界响应。");
        }
        ProbeOutcome::ConfigurationInvalid => {
            projection.status = "failed";
            projection.last_event = "host_configuration_invalid";
            projection.problem_code = Some("CAPY.GAMEPAD.HOST_CONFIGURATION_INVALID");
            projection.problem = Some("内置 Windows 手柄主机配置未通过固定边界校验。");
        }
    }
    projection
}

#[cfg(target_os = "windows")]
fn run_read_only_probe(kind: UiWindowsControllerKind) -> ProbeOutcome {
    let viiper_config = match ViiperLoopbackConfig::new(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 3242)),
        Duration::from_secs(2),
        Duration::from_secs(2),
        MAX_VIIPER_MANAGEMENT_RESPONSE_BYTES,
    ) {
        Ok(config) => config,
        Err(_) => return ProbeOutcome::ConfigurationInvalid,
    };
    if let Err(error) = ViiperLoopbackClient::new(viiper_config).probe() {
        return classify_viiper_error(&error);
    }

    let usbip_config = match UsbipWin2Config::new(
        PathBuf::from(r"C:\Program Files\USBip\usbip.exe"),
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, 3241),
        Duration::from_secs(5),
        MAX_USBIP_OUTPUT_BYTES,
    ) {
        Ok(config) => config,
        Err(_) => return ProbeOutcome::ConfigurationInvalid,
    };
    let client = UsbipWin2Client::new(usbip_config);
    let list_result = match kind {
        UiWindowsControllerKind::Xbox360 => client.list_xbox360_exports(),
        UiWindowsControllerKind::DualShock4 => client.list_dualshock4_exports(),
    };
    let exports = match list_result {
        Ok(exports) => exports,
        Err(error) => return classify_usbip_error(&error),
    };
    match exports.as_slice() {
        [] => ProbeOutcome::NoExport,
        [export] => ProbeOutcome::ExportReady(export.bus_id().as_str().to_owned()),
        _ => ProbeOutcome::AmbiguousExports(exports.len()),
    }
}

#[cfg(not(target_os = "windows"))]
fn run_read_only_probe(_kind: UiWindowsControllerKind) -> ProbeOutcome {
    ProbeOutcome::Unsupported
}

#[cfg(target_os = "windows")]
fn classify_viiper_error(error: &ViiperClientError) -> ProbeOutcome {
    match error {
        ViiperClientError::UnexpectedServer(_) | ViiperClientError::UnsupportedVersion(_) => {
            ProbeOutcome::ViiperIncompatible
        }
        ViiperClientError::NonLoopbackAddress(_)
        | ViiperClientError::InvalidPort
        | ViiperClientError::InvalidTimeout
        | ViiperClientError::InvalidResponseLimit { .. } => ProbeOutcome::ConfigurationInvalid,
        _ => ProbeOutcome::ViiperUnavailable,
    }
}

#[cfg(target_os = "windows")]
fn classify_usbip_error(error: &UsbipWin2Error) -> ProbeOutcome {
    match error {
        UsbipWin2Error::SpawnFailed(_) => ProbeOutcome::UsbipCliUnavailable,
        UsbipWin2Error::UnsupportedVersion(_) => ProbeOutcome::UsbipIncompatible,
        UsbipWin2Error::CommandFailed { operation, .. }
        | UsbipWin2Error::CommandTimedOut(operation)
            if *operation == "export list" =>
        {
            ProbeOutcome::UsbipServerUnavailable
        }
        UsbipWin2Error::ExecutablePathNotAbsolute(_)
        | UsbipWin2Error::UnexpectedExecutableName(_)
        | UsbipWin2Error::NonLoopbackServer(_)
        | UsbipWin2Error::InvalidServerPort
        | UsbipWin2Error::InvalidCommandTimeout(_)
        | UsbipWin2Error::InvalidOutputLimit { .. } => ProbeOutcome::ConfigurationInvalid,
        _ => ProbeOutcome::UsbipResponseInvalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_export_is_ready_but_does_not_claim_windows_enumeration() {
        let projection =
            projection_from_outcome(UiWindowsControllerKind::DualShock4, ProbeOutcome::NoExport);
        assert_eq!(projection.status, "host_gate_required");
        assert!(projection.viiper_ready);
        assert!(projection.usbip_ready);
        assert!(!projection.xinput_ready);
        assert_eq!(projection.export_count, 0);
        assert!(projection.bus_id.is_none());
        assert!(projection.owned_usbip_port.is_none());
        assert!(projection.problem_code.is_none());
    }

    #[test]
    fn one_export_is_visible_without_claiming_an_owned_attachment() {
        let projection = projection_from_outcome(
            UiWindowsControllerKind::DualShock4,
            ProbeOutcome::ExportReady("7-3".to_owned()),
        );
        assert_eq!(
            projection.controller_kind,
            UiWindowsControllerKind::DualShock4
        );
        assert!(projection.device_identity.contains("native motion"));
        assert_eq!(projection.status, "export_ready");
        assert_eq!(projection.bus_id.as_deref(), Some("7-3"));
        assert_eq!(projection.export_count, 1);
        assert!(projection.owned_usbip_port.is_none());
        assert_eq!(projection.last_event, "read_only_preflight_passed");
    }

    #[test]
    fn ambiguous_exports_fail_closed_with_a_stable_sanitized_problem() {
        let projection = projection_from_outcome(
            UiWindowsControllerKind::DualShock4,
            ProbeOutcome::AmbiguousExports(2),
        );
        assert_eq!(projection.status, "failed");
        assert_eq!(projection.export_count, 2);
        assert_eq!(
            projection.problem_code,
            Some("CAPY.GAMEPAD.USBIP_EXPORT_AMBIGUOUS")
        );
        assert!(!projection.problem.unwrap().contains(r"C:\"));
    }

    #[test]
    fn dependency_failures_preserve_partial_readiness() {
        let viiper = projection_from_outcome(
            UiWindowsControllerKind::DualShock4,
            ProbeOutcome::ViiperUnavailable,
        );
        assert!(!viiper.viiper_ready);
        assert!(!viiper.usbip_ready);
        assert_eq!(viiper.status, "offline");

        let usbip = projection_from_outcome(
            UiWindowsControllerKind::DualShock4,
            ProbeOutcome::UsbipCliUnavailable,
        );
        assert!(usbip.viiper_ready);
        assert!(!usbip.usbip_ready);
        assert_eq!(usbip.status, "offline");
    }

    #[test]
    fn selector_keeps_xbox_and_ds4_identity_explicit() {
        let xbox =
            projection_from_outcome(UiWindowsControllerKind::Xbox360, ProbeOutcome::NoExport);
        let ds4 =
            projection_from_outcome(UiWindowsControllerKind::DualShock4, ProbeOutcome::NoExport);
        assert_ne!(xbox.device_identity, ds4.device_identity);
        assert_eq!(xbox.controller_kind, UiWindowsControllerKind::Xbox360);
        assert_eq!(ds4.controller_kind, UiWindowsControllerKind::DualShock4);
    }

    #[test]
    fn controller_kind_accepts_the_public_webview_ds4_token() {
        let kind: UiWindowsControllerKind =
            serde_json::from_str("\"dualshock4\"").expect("public DS4 token should deserialize");
        assert_eq!(kind, UiWindowsControllerKind::DualShock4);
    }

    #[test]
    fn rejected_ds4_frame_is_counted_without_ending_an_active_projection() {
        let mut host = WindowsGamepadHost::new();
        host.projection.status = "active";
        host.projection.xinput_ready = true;

        host.record_ds4_rejection();

        assert_eq!(host.projection.status, "active");
        assert!(host.projection.xinput_ready);
        assert_eq!(host.projection.ds4_rejected_packets, 1);
        assert_eq!(host.projection.last_event, "ds4_state_rejected");
    }
}
