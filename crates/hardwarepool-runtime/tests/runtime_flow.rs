use std::collections::BTreeSet;

use hardwarepool_core::{
    AudioCapabilitySpec, AudioFormat, AudioProcessingSupport, AudioQosMode, Availability,
    BindingState, CapabilityDescriptor, CapabilityDetails, CapabilityId, CapabilityKind, LocalRole,
    NodeDescriptor, NodeId, NodeRole, PermissionRequirement, Platform, ProfileId, ProjectionKind,
    StreamRole,
};
use hardwarepool_runtime::NodeRuntime;

fn windows_node() -> NodeDescriptor {
    NodeDescriptor::new(
        NodeId::new(),
        "Windows PC",
        Platform::Windows,
        "test",
        [NodeRole::Consumer],
    )
}

fn android_node() -> (NodeDescriptor, CapabilityId, CapabilityId) {
    let microphone_id = CapabilityId::new();
    let speaker_id = CapabilityId::new();
    let mut node = NodeDescriptor::new(
        NodeId::new(),
        "Android phone",
        Platform::Android,
        "test",
        [NodeRole::Provider],
    );

    node.add_capability(CapabilityDescriptor {
        id: microphone_id,
        display_name: "Microphone".to_owned(),
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
            qos_modes: vec![AudioQosMode::VoiceInteractive],
            processing: AudioProcessingSupport::default(),
            supports_volume_control: false,
            supports_mute: true,
        }),
    })
    .expect("microphone");

    node.add_capability(CapabilityDescriptor {
        id: speaker_id,
        display_name: "Speaker".to_owned(),
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
            qos_modes: vec![AudioQosMode::MediaPlayback],
            processing: AudioProcessingSupport::default(),
            supports_volume_control: true,
            supports_mute: true,
        }),
    })
    .expect("speaker");

    (node, microphone_id, speaker_id)
}

#[test]
fn runtime_keeps_audio_projections_independent() {
    let (android, microphone_id, speaker_id) = android_node();
    let peer_id = android.id;
    let mut runtime = NodeRuntime::new(windows_node()).expect("runtime");
    runtime.register_peer(android, true).expect("peer");
    let session_id = runtime.open_session(peer_id).expect("session");

    runtime
        .activate_audio_projection(
            session_id,
            microphone_id,
            ProjectionKind::SystemCaptureEndpoint,
            1,
        )
        .expect("mic active");
    runtime
        .activate_audio_projection(
            session_id,
            speaker_id,
            ProjectionKind::SystemRenderEndpoint,
            2,
        )
        .expect("speaker active");
    runtime
        .deactivate_projection(session_id, microphone_id)
        .expect("stop mic");

    let session = runtime.session(session_id).expect("session");
    assert_eq!(
        session.binding(microphone_id).expect("mic").state,
        BindingState::Stopped
    );
    assert_eq!(
        session.binding(speaker_id).expect("speaker").state,
        BindingState::Active
    );
}

#[test]
fn peer_loss_marks_active_binding_offline() {
    let (android, _, speaker_id) = android_node();
    let peer_id = android.id;
    let mut runtime = NodeRuntime::new(windows_node()).expect("runtime");
    runtime.register_peer(android, true).expect("peer");
    let session_id = runtime.open_session(peer_id).expect("session");
    runtime
        .activate_audio_projection(
            session_id,
            speaker_id,
            ProjectionKind::SystemRenderEndpoint,
            1,
        )
        .expect("active");

    runtime.set_peer_online(peer_id, false).expect("offline");
    assert_eq!(
        runtime
            .session(session_id)
            .expect("session")
            .binding(speaker_id)
            .expect("speaker")
            .state,
        BindingState::Offline
    );
}
