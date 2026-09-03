use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use capyio_core::{
    AdapterDeploymentMode, AdapterHealth, AdapterInstanceDescriptor, AdapterInstanceId,
    AdapterState, Availability, CapabilityClass, CapabilityDescriptor, FormatDescriptor,
    InteroperabilityMode, NodeId, PermissionRequirement, PortDescriptor, PortDirection, PortRef,
    Problem, ProblemCategory, ProblemId, ProblemSeverity, ProfileId, QosMode, RouteBackend,
    RouteId, RouteState, SessionId, StreamId,
};
use capyio_input::{GamepadControls, GamepadState, InputFrameHeader};
use capyio_runtime::NodeRuntime;
use capyio_viiper_adapter::{
    ViiperAutoAttachDisabled, ViiperLoopbackClient, ViiperSubmitOutcome, ViiperXbox360Mapping,
    ViiperXbox360Worker, ViiperXbox360WorkerState, Xbox360RumbleFeedback,
};

use crate::{UsbipBusId, UsbipOwnedAttachment, UsbipWin2Client, UsbipWin2DeploymentVerified};

const VIIPER_ADAPTER_ID: &str = "00000000-0000-4000-8000-00000000d011";
const VIIPER_CAPABILITY_ID: &str = "00000000-0000-4000-8000-00000000d101";
const VIIPER_PORT_ID: &str = "00000000-0000-4000-8000-00000000d111";
const GAMEPAD_FORMAT: &str = "gamepad-state-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViiperGamepadRouteStatus {
    pub route_state: RouteState,
    pub route_epoch: u64,
    pub worker_state: Option<ViiperXbox360WorkerState>,
    pub bus_id: Option<u32>,
    pub usbip_port: Option<u8>,
}

/// Owns the one-to-one relationship between a Runtime gamepad Route and a
/// bounded VIIPER Xbox 360 session.
///
/// The controller does not start or configure the external server. Its caller
/// must retain independent evidence for the auto-attach-disabled assertion.
/// Explicit [`Self::stop`] is required for neutral delivery and owned-bus
/// cleanup; dropping this value only inherits the Worker's socket shutdown.
pub struct ViiperGamepadRouteController {
    client: ViiperLoopbackClient,
    auto_attach_disabled: ViiperAutoAttachDisabled,
    mapping: ViiperXbox360Mapping,
    route: ViiperGamepadRoute,
    worker: Option<ViiperXbox360Worker>,
    usbip_client: Option<UsbipWin2Client>,
    usbip_deployment_verified: Option<UsbipWin2DeploymentVerified>,
    usbip_attachment: Option<UsbipOwnedAttachment>,
}

impl ViiperGamepadRouteController {
    pub fn install(
        runtime: &mut NodeRuntime,
        session_id: SessionId,
        route_id: RouteId,
        source: PortRef,
        client: ViiperLoopbackClient,
        auto_attach_disabled: ViiperAutoAttachDisabled,
        mapping: ViiperXbox360Mapping,
    ) -> Result<Self, String> {
        Self::install_inner(
            runtime,
            session_id,
            route_id,
            source,
            client,
            auto_attach_disabled,
            mapping,
            None,
            None,
        )
    }

    /// Installs the Runtime Route with a constrained usbip-win2 attachment
    /// owner. Start reaches Active only after the exact VIIPER export is listed
    /// as Xbox 360 and a one-shot Windows attachment returns an owned hub port.
    #[allow(clippy::too_many_arguments)]
    pub fn install_with_usbip(
        runtime: &mut NodeRuntime,
        session_id: SessionId,
        route_id: RouteId,
        source: PortRef,
        client: ViiperLoopbackClient,
        auto_attach_disabled: ViiperAutoAttachDisabled,
        mapping: ViiperXbox360Mapping,
        usbip_client: UsbipWin2Client,
        usbip_deployment_verified: UsbipWin2DeploymentVerified,
    ) -> Result<Self, String> {
        Self::install_inner(
            runtime,
            session_id,
            route_id,
            source,
            client,
            auto_attach_disabled,
            mapping,
            Some(usbip_client),
            Some(usbip_deployment_verified),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn install_inner(
        runtime: &mut NodeRuntime,
        session_id: SessionId,
        route_id: RouteId,
        source: PortRef,
        client: ViiperLoopbackClient,
        auto_attach_disabled: ViiperAutoAttachDisabled,
        mapping: ViiperXbox360Mapping,
        usbip_client: Option<UsbipWin2Client>,
        usbip_deployment_verified: Option<UsbipWin2DeploymentVerified>,
    ) -> Result<Self, String> {
        let route = ViiperGamepadRoute::install(runtime, session_id, route_id, source)?;
        Ok(Self {
            client,
            auto_attach_disabled,
            mapping,
            route,
            worker: None,
            usbip_client,
            usbip_deployment_verified,
            usbip_attachment: None,
        })
    }

    /// Starts a fresh fixed-epoch Worker and activates the Route only after
    /// VIIPER provisioning, stream handshake and initial neutral all succeed.
    pub fn start(
        &mut self,
        runtime: &mut NodeRuntime,
        now_ms: u64,
        stream_id: StreamId,
        first_sequence: u64,
        source_timestamp_nanos: u64,
    ) -> Result<u64, String> {
        if self.worker.is_some() || self.usbip_attachment.is_some() {
            return Err("VIIPER gamepad Projection still owns a Worker or USB/IP port".to_owned());
        }
        let epoch = self.route.begin_start(runtime, now_ms)?;
        let anchor = GamepadState {
            header: InputFrameHeader {
                stream_id,
                stream_epoch: epoch,
                sequence: first_sequence,
                source_timestamp_nanos,
            },
            controls: GamepadControls::neutral(),
        };

        match self
            .client
            .open_xbox360(self.auto_attach_disabled, anchor, self.mapping)
        {
            Ok(worker) => self.worker = Some(worker),
            Err(error) => {
                let detail = error.to_string();
                let lifecycle = self.route.report_offline(
                    runtime,
                    GamepadProblemOwner::Projection,
                    "CAPY.GAMEPAD.VIIPER_OPEN_FAILED",
                    "The VIIPER gamepad Projection could not start",
                    detail.clone(),
                );
                return Err(combine_errors(
                    format!("VIIPER gamepad open failed: {detail}"),
                    None,
                    lifecycle.err(),
                ));
            }
        }

        if let Err(error) = self.attach_usbip_if_configured() {
            let detail = error.clone();
            let cleanup = self.cleanup_projection();
            let lifecycle = self.route.report_offline(
                runtime,
                GamepadProblemOwner::Projection,
                "CAPY.GAMEPAD.USBIP_ATTACH_FAILED",
                "The Windows Xbox 360 USB/IP attachment could not start",
                append_cleanup(&detail, cleanup.as_deref()),
            );
            return Err(combine_errors(error, cleanup, lifecycle.err()));
        }

        if let Err(error) = self.route.activate(runtime) {
            let cleanup = self.cleanup_projection();
            let lifecycle = self.route.report_offline(
                runtime,
                GamepadProblemOwner::Projection,
                "CAPY.GAMEPAD.ROUTE_ACTIVATION_FAILED",
                "The VIIPER gamepad Route could not activate",
                error.clone(),
            );
            return Err(combine_errors(error, cleanup, lifecycle.err()));
        }
        Ok(epoch)
    }

    /// Projects one complete state. Contract/codec rejection is non-terminal
    /// and does not consume Worker sequence. A terminal stream error cleans up
    /// the owned bus before the Runtime Route becomes Offline.
    pub fn submit(
        &mut self,
        runtime: &mut NodeRuntime,
        state: GamepadState,
    ) -> Result<ViiperSubmitOutcome, String> {
        if self.route.state(runtime)? != RouteState::Active {
            return Err("VIIPER gamepad Route is not Active".to_owned());
        }
        let worker = self
            .worker
            .as_mut()
            .ok_or_else(|| "Active VIIPER gamepad Route has no Worker".to_owned())?;
        match worker.submit(state) {
            Ok(outcome) if outcome.exhausted() => {
                let cleanup = self.cleanup_projection();
                let lifecycle = self.route.report_offline(
                    runtime,
                    GamepadProblemOwner::Projection,
                    "CAPY.GAMEPAD.SEQUENCE_EXHAUSTED",
                    "The gamepad stream sequence was exhausted",
                    cleanup
                        .as_deref()
                        .unwrap_or("the fixed-epoch sequence reached its maximum")
                        .to_owned(),
                );
                if cleanup.is_some() || lifecycle.is_err() {
                    return Err(combine_errors(
                        "gamepad stream sequence exhausted".to_owned(),
                        cleanup,
                        lifecycle.err(),
                    ));
                }
                Ok(outcome)
            }
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                let detail = error.to_string();
                if self
                    .worker
                    .as_ref()
                    .is_some_and(|worker| worker.state() == ViiperXbox360WorkerState::Failed)
                {
                    let cleanup = self.cleanup_projection();
                    let lifecycle = self.route.report_offline(
                        runtime,
                        GamepadProblemOwner::Projection,
                        "CAPY.GAMEPAD.VIIPER_STREAM_FAILED",
                        "The VIIPER gamepad stream failed",
                        append_cleanup(&detail, cleanup.as_deref()),
                    );
                    Err(combine_errors(
                        format!("VIIPER gamepad submit failed: {detail}"),
                        cleanup,
                        lifecycle.err(),
                    ))
                } else {
                    Err(format!("VIIPER gamepad state rejected: {detail}"))
                }
            }
        }
    }

    /// Polls at most one raw VIIPER feedback frame. This does not create or
    /// imply a CapyIO haptics Route. A terminal peer/stream failure is cleaned
    /// up before the gamepad Route becomes Offline.
    pub fn poll_rumble(
        &mut self,
        runtime: &mut NodeRuntime,
    ) -> Result<Option<Xbox360RumbleFeedback>, String> {
        if self.route.state(runtime)? != RouteState::Active {
            return Err("VIIPER gamepad Route is not Active".to_owned());
        }
        let worker = self
            .worker
            .as_mut()
            .ok_or_else(|| "Active VIIPER gamepad Route has no Worker".to_owned())?;
        match worker.poll_rumble() {
            Ok(feedback) => Ok(feedback),
            Err(error) => {
                let detail = error.to_string();
                if self
                    .worker
                    .as_ref()
                    .is_some_and(|worker| worker.state() == ViiperXbox360WorkerState::Failed)
                {
                    let cleanup = self.cleanup_projection();
                    let lifecycle = self.route.report_offline(
                        runtime,
                        GamepadProblemOwner::Projection,
                        "CAPY.GAMEPAD.VIIPER_STREAM_FAILED",
                        "The VIIPER gamepad stream failed",
                        append_cleanup(&detail, cleanup.as_deref()),
                    );
                    Err(combine_errors(
                        format!("VIIPER gamepad feedback failed: {detail}"),
                        cleanup,
                        lifecycle.err(),
                    ))
                } else {
                    Err(format!("VIIPER gamepad feedback rejected: {detail}"))
                }
            }
        }
    }

    /// Applies the upstream disconnect fail-safe: neutral and owned-resource
    /// cleanup complete before the Route is reported Offline.
    pub fn report_upstream_offline(
        &mut self,
        runtime: &mut NodeRuntime,
        detail: impl Into<String>,
    ) -> Result<(), String> {
        if !matches!(
            self.route.state(runtime)?,
            RouteState::Starting | RouteState::Active
        ) {
            return Err(
                "VIIPER gamepad upstream cannot go Offline from this Route state".to_owned(),
            );
        }
        let cleanup = self.cleanup_projection();
        let detail = append_cleanup(&detail.into(), cleanup.as_deref());
        let lifecycle = self.route.report_offline(
            runtime,
            GamepadProblemOwner::Source,
            "CAPY.GAMEPAD.UPSTREAM_OFFLINE",
            "The gamepad state source disconnected",
            detail,
        );
        if cleanup.is_some() || lifecycle.is_err() {
            return Err(combine_errors(
                "gamepad upstream disconnected".to_owned(),
                cleanup,
                lifecycle.err(),
            ));
        }
        Ok(())
    }

    /// Stops the Worker before completing the Runtime Route stop transition.
    /// Both cleanup and lifecycle errors are retained.
    pub fn stop(&mut self, runtime: &mut NodeRuntime) -> Result<(), String> {
        let cleanup = self.cleanup_projection();
        let lifecycle = self.route.stop(runtime);
        if cleanup.is_some() || lifecycle.is_err() {
            return Err(combine_errors(
                "VIIPER gamepad stop failed".to_owned(),
                cleanup,
                lifecycle.err(),
            ));
        }
        Ok(())
    }

    pub fn status(&self, runtime: &NodeRuntime) -> Result<ViiperGamepadRouteStatus, String> {
        Ok(ViiperGamepadRouteStatus {
            route_state: self.route.state(runtime)?,
            route_epoch: self.route.epoch(runtime)?,
            worker_state: self.worker.as_ref().map(ViiperXbox360Worker::state),
            bus_id: self.worker.as_ref().map(ViiperXbox360Worker::bus_id),
            usbip_port: self
                .usbip_attachment
                .as_ref()
                .map(UsbipOwnedAttachment::port),
        })
    }

    #[must_use]
    pub const fn route_id(&self) -> RouteId {
        self.route.route_id
    }

    fn attach_usbip_if_configured(&mut self) -> Result<(), String> {
        let Some(client) = self.usbip_client.as_ref() else {
            return Ok(());
        };
        let deployment_verified = self.usbip_deployment_verified.ok_or_else(|| {
            "USB/IP attachment requires an explicit verified-deployment assertion".to_owned()
        })?;
        let worker = self
            .worker
            .as_ref()
            .ok_or_else(|| "cannot attach USB/IP without an owned VIIPER Worker".to_owned())?;
        let bus_id = UsbipBusId::from_viiper(worker.bus_id(), worker.device_id())
            .map_err(|error| error.to_string())?;
        let attachment = client
            .attach_xbox360_once(deployment_verified, bus_id)
            .map_err(|error| format!("Windows USB/IP attach failed: {error}"))?;
        self.usbip_attachment = Some(attachment);
        Ok(())
    }

    fn cleanup_projection(&mut self) -> Option<String> {
        let mut errors = Vec::new();
        if self.usbip_attachment.is_some() {
            if let Some(worker) = self.worker.as_mut()
                && matches!(
                    worker.state(),
                    ViiperXbox360WorkerState::Running | ViiperXbox360WorkerState::Exhausted
                )
                && let Err(error) = worker.request_neutral()
            {
                errors.push(format!("pre-detach neutral failed: {error}"));
            }
            let detach = self
                .usbip_attachment
                .as_mut()
                .expect("attachment presence checked")
                .stop();
            match detach {
                Ok(()) => self.usbip_attachment = None,
                Err(error) => errors.push(format!("USB/IP detach failed: {error}")),
            }
        }
        if let Some(mut worker) = self.worker.take()
            && let Err(error) = worker.stop()
        {
            errors.push(format!("VIIPER Worker cleanup failed: {error}"));
        }
        (!errors.is_empty()).then(|| errors.join("; "))
    }
}

#[derive(Clone, Copy)]
struct ViiperGamepadRoute {
    route_id: RouteId,
    source_node_id: NodeId,
    source_adapter_id: AdapterInstanceId,
    sink_node_id: NodeId,
    sink_adapter_id: AdapterInstanceId,
}

#[derive(Clone, Copy)]
enum GamepadProblemOwner {
    Source,
    Projection,
}

impl ViiperGamepadRoute {
    fn install(
        runtime: &mut NodeRuntime,
        session_id: SessionId,
        route_id: RouteId,
        source: PortRef,
    ) -> Result<Self, String> {
        let source_adapter_id = adapter_id_for_port(runtime, source)?;
        let sink_node_id = runtime.snapshot().local_node.id;
        let sink_adapter_id = parse_id(VIIPER_ADAPTER_ID)?;
        let capability_id = parse_id(VIIPER_CAPABILITY_ID)?;
        let port_id = parse_id(VIIPER_PORT_ID)?;
        runtime
            .register_adapter_catalog(
                sink_node_id,
                AdapterInstanceDescriptor {
                    id: sink_adapter_id,
                    adapter_type: "capyio.viiper.xbox360".to_owned(),
                    display_name: "VIIPER Xbox 360 Projection".to_owned(),
                    deployment_mode: AdapterDeploymentMode::ExternalService,
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                    state: AdapterState::Ready,
                    health: AdapterHealth::Healthy,
                    owned_capabilities: BTreeSet::new(),
                    supported_route_modes: BTreeSet::from([RouteBackend::ExternalProtocol]),
                },
                vec![CapabilityDescriptor {
                    id: capability_id,
                    adapter_instance_id: sink_adapter_id,
                    display_name: "VIIPER Xbox 360 Gamepad".to_owned(),
                    class: CapabilityClass::Gamepad,
                    availability: Availability::Available,
                    permission_requirement: PermissionRequirement::None,
                    metadata: BTreeMap::new(),
                    ports: BTreeMap::from([(
                        port_id,
                        PortDescriptor {
                            id: port_id,
                            capability_id,
                            display_name: "VIIPER Gamepad State Sink".to_owned(),
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
                }],
            )
            .map_err(string_error)?;
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
                        Some(FormatDescriptor::new(GAMEPAD_FORMAT)),
                        QosMode::Interactive,
                        now_ms,
                    )
                    .map_err(string_error)?;
            }
            RouteState::Stopped => runtime
                .prepare_route(
                    self.route_id,
                    Some(FormatDescriptor::new(GAMEPAD_FORMAT)),
                    QosMode::Interactive,
                    now_ms,
                )
                .map_err(string_error)?,
            RouteState::Offline => runtime
                .recover_route(self.route_id, now_ms)
                .map_err(string_error)?,
            RouteState::Prepared => {}
            state => return Err(format!("VIIPER gamepad Route cannot start from {state:?}")),
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
        owner: GamepadProblemOwner,
        code: &str,
        message: &str,
        detail: String,
    ) -> Result<(), String> {
        let (category, related_node, related_adapter) = match owner {
            GamepadProblemOwner::Source => (
                ProblemCategory::Transport,
                self.source_node_id,
                self.source_adapter_id,
            ),
            GamepadProblemOwner::Projection => (
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
            RouteState::Failed => Err("failed VIIPER gamepad Route cannot be stopped".to_owned()),
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
            .ok_or_else(|| format!("unknown gamepad source Node {}", reference.node_id))?
    };
    let capability = node
        .capabilities
        .get(&reference.capability_id)
        .ok_or_else(|| {
            format!(
                "unknown gamepad source Capability {}",
                reference.capability_id
            )
        })?;
    if !capability.ports.contains_key(&reference.port_id) {
        return Err(format!("unknown gamepad source Port {}", reference.port_id));
    }
    Ok(capability.adapter_instance_id)
}

fn append_cleanup(detail: &str, cleanup: Option<&str>) -> String {
    cleanup.map_or_else(
        || detail.to_owned(),
        |cleanup| format!("{detail}; VIIPER cleanup reported: {cleanup}"),
    )
}

fn combine_errors(primary: String, cleanup: Option<String>, lifecycle: Option<String>) -> String {
    let mut combined = primary;
    if let Some(cleanup) = cleanup {
        combined.push_str("; cleanup failed: ");
        combined.push_str(&cleanup);
    }
    if let Some(lifecycle) = lifecycle {
        combined.push_str("; Runtime lifecycle update failed: ");
        combined.push_str(&lifecycle);
    }
    combined
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
