use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::{ADAPTER_CONTROL_PROTOCOL_MAJOR, ADAPTER_MANIFEST_SCHEMA_VERSION};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterKind {
    Physical,
    Projection,
    Connection,
    Composite,
    Export,
    Panel,
    Mock,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentMode {
    InProcess,
    Sidecar,
    ExternalService,
    DriverBacked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalServiceProbeKind {
    TcpConnect,
    HttpGet,
    LocalSocket,
    NamedPipe,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalServiceConnectionKind {
    Tcp,
    Http,
    #[serde(rename = "websocket")]
    WebSocket,
    LocalSocket,
    NamedPipe,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortTemplate {
    pub name: String,
    pub direction: String,
    pub profile: String,
    pub profile_major: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityTemplate {
    pub name: String,
    pub class: String,
    pub ports: Vec<PortTemplate>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LicenseMetadata {
    pub spdx: String,
    pub notice: String,
}

/// Platform-specific bindings for every deployment mode declared by a manifest.
///
/// A mode must have exactly one matching section here. Platform maps are kept
/// separate so one Adapter can, for example, be in-process on Android and a
/// Sidecar on desktop without pretending both mechanisms exist everywhere.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeploymentBindings {
    #[serde(
        default,
        deserialize_with = "deserialize_present_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub in_process: Option<InProcessDeployment>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub sidecar: Option<SidecarDeployment>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub external_service: Option<ExternalServiceDeployment>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub driver_backed: Option<DriverBackedDeployment>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InProcessDeployment {
    pub bindings: BTreeMap<String, InProcessPlatformBinding>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InProcessPlatformBinding {
    #[serde(
        default,
        deserialize_with = "deserialize_present_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub module: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub library: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SidecarDeployment {
    pub entrypoints: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalServiceDeployment {
    pub services: BTreeMap<String, ExternalServicePlatformBinding>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalServicePlatformBinding {
    pub probe: ExternalServiceProbe,
    pub connection: ExternalServiceConnection,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalServiceProbe {
    pub kind: ExternalServiceProbeKind,
    pub target: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalServiceConnection {
    pub kind: ExternalServiceConnectionKind,
    pub endpoint: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriverBackedDeployment {
    pub bindings: BTreeMap<String, DriverBackedPlatformBinding>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriverBackedPlatformBinding {
    pub controller: UserModeControllerBinding,
    pub driver_dependency: DriverDependencyMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserModeControllerBinding {
    /// Opaque executable path launched directly by the platform host, never by
    /// a command shell.
    pub entrypoint: String,
    pub control_interface: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriverDependencyMetadata {
    pub identifier: String,
    pub version_requirement: String,
    pub interface: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterManifest {
    pub schema_version: u16,
    pub id: String,
    pub name: String,
    pub version: String,
    pub control_protocol: ControlProtocolVersion,
    pub kind: AdapterKind,
    #[serde(deserialize_with = "deserialize_unique_set")]
    pub deployment_modes: BTreeSet<DeploymentMode>,
    #[serde(deserialize_with = "deserialize_unique_set")]
    pub platforms: BTreeSet<String>,
    pub mode_bindings: DeploymentBindings,
    #[serde(default, deserialize_with = "deserialize_unique_set")]
    pub permissions: BTreeSet<String>,
    pub capability_templates: Vec<CapabilityTemplate>,
    pub integration_mode: String,
    pub license: LicenseMetadata,
    #[serde(default)]
    pub upstream: Option<String>,
}

#[derive(Deserialize)]
struct ManifestVersion {
    schema_version: u16,
}

impl AdapterManifest {
    pub fn from_json(bytes: &[u8]) -> Result<Self, ManifestError> {
        let version: ManifestVersion = serde_json::from_slice(bytes)?;
        if version.schema_version != ADAPTER_MANIFEST_SCHEMA_VERSION {
            return Err(ManifestError::UnsupportedSchemaVersion(
                version.schema_version,
            ));
        }
        let manifest: Self = serde_json::from_slice(bytes)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != ADAPTER_MANIFEST_SCHEMA_VERSION {
            return Err(ManifestError::UnsupportedSchemaVersion(self.schema_version));
        }
        if self.control_protocol.major != ADAPTER_CONTROL_PROTOCOL_MAJOR {
            return Err(ManifestError::UnsupportedControlProtocol(
                self.control_protocol.major,
            ));
        }
        for (field, value) in [
            ("id", self.id.as_str()),
            ("name", self.name.as_str()),
            ("version", self.version.as_str()),
            ("integration_mode", self.integration_mode.as_str()),
            ("license.spdx", self.license.spdx.as_str()),
            ("license.notice", self.license.notice.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(ManifestError::EmptyField(field));
            }
        }
        if !is_valid_adapter_id(&self.id) {
            return Err(ManifestError::InvalidAdapterId);
        }
        if self.deployment_modes.is_empty() {
            return Err(ManifestError::EmptyCollection("deployment_modes"));
        }
        if self.platforms.is_empty() {
            return Err(ManifestError::EmptyCollection("platforms"));
        }
        if let Some(platform) = self
            .platforms
            .iter()
            .find(|platform| platform.trim().is_empty())
        {
            return Err(ManifestError::InvalidPlatform(platform.clone()));
        }
        if self
            .permissions
            .iter()
            .any(|permission| permission.trim().is_empty())
        {
            return Err(ManifestError::EmptyField("permissions[]"));
        }

        self.validate_deployment_bindings()?;

        if self.capability_templates.is_empty() {
            return Err(ManifestError::EmptyCollection("capability_templates"));
        }
        let mut capability_names = BTreeSet::new();
        for capability in &self.capability_templates {
            if !capability_names.insert(capability.name.as_str()) {
                return Err(ManifestError::DuplicateCapability(capability.name.clone()));
            }
            if capability.name.trim().is_empty()
                || capability.class.trim().is_empty()
                || capability.ports.is_empty()
            {
                return Err(ManifestError::InvalidCapability(capability.name.clone()));
            }
            let mut port_names = BTreeSet::new();
            for port in &capability.ports {
                if !port_names.insert(port.name.as_str()) {
                    return Err(ManifestError::DuplicatePort(port.name.clone()));
                }
                if port.name.trim().is_empty()
                    || !matches!(port.direction.as_str(), "source" | "sink" | "control")
                    || port.profile.trim().is_empty()
                    || port.profile_major == 0
                {
                    return Err(ManifestError::InvalidPort(port.name.clone()));
                }
            }
        }
        Ok(())
    }

    fn validate_deployment_bindings(&self) -> Result<(), ManifestError> {
        self.validate_mode_presence(
            DeploymentMode::InProcess,
            self.mode_bindings.in_process.is_some(),
        )?;
        self.validate_mode_presence(
            DeploymentMode::Sidecar,
            self.mode_bindings.sidecar.is_some(),
        )?;
        self.validate_mode_presence(
            DeploymentMode::ExternalService,
            self.mode_bindings.external_service.is_some(),
        )?;
        self.validate_mode_presence(
            DeploymentMode::DriverBacked,
            self.mode_bindings.driver_backed.is_some(),
        )?;

        let mut bound_platforms = BTreeSet::new();

        if let Some(deployment) = &self.mode_bindings.in_process {
            self.validate_binding_platforms(
                DeploymentMode::InProcess,
                deployment.bindings.keys(),
                &mut bound_platforms,
            )?;
            for (platform, binding) in &deployment.bindings {
                let has_module = binding
                    .module
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty());
                let has_library = binding
                    .library
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty());
                if !has_module && !has_library
                    || binding
                        .module
                        .as_deref()
                        .is_some_and(|value| value.trim().is_empty())
                    || binding
                        .library
                        .as_deref()
                        .is_some_and(|value| value.trim().is_empty())
                {
                    return Err(ManifestError::InvalidModeBinding {
                        mode: DeploymentMode::InProcess,
                        platform: platform.clone(),
                        field: "module/library",
                    });
                }
            }
        }

        if let Some(deployment) = &self.mode_bindings.sidecar {
            self.validate_binding_platforms(
                DeploymentMode::Sidecar,
                deployment.entrypoints.keys(),
                &mut bound_platforms,
            )?;
            for (platform, entrypoint) in &deployment.entrypoints {
                validate_binding_value(
                    DeploymentMode::Sidecar,
                    platform,
                    "entrypoint",
                    entrypoint,
                )?;
            }
        }

        if let Some(deployment) = &self.mode_bindings.external_service {
            self.validate_binding_platforms(
                DeploymentMode::ExternalService,
                deployment.services.keys(),
                &mut bound_platforms,
            )?;
            for (platform, service) in &deployment.services {
                validate_binding_value(
                    DeploymentMode::ExternalService,
                    platform,
                    "probe.target",
                    &service.probe.target,
                )?;
                validate_binding_value(
                    DeploymentMode::ExternalService,
                    platform,
                    "connection.endpoint",
                    &service.connection.endpoint,
                )?;
            }
        }

        if let Some(deployment) = &self.mode_bindings.driver_backed {
            self.validate_binding_platforms(
                DeploymentMode::DriverBacked,
                deployment.bindings.keys(),
                &mut bound_platforms,
            )?;
            for (platform, binding) in &deployment.bindings {
                for (field, value) in [
                    (
                        "controller.entrypoint",
                        binding.controller.entrypoint.as_str(),
                    ),
                    (
                        "controller.control_interface",
                        binding.controller.control_interface.as_str(),
                    ),
                    (
                        "driver_dependency.identifier",
                        binding.driver_dependency.identifier.as_str(),
                    ),
                    (
                        "driver_dependency.version_requirement",
                        binding.driver_dependency.version_requirement.as_str(),
                    ),
                    (
                        "driver_dependency.interface",
                        binding.driver_dependency.interface.as_str(),
                    ),
                ] {
                    validate_binding_value(DeploymentMode::DriverBacked, platform, field, value)?;
                }
            }
        }

        if let Some(platform) = self
            .platforms
            .iter()
            .find(|platform| !bound_platforms.contains(*platform))
        {
            return Err(ManifestError::UnboundPlatform(platform.clone()));
        }

        Ok(())
    }

    fn validate_mode_presence(
        &self,
        mode: DeploymentMode,
        binding_present: bool,
    ) -> Result<(), ManifestError> {
        match (self.deployment_modes.contains(&mode), binding_present) {
            (true, false) => Err(ManifestError::MissingModeBinding(mode)),
            (false, true) => Err(ManifestError::UndeclaredModeBinding(mode)),
            _ => Ok(()),
        }
    }

    fn validate_binding_platforms<'a>(
        &self,
        mode: DeploymentMode,
        platforms: impl Iterator<Item = &'a String>,
        bound_platforms: &mut BTreeSet<String>,
    ) -> Result<(), ManifestError> {
        let mut saw_platform = false;
        for platform in platforms {
            saw_platform = true;
            if platform.trim().is_empty() || !self.platforms.contains(platform) {
                return Err(ManifestError::BindingForUndeclaredPlatform {
                    mode,
                    platform: platform.clone(),
                });
            }
            bound_platforms.insert(platform.clone());
        }
        if !saw_platform {
            return Err(ManifestError::EmptyModeBinding(mode));
        }
        Ok(())
    }
}

fn validate_binding_value(
    mode: DeploymentMode,
    platform: &str,
    field: &'static str,
    value: &str,
) -> Result<(), ManifestError> {
    if value.trim().is_empty()
        || value
            .chars()
            .any(|character| matches!(character, '\0' | '\r' | '\n'))
    {
        return Err(ManifestError::InvalidModeBinding {
            mode,
            platform: platform.to_owned(),
            field,
        });
    }
    Ok(())
}

fn deserialize_present_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

fn deserialize_unique_set<'de, D, T>(deserializer: D) -> Result<BTreeSet<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Ord,
{
    let values = Vec::<T>::deserialize(deserializer)?;
    let value_count = values.len();
    let values = values.into_iter().collect::<BTreeSet<_>>();
    if values.len() != value_count {
        return Err(serde::de::Error::custom(
            "manifest arrays declared unique cannot contain duplicate values",
        ));
    }
    Ok(values)
}

fn is_valid_adapter_id(value: &str) -> bool {
    let mut saw_separator = false;
    let mut segment_has_character = false;

    for character in value.chars() {
        match character {
            '.' | '-' => {
                if !segment_has_character {
                    return false;
                }
                saw_separator = true;
                segment_has_character = false;
            }
            'a'..='z' | '0'..='9' => segment_has_character = true,
            _ => return false,
        }
    }

    saw_separator && segment_has_character
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("manifest JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("manifest schema version {0} is unsupported")]
    UnsupportedSchemaVersion(u16),
    #[error("Adapter control protocol major {0} is unsupported")]
    UnsupportedControlProtocol(u16),
    #[error("manifest field {0} cannot be empty")]
    EmptyField(&'static str),
    #[error("manifest collection {0} cannot be empty")]
    EmptyCollection(&'static str),
    #[error("Adapter ID must be a lowercase reverse-style identifier")]
    InvalidAdapterId,
    #[error("manifest platform {0:?} is invalid")]
    InvalidPlatform(String),
    #[error("declared deployment mode {0:?} is missing its binding")]
    MissingModeBinding(DeploymentMode),
    #[error("deployment binding {0:?} exists without declaring the mode")]
    UndeclaredModeBinding(DeploymentMode),
    #[error("deployment binding {0:?} must declare at least one platform")]
    EmptyModeBinding(DeploymentMode),
    #[error("deployment binding {mode:?} references undeclared platform {platform}")]
    BindingForUndeclaredPlatform {
        mode: DeploymentMode,
        platform: String,
    },
    #[error("manifest platform {0} has no deployment binding")]
    UnboundPlatform(String),
    #[error("deployment binding {mode:?} for {platform} has invalid {field}")]
    InvalidModeBinding {
        mode: DeploymentMode,
        platform: String,
        field: &'static str,
    },
    #[error("duplicate Capability template {0}")]
    DuplicateCapability(String),
    #[error("invalid Capability template {0}")]
    InvalidCapability(String),
    #[error("duplicate Port template {0}")]
    DuplicatePort(String),
    #[error("invalid Port template {0}")]
    InvalidPort(String),
}
