use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use capyio_core::{
    AdapterDeploymentMode, AdapterHealth, AdapterInstanceDescriptor, AdapterInstanceId,
    AdapterState, Availability, CapabilityClass, CapabilityDescriptor, FormatDescriptor,
    InteroperabilityMode, NodeId, PermissionRequirement, PortDescriptor, PortDirection, PortRef,
    Problem, ProblemCategory, ProblemId, ProblemSeverity, ProfileId, QosMode, RouteBackend,
    RouteId, RouteState, SessionId,
};
use capyio_micyou_adapter::{MicYouError, MicYouSupervisor, PeerTcpPresence, SupervisorStatus};
use capyio_runtime::NodeRuntime;
use capyio_testkit::{ANDROID_NODE_ID, WINDOWS_NODE_ID};

const ANDROID_ADAPTER_ID: &str = "00000000-0000-4000-8000-00000000d011";
const WINDOWS_ADAPTER_ID: &str = "00000000-0000-4000-8000-00000000d021";
const ANDROID_CAPABILITY_ID: &str = "00000000-0000-4000-8000-00000000d101";
const WINDOWS_CAPABILITY_ID: &str = "00000000-0000-4000-8000-00000000d201";
const ANDROID_PORT_ID: &str = "00000000-0000-4000-8000-00000000d111";
const WINDOWS_PORT_ID: &str = "00000000-0000-4000-8000-00000000d211";
const ROUTE_ID: &str = "00000000-0000-4000-8000-00000000d911";
const AUDIO_FORMAT: &str = "micyou-v2.0.1-private-negotiated";

pub const DEFAULT_STABLE_PHONE_POLLS: u8 = 3;
pub const DEFAULT_PHONE_WAIT_POLLS: u16 = 120;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MicYouStartError {
    ConfiguredEndpointUnavailable,
    Other(String),
}

impl std::fmt::Display for MicYouStartError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfiguredEndpointUnavailable => {
                formatter.write_str("the configured microphone ingress endpoint is unavailable")
            }
            Self::Other(detail) => formatter.write_str(detail),
        }
    }
}

pub trait MicYouProcessBoundary {
    fn start(&mut self) -> Result<(), MicYouStartError>;
    fn status(&mut self) -> Result<SupervisorStatus, String>;
    fn phone_presence(&mut self) -> Result<PeerTcpPresence, String>;
    fn stop(&mut self) -> Result<(), String>;
}

impl MicYouProcessBoundary for MicYouSupervisor {
    fn start(&mut self) -> Result<(), MicYouStartError> {
        MicYouSupervisor::start(self)
            .map(|_| ())
            .map_err(map_supervisor_start_error)
    }

    fn status(&mut self) -> Result<SupervisorStatus, String> {
        MicYouSupervisor::status(self).map_err(|error| error.to_string())
    }

    fn phone_presence(&mut self) -> Result<PeerTcpPresence, String> {
        MicYouSupervisor::peer_tcp_presence(self).map_err(|error| error.to_string())
    }

    fn stop(&mut self) -> Result<(), String> {
        MicYouSupervisor::stop(self)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn map_supervisor_start_error(error: MicYouError) -> MicYouStartError {
    if matches!(
        error,
        MicYouError::ConfiguredDeviceMissing
            | MicYouError::ConfiguredDeviceChanged
            | MicYouError::DuplicateDeviceId
    ) {
        MicYouStartError::ConfiguredEndpointUnavailable
    } else {
        MicYouStartError::Other(error.to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MicYouRouteStatus {
    pub route_state: RouteState,
    pub route_epoch: u64,
    pub stable_phone_polls: u8,
}

pub struct MicYouRouteController<P> {
    process: P,
    route: MicYouRoute,
    required_phone_polls: u8,
    stable_phone_polls: u8,
    required_phone_wait_polls: u16,
    phone_wait_polls: u16,
}

impl<P: MicYouProcessBoundary> MicYouRouteController<P> {
    pub fn install(
        runtime: &mut NodeRuntime,
        session_id: SessionId,
        process: P,
        required_phone_polls: u8,
        required_phone_wait_polls: u16,
    ) -> Result<Self, String> {
        if required_phone_polls == 0 {
            return Err("stable phone poll threshold must be positive".to_owned());
        }
        if required_phone_wait_polls < u16::from(required_phone_polls) {
            return Err("phone wait poll limit must cover the stable threshold".to_owned());
        }
        Ok(Self {
            process,
            route: MicYouRoute::install(runtime, session_id)?,
            required_phone_polls,
            stable_phone_polls: 0,
            required_phone_wait_polls,
            phone_wait_polls: 0,
        })
    }

    pub fn start(&mut self, runtime: &mut NodeRuntime, now_ms: u64) -> Result<u64, String> {
        let epoch = self.route.begin_start(runtime, now_ms)?;
        self.stable_phone_polls = 0;
        self.phone_wait_polls = 0;
        if !matches!(self.process.status(), Ok(SupervisorStatus::Running { .. }))
            && let Err(problem) = self.process.start()
        {
            let (code, message) = match &problem {
                MicYouStartError::ConfiguredEndpointUnavailable => (
                    "CAPY.MICYOU.ENDPOINT_UNAVAILABLE",
                    "The CapyIO microphone ingress endpoint is unavailable",
                ),
                MicYouStartError::Other(_) => (
                    "CAPY.MICYOU.PROCESS_START_FAILED",
                    "The MicYou receiver could not start",
                ),
            };
            let detail = problem.to_string();
            self.route.report_offline(
                runtime,
                self.route.windows_adapter_id,
                code,
                message,
                detail.clone(),
            )?;
            return Err(format!("MicYou process start failed: {detail}"));
        }
        Ok(epoch)
    }

    pub fn poll(&mut self, runtime: &mut NodeRuntime) -> Result<MicYouRouteStatus, String> {
        let route_state = self.route.state(runtime)?;
        if !matches!(route_state, RouteState::Starting | RouteState::Active) {
            return self.status(runtime);
        }

        match self.process.status() {
            Ok(SupervisorStatus::Running { .. }) => self.poll_running(runtime)?,
            Ok(SupervisorStatus::Exited { exit_code }) => {
                self.reset_poll_state();
                self.route.report_offline(
                    runtime,
                    self.route.windows_adapter_id,
                    "CAPY.MICYOU.PROCESS_EXITED",
                    "The MicYou receiver stopped unexpectedly",
                    format!("MicYou exited with code {exit_code:?}"),
                )?;
            }
            Ok(SupervisorStatus::Stopped) => {
                self.reset_poll_state();
                self.route.report_offline(
                    runtime,
                    self.route.windows_adapter_id,
                    "CAPY.MICYOU.PROCESS_STOPPED",
                    "The MicYou receiver is not running",
                    "the supervised process is stopped".to_owned(),
                )?;
            }
            Err(detail) => {
                self.reset_poll_state();
                self.route.report_offline(
                    runtime,
                    self.route.windows_adapter_id,
                    "CAPY.MICYOU.PROCESS_STATUS_FAILED",
                    "MicYou process status is unavailable",
                    detail,
                )?;
            }
        }
        self.status(runtime)
    }

    pub fn stop(&mut self, runtime: &mut NodeRuntime) -> Result<(), String> {
        let process_result = self.process.stop();
        self.reset_poll_state();
        self.route.stop(runtime)?;
        process_result
    }

    pub fn status(&self, runtime: &NodeRuntime) -> Result<MicYouRouteStatus, String> {
        Ok(MicYouRouteStatus {
            route_state: self.route.state(runtime)?,
            route_epoch: self.route.epoch(runtime)?,
            stable_phone_polls: self.stable_phone_polls,
        })
    }

    pub const fn route_id(&self) -> RouteId {
        self.route.route_id
    }

    pub fn route_state(&self, runtime: &NodeRuntime) -> Result<RouteState, String> {
        self.route.state(runtime)
    }

    fn poll_running(&mut self, runtime: &mut NodeRuntime) -> Result<(), String> {
        match self.process.phone_presence() {
            Ok(PeerTcpPresence::Established { connection_count }) if connection_count > 0 => {
                self.stable_phone_polls = self
                    .stable_phone_polls
                    .saturating_add(1)
                    .min(self.required_phone_polls);
                if self.stable_phone_polls == self.required_phone_polls
                    && self.route.state(runtime)? == RouteState::Starting
                {
                    self.route.activate(runtime)?;
                    self.phone_wait_polls = 0;
                }
            }
            Ok(PeerTcpPresence::Disconnected) => {
                self.phone_absent(runtime, "the phone TCP connection is absent")?;
            }
            Ok(PeerTcpPresence::SupervisorNotRunning) => {
                self.phone_absent(runtime, "the supervisor stopped during phone polling")?;
            }
            Ok(PeerTcpPresence::UnsupportedPlatform) => {
                self.reset_poll_state();
                self.route.report_offline(
                    runtime,
                    self.route.windows_adapter_id,
                    "CAPY.MICYOU.PEER_OBSERVATION_UNSUPPORTED",
                    "MicYou phone observation is unsupported",
                    "the current platform cannot query process-owned TCP state".to_owned(),
                )?;
            }
            Err(detail) => {
                self.reset_poll_state();
                self.route.report_offline(
                    runtime,
                    self.route.windows_adapter_id,
                    "CAPY.MICYOU.PEER_OBSERVATION_FAILED",
                    "MicYou phone observation failed",
                    detail,
                )?;
            }
            Ok(PeerTcpPresence::Established { .. }) => {
                self.phone_absent(runtime, "the phone connection count is zero")?;
            }
        }
        if self.route.state(runtime)? == RouteState::Starting {
            self.phone_wait_polls = self.phone_wait_polls.saturating_add(1);
            if self.phone_wait_polls >= self.required_phone_wait_polls {
                self.phone_wait_exhausted(runtime)?;
            }
        }
        Ok(())
    }

    fn phone_absent(&mut self, runtime: &mut NodeRuntime, detail: &str) -> Result<(), String> {
        self.stable_phone_polls = 0;
        if self.route.state(runtime)? == RouteState::Active {
            let stop_problem = self.process.stop().err();
            self.reset_poll_state();
            let mut detail = detail.to_owned();
            if let Some(problem) = stop_problem {
                detail.push_str("; process cleanup reported: ");
                detail.extend(problem.chars().take(512));
            }
            self.route.report_offline(
                runtime,
                self.route.android_adapter_id,
                "CAPY.MICYOU.PHONE_TCP_LOST",
                "The MicYou phone disconnected",
                detail,
            )?;
        }
        Ok(())
    }

    fn phone_wait_exhausted(&mut self, runtime: &mut NodeRuntime) -> Result<(), String> {
        let stop_problem = self.process.stop().err();
        self.reset_poll_state();
        let mut detail = format!(
            "phone was not stable after {} bounded host polls",
            self.required_phone_wait_polls
        );
        if let Some(problem) = stop_problem {
            detail.push_str("; process cleanup reported: ");
            detail.extend(problem.chars().take(512));
        }
        self.route.report_offline(
            runtime,
            self.route.android_adapter_id,
            "CAPY.MICYOU.PHONE_WAIT_EXHAUSTED",
            "The MicYou phone did not connect",
            detail,
        )
    }

    fn reset_poll_state(&mut self) {
        self.stable_phone_polls = 0;
        self.phone_wait_polls = 0;
    }
}

#[derive(Clone, Copy)]
struct MicYouRoute {
    route_id: RouteId,
    android_node_id: NodeId,
    windows_node_id: NodeId,
    android_adapter_id: AdapterInstanceId,
    windows_adapter_id: AdapterInstanceId,
}

impl MicYouRoute {
    fn install(runtime: &mut NodeRuntime, session_id: SessionId) -> Result<Self, String> {
        let windows_node_id = parse_id(WINDOWS_NODE_ID)?;
        let android_node_id = parse_id(ANDROID_NODE_ID)?;
        let android_adapter_id = parse_id(ANDROID_ADAPTER_ID)?;
        let windows_adapter_id = parse_id(WINDOWS_ADAPTER_ID)?;
        runtime
            .register_adapter_catalog(
                android_node_id,
                adapter(
                    android_adapter_id,
                    "capyio.micyou.android",
                    "MicYou Android Microphone",
                ),
                vec![capability(CapabilitySpec {
                    adapter_id: android_adapter_id,
                    capability_id: ANDROID_CAPABILITY_ID,
                    capability_name: "Android Microphone via MicYou",
                    class: CapabilityClass::Microphone,
                    port_id: ANDROID_PORT_ID,
                    port_name: "MicYou Microphone Source",
                    direction: PortDirection::Source,
                    permission: PermissionRequirement::ForegroundService,
                    clock_domain: "micyou.android-audio-record",
                })?],
            )
            .map_err(|error| error.to_string())?;
        runtime
            .register_adapter_catalog(
                windows_node_id,
                adapter(
                    windows_adapter_id,
                    "capyio.micyou.windows",
                    "MicYou Windows Receiver",
                ),
                vec![capability(CapabilitySpec {
                    adapter_id: windows_adapter_id,
                    capability_id: WINDOWS_CAPABILITY_ID,
                    capability_name: "CapyIO Microphone Projection",
                    class: CapabilityClass::Microphone,
                    port_id: WINDOWS_PORT_ID,
                    port_name: "CapyIO Microphone Sink",
                    direction: PortDirection::Sink,
                    permission: PermissionRequirement::None,
                    clock_domain: "windows.audio-engine",
                })?],
            )
            .map_err(|error| error.to_string())?;
        let route_id = parse_id(ROUTE_ID)?;
        runtime
            .create_route_with_id(
                route_id,
                session_id,
                port_ref(android_node_id, ANDROID_CAPABILITY_ID, ANDROID_PORT_ID)?,
                port_ref(windows_node_id, WINDOWS_CAPABILITY_ID, WINDOWS_PORT_ID)?,
                RouteBackend::AdapterManaged,
            )
            .map_err(|error| error.to_string())?;
        Ok(Self {
            route_id,
            android_node_id,
            windows_node_id,
            android_adapter_id,
            windows_adapter_id,
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
            state => return Err(format!("MicYou Route cannot start from {state:?}")),
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
        related_adapter: AdapterInstanceId,
        code: &str,
        message: &str,
        detail: String,
    ) -> Result<(), String> {
        let related_node = if related_adapter == self.windows_adapter_id {
            self.windows_node_id
        } else {
            self.android_node_id
        };
        runtime
            .report_route_offline(
                self.route_id,
                Problem {
                    id: ProblemId::new(),
                    code: code.to_owned(),
                    category: ProblemCategory::Transport,
                    severity: ProblemSeverity::Error,
                    retryable: true,
                    related_node: Some(related_node),
                    related_adapter: Some(related_adapter),
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
            RouteState::Failed => Err("failed MicYou Route cannot be stopped".to_owned()),
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
    permission: PermissionRequirement,
    clock_domain: &'a str,
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
        permission_requirement: spec.permission,
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
                clock_domain: Some(spec.clock_domain.to_owned()),
                availability: Availability::Available,
                permission_requirement: spec.permission,
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
    use std::collections::VecDeque;

    use capyio_testkit::DemoLab;

    use super::*;

    #[derive(Default)]
    struct FakeProcess {
        running: bool,
        presences: VecDeque<PeerTcpPresence>,
        exit_code: Option<Option<i32>>,
        start_error: Option<MicYouStartError>,
        starts: u32,
        stops: u32,
    }

    impl FakeProcess {
        fn with_presences(presences: impl IntoIterator<Item = PeerTcpPresence>) -> Self {
            Self {
                presences: presences.into_iter().collect(),
                ..Self::default()
            }
        }
    }

    impl MicYouProcessBoundary for FakeProcess {
        fn start(&mut self) -> Result<(), MicYouStartError> {
            if let Some(error) = self.start_error.clone() {
                return Err(error);
            }
            self.running = true;
            self.starts += 1;
            Ok(())
        }

        fn status(&mut self) -> Result<SupervisorStatus, String> {
            if let Some(exit_code) = self.exit_code {
                Ok(SupervisorStatus::Exited { exit_code })
            } else if self.running {
                Ok(SupervisorStatus::Running { process_id: 42 })
            } else {
                Ok(SupervisorStatus::Stopped)
            }
        }

        fn phone_presence(&mut self) -> Result<PeerTcpPresence, String> {
            Ok(self
                .presences
                .pop_front()
                .unwrap_or(PeerTcpPresence::Disconnected))
        }

        fn stop(&mut self) -> Result<(), String> {
            self.running = false;
            self.stops += 1;
            Ok(())
        }
    }

    fn established() -> PeerTcpPresence {
        PeerTcpPresence::Established {
            connection_count: 1,
        }
    }

    #[test]
    fn stable_phone_presence_activates_and_loss_offlines_only_microphone() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let imu_route = lab.routes.phone_imu_to_gamepad;
        lab.set_route_active(imu_route, true, 1)
            .expect("activate IMU Route");
        let process = FakeProcess::with_presences([
            PeerTcpPresence::Disconnected,
            established(),
            established(),
            established(),
            PeerTcpPresence::Disconnected,
        ]);
        let session_id = lab.session_id;
        let mut controller =
            MicYouRouteController::install(&mut lab.runtime, session_id, process, 3, 10)
                .expect("install microphone Route");
        controller.start(&mut lab.runtime, 10).expect("start");
        assert_eq!(
            controller
                .poll(&mut lab.runtime)
                .expect("waiting")
                .route_state,
            RouteState::Starting
        );
        for _ in 0..3 {
            controller.poll(&mut lab.runtime).expect("stable phone");
        }
        assert_eq!(
            controller.status(&lab.runtime).expect("active").route_state,
            RouteState::Active
        );
        assert_eq!(
            controller
                .poll(&mut lab.runtime)
                .expect("disconnect")
                .route_state,
            RouteState::Offline
        );
        assert_eq!(
            lab.runtime.route(imu_route).expect("IMU Route").state,
            RouteState::Active
        );
        assert_eq!(controller.process.stops, 1);
        assert!(lab.runtime.snapshot().problems.iter().any(|problem| {
            problem.code == "CAPY.MICYOU.PHONE_TCP_LOST"
                && problem.related_route == Some(controller.route_id())
        }));
    }

    #[test]
    fn explicit_retry_advances_epoch_and_stop_is_terminal() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let process = FakeProcess::with_presences([
            established(),
            established(),
            established(),
            PeerTcpPresence::Disconnected,
            established(),
            established(),
            established(),
        ]);
        let session_id = lab.session_id;
        let mut controller =
            MicYouRouteController::install(&mut lab.runtime, session_id, process, 3, 10)
                .expect("controller");
        let first_epoch = controller.start(&mut lab.runtime, 10).expect("start");
        for _ in 0..3 {
            controller.poll(&mut lab.runtime).expect("activate");
        }
        controller.poll(&mut lab.runtime).expect("offline");
        assert_eq!(controller.process.stops, 1);
        let second_epoch = controller.start(&mut lab.runtime, 20).expect("retry");
        assert!(second_epoch > first_epoch);
        assert_eq!(controller.process.starts, 2);
        for _ in 0..3 {
            controller.poll(&mut lab.runtime).expect("reactivate");
        }
        assert_eq!(
            controller.status(&lab.runtime).expect("active").route_state,
            RouteState::Active
        );
        controller.stop(&mut lab.runtime).expect("stop");
        assert_eq!(controller.process.stops, 2);
        assert_eq!(
            controller
                .status(&lab.runtime)
                .expect("stopped")
                .route_state,
            RouteState::Stopped
        );
    }

    #[test]
    fn bounded_phone_wait_stops_process_and_retains_problem() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let process = FakeProcess::with_presences([
            PeerTcpPresence::Disconnected,
            PeerTcpPresence::Disconnected,
            PeerTcpPresence::Disconnected,
        ]);
        let session_id = lab.session_id;
        let mut controller =
            MicYouRouteController::install(&mut lab.runtime, session_id, process, 2, 3)
                .expect("controller");
        controller.start(&mut lab.runtime, 10).expect("start");
        for _ in 0..3 {
            controller.poll(&mut lab.runtime).expect("bounded wait");
        }
        assert_eq!(
            controller
                .status(&lab.runtime)
                .expect("offline")
                .route_state,
            RouteState::Offline
        );
        assert_eq!(controller.process.stops, 1);
        assert!(
            lab.runtime
                .snapshot()
                .problems
                .iter()
                .any(|problem| problem.code == "CAPY.MICYOU.PHONE_WAIT_EXHAUSTED")
        );
    }

    #[test]
    fn endpoint_start_failure_is_typed_and_does_not_leak_identity() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let process = FakeProcess {
            start_error: Some(MicYouStartError::ConfiguredEndpointUnavailable),
            ..FakeProcess::default()
        };
        let session_id = lab.session_id;
        let mut controller =
            MicYouRouteController::install(&mut lab.runtime, session_id, process, 3, 10)
                .expect("controller");
        assert!(controller.start(&mut lab.runtime, 10).is_err());
        let problem = lab
            .runtime
            .snapshot()
            .problems
            .into_iter()
            .find(|problem| problem.code == "CAPY.MICYOU.ENDPOINT_UNAVAILABLE")
            .expect("typed endpoint Problem");
        assert_eq!(problem.related_route, Some(controller.route_id()));
        assert!(
            !problem
                .technical_detail
                .as_deref()
                .unwrap_or_default()
                .contains("{0.0.0")
        );
    }

    #[test]
    fn invalid_poll_bounds_are_rejected_before_catalog_mutation() {
        let mut lab = DemoLab::new().expect("demo Runtime");
        let routes = lab.runtime.snapshot().routes.len();
        let session_id = lab.session_id;
        assert!(
            MicYouRouteController::install(
                &mut lab.runtime,
                session_id,
                FakeProcess::default(),
                0,
                1
            )
            .is_err()
        );
        assert!(
            MicYouRouteController::install(
                &mut lab.runtime,
                session_id,
                FakeProcess::default(),
                3,
                2
            )
            .is_err()
        );
        assert_eq!(lab.runtime.snapshot().routes.len(), routes);
    }
}
