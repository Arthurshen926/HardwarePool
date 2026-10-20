//! Catalog selection is not consent or proof of an AdapterManaged wire contract.
//! Keep compatible-but-not-initiable peers visible without offering Start.
use capyio_core::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AudioDirection {
    Microphone,
    Speaker,
}
impl AudioDirection {
    pub(crate) fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "microphone" => Ok(Self::Microphone),
            "speaker" => Ok(Self::Speaker),
            _ => Err("unknown audio direction"),
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::Speaker => "speaker",
        }
    }
    fn format(self) -> FormatDescriptor {
        FormatDescriptor::new(match self {
            Self::Microphone => "pcm-s16le-48000-mono",
            Self::Speaker => "pcm-s16le-48000-stereo",
        })
    }
    fn local_direction(self) -> PortDirection {
        match self {
            Self::Microphone => PortDirection::Source,
            Self::Speaker => PortDirection::Sink,
        }
    }
}

pub(crate) const ADMISSION_UNSUPPORTED: &str =
    "该设备公布了兼容音频端口，但当前接收端契约尚不支持从 Android 发起。此处不会启动音频。";

fn usable(value: Availability) -> bool {
    // PermissionRequired is not permission granted. Draft still goes through consent.
    matches!(
        value,
        Availability::Available | Availability::PermissionRequired
    )
}

pub(crate) fn select_endpoint(
    node: &NodeDescriptor,
    direction: AudioDirection,
    local: bool,
) -> Result<PortRef, &'static str> {
    if node.online_state != OnlineState::Online {
        return Err("audio node is offline");
    }
    let expected = if local {
        direction.local_direction()
    } else {
        match direction.local_direction() {
            PortDirection::Source => PortDirection::Sink,
            _ => PortDirection::Source,
        }
    };
    let mut candidates = node
        .capabilities
        .values()
        .filter(|cap| {
            let class_matches = if local {
                cap.class
                    == match direction {
                        AudioDirection::Microphone => CapabilityClass::Microphone,
                        AudioDirection::Speaker => CapabilityClass::Speaker,
                    }
            } else {
                matches!(
                    cap.class,
                    CapabilityClass::Microphone | CapabilityClass::Speaker
                )
            };
            class_matches
                && usable(cap.availability)
                && node
                    .adapter_instances
                    .get(&cap.adapter_instance_id)
                    .is_some_and(|adapter| {
                        adapter.state == AdapterState::Ready
                            && adapter.owned_capabilities.contains(&cap.id)
                            && adapter
                                .supported_route_modes
                                .contains(&RouteBackend::AdapterManaged)
                    })
        })
        .flat_map(|cap| {
            cap.ports
                .values()
                .filter(move |port| port.capability_id == cap.id)
                .map(move |port| (cap, port))
        })
        .filter(|(_, port)| {
            port.direction == expected
                && port.profile == ProfileId::audio_frames_v1()
                && port.formats.contains(&direction.format())
                && port.qos_modes.contains(&QosMode::Interactive)
                && port.interoperability_mode == InteroperabilityMode::AdapterManaged
                && usable(port.availability)
        });
    let (cap, port) = candidates.next().ok_or("no compatible audio endpoint")?;
    if candidates.next().is_some() {
        return Err("ambiguous audio endpoints; explicit port selection is required");
    }
    Ok(PortRef {
        node_id: node.id,
        capability_id: cap.id,
        port_id: port.id,
    })
}

/// Existing Windows receiver is itself bound to these exact endpoints. This is
/// a legacy admission contract, not endpoint discovery or an OS-based fallback.
/// Add a negotiated, versioned contract here only with its receiver implemented.
pub(crate) fn supports_inbound_draft(
    node: &NodeDescriptor,
    endpoint: PortRef,
    direction: AudioDirection,
) -> bool {
    let Some(cap) = node.capabilities.get(&endpoint.capability_id) else {
        return false;
    };
    let Some(adapter) = node.adapter_instances.get(&cap.adapter_instance_id) else {
        return false;
    };
    let (cap_id, port_id) = match direction {
        AudioDirection::Microphone => (
            "d41251b3-63ed-4e83-807e-4c4ab9b20d21",
            "d41251b3-63ed-4e83-807e-4c4ab9b20d31",
        ),
        AudioDirection::Speaker => (
            "d41251b3-63ed-4e83-807e-4c4ab9b20d22",
            "d41251b3-63ed-4e83-807e-4c4ab9b20d32",
        ),
    };
    endpoint.node_id == node.id
        && adapter.adapter_type == "dev.capyio.windows.native-audio-route"
        && endpoint.capability_id.to_string() == cap_id
        && endpoint.port_id.to_string() == port_id
}

pub(crate) fn select_route(
    local: &NodeDescriptor,
    peer: &NodeDescriptor,
    sessions: &[Session],
    direction: AudioDirection,
) -> Result<(SessionId, PortRef, PortRef), &'static str> {
    let mut ready = sessions.iter().filter(|s| {
        s.local_node_id == local.id && s.remote_node_id == peer.id && s.state == SessionState::Ready
    });
    let session = ready.next().ok_or("peer is not connected")?;
    if ready.next().is_some() {
        return Err("ambiguous ready Sessions");
    }
    let local_port = select_endpoint(local, direction, true)?;
    let remote_port = select_endpoint(peer, direction, false)?;
    if !supports_inbound_draft(peer, remote_port, direction) {
        return Err(ADMISSION_UNSUPPORTED);
    }
    let (source, sink) = match direction {
        AudioDirection::Microphone => (local_port, remote_port),
        AudioDirection::Speaker => (remote_port, local_port),
    };
    Ok((session.id, source, sink))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    fn node(local: bool, direction: AudioDirection, windows: bool) -> NodeDescriptor {
        let mut node = NodeDescriptor::new(
            NodeId::new(),
            "fixture",
            if local {
                Platform::Android
            } else if windows {
                Platform::Windows
            } else {
                Platform::Linux
            },
            "test",
            "test",
            [ProtocolVersion::new(1, 0)],
        );
        let adapter_id = AdapterInstanceId::new();
        node.add_adapter(AdapterInstanceDescriptor {
            id: adapter_id,
            adapter_type: if windows {
                "dev.capyio.windows.native-audio-route"
            } else {
                "fixture"
            }
            .into(),
            display_name: "fixture".into(),
            deployment_mode: AdapterDeploymentMode::InProcess,
            version: "1".into(),
            state: AdapterState::Ready,
            health: AdapterHealth::Healthy,
            owned_capabilities: BTreeSet::new(),
            supported_route_modes: BTreeSet::from([RouteBackend::AdapterManaged]),
        })
        .unwrap();
        let (id, port_id) = if windows {
            match direction {
                AudioDirection::Microphone => (
                    "d41251b3-63ed-4e83-807e-4c4ab9b20d21".parse().unwrap(),
                    "d41251b3-63ed-4e83-807e-4c4ab9b20d31".parse().unwrap(),
                ),
                AudioDirection::Speaker => (
                    "d41251b3-63ed-4e83-807e-4c4ab9b20d22".parse().unwrap(),
                    "d41251b3-63ed-4e83-807e-4c4ab9b20d32".parse().unwrap(),
                ),
            }
        } else {
            (CapabilityId::new(), PortId::new())
        };
        let class = if local || windows {
            match direction {
                AudioDirection::Microphone => CapabilityClass::Microphone,
                AudioDirection::Speaker => CapabilityClass::Speaker,
            }
        } else {
            CapabilityClass::Speaker
        };
        let port_direction = if local {
            direction.local_direction()
        } else if direction == AudioDirection::Microphone {
            PortDirection::Sink
        } else {
            PortDirection::Source
        };
        node.add_capability(CapabilityDescriptor {
            id,
            adapter_instance_id: adapter_id,
            display_name: "fixture".into(),
            class,
            availability: Availability::Available,
            permission_requirement: PermissionRequirement::UserConfirmation,
            metadata: BTreeMap::new(),
            ports: BTreeMap::from([(
                port_id,
                PortDescriptor {
                    id: port_id,
                    capability_id: id,
                    display_name: "fixture".into(),
                    direction: port_direction,
                    profile: ProfileId::audio_frames_v1(),
                    schema_id: None,
                    formats: vec![direction.format()],
                    qos_modes: BTreeSet::from([QosMode::Interactive]),
                    clock_domain: None,
                    availability: Availability::Available,
                    permission_requirement: PermissionRequirement::UserConfirmation,
                    interoperability_mode: InteroperabilityMode::AdapterManaged,
                },
            )]),
        })
        .unwrap();
        node
    }
    fn port(node: &mut NodeDescriptor) -> &mut PortDescriptor {
        node.capabilities
            .values_mut()
            .next()
            .unwrap()
            .ports
            .values_mut()
            .next()
            .unwrap()
    }
    #[test]
    fn linux_random_speaker_sink_is_compatible_but_not_initiable() {
        let local = node(true, AudioDirection::Microphone, false);
        let linux = node(false, AudioDirection::Microphone, false);
        let endpoint = select_endpoint(&linux, AudioDirection::Microphone, false).unwrap();
        assert_eq!(
            endpoint.port_id,
            linux
                .capabilities
                .values()
                .next()
                .unwrap()
                .ports
                .values()
                .next()
                .unwrap()
                .id
        );
        assert!(!supports_inbound_draft(
            &linux,
            endpoint,
            AudioDirection::Microphone
        ));
        assert_eq!(
            select_route(
                &local,
                &linux,
                &[Session::new(local.id, linux.id)],
                AudioDirection::Microphone
            ),
            Err(ADMISSION_UNSUPPORTED)
        );
        assert!(select_endpoint(&linux, AudioDirection::Speaker, false).is_err());
    }
    #[test]
    fn windows_both_directions_bind_actual_advertised_endpoints() {
        for direction in [AudioDirection::Microphone, AudioDirection::Speaker] {
            let local = node(true, direction, false);
            let remote = node(false, direction, true);
            let session = Session::new(local.id, remote.id);
            let (session_id, source, sink) =
                select_route(&local, &remote, std::slice::from_ref(&session), direction).unwrap();
            assert_eq!(session_id, session.id);
            assert_eq!(
                if direction == AudioDirection::Microphone {
                    source
                } else {
                    sink
                },
                select_endpoint(&local, direction, true).unwrap()
            );
            assert_eq!(
                if direction == AudioDirection::Microphone {
                    sink
                } else {
                    source
                },
                select_endpoint(&remote, direction, false).unwrap()
            );
        }
    }
    #[test]
    fn incompatible_catalogs_fail_closed() {
        let base = node(false, AudioDirection::Microphone, false);
        for mutate in [
            (|n: &mut NodeDescriptor| port(n).profile = ProfileId::new("capyio.audio.frames", 2))
                as fn(&mut NodeDescriptor),
            |n| port(n).direction = PortDirection::Source,
            |n| port(n).formats = vec![FormatDescriptor::new("pcm-s16le-48000-stereo")],
            |n| {
                port(n).formats[0]
                    .parameters
                    .insert("unknown".into(), "required".into())
                    .map(|_| ())
                    .unwrap_or(())
            },
            |n| port(n).qos_modes = BTreeSet::from([QosMode::Basic]),
            |n| port(n).interoperability_mode = InteroperabilityMode::StandardPort,
            |n| port(n).availability = Availability::Busy,
            |n| n.capabilities.values_mut().next().unwrap().availability = Availability::Offline,
            |n| n.adapter_instances.values_mut().next().unwrap().state = AdapterState::Stopped,
            |n| {
                n.adapter_instances
                    .values_mut()
                    .next()
                    .unwrap()
                    .supported_route_modes
                    .clear()
            },
            |n| n.online_state = OnlineState::Offline,
        ] {
            let mut n = base.clone();
            mutate(&mut n);
            assert!(select_endpoint(&n, AudioDirection::Microphone, false).is_err());
        }
    }
    #[test]
    fn ambiguous_ports_and_sessions_are_rejected() {
        let local = node(true, AudioDirection::Microphone, false);
        let mut remote = node(false, AudioDirection::Microphone, true);
        let sessions = [
            Session::new(local.id, remote.id),
            Session::new(local.id, remote.id),
        ];
        assert_eq!(
            select_route(&local, &remote, &sessions, AudioDirection::Microphone),
            Err("ambiguous ready Sessions")
        );
        let mut duplicate = port(&mut remote).clone();
        duplicate.id = PortId::new();
        remote
            .capabilities
            .values_mut()
            .next()
            .unwrap()
            .ports
            .insert(duplicate.id, duplicate);
        assert!(
            select_endpoint(&remote, AudioDirection::Microphone, false)
                .unwrap_err()
                .contains("ambiguous")
        );
    }
    #[test]
    fn stale_or_disconnected_selection_cannot_start() {
        let local = node(true, AudioDirection::Microphone, false);
        let mut remote = node(false, AudioDirection::Microphone, true);
        let mut session = Session::new(local.id, remote.id);
        assert!(
            select_route(
                &local,
                &remote,
                std::slice::from_ref(&session),
                AudioDirection::Microphone
            )
            .is_ok()
        );
        session.state = SessionState::Suspended;
        assert!(
            select_route(
                &local,
                &remote,
                std::slice::from_ref(&session),
                AudioDirection::Microphone
            )
            .is_err()
        );
        session.state = SessionState::Ready;
        remote.capabilities.clear();
        assert!(select_route(&local, &remote, &[session], AudioDirection::Microphone).is_err());
    }
    #[test]
    fn matching_profile_or_ids_do_not_invent_receiver_support() {
        let mut remote = node(false, AudioDirection::Microphone, true);
        remote
            .adapter_instances
            .values_mut()
            .next()
            .unwrap()
            .adapter_type = "unknown".into();
        let endpoint = select_endpoint(&remote, AudioDirection::Microphone, false).unwrap();
        assert!(!supports_inbound_draft(
            &remote,
            endpoint,
            AudioDirection::Microphone
        ));
        assert!(AudioDirection::parse("camera").is_err());
        assert_eq!(
            AudioDirection::parse("microphone").unwrap().name(),
            "microphone"
        );
        assert_eq!(AudioDirection::parse("speaker").unwrap().name(), "speaker");
    }
}
