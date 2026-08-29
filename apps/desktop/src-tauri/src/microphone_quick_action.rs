use capyio_core::{RouteId, RouteState, SessionId};
use capyio_micyou_adapter::{MicYouSupervisor, PeerTcpPresence, SupervisorStatus};
use capyio_micyou_host_config::{TrustedMicYouHostConfig, load_trusted_host_config};
use capyio_runtime::NodeRuntime;
#[cfg(windows)]
use capyio_windows_service::{MicrophoneHostClient, MicrophoneHostSnapshot, MicrophoneHostState};

use crate::{
    micyou_runtime::{
        DEFAULT_PHONE_WAIT_POLLS, DEFAULT_STABLE_PHONE_POLLS, MicYouProcessBoundary,
        MicYouRouteController, MicYouStartError,
    },
    quick_actions::{
        InvokeQuickActionRequest, QUICK_ACTION_SCHEMA_VERSION, QuickActionOperation, UiQuickAction,
        action_status, bounded, latest_problem, operations, route_state_label,
    },
};

pub const MICYOU_ACTION_ID: &str = "capyio.quick-action.remote-microphone";

pub struct MicrophoneQuickAction {
    controller: Option<MicYouRouteController<MicYouHostProcess>>,
    connection_hint: Option<String>,
    headless: bool,
    configuration_problem: Option<String>,
    orchestration_problem: Option<String>,
}

enum MicYouHostProcess {
    Direct(Box<MicYouSupervisor>),
    #[cfg(windows)]
    Headless(HeadlessMicrophoneProcess),
}

impl MicYouProcessBoundary for MicYouHostProcess {
    fn start(&mut self) -> Result<(), MicYouStartError> {
        match self {
            Self::Direct(process) => MicYouProcessBoundary::start(process.as_mut()),
            #[cfg(windows)]
            Self::Headless(process) => process.start(),
        }
    }

    fn status(&mut self) -> Result<SupervisorStatus, String> {
        match self {
            Self::Direct(process) => MicYouProcessBoundary::status(process.as_mut()),
            #[cfg(windows)]
            Self::Headless(process) => process.status(),
        }
    }

    fn phone_presence(&mut self) -> Result<PeerTcpPresence, String> {
        match self {
            Self::Direct(process) => MicYouProcessBoundary::phone_presence(process.as_mut()),
            #[cfg(windows)]
            Self::Headless(process) => process.phone_presence(),
        }
    }

    fn stop(&mut self) -> Result<(), String> {
        match self {
            Self::Direct(process) => MicYouProcessBoundary::stop(process.as_mut()),
            #[cfg(windows)]
            Self::Headless(process) => process.stop(),
        }
    }
}

#[cfg(windows)]
struct HeadlessMicrophoneProcess {
    client: Box<dyn MicrophoneHostControl>,
}

#[cfg(windows)]
impl HeadlessMicrophoneProcess {
    fn start(&self) -> Result<(), MicYouStartError> {
        self.client.start().map(|_| ()).map_err(|problem| {
            if problem == "CAPY.MICROPHONE_HOST.ENDPOINT_UNAVAILABLE" {
                MicYouStartError::ConfiguredEndpointUnavailable
            } else {
                MicYouStartError::Other(problem)
            }
        })
    }

    fn status(&self) -> Result<SupervisorStatus, String> {
        self.client.status().map(headless_supervisor_status)
    }

    fn phone_presence(&self) -> Result<PeerTcpPresence, String> {
        self.client.status().map(|snapshot| match snapshot.state {
            MicrophoneHostState::Active => PeerTcpPresence::Established {
                connection_count: 1,
            },
            MicrophoneHostState::Stopped | MicrophoneHostState::Failed => {
                PeerTcpPresence::SupervisorNotRunning
            }
            MicrophoneHostState::WaitingForPhone => PeerTcpPresence::Disconnected,
        })
    }

    fn stop(&self) -> Result<(), String> {
        self.client.stop().map(|_| ())
    }
}

#[cfg(windows)]
trait MicrophoneHostControl: Send {
    fn start(&self) -> Result<MicrophoneHostSnapshot, String>;
    fn status(&self) -> Result<MicrophoneHostSnapshot, String>;
    fn stop(&self) -> Result<MicrophoneHostSnapshot, String>;
}

#[cfg(windows)]
impl MicrophoneHostControl for MicrophoneHostClient {
    fn start(&self) -> Result<MicrophoneHostSnapshot, String> {
        MicrophoneHostClient::start(self)
    }

    fn status(&self) -> Result<MicrophoneHostSnapshot, String> {
        MicrophoneHostClient::status(self)
    }

    fn stop(&self) -> Result<MicrophoneHostSnapshot, String> {
        MicrophoneHostClient::stop(self)
    }
}

#[cfg(windows)]
fn headless_supervisor_status(snapshot: MicrophoneHostSnapshot) -> SupervisorStatus {
    match snapshot.state {
        MicrophoneHostState::Stopped => SupervisorStatus::Stopped,
        MicrophoneHostState::WaitingForPhone | MicrophoneHostState::Active => {
            SupervisorStatus::Running { process_id: 0 }
        }
        MicrophoneHostState::Failed => SupervisorStatus::Exited { exit_code: None },
    }
}

struct ConfiguredMicYouHost {
    process: MicYouHostProcess,
    connection_hint: String,
    headless: bool,
    was_running: bool,
}

impl MicrophoneQuickAction {
    pub fn install(runtime: &mut NodeRuntime, session_id: SessionId) -> Self {
        Self::install_with_host(runtime, session_id, configured_host_process())
    }

    #[cfg(test)]
    fn install_with_config(
        runtime: &mut NodeRuntime,
        session_id: SessionId,
        loaded: Result<TrustedMicYouHostConfig, String>,
    ) -> Self {
        let host = loaded.and_then(direct_host);
        Self::install_with_host(runtime, session_id, host)
    }

    fn install_with_host(
        runtime: &mut NodeRuntime,
        session_id: SessionId,
        configured: Result<ConfiguredMicYouHost, String>,
    ) -> Self {
        match configured.and_then(|host| {
            MicYouRouteController::install(
                runtime,
                session_id,
                host.process,
                DEFAULT_STABLE_PHONE_POLLS,
                DEFAULT_PHONE_WAIT_POLLS,
            )
            .and_then(|mut controller| {
                if host.was_running {
                    controller.start(runtime, unix_time_ms()?)?;
                    controller.poll(runtime)?;
                }
                Ok((host.connection_hint, host.headless, controller))
            })
        }) {
            Ok((connection_hint, headless, controller)) => Self {
                controller: Some(controller),
                connection_hint: Some(connection_hint),
                headless,
                configuration_problem: None,
                orchestration_problem: None,
            },
            Err(problem) => Self {
                controller: None,
                connection_hint: None,
                headless: false,
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
        if request.action_id != MICYOU_ACTION_ID {
            return Err("unknown Quick Action".to_owned());
        }
        let controller = self
            .controller
            .as_mut()
            .ok_or_else(|| "MicYou host configuration is unavailable".to_owned())?;
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
                id: MICYOU_ACTION_ID,
                kind: "route_control",
                title: "将手机麦克风用作电脑麦克风",
                summary: "Android 麦克风 → CapyIO Microphone · 可信主机配置尚未就绪",
                status: "blocked",
                simulated: false,
                route_id: None,
                route_state: None,
                route_epoch: None,
                available_operations: Vec::new(),
                evidence_level: "not_started",
                connection_hint: None,
                problem_code: Some("CAPY.MICYOU.HOST_CONFIGURATION_MISSING".to_owned()),
                problem: self.configuration_problem.clone(),
            });
        };
        let status = controller.status(runtime)?;
        let problem = matches!(status.route_state, RouteState::Offline | RouteState::Failed)
            .then(|| latest_problem(runtime, controller.route_id()))
            .flatten();
        Ok(UiQuickAction {
            schema_version: QUICK_ACTION_SCHEMA_VERSION,
            id: MICYOU_ACTION_ID,
            kind: "route_control",
            title: "将手机麦克风用作电脑麦克风",
            summary: "Android 麦克风 → CapyIO Microphone · MicYou 私有音频链路",
            status: action_status(status.route_state),
            simulated: false,
            route_id: Some(controller.route_id().to_string()),
            route_state: Some(route_state_label(status.route_state)),
            route_epoch: Some(status.route_epoch),
            available_operations: operations(status.route_state),
            evidence_level: if status.route_state == RouteState::Active {
                "stable_phone_tcp_presence"
            } else {
                "process_and_route_state"
            },
            connection_hint: self.connection_hint.clone(),
            problem_code: problem.as_ref().map(|value| value.code.clone()),
            problem: self
                .orchestration_problem
                .clone()
                .or_else(|| problem.map(|value| value.human_message)),
        })
    }

    pub fn shutdown(&mut self, runtime: &mut NodeRuntime) {
        if self.headless {
            return;
        }
        if let Some(controller) = self.controller.as_mut() {
            let _ = controller.stop(runtime);
        }
    }
}

fn configured_host_process() -> Result<ConfiguredMicYouHost, String> {
    #[cfg(windows)]
    {
        let client = MicrophoneHostClient::default();
        if let Ok(snapshot) = client.try_status() {
            let was_running = matches!(
                snapshot.state,
                MicrophoneHostState::WaitingForPhone | MicrophoneHostState::Active
            );
            return Ok(ConfiguredMicYouHost {
                process: MicYouHostProcess::Headless(HeadlessMicrophoneProcess {
                    client: Box::new(client),
                }),
                connection_hint: format!("在 Android MicYou 中连接 {}", snapshot.bind_address),
                headless: true,
                was_running,
            });
        }
    }
    let loaded = load_trusted_host_config()
        .map(|loaded| loaded.config)
        .map_err(|error| error.to_string())?;
    direct_host(loaded)
}

fn direct_host(config: TrustedMicYouHostConfig) -> Result<ConfiguredMicYouHost, String> {
    let connection_hint = config.connection_hint();
    let supervisor = config.supervisor().map_err(|error| error.to_string())?;
    Ok(ConfiguredMicYouHost {
        process: MicYouHostProcess::Direct(Box::new(supervisor)),
        connection_hint,
        headless: false,
        was_running: false,
    })
}

fn unix_time_ms() -> Result<u64, String> {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "system clock is before Unix epoch".to_owned())?
        .as_millis();
    u64::try_from(millis).map_err(|_| "system time exceeds Runtime range".to_owned())
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "windows")]
    use std::{
        env,
        io::{self, Write},
        process::Command,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
        time::Duration,
    };

    use capyio_testkit::DemoLab;

    use super::*;

    #[test]
    fn blocked_microphone_action_is_versioned_real_and_has_no_operations() {
        let lab = DemoLab::new().expect("demo Runtime");
        let action = MicrophoneQuickAction {
            controller: None,
            connection_hint: None,
            headless: false,
            configuration_problem: Some("fixture host configuration missing".to_owned()),
            orchestration_problem: None,
        };
        let dto = action.dto(&lab.runtime).expect("blocked DTO");
        assert_eq!(dto.schema_version, QUICK_ACTION_SCHEMA_VERSION);
        assert_eq!(dto.id, MICYOU_ACTION_ID);
        assert_eq!(dto.status, "blocked");
        assert!(!dto.simulated);
        assert!(dto.route_id.is_none());
        assert!(dto.available_operations.is_empty());
        assert!(dto.connection_hint.is_none());
        assert_eq!(
            dto.problem_code.as_deref(),
            Some("CAPY.MICYOU.HOST_CONFIGURATION_MISSING")
        );
    }

    #[test]
    fn connection_hint_exposes_only_typed_bind_address() {
        let host = TrustedMicYouHostConfig::new(
            "private-cli.exe",
            "100.64.0.10".parse().expect("IP"),
            8554,
            "{private-endpoint-id}",
            "private endpoint",
        )
        .expect("host config");
        let hint = host.connection_hint();
        assert_eq!(hint, "在 Android MicYou 中连接 100.64.0.10:8554");
        assert!(!hint.contains("private-cli"));
        assert!(!hint.contains("endpoint"));
    }

    #[test]
    fn microphone_action_rejects_other_action_ids_before_host_access() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let mut action = MicrophoneQuickAction {
            controller: None,
            connection_hint: None,
            headless: false,
            configuration_problem: Some("fixture host configuration missing".to_owned()),
            orchestration_problem: None,
        };
        let error = action
            .invoke(
                &mut lab.runtime,
                InvokeQuickActionRequest {
                    action_id: "capyio.quick-action.remote-speaker".to_owned(),
                    operation: QuickActionOperation::Start,
                },
                1,
            )
            .expect_err("wrong action ID");
        assert_eq!(error, "unknown Quick Action");
    }

    #[test]
    fn trusted_file_equivalent_config_installs_route_without_exposing_private_values() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let host = TrustedMicYouHostConfig::new(
            "private-cli.exe",
            "100.64.0.10".parse().expect("IP"),
            8554,
            "{private-endpoint-id}",
            "private endpoint",
        )
        .expect("host config");
        let session_id = lab.session_id;
        let action =
            MicrophoneQuickAction::install_with_config(&mut lab.runtime, session_id, Ok(host));
        let dto = action.dto(&lab.runtime).expect("configured DTO");
        assert_eq!(dto.status, "idle");
        assert_eq!(
            dto.connection_hint.as_deref(),
            Some("在 Android MicYou 中连接 100.64.0.10:8554")
        );
        let serialized = serde_json::to_string(&dto).expect("serialize DTO");
        assert!(!serialized.contains("private-cli"));
        assert!(!serialized.contains("private-endpoint-id"));
    }

    #[cfg(target_os = "windows")]
    struct FakeHeadlessControl {
        snapshot: MicrophoneHostSnapshot,
        stops: Arc<AtomicUsize>,
    }

    #[cfg(target_os = "windows")]
    impl MicrophoneHostControl for FakeHeadlessControl {
        fn start(&self) -> Result<MicrophoneHostSnapshot, String> {
            Ok(self.snapshot.clone())
        }

        fn status(&self) -> Result<MicrophoneHostSnapshot, String> {
            Ok(self.snapshot.clone())
        }

        fn stop(&self) -> Result<MicrophoneHostSnapshot, String> {
            self.stops.fetch_add(1, Ordering::AcqRel);
            Ok(MicrophoneHostSnapshot {
                state: MicrophoneHostState::Stopped,
                phone_present: false,
                ..self.snapshot.clone()
            })
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn already_running_headless_host_is_adopted_and_preserved_on_ui_shutdown() {
        let stops = Arc::new(AtomicUsize::new(0));
        let snapshot = MicrophoneHostSnapshot {
            schema_version: 1,
            state: MicrophoneHostState::Active,
            generation: 7,
            phone_present: true,
            bind_address: "100.64.0.10:8554".parse().expect("address"),
            problem_code: None,
        };
        let configured = ConfiguredMicYouHost {
            process: MicYouHostProcess::Headless(HeadlessMicrophoneProcess {
                client: Box::new(FakeHeadlessControl {
                    snapshot,
                    stops: Arc::clone(&stops),
                }),
            }),
            connection_hint: "在 Android MicYou 中连接 100.64.0.10:8554".to_owned(),
            headless: true,
            was_running: true,
        };
        let mut lab = DemoLab::new().expect("demo Runtime");
        let session_id = lab.session_id;
        let mut action =
            MicrophoneQuickAction::install_with_host(&mut lab.runtime, session_id, Ok(configured));
        action.poll(&mut lab.runtime);
        action.poll(&mut lab.runtime);
        assert_eq!(action.dto(&lab.runtime).expect("DTO").status, "active");

        action.shutdown(&mut lab.runtime);
        assert_eq!(stops.load(Ordering::Acquire), 0);
        assert_eq!(
            action.dto(&lab.runtime).expect("preserved DTO").status,
            "active"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires an explicitly approved physical Android/Windows lab"]
    fn physical_quick_action_tracks_disconnect_retry_and_stop() {
        let adb = env::var("CAPYIO_ADB").expect("CAPYIO_ADB must name the approved adb executable");
        let serial = env::var("CAPYIO_ANDROID_ADB_SERIAL")
            .expect("CAPYIO_ANDROID_ADB_SERIAL must name the approved physical device");
        let _phone_cleanup = PhoneStopGuard {
            adb: adb.clone(),
            serial: serial.clone(),
        };
        let active_hold = hold_duration("CAPYIO_MIC_ACTIVE_HOLD_MS");
        let offline_hold = hold_duration("CAPYIO_MIC_OFFLINE_HOLD_MS");
        let retry_hold = hold_duration("CAPYIO_MIC_RETRY_HOLD_MS");

        run_adb(
            &adb,
            &serial,
            &["shell", "am", "force-stop", "com.lanrhyme.micyou"],
        );

        let mut lab = DemoLab::new().expect("demo Runtime");
        let session_id = lab.session_id;
        let mut action = MicrophoneQuickAction::install(&mut lab.runtime, session_id);
        assert!(action.is_configured(), "trusted MicYou host configuration");
        let started = action
            .invoke(
                &mut lab.runtime,
                InvokeQuickActionRequest {
                    action_id: MICYOU_ACTION_ID.to_owned(),
                    operation: QuickActionOperation::Start,
                },
                1,
            )
            .expect("start physical microphone Quick Action");
        assert_eq!(started.route_state, Some("starting"));

        quick_start_phone(&adb, &serial);
        wait_for_state(&mut action, &mut lab, RouteState::Active, 120);
        announce_phase("active");
        thread::sleep(active_hold);

        run_adb(
            &adb,
            &serial,
            &["shell", "am", "force-stop", "com.lanrhyme.micyou"],
        );
        wait_for_state(&mut action, &mut lab, RouteState::Offline, 120);
        announce_phase("offline");
        thread::sleep(offline_hold);

        let retried = action
            .invoke(
                &mut lab.runtime,
                InvokeQuickActionRequest {
                    action_id: MICYOU_ACTION_ID.to_owned(),
                    operation: QuickActionOperation::Retry,
                },
                2,
            )
            .expect("retry physical microphone Quick Action");
        assert_eq!(retried.route_state, Some("starting"));
        quick_start_phone(&adb, &serial);
        wait_for_state(&mut action, &mut lab, RouteState::Active, 120);
        announce_phase("retried-active");
        thread::sleep(retry_hold);

        let stopped = action
            .invoke(
                &mut lab.runtime,
                InvokeQuickActionRequest {
                    action_id: MICYOU_ACTION_ID.to_owned(),
                    operation: QuickActionOperation::Stop,
                },
                3,
            )
            .expect("stop physical microphone Quick Action");
        assert_eq!(stopped.route_state, Some("stopped"));
        run_adb(
            &adb,
            &serial,
            &["shell", "am", "force-stop", "com.lanrhyme.micyou"],
        );
        announce_phase("stopped");
    }

    #[cfg(target_os = "windows")]
    fn quick_start_phone(adb: &str, serial: &str) {
        run_adb(
            adb,
            serial,
            &[
                "shell",
                "am",
                "start",
                "-a",
                "com.lanrhyme.micyou.ACTION_QUICK_START",
                "-n",
                "com.lanrhyme.micyou/.MainActivity",
            ],
        );
    }

    #[cfg(target_os = "windows")]
    fn run_adb(adb: &str, serial: &str, arguments: &[&str]) {
        let status = Command::new(adb)
            .arg("-s")
            .arg(serial)
            .args(arguments)
            .status()
            .expect("launch approved adb command");
        assert!(status.success(), "adb command failed with {status}");
    }

    #[cfg(target_os = "windows")]
    struct PhoneStopGuard {
        adb: String,
        serial: String,
    }

    #[cfg(target_os = "windows")]
    impl Drop for PhoneStopGuard {
        fn drop(&mut self) {
            let _ = Command::new(&self.adb)
                .arg("-s")
                .arg(&self.serial)
                .args(["shell", "am", "force-stop", "com.lanrhyme.micyou"])
                .status();
        }
    }

    #[cfg(target_os = "windows")]
    fn wait_for_state(
        action: &mut MicrophoneQuickAction,
        lab: &mut DemoLab,
        expected: RouteState,
        poll_limit: usize,
    ) {
        for _ in 0..poll_limit {
            action.poll(&mut lab.runtime);
            let state = action
                .controller
                .as_ref()
                .expect("configured controller")
                .route_state(&lab.runtime)
                .expect("physical Route state");
            if state == expected {
                return;
            }
            if state == RouteState::Failed {
                panic!("physical microphone Route failed while waiting for {expected:?}");
            }
            thread::sleep(Duration::from_millis(250));
        }
        let final_state = action
            .controller
            .as_ref()
            .expect("configured controller")
            .route_state(&lab.runtime)
            .expect("final physical Route state");
        panic!("timed out waiting for {expected:?}; final state was {final_state:?}");
    }

    #[cfg(target_os = "windows")]
    fn hold_duration(variable: &str) -> Duration {
        let milliseconds = env::var(variable)
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0)
            .min(120_000);
        Duration::from_millis(milliseconds)
    }

    #[cfg(target_os = "windows")]
    fn announce_phase(phase: &str) {
        println!("CAPYIO_MIC_PHYSICAL_PHASE={phase}");
        io::stdout().flush().expect("flush physical phase marker");
    }
}
