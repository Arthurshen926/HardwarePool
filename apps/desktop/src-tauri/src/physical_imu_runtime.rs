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
use capyio_runtime::NodeRuntime;
use capyio_testkit::{ANDROID_NODE_ID, WINDOWS_NODE_ID};

const PANEL_ADAPTER_ID: &str = "00000000-0000-4000-8000-00000000b011";
const SENSOR_SERVER_ADAPTER_ID: &str = "00000000-0000-4000-8000-00000000b021";
const PANEL_CAPABILITY_ID: &str = "00000000-0000-4000-8000-00000000b101";
const SENSOR_CAPABILITY_ID: &str = "00000000-0000-4000-8000-00000000b201";
const PANEL_PORT_ID: &str = "00000000-0000-4000-8000-00000000b111";
const SENSOR_PORT_ID: &str = "00000000-0000-4000-8000-00000000b211";
const ROUTE_ID: &str = "00000000-0000-4000-8000-00000000b911";

#[derive(Clone, Copy)]
pub struct PhysicalImuRoute {
    route_id: RouteId,
    source_node_id: NodeId,
    source_adapter_id: AdapterInstanceId,
}

impl PhysicalImuRoute {
    pub fn install(runtime: &mut NodeRuntime, session_id: SessionId) -> Result<Self, String> {
        let local_node_id = parse_id(WINDOWS_NODE_ID)?;
        let source_node_id = parse_id(ANDROID_NODE_ID)?;
        let panel_adapter_id = parse_id(PANEL_ADAPTER_ID)?;
        let source_adapter_id = parse_id(SENSOR_SERVER_ADAPTER_ID)?;

        runtime
            .register_adapter_catalog(
                local_node_id,
                adapter(
                    panel_adapter_id,
                    "capyio.panel.imu",
                    "Built-in IMU Panel Adapter",
                    AdapterDeploymentMode::InProcess,
                ),
                vec![capability(CapabilitySpec {
                    adapter_id: panel_adapter_id,
                    capability_id: PANEL_CAPABILITY_ID,
                    capability_name: "Live IMU Numeric Panel",
                    capability_class: CapabilityClass::Panel,
                    port_id: PANEL_PORT_ID,
                    port_name: "Live IMU Panel Sink",
                    direction: PortDirection::Sink,
                    clock_domain: None,
                })?],
            )
            .map_err(|error| error.to_string())?;
        runtime
            .register_adapter_catalog(
                source_node_id,
                adapter(
                    source_adapter_id,
                    "dev.capyio.sensorserver",
                    "SensorServer External Service Adapter",
                    AdapterDeploymentMode::ExternalService,
                ),
                vec![capability(CapabilitySpec {
                    adapter_id: source_adapter_id,
                    capability_id: SENSOR_CAPABILITY_ID,
                    capability_name: "Phone IMU",
                    capability_class: CapabilityClass::Imu,
                    port_id: SENSOR_PORT_ID,
                    port_name: "SensorServer IMU Source",
                    direction: PortDirection::Source,
                    clock_domain: Some("android.sensor.elapsed_realtime".to_owned()),
                })?],
            )
            .map_err(|error| error.to_string())?;

        let route_id = parse_id(ROUTE_ID)?;
        runtime
            .create_route_with_id(
                route_id,
                session_id,
                port_ref(source_node_id, SENSOR_CAPABILITY_ID, SENSOR_PORT_ID)?,
                port_ref(local_node_id, PANEL_CAPABILITY_ID, PANEL_PORT_ID)?,
                RouteBackend::ExternalProtocol,
            )
            .map_err(|error| error.to_string())?;
        Ok(Self {
            route_id,
            source_node_id,
            source_adapter_id,
        })
    }

    pub fn begin_start(self, runtime: &mut NodeRuntime, now_ms: u64) -> Result<u64, String> {
        match self.route_state(runtime)? {
            RouteState::Draft => {
                runtime
                    .authorize_route(self.route_id, None)
                    .map_err(|error| error.to_string())?;
                runtime
                    .prepare_route(
                        self.route_id,
                        Some(FormatDescriptor::new("imu-si-f32-le")),
                        QosMode::Measurement,
                        now_ms,
                    )
                    .map_err(|error| error.to_string())?;
            }
            RouteState::Stopped => runtime
                .prepare_route(
                    self.route_id,
                    Some(FormatDescriptor::new("imu-si-f32-le")),
                    QosMode::Measurement,
                    now_ms,
                )
                .map_err(|error| error.to_string())?,
            RouteState::Offline => runtime
                .recover_route(self.route_id, now_ms)
                .map_err(|error| error.to_string())?,
            RouteState::Prepared => {}
            state => return Err(format!("physical IMU Route cannot start from {state:?}")),
        }
        runtime
            .begin_route_start(self.route_id, now_ms)
            .map_err(|error| error.to_string())?;
        self.route_epoch(runtime)
    }

    pub fn activate(self, runtime: &mut NodeRuntime) -> Result<(), String> {
        runtime
            .activate_route(self.route_id)
            .map_err(|error| error.to_string())
    }

    pub fn report_offline(self, runtime: &mut NodeRuntime, detail: String) -> Result<(), String> {
        runtime
            .report_route_offline(
                self.route_id,
                Problem {
                    id: ProblemId::new(),
                    code: "CAPY.IMU.SENSORSERVER_DISCONNECTED".to_owned(),
                    category: ProblemCategory::Transport,
                    severity: ProblemSeverity::Error,
                    retryable: true,
                    related_node: Some(self.source_node_id),
                    related_adapter: Some(self.source_adapter_id),
                    related_route: Some(self.route_id),
                    human_message: "The physical IMU source is unavailable".to_owned(),
                    technical_detail: Some(detail.chars().take(1024).collect()),
                },
            )
            .map_err(|error| error.to_string())
    }

    pub fn stop(self, runtime: &mut NodeRuntime) -> Result<(), String> {
        match self.route_state(runtime)? {
            RouteState::Prepared
            | RouteState::Starting
            | RouteState::Active
            | RouteState::Offline => {
                runtime
                    .begin_route_stop(self.route_id)
                    .map_err(|error| error.to_string())?;
                runtime
                    .stop_route(self.route_id)
                    .map_err(|error| error.to_string())
            }
            RouteState::Stopping => runtime
                .stop_route(self.route_id)
                .map_err(|error| error.to_string()),
            RouteState::Draft | RouteState::Stopped => Ok(()),
            RouteState::Failed => Err("failed physical IMU Route cannot be stopped".to_owned()),
        }
    }

    pub const fn route_id(self) -> RouteId {
        self.route_id
    }

    pub fn route_state(self, runtime: &NodeRuntime) -> Result<RouteState, String> {
        runtime
            .route(self.route_id)
            .map(|route| route.state)
            .map_err(|error| error.to_string())
    }

    pub fn route_epoch(self, runtime: &NodeRuntime) -> Result<u64, String> {
        runtime
            .route(self.route_id)
            .map(|route| route.epoch)
            .map_err(|error| error.to_string())
    }

    pub fn latest_problem(self, runtime: &NodeRuntime) -> Option<Problem> {
        runtime
            .snapshot()
            .problems
            .into_iter()
            .rev()
            .find(|problem| problem.related_route == Some(self.route_id))
    }
}

fn adapter(
    id: AdapterInstanceId,
    adapter_type: &str,
    display_name: &str,
    deployment_mode: AdapterDeploymentMode,
) -> AdapterInstanceDescriptor {
    AdapterInstanceDescriptor {
        id,
        adapter_type: adapter_type.to_owned(),
        display_name: display_name.to_owned(),
        deployment_mode,
        version: env!("CARGO_PKG_VERSION").to_owned(),
        state: AdapterState::Ready,
        health: AdapterHealth::Healthy,
        owned_capabilities: BTreeSet::new(),
        supported_route_modes: BTreeSet::from([RouteBackend::ExternalProtocol]),
    }
}

struct CapabilitySpec<'a> {
    adapter_id: AdapterInstanceId,
    capability_id: &'a str,
    capability_name: &'a str,
    capability_class: CapabilityClass,
    port_id: &'a str,
    port_name: &'a str,
    direction: PortDirection,
    clock_domain: Option<String>,
}

fn capability(spec: CapabilitySpec<'_>) -> Result<CapabilityDescriptor, String> {
    let capability_id = parse_id(spec.capability_id)?;
    let port_id = parse_id(spec.port_id)?;
    let port = PortDescriptor {
        id: port_id,
        capability_id,
        display_name: spec.port_name.to_owned(),
        direction: spec.direction,
        profile: ProfileId::imu_samples_v1(),
        schema_id: None,
        formats: vec![FormatDescriptor::new("imu-si-f32-le")],
        qos_modes: BTreeSet::from([QosMode::Measurement]),
        clock_domain: spec.clock_domain,
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::None,
        interoperability_mode: InteroperabilityMode::StandardPort,
    };
    Ok(CapabilityDescriptor {
        id: capability_id,
        adapter_instance_id: spec.adapter_id,
        display_name: spec.capability_name.to_owned(),
        class: spec.capability_class,
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::None,
        metadata: BTreeMap::new(),
        ports: BTreeMap::from([(port_id, port)]),
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

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_testkit::DemoLab;

    #[test]
    fn runtime_route_disconnect_retry_and_stop_are_explicit() {
        let mut lab = DemoLab::new().expect("demo Runtime fixture");
        let session_id = lab.session_id;
        let route = PhysicalImuRoute::install(&mut lab.runtime, session_id)
            .expect("physical Route fixture");
        let first_epoch = route.begin_start(&mut lab.runtime, 1).expect("start Route");
        assert_eq!(route.route_state(&lab.runtime), Ok(RouteState::Starting));
        route.activate(&mut lab.runtime).expect("activate Route");
        route
            .report_offline(&mut lab.runtime, "loopback disconnect".to_owned())
            .expect("offline Route");
        let offline_epoch = route.route_epoch(&lab.runtime).expect("offline epoch");
        assert!(offline_epoch > first_epoch);
        let retry_epoch = route.begin_start(&mut lab.runtime, 2).expect("retry Route");
        assert!(retry_epoch > offline_epoch);
        route.activate(&mut lab.runtime).expect("reactivate Route");
        route.stop(&mut lab.runtime).expect("stop Route");
        assert_eq!(route.route_state(&lab.runtime), Ok(RouteState::Stopped));
        let snapshot = lab.runtime.snapshot();
        assert!(snapshot.problems.iter().any(|problem| problem.code
            == "CAPY.IMU.SENSORSERVER_DISCONNECTED"
            && problem.related_route == Some(route.route_id())));
        assert!(
            snapshot
                .events
                .windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence)
        );
    }
}
