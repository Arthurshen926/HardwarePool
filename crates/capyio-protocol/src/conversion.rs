use std::collections::BTreeMap;
use std::str::FromStr;

use capyio_core as core;

use crate::{ProtocolError, v1 as pb};

impl From<&core::ProfileId> for pb::ProfileId {
    fn from(value: &core::ProfileId) -> Self {
        Self {
            name: value.name.clone(),
            major: u32::from(value.major),
        }
    }
}

impl TryFrom<pb::ProfileId> for core::ProfileId {
    type Error = ProtocolError;

    fn try_from(value: pb::ProfileId) -> Result<Self, Self::Error> {
        let profile = Self::new(value.name, u16_value("profile.major", value.major)?);
        profile.validate()?;
        Ok(profile)
    }
}

impl From<&core::FormatDescriptor> for pb::FormatDescriptor {
    fn from(value: &core::FormatDescriptor) -> Self {
        Self {
            id: value.id.clone(),
            parameters: value.parameters.clone().into_iter().collect(),
        }
    }
}

impl TryFrom<pb::FormatDescriptor> for core::FormatDescriptor {
    type Error = ProtocolError;

    fn try_from(value: pb::FormatDescriptor) -> Result<Self, Self::Error> {
        let format = Self {
            id: value.id,
            parameters: value.parameters.into_iter().collect(),
        };
        format.validate()?;
        Ok(format)
    }
}

impl From<&core::PortDescriptor> for pb::PortDescriptor {
    fn from(value: &core::PortDescriptor) -> Self {
        Self {
            id: value.id.to_string(),
            capability_id: value.capability_id.to_string(),
            display_name: value.display_name.clone(),
            direction: port_direction_to_wire(value.direction),
            profile: Some((&value.profile).into()),
            schema_id: value.schema_id.clone(),
            formats: value.formats.iter().map(Into::into).collect(),
            qos_modes: value.qos_modes.iter().map(qos_to_wire).collect(),
            clock_domain: value.clock_domain.clone(),
            availability: availability_to_wire(value.availability),
            permission_requirement: permission_to_wire(value.permission_requirement),
            interoperability_mode: interoperability_to_wire(value.interoperability_mode),
        }
    }
}

impl TryFrom<pb::PortDescriptor> for core::PortDescriptor {
    type Error = ProtocolError;

    fn try_from(value: pb::PortDescriptor) -> Result<Self, Self::Error> {
        let port = Self {
            id: parse_id("port.id", &value.id)?,
            capability_id: parse_id("port.capability_id", &value.capability_id)?,
            display_name: value.display_name,
            direction: port_direction_from_wire(value.direction)?,
            profile: required(value.profile, "port.profile")?.try_into()?,
            schema_id: value.schema_id,
            formats: try_collect(value.formats)?,
            qos_modes: value
                .qos_modes
                .into_iter()
                .map(qos_from_wire)
                .collect::<Result<_, _>>()?,
            clock_domain: value.clock_domain,
            availability: availability_from_wire(value.availability)?,
            permission_requirement: permission_from_wire(value.permission_requirement)?,
            interoperability_mode: interoperability_from_wire(value.interoperability_mode)?,
        };
        port.validate()?;
        Ok(port)
    }
}

impl TryFrom<&core::AdapterInstanceDescriptor> for pb::AdapterInstanceDescriptor {
    type Error = ProtocolError;

    fn try_from(value: &core::AdapterInstanceDescriptor) -> Result<Self, Self::Error> {
        value.validate()?;
        Ok(Self {
            id: value.id.to_string(),
            adapter_type: value.adapter_type.clone(),
            display_name: value.display_name.clone(),
            deployment_mode: deployment_to_wire(value.deployment_mode),
            version: value.version.clone(),
            state: adapter_state_to_wire(value.state),
            health: adapter_health_to_wire(value.health),
            owned_capability_ids: value
                .owned_capabilities
                .iter()
                .map(ToString::to_string)
                .collect(),
            supported_route_modes: value
                .supported_route_modes
                .iter()
                .copied()
                .map(route_backend_to_wire)
                .collect(),
        })
    }
}

impl TryFrom<pb::AdapterInstanceDescriptor> for core::AdapterInstanceDescriptor {
    type Error = ProtocolError;

    fn try_from(value: pb::AdapterInstanceDescriptor) -> Result<Self, Self::Error> {
        let adapter = Self {
            id: parse_id("adapter.id", &value.id)?,
            adapter_type: value.adapter_type,
            display_name: value.display_name,
            deployment_mode: deployment_from_wire(value.deployment_mode)?,
            version: value.version,
            state: adapter_state_from_wire(value.state)?,
            health: adapter_health_from_wire(value.health)?,
            owned_capabilities: value
                .owned_capability_ids
                .into_iter()
                .map(|id| parse_id("adapter.owned_capability_ids", &id))
                .collect::<Result<_, _>>()?,
            supported_route_modes: value
                .supported_route_modes
                .into_iter()
                .map(route_backend_from_wire)
                .collect::<Result<_, _>>()?,
        };
        adapter.validate()?;
        Ok(adapter)
    }
}

impl TryFrom<&core::CapabilityDescriptor> for pb::CapabilityDescriptor {
    type Error = ProtocolError;

    fn try_from(value: &core::CapabilityDescriptor) -> Result<Self, Self::Error> {
        value.validate()?;
        let (capability_class, custom_class) = capability_class_to_wire(&value.class);
        Ok(Self {
            id: value.id.to_string(),
            display_name: value.display_name.clone(),
            adapter_instance_id: value.adapter_instance_id.to_string(),
            capability_class,
            custom_class,
            availability: availability_to_wire(value.availability),
            permission_requirement: permission_to_wire(value.permission_requirement),
            metadata: value.metadata.clone().into_iter().collect(),
            ports: value.ports.values().map(Into::into).collect(),
        })
    }
}

impl TryFrom<pb::CapabilityDescriptor> for core::CapabilityDescriptor {
    type Error = ProtocolError;

    fn try_from(value: pb::CapabilityDescriptor) -> Result<Self, Self::Error> {
        let ports = value
            .ports
            .into_iter()
            .map(|port| {
                let port = core::PortDescriptor::try_from(port)?;
                Ok((port.id, port))
            })
            .collect::<Result<BTreeMap<_, _>, ProtocolError>>()?;
        let capability = Self {
            id: parse_id("capability.id", &value.id)?,
            adapter_instance_id: parse_id(
                "capability.adapter_instance_id",
                &value.adapter_instance_id,
            )?,
            display_name: value.display_name,
            class: capability_class_from_wire(value.capability_class, value.custom_class)?,
            availability: availability_from_wire(value.availability)?,
            permission_requirement: permission_from_wire(value.permission_requirement)?,
            metadata: value.metadata.into_iter().collect(),
            ports,
        };
        capability.validate()?;
        Ok(capability)
    }
}

impl TryFrom<&core::NodeDescriptor> for pb::NodeDescriptor {
    type Error = ProtocolError;

    fn try_from(value: &core::NodeDescriptor) -> Result<Self, Self::Error> {
        value.validate()?;
        let (platform, platform_detail) = platform_to_wire(&value.platform);
        Ok(Self {
            id: value.id.to_string(),
            display_name: value.display_name.clone(),
            platform,
            platform_detail,
            platform_version: value.platform_version.clone(),
            capabilities: value
                .capabilities
                .values()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            runtime_version: value.runtime_version.clone(),
            protocol_versions: value
                .protocol_versions
                .iter()
                .map(|version| pb::ProtocolVersion {
                    major: u32::from(version.major),
                    minor: u32::from(version.minor),
                })
                .collect(),
            online_state: online_state_to_wire(value.online_state),
            adapter_instances: value
                .adapter_instances
                .values()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

impl TryFrom<pb::NodeDescriptor> for core::NodeDescriptor {
    type Error = ProtocolError;

    fn try_from(value: pb::NodeDescriptor) -> Result<Self, Self::Error> {
        let versions = value
            .protocol_versions
            .into_iter()
            .map(|version| {
                Ok(core::ProtocolVersion::new(
                    u16_value("node.protocol_versions.major", version.major)?,
                    u16_value("node.protocol_versions.minor", version.minor)?,
                ))
            })
            .collect::<Result<Vec<_>, ProtocolError>>()?;
        let mut node = core::NodeDescriptor::new(
            parse_id("node.id", &value.id)?,
            value.display_name,
            platform_from_wire(value.platform, value.platform_detail)?,
            value.platform_version,
            value.runtime_version,
            versions,
        );
        node.online_state = online_state_from_wire(value.online_state)?;

        let mut expected_ownership = BTreeMap::new();
        for adapter in value.adapter_instances {
            let mut adapter = core::AdapterInstanceDescriptor::try_from(adapter)?;
            expected_ownership.insert(adapter.id, adapter.owned_capabilities.clone());
            adapter.owned_capabilities.clear();
            node.add_adapter(adapter)?;
        }
        for capability in value.capabilities {
            node.add_capability(capability.try_into()?)?;
        }
        for (adapter_id, expected) in expected_ownership {
            let actual = &node
                .adapter_instances
                .get(&adapter_id)
                .ok_or(ProtocolError::MissingField("node.adapter_instances"))?
                .owned_capabilities;
            if *actual != expected {
                return Err(ProtocolError::CatalogOwnershipMismatch {
                    adapter_id: adapter_id.to_string(),
                });
            }
        }
        node.validate()?;
        Ok(node)
    }
}

impl From<&core::PortRef> for pb::PortRef {
    fn from(value: &core::PortRef) -> Self {
        Self {
            node_id: value.node_id.to_string(),
            capability_id: value.capability_id.to_string(),
            port_id: value.port_id.to_string(),
        }
    }
}

impl TryFrom<pb::PortRef> for core::PortRef {
    type Error = ProtocolError;

    fn try_from(value: pb::PortRef) -> Result<Self, Self::Error> {
        Ok(Self {
            node_id: parse_id("port_ref.node_id", &value.node_id)?,
            capability_id: parse_id("port_ref.capability_id", &value.capability_id)?,
            port_id: parse_id("port_ref.port_id", &value.port_id)?,
        })
    }
}

impl TryFrom<&core::Route> for pb::RouteDescriptor {
    type Error = ProtocolError;

    fn try_from(value: &core::Route) -> Result<Self, Self::Error> {
        let (authorization, authorization_expires_at_ms) =
            authorization_to_wire(value.authorization);
        Ok(Self {
            id: value.id.to_string(),
            session_id: value.session_id.to_string(),
            source: Some((&value.source).into()),
            sink: Some((&value.sink).into()),
            profile: Some((&value.profile).into()),
            backend: route_backend_to_wire(value.backend),
            compatible_formats: value.compatible_formats.iter().map(Into::into).collect(),
            compatible_qos_modes: value.compatible_qos_modes.iter().map(qos_to_wire).collect(),
            selected_format: value.selected_format.as_ref().map(Into::into),
            selected_qos_mode: value.selected_qos_mode.as_ref().map(qos_to_wire),
            state: route_state_to_wire(value.state),
            authorization,
            authorization_expires_at_ms,
            epoch: value.epoch,
            diagnostic_ids: value
                .diagnostic_ids
                .iter()
                .map(ToString::to_string)
                .collect(),
        })
    }
}

impl TryFrom<pb::RouteDescriptor> for core::Route {
    type Error = ProtocolError;

    fn try_from(value: pb::RouteDescriptor) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_id("route.id", &value.id)?,
            session_id: parse_id("route.session_id", &value.session_id)?,
            source: required(value.source, "route.source")?.try_into()?,
            sink: required(value.sink, "route.sink")?.try_into()?,
            profile: required(value.profile, "route.profile")?.try_into()?,
            backend: route_backend_from_wire(value.backend)?,
            compatible_formats: try_collect(value.compatible_formats)?,
            compatible_qos_modes: value
                .compatible_qos_modes
                .into_iter()
                .map(qos_from_wire)
                .collect::<Result<_, _>>()?,
            selected_format: value.selected_format.map(TryInto::try_into).transpose()?,
            selected_qos_mode: value.selected_qos_mode.map(qos_from_wire).transpose()?,
            state: route_state_from_wire(value.state)?,
            authorization: authorization_from_wire(
                value.authorization,
                value.authorization_expires_at_ms,
            )?,
            epoch: value.epoch,
            diagnostic_ids: value
                .diagnostic_ids
                .into_iter()
                .map(|id| parse_id("route.diagnostic_ids", &id))
                .collect::<Result<_, _>>()?,
        })
    }
}

impl TryFrom<&core::Problem> for pb::ProblemDescriptor {
    type Error = ProtocolError;

    fn try_from(value: &core::Problem) -> Result<Self, Self::Error> {
        value.validate()?;
        Ok(Self {
            id: value.id.to_string(),
            code: value.code.clone(),
            category: problem_category_to_wire(value.category),
            severity: problem_severity_to_wire(value.severity),
            retryable: value.retryable,
            related_node_id: value.related_node.map(|id| id.to_string()),
            related_adapter_id: value.related_adapter.map(|id| id.to_string()),
            related_route_id: value.related_route.map(|id| id.to_string()),
            human_message: value.human_message.clone(),
            technical_detail: value.technical_detail.clone(),
        })
    }
}

impl TryFrom<pb::ProblemDescriptor> for core::Problem {
    type Error = ProtocolError;

    fn try_from(value: pb::ProblemDescriptor) -> Result<Self, Self::Error> {
        let problem = Self {
            id: parse_id("problem.id", &value.id)?,
            code: value.code,
            category: problem_category_from_wire(value.category)?,
            severity: problem_severity_from_wire(value.severity)?,
            retryable: value.retryable,
            related_node: parse_optional_id("problem.related_node_id", value.related_node_id)?,
            related_adapter: parse_optional_id(
                "problem.related_adapter_id",
                value.related_adapter_id,
            )?,
            related_route: parse_optional_id("problem.related_route_id", value.related_route_id)?,
            human_message: value.human_message,
            technical_detail: value.technical_detail,
        };
        problem.validate()?;
        Ok(problem)
    }
}

fn required<T>(value: Option<T>, field: &'static str) -> Result<T, ProtocolError> {
    value.ok_or(ProtocolError::MissingField(field))
}
fn u16_value(field: &'static str, value: u32) -> Result<u16, ProtocolError> {
    u16::try_from(value).map_err(|_| ProtocolError::NumericRange {
        field,
        value: u64::from(value),
    })
}
fn try_collect<T, U>(values: Vec<T>) -> Result<Vec<U>, ProtocolError>
where
    U: TryFrom<T, Error = ProtocolError>,
{
    values.into_iter().map(TryInto::try_into).collect()
}
fn parse_id<T>(field: &'static str, value: &str) -> Result<T, ProtocolError>
where
    T: FromStr<Err = uuid::Error>,
{
    value.parse().map_err(|_| ProtocolError::InvalidId {
        field,
        value: value.to_owned(),
    })
}
fn parse_optional_id<T>(
    field: &'static str,
    value: Option<String>,
) -> Result<Option<T>, ProtocolError>
where
    T: FromStr<Err = uuid::Error>,
{
    value.map(|id| parse_id(field, &id)).transpose()
}
fn invalid_enum(field: &'static str, value: i32) -> ProtocolError {
    ProtocolError::InvalidEnum { field, value }
}

fn platform_to_wire(value: &core::Platform) -> (i32, String) {
    match value {
        core::Platform::Windows => (1, String::new()),
        core::Platform::Android => (2, String::new()),
        core::Platform::Linux => (3, String::new()),
        core::Platform::Macos => (4, String::new()),
        core::Platform::Ios => (5, String::new()),
        core::Platform::Embedded => (6, String::new()),
        core::Platform::Unknown(detail) => (100, detail.clone()),
    }
}
fn platform_from_wire(value: i32, detail: String) -> Result<core::Platform, ProtocolError> {
    match value {
        1 => Ok(core::Platform::Windows),
        2 => Ok(core::Platform::Android),
        3 => Ok(core::Platform::Linux),
        4 => Ok(core::Platform::Macos),
        5 => Ok(core::Platform::Ios),
        6 => Ok(core::Platform::Embedded),
        100 if !detail.trim().is_empty() => Ok(core::Platform::Unknown(detail)),
        _ => Err(invalid_enum("node.platform", value)),
    }
}
fn online_state_to_wire(value: core::OnlineState) -> i32 {
    match value {
        core::OnlineState::Online => 1,
        core::OnlineState::Offline => 2,
    }
}
fn online_state_from_wire(value: i32) -> Result<core::OnlineState, ProtocolError> {
    match value {
        1 => Ok(core::OnlineState::Online),
        2 => Ok(core::OnlineState::Offline),
        _ => Err(invalid_enum("node.online_state", value)),
    }
}
fn permission_to_wire(value: core::PermissionRequirement) -> i32 {
    match value {
        core::PermissionRequirement::None => 1,
        core::PermissionRequirement::UserConfirmation => 2,
        core::PermissionRequirement::ForegroundService => 3,
        core::PermissionRequirement::Privileged => 4,
    }
}
fn permission_from_wire(value: i32) -> Result<core::PermissionRequirement, ProtocolError> {
    match value {
        1 => Ok(core::PermissionRequirement::None),
        2 => Ok(core::PermissionRequirement::UserConfirmation),
        3 => Ok(core::PermissionRequirement::ForegroundService),
        4 => Ok(core::PermissionRequirement::Privileged),
        _ => Err(invalid_enum("permission_requirement", value)),
    }
}
fn availability_to_wire(value: core::Availability) -> i32 {
    match value {
        core::Availability::Available => 1,
        core::Availability::Busy => 2,
        core::Availability::PermissionRequired => 3,
        core::Availability::Offline => 4,
    }
}
fn availability_from_wire(value: i32) -> Result<core::Availability, ProtocolError> {
    match value {
        1 => Ok(core::Availability::Available),
        2 => Ok(core::Availability::Busy),
        3 => Ok(core::Availability::PermissionRequired),
        4 => Ok(core::Availability::Offline),
        _ => Err(invalid_enum("availability", value)),
    }
}
fn port_direction_to_wire(value: core::PortDirection) -> i32 {
    match value {
        core::PortDirection::Source => 1,
        core::PortDirection::Sink => 2,
        core::PortDirection::Control => 3,
    }
}
fn port_direction_from_wire(value: i32) -> Result<core::PortDirection, ProtocolError> {
    match value {
        1 => Ok(core::PortDirection::Source),
        2 => Ok(core::PortDirection::Sink),
        3 => Ok(core::PortDirection::Control),
        _ => Err(invalid_enum("port.direction", value)),
    }
}
fn interoperability_to_wire(value: core::InteroperabilityMode) -> i32 {
    match value {
        core::InteroperabilityMode::StandardPort => 1,
        core::InteroperabilityMode::AdapterManaged => 2,
    }
}
fn interoperability_from_wire(value: i32) -> Result<core::InteroperabilityMode, ProtocolError> {
    match value {
        1 => Ok(core::InteroperabilityMode::StandardPort),
        2 => Ok(core::InteroperabilityMode::AdapterManaged),
        _ => Err(invalid_enum("port.interoperability_mode", value)),
    }
}
fn deployment_to_wire(value: core::AdapterDeploymentMode) -> i32 {
    match value {
        core::AdapterDeploymentMode::InProcess => 1,
        core::AdapterDeploymentMode::Sidecar => 2,
        core::AdapterDeploymentMode::ExternalService => 3,
        core::AdapterDeploymentMode::DriverBacked => 4,
    }
}
fn deployment_from_wire(value: i32) -> Result<core::AdapterDeploymentMode, ProtocolError> {
    match value {
        1 => Ok(core::AdapterDeploymentMode::InProcess),
        2 => Ok(core::AdapterDeploymentMode::Sidecar),
        3 => Ok(core::AdapterDeploymentMode::ExternalService),
        4 => Ok(core::AdapterDeploymentMode::DriverBacked),
        _ => Err(invalid_enum("adapter.deployment_mode", value)),
    }
}
fn adapter_state_to_wire(value: core::AdapterState) -> i32 {
    match value {
        core::AdapterState::Discovered => 1,
        core::AdapterState::Initializing => 2,
        core::AdapterState::Ready => 3,
        core::AdapterState::Degraded => 4,
        core::AdapterState::Failed => 5,
        core::AdapterState::Stopped => 6,
    }
}
fn adapter_state_from_wire(value: i32) -> Result<core::AdapterState, ProtocolError> {
    match value {
        1 => Ok(core::AdapterState::Discovered),
        2 => Ok(core::AdapterState::Initializing),
        3 => Ok(core::AdapterState::Ready),
        4 => Ok(core::AdapterState::Degraded),
        5 => Ok(core::AdapterState::Failed),
        6 => Ok(core::AdapterState::Stopped),
        _ => Err(invalid_enum("adapter.state", value)),
    }
}
fn adapter_health_to_wire(value: core::AdapterHealth) -> i32 {
    match value {
        core::AdapterHealth::Unknown => 1,
        core::AdapterHealth::Healthy => 2,
        core::AdapterHealth::Degraded => 3,
        core::AdapterHealth::Unhealthy => 4,
    }
}
fn adapter_health_from_wire(value: i32) -> Result<core::AdapterHealth, ProtocolError> {
    match value {
        1 => Ok(core::AdapterHealth::Unknown),
        2 => Ok(core::AdapterHealth::Healthy),
        3 => Ok(core::AdapterHealth::Degraded),
        4 => Ok(core::AdapterHealth::Unhealthy),
        _ => Err(invalid_enum("adapter.health", value)),
    }
}
fn route_backend_to_wire(value: core::RouteBackend) -> i32 {
    match value {
        core::RouteBackend::CapyDataPlane => 1,
        core::RouteBackend::AdapterManaged => 2,
        core::RouteBackend::LocalPipeline => 3,
        core::RouteBackend::ExternalProtocol => 4,
    }
}
fn route_backend_from_wire(value: i32) -> Result<core::RouteBackend, ProtocolError> {
    match value {
        1 => Ok(core::RouteBackend::CapyDataPlane),
        2 => Ok(core::RouteBackend::AdapterManaged),
        3 => Ok(core::RouteBackend::LocalPipeline),
        4 => Ok(core::RouteBackend::ExternalProtocol),
        _ => Err(invalid_enum("route.backend", value)),
    }
}
fn qos_to_wire(value: &core::QosMode) -> pb::QosDescriptor {
    let (mode, custom_mode) = match value {
        core::QosMode::Basic => (1, String::new()),
        core::QosMode::Interactive => (2, String::new()),
        core::QosMode::Measurement => (3, String::new()),
        core::QosMode::Custom(custom) => (100, custom.clone()),
    };
    pb::QosDescriptor { mode, custom_mode }
}
fn qos_from_wire(value: pb::QosDescriptor) -> Result<core::QosMode, ProtocolError> {
    match value.mode {
        1 => Ok(core::QosMode::Basic),
        2 => Ok(core::QosMode::Interactive),
        3 => Ok(core::QosMode::Measurement),
        100 if !value.custom_mode.trim().is_empty() => Ok(core::QosMode::Custom(value.custom_mode)),
        _ => Err(invalid_enum("qos.mode", value.mode)),
    }
}

fn capability_class_to_wire(value: &core::CapabilityClass) -> (i32, String) {
    match value {
        core::CapabilityClass::Microphone => (1, String::new()),
        core::CapabilityClass::Speaker => (2, String::new()),
        core::CapabilityClass::Camera => (3, String::new()),
        core::CapabilityClass::Display => (4, String::new()),
        core::CapabilityClass::Keyboard => (5, String::new()),
        core::CapabilityClass::Pointer => (6, String::new()),
        core::CapabilityClass::Touchscreen => (7, String::new()),
        core::CapabilityClass::Gamepad => (8, String::new()),
        core::CapabilityClass::Imu => (9, String::new()),
        core::CapabilityClass::Gnss => (10, String::new()),
        core::CapabilityClass::SensorSuite => (11, String::new()),
        core::CapabilityClass::Haptics => (12, String::new()),
        core::CapabilityClass::Recorder => (13, String::new()),
        core::CapabilityClass::Panel => (14, String::new()),
        core::CapabilityClass::Bridge => (15, String::new()),
        core::CapabilityClass::Touchpad => (16, String::new()),
        core::CapabilityClass::Custom(custom) => (100, custom.clone()),
    }
}
fn capability_class_from_wire(
    value: i32,
    custom: String,
) -> Result<core::CapabilityClass, ProtocolError> {
    match value {
        1 => Ok(core::CapabilityClass::Microphone),
        2 => Ok(core::CapabilityClass::Speaker),
        3 => Ok(core::CapabilityClass::Camera),
        4 => Ok(core::CapabilityClass::Display),
        5 => Ok(core::CapabilityClass::Keyboard),
        6 => Ok(core::CapabilityClass::Pointer),
        7 => Ok(core::CapabilityClass::Touchscreen),
        8 => Ok(core::CapabilityClass::Gamepad),
        9 => Ok(core::CapabilityClass::Imu),
        10 => Ok(core::CapabilityClass::Gnss),
        11 => Ok(core::CapabilityClass::SensorSuite),
        12 => Ok(core::CapabilityClass::Haptics),
        13 => Ok(core::CapabilityClass::Recorder),
        14 => Ok(core::CapabilityClass::Panel),
        15 => Ok(core::CapabilityClass::Bridge),
        16 => Ok(core::CapabilityClass::Touchpad),
        100 if !custom.trim().is_empty() => Ok(core::CapabilityClass::Custom(custom)),
        _ => Err(invalid_enum("capability.class", value)),
    }
}
fn route_state_to_wire(value: core::RouteState) -> i32 {
    match value {
        core::RouteState::Draft => 1,
        core::RouteState::Prepared => 2,
        core::RouteState::Starting => 3,
        core::RouteState::Active => 4,
        core::RouteState::Stopping => 5,
        core::RouteState::Stopped => 6,
        core::RouteState::Failed => 7,
        core::RouteState::Offline => 8,
    }
}
fn route_state_from_wire(value: i32) -> Result<core::RouteState, ProtocolError> {
    match value {
        1 => Ok(core::RouteState::Draft),
        2 => Ok(core::RouteState::Prepared),
        3 => Ok(core::RouteState::Starting),
        4 => Ok(core::RouteState::Active),
        5 => Ok(core::RouteState::Stopping),
        6 => Ok(core::RouteState::Stopped),
        7 => Ok(core::RouteState::Failed),
        8 => Ok(core::RouteState::Offline),
        _ => Err(invalid_enum("route.state", value)),
    }
}
fn authorization_to_wire(value: core::AuthorizationState) -> (i32, Option<u64>) {
    match value {
        core::AuthorizationState::Pending => (1, None),
        core::AuthorizationState::Authorized { expires_at_ms } => (2, expires_at_ms),
        core::AuthorizationState::Denied => (3, None),
        core::AuthorizationState::Revoked => (4, None),
    }
}
fn authorization_from_wire(
    value: i32,
    expires_at_ms: Option<u64>,
) -> Result<core::AuthorizationState, ProtocolError> {
    match value {
        1 if expires_at_ms.is_none() => Ok(core::AuthorizationState::Pending),
        2 => Ok(core::AuthorizationState::Authorized { expires_at_ms }),
        3 if expires_at_ms.is_none() => Ok(core::AuthorizationState::Denied),
        4 if expires_at_ms.is_none() => Ok(core::AuthorizationState::Revoked),
        _ => Err(invalid_enum("route.authorization", value)),
    }
}
fn problem_category_to_wire(value: core::ProblemCategory) -> i32 {
    value as i32 + 1
}
fn problem_category_from_wire(value: i32) -> Result<core::ProblemCategory, ProtocolError> {
    match value {
        1 => Ok(core::ProblemCategory::Protocol),
        2 => Ok(core::ProblemCategory::Identity),
        3 => Ok(core::ProblemCategory::Authorization),
        4 => Ok(core::ProblemCategory::Capability),
        5 => Ok(core::ProblemCategory::Route),
        6 => Ok(core::ProblemCategory::Adapter),
        7 => Ok(core::ProblemCategory::Transport),
        8 => Ok(core::ProblemCategory::Platform),
        9 => Ok(core::ProblemCategory::Data),
        10 => Ok(core::ProblemCategory::Driver),
        11 => Ok(core::ProblemCategory::Internal),
        _ => Err(invalid_enum("problem.category", value)),
    }
}
fn problem_severity_to_wire(value: core::ProblemSeverity) -> i32 {
    value as i32 + 1
}
fn problem_severity_from_wire(value: i32) -> Result<core::ProblemSeverity, ProtocolError> {
    match value {
        1 => Ok(core::ProblemSeverity::Info),
        2 => Ok(core::ProblemSeverity::Warning),
        3 => Ok(core::ProblemSeverity::Error),
        4 => Ok(core::ProblemSeverity::Critical),
        _ => Err(invalid_enum("problem.severity", value)),
    }
}
