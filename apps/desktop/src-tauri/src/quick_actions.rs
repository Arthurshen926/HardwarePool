use std::{env, net::IpAddr, path::PathBuf};

use capyio_audio_share_adapter::{
    AudioEncoding, AudioShareConfig, AudioShareSupervisor, ProbeLimits, SupervisorLimits,
};
use capyio_core::{Problem, RouteId, RouteState, SessionId};
use capyio_runtime::NodeRuntime;
use serde::{Deserialize, Serialize};

use crate::audio_share_runtime::{AudioShareRouteController, DEFAULT_STABLE_RECEIVER_POLLS};

pub const QUICK_ACTION_SCHEMA_VERSION: u8 = 1;
pub const AUDIO_SHARE_ACTION_ID: &str = "capyio.quick-action.remote-speaker";

const ENV_EXE: &str = "CAPYIO_AUDIO_SHARE_EXE";
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
    pub problem_code: Option<String>,
    pub problem: Option<String>,
}

pub struct AudioShareQuickAction {
    controller: Option<AudioShareRouteController<AudioShareSupervisor>>,
    configuration_problem: Option<String>,
    orchestration_problem: Option<String>,
}

impl AudioShareQuickAction {
    pub fn install(runtime: &mut NodeRuntime, session_id: SessionId) -> Self {
        match configured_supervisor().and_then(|supervisor| {
            AudioShareRouteController::install(
                runtime,
                session_id,
                supervisor,
                DEFAULT_STABLE_RECEIVER_POLLS,
            )
        }) {
            Ok(controller) => Self {
                controller: Some(controller),
                configuration_problem: None,
                orchestration_problem: None,
            },
            Err(problem) => Self {
                controller: None,
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

    pub fn dto(&self, runtime: &NodeRuntime) -> Result<UiQuickAction, String> {
        let Some(controller) = self.controller.as_ref() else {
            return Ok(UiQuickAction {
                schema_version: QUICK_ACTION_SCHEMA_VERSION,
                id: AUDIO_SHARE_ACTION_ID,
                kind: "route_control",
                title: "将电脑声音发送到手机",
                summary: "Audio Share 外部进程 · 宿主尚未配置",
                status: "blocked",
                simulated: false,
                route_id: None,
                route_state: None,
                route_epoch: None,
                available_operations: Vec::new(),
                evidence_level: "not_started",
                problem_code: Some("CAPY.AUDIO_SHARE.HOST_CONFIGURATION_MISSING".to_owned()),
                problem: self.configuration_problem.clone(),
            });
        };
        let status = controller.status(runtime)?;
        let problem = latest_problem(runtime, controller.route_id());
        Ok(UiQuickAction {
            schema_version: QUICK_ACTION_SCHEMA_VERSION,
            id: AUDIO_SHARE_ACTION_ID,
            kind: "route_control",
            title: "将电脑声音发送到手机",
            summary: "Windows 系统播放 → Android 扬声器",
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
            problem_code: problem.as_ref().map(|value| value.code.clone()),
            problem: self
                .orchestration_problem
                .clone()
                .or_else(|| problem.map(|value| value.human_message)),
        })
    }

    pub fn shutdown(&mut self, runtime: &mut NodeRuntime) {
        if let Some(controller) = self.controller.as_mut() {
            let _ = controller.stop(runtime);
        }
    }
}

fn configured_supervisor() -> Result<AudioShareSupervisor, String> {
    let executable = required_env_path(ENV_EXE)?;
    let bind_ip = required_env(ENV_BIND_IP)?
        .parse::<IpAddr>()
        .map_err(|_| format!("{ENV_BIND_IP} must be an IP literal"))?;
    let port = required_env(ENV_PORT)?
        .parse::<u16>()
        .map_err(|_| format!("{ENV_PORT} must be a non-zero u16"))?;
    let endpoint = required_env(ENV_ENDPOINT)?;
    let config = AudioShareConfig::new(
        executable,
        bind_ip,
        port,
        endpoint,
        AudioEncoding::S16,
        2,
        48_000,
    )
    .map_err(|error| error.to_string())?;
    AudioShareSupervisor::new(config, ProbeLimits::default(), SupervisorLimits::default())
        .map_err(|error| error.to_string())
}

fn required_env(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("set {name} in the trusted desktop host environment"))
}

fn required_env_path(name: &str) -> Result<PathBuf, String> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| format!("set {name} in the trusted desktop host environment"))
}

fn latest_problem(runtime: &NodeRuntime, route_id: RouteId) -> Option<Problem> {
    runtime
        .snapshot()
        .problems
        .into_iter()
        .rev()
        .find(|problem| problem.related_route == Some(route_id))
}

fn action_status(state: RouteState) -> &'static str {
    match state {
        RouteState::Draft | RouteState::Prepared | RouteState::Stopped => "idle",
        RouteState::Starting => "starting",
        RouteState::Active => "active",
        RouteState::Stopping => "stopping",
        RouteState::Offline => "offline",
        RouteState::Failed => "failed",
    }
}

fn route_state_label(state: RouteState) -> &'static str {
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

fn operations(state: RouteState) -> Vec<&'static str> {
    match state {
        RouteState::Draft | RouteState::Prepared | RouteState::Stopped => vec!["start"],
        RouteState::Starting | RouteState::Active | RouteState::Stopping => vec!["stop"],
        RouteState::Offline => vec!["retry", "stop"],
        RouteState::Failed => Vec::new(),
    }
}

fn bounded(value: String) -> String {
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
            configuration_problem: Some("fixture host configuration missing".to_owned()),
            orchestration_problem: None,
        };
        let dto = action.dto(&lab.runtime).expect("blocked DTO");
        assert_eq!(dto.schema_version, QUICK_ACTION_SCHEMA_VERSION);
        assert_eq!(dto.id, AUDIO_SHARE_ACTION_ID);
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
}
