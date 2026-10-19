use capyio_core::SessionId;
use capyio_protocol::{
    decode_envelope, encode_envelope_checked, new_envelope, v1, validate_display_session_control,
};

fn launch() -> v1::DisplaySessionControl {
    v1::DisplaySessionControl {
        operation_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),
        action: v1::DisplaySessionAction::Launch as i32,
        route_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
        epoch: 1,
        selection: Some(v1::DisplayStreamSelection {
            mode: Some(v1::DisplayStreamMode {
                width: 1920,
                height: 1080,
                refresh_millihertz: 60000,
            }),
            bitrate_kbps: 15000,
        }),
        launch_record: vec![0; 176],
        ..Default::default()
    }
}

#[test]
fn display_launch_is_bounded_and_requires_authenticated_session_envelope() {
    let value = launch();
    let envelope = new_envelope(
        Some(SessionId::new()),
        v1::envelope::Payload::DisplaySessionControl(value.clone()),
    );
    let bytes = encode_envelope_checked(&envelope).unwrap();
    assert_eq!(decode_envelope(&bytes).unwrap(), envelope);
    let missing = new_envelope(None, v1::envelope::Payload::DisplaySessionControl(value));
    assert!(encode_envelope_checked(&missing).is_err());
    let mut wrong = launch();
    wrong.launch_record.push(0);
    assert!(validate_display_session_control(&wrong).is_err());
    wrong = launch();
    wrong.route_id.clear();
    assert!(validate_display_session_control(&wrong).is_err());
    wrong = launch();
    wrong.action = v1::DisplaySessionAction::Stop as i32;
    assert!(validate_display_session_control(&wrong).is_err());
}

#[test]
fn acknowledgement_and_first_frame_are_not_interchangeable_with_launch() {
    let mut value = launch();
    value.launch_record.clear();
    value.action = v1::DisplaySessionAction::Ack as i32;
    value.acknowledgement = vec![0; 64];
    validate_display_session_control(&value).unwrap();
    value.action = v1::DisplaySessionAction::Presented as i32;
    assert!(validate_display_session_control(&value).is_err());
    value.acknowledgement.clear();
    assert!(validate_display_session_control(&value).is_err());
    value.presented_frames = 1;
    validate_display_session_control(&value).unwrap();
}

#[test]
fn automatic_is_a_start_request_not_a_launch_override() {
    let mut value = launch();
    value.automatic = true;
    assert!(validate_display_session_control(&value).is_err());
    value.action = v1::DisplaySessionAction::Start as i32;
    value.launch_record.clear();
    validate_display_session_control(&value).unwrap();
    let envelope = new_envelope(
        Some(SessionId::new()),
        v1::envelope::Payload::DisplaySessionControl(value),
    );
    assert_eq!(
        decode_envelope(&encode_envelope_checked(&envelope).unwrap()).unwrap(),
        envelope
    );
}
