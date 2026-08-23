use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
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
pub struct ControlProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PortTemplate {
    pub name: String,
    pub direction: String,
    pub profile: String,
    pub profile_major: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilityTemplate {
    pub name: String,
    pub class: String,
    pub ports: Vec<PortTemplate>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LicenseMetadata {
    pub spdx: String,
    pub notice: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterManifest {
    pub schema_version: u16,
    pub id: String,
    pub name: String,
    pub version: String,
    pub control_protocol: ControlProtocolVersion,
    pub kind: AdapterKind,
    pub deployment_modes: BTreeSet<DeploymentMode>,
    pub platforms: BTreeSet<String>,
    pub entrypoints: BTreeMap<String, String>,
    #[serde(default)]
    pub permissions: BTreeSet<String>,
    pub capability_templates: Vec<CapabilityTemplate>,
    pub integration_mode: String,
    pub license: LicenseMetadata,
    #[serde(default)]
    pub upstream: Option<String>,
}

impl AdapterManifest {
    pub fn from_json(bytes: &[u8]) -> Result<Self, ManifestError> {
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
        if !self.id.contains('.') || self.id.chars().any(char::is_whitespace) {
            return Err(ManifestError::InvalidAdapterId);
        }
        if self.deployment_modes.is_empty() || self.platforms.is_empty() {
            return Err(ManifestError::EmptyCollection("deployment_modes/platforms"));
        }
        if !self.deployment_modes.contains(&DeploymentMode::Sidecar) {
            return Err(ManifestError::MissingSidecarMode);
        }
        if self.entrypoints.is_empty()
            || self
                .entrypoints
                .values()
                .any(|value| value.trim().is_empty())
        {
            return Err(ManifestError::EmptyCollection("entrypoints"));
        }
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
    #[error("Adapter ID must be a non-whitespace reverse-domain identifier")]
    InvalidAdapterId,
    #[error("foundation executable Adapter must declare sidecar deployment")]
    MissingSidecarMode,
    #[error("duplicate Capability template {0}")]
    DuplicateCapability(String),
    #[error("invalid Capability template {0}")]
    InvalidCapability(String),
    #[error("duplicate Port template {0}")]
    DuplicatePort(String),
    #[error("invalid Port template {0}")]
    InvalidPort(String),
}
