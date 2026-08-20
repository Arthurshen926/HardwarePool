use std::collections::BTreeSet;

use hardwarepool_core::{
    AudioCapabilitySpec, AudioFormat, AudioProcessingSupport, AudioQosMode, Availability,
    CapabilityDescriptor, CapabilityDetails, CapabilityId, CapabilityKind, LocalRole,
    NodeDescriptor, NodeId, NodeRole, PermissionRequirement, Platform, ProfileId, ProjectionKind,
    StreamRole,
};
use hardwarepool_protocol::{
    PROTOCOL_MAJOR, ProtocolError, decode_envelope, encode_envelope, new_envelope, v1,
};

fn android_node() -> NodeDescriptor {
    let mut node = NodeDescriptor::new(
        NodeId::new(),
        "Android phone",
        Platform::Android,
        "test-build",
        [NodeRole::Provider],
    );
    node.add_capability(CapabilityDescriptor {
        id: CapabilityId::new(),
        display_name: "Internal microphone".to_owned(),
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
    .expect("valid capability");
    node
}

#[test]
fn core_node_round_trips_through_protobuf_model() {
    let original = android_node();
    let wire = v1::NodeDescriptor::try_from(&original).expect("to wire");
    let decoded = NodeDescriptor::try_from(wire).expect("to core");
    assert_eq!(original, decoded);
}

#[test]
fn envelope_round_trips_as_binary() {
    let node = android_node();
    let hello = v1::Hello {
        node: Some(v1::NodeDescriptor::try_from(&node).expect("node conversion")),
        supported_protocol_majors: vec![PROTOCOL_MAJOR],
    };
    let envelope = new_envelope(None, v1::envelope::Payload::Hello(hello));
    let bytes = encode_envelope(&envelope);
    let decoded = decode_envelope(&bytes).expect("decode envelope");
    assert_eq!(decoded.protocol_major, PROTOCOL_MAJOR);
    assert!(matches!(
        decoded.payload,
        Some(v1::envelope::Payload::Hello(_))
    ));
}

#[test]
fn unsupported_major_is_rejected() {
    let mut envelope = new_envelope(
        None,
        v1::envelope::Payload::Error(v1::ErrorMessage {
            code: "test".to_owned(),
            category: "protocol".to_owned(),
            retryable: false,
            detail: String::new(),
            related_id: String::new(),
        }),
    );
    envelope.protocol_major = PROTOCOL_MAJOR + 1;
    let bytes = encode_envelope(&envelope);
    assert!(matches!(
        decode_envelope(&bytes),
        Err(ProtocolError::UnsupportedProtocolMajor { .. })
    ));
}
