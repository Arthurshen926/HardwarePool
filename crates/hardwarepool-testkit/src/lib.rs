#![forbid(unsafe_code)]

//! Deterministic sample devices used by tests, the CLI and the browser/Tauri demo.

use std::collections::BTreeSet;
use std::str::FromStr;

use hardwarepool_core::{
    AudioBundleSpec, AudioCapabilitySpec, AudioFormat, AudioProcessingSupport, AudioQosMode,
    Availability, CapabilityDescriptor, CapabilityDetails, CapabilityId, CapabilityKind, LocalRole,
    NodeDescriptor, NodeId, NodeRole, PermissionRequirement, Platform, ProfileId, ProjectionKind,
    SessionId, StreamRole,
};
use hardwarepool_runtime::{NodeRuntime, RuntimeError};

pub const WINDOWS_NODE_ID: &str = "00000000-0000-4000-8000-000000000001";
pub const ANDROID_NODE_ID: &str = "00000000-0000-4000-8000-000000000002";
pub const MICROPHONE_CAPABILITY_ID: &str = "00000000-0000-4000-8000-000000000101";
pub const SPEAKER_CAPABILITY_ID: &str = "00000000-0000-4000-8000-000000000102";
pub const DUPLEX_BUNDLE_CAPABILITY_ID: &str = "00000000-0000-4000-8000-000000000103";

#[derive(Clone, Debug)]
pub struct DemoLab {
    pub runtime: NodeRuntime,
    pub session_id: SessionId,
    pub android_node_id: NodeId,
    pub microphone_capability_id: CapabilityId,
    pub speaker_capability_id: CapabilityId,
}

impl DemoLab {
    pub fn new() -> Result<Self, RuntimeError> {
        let local = windows_node();
        let android = android_node();
        let android_node_id = android.id;
        let microphone_capability_id = id(MICROPHONE_CAPABILITY_ID);
        let speaker_capability_id = id(SPEAKER_CAPABILITY_ID);

        let mut runtime = NodeRuntime::new(local)?;
        runtime.register_peer(android, true)?;
        let session_id = runtime.open_session(android_node_id)?;

        Ok(Self {
            runtime,
            session_id,
            android_node_id,
            microphone_capability_id,
            speaker_capability_id,
        })
    }

    pub fn set_microphone_active(&mut self, active: bool, now_ms: u64) -> Result<(), RuntimeError> {
        if active {
            self.runtime.activate_audio_projection(
                self.session_id,
                self.microphone_capability_id,
                ProjectionKind::SystemCaptureEndpoint,
                now_ms,
            )
        } else {
            self.runtime
                .deactivate_projection(self.session_id, self.microphone_capability_id)
        }
    }

    pub fn set_speaker_active(&mut self, active: bool, now_ms: u64) -> Result<(), RuntimeError> {
        if active {
            self.runtime.activate_audio_projection(
                self.session_id,
                self.speaker_capability_id,
                ProjectionKind::SystemRenderEndpoint,
                now_ms,
            )
        } else {
            self.runtime
                .deactivate_projection(self.session_id, self.speaker_capability_id)
        }
    }
}

#[must_use]
pub fn windows_node() -> NodeDescriptor {
    NodeDescriptor::new(
        node_id(WINDOWS_NODE_ID),
        "HP OmniBook Ultra Flip 14",
        Platform::Windows,
        "Windows build not inventoried",
        [NodeRole::Consumer, NodeRole::Duplex],
    )
}

#[must_use]
pub fn android_node() -> NodeDescriptor {
    let microphone_id = id(MICROPHONE_CAPABILITY_ID);
    let speaker_id = id(SPEAKER_CAPABILITY_ID);
    let mut node = NodeDescriptor::new(
        node_id(ANDROID_NODE_ID),
        "vivo X200 Pro mini",
        Platform::Android,
        "Android build not inventoried",
        [NodeRole::Provider, NodeRole::Duplex],
    );

    node.add_capability(CapabilityDescriptor {
        id: microphone_id,
        display_name: "Internal Microphone".to_owned(),
        profile: ProfileId::audio_capture_v1(),
        kind: CapabilityKind::AudioCapture,
        local_role: LocalRole::Capture,
        stream_role: StreamRole::Producer,
        supported_projections: BTreeSet::from([
            ProjectionKind::ApplicationStream,
            ProjectionKind::SystemCaptureEndpoint,
        ]),
        permission_requirement: PermissionRequirement::ForegroundService,
        availability: Availability::PermissionRequired,
        details: CapabilityDetails::Audio(AudioCapabilitySpec {
            formats: vec![AudioFormat::microphone_baseline()],
            qos_modes: vec![AudioQosMode::VoiceInteractive, AudioQosMode::RawLan],
            processing: AudioProcessingSupport {
                acoustic_echo_cancellation: true,
                noise_suppression: true,
                automatic_gain_control: true,
                raw_capture: true,
            },
            supports_volume_control: false,
            supports_mute: true,
        }),
    })
    .expect("fixture microphone is valid");

    node.add_capability(CapabilityDescriptor {
        id: speaker_id,
        display_name: "Internal Speaker".to_owned(),
        profile: ProfileId::audio_render_v1(),
        kind: CapabilityKind::AudioRender,
        local_role: LocalRole::Render,
        stream_role: StreamRole::Consumer,
        supported_projections: BTreeSet::from([
            ProjectionKind::ApplicationStream,
            ProjectionKind::SystemRenderEndpoint,
        ]),
        permission_requirement: PermissionRequirement::UserConfirmation,
        availability: Availability::Available,
        details: CapabilityDetails::Audio(AudioCapabilitySpec {
            formats: vec![AudioFormat::speaker_baseline()],
            qos_modes: vec![AudioQosMode::MediaPlayback, AudioQosMode::VoiceInteractive],
            processing: AudioProcessingSupport::default(),
            supports_volume_control: true,
            supports_mute: true,
        }),
    })
    .expect("fixture speaker is valid");

    node.add_capability(CapabilityDescriptor {
        id: id(DUPLEX_BUNDLE_CAPABILITY_ID),
        display_name: "Built-in Duplex Audio".to_owned(),
        profile: ProfileId::audio_duplex_bundle_v1(),
        kind: CapabilityKind::AudioDuplexBundle,
        local_role: LocalRole::Composite,
        stream_role: StreamRole::None,
        supported_projections: BTreeSet::new(),
        permission_requirement: PermissionRequirement::UserConfirmation,
        availability: Availability::Available,
        details: CapabilityDetails::AudioDuplexBundle(AudioBundleSpec {
            capture_capability_id: microphone_id,
            render_capability_id: speaker_id,
            shared_acoustic_environment: true,
        }),
    })
    .expect("fixture bundle is valid");

    node
}

fn node_id(value: &str) -> NodeId {
    NodeId::from_str(value).expect("fixture NodeId")
}

fn id(value: &str) -> CapabilityId {
    CapabilityId::from_str(value).expect("fixture CapabilityId")
}

#[cfg(test)]
mod tests {
    use hardwarepool_core::BindingState;

    use super::*;

    #[test]
    fn demo_lab_controls_capabilities_independently() {
        let mut lab = DemoLab::new().expect("demo lab");
        lab.set_speaker_active(true, 1).expect("speaker");
        lab.set_microphone_active(true, 2).expect("microphone");
        lab.set_microphone_active(false, 3)
            .expect("stop microphone");

        let session = lab.runtime.session(lab.session_id).expect("session");
        assert_eq!(
            session
                .binding(lab.speaker_capability_id)
                .expect("speaker")
                .state,
            BindingState::Active
        );
        assert_eq!(
            session
                .binding(lab.microphone_capability_id)
                .expect("microphone")
                .state,
            BindingState::Stopped
        );
    }
}
