use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use capyio_audio_share_adapter::{
    AudioShareError, AudioShareSupervisor, ReceiverTcpPresence, SupervisorStatus,
};
use capyio_core::{
    AdapterDeploymentMode, AdapterHealth, AdapterInstanceDescriptor, AdapterInstanceId,
    AdapterState, Availability, CapabilityClass, CapabilityDescriptor, FormatDescriptor,
    InteroperabilityMode, NodeId, PermissionRequirement, PortDescriptor, PortDirection, PortRef,
    Problem, ProblemCategory, ProblemId, ProblemSeverity, ProfileId, QosMode, RouteBackend,
    RouteId, RouteState, SessionId,
};
use capyio_runtime::NodeRuntime;
use capyio_testkit::{ANDROID_NODE_ID, WINDOWS_NODE_ID};

const WINDOWS_ADAPTER_ID: &str = "00000000-0000-4000-8000-00000000c011";
const ANDROID_ADAPTER_ID: &str = "00000000-0000-4000-8000-00000000c021";
const WINDOWS_CAPABILITY_ID: &str = "00000000-0000-4000-8000-00000000c101";
const ANDROID_CAPABILITY_ID: &str = "00000000-0000-4000-8000-00000000c201";
const WINDOWS_PORT_ID: &str = "00000000-0000-4000-8000-00000000c111";
const ANDROID_PORT_ID: &str = "00000000-0000-4000-8000-00000000c211";
const ROUTE_ID: &str = "00000000-0000-4000-8000-00000000c911";
const AUDIO_FORMAT: &str = "audio-share-v0.3.4-private-negotiated";

pub const DEFAULT_STABLE_RECEIVER_POLLS: u8 = 3;
pub const DEFAULT_RECEIVER_WAIT_POLLS: u16 = 120;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioShareStartError {
    ConfiguredEndpointUnavailable,
    Other(String),
}

impl std::fmt::Display for AudioShareStartError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfiguredEndpointUnavailable => {
                formatter.write_str("the configured playback endpoint is no longer enumerated")
            }
            Self::Other(detail) => formatter.write_str(detail),
        }
    }
}

pub trait AudioShareProcessBoundary {
    fn start(&mut self) -> Result<(), AudioShareStartError>;
    fn status(&mut self) -> Result<SupervisorStatus, String>;
    fn receiver_presence(&mut self) -> Result<ReceiverTcpPresence, String>;
    fn stop(&mut self) -> Result<(), String>;
}

impl AudioShareProcessBoundary for AudioShareSupervisor {
    fn start(&mut self) -> Result<(), AudioShareStartError> {
        AudioShareSupervisor::start(self)
            .map(|_| ())
            .map_err(map_supervisor_start_error)
    }

    fn status(&mut self) -> Result<SupervisorStatus, String> {
        AudioShareSupervisor::status(self).map_err(|error| error.to_string())
    }

    fn receiver_presence(&mut self) -> Result<ReceiverTcpPresence, String> {
        AudioShareSupervisor::receiver_tcp_presence(self).map_err(|error| error.to_string())
    }

    fn stop(&mut self) -> Result<(), String> {
        AudioShareSupervisor::stop(self)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn map_supervisor_start_error(error: AudioShareError) -> AudioShareStartError {
    if matches!(error, AudioShareError::ConfiguredEndpointMissing { .. }) {
        AudioShareStartError::ConfiguredEndpointUnavailable
    } else {
        AudioShareStartError::Other(error.to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioShareRouteStatus {
    pub route_state: RouteState,
    pub route_epoch: u64,
    pub stable_receiver_polls: u8,
}

pub struct AudioShareRouteController<P> {
    process: P,
    route: AudioShareRoute,
    required_receiver_polls: u8,
    stable_receiver_polls: u8,
    required_receiver_wait_polls: u16,
    receiver_wait_polls: u16,
}

impl<P: AudioShareProcessBoundary> AudioShareRouteController<P> {
    pub fn install(
        runtime: &mut NodeRuntime,
        session_id: SessionId,
        process: P,
        required_receiver_polls: u8,
        required_receiver_wait_polls: u16,
    ) -> Result<Self, String> {
        if required_receiver_polls == 0 {
            return Err("stable receiver poll threshold must be positive".to_owned());
        }
        if required_receiver_wait_polls < u16::from(required_receiver_polls) {
            return Err(
                "receiver wait poll limit must cover the stable receiver threshold".to_owned(),
            );
        }
        Ok(Self {
            process,
            route: AudioShareRoute::install(runtime, session_id)?,
            required_receiver_polls,
            stable_receiver_polls: 0,
            required_receiver_wait_polls,
            receiver_wait_polls: 0,
        })
    }

    pub fn start(&mut self, runtime: &mut NodeRuntime, now_ms: u64) -> Result<u64, String> {
        let epoch = self.route.begin_start(runtime, now_ms)?;
        self.stable_receiver_polls = 0;
        self.receiver_wait_polls = 0;
        if !matches!(self.process.status(), Ok(SupervisorStatus::Running { .. }))
            && let Err(problem) = self.process.start()
        {
            let (code, message) = match &problem {
                AudioShareStartError::ConfiguredEndpointUnavailable => (
                    "CAPY.AUDIO_SHARE.ENDPOINT_UNAVAILABLE",
                    "The configured Windows playback endpoint is unavailable",
                ),
                AudioShareStartError::Other(_) => (
                    "CAPY.AUDIO_SHARE.PROCESS_START_FAILED",
                    "Audio Share could not start",
                ),
            };
            let detail = problem.to_string();
            self.route
                .report_offline(runtime, code, message, detail.clone())?;
            return Err(format!("Audio Share process start failed: {detail}"));
        }
        Ok(epoch)
    }

    pub fn poll(&mut self, runtime: &mut NodeRuntime) -> Result<AudioShareRouteStatus, String> {
        let route_state = self.route.state(runtime)?;
        if !matches!(route_state, RouteState::Starting | RouteState::Active) {
            return self.status(runtime);
        }

        match self.process.status() {
            Ok(SupervisorStatus::Running { .. }) => self.poll_running(runtime)?,
            Ok(SupervisorStatus::Exited(report)) => {
                self.stable_receiver_polls = 0;
                self.receiver_wait_polls = 0;
                self.route.report_offline(
                    runtime,
                    "CAPY.AUDIO_SHARE.PROCESS_EXITED",
                    "Audio Share stopped unexpectedly",
                    format!("Audio Share exited with code {:?}", report.exit_code),
                )?;
            }
            Ok(SupervisorStatus::Stopped) => {
                self.stable_receiver_polls = 0;
                self.receiver_wait_polls = 0;
                self.route.report_offline(
                    runtime,
                    "CAPY.AUDIO_SHARE.PROCESS_STOPPED",
                    "Audio Share is not running",
                    "the supervised process is stopped".to_owned(),
                )?;
            }
            Err(detail) => {
                self.stable_receiver_polls = 0;
                self.receiver_wait_polls = 0;
                self.route.report_offline(
                    runtime,
                    "CAPY.AUDIO_SHARE.PROCESS_STATUS_FAILED",
                    "Audio Share process status is unavailable",
                    detail,
                )?;
            }
        }
        self.status(runtime)
    }

    pub fn stop(&mut self, runtime: &mut NodeRuntime) -> Result<(), String> {
        let process_result = self.process.stop();
        self.stable_receiver_polls = 0;
        self.receiver_wait_polls = 0;
        self.route.stop(runtime)?;
        process_result
    }

    pub fn replace_process(&mut self, runtime: &NodeRuntime, process: P) -> Result<(), String> {
        match self.route.state(runtime)? {
            RouteState::Draft
            | RouteState::Prepared
            | RouteState::Stopped
            | RouteState::Offline => {}
            state => {
                return Err(format!(
                    "Audio Share endpoint cannot change while Route is {state:?}"
                ));
            }
        }
        self.process.stop()?;
        self.process = process;
        self.stable_receiver_polls = 0;
        self.receiver_wait_polls = 0;
        Ok(())
    }

    pub fn status(&self, runtime: &NodeRuntime) -> Result<AudioShareRouteStatus, String> {
        Ok(AudioShareRouteStatus {
            route_state: self.route.state(runtime)?,
            route_epoch: self.route.epoch(runtime)?,
            stable_receiver_polls: self.stable_receiver_polls,
        })
    }

    pub const fn route_id(&self) -> RouteId {
        self.route.route_id
    }

    pub fn route_state(&self, runtime: &NodeRuntime) -> Result<RouteState, String> {
        self.route.state(runtime)
    }

    fn poll_running(&mut self, runtime: &mut NodeRuntime) -> Result<(), String> {
        match self.process.receiver_presence() {
            Ok(ReceiverTcpPresence::Established { connection_count }) if connection_count > 0 => {
                self.stable_receiver_polls = self
                    .stable_receiver_polls
                    .saturating_add(1)
                    .min(self.required_receiver_polls);
                if self.stable_receiver_polls == self.required_receiver_polls
                    && self.route.state(runtime)? == RouteState::Starting
                {
                    self.route.activate(runtime)?;
                    self.receiver_wait_polls = 0;
                }
            }
            Ok(ReceiverTcpPresence::Disconnected) => {
                self.receiver_absent(runtime, "the receiver TCP connection is absent")?;
            }
            Ok(ReceiverTcpPresence::SupervisorNotRunning) => {
                self.receiver_absent(runtime, "the supervisor stopped during receiver polling")?;
            }
            Ok(ReceiverTcpPresence::UnsupportedPlatform) => {
                self.stable_receiver_polls = 0;
                self.receiver_wait_polls = 0;
                self.route.report_offline(
                    runtime,
                    "CAPY.AUDIO_SHARE.RECEIVER_OBSERVATION_UNSUPPORTED",
                    "Audio Share receiver observation is unsupported",
                    "the current platform cannot query owner-scoped TCP state".to_owned(),
                )?;
            }
            Err(detail) => {
                self.stable_receiver_polls = 0;
                self.receiver_wait_polls = 0;
                self.route.report_offline(
                    runtime,
                    "CAPY.AUDIO_SHARE.RECEIVER_OBSERVATION_FAILED",
                    "Audio Share receiver observation failed",
                    detail,
                )?;
            }
            Ok(ReceiverTcpPresence::Established { .. }) => {
                self.receiver_absent(runtime, "the receiver connection count is zero")?;
            }
        }
        if self.route.state(runtime)? == RouteState::Starting {
            self.receiver_wait_polls = self.receiver_wait_polls.saturating_add(1);
            if self.receiver_wait_polls >= self.required_receiver_wait_polls {
                self.receiver_wait_exhausted(runtime)?;
            }
        }
        Ok(())
    }

    fn receiver_absent(&mut self, runtime: &mut NodeRuntime, detail: &str) -> Result<(), String> {
        self.stable_receiver_polls = 0;
        if self.route.state(runtime)? == RouteState::Active {
            self.route.report_offline(
                runtime,
                "CAPY.AUDIO_SHARE.RECEIVER_TCP_LOST",
                "The Audio Share receiver disconnected",
                detail.to_owned(),
            )?;
        }
        Ok(())
    }

    fn receiver_wait_exhausted(&mut self, runtime: &mut NodeRuntime) -> Result<(), String> {
        let stop_problem = self.process.stop().err();
        self.stable_receiver_polls = 0;
        self.receiver_wait_polls = 0;
        let mut detail = format!(
            "receiver was not stable after {} bounded host polls",
            self.required_receiver_wait_polls
        );
        if let Some(problem) = stop_problem {
            detail.push_str("; process cleanup reported: ");
            detail.extend(problem.chars().take(512));
        }
        self.route.report_offline(
            runtime,
            "CAPY.AUDIO_SHARE.RECEIVER_WAIT_EXHAUSTED",
            "The Audio Share receiver did not connect",
            detail,
        )
    }
}

#[derive(Clone, Copy)]
struct AudioShareRoute {
    route_id: RouteId,
    android_node_id: NodeId,
    android_adapter_id: AdapterInstanceId,
}

impl AudioShareRoute {
    fn install(runtime: &mut NodeRuntime, session_id: SessionId) -> Result<Self, String> {
        let windows_node_id = parse_id(WINDOWS_NODE_ID)?;
        let android_node_id = parse_id(ANDROID_NODE_ID)?;
        let windows_adapter_id = parse_id(WINDOWS_ADAPTER_ID)?;
        let android_adapter_id = parse_id(ANDROID_ADAPTER_ID)?;
        runtime
            .register_adapter_catalog(
                windows_node_id,
                adapter(
                    windows_adapter_id,
                    "capyio.audio-share.windows",
                    "Audio Share Windows Server",
                ),
                vec![capability(CapabilitySpec {
                    adapter_id: windows_adapter_id,
                    capability_id: WINDOWS_CAPABILITY_ID,
                    capability_name: "Windows System Audio for Audio Share",
                    class: CapabilityClass::Custom("system_audio_capture".to_owned()),
                    port_id: WINDOWS_PORT_ID,
                    port_name: "Audio Share PCM Source",
                    direction: PortDirection::Source,
                })?],
            )
            .map_err(|error| error.to_string())?;
        runtime
            .register_adapter_catalog(
                android_node_id,
                adapter(
                    android_adapter_id,
                    "capyio.audio-share.android",
                    "Audio Share Android Receiver",
                ),
                vec![capability(CapabilitySpec {
                    adapter_id: android_adapter_id,
                    capability_id: ANDROID_CAPABILITY_ID,
                    capability_name: "Android Speaker via Audio Share",
                    class: CapabilityClass::Speaker,
                    port_id: ANDROID_PORT_ID,
                    port_name: "Audio Share Speaker Sink",
                    direction: PortDirection::Sink,
                })?],
            )
            .map_err(|error| error.to_string())?;
        let route_id = parse_id(ROUTE_ID)?;
        runtime
            .create_route_with_id(
                route_id,
                session_id,
                port_ref(windows_node_id, WINDOWS_CAPABILITY_ID, WINDOWS_PORT_ID)?,
                port_ref(android_node_id, ANDROID_CAPABILITY_ID, ANDROID_PORT_ID)?,
                RouteBackend::AdapterManaged,
            )
            .map_err(|error| error.to_string())?;
        Ok(Self {
            route_id,
            android_node_id,
            android_adapter_id,
        })
    }

    fn begin_start(self, runtime: &mut NodeRuntime, now_ms: u64) -> Result<u64, String> {
        match self.state(runtime)? {
            RouteState::Draft => {
                runtime
                    .authorize_route(self.route_id, None)
                    .map_err(string_error)?;
                runtime
                    .prepare_route(
                        self.route_id,
                        Some(FormatDescriptor::new(AUDIO_FORMAT)),
                        QosMode::Interactive,
                        now_ms,
                    )
                    .map_err(string_error)?;
            }
            RouteState::Stopped => runtime
                .prepare_route(
                    self.route_id,
                    Some(FormatDescriptor::new(AUDIO_FORMAT)),
                    QosMode::Interactive,
                    now_ms,
                )
                .map_err(string_error)?,
            RouteState::Offline => runtime
                .recover_route(self.route_id, now_ms)
                .map_err(string_error)?,
            RouteState::Prepared => {}
            state => return Err(format!("Audio Share Route cannot start from {state:?}")),
        }
        runtime
            .begin_route_start(self.route_id, now_ms)
            .map_err(string_error)?;
        self.epoch(runtime)
    }

    fn activate(self, runtime: &mut NodeRuntime) -> Result<(), String> {
        runtime.activate_route(self.route_id).map_err(string_error)
    }

    fn report_offline(
        self,
        runtime: &mut NodeRuntime,
        code: &str,
        message: &str,
        detail: String,
    ) -> Result<(), String> {
        runtime
            .report_route_offline(
                self.route_id,
                Problem {
                    id: ProblemId::new(),
                    code: code.to_owned(),
                    category: ProblemCategory::Transport,
                    severity: ProblemSeverity::Error,
                    retryable: true,
                    related_node: Some(self.android_node_id),
                    related_adapter: Some(self.android_adapter_id),
                    related_route: Some(self.route_id),
                    human_message: message.to_owned(),
                    technical_detail: Some(detail.chars().take(1024).collect()),
                },
            )
            .map_err(string_error)
    }

    fn stop(self, runtime: &mut NodeRuntime) -> Result<(), String> {
        match self.state(runtime)? {
            RouteState::Prepared
            | RouteState::Starting
            | RouteState::Active
            | RouteState::Offline => {
                runtime
                    .begin_route_stop(self.route_id)
                    .map_err(string_error)?;
                runtime.stop_route(self.route_id).map_err(string_error)
            }
            RouteState::Stopping => runtime.stop_route(self.route_id).map_err(string_error),
            RouteState::Draft | RouteState::Stopped => Ok(()),
            RouteState::Failed => Err("failed Audio Share Route cannot be stopped".to_owned()),
        }
    }

    fn state(self, runtime: &NodeRuntime) -> Result<RouteState, String> {
        runtime
            .route(self.route_id)
            .map(|route| route.state)
            .map_err(string_error)
    }

    fn epoch(self, runtime: &NodeRuntime) -> Result<u64, String> {
        runtime
            .route(self.route_id)
            .map(|route| route.epoch)
            .map_err(string_error)
    }
}

fn adapter(
    id: AdapterInstanceId,
    adapter_type: &str,
    display_name: &str,
) -> AdapterInstanceDescriptor {
    AdapterInstanceDescriptor {
        id,
        adapter_type: adapter_type.to_owned(),
        display_name: display_name.to_owned(),
        deployment_mode: AdapterDeploymentMode::ExternalService,
        version: env!("CARGO_PKG_VERSION").to_owned(),
        state: AdapterState::Ready,
        health: AdapterHealth::Healthy,
        owned_capabilities: BTreeSet::new(),
        supported_route_modes: BTreeSet::from([RouteBackend::AdapterManaged]),
    }
}

struct CapabilitySpec<'a> {
    adapter_id: AdapterInstanceId,
    capability_id: &'a str,
    capability_name: &'a str,
    class: CapabilityClass,
    port_id: &'a str,
    port_name: &'a str,
    direction: PortDirection,
}

fn capability(spec: CapabilitySpec<'_>) -> Result<CapabilityDescriptor, String> {
    let capability_id = parse_id(spec.capability_id)?;
    let port_id = parse_id(spec.port_id)?;
    Ok(CapabilityDescriptor {
        id: capability_id,
        adapter_instance_id: spec.adapter_id,
        display_name: spec.capability_name.to_owned(),
        class: spec.class,
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::None,
        metadata: BTreeMap::new(),
        ports: BTreeMap::from([(
            port_id,
            PortDescriptor {
                id: port_id,
                capability_id,
                display_name: spec.port_name.to_owned(),
                direction: spec.direction,
                profile: ProfileId::audio_frames_v1(),
                schema_id: None,
                formats: vec![FormatDescriptor::new(AUDIO_FORMAT)],
                qos_modes: BTreeSet::from([QosMode::Interactive]),
                clock_domain: Some("audio-share.process".to_owned()),
                availability: Availability::Available,
                permission_requirement: PermissionRequirement::None,
                interoperability_mode: InteroperabilityMode::AdapterManaged,
            },
        )]),
    })
}

fn port_ref(node_id: NodeId, capability_id: &str, port_id: &str) -> Result<PortRef, String> {
    Ok(PortRef {
        node_id,
        capability_id: parse_id(capability_id)?,
        port_id: parse_id(port_id)?,
    })
}

fn parse_id<T: FromStr>(value: &str) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    value.parse::<T>().map_err(|error| error.to_string())
}

fn string_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        net::IpAddr,
        path::PathBuf,
        thread,
        time::{Duration, Instant},
    };

    use capyio_audio_share_adapter::{
        AudioEncoding, AudioShareConfig, AudioShareSupervisor, ProbeLimits, ProcessExitReport,
        ProcessOutputSummary, SupervisorLimits,
    };
    use capyio_testkit::DemoLab;

    use super::*;

    #[derive(Default)]
    struct FakeProcess {
        running: bool,
        presences: VecDeque<ReceiverTcpPresence>,
        exit: Option<ProcessExitReport>,
        start_error: Option<AudioShareStartError>,
        starts: u32,
        stops: u32,
    }

    impl FakeProcess {
        fn with_presences(presences: impl IntoIterator<Item = ReceiverTcpPresence>) -> Self {
            Self {
                presences: presences.into_iter().collect(),
                ..Self::default()
            }
        }
    }

    impl AudioShareProcessBoundary for FakeProcess {
        fn start(&mut self) -> Result<(), AudioShareStartError> {
            if let Some(error) = self.start_error.clone() {
                return Err(error);
            }
            self.running = true;
            self.exit = None;
            self.starts += 1;
            Ok(())
        }

        fn status(&mut self) -> Result<SupervisorStatus, String> {
            if let Some(exit) = self.exit {
                Ok(SupervisorStatus::Exited(exit))
            } else if self.running {
                Ok(SupervisorStatus::Running { process_id: 42 })
            } else {
                Ok(SupervisorStatus::Stopped)
            }
        }

        fn receiver_presence(&mut self) -> Result<ReceiverTcpPresence, String> {
            Ok(self
                .presences
                .pop_front()
                .unwrap_or(ReceiverTcpPresence::Disconnected))
        }

        fn stop(&mut self) -> Result<(), String> {
            self.running = false;
            self.stops += 1;
            Ok(())
        }
    }

    fn established() -> ReceiverTcpPresence {
        ReceiverTcpPresence::Established {
            connection_count: 1,
        }
    }

    #[test]
    fn stable_receiver_presence_activates_then_loss_offlines_only_audio_route() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let imu_route = lab.routes.phone_imu_to_gamepad;
        lab.set_route_active(imu_route, true, 1)
            .expect("activate IMU Route");
        let process = FakeProcess::with_presences([
            established(),
            ReceiverTcpPresence::Disconnected,
            established(),
            established(),
            established(),
            ReceiverTcpPresence::Disconnected,
        ]);
        let session_id = lab.session_id;
        let mut controller = AudioShareRouteController::install(
            &mut lab.runtime,
            session_id,
            process,
            DEFAULT_STABLE_RECEIVER_POLLS,
            DEFAULT_RECEIVER_WAIT_POLLS,
        )
        .expect("install Audio Share Route");
        let first_epoch = controller.start(&mut lab.runtime, 2).expect("start audio");

        for expected in [1, 0, 1, 2] {
            let status = controller.poll(&mut lab.runtime).expect("poll receiver");
            assert_eq!(status.route_state, RouteState::Starting);
            assert_eq!(status.stable_receiver_polls, expected);
        }
        assert_eq!(
            controller
                .poll(&mut lab.runtime)
                .expect("stable receiver")
                .route_state,
            RouteState::Active
        );
        let offline = controller.poll(&mut lab.runtime).expect("receiver loss");
        assert_eq!(offline.route_state, RouteState::Offline);
        assert!(offline.route_epoch > first_epoch);
        assert_eq!(
            lab.runtime.route(imu_route).expect("IMU Route").state,
            RouteState::Active
        );
        assert!(lab.runtime.snapshot().problems.iter().any(|problem| {
            problem.code == "CAPY.AUDIO_SHARE.RECEIVER_TCP_LOST"
                && problem.related_route == Some(controller.route_id())
        }));
    }

    #[test]
    fn adapter_managed_route_does_not_claim_the_requested_pcm_format() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let session_id = lab.session_id;
        let mut controller = AudioShareRouteController::install(
            &mut lab.runtime,
            session_id,
            FakeProcess::default(),
            1,
            DEFAULT_RECEIVER_WAIT_POLLS,
        )
        .expect("install Route");
        controller.start(&mut lab.runtime, 1).expect("start");
        let route = lab
            .runtime
            .route(controller.route_id())
            .expect("Audio Share Route");
        assert_eq!(
            route
                .selected_format
                .as_ref()
                .map(|format| format.id.as_str()),
            Some(AUDIO_FORMAT)
        );
        assert_ne!(AUDIO_FORMAT, "pcm-s16le-48000-stereo");
    }

    #[test]
    fn explicit_retry_advances_epoch_and_stop_is_terminal() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let process = FakeProcess::with_presences([
            established(),
            established(),
            established(),
            ReceiverTcpPresence::Disconnected,
            established(),
            established(),
            established(),
        ]);
        let session_id = lab.session_id;
        let mut controller = AudioShareRouteController::install(
            &mut lab.runtime,
            session_id,
            process,
            3,
            DEFAULT_RECEIVER_WAIT_POLLS,
        )
        .expect("install Route");
        let first_epoch = controller.start(&mut lab.runtime, 1).expect("first start");
        for _ in 0..3 {
            controller.poll(&mut lab.runtime).expect("activation poll");
        }
        controller.poll(&mut lab.runtime).expect("disconnect poll");
        let offline_epoch = controller
            .status(&lab.runtime)
            .expect("offline")
            .route_epoch;
        assert!(offline_epoch > first_epoch);
        let retry_epoch = controller.start(&mut lab.runtime, 2).expect("retry");
        assert!(retry_epoch > offline_epoch);
        for _ in 0..3 {
            controller.poll(&mut lab.runtime).expect("retry poll");
        }
        assert_eq!(
            controller.status(&lab.runtime).expect("active").route_state,
            RouteState::Active
        );
        controller.stop(&mut lab.runtime).expect("stop");
        assert_eq!(
            controller
                .status(&lab.runtime)
                .expect("stopped")
                .route_state,
            RouteState::Stopped
        );
        assert!(!controller.process.running);
        assert_eq!(controller.process.starts, 1);
        assert_eq!(controller.process.stops, 1);
    }

    #[test]
    fn receiver_wait_exhaustion_offlines_reaps_and_allows_later_retry() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let imu_route = lab.routes.phone_imu_to_gamepad;
        lab.set_route_active(imu_route, true, 1)
            .expect("activate IMU Route");
        let session_id = lab.session_id;
        let mut controller = AudioShareRouteController::install(
            &mut lab.runtime,
            session_id,
            FakeProcess::default(),
            2,
            3,
        )
        .expect("install Route");
        let first_epoch = controller.start(&mut lab.runtime, 2).expect("start");

        for _ in 0..2 {
            assert_eq!(
                controller
                    .poll(&mut lab.runtime)
                    .expect("bounded receiver wait")
                    .route_state,
                RouteState::Starting
            );
        }
        let offline = controller
            .poll(&mut lab.runtime)
            .expect("receiver wait exhaustion");
        assert_eq!(offline.route_state, RouteState::Offline);
        assert!(offline.route_epoch > first_epoch);
        assert!(!controller.process.running);
        assert_eq!(controller.process.stops, 1);
        assert_eq!(
            lab.runtime.route(imu_route).expect("IMU Route").state,
            RouteState::Active
        );
        assert!(lab.runtime.snapshot().problems.iter().any(|problem| {
            problem.code == "CAPY.AUDIO_SHARE.RECEIVER_WAIT_EXHAUSTED"
                && problem.related_route == Some(controller.route_id())
                && problem.retryable
        }));

        controller
            .process
            .presences
            .extend([established(), established()]);
        let retry_epoch = controller.start(&mut lab.runtime, 3).expect("retry");
        assert!(retry_epoch > offline.route_epoch);
        controller
            .poll(&mut lab.runtime)
            .expect("first stable poll");
        assert_eq!(
            controller
                .poll(&mut lab.runtime)
                .expect("second stable poll")
                .route_state,
            RouteState::Active
        );
        assert_eq!(controller.process.starts, 2);
    }

    #[test]
    fn child_exit_offlines_route_with_typed_problem() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let process = FakeProcess::with_presences([]);
        let session_id = lab.session_id;
        let mut controller = AudioShareRouteController::install(
            &mut lab.runtime,
            session_id,
            process,
            1,
            DEFAULT_RECEIVER_WAIT_POLLS,
        )
        .expect("install Route");
        controller.start(&mut lab.runtime, 1).expect("start");
        controller.process.exit = Some(ProcessExitReport {
            exit_code: Some(23),
            output: ProcessOutputSummary {
                stdout_retained_bytes: 0,
                stderr_retained_bytes: 0,
                stdout_overflowed: false,
                stderr_overflowed: false,
            },
        });
        assert_eq!(
            controller
                .poll(&mut lab.runtime)
                .expect("exit poll")
                .route_state,
            RouteState::Offline
        );
        assert!(lab.runtime.snapshot().problems.iter().any(|problem| {
            problem.code == "CAPY.AUDIO_SHARE.PROCESS_EXITED"
                && problem
                    .technical_detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("23"))
        }));
    }

    #[test]
    fn process_start_failure_offlines_route_with_typed_problem() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let process = FakeProcess {
            start_error: Some(AudioShareStartError::Other(
                "fixture spawn denied".to_owned(),
            )),
            ..FakeProcess::default()
        };
        let session_id = lab.session_id;
        let mut controller = AudioShareRouteController::install(
            &mut lab.runtime,
            session_id,
            process,
            1,
            DEFAULT_RECEIVER_WAIT_POLLS,
        )
        .expect("install Route");
        let error = controller
            .start(&mut lab.runtime, 1)
            .expect_err("start must fail");
        assert!(error.contains("fixture spawn denied"));
        assert_eq!(
            controller
                .status(&lab.runtime)
                .expect("offline")
                .route_state,
            RouteState::Offline
        );
        assert!(lab.runtime.snapshot().problems.iter().any(|problem| {
            problem.code == "CAPY.AUDIO_SHARE.PROCESS_START_FAILED"
                && problem.related_route == Some(controller.route_id())
        }));
    }

    #[test]
    fn stale_configured_endpoint_has_a_stable_sanitized_problem() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let session_id = lab.session_id;
        let process = FakeProcess {
            start_error: Some(AudioShareStartError::ConfiguredEndpointUnavailable),
            ..FakeProcess::default()
        };
        let mut controller = AudioShareRouteController::install(
            &mut lab.runtime,
            session_id,
            process,
            1,
            DEFAULT_RECEIVER_WAIT_POLLS,
        )
        .expect("install Route");

        let error = controller
            .start(&mut lab.runtime, 1)
            .expect_err("stale endpoint must fail");
        assert!(error.contains("no longer enumerated"));
        assert_eq!(
            controller
                .status(&lab.runtime)
                .expect("offline")
                .route_state,
            RouteState::Offline
        );
        let problem = lab
            .runtime
            .snapshot()
            .problems
            .into_iter()
            .find(|problem| problem.related_route == Some(controller.route_id()))
            .expect("endpoint Problem");
        assert_eq!(problem.code, "CAPY.AUDIO_SHARE.ENDPOINT_UNAVAILABLE");
        assert!(problem.retryable);
        assert_eq!(
            problem.technical_detail.as_deref(),
            Some("the configured playback endpoint is no longer enumerated")
        );
    }

    #[test]
    fn concrete_missing_endpoint_error_maps_without_retaining_the_endpoint_id() {
        let mapped = map_supervisor_start_error(AudioShareError::ConfiguredEndpointMissing {
            endpoint_id: "sensitive-endpoint-id".to_owned(),
        });
        assert_eq!(mapped, AudioShareStartError::ConfiguredEndpointUnavailable);
        assert!(!mapped.to_string().contains("sensitive-endpoint-id"));
    }

    #[test]
    fn zero_stability_threshold_is_rejected_before_catalog_mutation() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let session_id = lab.session_id;
        let result = AudioShareRouteController::install(
            &mut lab.runtime,
            session_id,
            FakeProcess::default(),
            0,
            DEFAULT_RECEIVER_WAIT_POLLS,
        );
        assert!(result.is_err());
        assert!(
            lab.runtime
                .route(parse_id(ROUTE_ID).expect("Route ID"))
                .is_err()
        );
    }

    #[test]
    fn receiver_wait_limit_must_cover_stability_before_catalog_mutation() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let session_id = lab.session_id;
        let result = AudioShareRouteController::install(
            &mut lab.runtime,
            session_id,
            FakeProcess::default(),
            3,
            2,
        );
        assert!(result.is_err());
        assert!(
            lab.runtime
                .route(parse_id(ROUTE_ID).expect("Route ID"))
                .is_err()
        );
    }

    #[test]
    fn inactive_route_accepts_a_replacement_process() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let session_id = lab.session_id;
        let mut controller = AudioShareRouteController::install(
            &mut lab.runtime,
            session_id,
            FakeProcess::default(),
            1,
            DEFAULT_RECEIVER_WAIT_POLLS,
        )
        .expect("install Route");

        controller
            .replace_process(&lab.runtime, FakeProcess::with_presences([established()]))
            .expect("replace inactive process");
        controller
            .start(&mut lab.runtime, 1)
            .expect("start replacement");
        assert_eq!(controller.process.starts, 1);
        assert_eq!(
            controller
                .poll(&mut lab.runtime)
                .expect("activate")
                .route_state,
            RouteState::Active
        );
    }

    #[test]
    fn active_route_rejects_process_replacement() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let session_id = lab.session_id;
        let mut controller = AudioShareRouteController::install(
            &mut lab.runtime,
            session_id,
            FakeProcess::with_presences([established()]),
            1,
            DEFAULT_RECEIVER_WAIT_POLLS,
        )
        .expect("install Route");
        controller.start(&mut lab.runtime, 1).expect("start");
        controller.poll(&mut lab.runtime).expect("activate");

        let error = controller
            .replace_process(&lab.runtime, FakeProcess::default())
            .expect_err("active replacement must fail");
        assert!(error.contains("Active"));
        assert_eq!(controller.process.starts, 1);
    }

    #[test]
    #[ignore = "requires the authorized physical Audio Share Windows/Android lab"]
    fn physical_audio_share_receiver_disconnect_retry_and_stop() {
        let executable = std::env::var_os("CAPYIO_AUDIO_SHARE_EXE")
            .map(PathBuf::from)
            .expect("set CAPYIO_AUDIO_SHARE_EXE to the hash-verified v0.3.4 CLI");
        let bind_ip = std::env::var("CAPYIO_AUDIO_SHARE_BIND_IP")
            .expect("set CAPYIO_AUDIO_SHARE_BIND_IP")
            .parse::<IpAddr>()
            .expect("bind IP literal");
        let port = std::env::var("CAPYIO_AUDIO_SHARE_PORT")
            .expect("set CAPYIO_AUDIO_SHARE_PORT")
            .parse::<u16>()
            .expect("non-zero port");
        let endpoint =
            std::env::var("CAPYIO_AUDIO_SHARE_ENDPOINT").expect("set CAPYIO_AUDIO_SHARE_ENDPOINT");
        let config = AudioShareConfig::new(
            executable,
            bind_ip,
            port,
            endpoint,
            AudioEncoding::S16,
            2,
            48_000,
        )
        .expect("physical lab configuration");
        let supervisor =
            AudioShareSupervisor::new(config, ProbeLimits::default(), SupervisorLimits::default())
                .expect("supervisor");
        let mut lab = DemoLab::new().expect("demo Runtime");
        let imu_route = lab.routes.phone_imu_to_gamepad;
        lab.set_route_active(imu_route, true, 1)
            .expect("independent IMU Route");
        let session_id = lab.session_id;
        let mut controller = AudioShareRouteController::install(
            &mut lab.runtime,
            session_id,
            supervisor,
            DEFAULT_STABLE_RECEIVER_POLLS,
            DEFAULT_RECEIVER_WAIT_POLLS,
        )
        .expect("physical audio Route");
        let first_epoch = controller.start(&mut lab.runtime, 2).expect("start server");
        eprintln!("CAPYIO_PHYSICAL_AUDIO_WAITING_FOR_RECEIVER");
        wait_for_state(
            &mut controller,
            &mut lab.runtime,
            RouteState::Active,
            Duration::from_secs(60),
        );
        eprintln!("CAPYIO_PHYSICAL_AUDIO_ACTIVE epoch={first_epoch}");
        wait_for_state(
            &mut controller,
            &mut lab.runtime,
            RouteState::Offline,
            Duration::from_secs(60),
        );
        let offline_epoch = controller
            .status(&lab.runtime)
            .expect("offline status")
            .route_epoch;
        assert!(offline_epoch > first_epoch);
        eprintln!("CAPYIO_PHYSICAL_AUDIO_OFFLINE epoch={offline_epoch}");
        let retry_epoch = controller.start(&mut lab.runtime, 3).expect("retry server");
        assert!(retry_epoch > offline_epoch);
        eprintln!("CAPYIO_PHYSICAL_AUDIO_WAITING_FOR_RETRY_RECEIVER epoch={retry_epoch}");
        wait_for_state(
            &mut controller,
            &mut lab.runtime,
            RouteState::Active,
            Duration::from_secs(60),
        );
        assert_eq!(
            lab.runtime.route(imu_route).expect("IMU Route").state,
            RouteState::Active
        );
        controller.stop(&mut lab.runtime).expect("explicit stop");
        assert_eq!(
            controller
                .status(&lab.runtime)
                .expect("stopped")
                .route_state,
            RouteState::Stopped
        );
        eprintln!("CAPYIO_PHYSICAL_AUDIO_COMPLETE");
    }

    fn wait_for_state<P: AudioShareProcessBoundary>(
        controller: &mut AudioShareRouteController<P>,
        runtime: &mut NodeRuntime,
        expected: RouteState,
        timeout: Duration,
    ) {
        let deadline = Instant::now() + timeout;
        loop {
            let status = controller.poll(runtime).expect("physical poll");
            if status.route_state == expected {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {expected:?}, current {:?}",
                status.route_state
            );
            thread::sleep(Duration::from_millis(250));
        }
    }
}
