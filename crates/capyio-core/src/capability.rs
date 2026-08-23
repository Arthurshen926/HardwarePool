use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use serde::{Deserialize, Serialize};

use crate::{AudioBundleSpec, AudioCapabilitySpec, CapabilityId, CoreError, NodeId};

/// Host operating-system family.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

/// Roles a Node can perform in the capability graph.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeRole {
    Provider,
    Consumer,
    Duplex,
    Lightweight,
}

/// Versioned semantic contract for one class of capability.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    pub fn audio_capture_v1() -> Self {
        Self::new("capyio.audio.capture", 1)
    }

    #[must_use]
    pub fn audio_render_v1() -> Self {
        Self::new("capyio.audio.render", 1)
    }

    #[must_use]
    pub fn audio_duplex_bundle_v1() -> Self {
        Self::new("capyio.audio.duplex_bundle", 1)
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        if self.name.trim().is_empty() {
            return Err(CoreError::InvalidProfile(
                "profile name cannot be empty".to_owned(),
            ));
        }
        if self.major == 0 {
            return Err(CoreError::InvalidProfile(
                "profile major version must be greater than zero".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Broad capability class used for indexing and UI; typed details remain authoritative.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    AudioCapture,
    AudioRender,
    AudioDuplexBundle,
    Custom(String),
}

/// Physical/API role on the provider Node.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalRole {
    Capture,
    Render,
    Control,
    Compute,
    Composite,
}

/// Direction of data relative to the provider Node.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamRole {
    Producer,
    Consumer,
    Duplex,
    None,
}

/// Local representation available on a consumer.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionKind {
    ApplicationStream,
    SystemCaptureEndpoint,
    SystemRenderEndpoint,
    VirtualInputDevice,
    VirtualDisplay,
    RemoteComputeService,
}

/// Provider-side consent/lifecycle requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionRequirement {
    None,
    UserConfirmation,
    ForegroundService,
    Privileged,
}

/// Current static availability advertised by a provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    Busy,
    PermissionRequired,
    Offline,
}

/// Extension payload used when Core does not have a typed Profile model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OpaqueCapabilitySpec {
    pub schema_uri: String,
    pub metadata: BTreeMap<String, String>,
}

/// Typed details attached to a Capability.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum CapabilityDetails {
    Audio(AudioCapabilitySpec),
    AudioDuplexBundle(AudioBundleSpec),
    Opaque(OpaqueCapabilitySpec),
}

/// Machine-readable description of one independently authorized ability.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilityDescriptor {
    pub id: CapabilityId,
    pub display_name: String,
    pub profile: ProfileId,
    pub kind: CapabilityKind,
    pub local_role: LocalRole,
    pub stream_role: StreamRole,
    pub supported_projections: BTreeSet<ProjectionKind>,
    pub permission_requirement: PermissionRequirement,
    pub availability: Availability,
    pub details: CapabilityDetails,
}

impl CapabilityDescriptor {
    pub fn validate(&self) -> Result<(), CoreError> {
        self.profile.validate()?;

        if self.display_name.trim().is_empty() {
            return Err(self.invalid("display name cannot be empty"));
        }

        match (&self.kind, &self.details) {
            (CapabilityKind::AudioCapture, CapabilityDetails::Audio(spec)) => {
                self.validate_profile("capyio.audio.capture")?;
                if self.local_role != LocalRole::Capture || self.stream_role != StreamRole::Producer
                {
                    return Err(self.invalid(
                        "audio capture requires local_role=capture and stream_role=producer",
                    ));
                }
                if !self
                    .supported_projections
                    .contains(&ProjectionKind::ApplicationStream)
                    && !self
                        .supported_projections
                        .contains(&ProjectionKind::SystemCaptureEndpoint)
                {
                    return Err(self.invalid(
                        "audio capture requires an application or system capture projection",
                    ));
                }
                spec.validate()?;
            }
            (CapabilityKind::AudioRender, CapabilityDetails::Audio(spec)) => {
                self.validate_profile("capyio.audio.render")?;
                if self.local_role != LocalRole::Render || self.stream_role != StreamRole::Consumer
                {
                    return Err(self.invalid(
                        "audio render requires local_role=render and stream_role=consumer",
                    ));
                }
                if !self
                    .supported_projections
                    .contains(&ProjectionKind::ApplicationStream)
                    && !self
                        .supported_projections
                        .contains(&ProjectionKind::SystemRenderEndpoint)
                {
                    return Err(self.invalid(
                        "audio render requires an application or system render projection",
                    ));
                }
                spec.validate()?;
            }
            (CapabilityKind::AudioDuplexBundle, CapabilityDetails::AudioDuplexBundle(bundle)) => {
                self.validate_profile("capyio.audio.duplex_bundle")?;
                if self.local_role != LocalRole::Composite || self.stream_role != StreamRole::None {
                    return Err(self.invalid(
                        "audio duplex bundle requires local_role=composite and stream_role=none",
                    ));
                }
                if bundle.capture_capability_id == bundle.render_capability_id {
                    return Err(
                        self.invalid("duplex bundle capture and render members must be different")
                    );
                }
                if !self.supported_projections.is_empty() {
                    return Err(self.invalid(
                        "duplex bundle is relationship metadata and must not advertise projections",
                    ));
                }
            }
            (CapabilityKind::Custom(_), CapabilityDetails::Opaque(spec)) => {
                if spec.schema_uri.trim().is_empty() {
                    return Err(self.invalid("opaque capability requires a schema URI"));
                }
            }
            _ => {
                return Err(self.invalid("capability kind does not match typed details"));
            }
        }

        Ok(())
    }

    #[must_use]
    pub fn supports_projection(&self, projection: ProjectionKind) -> bool {
        self.supported_projections.contains(&projection)
    }

    #[must_use]
    pub fn audio_spec(&self) -> Option<&AudioCapabilitySpec> {
        match &self.details {
            CapabilityDetails::Audio(spec) => Some(spec),
            _ => None,
        }
    }

    fn validate_profile(&self, expected_name: &str) -> Result<(), CoreError> {
        if self.profile.name != expected_name || self.profile.major != 1 {
            return Err(self.invalid(format!(
                "expected profile {expected_name}/1, got {}/{}",
                self.profile.name, self.profile.major
            )));
        }
        Ok(())
    }

    fn invalid(&self, reason: impl Into<String>) -> CoreError {
        CoreError::InvalidCapability {
            capability_id: self.id,
            reason: reason.into(),
        }
    }
}

/// Static description of a CapyIO runtime instance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeDescriptor {
    pub id: NodeId,
    pub display_name: String,
    pub platform: Platform,
    pub platform_version: String,
    pub roles: BTreeSet<NodeRole>,
    pub capabilities: BTreeMap<CapabilityId, CapabilityDescriptor>,
}

impl NodeDescriptor {
    #[must_use]
    pub fn new(
        id: NodeId,
        display_name: impl Into<String>,
        platform: Platform,
        platform_version: impl Into<String>,
        roles: impl IntoIterator<Item = NodeRole>,
    ) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            platform,
            platform_version: platform_version.into(),
            roles: roles.into_iter().collect(),
            capabilities: BTreeMap::new(),
        }
    }

    pub fn add_capability(&mut self, capability: CapabilityDescriptor) -> Result<(), CoreError> {
        capability.validate()?;
        match self.capabilities.entry(capability.id) {
            Entry::Occupied(_) => Err(CoreError::DuplicateCapability(capability.id)),
            Entry::Vacant(entry) => {
                entry.insert(capability);
                Ok(())
            }
        }
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        if self.display_name.trim().is_empty() {
            return Err(CoreError::InvalidProfile(
                "node display name cannot be empty".to_owned(),
            ));
        }
        if self.roles.is_empty() {
            return Err(CoreError::InvalidProfile(
                "node must advertise at least one role".to_owned(),
            ));
        }
        for (id, capability) in &self.capabilities {
            if *id != capability.id {
                return Err(CoreError::InvalidCapability {
                    capability_id: capability.id,
                    reason: "capability map key does not match descriptor ID".to_owned(),
                });
            }
            capability.validate()?;
        }

        for capability in self.capabilities.values() {
            let CapabilityDetails::AudioDuplexBundle(bundle) = &capability.details else {
                continue;
            };
            let capture = self
                .capabilities
                .get(&bundle.capture_capability_id)
                .ok_or_else(|| capability.invalid("duplex bundle capture member is missing"))?;
            let render = self
                .capabilities
                .get(&bundle.render_capability_id)
                .ok_or_else(|| capability.invalid("duplex bundle render member is missing"))?;
            if capture.kind != CapabilityKind::AudioCapture {
                return Err(capability.invalid(
                    "duplex bundle capture member must reference an audio capture capability",
                ));
            }
            if render.kind != CapabilityKind::AudioRender {
                return Err(capability.invalid(
                    "duplex bundle render member must reference an audio render capability",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioFormat, AudioProcessingSupport, AudioQosMode, CapabilityId, NodeId};

    fn capture_capability() -> CapabilityDescriptor {
        CapabilityDescriptor {
            id: CapabilityId::new(),
            display_name: "Internal microphone".to_owned(),
            profile: ProfileId::audio_capture_v1(),
            kind: CapabilityKind::AudioCapture,
            local_role: LocalRole::Capture,
            stream_role: StreamRole::Producer,
            supported_projections: [
                ProjectionKind::ApplicationStream,
                ProjectionKind::SystemCaptureEndpoint,
            ]
            .into_iter()
            .collect(),
            permission_requirement: PermissionRequirement::ForegroundService,
            availability: Availability::PermissionRequired,
            details: CapabilityDetails::Audio(AudioCapabilitySpec {
                formats: vec![AudioFormat::microphone_baseline()],
                qos_modes: vec![AudioQosMode::VoiceInteractive],
                processing: AudioProcessingSupport::default(),
                supports_volume_control: false,
                supports_mute: true,
            }),
        }
    }

    #[test]
    fn capture_role_mismatch_is_rejected() {
        let mut capability = capture_capability();
        capability.local_role = LocalRole::Render;
        assert!(matches!(
            capability.validate(),
            Err(CoreError::InvalidCapability { .. })
        ));
    }

    #[test]
    fn duplicate_capability_id_is_rejected() {
        let mut node = NodeDescriptor::new(
            NodeId::new(),
            "Android phone",
            Platform::Android,
            "test",
            [NodeRole::Provider],
        );
        let capability = capture_capability();
        node.add_capability(capability.clone())
            .expect("first insert");
        assert_eq!(
            node.add_capability(capability.clone()),
            Err(CoreError::DuplicateCapability(capability.id))
        );
    }

    #[test]
    fn duplex_bundle_requires_existing_typed_members() {
        let capture = capture_capability();
        let missing_render = CapabilityId::new();
        let bundle_id = CapabilityId::new();
        let mut node = NodeDescriptor::new(
            NodeId::new(),
            "Android phone",
            Platform::Android,
            "test",
            [NodeRole::Provider],
        );
        node.add_capability(capture.clone()).expect("capture");
        node.add_capability(CapabilityDescriptor {
            id: bundle_id,
            display_name: "Built-in audio".to_owned(),
            profile: ProfileId::audio_duplex_bundle_v1(),
            kind: CapabilityKind::AudioDuplexBundle,
            local_role: LocalRole::Composite,
            stream_role: StreamRole::None,
            supported_projections: BTreeSet::new(),
            permission_requirement: PermissionRequirement::UserConfirmation,
            availability: Availability::Available,
            details: CapabilityDetails::AudioDuplexBundle(AudioBundleSpec {
                capture_capability_id: capture.id,
                render_capability_id: missing_render,
                shared_acoustic_environment: true,
            }),
        })
        .expect("bundle descriptor is locally valid");

        assert!(matches!(
            node.validate(),
            Err(CoreError::InvalidCapability {
                capability_id,
                ..
            }) if capability_id == bundle_id
        ));
    }

    #[test]
    fn valid_node_accepts_capability() {
        let mut node = NodeDescriptor::new(
            NodeId::new(),
            "Android phone",
            Platform::Android,
            "test",
            [NodeRole::Provider],
        );
        node.add_capability(capture_capability())
            .expect("valid capability");
        node.validate().expect("valid node");
    }
}
