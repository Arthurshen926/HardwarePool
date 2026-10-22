use capyio_core::{
    AuthorizationState, CapabilityClass, PortDirection, PortRef, ProfileId, Route, RouteId,
    RouteState, SessionState,
};
use capyio_runtime::{NodeRuntime, PortLeasePolicy};
use serde::Serialize;
use std::collections::BTreeMap;

const REQUEST_TTL_MS: u64 = 60_000;
// A single visible approval covers ordinary use; short Port leases are renewed
// separately by the authenticated control owner and never extend this grant.
const GRANT_TTL_MS: u64 = 8 * 60 * 60 * 1_000;
const MAX_PENDING: usize = 16;

#[derive(Clone, Debug, Default)]
pub(crate) struct TouchpadConsentGate {
    pending: BTreeMap<RouteId, u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_core::{PortRef, RouteBackend};

    #[test]
    fn camera_source_requires_its_own_consent_and_lease() {
        let android = capyio_testkit::android_node();
        let mut windows = capyio_testkit::windows_node();
        let cap = android
            .capabilities
            .values()
            .find(|c| c.class == CapabilityClass::Camera)
            .unwrap();
        let port = cap.ports.values().next().unwrap();
        let source = PortRef {
            node_id: android.id,
            capability_id: cap.id,
            port_id: port.id,
        };
        let sink_cap = windows.capabilities.values_mut().next().unwrap();
        sink_cap.class = CapabilityClass::Camera;
        let sink_port = sink_cap.ports.values_mut().next().unwrap();
        sink_port.direction = PortDirection::Sink;
        sink_port.profile = port.profile.clone();
        sink_port.formats = port.formats.clone();
        sink_port.qos_modes = port.qos_modes.clone();
        sink_port.interoperability_mode = port.interoperability_mode;
        let sink = PortRef {
            node_id: windows.id,
            capability_id: sink_cap.id,
            port_id: sink_port.id,
        };
        let mut runtime = NodeRuntime::new(android).unwrap();
        runtime.register_peer(windows, true).unwrap();
        let session = runtime.open_session(sink.node_id).unwrap();
        let id = runtime
            .create_route(session, source, sink, RouteBackend::CapyDataPlane)
            .unwrap();
        let mut gate = TouchpadConsentGate::default();
        gate.enqueue(&mut runtime, id, 100).unwrap();
        assert_eq!(gate.requests(&mut runtime, 101)[0].resource_kind, "camera");
        assert!(
            runtime
                .acquire_route_port_lease(id, source, 1000, 101)
                .is_err()
        );
        gate.resolve(&mut runtime, id, true, 102).unwrap();
        runtime
            .acquire_route_port_lease(id, source, 1000, 103)
            .unwrap();
    }

    fn setup_haptics() -> (NodeRuntime, RouteId) {
        let android = capyio_testkit::android_node();
        let mut windows = capyio_testkit::windows_node();
        let sink_cap = android
            .capabilities
            .values()
            .find(|cap| cap.class == CapabilityClass::Haptics)
            .unwrap();
        let sink_port = sink_cap.ports.values().next().unwrap();
        let sink = PortRef {
            node_id: android.id,
            capability_id: sink_cap.id,
            port_id: sink_port.id,
        };
        // A fixture Source, not a claim that the production Windows catalog is wired.
        let source_cap = windows.capabilities.values_mut().next().unwrap();
        source_cap.class = CapabilityClass::Haptics;
        let source_port = source_cap.ports.values_mut().next().unwrap();
        source_port.direction = PortDirection::Source;
        source_port.profile = sink_port.profile.clone();
        source_port.formats = sink_port.formats.clone();
        source_port.qos_modes = sink_port.qos_modes.clone();
        source_port.interoperability_mode = sink_port.interoperability_mode;
        let source = PortRef {
            node_id: windows.id,
            capability_id: source_cap.id,
            port_id: source_port.id,
        };
        let mut runtime = NodeRuntime::new(android).unwrap();
        runtime.register_peer(windows, true).unwrap();
        let session = runtime.open_session(source.node_id).unwrap();
        let route = runtime
            .create_route(session, source, sink, RouteBackend::CapyDataPlane)
            .unwrap();
        (runtime, route)
    }

    #[test]
    fn haptics_uses_local_sink_consent_and_an_independent_exclusive_lease() {
        let (mut runtime, id) = setup_haptics();
        let route = runtime.route(id).unwrap().clone();
        assert!(TouchpadConsentGate::requires_confirmation(&runtime, &route));
        assert!(!is_local_touchpad(&runtime, &route));
        let mut gate = TouchpadConsentGate::default();
        gate.enqueue(&mut runtime, id, 100).unwrap();
        let requests = gate.requests(&mut runtime, 101);
        assert_eq!(requests[0].resource_kind, "haptics");
        assert_eq!(
            requests[0].capability_id,
            route.sink.capability_id.to_string()
        );
        assert!(
            runtime
                .acquire_route_port_lease(id, route.sink, 1000, 101)
                .is_err()
        );
        gate.resolve(&mut runtime, id, true, 102).unwrap();
        runtime
            .acquire_route_port_lease(id, route.sink, 1000, 103)
            .unwrap();

        let second = runtime
            .create_route(route.session_id, route.source, route.sink, route.backend)
            .unwrap();
        gate.enqueue(&mut runtime, second, 104).unwrap();
        assert_eq!(
            runtime.route(second).unwrap().authorization,
            AuthorizationState::Pending
        );
        gate.resolve(&mut runtime, second, true, 105).unwrap();
        assert!(
            runtime
                .acquire_route_port_lease(second, route.sink, 1000, 106)
                .is_err()
        );
    }

    #[test]
    fn haptics_denial_does_not_grant_an_actuator() {
        let (mut runtime, id) = setup_haptics();
        let mut gate = TouchpadConsentGate::default();
        gate.enqueue(&mut runtime, id, 100).unwrap();
        gate.resolve(&mut runtime, id, false, 101).unwrap();
        assert_eq!(
            runtime.route(id).unwrap().authorization,
            AuthorizationState::Denied
        );
        assert!(gate.requests(&mut runtime, 102).is_empty());
    }

    fn setup() -> (NodeRuntime, RouteId) {
        setup_source(CapabilityClass::Touchpad)
    }

    fn setup_source(class: CapabilityClass) -> (NodeRuntime, RouteId) {
        let android = capyio_testkit::android_node();
        let mut windows = capyio_testkit::windows_node();
        if class == CapabilityClass::Gamepad {
            // The shared historical fixture projects IMU only. Supply a test-only
            // compatible controls Sink, not a production Windows catalog claim.
            let source = android
                .capabilities
                .values()
                .find(|cap| cap.class == class)
                .unwrap()
                .ports
                .values()
                .next()
                .unwrap();
            let sink = windows
                .capabilities
                .values_mut()
                .find(|cap| cap.class == class)
                .unwrap()
                .ports
                .values_mut()
                .next()
                .unwrap();
            sink.profile = source.profile.clone();
            sink.formats = source.formats.clone();
            sink.qos_modes = source.qos_modes.clone();
            sink.interoperability_mode = source.interoperability_mode;
        }
        let endpoint = |node: &capyio_core::NodeDescriptor| {
            let cap = node
                .capabilities
                .values()
                .find(|c| {
                    c.class == class
                        && c.ports.values().any(|port| {
                            port.profile
                                == if class == CapabilityClass::Gamepad {
                                    ProfileId::gamepad_state_v1()
                                } else {
                                    ProfileId::touchpad_frames_v1()
                                }
                        })
                })
                .unwrap();
            PortRef {
                node_id: node.id,
                capability_id: cap.id,
                port_id: cap
                    .ports
                    .values()
                    .find(|port| {
                        port.profile
                            == if class == CapabilityClass::Gamepad {
                                ProfileId::gamepad_state_v1()
                            } else {
                                ProfileId::touchpad_frames_v1()
                            }
                    })
                    .unwrap()
                    .id,
            }
        };
        let source = endpoint(&android);
        let sink = endpoint(&windows);
        let mut runtime = NodeRuntime::new(android).unwrap();
        runtime.register_peer(windows, true).unwrap();
        let session = runtime.open_session(sink.node_id).unwrap();
        let route = runtime
            .create_route(session, source, sink, RouteBackend::CapyDataPlane)
            .unwrap();
        (runtime, route)
    }

    #[test]
    fn gamepad_consent_is_not_touchpad_or_actuator_authority() {
        let (mut runtime, id) = setup_source(CapabilityClass::Gamepad);
        let route = runtime.route(id).unwrap().clone();
        assert!(!is_local_touchpad(&runtime, &route));
        assert!(TouchpadConsentGate::requires_confirmation(&runtime, &route));
        let mut gate = TouchpadConsentGate::default();
        gate.enqueue(&mut runtime, id, 100).unwrap();
        let requests = gate.requests(&mut runtime, 101);
        assert_eq!(requests[0].resource_kind, "gamepad");
        assert_eq!(
            requests[0].capability_id,
            route.source.capability_id.to_string()
        );
        assert!(
            runtime
                .acquire_route_port_lease(id, route.source, 1000, 101)
                .is_err()
        );
        gate.resolve(&mut runtime, id, true, 102).unwrap();
        runtime
            .acquire_route_port_lease(id, route.source, 1000, 103)
            .unwrap();
        assert_eq!(runtime.route(id).unwrap().state, RouteState::Draft);
        let second = runtime
            .create_route(route.session_id, route.source, route.sink, route.backend)
            .unwrap();
        gate.enqueue(&mut runtime, second, 104).unwrap();
        gate.resolve(&mut runtime, second, false, 105).unwrap();
        assert_eq!(
            runtime.route(second).unwrap().authorization,
            AuthorizationState::Denied
        );
    }

    #[test]
    fn peer_bounded_consent_keeps_the_earlier_finite_deadline() {
        for peer_deadline in [3_600_100, 100 + GRANT_TTL_MS + 1_000] {
            let (mut runtime, id) = setup();
            let mut gate = TouchpadConsentGate::default();
            gate.enqueue(&mut runtime, id, 100).unwrap();
            gate.resolve_bounded(&mut runtime, id, true, 100, Some(peer_deadline))
                .unwrap();
            assert_eq!(
                runtime.route(id).unwrap().authorization,
                AuthorizationState::Authorized {
                    expires_at_ms: Some(peer_deadline.min(100 + GRANT_TTL_MS)),
                }
            );
        }
    }

    #[test]
    fn expired_peer_deadline_does_not_authorize_local_hardware() {
        let (mut runtime, id) = setup();
        let mut gate = TouchpadConsentGate::default();
        gate.enqueue(&mut runtime, id, 100).unwrap();
        assert!(
            gate.resolve_bounded(&mut runtime, id, true, 101, Some(101))
                .is_err()
        );
        assert_eq!(
            runtime.route(id).unwrap().authorization,
            AuthorizationState::Pending
        );
    }

    #[test]
    fn pending_cannot_lease_and_allow_is_finite_and_single_use() {
        let (mut runtime, id) = setup();
        let mut gate = TouchpadConsentGate::default();
        gate.enqueue(&mut runtime, id, 100).unwrap();
        let source = runtime.route(id).unwrap().source;
        assert!(
            runtime
                .acquire_route_port_lease(id, source, 1000, 101)
                .is_err()
        );
        assert_eq!(gate.requests(&mut runtime, 102).len(), 1);
        gate.resolve(&mut runtime, id, true, 103).unwrap();
        assert_eq!(
            runtime.route(id).unwrap().authorization,
            AuthorizationState::Authorized {
                expires_at_ms: Some(103 + GRANT_TTL_MS)
            }
        );
        assert!(
            runtime
                .acquire_route_port_lease(id, source, 1000, 104)
                .is_ok()
        );
        assert!(gate.resolve(&mut runtime, id, false, 105).is_err());
        assert!(runtime.deny_pending_route(id).is_err());
        // Longer use does not increase the original grant or persist consent.
        for now in (60_000..=420_000).step_by(60_000) {
            runtime
                .acquire_route_port_lease(id, source, now + 300_000, now)
                .unwrap();
        }
        assert_eq!(
            runtime.route(id).unwrap().authorization,
            AuthorizationState::Authorized {
                expires_at_ms: Some(103 + GRANT_TTL_MS)
            }
        );
        assert!(
            runtime
                .acquire_route_port_lease(id, source, GRANT_TTL_MS + 104, GRANT_TTL_MS + 103)
                .is_err()
        );
    }

    #[test]
    fn denial_and_expiry_never_become_grants_and_retries_do_not_extend_timeout() {
        for deny in [true, false] {
            let (mut runtime, id) = setup();
            let mut gate = TouchpadConsentGate::default();
            gate.enqueue(&mut runtime, id, 100).unwrap();
            gate.enqueue(&mut runtime, id, 200).unwrap();
            assert_eq!(gate.requests(&mut runtime, 201)[0].expires_at_ms, 60_100);
            if deny {
                gate.resolve(&mut runtime, id, false, 300).unwrap();
            } else {
                gate.expire(&mut runtime, 60_100);
            }
            assert_eq!(
                runtime.route(id).unwrap().authorization,
                AuthorizationState::Denied
            );
            assert!(gate.resolve(&mut runtime, id, true, 60_101).is_err());
            assert!(gate.requests(&mut runtime, 60_101).is_empty());
            let before = runtime.snapshot().events.len();
            runtime.deny_pending_route(id).unwrap();
            assert_eq!(runtime.snapshot().events.len(), before);
        }
    }

    #[test]
    fn disconnected_requests_cannot_be_approved_and_queue_is_bounded() {
        let (mut runtime, id) = setup();
        let route = runtime.route(id).unwrap().clone();
        let mut gate = TouchpadConsentGate::default();
        gate.enqueue(&mut runtime, id, 100).unwrap();
        for index in 1..=MAX_PENDING {
            let next = runtime
                .create_route(route.session_id, route.source, route.sink, route.backend)
                .unwrap();
            gate.enqueue(&mut runtime, next, 100).unwrap();
            if index == MAX_PENDING {
                assert_eq!(
                    runtime.route(next).unwrap().authorization,
                    AuthorizationState::Denied
                );
                assert!(!gate.pending.contains_key(&next));
            } else {
                assert_eq!(
                    runtime.route(next).unwrap().authorization,
                    AuthorizationState::Pending
                );
            }
        }
        assert_eq!(gate.requests(&mut runtime, 101).len(), MAX_PENDING);
        runtime.set_peer_online(route.sink.node_id, false).unwrap();
        assert!(gate.resolve(&mut runtime, id, true, 102).is_err());
        assert!(gate.requests(&mut runtime, 103).is_empty());
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConsentRequest {
    route_id: String,
    peer_node_id: String,
    peer_name: String,
    capability_name: String,
    capability_id: String,
    resource_kind: &'static str,
    expires_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile_id: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selection_revision: Option<String>,
}

pub(crate) fn is_local_touchpad(runtime: &NodeRuntime, route: &Route) -> bool {
    route.profile == ProfileId::touchpad_frames_v1()
        && consent_port(runtime, route) == Some(route.source)
}

/// Consent belongs to the locally owned resource, which can be either the
/// producing touch surface or the consuming actuator. Node roles stay symmetric.
fn consent_port(runtime: &NodeRuntime, route: &Route) -> Option<PortRef> {
    let snapshot = runtime.snapshot();
    for (endpoint, class, direction, profile) in [
        (
            route.source,
            CapabilityClass::Microphone,
            PortDirection::Source,
            ProfileId::audio_frames_v1(),
        ),
        (
            route.sink,
            CapabilityClass::Speaker,
            PortDirection::Sink,
            ProfileId::audio_frames_v1(),
        ),
        (
            route.sink,
            CapabilityClass::Display,
            PortDirection::Sink,
            ProfileId::video_frames_v1(),
        ),
        (
            route.source,
            CapabilityClass::Camera,
            PortDirection::Source,
            ProfileId::video_frames_v1(),
        ),
        (
            route.source,
            CapabilityClass::Touchpad,
            PortDirection::Source,
            ProfileId::touchpad_frames_v1(),
        ),
        (
            route.source,
            CapabilityClass::Gamepad,
            PortDirection::Source,
            ProfileId::gamepad_state_v1(),
        ),
        (
            route.sink,
            CapabilityClass::Haptics,
            PortDirection::Sink,
            ProfileId::haptics_feedback_v1(),
        ),
    ] {
        if endpoint.node_id != snapshot.local_node.id {
            continue;
        }
        let Some(cap) = snapshot
            .local_node
            .capabilities
            .get(&endpoint.capability_id)
        else {
            continue;
        };
        if cap.class == class
            && cap
                .ports
                .get(&endpoint.port_id)
                .is_some_and(|port| port.direction == direction && port.profile == profile)
            && route.profile == profile
        {
            return Some(endpoint);
        }
    }
    None
}

impl TouchpadConsentGate {
    pub(crate) fn requires_confirmation(runtime: &NodeRuntime, route: &Route) -> bool {
        consent_port(runtime, route).is_some()
    }

    pub(crate) fn expire(&mut self, runtime: &mut NodeRuntime, now_ms: u64) {
        self.pending.retain(|id, expires| {
            let Ok(route) = runtime.route(*id) else {
                return false;
            };
            let ready = runtime
                .session(route.session_id)
                .is_ok_and(|s| s.state == SessionState::Ready);
            if !ready || *expires <= now_ms || consent_port(runtime, route).is_none() {
                let _ = runtime.deny_pending_route(*id);
                return false;
            }
            route.authorization == AuthorizationState::Pending && route.state == RouteState::Draft
        });
    }

    // Called only after CatalogControlLink has validated and admitted the exact proposal.
    pub(crate) fn enqueue(
        &mut self,
        runtime: &mut NodeRuntime,
        route_id: RouteId,
        now_ms: u64,
    ) -> Result<(), String> {
        self.expire(runtime, now_ms);
        let route = runtime.route(route_id).map_err(|e| e.to_string())?;
        if consent_port(runtime, route).is_none() {
            return Err("not a local touchpad/gamepad Source or haptics Sink request".into());
        }
        if route.authorization != AuthorizationState::Pending {
            return Ok(());
        }
        if self.pending.contains_key(&route_id) {
            return Ok(());
        }
        if self.pending.len() >= MAX_PENDING {
            runtime
                .deny_pending_route(route_id)
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        self.pending
            .insert(route_id, now_ms.saturating_add(REQUEST_TTL_MS));
        Ok(())
    }

    pub(crate) fn requests(
        &mut self,
        runtime: &mut NodeRuntime,
        now_ms: u64,
    ) -> Vec<ConsentRequest> {
        self.expire(runtime, now_ms);
        let snapshot = runtime.snapshot();
        self.pending
            .iter()
            .filter_map(|(id, expires)| {
                let route = runtime.route(*id).ok()?;
                let session = runtime.session(route.session_id).ok()?;
                let peer = snapshot
                    .peers
                    .iter()
                    .find(|p| p.id == session.remote_node_id)?;
                let capability = snapshot
                    .local_node
                    .capabilities
                    .get(&consent_port(runtime, route)?.capability_id)?;
                Some(ConsentRequest {
                    route_id: id.to_string(),
                    peer_node_id: peer.id.to_string(),
                    peer_name: peer.display_name.clone(),
                    capability_name: capability.display_name.clone(),
                    capability_id: capability.id.to_string(),
                    resource_kind: if capability.class == CapabilityClass::Haptics {
                        "haptics"
                    } else if capability.class == CapabilityClass::Microphone {
                        "microphone"
                    } else if capability.class == CapabilityClass::Speaker {
                        "speaker"
                    } else if capability.class == CapabilityClass::Display {
                        "display"
                    } else if capability.class == CapabilityClass::Camera {
                        "camera"
                    } else if capability.class == CapabilityClass::Gamepad {
                        "gamepad"
                    } else {
                        "touchpad"
                    },
                    expires_at_ms: *expires,
                    profile_id: crate::secure_speaker_policy::selection(runtime, route)
                        .ok()
                        .map(|_| crate::secure_speaker_policy::PROFILE),
                    selection_revision: crate::secure_speaker_policy::selection(runtime, route)
                        .ok()
                        .map(|revision| revision.to_string()),
                })
            })
            .collect()
    }

    pub(crate) fn resolve(
        &mut self,
        runtime: &mut NodeRuntime,
        route_id: RouteId,
        allow: bool,
        now_ms: u64,
    ) -> Result<(), String> {
        self.resolve_bounded(runtime, route_id, allow, now_ms, None)
    }

    pub(crate) fn resolve_bounded(
        &mut self,
        runtime: &mut NodeRuntime,
        route_id: RouteId,
        allow: bool,
        now_ms: u64,
        peer_deadline: Option<u64>,
    ) -> Result<(), String> {
        self.expire(runtime, now_ms);
        if !self.pending.contains_key(&route_id) {
            return Err("request expired or already resolved".into());
        }
        let route = runtime.route(route_id).map_err(|e| e.to_string())?.clone();
        if allow {
            let local_deadline = now_ms.saturating_add(GRANT_TTL_MS);
            let deadline = peer_deadline.map_or(local_deadline, |peer| peer.min(local_deadline));
            if deadline <= now_ms {
                return Err("peer resource grant expired".into());
            }
            // Independent owner policy must be established before granting authority.
            runtime
                .set_local_port_lease_policy(
                    consent_port(runtime, &route).ok_or("local consent endpoint is unavailable")?,
                    PortLeasePolicy::Exclusive,
                )
                .map_err(|e| e.to_string())?;
            runtime
                .authorize_route(route_id, Some(deadline))
                .map_err(|e| e.to_string())?;
        } else {
            runtime
                .deny_pending_route(route_id)
                .map_err(|e| e.to_string())?;
        }
        self.pending.remove(&route_id);
        Ok(())
    }
}
