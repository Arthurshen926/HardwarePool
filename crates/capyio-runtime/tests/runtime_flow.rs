use std::collections::BTreeSet;

use capyio_core::{
    AudioCapabilitySpec, AudioFormat, AudioProcessingSupport, AudioQosMode, Availability,
    BindingState, CapabilityDescriptor, CapabilityDetails, CapabilityId, CapabilityKind, LocalRole,
    NodeDescriptor, NodeId, NodeRole, PermissionRequirement, Platform, ProfileId, ProjectionKind,
    StreamRole,
};
use capyio_runtime::{
    ActualAudioStreamParameters, HostOperation, HostOperationCompletion, HostOperationOutput,
    NodeRuntime, OperationStatus, OperationUpdate, RuntimeEventKind,
};

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

#[test]
fn host_completion_enters_runtime_through_ordered_events() {
    let mut runtime = NodeRuntime::new(windows_node()).expect("runtime");
    let session_id = capyio_core::SessionId::new();
    let capability_id = CapabilityId::new();
    let operation_id = runtime
        .begin_host_operation(HostOperation::StartAudioStream {
            session_id,
            capability_id,
            projection_kind: ProjectionKind::ApplicationStream,
            requested_format: AudioFormat::microphone_baseline(),
        })
        .expect("begin");

    let update = runtime
        .complete_host_operation(
            operation_id,
            HostOperationCompletion::Succeeded {
                output: HostOperationOutput::AudioStreamStarted {
                    actual: ActualAudioStreamParameters {
                        format: AudioFormat::microphone_baseline(),
                        frames_per_burst: Some(192),
                        buffer_capacity_frames: 960,
                    },
                },
            },
        )
        .expect("complete");
    assert_eq!(update, OperationUpdate::Applied(OperationStatus::Completed));

    let events_before_late_cancel = runtime.snapshot().events.len();
    assert_eq!(
        runtime
            .cancel_host_operation(operation_id)
            .expect("late cancel"),
        OperationUpdate::AlreadyTerminal(OperationStatus::Completed)
    );

    let snapshot = runtime.snapshot();
    assert_eq!(snapshot.operations.len(), 1);
    assert_eq!(snapshot.operations[0].status, OperationStatus::Completed);
    assert_eq!(snapshot.events.len(), events_before_late_cancel);
    assert!(matches!(
        snapshot.events[snapshot.events.len() - 2].kind,
        RuntimeEventKind::OperationChanged {
            operation_id: id,
            status: OperationStatus::Pending,
        } if id == operation_id
    ));
    assert!(matches!(
        snapshot.events[snapshot.events.len() - 1].kind,
        RuntimeEventKind::OperationChanged {
            operation_id: id,
            status: OperationStatus::Completed,
        } if id == operation_id
    ));
}
