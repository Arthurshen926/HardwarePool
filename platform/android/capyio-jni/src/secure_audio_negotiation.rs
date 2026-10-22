//! Exact secure-mono control policy; no UDP endpoint or Java credential handoff.
use crate::AndroidCatalogRuntimeV1;
use capyio_core::*;
use capyio_protocol::{SECURE_AUDIO_MONO_FORMAT, ValidatedRouteStatus};

#[cfg(any(target_os = "android", test))]
#[derive(Clone, Debug)]
pub(crate) struct SecureAudioUse {
    pub(crate) binding: capyio_control_tls::audio::AudioTlsBinding,
    pub(crate) generation: crate::audio_job::GenerationClaim,
    pub(crate) terminal: Option<i32>,
}

pub(crate) fn selected(route: &Route) -> bool {
    route.selected_format == Some(FormatDescriptor::new(SECURE_AUDIO_MONO_FORMAT))
}
pub(crate) fn proposed(route: &Route) -> bool {
    selected(route)
        || (route.selected_format.is_none()
            && route.compatible_formats == [FormatDescriptor::new(SECURE_AUDIO_MONO_FORMAT)])
}
pub(crate) fn direction<'a>(route: &Route, legacy: &'a str) -> &'a str {
    if legacy == "microphone" && proposed(route) {
        "microphone-secure"
    } else {
        legacy
    }
}

#[cfg(any(target_os = "android", test))]
pub(crate) fn config() -> serde_json::Value {
    serde_json::json!({
        "schemaVersion":2,"transport":"pinned-tls13-mono-v1",
        "format":SECURE_AUDIO_MONO_FORMAT,"pcmBytesPerFrame":960,
        "sampleRateHz":48000,"channels":1,"samplesPerFrame":480
    })
}

/// A peer can select only the exact advertised secure Source. This copies no
/// grant and opens no hardware. Start remains a separate authenticated command.
pub(crate) fn apply_selection(
    host: &mut AndroidCatalogRuntimeV1,
    status: &ValidatedRouteStatus,
    now: u64,
) -> Result<(), String> {
    let peer = &status.route;
    if !selected(peer) {
        return Ok(());
    }
    let local = host
        .runtime
        .route(peer.id)
        .map_err(|e| e.to_string())?
        .clone();
    if !crate::audio_negotiation::microphone_source(&host.runtime, &local)
        || !local
            .compatible_formats
            .contains(&FormatDescriptor::new(SECURE_AUDIO_MONO_FORMAT))
        || peer.selected_qos_mode != Some(QosMode::Interactive)
        || peer.authorization != local.authorization
        || peer.source != local.source
        || peer.sink != local.sink
        || peer.session_id != local.session_id
        || peer.profile != local.profile
        || peer.backend != local.backend
    {
        return Err("secure audio selection differs from local consent or catalog".into());
    }
    if local.state == RouteState::Draft
        && peer.state == RouteState::Prepared
        && peer.epoch == local.epoch
    {
        crate::audio_negotiation::secure_source_authority(&host.runtime, &local, now)
            .ok_or("secure audio Source lease absent")?;
        host.runtime
            .prepare_route(
                local.id,
                Some(FormatDescriptor::new(SECURE_AUDIO_MONO_FORMAT)),
                QosMode::Interactive,
                now,
            )
            .map_err(|e| e.to_string())?;
    } else if !selected(&local) || peer.epoch != local.epoch {
        return Err("secure audio selection requires the current prepared generation".into());
    }
    Ok(())
}

impl AndroidCatalogRuntimeV1 {
    pub fn begin_secure_audio_use(&mut self, peer: NodeId) -> Result<Route, String> {
        let snapshot = self.runtime.snapshot();
        if snapshot.routes.iter().any(|route| {
            crate::audio_negotiation::local_port(self, route)
                .is_some_and(|(_, d)| d == "microphone")
                && !matches!(
                    route.state,
                    RouteState::Stopped | RouteState::Offline | RouteState::Failed
                )
                && route.authorization != AuthorizationState::Denied
        }) {
            return Err("microphone already has a pending or running Route".into());
        }
        let peer = snapshot
            .peers
            .iter()
            .find(|n| n.id == peer)
            .ok_or("peer catalog absent")?;
        let mut sessions = snapshot.sessions.iter().filter(|s| {
            s.local_node_id == snapshot.local_node.id
                && s.remote_node_id == peer.id
                && s.state == SessionState::Ready
        });
        let session = sessions.next().ok_or("peer is not connected")?;
        if sessions.next().is_some() {
            return Err("ambiguous ready Sessions".into());
        }
        let source = crate::audio_selection::select_secure_endpoint(&snapshot.local_node, true)?;
        let sink = crate::audio_selection::select_secure_endpoint(peer, false)?;
        let id = self
            .runtime
            .create_route(session.id, source, sink, RouteBackend::AdapterManaged)
            .map_err(|e| e.to_string())?;
        self.audio_local_intent.insert(id);
        self.audio_status_pending.insert(id);
        self.runtime.route(id).cloned().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_protocol::ValidatedRouteStatus;
    fn fixture() -> (AndroidCatalogRuntimeV1, RouteId) {
        let json = include_str!("../../fixtures/android-core-catalog-v1.json")
            .replace("pcm-s16le-48000-mono", SECURE_AUDIO_MONO_FORMAT);
        let mut host = AndroidCatalogRuntimeV1::new(&json).unwrap();
        let local = host.snapshot().local_node;
        let source = crate::audio_selection::select_secure_endpoint(&local, true).unwrap();
        let mut peer = local.clone();
        peer.id = NodeId::new();
        let cap = peer.capabilities.get_mut(&source.capability_id).unwrap();
        cap.class = CapabilityClass::Speaker;
        cap.ports.get_mut(&source.port_id).unwrap().direction = PortDirection::Sink;
        host.runtime.register_peer(peer.clone(), true).unwrap();
        host.runtime.open_session(peer.id).unwrap();
        let route = host.begin_secure_audio_use(peer.id).unwrap();
        (host, route.id)
    }
    fn selected_status(route: &Route) -> ValidatedRouteStatus {
        let mut peer = route.clone();
        peer.state = RouteState::Prepared;
        peer.selected_format = Some(FormatDescriptor::new(SECURE_AUDIO_MONO_FORMAT));
        peer.selected_qos_mode = Some(QosMode::Interactive);
        ValidatedRouteStatus {
            message_id: MessageId::new(),
            session_id: route.session_id,
            route: peer,
            native_audio_data: None,
            camera_data: None,
            secure_camera_data: None,
            secure_audio_data: None,
            touchpad_data: None,
            haptics_data: None,
        }
    }
    #[test]
    fn secure_dto_is_separate_and_contains_no_endpoint_or_authority() {
        let config = config();
        assert_eq!(config.as_object().unwrap().len(), 7);
        assert_eq!(config["schemaVersion"], 2);
        assert_eq!(config["transport"], "pinned-tls13-mono-v1");
        assert_eq!(config["format"], SECURE_AUDIO_MONO_FORMAT);
        assert_eq!(config["pcmBytesPerFrame"], 960);
        for field in [
            "host",
            "port",
            "peerPort",
            "streamId",
            "epoch",
            "expiresAtMs",
            "certificate",
            "privateKey",
        ] {
            assert!(config.get(field).is_none());
        }
    }
    #[test]
    fn secure_selection_requires_exact_opt_in_and_never_falls_back_to_legacy() {
        let (mut host, id) = fixture();
        let route = host.runtime.route(id).unwrap().clone();
        assert!(proposed(&route));
        assert_eq!(direction(&route, "microphone"), "microphone-secure");
        assert!(
            host.begin_audio_use(route.sink.node_id, "microphone")
                .is_err()
        );
        assert!(
            crate::audio_selection::select_endpoint(
                &host.snapshot().local_node,
                crate::audio_selection::AudioDirection::Microphone,
                true
            )
            .is_err()
        );
        assert!(host.secure_audio_uses.is_empty());
        host.stop_audio_direction("microphone-secure").unwrap();
        assert_eq!(
            host.runtime.route(id).unwrap().authorization,
            AuthorizationState::Denied
        );
    }
    #[test]
    fn prepared_selection_cannot_create_consent_or_source_lease() {
        let (mut host, id) = fixture();
        let pending = host.runtime.route(id).unwrap().clone();
        assert!(apply_selection(&mut host, &selected_status(&pending), 100).is_err());
        host.enqueue_touchpad_request(id, 100).unwrap();
        host.resolve_touchpad_request(id, true, 101).unwrap();
        let authorized = host.runtime.route(id).unwrap().clone();
        assert!(apply_selection(&mut host, &selected_status(&authorized), 102).is_err());
        assert!(
            host.runtime
                .route_port_lease_claim(id, authorized.source)
                .is_none()
        );
        host.runtime
            .acquire_route_port_lease(id, authorized.source, 2000, 102)
            .unwrap();
        let status = selected_status(&authorized);
        apply_selection(&mut host, &status, 103).unwrap();
        assert_eq!(host.runtime.route(id).unwrap().state, RouteState::Prepared);
        assert!(selected(host.runtime.route(id).unwrap()));
        assert!(host.secure_audio_uses.is_empty());
        assert!(!host.audio_should_run(id, 103));
    }
    #[test]
    fn peer_selection_cannot_change_grant_epoch_qos_or_source_binding() {
        for mutate in [
            (|r: &mut Route| r.epoch += 1) as fn(&mut Route),
            |r| r.selected_qos_mode = Some(QosMode::Basic),
            |r| r.source.port_id = PortId::new(),
            |r| {
                r.authorization = AuthorizationState::Authorized {
                    expires_at_ms: Some(9999),
                }
            },
            |r| r.profile = ProfileId::video_frames_v1(),
        ] {
            let (mut host, id) = fixture();
            host.enqueue_touchpad_request(id, 100).unwrap();
            host.resolve_touchpad_request(id, true, 101).unwrap();
            let route = host.runtime.route(id).unwrap().clone();
            host.runtime
                .acquire_route_port_lease(id, route.source, 2000, 102)
                .unwrap();
            let mut status = selected_status(&route);
            mutate(&mut status.route);
            assert!(apply_selection(&mut host, &status, 103).is_err());
            assert_eq!(host.runtime.route(id).unwrap(), &route);
        }
    }
    #[test]
    fn runtime_generation_claim_survives_other_invalid_route_and_worker_retirement() {
        let (mut host, id) = fixture();
        let route = host.runtime.route(id).unwrap();
        let binding = capyio_control_tls::audio::AudioTlsBinding {
            session: capyio_control_tls::ControlTlsBinding::new(
                route.session_id,
                route.sink.node_id,
                route.source.node_id,
            )
            .unwrap(),
            route_id: id,
            source: route.source,
            sink: route.sink,
            stream_id: StreamId::new(),
            epoch: 1,
            expires_at_ms: 2000,
        };
        host.secure_audio_uses.insert(
            id,
            SecureAudioUse {
                binding,
                generation: Default::default(),
                terminal: None,
            },
        );
        host.secure_audio_uses
            .get_mut(&id)
            .unwrap()
            .generation
            .reserve()
            .unwrap();
        assert!(host.secure_audio_uses.get_mut(&RouteId::new()).is_none());
        assert_eq!(host.secure_audio_uses.get(&id).unwrap().binding, binding);
        assert_eq!(host.secure_audio_uses.get(&id).unwrap().terminal, None);
        assert!(
            host.secure_audio_uses
                .get_mut(&id)
                .unwrap()
                .generation
                .reserve()
                .is_err()
        );
    }
}
