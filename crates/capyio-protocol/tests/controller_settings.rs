use capyio_protocol::{decode_envelope, encode_envelope_checked, new_envelope, v1};

#[test]
fn controller_settings_wire_preserves_revision_and_applied_ack() {
    let request = v1::ControllerSettingsRequest {
        request_id: 7,
        expected_revision: 11,
        motion_enabled: false,
        haptics_strength: 35,
    };
    let envelope = new_envelope(
        None,
        v1::envelope::Payload::ControllerSettingsRequest(request),
    );
    let decoded = decode_envelope(&encode_envelope_checked(&envelope).unwrap()).unwrap();
    assert_eq!(
        decoded.payload,
        Some(v1::envelope::Payload::ControllerSettingsRequest(request))
    );
    let state = v1::ControllerSettingsState {
        revision: 12,
        motion_enabled: false,
        haptics_strength: 35,
        request_id: 7,
        error: String::new(),
    };
    let envelope = new_envelope(
        None,
        v1::envelope::Payload::ControllerSettingsState(state.clone()),
    );
    let decoded = decode_envelope(&encode_envelope_checked(&envelope).unwrap()).unwrap();
    assert_eq!(
        decoded.payload,
        Some(v1::envelope::Payload::ControllerSettingsState(state))
    );
}
