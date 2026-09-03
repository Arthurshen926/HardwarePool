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
use capyio_data_plane::{DataEnvelope, ImuSampleV1};
use capyio_input::GamepadState;
use capyio_runtime::NodeRuntime;
use capyio_viiper_adapter::{
    ViiperAutoAttachDisabled, ViiperDs4ControlsMapping, ViiperDs4Feedback, ViiperDs4MotionMapping,
    ViiperDs4Worker, ViiperDs4WorkerState, ViiperLoopbackClient, ViiperSubmitOutcome,
};

use crate::{UsbipBusId, UsbipOwnedAttachment, UsbipWin2Client, UsbipWin2DeploymentVerified};

const ADAPTER_ID: &str = "00000000-0000-4000-8000-00000000d013";
const CONTROLS_CAPABILITY_ID: &str = "00000000-0000-4000-8000-00000000d103";
const CONTROLS_PORT_ID: &str = "00000000-0000-4000-8000-00000000d113";
const MOTION_CAPABILITY_ID: &str = "00000000-0000-4000-8000-00000000d104";
const MOTION_PORT_ID: &str = "00000000-0000-4000-8000-00000000d114";
const GAMEPAD_FORMAT: &str = "gamepad-state-v1";
const IMU_FORMAT: &str = "imu-si-f32-le";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViiperDs4RouteEpochs {
    pub controls: u64,
    pub motion: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViiperDs4RouteStatus {
    pub controls_state: RouteState,
    pub motion_state: RouteState,
    pub epochs: ViiperDs4RouteEpochs,
    pub worker_state: Option<ViiperDs4WorkerState>,
    pub bus_id: Option<u32>,
    pub usbip_port: Option<u8>,
}

/// Owns the two typed Runtime Routes that jointly feed one virtual DS4.
///
/// Controls and IMU remain independent StandardPort Routes. The external DS4
/// projection is their shared, fail-closed resource: it becomes visible only
/// after both stream anchors are valid, VIIPER is ready and an optional USB/IP
/// attachment succeeds. A failure neutralizes the device and offlines both
/// owned Routes without changing unrelated Routes.
pub struct ViiperDs4RouteController {
    client: ViiperLoopbackClient,
    auto_attach_disabled: ViiperAutoAttachDisabled,
    controls_mapping: ViiperDs4ControlsMapping,
    motion_mapping: ViiperDs4MotionMapping,
    controls_route: Ds4Route,
    motion_route: Ds4Route,
    worker: Option<ViiperDs4Worker>,
    usbip_client: Option<UsbipWin2Client>,
    usbip_deployment_verified: Option<UsbipWin2DeploymentVerified>,
    usbip_attachment: Option<UsbipOwnedAttachment>,
}

impl ViiperDs4RouteController {
    #[allow(clippy::too_many_arguments)]
    pub fn install(
        runtime: &mut NodeRuntime,
        session_id: SessionId,
        controls_route_id: RouteId,
        controls_source: PortRef,
        motion_route_id: RouteId,
        motion_source: PortRef,
        client: ViiperLoopbackClient,
        auto_attach_disabled: ViiperAutoAttachDisabled,
        controls_mapping: ViiperDs4ControlsMapping,
        motion_mapping: ViiperDs4MotionMapping,
    ) -> Result<Self, String> {
        Self::install_inner(
            runtime,
            session_id,
            controls_route_id,
            controls_source,
            motion_route_id,
            motion_source,
            client,
            auto_attach_disabled,
            controls_mapping,
            motion_mapping,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn install_with_usbip(
        runtime: &mut NodeRuntime,
        session_id: SessionId,
        controls_route_id: RouteId,
        controls_source: PortRef,
        motion_route_id: RouteId,
        motion_source: PortRef,
        client: ViiperLoopbackClient,
        auto_attach_disabled: ViiperAutoAttachDisabled,
        controls_mapping: ViiperDs4ControlsMapping,
        motion_mapping: ViiperDs4MotionMapping,
        usbip_client: UsbipWin2Client,
        usbip_deployment_verified: UsbipWin2DeploymentVerified,
    ) -> Result<Self, String> {
        Self::install_inner(
            runtime,
            session_id,
            controls_route_id,
            controls_source,
            motion_route_id,
            motion_source,
            client,
            auto_attach_disabled,
            controls_mapping,
            motion_mapping,
            Some(usbip_client),
            Some(usbip_deployment_verified),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn install_inner(
        runtime: &mut NodeRuntime,
        session_id: SessionId,
        controls_route_id: RouteId,
        controls_source: PortRef,
        motion_route_id: RouteId,
        motion_source: PortRef,
        client: ViiperLoopbackClient,
        auto_attach_disabled: ViiperAutoAttachDisabled,
        controls_mapping: ViiperDs4ControlsMapping,
        motion_mapping: ViiperDs4MotionMapping,
        usbip_client: Option<UsbipWin2Client>,
        usbip_deployment_verified: Option<UsbipWin2DeploymentVerified>,
    ) -> Result<Self, String> {
        if controls_route_id == motion_route_id {
            return Err("DS4 controls and motion require distinct Runtime Route IDs".to_owned());
        }
        let sink_node_id = runtime.snapshot().local_node.id;
        register_ds4_catalog(runtime, sink_node_id)?;
        let sink_adapter_id = parse_id(ADAPTER_ID)?;
        let controls_route = Ds4Route::install(
            runtime,
            session_id,
            controls_route_id,
            controls_source,
            sink_node_id,
            sink_adapter_id,
            parse_id(CONTROLS_CAPABILITY_ID)?,
            parse_id(CONTROLS_PORT_ID)?,
            GAMEPAD_FORMAT,
            QosMode::Interactive,
        )?;
        let motion_route = Ds4Route::install(
            runtime,
            session_id,
            motion_route_id,
            motion_source,
            sink_node_id,
            sink_adapter_id,
            parse_id(MOTION_CAPABILITY_ID)?,
            parse_id(MOTION_PORT_ID)?,
            IMU_FORMAT,
            QosMode::Measurement,
        )?;
        Ok(Self {
            client,
            auto_attach_disabled,
            controls_mapping,
            motion_mapping,
            controls_route,
            motion_route,
            worker: None,
            usbip_client,
            usbip_deployment_verified,
            usbip_attachment: None,
        })
    }

    pub fn begin_start(
        &mut self,
        runtime: &mut NodeRuntime,
        now_ms: u64,
    ) -> Result<ViiperDs4RouteEpochs, String> {
        if self.worker.is_some() || self.usbip_attachment.is_some() {
            return Err("DS4 Projection still owns a Worker or USB/IP port".to_owned());
        }
        let controls = self.controls_route.begin_start(runtime, now_ms)?;
        match self.motion_route.begin_start(runtime, now_ms) {
            Ok(motion) => Ok(ViiperDs4RouteEpochs { controls, motion }),
            Err(error) => {
                let rollback = self.controls_route.report_offline(
                    runtime,
                    ProblemOwner::Projection,
                    "CAPY.GAMEPAD.DS4_START_ROLLBACK",
                    "The paired DS4 Route could not start",
                    error.clone(),
                );
                Err(combine_errors(error, None, rollback.err()))
            }
        }
    }

    pub fn activate(
        &mut self,
        runtime: &mut NodeRuntime,
        controls_anchor: GamepadState,
        motion_anchor: &DataEnvelope<ImuSampleV1>,
    ) -> Result<(), String> {
        if self.worker.is_some() || self.usbip_attachment.is_some() {
            return Err("DS4 Projection still owns a Worker or USB/IP port".to_owned());
        }
        if let Err(detail) = self.require_starting_anchors(runtime, &controls_anchor, motion_anchor)
        {
            let lifecycle = self.offline_both(
                runtime,
                ProblemOwner::Source,
                "CAPY.GAMEPAD.DS4_STREAM_ANCHOR_INVALID",
                "A DualShock 4 source anchor does not match its Route",
                detail.clone(),
            );
            return Err(combine_errors(detail, None, lifecycle.err()));
        }
        match self.client.open_dualshock4(
            self.auto_attach_disabled,
            controls_anchor,
            motion_anchor,
            self.controls_mapping,
            self.motion_mapping,
        ) {
            Ok(worker) => self.worker = Some(worker),
            Err(error) => {
                let detail = error.to_string();
                self.offline_both(
                    runtime,
                    ProblemOwner::Projection,
                    "CAPY.GAMEPAD.DS4_OPEN_FAILED",
                    "The VIIPER DualShock 4 Projection could not start",
                    detail.clone(),
                )?;
                return Err(format!("VIIPER DualShock 4 open failed: {detail}"));
            }
        }
        if let Err(error) = self.attach_usbip_if_configured() {
            return self.fail_projection(
                runtime,
                "CAPY.GAMEPAD.DS4_USBIP_ATTACH_FAILED",
                "The Windows DualShock 4 USB/IP attachment could not start",
                error,
            );
        }
        if let Err(error) = self.controls_route.activate(runtime) {
            return self.fail_projection(
                runtime,
                "CAPY.GAMEPAD.DS4_ROUTE_ACTIVATION_FAILED",
                "The DS4 controls Route could not activate",
                error,
            );
        }
        if let Err(error) = self.motion_route.activate(runtime) {
            return self.fail_projection(
                runtime,
                "CAPY.GAMEPAD.DS4_ROUTE_ACTIVATION_FAILED",
                "The DS4 motion Route could not activate",
                error,
            );
        }
        Ok(())
    }

    pub fn submit(
        &mut self,
        runtime: &mut NodeRuntime,
        controls: GamepadState,
        motion: &DataEnvelope<ImuSampleV1>,
    ) -> Result<ViiperSubmitOutcome, String> {
        self.require_active(runtime)?;
        let result = self
            .worker
            .as_mut()
            .ok_or_else(|| "Active DS4 Routes have no Worker".to_owned())?
            .submit(controls, motion);
        match result {
            Ok(outcome) if outcome.exhausted() => {
                let cleanup = self.cleanup_projection();
                self.offline_both(
                    runtime,
                    ProblemOwner::Projection,
                    "CAPY.GAMEPAD.DS4_SEQUENCE_EXHAUSTED",
                    "A DualShock 4 source stream sequence was exhausted",
                    append_cleanup(
                        "a fixed-epoch sequence reached its maximum",
                        cleanup.as_deref(),
                    ),
                )?;
                Ok(outcome)
            }
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                let detail = error.to_string();
                if self
                    .worker
                    .as_ref()
                    .is_some_and(|worker| worker.state() == ViiperDs4WorkerState::Failed)
                {
                    self.fail_projection(
                        runtime,
                        "CAPY.GAMEPAD.DS4_STREAM_FAILED",
                        "The VIIPER DualShock 4 stream failed",
                        detail,
                    )
                } else {
                    Err(format!("DualShock 4 state rejected: {detail}"))
                }
            }
        }
    }

    pub fn poll_feedback(
        &mut self,
        runtime: &mut NodeRuntime,
    ) -> Result<Option<ViiperDs4Feedback>, String> {
        self.require_active(runtime)?;
        let result = self
            .worker
            .as_mut()
            .ok_or_else(|| "Active DS4 Routes have no Worker".to_owned())?
            .poll_feedback();
        match result {
            Ok(feedback) => Ok(feedback),
            Err(error) => self.fail_projection(
                runtime,
                "CAPY.GAMEPAD.DS4_STREAM_FAILED",
                "The VIIPER DualShock 4 feedback stream failed",
                error.to_string(),
            ),
        }
    }

    pub fn report_controls_offline(
        &mut self,
        runtime: &mut NodeRuntime,
        detail: impl Into<String>,
    ) -> Result<(), String> {
        self.report_source_offline(runtime, true, detail.into())
    }

    pub fn report_motion_offline(
        &mut self,
        runtime: &mut NodeRuntime,
        detail: impl Into<String>,
    ) -> Result<(), String> {
        self.report_source_offline(runtime, false, detail.into())
    }

    pub fn stop(&mut self, runtime: &mut NodeRuntime) -> Result<(), String> {
        let cleanup = self.cleanup_projection();
        let controls = self.controls_route.stop(runtime).err();
        let motion = self.motion_route.stop(runtime).err();
        combine_many("DualShock 4 stop failed", cleanup, controls, motion)
    }

    pub fn status(&self, runtime: &NodeRuntime) -> Result<ViiperDs4RouteStatus, String> {
        Ok(ViiperDs4RouteStatus {
            controls_state: self.controls_route.state(runtime)?,
            motion_state: self.motion_route.state(runtime)?,
            epochs: ViiperDs4RouteEpochs {
                controls: self.controls_route.epoch(runtime)?,
                motion: self.motion_route.epoch(runtime)?,
            },
            worker_state: self.worker.as_ref().map(ViiperDs4Worker::state),
            bus_id: self.worker.as_ref().map(ViiperDs4Worker::bus_id),
            usbip_port: self
                .usbip_attachment
                .as_ref()
                .map(UsbipOwnedAttachment::port),
        })
    }

    pub const fn controls_route_id(&self) -> RouteId {
        self.controls_route.route_id
    }
    pub const fn motion_route_id(&self) -> RouteId {
        self.motion_route.route_id
    }

    fn require_starting_anchors(
        &self,
        runtime: &NodeRuntime,
        controls: &GamepadState,
        motion: &DataEnvelope<ImuSampleV1>,
    ) -> Result<(), String> {
        if self.controls_route.state(runtime)? != RouteState::Starting
            || self.motion_route.state(runtime)? != RouteState::Starting
        {
            return Err("both DS4 Routes must be Starting before activation".to_owned());
        }
        let expected = self.status(runtime)?.epochs;
        if controls.header.stream_epoch != expected.controls
            || motion.stream_epoch != expected.motion
        {
            return Err(format!(
                "DS4 anchor epochs controls={} motion={} do not match Runtime epochs controls={} motion={}",
                controls.header.stream_epoch,
                motion.stream_epoch,
                expected.controls,
                expected.motion
            ));
        }
        Ok(())
    }

    fn require_active(&self, runtime: &NodeRuntime) -> Result<(), String> {
        if self.controls_route.state(runtime)? == RouteState::Active
            && self.motion_route.state(runtime)? == RouteState::Active
        {
            Ok(())
        } else {
            Err("both DS4 Routes must be Active".to_owned())
        }
    }

    fn report_source_offline(
        &mut self,
        runtime: &mut NodeRuntime,
        controls_failed: bool,
        detail: String,
    ) -> Result<(), String> {
        self.require_live(runtime)?;
        let cleanup = self.cleanup_projection();
        let detail = append_cleanup(&detail, cleanup.as_deref());
        let (failed, dependent, label) = if controls_failed {
            (&self.controls_route, &self.motion_route, "controls")
        } else {
            (&self.motion_route, &self.controls_route, "motion")
        };
        let first = failed.report_offline(
            runtime,
            ProblemOwner::Source,
            "CAPY.GAMEPAD.DS4_UPSTREAM_OFFLINE",
            "A DualShock 4 source disconnected",
            detail.clone(),
        );
        let second = dependent.report_offline(
            runtime,
            ProblemOwner::Projection,
            "CAPY.GAMEPAD.DS4_PAIRED_SOURCE_OFFLINE",
            "The paired DualShock 4 source is unavailable",
            format!("the {label} source disconnected; {detail}"),
        );
        combine_many(
            "DualShock 4 source disconnect cleanup failed",
            cleanup,
            first.err(),
            second.err(),
        )
    }

    fn require_live(&self, runtime: &NodeRuntime) -> Result<(), String> {
        let live = |state| matches!(state, RouteState::Starting | RouteState::Active);
        if live(self.controls_route.state(runtime)?) && live(self.motion_route.state(runtime)?) {
            Ok(())
        } else {
            Err("both DS4 Routes must be Starting or Active".to_owned())
        }
    }

    fn fail_projection<T>(
        &mut self,
        runtime: &mut NodeRuntime,
        code: &str,
        message: &str,
        detail: String,
    ) -> Result<T, String> {
        let cleanup = self.cleanup_projection();
        let lifecycle = self.offline_both(
            runtime,
            ProblemOwner::Projection,
            code,
            message,
            append_cleanup(&detail, cleanup.as_deref()),
        );
        Err(combine_errors(detail, cleanup, lifecycle.err()))
    }

    fn offline_both(
        &self,
        runtime: &mut NodeRuntime,
        owner: ProblemOwner,
        code: &str,
        message: &str,
        detail: String,
    ) -> Result<(), String> {
        let controls =
            self.controls_route
                .report_offline(runtime, owner, code, message, detail.clone());
        let motion = self
            .motion_route
            .report_offline(runtime, owner, code, message, detail);
        combine_many(
            "paired DS4 Route offline update failed",
            None,
            controls.err(),
            motion.err(),
        )
    }

    fn attach_usbip_if_configured(&mut self) -> Result<(), String> {
        let Some(client) = self.usbip_client.as_ref() else {
            return Ok(());
        };
        let verified = self.usbip_deployment_verified.ok_or_else(|| {
            "USB/IP attachment requires an explicit verified-deployment assertion".to_owned()
        })?;
        let worker = self
            .worker
            .as_ref()
            .ok_or_else(|| "cannot attach USB/IP without an owned DualShock 4 Worker".to_owned())?;
        let bus_id = UsbipBusId::from_viiper(worker.bus_id(), worker.device_id())
            .map_err(|error| error.to_string())?;
        self.usbip_attachment = Some(
            client
                .attach_dualshock4_once(verified, bus_id)
                .map_err(|error| format!("Windows DualShock 4 USB/IP attach failed: {error}"))?,
        );
        Ok(())
    }

    fn cleanup_projection(&mut self) -> Option<String> {
        let mut errors = Vec::new();
        if self.usbip_attachment.is_some() {
            if let Some(worker) = self.worker.as_mut()
                && matches!(
                    worker.state(),
                    ViiperDs4WorkerState::Running | ViiperDs4WorkerState::Exhausted
                )
                && let Err(error) = worker.request_safe_state()
            {
                errors.push(format!("pre-detach safe state failed: {error}"));
            }
            match self
                .usbip_attachment
                .as_mut()
                .expect("presence checked")
                .stop()
            {
                Ok(()) => self.usbip_attachment = None,
                Err(error) => errors.push(format!("USB/IP detach failed: {error}")),
            }
        }
        if let Some(mut worker) = self.worker.take()
            && let Err(error) = worker.stop()
        {
            errors.push(format!("VIIPER DualShock 4 cleanup failed: {error}"));
        }
        (!errors.is_empty()).then(|| errors.join("; "))
    }
}

#[derive(Clone)]
struct Ds4Route {
    route_id: RouteId,
    source_node_id: NodeId,
    source_adapter_id: AdapterInstanceId,
    sink_node_id: NodeId,
    sink_adapter_id: AdapterInstanceId,
    format: &'static str,
    qos: QosMode,
}

#[derive(Clone, Copy)]
enum ProblemOwner {
    Source,
    Projection,
}

impl Ds4Route {
    #[allow(clippy::too_many_arguments)]
    fn install(
        runtime: &mut NodeRuntime,
        session_id: SessionId,
        route_id: RouteId,
        source: PortRef,
        sink_node_id: NodeId,
        sink_adapter_id: AdapterInstanceId,
        capability_id: capyio_core::CapabilityId,
        port_id: capyio_core::PortId,
        format: &'static str,
        qos: QosMode,
    ) -> Result<Self, String> {
        let source_adapter_id = adapter_id_for_port(runtime, source)?;
        runtime
            .create_route_with_id(
                route_id,
                session_id,
                source,
                PortRef {
                    node_id: sink_node_id,
                    capability_id,
                    port_id,
                },
                RouteBackend::ExternalProtocol,
            )
            .map_err(string_error)?;
        Ok(Self {
            route_id,
            source_node_id: source.node_id,
            source_adapter_id,
            sink_node_id,
            sink_adapter_id,
            format,
            qos,
        })
    }

    fn begin_start(&self, runtime: &mut NodeRuntime, now_ms: u64) -> Result<u64, String> {
        match self.state(runtime)? {
            RouteState::Draft => {
                runtime
                    .authorize_route(self.route_id, None)
                    .map_err(string_error)?;
                runtime
                    .prepare_route(
                        self.route_id,
                        Some(FormatDescriptor::new(self.format)),
                        self.qos.clone(),
                        now_ms,
                    )
                    .map_err(string_error)?;
            }
            RouteState::Stopped => runtime
                .prepare_route(
                    self.route_id,
                    Some(FormatDescriptor::new(self.format)),
                    self.qos.clone(),
                    now_ms,
                )
                .map_err(string_error)?,
            RouteState::Offline => runtime
                .recover_route(self.route_id, now_ms)
                .map_err(string_error)?,
            RouteState::Prepared => {}
            state => return Err(format!("DS4 Route cannot start from {state:?}")),
        }
        runtime
            .begin_route_start(self.route_id, now_ms)
            .map_err(string_error)?;
        self.epoch(runtime)
    }

    fn activate(&self, runtime: &mut NodeRuntime) -> Result<(), String> {
        runtime.activate_route(self.route_id).map_err(string_error)
    }

    fn report_offline(
        &self,
        runtime: &mut NodeRuntime,
        owner: ProblemOwner,
        code: &str,
        message: &str,
        detail: String,
    ) -> Result<(), String> {
        if !matches!(
            self.state(runtime)?,
            RouteState::Starting | RouteState::Active
        ) {
            return Ok(());
        }
        let (category, node, adapter) = match owner {
            ProblemOwner::Source => (
                ProblemCategory::Transport,
                self.source_node_id,
                self.source_adapter_id,
            ),
            ProblemOwner::Projection => (
                ProblemCategory::Adapter,
                self.sink_node_id,
                self.sink_adapter_id,
            ),
        };
        runtime
            .report_route_offline(
                self.route_id,
                Problem {
                    id: ProblemId::new(),
                    code: code.to_owned(),
                    category,
                    severity: ProblemSeverity::Error,
                    retryable: true,
                    related_node: Some(node),
                    related_adapter: Some(adapter),
                    related_route: Some(self.route_id),
                    human_message: message.to_owned(),
                    technical_detail: Some(detail.chars().take(1024).collect()),
                },
            )
            .map_err(string_error)
    }

    fn stop(&self, runtime: &mut NodeRuntime) -> Result<(), String> {
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
            RouteState::Failed => Err("failed DS4 Route cannot be stopped".to_owned()),
        }
    }

    fn state(&self, runtime: &NodeRuntime) -> Result<RouteState, String> {
        runtime
            .route(self.route_id)
            .map(|route| route.state)
            .map_err(string_error)
    }
    fn epoch(&self, runtime: &NodeRuntime) -> Result<u64, String> {
        runtime
            .route(self.route_id)
            .map(|route| route.epoch)
            .map_err(string_error)
    }
}

fn register_ds4_catalog(runtime: &mut NodeRuntime, node_id: NodeId) -> Result<(), String> {
    let adapter_id = parse_id(ADAPTER_ID)?;
    let controls_capability_id = parse_id(CONTROLS_CAPABILITY_ID)?;
    let controls_port_id = parse_id(CONTROLS_PORT_ID)?;
    let motion_capability_id = parse_id(MOTION_CAPABILITY_ID)?;
    let motion_port_id = parse_id(MOTION_PORT_ID)?;
    runtime
        .register_adapter_catalog(
            node_id,
            AdapterInstanceDescriptor {
                id: adapter_id,
                adapter_type: "capyio.viiper.dualshock4".to_owned(),
                display_name: "VIIPER DualShock 4 Projection".to_owned(),
                deployment_mode: AdapterDeploymentMode::ExternalService,
                version: env!("CARGO_PKG_VERSION").to_owned(),
                state: AdapterState::Ready,
                health: AdapterHealth::Healthy,
                owned_capabilities: BTreeSet::new(),
                supported_route_modes: BTreeSet::from([RouteBackend::ExternalProtocol]),
            },
            vec![
                CapabilityDescriptor {
                    id: controls_capability_id,
                    adapter_instance_id: adapter_id,
                    display_name: "Virtual DualShock 4 Controls".to_owned(),
                    class: CapabilityClass::Gamepad,
                    availability: Availability::Available,
                    permission_requirement: PermissionRequirement::None,
                    metadata: BTreeMap::new(),
                    ports: BTreeMap::from([(
                        controls_port_id,
                        PortDescriptor {
                            id: controls_port_id,
                            capability_id: controls_capability_id,
                            display_name: "DualShock 4 Gamepad State Sink".to_owned(),
                            direction: PortDirection::Sink,
                            profile: ProfileId::gamepad_state_v1(),
                            schema_id: None,
                            formats: vec![FormatDescriptor::new(GAMEPAD_FORMAT)],
                            qos_modes: BTreeSet::from([QosMode::Interactive]),
                            clock_domain: None,
                            availability: Availability::Available,
                            permission_requirement: PermissionRequirement::None,
                            interoperability_mode: InteroperabilityMode::StandardPort,
                        },
                    )]),
                },
                CapabilityDescriptor {
                    id: motion_capability_id,
                    adapter_instance_id: adapter_id,
                    display_name: "Virtual DualShock 4 Motion".to_owned(),
                    class: CapabilityClass::Imu,
                    availability: Availability::Available,
                    permission_requirement: PermissionRequirement::None,
                    metadata: BTreeMap::new(),
                    ports: BTreeMap::from([(
                        motion_port_id,
                        PortDescriptor {
                            id: motion_port_id,
                            capability_id: motion_capability_id,
                            display_name: "DualShock 4 IMU Sample Sink".to_owned(),
                            direction: PortDirection::Sink,
                            profile: ProfileId::imu_samples_v1(),
                            schema_id: None,
                            formats: vec![FormatDescriptor::new(IMU_FORMAT)],
                            qos_modes: BTreeSet::from([QosMode::Measurement]),
                            clock_domain: None,
                            availability: Availability::Available,
                            permission_requirement: PermissionRequirement::None,
                            interoperability_mode: InteroperabilityMode::StandardPort,
                        },
                    )]),
                },
            ],
        )
        .map_err(string_error)
}

fn adapter_id_for_port(
    runtime: &NodeRuntime,
    reference: PortRef,
) -> Result<AdapterInstanceId, String> {
    let snapshot = runtime.snapshot();
    let node = if snapshot.local_node.id == reference.node_id {
        &snapshot.local_node
    } else {
        snapshot
            .peers
            .iter()
            .find(|node| node.id == reference.node_id)
            .ok_or_else(|| format!("unknown DS4 source Node {}", reference.node_id))?
    };
    let capability = node
        .capabilities
        .get(&reference.capability_id)
        .ok_or_else(|| format!("unknown DS4 source Capability {}", reference.capability_id))?;
    if !capability.ports.contains_key(&reference.port_id) {
        return Err(format!("unknown DS4 source Port {}", reference.port_id));
    }
    Ok(capability.adapter_instance_id)
}

fn append_cleanup(detail: &str, cleanup: Option<&str>) -> String {
    cleanup.map_or_else(
        || detail.to_owned(),
        |cleanup| format!("{detail}; projection cleanup reported: {cleanup}"),
    )
}

fn combine_errors(primary: String, cleanup: Option<String>, lifecycle: Option<String>) -> String {
    let mut result = primary;
    if let Some(error) = cleanup {
        result.push_str("; cleanup failed: ");
        result.push_str(&error);
    }
    if let Some(error) = lifecycle {
        result.push_str("; Runtime lifecycle update failed: ");
        result.push_str(&error);
    }
    result
}

fn combine_many(
    label: &str,
    cleanup: Option<String>,
    first: Option<String>,
    second: Option<String>,
) -> Result<(), String> {
    if cleanup.is_none() && first.is_none() && second.is_none() {
        return Ok(());
    }
    let mut parts = Vec::new();
    if let Some(error) = cleanup {
        parts.push(format!("cleanup: {error}"));
    }
    if let Some(error) = first {
        parts.push(format!("controls/first Route: {error}"));
    }
    if let Some(error) = second {
        parts.push(format!("motion/second Route: {error}"));
    }
    Err(format!("{label}: {}", parts.join("; ")))
}

fn parse_id<T: FromStr>(value: &str) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    value.parse::<T>().map_err(string_error)
}
fn string_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
