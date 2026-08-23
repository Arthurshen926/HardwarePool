use std::collections::BTreeSet;

use capyio_core::{
    AudioCapabilitySpec, AudioFormat, AudioProcessingSupport, AudioQosMode, Availability,
    BindingState, CapabilityDescriptor, CapabilityDetails, CapabilityId, CapabilityKind, LocalRole,
    NodeId, PermissionRequirement, ProfileId, ProjectionKind, Session, StreamRole,
};

fn audio_capability(capture: bool) -> CapabilityDescriptor {
    let id = CapabilityId::new();
    if capture {
        CapabilityDescriptor {
            id,
            display_name: "Phone microphone".to_owned(),
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
        }
    } else {
        CapabilityDescriptor {
            id,
            display_name: "Phone speaker".to_owned(),
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
        }
    }
}

fn activate(session: &mut Session, capability: &CapabilityDescriptor, projection: ProjectionKind) {
    session
        .request_binding(capability, projection)
        .expect("request");
    session
        .authorize(capability.id, 0, 60_000)
        .expect("authorize");
    let format = capability
        .audio_spec()
        .expect("audio")
        .formats
        .first()
        .expect("format")
        .clone();
    session
        .negotiate_audio(capability, format, 1)
        .expect("negotiate");
    session.begin_start(capability.id, 2).expect("start");
    session.mark_active(capability.id).expect("active");
}

#[test]
fn microphone_can_stop_without_interrupting_speaker() {
    let microphone = audio_capability(true);
    let speaker = audio_capability(false);
    let mut session = Session::new(NodeId::new(), NodeId::new());

    activate(
        &mut session,
        &microphone,
        ProjectionKind::SystemCaptureEndpoint,
    );
    activate(&mut session, &speaker, ProjectionKind::SystemRenderEndpoint);

    session.begin_stop(microphone.id).expect("begin stop mic");
    session.mark_stopped(microphone.id).expect("stop mic");

    assert_eq!(
        session.binding(microphone.id).expect("mic binding").state,
        BindingState::Stopped
    );
    assert_eq!(
        session.binding(speaker.id).expect("speaker binding").state,
        BindingState::Active
    );
    assert_eq!(session.active_binding_count(), 1);
}
