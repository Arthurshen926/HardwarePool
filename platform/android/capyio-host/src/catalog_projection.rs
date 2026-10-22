use std::{collections::BTreeSet, error::Error, fmt, str::FromStr};

use capyio_core::{
    AdapterDeploymentMode, AdapterHealth, AdapterInstanceDescriptor, AdapterState, Availability,
    CapabilityClass, CapabilityDescriptor, CoreError, FormatDescriptor, InteroperabilityMode,
    NodeDescriptor, NodeId, PermissionRequirement, Platform, PortDescriptor, PortDirection,
    ProfileId, ProtocolVersion, QosMode, RouteBackend,
};
use serde::{Deserialize, Serialize};

use crate::{
    AndroidCatalogIdentityError, android_adapter_id, android_capability_id, android_port_id,
};

const MAX_CAPABILITIES: usize = 64;
const MAX_PORTS_PER_CAPABILITY: usize = 8;
const MAX_FORMATS_PER_PORT: usize = 16;
pub const MAX_ANDROID_CATALOG_JSON_BYTES: usize = 64 * 1024;
pub const ANDROID_CATALOG_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AndroidCapabilityClassV1 {
    Microphone,
    Speaker,
    Camera,
    Imu,
    Gamepad,
    Haptics,
    Touchpad,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AndroidCapabilityAvailabilityV1 {
    Available,
    Busy,
    PermissionRequired,
    Offline,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AndroidPermissionRequirementV1 {
    None,
    UserConfirmation,
    ForegroundService,
    Privileged,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AndroidPortDirectionV1 {
    Source,
    Sink,
    Control,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AndroidQosModeV1 {
    Basic,
    Interactive,
    Measurement,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AndroidInteroperabilityModeV1 {
    StandardPort,
    AdapterManaged,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AndroidAdapterRouteModeV1 {
    CapyDataPlane,
    AdapterManaged,
    LocalPipeline,
    ExternalProtocol,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AndroidPortDtoV1 {
    pub key: String,
    pub display_name: String,
    pub direction: AndroidPortDirectionV1,
    pub profile: String,
    pub formats: Vec<String>,
    pub qos_modes: Vec<AndroidQosModeV1>,
    pub clock_domain: Option<String>,
    pub interoperability: AndroidInteroperabilityModeV1,
}

/// Explicit speaker setting; absence retains the historical legacy catalog.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AndroidSpeakerSelectionV1 {
    pub profile: String,
    pub revision: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AndroidCapabilityDtoV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_selection: Option<AndroidSpeakerSelectionV1>,
    pub key: String,
    pub display_name: String,
    pub class: AndroidCapabilityClassV1,
    pub availability: AndroidCapabilityAvailabilityV1,
    pub permission_requirement: AndroidPermissionRequirementV1,
    pub adapter_attached: bool,
    pub adapter_type: String,
    pub adapter_version: String,
    pub supported_route_modes: BTreeSet<AndroidAdapterRouteModeV1>,
    pub ports: Vec<AndroidPortDtoV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AndroidCatalogDtoV1 {
    pub schema_version: u32,
    pub node_id: String,
    pub display_name: String,
    pub platform_version: String,
    pub runtime_version: String,
    pub capabilities: Vec<AndroidCapabilityDtoV1>,
}

#[derive(Debug)]
pub enum AndroidCatalogProjectionError {
    MessageTooLarge { limit: usize, actual: usize },
    InvalidJson(serde_json::Error),
    UnsupportedSchemaVersion(u32),
    InvalidNodeId,
    InvalidField(&'static str),
    DuplicateCapabilityKey(String),
    Identity(AndroidCatalogIdentityError),
    Core(CoreError),
}

impl fmt::Display for AndroidCatalogProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MessageTooLarge { limit, actual } => {
                write!(
                    formatter,
                    "Android catalog JSON is {actual} bytes; maximum is {limit}"
                )
            }
            Self::InvalidJson(error) => error.fmt(formatter),
            Self::UnsupportedSchemaVersion(version) => {
                write!(
                    formatter,
                    "unsupported Android catalog schema version {version}"
                )
            }
            Self::InvalidNodeId => formatter.write_str("Android Node ID must be a UUID"),
            Self::InvalidField(field) => {
                write!(formatter, "invalid Android catalog field: {field}")
            }
            Self::DuplicateCapabilityKey(key) => {
                write!(formatter, "duplicate Android Capability key: {key}")
            }
            Self::Identity(error) => error.fmt(formatter),
            Self::Core(error) => error.fmt(formatter),
        }
    }
}

impl Error for AndroidCatalogProjectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidJson(error) => Some(error),
            Self::Identity(error) => Some(error),
            Self::Core(error) => Some(error),
            _ => None,
        }
    }
}

pub fn decode_android_catalog_json(
    json: &str,
) -> Result<NodeDescriptor, AndroidCatalogProjectionError> {
    if json.len() > MAX_ANDROID_CATALOG_JSON_BYTES {
        return Err(AndroidCatalogProjectionError::MessageTooLarge {
            limit: MAX_ANDROID_CATALOG_JSON_BYTES,
            actual: json.len(),
        });
    }
    let dto: AndroidCatalogDtoV1 =
        serde_json::from_str(json).map_err(AndroidCatalogProjectionError::InvalidJson)?;
    project_android_catalog(dto)
}

impl From<AndroidCatalogIdentityError> for AndroidCatalogProjectionError {
    fn from(value: AndroidCatalogIdentityError) -> Self {
        Self::Identity(value)
    }
}

impl From<CoreError> for AndroidCatalogProjectionError {
    fn from(value: CoreError) -> Self {
        Self::Core(value)
    }
}

pub fn project_android_catalog(
    dto: AndroidCatalogDtoV1,
) -> Result<NodeDescriptor, AndroidCatalogProjectionError> {
    if dto.schema_version != ANDROID_CATALOG_SCHEMA_VERSION {
        return Err(AndroidCatalogProjectionError::UnsupportedSchemaVersion(
            dto.schema_version,
        ));
    }
    let node_id =
        NodeId::from_str(&dto.node_id).map_err(|_| AndroidCatalogProjectionError::InvalidNodeId)?;
    require_text(&dto.display_name, "node.display_name")?;
    require_text(&dto.platform_version, "node.platform_version")?;
    require_text(&dto.runtime_version, "node.runtime_version")?;
    if dto.capabilities.is_empty() || dto.capabilities.len() > MAX_CAPABILITIES {
        return Err(AndroidCatalogProjectionError::InvalidField(
            "node.capabilities",
        ));
    }

    let mut node = NodeDescriptor::new(
        node_id,
        dto.display_name,
        Platform::Android,
        dto.platform_version,
        dto.runtime_version,
        [ProtocolVersion::new(1, 0)],
    );
    let mut capability_keys = BTreeSet::new();
    for capability in dto.capabilities {
        if !capability_keys.insert(capability.key.clone()) {
            return Err(AndroidCatalogProjectionError::DuplicateCapabilityKey(
                capability.key,
            ));
        }
        add_capability(&mut node, node_id, capability)?;
    }
    node.validate()?;
    Ok(node)
}

fn add_capability(
    node: &mut NodeDescriptor,
    node_id: NodeId,
    dto: AndroidCapabilityDtoV1,
) -> Result<(), AndroidCatalogProjectionError> {
    require_text(&dto.display_name, "capability.display_name")?;
    require_text(&dto.adapter_type, "capability.adapter_type")?;
    require_text(&dto.adapter_version, "capability.adapter_version")?;
    if dto.supported_route_modes.is_empty()
        || dto.ports.is_empty()
        || dto.ports.len() > MAX_PORTS_PER_CAPABILITY
    {
        return Err(AndroidCatalogProjectionError::InvalidField(
            "capability routes or ports",
        ));
    }
    const SECURE_STEREO: &str = "pcm-s16le-48000-stereo-tls13-v1";
    let advertises_secure = dto
        .ports
        .iter()
        .any(|port| port.formats.iter().any(|f| f == SECURE_STEREO));
    if let Some(selection) = &dto.speaker_selection {
        if dto.class != AndroidCapabilityClassV1::Speaker
            || selection.revision == 0
            || selection.revision > 9_007_199_254_740_991
            || !matches!(
                selection.profile.as_str(),
                "legacy" | "secure_stereo_tls13_v1"
            )
            || (selection.profile == "secure_stereo_tls13_v1") != advertises_secure
        {
            return Err(AndroidCatalogProjectionError::InvalidField(
                "speaker_selection",
            ));
        }
    } else if advertises_secure {
        return Err(AndroidCatalogProjectionError::InvalidField(
            "speaker_selection required for secure stereo",
        ));
    }
    if advertises_secure
        && (dto.ports.len() != 1
            || dto.ports[0].formats != [SECURE_STEREO]
            || dto.ports[0].direction != AndroidPortDirectionV1::Sink
            || dto.ports[0].profile != "capyio.audio.frames/1"
            || dto.ports[0].qos_modes != [AndroidQosModeV1::Interactive]
            || dto.ports[0].interoperability != AndroidInteroperabilityModeV1::AdapterManaged
            || !dto
                .supported_route_modes
                .contains(&AndroidAdapterRouteModeV1::AdapterManaged))
    {
        return Err(AndroidCatalogProjectionError::InvalidField(
            "exact secure speaker port",
        ));
    }
    let availability = availability(dto.availability);
    if !dto.adapter_attached && availability != Availability::Offline {
        return Err(AndroidCatalogProjectionError::InvalidField(
            "unattached capability availability",
        ));
    }
    let adapter_id = android_adapter_id(node_id, &dto.key)?;
    let capability_id = android_capability_id(node_id, &dto.key)?;
    let (state, health) = adapter_state(dto.adapter_attached, availability);
    node.add_adapter(AdapterInstanceDescriptor {
        id: adapter_id,
        adapter_type: dto.adapter_type,
        display_name: format!("{} Adapter", dto.display_name),
        deployment_mode: AdapterDeploymentMode::InProcess,
        version: dto.adapter_version,
        state,
        health,
        owned_capabilities: BTreeSet::new(),
        supported_route_modes: dto
            .supported_route_modes
            .into_iter()
            .map(route_mode)
            .collect(),
    })?;

    let permission_requirement = permission(dto.permission_requirement);
    let mut ports = std::collections::BTreeMap::new();
    for port in dto.ports {
        require_text(&port.display_name, "port.display_name")?;
        if port.formats.is_empty()
            || port.formats.len() > MAX_FORMATS_PER_PORT
            || port.qos_modes.is_empty()
        {
            return Err(AndroidCatalogProjectionError::InvalidField(
                "port formats or qos_modes",
            ));
        }
        if let Some(clock_domain) = &port.clock_domain {
            require_text(clock_domain, "port.clock_domain")?;
        }
        let mut unique_formats = BTreeSet::new();
        for format in &port.formats {
            require_platform_value(format, "port.formats")?;
            if !unique_formats.insert(format) {
                return Err(AndroidCatalogProjectionError::InvalidField(
                    "duplicate port format",
                ));
            }
        }
        let qos_modes: BTreeSet<_> = port.qos_modes.iter().copied().map(qos).collect();
        if qos_modes.len() != port.qos_modes.len() {
            return Err(AndroidCatalogProjectionError::InvalidField(
                "duplicate port qos mode",
            ));
        }
        let port_id = android_port_id(node_id, &dto.key, &port.key)?;
        let profile = parse_profile(&port.profile)?;
        let descriptor = PortDescriptor {
            id: port_id,
            capability_id,
            display_name: port.display_name,
            direction: direction(port.direction),
            profile,
            schema_id: None,
            formats: port
                .formats
                .into_iter()
                .map(FormatDescriptor::new)
                .collect(),
            qos_modes,
            clock_domain: port.clock_domain,
            availability,
            permission_requirement,
            interoperability_mode: interoperability(port.interoperability),
        };
        if ports.insert(port_id, descriptor).is_some() {
            return Err(AndroidCatalogProjectionError::InvalidField(
                "duplicate port key",
            ));
        }
    }
    let mut metadata = std::collections::BTreeMap::new();
    if let Some(selection) = dto.speaker_selection {
        metadata.insert("speaker.profile".into(), selection.profile);
        metadata.insert(
            "speaker.selectionRevision".into(),
            selection.revision.to_string(),
        );
    }
    metadata.insert("android.platform_key".to_owned(), dto.key);
    metadata.insert(
        "android.adapter_attached".to_owned(),
        dto.adapter_attached.to_string(),
    );
    node.add_capability(CapabilityDescriptor {
        id: capability_id,
        adapter_instance_id: adapter_id,
        display_name: dto.display_name,
        class: capability_class(dto.class),
        availability,
        permission_requirement,
        metadata,
        ports,
    })?;
    Ok(())
}

fn parse_profile(value: &str) -> Result<ProfileId, AndroidCatalogProjectionError> {
    let (name, major) = value
        .rsplit_once('/')
        .ok_or(AndroidCatalogProjectionError::InvalidField("port.profile"))?;
    if name.is_empty() || major.starts_with('0') {
        return Err(AndroidCatalogProjectionError::InvalidField("port.profile"));
    }
    let major = major
        .parse::<u16>()
        .map_err(|_| AndroidCatalogProjectionError::InvalidField("port.profile"))?;
    let profile = ProfileId::new(name, major);
    profile.validate()?;
    Ok(profile)
}

fn require_text(value: &str, field: &'static str) -> Result<(), AndroidCatalogProjectionError> {
    if value.trim().is_empty() || value.len() > 128 {
        return Err(AndroidCatalogProjectionError::InvalidField(field));
    }
    Ok(())
}

fn require_platform_value(
    value: &str,
    field: &'static str,
) -> Result<(), AndroidCatalogProjectionError> {
    if value.is_empty()
        || value.len() > 96
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
    {
        return Err(AndroidCatalogProjectionError::InvalidField(field));
    }
    Ok(())
}

fn adapter_state(attached: bool, availability: Availability) -> (AdapterState, AdapterHealth) {
    if !attached {
        (AdapterState::Stopped, AdapterHealth::Unknown)
    } else if availability == Availability::Offline {
        (AdapterState::Degraded, AdapterHealth::Unhealthy)
    } else {
        (AdapterState::Ready, AdapterHealth::Healthy)
    }
}

fn capability_class(value: AndroidCapabilityClassV1) -> CapabilityClass {
    match value {
        AndroidCapabilityClassV1::Microphone => CapabilityClass::Microphone,
        AndroidCapabilityClassV1::Speaker => CapabilityClass::Speaker,
        AndroidCapabilityClassV1::Camera => CapabilityClass::Camera,
        AndroidCapabilityClassV1::Imu => CapabilityClass::Imu,
        AndroidCapabilityClassV1::Gamepad => CapabilityClass::Gamepad,
        AndroidCapabilityClassV1::Haptics => CapabilityClass::Haptics,
        AndroidCapabilityClassV1::Touchpad => CapabilityClass::Touchpad,
    }
}

fn availability(value: AndroidCapabilityAvailabilityV1) -> Availability {
    match value {
        AndroidCapabilityAvailabilityV1::Available => Availability::Available,
        AndroidCapabilityAvailabilityV1::Busy => Availability::Busy,
        AndroidCapabilityAvailabilityV1::PermissionRequired => Availability::PermissionRequired,
        AndroidCapabilityAvailabilityV1::Offline => Availability::Offline,
    }
}

fn permission(value: AndroidPermissionRequirementV1) -> PermissionRequirement {
    match value {
        AndroidPermissionRequirementV1::None => PermissionRequirement::None,
        AndroidPermissionRequirementV1::UserConfirmation => PermissionRequirement::UserConfirmation,
        AndroidPermissionRequirementV1::ForegroundService => {
            PermissionRequirement::ForegroundService
        }
        AndroidPermissionRequirementV1::Privileged => PermissionRequirement::Privileged,
    }
}

fn direction(value: AndroidPortDirectionV1) -> PortDirection {
    match value {
        AndroidPortDirectionV1::Source => PortDirection::Source,
        AndroidPortDirectionV1::Sink => PortDirection::Sink,
        AndroidPortDirectionV1::Control => PortDirection::Control,
    }
}

fn qos(value: AndroidQosModeV1) -> QosMode {
    match value {
        AndroidQosModeV1::Basic => QosMode::Basic,
        AndroidQosModeV1::Interactive => QosMode::Interactive,
        AndroidQosModeV1::Measurement => QosMode::Measurement,
    }
}

fn interoperability(value: AndroidInteroperabilityModeV1) -> InteroperabilityMode {
    match value {
        AndroidInteroperabilityModeV1::StandardPort => InteroperabilityMode::StandardPort,
        AndroidInteroperabilityModeV1::AdapterManaged => InteroperabilityMode::AdapterManaged,
    }
}

fn route_mode(value: AndroidAdapterRouteModeV1) -> RouteBackend {
    match value {
        AndroidAdapterRouteModeV1::CapyDataPlane => RouteBackend::CapyDataPlane,
        AndroidAdapterRouteModeV1::AdapterManaged => RouteBackend::AdapterManaged,
        AndroidAdapterRouteModeV1::LocalPipeline => RouteBackend::LocalPipeline,
        AndroidAdapterRouteModeV1::ExternalProtocol => RouteBackend::ExternalProtocol,
    }
}
