use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use serde::{Deserialize, Serialize};

use crate::{AdapterInstanceId, CapabilityId, CoreError, NodeId, PortId, RouteBackend};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Windows,
    Android,
    Linux,
    Macos,
    Ios,
    Embedded,
    Unknown(String),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

impl ProtocolVersion {
    #[must_use]
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    pub fn validate(self) -> Result<(), CoreError> {
        if self.major == 0 {
            return Err(CoreError::InvalidNode(
                "protocol major must be greater than zero".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnlineState {
    Online,
    Offline,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterDeploymentMode {
    InProcess,
    Sidecar,
    ExternalService,
    DriverBacked,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterState {
    Discovered,
    Initializing,
    Ready,
    Degraded,
    Failed,
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterHealth {
    Unknown,
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterInstanceDescriptor {
    pub id: AdapterInstanceId,
    pub adapter_type: String,
    pub display_name: String,
    pub deployment_mode: AdapterDeploymentMode,
    pub version: String,
    pub state: AdapterState,
    pub health: AdapterHealth,
    pub owned_capabilities: BTreeSet<CapabilityId>,
    pub supported_route_modes: BTreeSet<RouteBackend>,
}

impl AdapterInstanceDescriptor {
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.adapter_type.trim().is_empty() {
            return Err(CoreError::InvalidAdapter {
                adapter_id: self.id,
                reason: "adapter type cannot be empty".to_owned(),
            });
        }
        if self.display_name.trim().is_empty() || self.version.trim().is_empty() {
            return Err(CoreError::InvalidAdapter {
                adapter_id: self.id,
                reason: "display name and version cannot be empty".to_owned(),
            });
        }
        if self.supported_route_modes.is_empty() {
            return Err(CoreError::InvalidAdapter {
                adapter_id: self.id,
                reason: "at least one Route backend must be declared".to_owned(),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityClass {
    Microphone,
    Speaker,
    Camera,
    Display,
    Keyboard,
    Pointer,
    Touchscreen,
    Gamepad,
    Imu,
    Gnss,
    SensorSuite,
    Haptics,
    Recorder,
    Panel,
    Bridge,
    Custom(String),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    Busy,
    PermissionRequired,
    Offline,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionRequirement {
    None,
    UserConfirmation,
    ForegroundService,
    Privileged,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortDirection {
    Source,
    Sink,
    Control,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ProfileId {
    pub name: String,
    pub major: u16,
}

impl ProfileId {
    #[must_use]
    pub fn new(name: impl Into<String>, major: u16) -> Self {
        Self {
            name: name.into(),
            major,
        }
    }

    #[must_use]
    pub fn audio_frames_v1() -> Self {
        Self::new("capyio.audio.frames", 1)
    }

    #[must_use]
    pub fn video_frames_v1() -> Self {
        Self::new("capyio.video.frames", 1)
    }

    #[must_use]
    pub fn imu_samples_v1() -> Self {
        Self::new("capyio.motion.imu-samples", 1)
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        if self.name.trim().is_empty() || self.major == 0 {
            return Err(CoreError::InvalidProfile(format!(
                "profile must have a non-empty name and positive major: {}/{}",
                self.name, self.major
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct FormatDescriptor {
    pub id: String,
    pub parameters: BTreeMap<String, String>,
}

impl FormatDescriptor {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            parameters: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        if self.id.trim().is_empty() {
            return Err(CoreError::InvalidFormat(
                "format descriptor ID cannot be empty".to_owned(),
            ));
        }
        if self.parameters.keys().any(|key| key.trim().is_empty()) {
            return Err(CoreError::InvalidFormat(
                "format parameter keys cannot be empty".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QosMode {
    Basic,
    Interactive,
    Measurement,
    Custom(String),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteroperabilityMode {
    StandardPort,
    AdapterManaged,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PortDescriptor {
    pub id: PortId,
    pub capability_id: CapabilityId,
    pub display_name: String,
    pub direction: PortDirection,
    pub profile: ProfileId,
    pub schema_id: Option<String>,
    pub formats: Vec<FormatDescriptor>,
    pub qos_modes: BTreeSet<QosMode>,
    pub clock_domain: Option<String>,
    pub availability: Availability,
    pub permission_requirement: PermissionRequirement,
    pub interoperability_mode: InteroperabilityMode,
}

impl PortDescriptor {
    pub fn validate(&self) -> Result<(), CoreError> {
        self.profile.validate()?;
        if self.display_name.trim().is_empty() {
            return Err(CoreError::InvalidPort {
                port_id: self.id,
                reason: "display name cannot be empty".to_owned(),
            });
        }
        if self
            .schema_id
            .as_ref()
            .is_some_and(|schema| schema.trim().is_empty())
        {
            return Err(CoreError::InvalidPort {
                port_id: self.id,
                reason: "schema ID cannot be empty when present".to_owned(),
            });
        }
        for format in &self.formats {
            format.validate()?;
        }
        if self.qos_modes.is_empty() {
            return Err(CoreError::InvalidPort {
                port_id: self.id,
                reason: "at least one QoS mode must be declared".to_owned(),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilityDescriptor {
    pub id: CapabilityId,
    pub adapter_instance_id: AdapterInstanceId,
    pub display_name: String,
    pub class: CapabilityClass,
    pub availability: Availability,
    pub permission_requirement: PermissionRequirement,
    pub metadata: BTreeMap<String, String>,
    pub ports: BTreeMap<PortId, PortDescriptor>,
}

impl CapabilityDescriptor {
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.display_name.trim().is_empty() {
            return Err(CoreError::InvalidCapability {
                capability_id: self.id,
                reason: "display name cannot be empty".to_owned(),
            });
        }
        if self.ports.is_empty() {
            return Err(CoreError::InvalidCapability {
                capability_id: self.id,
                reason: "at least one Port is required".to_owned(),
            });
        }
        for (id, port) in &self.ports {
            if *id != port.id || port.capability_id != self.id {
                return Err(CoreError::InvalidCapability {
                    capability_id: self.id,
                    reason: "Port map key/owner does not match descriptor".to_owned(),
                });
            }
            port.validate()?;
        }
        Ok(())
    }

    pub fn add_port(&mut self, port: PortDescriptor) -> Result<(), CoreError> {
        if port.capability_id != self.id {
            return Err(CoreError::InvalidPort {
                port_id: port.id,
                reason: "Port capability ID does not match owner".to_owned(),
            });
        }
        port.validate()?;
        match self.ports.entry(port.id) {
            Entry::Occupied(_) => Err(CoreError::DuplicatePort(port.id)),
            Entry::Vacant(entry) => {
                entry.insert(port);
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeDescriptor {
    pub id: NodeId,
    pub display_name: String,
    pub platform: Platform,
    pub platform_version: String,
    pub runtime_version: String,
    pub protocol_versions: BTreeSet<ProtocolVersion>,
    pub online_state: OnlineState,
    pub adapter_instances: BTreeMap<AdapterInstanceId, AdapterInstanceDescriptor>,
    pub capabilities: BTreeMap<CapabilityId, CapabilityDescriptor>,
}

impl NodeDescriptor {
    #[must_use]
    pub fn new(
        id: NodeId,
        display_name: impl Into<String>,
        platform: Platform,
        platform_version: impl Into<String>,
        runtime_version: impl Into<String>,
        protocol_versions: impl IntoIterator<Item = ProtocolVersion>,
    ) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            platform,
            platform_version: platform_version.into(),
            runtime_version: runtime_version.into(),
            protocol_versions: protocol_versions.into_iter().collect(),
            online_state: OnlineState::Online,
            adapter_instances: BTreeMap::new(),
            capabilities: BTreeMap::new(),
        }
    }

    pub fn add_adapter(&mut self, adapter: AdapterInstanceDescriptor) -> Result<(), CoreError> {
        adapter.validate()?;
        if !adapter.owned_capabilities.is_empty() {
            return Err(CoreError::InvalidAdapter {
                adapter_id: adapter.id,
                reason: "new Adapter must be registered before its Capabilities".to_owned(),
            });
        }
        match self.adapter_instances.entry(adapter.id) {
            Entry::Occupied(_) => Err(CoreError::DuplicateAdapter(adapter.id)),
            Entry::Vacant(entry) => {
                entry.insert(adapter);
                Ok(())
            }
        }
    }

    pub fn add_capability(&mut self, capability: CapabilityDescriptor) -> Result<(), CoreError> {
        capability.validate()?;
        if self.capabilities.contains_key(&capability.id) {
            return Err(CoreError::DuplicateCapability(capability.id));
        }
        let adapter = self
            .adapter_instances
            .get_mut(&capability.adapter_instance_id)
            .ok_or(CoreError::UnknownAdapter(capability.adapter_instance_id))?;
        adapter.owned_capabilities.insert(capability.id);
        self.capabilities.insert(capability.id, capability);
        Ok(())
    }

    pub fn replace_adapter_catalog(
        &mut self,
        adapter_id: AdapterInstanceId,
        capabilities: Vec<CapabilityDescriptor>,
    ) -> Result<(), CoreError> {
        let adapter = self
            .adapter_instances
            .get(&adapter_id)
            .ok_or(CoreError::UnknownAdapter(adapter_id))?;
        let old_ids = adapter.owned_capabilities.clone();

        let mut new_ids = BTreeSet::new();
        for capability in &capabilities {
            capability.validate()?;
            if capability.adapter_instance_id != adapter_id {
                return Err(CoreError::InvalidCapability {
                    capability_id: capability.id,
                    reason: "replacement Capability belongs to another Adapter".to_owned(),
                });
            }
            if !new_ids.insert(capability.id)
                || (self.capabilities.contains_key(&capability.id)
                    && !old_ids.contains(&capability.id))
            {
                return Err(CoreError::DuplicateCapability(capability.id));
            }
        }

        for id in old_ids {
            self.capabilities.remove(&id);
        }
        let adapter = self
            .adapter_instances
            .get_mut(&adapter_id)
            .expect("Adapter existence checked above");
        adapter.owned_capabilities.clear();
        for capability in capabilities {
            adapter.owned_capabilities.insert(capability.id);
            self.capabilities.insert(capability.id, capability);
        }
        Ok(())
    }

    pub fn port(
        &self,
        capability_id: CapabilityId,
        port_id: PortId,
    ) -> Result<&PortDescriptor, CoreError> {
        self.capabilities
            .get(&capability_id)
            .ok_or(CoreError::UnknownCapability(capability_id))?
            .ports
            .get(&port_id)
            .ok_or(CoreError::UnknownPort(port_id))
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        if self.display_name.trim().is_empty()
            || self.platform_version.trim().is_empty()
            || self.runtime_version.trim().is_empty()
        {
            return Err(CoreError::InvalidNode(
                "display/platform/runtime versions cannot be empty".to_owned(),
            ));
        }
        if self.protocol_versions.is_empty() {
            return Err(CoreError::InvalidNode(
                "at least one protocol version is required".to_owned(),
            ));
        }
        for version in &self.protocol_versions {
            version.validate()?;
        }
        for (id, adapter) in &self.adapter_instances {
            if *id != adapter.id {
                return Err(CoreError::InvalidAdapter {
                    adapter_id: adapter.id,
                    reason: "Adapter map key does not match descriptor".to_owned(),
                });
            }
            adapter.validate()?;
            for capability_id in &adapter.owned_capabilities {
                let capability = self
                    .capabilities
                    .get(capability_id)
                    .ok_or(CoreError::UnknownCapability(*capability_id))?;
                if capability.adapter_instance_id != adapter.id {
                    return Err(CoreError::InvalidCapability {
                        capability_id: capability.id,
                        reason: "Capability owner and Adapter catalog disagree".to_owned(),
                    });
                }
            }
        }
        for (id, capability) in &self.capabilities {
            if *id != capability.id {
                return Err(CoreError::InvalidCapability {
                    capability_id: capability.id,
                    reason: "Capability map key does not match descriptor".to_owned(),
                });
            }
            capability.validate()?;
            let adapter = self
                .adapter_instances
                .get(&capability.adapter_instance_id)
                .ok_or(CoreError::UnknownAdapter(capability.adapter_instance_id))?;
            if !adapter.owned_capabilities.contains(&capability.id) {
                return Err(CoreError::InvalidCapability {
                    capability_id: capability.id,
                    reason: "owning Adapter does not list Capability".to_owned(),
                });
            }
        }
        Ok(())
    }
}
