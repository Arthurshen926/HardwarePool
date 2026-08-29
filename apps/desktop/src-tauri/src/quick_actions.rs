use std::{
    collections::BTreeMap,
    env,
    net::IpAddr,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use capyio_audio_share_adapter::{
    AudioEncoding, AudioShareConfig, AudioShareProbe, AudioShareSupervisor, ProbeLimits,
    ProcessExitReport, ProcessOutputSummary, ReceiverTcpPresence, SupervisorLimits,
    SupervisorStatus,
};
use capyio_core::{Problem, RouteId, RouteState, SessionId};
use capyio_runtime::NodeRuntime;
use serde::{Deserialize, Serialize};

use crate::audio_share_runtime::{
    AudioShareProcessBoundary, AudioShareRouteController, AudioShareStartError,
    DEFAULT_RECEIVER_WAIT_POLLS, DEFAULT_STABLE_RECEIVER_POLLS,
};

#[cfg(windows)]
use capyio_windows_service::{BrokerServiceClient, BrokerServiceSnapshot, BrokerServiceState};

pub const QUICK_ACTION_SCHEMA_VERSION: u8 = 2;
pub const AUDIO_SHARE_ACTION_ID: &str = "capyio.quick-action.remote-speaker";

const ENV_EXE: &str = "CAPYIO_AUDIO_SHARE_EXE";
const ENV_VIRTUAL_SPEAKER_EXE: &str = "CAPYIO_VIRTUAL_SPEAKER_EXE";
const ENV_BIND_IP: &str = "CAPYIO_AUDIO_SHARE_BIND_IP";
const ENV_PORT: &str = "CAPYIO_AUDIO_SHARE_PORT";
const ENV_ENDPOINT: &str = "CAPYIO_AUDIO_SHARE_ENDPOINT";

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickActionOperation {
    Start,
    Retry,
    Stop,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvokeQuickActionRequest {
    pub action_id: String,
    pub operation: QuickActionOperation,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectAudioEndpointRequest {
    pub action_id: String,
    pub selection_token: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiAudioEndpointChoice {
    pub selection_token: String,
    pub display_name: String,
    pub is_default: bool,
    pub selected: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiAudioEndpointCatalog {
    pub schema_version: u8,
    pub action_id: &'static str,
    pub supported: bool,
    pub can_select: bool,
    pub choices: Vec<UiAudioEndpointChoice>,
    pub problem: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiQuickAction {
    pub schema_version: u8,
    pub id: &'static str,
    pub kind: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub status: &'static str,
    pub simulated: bool,
    pub route_id: Option<String>,
    pub route_state: Option<&'static str>,
    pub route_epoch: Option<u64>,
    pub available_operations: Vec<&'static str>,
    pub evidence_level: &'static str,
    pub connection_hint: Option<String>,
    pub problem_code: Option<String>,
    pub problem: Option<String>,
}

pub struct AudioShareQuickAction {
    controller: Option<AudioShareRouteController<AudioShareHostProcess>>,
    host_config: Option<TrustedAudioShareHostConfig>,
    endpoint_selection: EndpointSelectionCache,
    configuration_problem: Option<String>,
    orchestration_problem: Option<String>,
}

impl AudioShareQuickAction {
    pub fn install(runtime: &mut NodeRuntime, session_id: SessionId) -> Self {
        let configured = configured_host_process();
        match configured.and_then(|(host, process, service_was_running)| {
            AudioShareRouteController::install(
                runtime,
                session_id,
                process,
                DEFAULT_STABLE_RECEIVER_POLLS,
                DEFAULT_RECEIVER_WAIT_POLLS,
            )
            .and_then(|mut controller| {
                if service_was_running {
                    controller.start(runtime, unix_time_ms()?)?;
                    controller.poll(runtime)?;
                }
                Ok((host, controller))
            })
        }) {
            Ok((host_config, controller)) => Self {
                controller: Some(controller),
                host_config: Some(host_config),
                endpoint_selection: EndpointSelectionCache::default(),
                configuration_problem: None,
                orchestration_problem: None,
            },
            Err(problem) => Self {
                controller: None,
                host_config: None,
                endpoint_selection: EndpointSelectionCache::default(),
                configuration_problem: Some(bounded(problem)),
                orchestration_problem: None,
            },
        }
    }

    pub fn is_configured(&self) -> bool {
        self.controller.is_some()
    }

    pub fn owns_route(&self, route_id: RouteId) -> bool {
        self.controller
            .as_ref()
            .is_some_and(|controller| controller.route_id() == route_id)
    }

    pub fn poll(&mut self, runtime: &mut NodeRuntime) {
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        if matches!(
            controller.route_state(runtime),
            Ok(RouteState::Starting | RouteState::Active)
        ) {
            self.orchestration_problem = controller.poll(runtime).err().map(bounded);
        }
    }

    pub fn invoke(
        &mut self,
        runtime: &mut NodeRuntime,
        request: InvokeQuickActionRequest,
        now_ms: u64,
    ) -> Result<UiQuickAction, String> {
        if request.action_id != AUDIO_SHARE_ACTION_ID {
            return Err("unknown Quick Action".to_owned());
        }
        let controller = self
            .controller
            .as_mut()
            .ok_or_else(|| "Audio Share host configuration is unavailable".to_owned())?;
        self.orchestration_problem = None;
        match request.operation {
            QuickActionOperation::Start | QuickActionOperation::Retry => {
                controller.start(runtime, now_ms)?;
            }
            QuickActionOperation::Stop => controller.stop(runtime)?,
        }
        self.dto(runtime)
    }

    pub fn endpoint_selection_available(&self, runtime: &NodeRuntime) -> bool {
        self.controller
            .as_ref()
            .and_then(|controller| controller.route_state(runtime).ok())
            .is_some_and(endpoint_selection_allowed)
    }

    pub fn endpoint_catalog(&mut self, can_select: bool) -> UiAudioEndpointCatalog {
        self.endpoint_selection.by_token.clear();
        let Some(host_config) = self.host_config.as_ref() else {
            return endpoint_catalog_problem(
                false,
                "Audio Share host configuration is unavailable",
            );
        };
        if host_config.mode.is_fixed_projection() {
            return UiAudioEndpointCatalog {
                schema_version: 1,
                action_id: AUDIO_SHARE_ACTION_ID,
                supported: false,
                can_select: false,
                choices: Vec::new(),
                problem: None,
            };
        }
        if !can_select {
            return endpoint_catalog_problem(
                true,
                "stop the Audio Share Route before scanning endpoints",
            );
        }
        let probe = match AudioShareProbe::new(ProbeLimits::default()) {
            Ok(probe) => probe,
            Err(_) => return endpoint_scan_failed(),
        };
        let inventory = match probe.inventory(&host_config.executable) {
            Ok(inventory) => inventory,
            Err(_) => return endpoint_scan_failed(),
        };
        let Some(generation) = self.endpoint_selection.generation.checked_add(1) else {
            return endpoint_catalog_problem(true, "endpoint selection generation exhausted");
        };
        self.endpoint_selection.generation = generation;
        let choices = inventory
            .endpoints
            .into_iter()
            .enumerate()
            .map(|(index, endpoint)| {
                let selection_token = format!("audio-endpoint-{generation}-{index}");
                self.endpoint_selection
                    .by_token
                    .insert(selection_token.clone(), endpoint.id.clone());
                UiAudioEndpointChoice {
                    selection_token,
                    display_name: sanitized_endpoint_name(&endpoint.name),
                    is_default: endpoint.is_default,
                    selected: endpoint.id == host_config.endpoint_id,
                }
            })
            .collect();
        UiAudioEndpointCatalog {
            schema_version: 1,
            action_id: AUDIO_SHARE_ACTION_ID,
            supported: true,
            can_select,
            choices,
            problem: None,
        }
    }

    pub fn select_endpoint(
        &mut self,
        runtime: &NodeRuntime,
        request: SelectAudioEndpointRequest,
    ) -> Result<UiQuickAction, String> {
        if request.action_id != AUDIO_SHARE_ACTION_ID {
            return Err("unknown Quick Action".to_owned());
        }
        validate_selection_token(&request.selection_token)?;
        let endpoint_id = self
            .endpoint_selection
            .by_token
            .get(&request.selection_token)
            .cloned()
            .ok_or_else(|| {
                "refresh the endpoint list and choose an enumerated endpoint".to_owned()
            })?;
        let host_config = self
            .host_config
            .as_ref()
            .ok_or_else(|| "Audio Share host configuration is unavailable".to_owned())?;
        if host_config.mode.is_fixed_projection() {
            return Err("CapyIO Speaker is a fixed projection and cannot be reselected".to_owned());
        }
        let supervisor = host_config.supervisor_for(endpoint_id.clone())?;
        self.controller
            .as_mut()
            .ok_or_else(|| "Audio Share host configuration is unavailable".to_owned())?
            .replace_process(runtime, AudioShareHostProcess::Direct(Box::new(supervisor)))?;
        self.host_config
            .as_mut()
            .expect("configured controller retains host configuration")
            .endpoint_id = endpoint_id;
        self.endpoint_selection.by_token.clear();
        self.orchestration_problem = None;
        self.dto(runtime)
    }

    pub fn dto(&self, runtime: &NodeRuntime) -> Result<UiQuickAction, String> {
        let Some(controller) = self.controller.as_ref() else {
            return Ok(UiQuickAction {
                schema_version: QUICK_ACTION_SCHEMA_VERSION,
                id: AUDIO_SHARE_ACTION_ID,
                kind: "route_control",
                title: "将电脑声音镜像到手机",
                summary: "系统音频镜像 · 非虚拟扬声器 · 宿主尚未配置",
                status: "blocked",
                simulated: false,
                route_id: None,
                route_state: None,
                route_epoch: None,
                available_operations: Vec::new(),
                evidence_level: "not_started",
                connection_hint: None,
                problem_code: Some("CAPY.AUDIO_SHARE.HOST_CONFIGURATION_MISSING".to_owned()),
                problem: self.configuration_problem.clone(),
            });
        };
        let status = controller.status(runtime)?;
        let problem = matches!(status.route_state, RouteState::Offline | RouteState::Failed)
            .then(|| latest_problem(runtime, controller.route_id()))
            .flatten();
        let virtual_speaker = self
            .host_config
            .as_ref()
            .is_some_and(|host| host.mode.is_fixed_projection());
        Ok(UiQuickAction {
            schema_version: QUICK_ACTION_SCHEMA_VERSION,
            id: AUDIO_SHARE_ACTION_ID,
            kind: "route_control",
            title: if virtual_speaker {
                "使用 CapyIO 虚拟扬声器"
            } else {
                "将电脑声音镜像到手机"
            },
            summary: if virtual_speaker {
                "CapyIO Speaker → Android 扬声器 · 独立虚拟设备"
            } else {
                "Windows 系统音频镜像 → Android 扬声器 · 电脑端仍可能播放"
            },
            status: action_status(status.route_state),
            simulated: false,
            route_id: Some(controller.route_id().to_string()),
            route_state: Some(route_state_label(status.route_state)),
            route_epoch: Some(status.route_epoch),
            available_operations: operations(status.route_state),
            evidence_level: if status.route_state == RouteState::Active {
                "stable_tcp_receiver_presence"
            } else {
                "process_and_route_state"
            },
            connection_hint: None,
            problem_code: problem.as_ref().map(|value| value.code.clone()),
            problem: self
                .orchestration_problem
                .clone()
                .or_else(|| problem.map(|value| value.human_message)),
        })
    }

    pub fn shutdown(&mut self, runtime: &mut NodeRuntime) {
        if self
            .host_config
            .as_ref()
            .is_some_and(|host| host.mode == TrustedAudioShareMode::WindowsService)
        {
            return;
        }
        if let Some(controller) = self.controller.as_mut() {
            let _ = controller.stop(runtime);
        }
    }
}

#[derive(Clone)]
struct TrustedAudioShareHostConfig {
    mode: TrustedAudioShareMode,
    executable: PathBuf,
    bind_ip: IpAddr,
    port: u16,
    endpoint_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrustedAudioShareMode {
    SystemMirror,
    VirtualSpeaker,
    WindowsService,
}

impl TrustedAudioShareMode {
    const fn is_fixed_projection(self) -> bool {
        matches!(self, Self::VirtualSpeaker | Self::WindowsService)
    }
}

impl TrustedAudioShareHostConfig {
    fn from_environment() -> Result<Self, String> {
        let virtual_speaker = env::var_os(ENV_VIRTUAL_SPEAKER_EXE).map(PathBuf::from);
        let mode = if virtual_speaker.is_some() {
            TrustedAudioShareMode::VirtualSpeaker
        } else {
            TrustedAudioShareMode::SystemMirror
        };
        Ok(Self {
            mode,
            executable: match virtual_speaker {
                Some(executable) => executable,
                None => required_env_path(ENV_EXE)?,
            },
            bind_ip: required_env(ENV_BIND_IP)?
                .parse::<IpAddr>()
                .map_err(|_| format!("{ENV_BIND_IP} must be an IP literal"))?,
            port: required_env(ENV_PORT)?
                .parse::<u16>()
                .map_err(|_| format!("{ENV_PORT} must be a non-zero u16"))?,
            endpoint_id: if mode == TrustedAudioShareMode::VirtualSpeaker {
                "capyio-virtual-speaker".to_owned()
            } else {
                required_env(ENV_ENDPOINT)?
            },
        })
    }

    fn supervisor(&self) -> Result<AudioShareSupervisor, String> {
        self.supervisor_for(self.endpoint_id.clone())
    }

    fn supervisor_for(&self, endpoint_id: String) -> Result<AudioShareSupervisor, String> {
        if self.mode == TrustedAudioShareMode::WindowsService {
            return Err("Windows service mode does not create a desktop Broker".to_owned());
        }
        if self.mode == TrustedAudioShareMode::VirtualSpeaker {
            return AudioShareSupervisor::new_virtual_speaker(
                self.executable.clone(),
                self.bind_ip,
                self.port,
                SupervisorLimits::default(),
            )
            .map_err(|error| error.to_string());
        }
        let config = AudioShareConfig::new(
            self.executable.clone(),
            self.bind_ip,
            self.port,
            endpoint_id,
            AudioEncoding::S16,
            2,
            48_000,
        )
        .map_err(|error| error.to_string())?;
        AudioShareSupervisor::new(config, ProbeLimits::default(), SupervisorLimits::default())
            .map_err(|error| error.to_string())
    }
}

enum AudioShareHostProcess {
    Direct(Box<AudioShareSupervisor>),
    #[cfg(windows)]
    WindowsService(BrokerServiceProcess),
}

impl AudioShareProcessBoundary for AudioShareHostProcess {
    fn start(&mut self) -> Result<(), AudioShareStartError> {
        match self {
            Self::Direct(process) => AudioShareProcessBoundary::start(process.as_mut()),
            #[cfg(windows)]
            Self::WindowsService(process) => process.start(),
        }
    }

    fn status(&mut self) -> Result<SupervisorStatus, String> {
        match self {
            Self::Direct(process) => AudioShareProcessBoundary::status(process.as_mut()),
            #[cfg(windows)]
            Self::WindowsService(process) => process.status(),
        }
    }

    fn receiver_presence(&mut self) -> Result<ReceiverTcpPresence, String> {
        match self {
            Self::Direct(process) => AudioShareProcessBoundary::receiver_presence(process.as_mut()),
            #[cfg(windows)]
            Self::WindowsService(process) => process.receiver_presence(),
        }
    }

    fn stop(&mut self) -> Result<(), String> {
        match self {
            Self::Direct(process) => AudioShareProcessBoundary::stop(process.as_mut()),
            #[cfg(windows)]
            Self::WindowsService(process) => process.stop(),
        }
    }
}

#[cfg(windows)]
struct BrokerServiceProcess {
    client: BrokerServiceClient,
}

#[cfg(windows)]
impl BrokerServiceProcess {
    fn start(&self) -> Result<(), AudioShareStartError> {
        self.client
            .start()
            .map(|_| ())
            .map_err(AudioShareStartError::Other)
    }

    fn status(&self) -> Result<SupervisorStatus, String> {
        self.client.status().map(service_supervisor_status)
    }

    fn receiver_presence(&self) -> Result<ReceiverTcpPresence, String> {
        self.client.status().map(|snapshot| {
            if snapshot.state == BrokerServiceState::Active {
                ReceiverTcpPresence::Established {
                    connection_count: 1,
                }
            } else if snapshot.state == BrokerServiceState::Stopped {
                ReceiverTcpPresence::SupervisorNotRunning
            } else {
                ReceiverTcpPresence::Disconnected
            }
        })
    }

    fn stop(&self) -> Result<(), String> {
        self.client.stop().map(|_| ())
    }
}

#[cfg(windows)]
fn service_supervisor_status(snapshot: BrokerServiceSnapshot) -> SupervisorStatus {
    match snapshot.state {
        BrokerServiceState::Stopped => SupervisorStatus::Stopped,
        BrokerServiceState::WaitingForReceiver | BrokerServiceState::Active => {
            SupervisorStatus::Running { process_id: 0 }
        }
        BrokerServiceState::Failed => SupervisorStatus::Exited(ProcessExitReport {
            exit_code: None,
            output: ProcessOutputSummary {
                stdout_retained_bytes: 0,
                stderr_retained_bytes: 0,
                stdout_overflowed: false,
                stderr_overflowed: false,
            },
        }),
    }
}

fn configured_host_process()
-> Result<(TrustedAudioShareHostConfig, AudioShareHostProcess, bool), String> {
    #[cfg(windows)]
    {
        let client = BrokerServiceClient::default();
        if let Ok(snapshot) = client.status() {
            let was_running = matches!(
                snapshot.state,
                BrokerServiceState::WaitingForReceiver | BrokerServiceState::Active
            );
            return Ok((
                TrustedAudioShareHostConfig {
                    mode: TrustedAudioShareMode::WindowsService,
                    executable: PathBuf::new(),
                    bind_ip: "127.0.0.1".parse().expect("literal IPv4"),
                    port: 1,
                    endpoint_id: "capyio-virtual-speaker".to_owned(),
                },
                AudioShareHostProcess::WindowsService(BrokerServiceProcess { client }),
                was_running,
            ));
        }
    }
    let host = TrustedAudioShareHostConfig::from_environment()?;
    let supervisor = host.supervisor()?;
    Ok((
        host,
        AudioShareHostProcess::Direct(Box::new(supervisor)),
        false,
    ))
}

fn unix_time_ms() -> Result<u64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before Unix epoch".to_owned())?
        .as_millis();
    u64::try_from(millis).map_err(|_| "system time exceeds Runtime range".to_owned())
}

#[derive(Default)]
struct EndpointSelectionCache {
    generation: u64,
    by_token: BTreeMap<String, String>,
}

fn endpoint_selection_allowed(state: RouteState) -> bool {
    matches!(
        state,
        RouteState::Draft | RouteState::Prepared | RouteState::Stopped | RouteState::Offline
    )
}

fn validate_selection_token(token: &str) -> Result<(), String> {
    if token.is_empty()
        || token.len() > 64
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err("invalid endpoint selection token".to_owned());
    }
    Ok(())
}

fn endpoint_catalog_problem(supported: bool, problem: &str) -> UiAudioEndpointCatalog {
    UiAudioEndpointCatalog {
        schema_version: 1,
        action_id: AUDIO_SHARE_ACTION_ID,
        supported,
        can_select: false,
        choices: Vec::new(),
        problem: Some(bounded(problem.to_owned())),
    }
}

fn endpoint_scan_failed() -> UiAudioEndpointCatalog {
    endpoint_catalog_problem(
        true,
        "Windows playback endpoint scan failed; check trusted host configuration",
    )
}

fn sanitized_endpoint_name(name: &str) -> String {
    name.chars()
        .take(256)
        .map(|character| {
            if character.is_control() {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

fn required_env(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("set {name} in the trusted desktop host environment"))
}

fn required_env_path(name: &str) -> Result<PathBuf, String> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| format!("set {name} in the trusted desktop host environment"))
}

pub(crate) fn latest_problem(runtime: &NodeRuntime, route_id: RouteId) -> Option<Problem> {
    runtime
        .snapshot()
        .problems
        .into_iter()
        .rev()
        .find(|problem| problem.related_route == Some(route_id))
}

pub(crate) fn action_status(state: RouteState) -> &'static str {
    match state {
        RouteState::Draft | RouteState::Prepared | RouteState::Stopped => "idle",
        RouteState::Starting => "starting",
        RouteState::Active => "active",
        RouteState::Stopping => "stopping",
        RouteState::Offline => "offline",
        RouteState::Failed => "failed",
    }
}

pub(crate) fn route_state_label(state: RouteState) -> &'static str {
    match state {
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

pub(crate) fn operations(state: RouteState) -> Vec<&'static str> {
    match state {
        RouteState::Draft | RouteState::Prepared | RouteState::Stopped => vec!["start"],
        RouteState::Starting | RouteState::Active | RouteState::Stopping => vec!["stop"],
        RouteState::Offline => vec!["retry", "stop"],
        RouteState::Failed => Vec::new(),
    }
}

pub(crate) fn bounded(value: String) -> String {
    value.chars().take(1024).collect()
}

#[cfg(test)]
mod tests {
    use capyio_testkit::DemoLab;

    use super::*;

    #[test]
    fn blocked_projection_is_versioned_real_and_has_no_operations() {
        let lab = DemoLab::new().expect("demo Runtime");
        let action = AudioShareQuickAction {
            controller: None,
            host_config: None,
            endpoint_selection: EndpointSelectionCache::default(),
            configuration_problem: Some("fixture host configuration missing".to_owned()),
            orchestration_problem: None,
        };
        let dto = action.dto(&lab.runtime).expect("blocked DTO");
        assert_eq!(dto.schema_version, QUICK_ACTION_SCHEMA_VERSION);
        assert_eq!(dto.id, AUDIO_SHARE_ACTION_ID);
        assert_eq!(dto.title, "将电脑声音镜像到手机");
        assert!(dto.summary.contains("宿主尚未配置"));
        assert_eq!(dto.status, "blocked");
        assert!(!dto.simulated);
        assert!(dto.route_id.is_none());
        assert!(dto.available_operations.is_empty());
        assert_eq!(
            dto.problem_code.as_deref(),
            Some("CAPY.AUDIO_SHARE.HOST_CONFIGURATION_MISSING")
        );
    }

    #[test]
    fn operations_follow_route_state_without_audio_specific_ui_commands() {
        assert_eq!(operations(RouteState::Draft), ["start"]);
        assert_eq!(operations(RouteState::Active), ["stop"]);
        assert_eq!(operations(RouteState::Offline), ["retry", "stop"]);
        assert!(operations(RouteState::Failed).is_empty());
    }

    #[test]
    fn quick_action_request_rejects_unknown_fields() {
        let result = serde_json::from_str::<InvokeQuickActionRequest>(
            r#"{"actionId":"capyio.quick-action.remote-speaker","operation":"start","path":"untrusted.exe"}"#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn endpoint_selection_request_rejects_unknown_fields() {
        let result = serde_json::from_str::<SelectAudioEndpointRequest>(
            r#"{"actionId":"capyio.quick-action.remote-speaker","selectionToken":"audio-endpoint-1-0","endpointId":"untrusted"}"#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn endpoint_selection_tokens_are_bounded_and_closed() {
        assert!(validate_selection_token("audio-endpoint-1-0").is_ok());
        assert!(validate_selection_token("").is_err());
        assert!(validate_selection_token("../raw-endpoint-id").is_err());
        assert!(validate_selection_token(&"a".repeat(65)).is_err());
    }

    #[test]
    fn endpoint_names_are_bounded_and_control_characters_are_replaced() {
        let sanitized = sanitized_endpoint_name(&format!("Remote\nAudio{}", "x".repeat(300)));
        assert_eq!(sanitized.chars().count(), 256);
        assert!(sanitized.contains('\u{fffd}'));
        assert!(!sanitized.contains('\n'));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires the approved installed CapyIOBroker Windows service"]
    fn physical_windows_service_quick_action_controls_and_preserves_broker() {
        let client = BrokerServiceClient::default();
        client.stop().expect("stop Broker fixture");

        let mut lab = DemoLab::new().expect("demo Runtime");
        let session_id = lab.session_id;
        let mut action = AudioShareQuickAction::install(&mut lab.runtime, session_id);
        assert!(action.is_configured());
        assert_eq!(
            action.host_config.as_ref().map(|host| host.mode),
            Some(TrustedAudioShareMode::WindowsService)
        );

        let started = action
            .invoke(
                &mut lab.runtime,
                InvokeQuickActionRequest {
                    action_id: AUDIO_SHARE_ACTION_ID.to_owned(),
                    operation: QuickActionOperation::Start,
                },
                unix_time_ms().expect("wall clock"),
            )
            .expect("start through Quick Action");
        assert_eq!(started.status, "starting");
        assert_eq!(started.title, "使用 CapyIO 虚拟扬声器");

        action.shutdown(&mut lab.runtime);
        let snapshot = client
            .status()
            .expect("service state after desktop shutdown");
        assert!(matches!(
            snapshot.state,
            BrokerServiceState::WaitingForReceiver | BrokerServiceState::Active
        ));
    }
}
