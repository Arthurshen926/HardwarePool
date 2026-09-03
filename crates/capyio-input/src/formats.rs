use capyio_core::{FormatDescriptor, ProfileId};

#[must_use]
pub fn key_events_profile() -> ProfileId {
    ProfileId::key_events_v1()
}

#[must_use]
pub fn pointer_events_profile() -> ProfileId {
    ProfileId::pointer_events_v1()
}

#[must_use]
pub fn touch_events_profile() -> ProfileId {
    ProfileId::touch_events_v1()
}

#[must_use]
pub fn touchpad_frames_profile() -> ProfileId {
    ProfileId::touchpad_frames_v1()
}

#[must_use]
pub fn gamepad_state_profile() -> ProfileId {
    ProfileId::gamepad_state_v1()
}

#[must_use]
pub fn haptics_feedback_profile() -> ProfileId {
    ProfileId::haptics_feedback_v1()
}

#[must_use]
pub fn key_events_format() -> FormatDescriptor {
    FormatDescriptor::new("key-events-v1")
}

#[must_use]
pub fn pointer_events_format() -> FormatDescriptor {
    FormatDescriptor::new("pointer-events-v1")
}

#[must_use]
pub fn touch_snapshot_format() -> FormatDescriptor {
    FormatDescriptor::new("touch-snapshot-v1")
}

#[must_use]
pub fn touchpad_frame_format() -> FormatDescriptor {
    FormatDescriptor::new("touchpad-frame-v1")
}

#[must_use]
pub fn gamepad_state_format() -> FormatDescriptor {
    FormatDescriptor::new("gamepad-state-v1")
}

#[must_use]
pub fn dual_rumble_format() -> FormatDescriptor {
    FormatDescriptor::new("dual-rumble-v1")
}
