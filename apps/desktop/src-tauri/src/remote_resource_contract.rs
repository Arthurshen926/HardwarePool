//! One backend eligibility rule for the command guard and real coordinators.
use capyio_core::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Touchpad,
    Haptics,
    Microphone,
    Speaker,
    Camera,
}
pub(super) fn kind(capability: &CapabilityDescriptor) -> Option<Kind> {
    let expected = match capability.class {
        CapabilityClass::Touchpad => (
            Kind::Touchpad,
            PortDirection::Source,
            ProfileId::touchpad_frames_v1(),
            None,
        ),
        CapabilityClass::Haptics => (
            Kind::Haptics,
            PortDirection::Sink,
            ProfileId::haptics_feedback_v1(),
            None,
        ),
        CapabilityClass::Microphone => (
            Kind::Microphone,
            PortDirection::Source,
            ProfileId::audio_frames_v1(),
            Some("pcm-s16le-48000-mono"),
        ),
        CapabilityClass::Speaker => (
            Kind::Speaker,
            PortDirection::Sink,
            ProfileId::audio_frames_v1(),
            Some("pcm-s16le-48000-stereo"),
        ),
        CapabilityClass::Camera => (
            Kind::Camera,
            PortDirection::Source,
            ProfileId::video_frames_v1(),
            Some("cavc-avc-access-unit-v1"),
        ),
        _ => return None,
    };
    capability
        .ports
        .values()
        .any(|port| {
            port.direction == expected.1
                && port.profile == expected.2
                && expected.3.is_none_or(|format| {
                    port.interoperability_mode == InteroperabilityMode::AdapterManaged
                        && port.formats.contains(&FormatDescriptor::new(format))
                })
        })
        .then_some(expected.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    fn capability(
        class: CapabilityClass,
        direction: PortDirection,
        profile: ProfileId,
        format: &str,
    ) -> CapabilityDescriptor {
        let id = CapabilityId::new();
        let port = PortId::new();
        CapabilityDescriptor {
            id,
            adapter_instance_id: AdapterInstanceId::new(),
            display_name: "resource".into(),
            class,
            availability: Availability::Available,
            permission_requirement: PermissionRequirement::UserConfirmation,
            metadata: BTreeMap::new(),
            ports: BTreeMap::from([(
                port,
                PortDescriptor {
                    id: port,
                    capability_id: id,
                    display_name: "port".into(),
                    direction,
                    profile,
                    schema_id: None,
                    formats: vec![FormatDescriptor::new(format)],
                    qos_modes: BTreeSet::from([QosMode::Interactive]),
                    clock_domain: None,
                    availability: Availability::Available,
                    permission_requirement: PermissionRequirement::UserConfirmation,
                    interoperability_mode: InteroperabilityMode::AdapterManaged,
                },
            )]),
        }
    }
    #[test]
    fn media_entry_guard_accepts_only_implemented_contracts() {
        for (class, direction, profile, format, expected) in [
            (
                CapabilityClass::Microphone,
                PortDirection::Source,
                ProfileId::audio_frames_v1(),
                "pcm-s16le-48000-mono",
                Kind::Microphone,
            ),
            (
                CapabilityClass::Speaker,
                PortDirection::Sink,
                ProfileId::audio_frames_v1(),
                "pcm-s16le-48000-stereo",
                Kind::Speaker,
            ),
            (
                CapabilityClass::Camera,
                PortDirection::Source,
                ProfileId::video_frames_v1(),
                "cavc-avc-access-unit-v1",
                Kind::Camera,
            ),
        ] {
            let mut cap = capability(class, direction, profile, format);
            assert_eq!(kind(&cap), Some(expected));
            let port = cap.ports.values_mut().next().unwrap();
            port.interoperability_mode = InteroperabilityMode::StandardPort;
            assert_eq!(kind(&cap), None);
            cap.ports.values_mut().next().unwrap().interoperability_mode =
                InteroperabilityMode::AdapterManaged;
            cap.ports.values_mut().next().unwrap().formats = vec![FormatDescriptor::new("fixture")];
            assert_eq!(kind(&cap), None);
        }
        assert_eq!(
            kind(&capability(
                CapabilityClass::Speaker,
                PortDirection::Source,
                ProfileId::audio_frames_v1(),
                "pcm-s16le-48000-stereo"
            )),
            None
        );
    }
    #[test]
    fn original_touchpad_and_haptics_entries_remain_supported() {
        assert_eq!(
            kind(&capability(
                CapabilityClass::Touchpad,
                PortDirection::Source,
                ProfileId::touchpad_frames_v1(),
                "touchpad"
            )),
            Some(Kind::Touchpad)
        );
        assert_eq!(
            kind(&capability(
                CapabilityClass::Haptics,
                PortDirection::Sink,
                ProfileId::haptics_feedback_v1(),
                "haptics"
            )),
            Some(Kind::Haptics)
        );
    }
}
