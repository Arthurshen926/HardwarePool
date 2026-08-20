use std::{collections::BTreeSet, str::FromStr};

use hardwarepool_core as core;

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
        let major = u16::try_from(value.major).map_err(|_| ProtocolError::NumericRange {
            field: "profile.major",
            value: u64::from(value.major),
        })?;
        let profile = core::ProfileId::new(value.name, major);
        profile.validate()?;
        Ok(profile)
    }
}

impl From<&core::AudioFormat> for pb::AudioFormat {
    fn from(value: &core::AudioFormat) -> Self {
        Self {
            sample_rate_hz: value.sample_rate_hz,
            sample_format: sample_format_to_wire(value.sample_format),
            channels: u32::from(value.channels),
            channel_layout: channel_layout_to_wire(value.channel_layout),
            frame_duration_micros: value.frame_duration_micros,
        }
    }
}

impl TryFrom<pb::AudioFormat> for core::AudioFormat {
    type Error = ProtocolError;

    fn try_from(value: pb::AudioFormat) -> Result<Self, Self::Error> {
        let channels = u16::try_from(value.channels).map_err(|_| ProtocolError::NumericRange {
            field: "audio_format.channels",
            value: u64::from(value.channels),
        })?;
        let format = core::AudioFormat {
            sample_rate_hz: value.sample_rate_hz,
            sample_format: sample_format_from_wire(value.sample_format)?,
            channels,
            channel_layout: channel_layout_from_wire(value.channel_layout)?,
            frame_duration_micros: value.frame_duration_micros,
        };
        format.validate()?;
        Ok(format)
    }
}

impl From<&core::AudioProcessingSupport> for pb::AudioProcessingSupport {
    fn from(value: &core::AudioProcessingSupport) -> Self {
        Self {
            acoustic_echo_cancellation: value.acoustic_echo_cancellation,
            noise_suppression: value.noise_suppression,
            automatic_gain_control: value.automatic_gain_control,
            raw_capture: value.raw_capture,
        }
    }
}

impl From<pb::AudioProcessingSupport> for core::AudioProcessingSupport {
    fn from(value: pb::AudioProcessingSupport) -> Self {
        Self {
            acoustic_echo_cancellation: value.acoustic_echo_cancellation,
            noise_suppression: value.noise_suppression,
            automatic_gain_control: value.automatic_gain_control,
            raw_capture: value.raw_capture,
        }
    }
}

impl TryFrom<&core::CapabilityDescriptor> for pb::CapabilityDescriptor {
    type Error = ProtocolError;

    fn try_from(value: &core::CapabilityDescriptor) -> Result<Self, Self::Error> {
        value.validate()?;
        let (kind, custom_kind) = capability_kind_to_wire(&value.kind);
        let details = match &value.details {
            core::CapabilityDetails::Audio(spec) => Some(
                pb::capability_descriptor::Details::Audio(pb::AudioCapabilitySpec {
                    formats: spec.formats.iter().map(Into::into).collect(),
                    qos_modes: spec
                        .qos_modes
                        .iter()
                        .copied()
                        .map(audio_qos_to_wire)
                        .collect(),
                    processing: Some((&spec.processing).into()),
                    supports_volume_control: spec.supports_volume_control,
                    supports_mute: spec.supports_mute,
                }),
            ),
            core::CapabilityDetails::AudioDuplexBundle(spec) => Some(
                pb::capability_descriptor::Details::AudioBundle(pb::AudioBundleSpec {
                    capture_capability_id: spec.capture_capability_id.to_string(),
                    render_capability_id: spec.render_capability_id.to_string(),
                    shared_acoustic_environment: spec.shared_acoustic_environment,
                }),
            ),
            core::CapabilityDetails::Opaque(spec) => Some(
                pb::capability_descriptor::Details::Opaque(pb::OpaqueCapabilitySpec {
                    schema_uri: spec.schema_uri.clone(),
                    metadata: spec.metadata.clone().into_iter().collect(),
                }),
            ),
        };

        Ok(Self {
            id: value.id.to_string(),
            display_name: value.display_name.clone(),
            profile: Some((&value.profile).into()),
            kind,
            custom_kind,
            local_role: local_role_to_wire(value.local_role),
            stream_role: stream_role_to_wire(value.stream_role),
            supported_projections: value
                .supported_projections
                .iter()
                .copied()
                .map(projection_to_wire)
                .collect(),
            permission_requirement: permission_to_wire(value.permission_requirement),
            availability: availability_to_wire(value.availability),
            details,
        })
    }
}

impl TryFrom<pb::CapabilityDescriptor> for core::CapabilityDescriptor {
    type Error = ProtocolError;

    fn try_from(value: pb::CapabilityDescriptor) -> Result<Self, Self::Error> {
        let id = parse_id::<core::CapabilityId>("capability.id", &value.id)?;
        let profile = value
            .profile
            .ok_or(ProtocolError::MissingField("capability.profile"))?
            .try_into()?;
        let kind = capability_kind_from_wire(value.kind, value.custom_kind)?;
        let projections = value
            .supported_projections
            .into_iter()
            .map(projection_from_wire)
            .collect::<Result<BTreeSet<_>, _>>()?;
        let details = match value
            .details
            .ok_or(ProtocolError::MissingField("capability.details"))?
        {
            pb::capability_descriptor::Details::Audio(spec) => {
                let formats = spec
                    .formats
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?;
                let qos_modes = spec
                    .qos_modes
                    .into_iter()
                    .map(audio_qos_from_wire)
                    .collect::<Result<Vec<_>, _>>()?;
                core::CapabilityDetails::Audio(core::AudioCapabilitySpec {
                    formats,
                    qos_modes,
                    processing: spec.processing.map(Into::into).unwrap_or_default(),
                    supports_volume_control: spec.supports_volume_control,
                    supports_mute: spec.supports_mute,
                })
            }
            pb::capability_descriptor::Details::AudioBundle(spec) => {
                core::CapabilityDetails::AudioDuplexBundle(core::AudioBundleSpec {
                    capture_capability_id: parse_id(
                        "audio_bundle.capture_capability_id",
                        &spec.capture_capability_id,
                    )?,
                    render_capability_id: parse_id(
                        "audio_bundle.render_capability_id",
                        &spec.render_capability_id,
                    )?,
                    shared_acoustic_environment: spec.shared_acoustic_environment,
                })
            }
            pb::capability_descriptor::Details::Opaque(spec) => {
                core::CapabilityDetails::Opaque(core::OpaqueCapabilitySpec {
                    schema_uri: spec.schema_uri,
                    metadata: spec.metadata.into_iter().collect(),
                })
            }
        };

        let capability = core::CapabilityDescriptor {
            id,
            display_name: value.display_name,
            profile,
            kind,
            local_role: local_role_from_wire(value.local_role)?,
            stream_role: stream_role_from_wire(value.stream_role)?,
            supported_projections: projections,
            permission_requirement: permission_from_wire(value.permission_requirement)?,
            availability: availability_from_wire(value.availability)?,
            details,
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
        let capabilities = value
            .capabilities
            .values()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            id: value.id.to_string(),
            display_name: value.display_name.clone(),
            platform,
            platform_detail,
            platform_version: value.platform_version.clone(),
            roles: value.roles.iter().copied().map(node_role_to_wire).collect(),
            capabilities,
        })
    }
}

impl TryFrom<pb::NodeDescriptor> for core::NodeDescriptor {
    type Error = ProtocolError;

    fn try_from(value: pb::NodeDescriptor) -> Result<Self, Self::Error> {
        let id = parse_id::<core::NodeId>("node.id", &value.id)?;
        let roles = value
            .roles
            .into_iter()
            .map(node_role_from_wire)
            .collect::<Result<BTreeSet<_>, _>>()?;
        let mut node = core::NodeDescriptor::new(
            id,
            value.display_name,
            platform_from_wire(value.platform, value.platform_detail)?,
            value.platform_version,
            roles,
        );
        for capability in value.capabilities {
            node.add_capability(capability.try_into()?)?;
        }
        node.validate()?;
        Ok(node)
    }
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
        100 => Ok(core::Platform::Unknown(detail)),
        _ => Err(invalid_enum("node.platform", value)),
    }
}

fn node_role_to_wire(value: core::NodeRole) -> i32 {
    match value {
        core::NodeRole::Provider => 1,
        core::NodeRole::Consumer => 2,
        core::NodeRole::Duplex => 3,
        core::NodeRole::Lightweight => 4,
    }
}

fn node_role_from_wire(value: i32) -> Result<core::NodeRole, ProtocolError> {
    match value {
        1 => Ok(core::NodeRole::Provider),
        2 => Ok(core::NodeRole::Consumer),
        3 => Ok(core::NodeRole::Duplex),
        4 => Ok(core::NodeRole::Lightweight),
        _ => Err(invalid_enum("node.roles", value)),
    }
}

fn capability_kind_to_wire(value: &core::CapabilityKind) -> (i32, String) {
    match value {
        core::CapabilityKind::AudioCapture => (1, String::new()),
        core::CapabilityKind::AudioRender => (2, String::new()),
        core::CapabilityKind::AudioDuplexBundle => (3, String::new()),
        core::CapabilityKind::Custom(name) => (100, name.clone()),
    }
}

fn capability_kind_from_wire(
    value: i32,
    custom_kind: String,
) -> Result<core::CapabilityKind, ProtocolError> {
    match value {
        1 => Ok(core::CapabilityKind::AudioCapture),
        2 => Ok(core::CapabilityKind::AudioRender),
        3 => Ok(core::CapabilityKind::AudioDuplexBundle),
        100 if !custom_kind.trim().is_empty() => Ok(core::CapabilityKind::Custom(custom_kind)),
        100 => Err(ProtocolError::MissingField("capability.custom_kind")),
        _ => Err(invalid_enum("capability.kind", value)),
    }
}

fn local_role_to_wire(value: core::LocalRole) -> i32 {
    match value {
        core::LocalRole::Capture => 1,
        core::LocalRole::Render => 2,
        core::LocalRole::Control => 3,
        core::LocalRole::Compute => 4,
        core::LocalRole::Composite => 5,
    }
}

fn local_role_from_wire(value: i32) -> Result<core::LocalRole, ProtocolError> {
    match value {
        1 => Ok(core::LocalRole::Capture),
        2 => Ok(core::LocalRole::Render),
        3 => Ok(core::LocalRole::Control),
        4 => Ok(core::LocalRole::Compute),
        5 => Ok(core::LocalRole::Composite),
        _ => Err(invalid_enum("capability.local_role", value)),
    }
}

fn stream_role_to_wire(value: core::StreamRole) -> i32 {
    match value {
        core::StreamRole::Producer => 1,
        core::StreamRole::Consumer => 2,
        core::StreamRole::Duplex => 3,
        core::StreamRole::None => 4,
    }
}

fn stream_role_from_wire(value: i32) -> Result<core::StreamRole, ProtocolError> {
    match value {
        1 => Ok(core::StreamRole::Producer),
        2 => Ok(core::StreamRole::Consumer),
        3 => Ok(core::StreamRole::Duplex),
        4 => Ok(core::StreamRole::None),
        _ => Err(invalid_enum("capability.stream_role", value)),
    }
}

fn projection_to_wire(value: core::ProjectionKind) -> i32 {
    match value {
        core::ProjectionKind::ApplicationStream => 1,
        core::ProjectionKind::SystemCaptureEndpoint => 2,
        core::ProjectionKind::SystemRenderEndpoint => 3,
        core::ProjectionKind::VirtualInputDevice => 4,
        core::ProjectionKind::VirtualDisplay => 5,
        core::ProjectionKind::RemoteComputeService => 6,
    }
}

fn projection_from_wire(value: i32) -> Result<core::ProjectionKind, ProtocolError> {
    match value {
        1 => Ok(core::ProjectionKind::ApplicationStream),
        2 => Ok(core::ProjectionKind::SystemCaptureEndpoint),
        3 => Ok(core::ProjectionKind::SystemRenderEndpoint),
        4 => Ok(core::ProjectionKind::VirtualInputDevice),
        5 => Ok(core::ProjectionKind::VirtualDisplay),
        6 => Ok(core::ProjectionKind::RemoteComputeService),
        _ => Err(invalid_enum("capability.supported_projections", value)),
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
        _ => Err(invalid_enum("capability.permission_requirement", value)),
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
        _ => Err(invalid_enum("capability.availability", value)),
    }
}

fn sample_format_to_wire(value: core::AudioSampleFormat) -> i32 {
    match value {
        core::AudioSampleFormat::SignedI16Le => 1,
        core::AudioSampleFormat::SignedI24Le => 2,
        core::AudioSampleFormat::SignedI32Le => 3,
        core::AudioSampleFormat::FloatF32Le => 4,
    }
}

fn sample_format_from_wire(value: i32) -> Result<core::AudioSampleFormat, ProtocolError> {
    match value {
        1 => Ok(core::AudioSampleFormat::SignedI16Le),
        2 => Ok(core::AudioSampleFormat::SignedI24Le),
        3 => Ok(core::AudioSampleFormat::SignedI32Le),
        4 => Ok(core::AudioSampleFormat::FloatF32Le),
        _ => Err(invalid_enum("audio_format.sample_format", value)),
    }
}

fn channel_layout_to_wire(value: core::ChannelLayout) -> i32 {
    match value {
        core::ChannelLayout::Mono => 1,
        core::ChannelLayout::Stereo => 2,
        core::ChannelLayout::Discrete => 3,
    }
}

fn channel_layout_from_wire(value: i32) -> Result<core::ChannelLayout, ProtocolError> {
    match value {
        1 => Ok(core::ChannelLayout::Mono),
        2 => Ok(core::ChannelLayout::Stereo),
        3 => Ok(core::ChannelLayout::Discrete),
        _ => Err(invalid_enum("audio_format.channel_layout", value)),
    }
}

fn audio_qos_to_wire(value: core::AudioQosMode) -> i32 {
    match value {
        core::AudioQosMode::MediaPlayback => 1,
        core::AudioQosMode::VoiceInteractive => 2,
        core::AudioQosMode::RawLan => 3,
        core::AudioQosMode::RawDuplex => 4,
    }
}

fn audio_qos_from_wire(value: i32) -> Result<core::AudioQosMode, ProtocolError> {
    match value {
        1 => Ok(core::AudioQosMode::MediaPlayback),
        2 => Ok(core::AudioQosMode::VoiceInteractive),
        3 => Ok(core::AudioQosMode::RawLan),
        4 => Ok(core::AudioQosMode::RawDuplex),
        _ => Err(invalid_enum("audio_spec.qos_modes", value)),
    }
}

fn invalid_enum(field: &'static str, value: i32) -> ProtocolError {
    ProtocolError::InvalidEnum { field, value }
}
