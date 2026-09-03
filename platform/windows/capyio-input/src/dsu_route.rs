use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddrV4,
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
use capyio_dsu_adapter::{DsuImuWorker, DsuImuWorkerConfig, DsuImuWorkerStats, DsuSubmitOutcome};
use capyio_runtime::NodeRuntime;

const DSU_ADAPTER_ID: &str = "00000000-0000-4000-8000-00000000d012";
const DSU_CAPABILITY_ID: &str = "00000000-0000-4000-8000-00000000d102";
const DSU_PORT_ID: &str = "00000000-0000-4000-8000-00000000d112";
const IMU_FORMAT: &str = "imu-si-f32-le";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DsuImuRouteStatus {
    pub route_state: RouteState,
    pub route_epoch: u64,
    pub local_address: Option<SocketAddrV4>,
    pub worker_stats: Option<DsuImuWorkerStats>,
}

/// Owns one Runtime IMU Route and its bounded IPv4-loopback DSU Worker.
///
/// The controller does not connect to SensorServer or select an emulator.
/// Callers first obtain the Runtime epoch from [`Self::begin_start`], construct
/// their source stream for that epoch, then pass its validated anchor to
/// [`Self::activate`]. Explicit [`Self::stop`] is required to join the Worker
/// and release the UDP endpoint before completing the Runtime stop transition.
pub struct DsuImuRouteController {
    config: DsuImuWorkerConfig,
    route: DsuImuRoute,
    worker: Option<DsuImuWorker>,
}

impl DsuImuRouteController {
    pub fn install(
        runtime: &mut NodeRuntime,
        session_id: SessionId,
        route_id: RouteId,
        source: PortRef,
        config: DsuImuWorkerConfig,
    ) -> Result<Self, String> {
        let route = DsuImuRoute::install(runtime, session_id, route_id, source)?;
        Ok(Self {
            config,
            route,
            worker: None,
        })
    }

    /// Advances the Runtime Route into Starting and returns the new fixed
    /// stream epoch. No socket or Worker exists until [`Self::activate`].
    pub fn begin_start(&mut self, runtime: &mut NodeRuntime, now_ms: u64) -> Result<u64, String> {
        if self.worker.is_some() {
            return Err("DSU IMU Worker is already owned".to_owned());
        }
        self.route.begin_start(runtime, now_ms)
    }

    /// Binds a fresh loopback endpoint and activates only after the Worker has
    /// accepted a valid anchor for the Runtime-selected epoch.
    pub fn activate(
        &mut self,
        runtime: &mut NodeRuntime,
        stream_anchor: &DataEnvelope<ImuSampleV1>,
    ) -> Result<(), String> {
        if self.worker.is_some() {
            return Err("DSU IMU Worker is already owned".to_owned());
        }
        if self.route.state(runtime)? != RouteState::Starting {
            return Err("DSU IMU Route is not Starting".to_owned());
        }
        let epoch = self.route.epoch(runtime)?;
        if stream_anchor.stream_epoch != epoch {
            let detail = format!(
                "DSU stream anchor epoch {} does not match Runtime Route epoch {epoch}",
                stream_anchor.stream_epoch
            );
            let lifecycle = self.route.report_offline(
                runtime,
                DsuProblemOwner::Source,
                "CAPY.GAMEPAD.DSU_STREAM_ANCHOR_INVALID",
                "The DSU IMU stream anchor does not match the Route",
                detail.clone(),
            );
            return Err(combine_errors(detail, None, lifecycle.err()));
        }

        match DsuImuWorker::start(self.config, stream_anchor) {
            Ok(worker) => self.worker = Some(worker),
            Err(error) => {
                let detail = error.to_string();
                let lifecycle = self.route.report_offline(
                    runtime,
                    DsuProblemOwner::Projection,
                    "CAPY.GAMEPAD.DSU_OPEN_FAILED",
                    "The DSU IMU Projection could not start",
                    detail.clone(),
                );
                return Err(combine_errors(
                    format!("DSU IMU Worker start failed: {detail}"),
                    None,
                    lifecycle.err(),
                ));
            }
        }

        if let Err(error) = self.route.activate(runtime) {
            let cleanup = self.cleanup_worker();
            let lifecycle = self.route.report_offline(
                runtime,
                DsuProblemOwner::Projection,
                "CAPY.GAMEPAD.DSU_ROUTE_ACTIVATION_FAILED",
                "The DSU IMU Route could not activate",
                error.clone(),
            );
            return Err(combine_errors(error, cleanup, lifecycle.err()));
        }
        Ok(())
    }

    /// Enqueues one IMU envelope without blocking the producer. Queue pressure
    /// is observable but non-terminal. A stopped Worker is cleaned up before
    /// the Route is reported Offline.
    pub fn submit(
        &mut self,
        runtime: &mut NodeRuntime,
        envelope: DataEnvelope<ImuSampleV1>,
    ) -> Result<DsuSubmitOutcome, String> {
        if self.route.state(runtime)? != RouteState::Active {
            return Err("DSU IMU Route is not Active".to_owned());
        }
        let outcome = self
            .worker
            .as_ref()
            .ok_or_else(|| "Active DSU IMU Route has no Worker".to_owned())?
            .sender()
            .try_submit(envelope);
        if outcome != DsuSubmitOutcome::Stopped {
            return Ok(outcome);
        }
        let cleanup = self.cleanup_worker();
        let lifecycle = self.route.report_offline(
            runtime,
            DsuProblemOwner::Projection,
            "CAPY.GAMEPAD.DSU_WORKER_STOPPED",
            "The DSU IMU Projection stopped unexpectedly",
            append_cleanup(
                "the bounded Worker no longer accepts input",
                cleanup.as_deref(),
            ),
        );
        if cleanup.is_some() || lifecycle.is_err() {
            return Err(combine_errors(
                "DSU IMU Worker stopped unexpectedly".to_owned(),
                cleanup,
                lifecycle.err(),
            ));
        }
        Ok(outcome)
    }

    /// Maps an asynchronously observed Worker failure onto the Route. This is
    /// a bounded atomic snapshot plus, only on failure, a blocking thread join;
    /// host Adapter workers must call it outside UI/real-time callbacks.
    pub fn poll_health(&mut self, runtime: &mut NodeRuntime) -> Result<DsuImuWorkerStats, String> {
        if self.route.state(runtime)? != RouteState::Active {
            return Err("DSU IMU Route is not Active".to_owned());
        }
        let stats = self
            .worker
            .as_ref()
            .ok_or_else(|| "Active DSU IMU Route has no Worker".to_owned())?
            .stats();
        if !stats.stopped && stats.transport_failures == 0 {
            return Ok(stats);
        }
        let cleanup = self.cleanup_worker();
        let lifecycle = self.route.report_offline(
            runtime,
            DsuProblemOwner::Projection,
            "CAPY.GAMEPAD.DSU_TRANSPORT_FAILED",
            "The DSU IMU Projection transport failed",
            append_cleanup(
                &format!(
                    "DSU Worker stopped={} transport_failures={}",
                    stats.stopped, stats.transport_failures
                ),
                cleanup.as_deref(),
            ),
        );
        Err(combine_errors(
            "DSU IMU transport failed".to_owned(),
            cleanup,
            lifecycle.err(),
        ))
    }

    /// Stops and joins the projection before marking an upstream disconnect.
    pub fn report_upstream_offline(
        &mut self,
        runtime: &mut NodeRuntime,
        detail: impl Into<String>,
    ) -> Result<(), String> {
        if !matches!(
            self.route.state(runtime)?,
            RouteState::Starting | RouteState::Active
        ) {
            return Err("DSU IMU upstream cannot go Offline from this Route state".to_owned());
        }
        let cleanup = self.cleanup_worker();
        let lifecycle = self.route.report_offline(
            runtime,
            DsuProblemOwner::Source,
            "CAPY.GAMEPAD.DSU_UPSTREAM_OFFLINE",
            "The DSU IMU source disconnected",
            append_cleanup(&detail.into(), cleanup.as_deref()),
        );
        if cleanup.is_some() || lifecycle.is_err() {
            return Err(combine_errors(
                "DSU IMU upstream disconnected".to_owned(),
                cleanup,
                lifecycle.err(),
            ));
        }
        Ok(())
    }

    /// Joins the Worker and releases its loopback socket before Stopped.
    pub fn stop(&mut self, runtime: &mut NodeRuntime) -> Result<(), String> {
        let cleanup = self.cleanup_worker();
        let lifecycle = self.route.stop(runtime);
        if cleanup.is_some() || lifecycle.is_err() {
            return Err(combine_errors(
                "DSU IMU stop failed".to_owned(),
                cleanup,
                lifecycle.err(),
            ));
        }
        Ok(())
    }

    pub fn status(&self, runtime: &NodeRuntime) -> Result<DsuImuRouteStatus, String> {
        Ok(DsuImuRouteStatus {
            route_state: self.route.state(runtime)?,
            route_epoch: self.route.epoch(runtime)?,
            local_address: self.worker.as_ref().map(DsuImuWorker::local_address),
            worker_stats: self.worker.as_ref().map(DsuImuWorker::stats),
        })
    }

    #[must_use]
    pub const fn route_id(&self) -> RouteId {
        self.route.route_id
    }

    fn cleanup_worker(&mut self) -> Option<String> {
        self.worker
            .take()
            .and_then(|mut worker| worker.stop().err().map(|error| error.to_string()))
    }
}

#[derive(Clone, Copy)]
struct DsuImuRoute {
    route_id: RouteId,
    source_node_id: NodeId,
    source_adapter_id: AdapterInstanceId,
    sink_node_id: NodeId,
    sink_adapter_id: AdapterInstanceId,
}

#[derive(Clone, Copy)]
enum DsuProblemOwner {
    Source,
    Projection,
}

impl DsuImuRoute {
    fn install(
        runtime: &mut NodeRuntime,
        session_id: SessionId,
        route_id: RouteId,
        source: PortRef,
    ) -> Result<Self, String> {
        let source_adapter_id = adapter_id_for_port(runtime, source)?;
        let sink_node_id = runtime.snapshot().local_node.id;
        let sink_adapter_id = parse_id(DSU_ADAPTER_ID)?;
        let capability_id = parse_id(DSU_CAPABILITY_ID)?;
        let port_id = parse_id(DSU_PORT_ID)?;
        runtime
            .register_adapter_catalog(
                sink_node_id,
                AdapterInstanceDescriptor {
                    id: sink_adapter_id,
                    adapter_type: "capyio.dsu.imu".to_owned(),
                    display_name: "DSU IMU Projection".to_owned(),
                    deployment_mode: AdapterDeploymentMode::InProcess,
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                    state: AdapterState::Ready,
                    health: AdapterHealth::Healthy,
                    owned_capabilities: BTreeSet::new(),
                    supported_route_modes: BTreeSet::from([RouteBackend::ExternalProtocol]),
                },
                vec![CapabilityDescriptor {
                    id: capability_id,
                    adapter_instance_id: sink_adapter_id,
                    display_name: "DSU Motion Controller".to_owned(),
                    class: CapabilityClass::Gamepad,
                    availability: Availability::Available,
                    permission_requirement: PermissionRequirement::None,
                    metadata: BTreeMap::new(),
                    ports: BTreeMap::from([(
                        port_id,
                        PortDescriptor {
                            id: port_id,
                            capability_id,
                            display_name: "DSU IMU Sample Sink".to_owned(),
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
                        Some(FormatDescriptor::new(IMU_FORMAT)),
                        QosMode::Measurement,
                        now_ms,
                    )
                    .map_err(string_error)?;
            }
            RouteState::Stopped => runtime
                .prepare_route(
                    self.route_id,
                    Some(FormatDescriptor::new(IMU_FORMAT)),
                    QosMode::Measurement,
                    now_ms,
                )
                .map_err(string_error)?,
            RouteState::Offline => runtime
                .recover_route(self.route_id, now_ms)
                .map_err(string_error)?,
            RouteState::Prepared => {}
            state => return Err(format!("DSU IMU Route cannot start from {state:?}")),
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
        owner: DsuProblemOwner,
        code: &str,
        message: &str,
        detail: String,
    ) -> Result<(), String> {
        let (category, related_node, related_adapter) = match owner {
            DsuProblemOwner::Source => (
                ProblemCategory::Transport,
                self.source_node_id,
                self.source_adapter_id,
            ),
            DsuProblemOwner::Projection => (
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
            RouteState::Failed => Err("failed DSU IMU Route cannot be stopped".to_owned()),
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
            .ok_or_else(|| format!("unknown DSU IMU source Node {}", reference.node_id))?
    };
    let capability = node
        .capabilities
        .get(&reference.capability_id)
        .ok_or_else(|| {
            format!(
                "unknown DSU IMU source Capability {}",
                reference.capability_id
            )
        })?;
    if !capability.ports.contains_key(&reference.port_id) {
        return Err(format!("unknown DSU IMU source Port {}", reference.port_id));
    }
    Ok(capability.adapter_instance_id)
}

fn append_cleanup(detail: &str, cleanup: Option<&str>) -> String {
    cleanup.map_or_else(
        || detail.to_owned(),
        |cleanup| format!("{detail}; DSU Worker cleanup reported: {cleanup}"),
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
