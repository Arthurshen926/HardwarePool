use capyio_core::ProfileId;

#[test]
fn video_and_input_profile_helpers_match_the_normative_registry() {
    let profiles = [
        (ProfileId::video_frames_v1(), "capyio.video.frames"),
        (ProfileId::key_events_v1(), "capyio.input.key-events"),
        (
            ProfileId::pointer_events_v1(),
            "capyio.input.pointer-events",
        ),
        (ProfileId::touch_events_v1(), "capyio.input.touch-events"),
        (
            ProfileId::touchpad_frames_v1(),
            "capyio.input.touchpad-frames",
        ),
        (ProfileId::gamepad_state_v1(), "capyio.input.gamepad-state"),
        (ProfileId::haptics_feedback_v1(), "capyio.haptics.feedback"),
    ];

    for (profile, expected_name) in profiles {
        profile.validate().expect("registered Profile ID");
        assert_eq!(profile.name, expected_name);
        assert_eq!(profile.major, 1);
    }
}
