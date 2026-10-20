use crate::AndroidCatalogRuntimeV1;
use capyio_control_link::CatalogControlLink;
use capyio_core::*;
use capyio_protocol::{NativeAudioDataOffer, NativeAudioEndpoint, NativeAudioPreset};

pub(crate) fn local_port(
    host: &AndroidCatalogRuntimeV1,
    route: &Route,
) -> Option<(PortRef, &'static str)> {
    let local = host.runtime.snapshot().local_node;
    for (port, class, direction) in [
        (route.source, CapabilityClass::Microphone, "microphone"),
        (route.sink, CapabilityClass::Speaker, "speaker"),
    ] {
        if port.node_id == local.id
            && local
                .capabilities
                .get(&port.capability_id)
                .is_some_and(|cap| cap.class == class)
            && route.profile == ProfileId::audio_frames_v1()
        {
            return Some((port, direction));
        }
    }
    None
}

pub(crate) fn authority(host: &AndroidCatalogRuntimeV1, route: &Route, now: u64) -> Option<u64> {
    let (port, _) = local_port(host, route)?;
    let AuthorizationState::Authorized {
        expires_at_ms: Some(expiry),
    } = route.authorization
    else {
        return None;
    };
    (expiry > now
        && host
            .runtime
            .session(route.session_id)
            .is_ok_and(|s| s.state == SessionState::Ready)
        && host
            .runtime
            .route_port_lease_claim(route.id, port)
            .is_some_and(|lease| lease.expires_at_ms > now))
    .then_some(expiry)
}

pub(crate) fn receive_offer(
    host: &mut AndroidCatalogRuntimeV1,
    peer: &Route,
    offer: NativeAudioDataOffer,
    address: String,
    now: u64,
) -> Result<(), String> {
    let local = host.runtime.route(peer.id).map_err(|e| e.to_string())?;
    let (_, direction) = local_port(host, local).ok_or("audio offer is not for local hardware")?;
    if authority(host, local, now).is_none()
        || local.authorization != peer.authorization
        || local.session_id != peer.session_id
        || local.source != peer.source
        || local.sink != peer.sink
        || local.profile != peer.profile
        || local.backend != peer.backend
        || offer.owner(peer) == local.source.node_id && direction == "microphone"
        || offer.owner(peer) == local.sink.node_id && direction == "speaker"
        || offer.preset
            != if direction == "microphone" {
                NativeAudioPreset::PcmS16le48000Mono
            } else {
                NativeAudioPreset::PcmS16le48000Stereo
            }
    {
        return Err("audio offer lacks current local authority or preset".into());
    }
    if let Some(existing) = host.audio_uses.get(&peer.id) {
        if existing.offer != offer || existing.peer_route.epoch != peer.epoch {
            return Err("audio offer changed within one Route epoch".into());
        }
        return Ok(());
    }
    host.audio_uses.retain(|id, _| {
        host.runtime.route(*id).is_ok_and(|r| {
            !matches!(
                r.state,
                RouteState::Stopped | RouteState::Offline | RouteState::Failed
            )
        })
    });
    if host.audio_uses.len() >= 2 {
        return Err("audio hardware is busy".into());
    }
    host.audio_uses.insert(
        peer.id,
        crate::AndroidAudioUse {
            peer_route: peer.clone(),
            offer,
            peer_address: address,
            local_port: None,
            offer_pending: false,
            observed_state: local.state,
        },
    );
    Ok(())
}

pub(crate) fn remote_status(
    host: &mut AndroidCatalogRuntimeV1,
    peer: &Route,
) -> Result<(), String> {
    if local_port(host, peer).is_none() {
        return Ok(());
    }
    let local = host
        .runtime
        .route(peer.id)
        .map_err(|e| e.to_string())?
        .clone();
    if peer.authorization == AuthorizationState::Denied
        && local.authorization == AuthorizationState::Pending
    {
        host.runtime
            .deny_pending_route(peer.id)
            .map_err(|e| e.to_string())?;
    }
    if matches!(
        peer.state,
        RouteState::Stopped | RouteState::Offline | RouteState::Failed
    ) && matches!(
        local.state,
        RouteState::Prepared | RouteState::Starting | RouteState::Active
    ) {
        host.runtime
            .begin_route_stop(peer.id)
            .map_err(|e| e.to_string())?;
        host.runtime
            .stop_route(peer.id)
            .map_err(|e| e.to_string())?;
    } else if matches!(
        peer.state,
        RouteState::Stopped | RouteState::Offline | RouteState::Failed
    ) && local.state == RouteState::Draft
        && matches!(local.authorization, AuthorizationState::Authorized { .. })
    {
        // A receiver may terminate after the offer but before Start. Draft is
        // not stoppable in core; terminate this authorized preparation explicitly.
        fail(host, peer.id)?;
    }
    Ok(())
}

pub(crate) fn publish(
    host: &mut AndroidCatalogRuntimeV1,
    link: &mut CatalogControlLink,
    now: u64,
) -> Result<(), String> {
    for id in std::mem::take(&mut host.audio_status_pending) {
        let route = host.runtime.route(id).map_err(|e| e.to_string())?;
        if route.session_id == link.session_id() {
            let selected = host.published_source_capabilities();
            link.send_catalog(
                link.prepare_catalog(&host.runtime, &selected)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            link.send_route_status(route).map_err(|e| e.to_string())?;
        } else {
            host.audio_status_pending.insert(id);
        }
    }
    let ids: Vec<_> = host
        .audio_uses
        .iter()
        .filter(|(_, job)| job.offer_pending)
        .map(|(id, _)| *id)
        .collect();
    for id in ids {
        let route = host.runtime.route(id).map_err(|e| e.to_string())?.clone();
        if route.session_id != link.session_id() || authority(host, &route, now).is_none() {
            continue;
        }
        let job = host.audio_uses.get_mut(&id).ok_or("audio job missing")?;
        let mut advertisement = job.peer_route.clone();
        advertisement.authorization = route.authorization;
        let offer = NativeAudioDataOffer {
            port: job.local_port.ok_or("audio port not prepared")?,
            stream_id: job.offer.stream_id,
            endpoint: if job.offer.endpoint == NativeAudioEndpoint::Source {
                NativeAudioEndpoint::Sink
            } else {
                NativeAudioEndpoint::Source
            },
            preset: job.offer.preset,
        };
        link.send_native_audio_data_offer(&advertisement, offer, now)
            .map_err(|e| e.to_string())?;
        job.offer_pending = false;
    }
    Ok(())
}

pub(crate) fn fail(host: &mut AndroidCatalogRuntimeV1, id: RouteId) -> Result<(), String> {
    fail_message(host, id, "音频硬件启动失败或授权已结束")
}

pub(crate) fn fail_message(
    host: &mut AndroidCatalogRuntimeV1,
    id: RouteId,
    message: &str,
) -> Result<(), String> {
    host.runtime
        .report_route_offline(
            id,
            Problem {
                id: ProblemId::new(),
                code: "CAPY.ANDROID.AUDIO_ROUTE_FAILED".into(),
                category: ProblemCategory::Adapter,
                severity: ProblemSeverity::Error,
                retryable: true,
                related_node: None,
                related_adapter: None,
                related_route: Some(id),
                human_message: message.into(),
                technical_detail: None,
            },
        )
        .map_err(|e| e.to_string())?;
    host.audio_status_pending.insert(id);
    Ok(())
}

impl AndroidCatalogRuntimeV1 {
    /// Process an audio status received through this authenticated catalog link.
    /// This imports only Draft/Pending proposals. Consent remains local; offers
    /// cannot create grants or leases. The address must be the connected peer's
    /// socket address, never peer-controlled offer metadata.
    pub fn receive_audio_route_status(
        &mut self,
        link: &CatalogControlLink,
        status: &capyio_protocol::ValidatedRouteStatus,
        authenticated_peer_address: &str,
        now: u64,
    ) -> Result<Route, String> {
        if local_port(self, &status.route).is_none() {
            return Err("audio status is not for local hardware".into());
        }
        let confirm = self.resource_requires_confirmation(&status.route);
        let route = link
            .apply_route_status(&mut self.runtime, status, !confirm)
            .map_err(|e| e.to_string())?;
        self.apply_audio_status_policy(status, authenticated_peer_address, now, route, confirm)
    }

    fn apply_audio_status_policy(
        &mut self,
        status: &capyio_protocol::ValidatedRouteStatus,
        authenticated_peer_address: &str,
        now: u64,
        route: Route,
        confirm: bool,
    ) -> Result<Route, String> {
        if status.native_audio_data.is_none()
            && route.state == RouteState::Draft
            && status.route.state == RouteState::Draft
            && let (
                AuthorizationState::Authorized {
                    expires_at_ms: Some(local_expiry),
                },
                AuthorizationState::Authorized {
                    expires_at_ms: Some(peer_expiry),
                },
            ) = (route.authorization, status.route.authorization)
        {
            if route.session_id != status.route.session_id
                || route.source != status.route.source
                || route.sink != status.route.sink
                || route.profile != status.route.profile
                || route.backend != status.route.backend
                || route.epoch != status.route.epoch
                || peer_expiry <= now
                || local_expiry <= now
                || peer_expiry > local_expiry
            {
                return Err("audio Draft grant does not match existing local consent".into());
            }
            // An equal expiry is an ordinary non-mutating status acknowledgement,
            // including duplicates after endpoint preparation. Only an actual
            // restriction enters the no-job/no-retained-lease transition.
            if peer_expiry < local_expiry {
                if self.audio_uses.contains_key(&route.id) {
                    return Err("audio Draft grant cannot change after endpoint preparation".into());
                }
                self.runtime
                    .restrict_draft_authorization(route.id, peer_expiry, now)
                    .map_err(|e| e.to_string())?;
            }
        }
        if confirm && status.route.authorization != AuthorizationState::Denied {
            if self.audio_local_intent.contains(&route.id) {
                // Existing Android-initiated intent already represents local
                // consent. Keep its prior bounded-remote-grant behavior.
                if route.authorization == AuthorizationState::Pending
                    && let AuthorizationState::Authorized {
                        expires_at_ms: Some(deadline),
                    } = status.route.authorization
                {
                    self.enqueue_touchpad_request(route.id, now)?;
                    self.touchpad_consent.resolve_bounded(
                        &mut self.runtime,
                        route.id,
                        true,
                        now,
                        Some(deadline),
                    )?;
                    self.audio_status_pending.insert(route.id);
                }
            } else {
                self.enqueue_touchpad_request(route.id, now)?;
            }
        }
        if let Some(offer) = status.native_audio_data.as_deref() {
            receive_offer(
                self,
                &status.route,
                *offer,
                authenticated_peer_address.into(),
                now,
            )?;
        }
        remote_status(self, &status.route)?;
        self.runtime
            .route(route.id)
            .cloned()
            .map_err(|e| e.to_string())
    }

    /// Publish queued consent/status and prepared endpoint replies. Merely
    /// receiving an offer does not prepare hardware or activate capture.
    pub fn publish_audio_control(
        &mut self,
        link: &mut CatalogControlLink,
        now: u64,
    ) -> Result<(), String> {
        publish(self, link, now)
    }

    /// Called only after the platform adapter successfully prepared its owned
    /// endpoint. It supplies a reciprocal UDP port, not authority to capture.
    pub fn prepare_audio_endpoint(&mut self, id: RouteId, port: u16) -> Result<(), String> {
        if port == 0 {
            return Err("invalid audio port".into());
        }
        let job = self.audio_uses.get_mut(&id).ok_or("audio offer absent")?;
        job.local_port = Some(port);
        job.offer_pending = true;
        Ok(())
    }

    pub fn stage_audio_route_command(
        &mut self,
        link: &CatalogControlLink,
        command: &capyio_protocol::ValidatedRouteCommand,
        now: u64,
    ) -> Result<Route, String> {
        let route = self
            .runtime
            .route(command.route_id)
            .map_err(|e| e.to_string())?;
        if local_port(self, route).is_none() {
            return Err("not a local audio Route".into());
        }
        if command.action == capyio_protocol::RouteCommandAction::Start {
            self.require_audio_endpoint_prepared(command.route_id)?;
        }
        link.stage_route_command(&mut self.runtime, command, now)
            .map_err(|e| e.to_string())
    }

    fn require_audio_endpoint_prepared(&self, id: RouteId) -> Result<(), String> {
        let route = self.runtime.route(id).map_err(|e| e.to_string())?;
        if !matches!(
            route.state,
            RouteState::Draft | RouteState::Prepared | RouteState::Starting | RouteState::Active
        ) || !self
            .audio_uses
            .get(&id)
            .is_some_and(|job| job.local_port.is_some())
        {
            return Err("audio endpoint is not prepared".into());
        }
        Ok(())
    }

    /// Local UI Stop cancels pending and authorized preparation as well as
    /// running audio. In particular, Draft is not a reason to ignore Stop.
    pub fn stop_audio_direction(&mut self, direction: &str) -> Result<(), String> {
        if !matches!(direction, "microphone" | "speaker") {
            return Err("unknown audio direction".into());
        }
        let ids: Vec<_> = self
            .runtime
            .snapshot()
            .routes
            .iter()
            .filter(|route| local_port(self, route).is_some_and(|(_, value)| value == direction))
            .map(|route| route.id)
            .collect();
        for id in ids {
            let route = self.runtime.route(id).map_err(|e| e.to_string())?.clone();
            if route.state == RouteState::Draft {
                if route.authorization == AuthorizationState::Pending {
                    self.runtime
                        .deny_pending_route(id)
                        .map_err(|e| e.to_string())?;
                } else if matches!(route.authorization, AuthorizationState::Authorized { .. }) {
                    // Core Draft cannot transition through Stopping. Terminate
                    // its preparation using the existing offline/release path.
                    fail_message(self, id, "音频使用已由本机停止")?;
                }
            } else if matches!(
                route.state,
                RouteState::Prepared | RouteState::Starting | RouteState::Active
            ) {
                self.runtime
                    .begin_route_stop(id)
                    .map_err(|e| e.to_string())?;
                self.runtime.stop_route(id).map_err(|e| e.to_string())?;
            }
            self.audio_uses.remove(&id);
            self.audio_local_intent.remove(&id);
            self.audio_status_pending.insert(id);
        }
        Ok(())
    }

    pub fn answer_audio_port_lease(
        &mut self,
        link: &mut CatalogControlLink,
        request: capyio_control_link::ReceivedPortLeaseRequest,
        now: u64,
    ) -> Result<(), String> {
        self.runtime.expire_port_leases(now);
        link.answer_port_lease_request(&mut self.runtime, request, now)
            .map_err(|e| e.to_string())
    }

    /// Current consent and lease plus an explicit Start are required before
    /// the platform's capture/playback lifecycle may run.
    pub fn audio_should_run(&self, id: RouteId, now: u64) -> bool {
        self.runtime.route(id).is_ok_and(|route| {
            matches!(route.state, RouteState::Starting | RouteState::Active)
                && authority(self, route, now).is_some()
                && self
                    .audio_uses
                    .get(&id)
                    .is_some_and(|job| job.local_port.is_some())
        })
    }
}

impl AndroidCatalogRuntimeV1 {
    /// Explicit local user intent only: select the current advertised binding and
    /// queue a Pending Draft. This never grants the peer's Sink or starts media.
    pub fn begin_audio_use(&mut self, peer: NodeId, direction: &str) -> Result<Route, String> {
        let snapshot = self.runtime.snapshot();
        if snapshot.routes.iter().any(|route| {
            local_port(self, route).is_some_and(|(_, current)| current == direction)
                && !matches!(
                    route.state,
                    RouteState::Stopped | RouteState::Offline | RouteState::Failed
                )
                && route.authorization != AuthorizationState::Denied
        }) {
            return Err("audio direction already has a pending or running Route".into());
        }
        let direction = crate::audio_selection::AudioDirection::parse(direction)?;
        let peer = snapshot
            .peers
            .iter()
            .find(|node| node.id == peer)
            .ok_or("peer catalog is absent")?;
        let (session, source, sink) = crate::audio_selection::select_route(
            &snapshot.local_node,
            peer,
            &snapshot.sessions,
            direction,
        )?;
        let id = self
            .runtime
            .create_route(session, source, sink, RouteBackend::AdapterManaged)
            .map_err(|error| error.to_string())?;
        self.audio_local_intent.insert(id);
        self.audio_status_pending.insert(id);
        self.runtime
            .route(id)
            .cloned()
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_protocol::ValidatedRouteStatus;

    fn setup() -> (AndroidCatalogRuntimeV1, RouteId) {
        let mut host = AndroidCatalogRuntimeV1::new(include_str!(
            "../../fixtures/android-core-catalog-v1.json"
        ))
        .unwrap();
        let local = host.snapshot().local_node;
        let cap = local
            .capabilities
            .values()
            .find(|cap| cap.class == CapabilityClass::Microphone)
            .unwrap();
        let source_port = cap.ports.values().next().unwrap();
        let source = PortRef {
            node_id: local.id,
            capability_id: cap.id,
            port_id: source_port.id,
        };
        let mut peer = local.clone();
        peer.id = NodeId::new();
        let sink_cap = peer
            .capabilities
            .values_mut()
            .find(|cap| cap.class == CapabilityClass::Microphone)
            .unwrap();
        sink_cap.class = CapabilityClass::Speaker;
        let sink_port = sink_cap.ports.values_mut().next().unwrap();
        sink_port.direction = PortDirection::Sink;
        let sink = PortRef {
            node_id: peer.id,
            capability_id: sink_cap.id,
            port_id: sink_port.id,
        };
        host.runtime.register_peer(peer, true).unwrap();
        let session = host.runtime.open_session(sink.node_id).unwrap();
        let id = host
            .runtime
            .create_route(session, source, sink, RouteBackend::AdapterManaged)
            .unwrap();
        (host, id)
    }

    fn status(route: Route) -> ValidatedRouteStatus {
        ValidatedRouteStatus {
            message_id: MessageId::new(),
            session_id: route.session_id,
            route,
            touchpad_data: None,
            haptics_data: None,
            native_audio_data: None,
            camera_data: None,
        }
    }

    fn apply(
        host: &mut AndroidCatalogRuntimeV1,
        peer: &ValidatedRouteStatus,
        now: u64,
    ) -> Result<Route, String> {
        let local = host.runtime.route(peer.route.id).unwrap().clone();
        host.apply_audio_status_policy(peer, "127.0.0.1", now, local, true)
    }

    #[test]
    fn audio_policy_requires_explicit_consent_before_finite_narrowing() {
        let (mut host, id) = setup();
        let pending = status(host.runtime.route(id).unwrap().clone());
        apply(&mut host, &pending, 100).unwrap();
        assert_eq!(
            host.runtime.route(id).unwrap().authorization,
            AuthorizationState::Pending
        );
        assert!(!host.audio_should_run(id, 100));
        host.resolve_touchpad_request(id, true, 101).unwrap();
        let mut peer = status(host.runtime.route(id).unwrap().clone());
        peer.route.authorization = AuthorizationState::Authorized {
            expires_at_ms: Some(1000),
        };
        let narrowed = apply(&mut host, &peer, 102).unwrap();
        assert_eq!(narrowed.authorization, peer.route.authorization);
        assert_eq!(narrowed.state, RouteState::Draft);
        assert!(host.audio_uses.is_empty());
        assert!(!host.audio_should_run(id, 102));
        assert_eq!(
            apply(&mut host, &peer, 103).unwrap().authorization,
            peer.route.authorization
        );
    }

    #[test]
    fn audio_policy_rejects_grant_extension_expiry_mismatch_and_existing_lease() {
        let (mut host, id) = setup();
        let pending = status(host.runtime.route(id).unwrap().clone());
        apply(&mut host, &pending, 100).unwrap();
        host.resolve_touchpad_request(id, true, 101).unwrap();
        let baseline = host.runtime.route(id).unwrap().clone();
        let AuthorizationState::Authorized {
            expires_at_ms: Some(expiry),
        } = baseline.authorization
        else {
            panic!("finite grant")
        };
        for deadline in [expiry + 1, 102] {
            let mut peer = status(baseline.clone());
            peer.route.authorization = AuthorizationState::Authorized {
                expires_at_ms: Some(deadline),
            };
            assert!(apply(&mut host, &peer, 102).is_err());
            assert_eq!(host.runtime.route(id).unwrap(), &baseline);
        }
        let mut peer = status(baseline.clone());
        peer.route.authorization = AuthorizationState::Authorized {
            expires_at_ms: Some(1000),
        };
        peer.route.sink.port_id = PortId::new();
        assert!(apply(&mut host, &peer, 102).is_err());
        peer.route.sink = baseline.sink;
        host.runtime
            .acquire_route_port_lease(id, baseline.source, 2000, 102)
            .unwrap();
        assert!(apply(&mut host, &peer, 103).is_err());
        assert_eq!(host.runtime.route(id).unwrap(), &baseline);
    }

    #[test]
    fn audio_offer_requires_lease_and_preserves_dynamic_binding_without_start() {
        let (mut host, id) = setup();
        let pending = status(host.runtime.route(id).unwrap().clone());
        apply(&mut host, &pending, 100).unwrap();
        host.resolve_touchpad_request(id, true, 101).unwrap();
        let mut peer = host.runtime.route(id).unwrap().clone();
        peer.selected_format = Some(FormatDescriptor::new("pcm-s16le-48000-mono"));
        peer.selected_qos_mode = Some(QosMode::Interactive);
        peer.state = RouteState::Starting;
        peer.epoch = 1;
        let offer = NativeAudioDataOffer {
            port: 43210,
            stream_id: StreamId::new(),
            endpoint: NativeAudioEndpoint::Sink,
            preset: NativeAudioPreset::PcmS16le48000Mono,
        };
        assert!(receive_offer(&mut host, &peer, offer, "127.0.0.1".into(), 102).is_err());
        host.runtime
            .acquire_route_port_lease(id, peer.source, 2000, 102)
            .unwrap();
        receive_offer(&mut host, &peer, offer, "127.0.0.1".into(), 103).unwrap();
        host.prepare_audio_endpoint(id, 43211).unwrap();
        let job = host.audio_uses.get(&id).unwrap();
        assert_eq!(job.offer.stream_id, offer.stream_id);
        assert_eq!(job.peer_route.epoch, 1);
        assert_eq!(job.local_port, Some(43211));
        assert!(!host.audio_should_run(id, 103));
    }

    #[test]
    fn audio_local_stop_cancels_authorized_draft_and_rejects_delayed_start() {
        let (mut host, id) = setup();
        let pending = status(host.runtime.route(id).unwrap().clone());
        apply(&mut host, &pending, 100).unwrap();
        host.resolve_touchpad_request(id, true, 101).unwrap();
        let mut peer = host.runtime.route(id).unwrap().clone();
        host.runtime
            .acquire_route_port_lease(id, peer.source, 2000, 102)
            .unwrap();
        peer.state = RouteState::Starting;
        peer.epoch = 1;
        let offer = NativeAudioDataOffer {
            port: 43210,
            stream_id: StreamId::new(),
            endpoint: NativeAudioEndpoint::Sink,
            preset: NativeAudioPreset::PcmS16le48000Mono,
        };
        receive_offer(&mut host, &peer, offer, "127.0.0.1".into(), 103).unwrap();
        host.prepare_audio_endpoint(id, 43211).unwrap();
        assert!(host.require_audio_endpoint_prepared(id).is_ok());
        host.stop_audio_direction("microphone").unwrap();
        assert_eq!(host.runtime.route(id).unwrap().state, RouteState::Offline);
        assert!(
            host.runtime
                .route_port_lease_claim(id, peer.source)
                .is_none()
        );
        assert!(!host.audio_uses.contains_key(&id));
        assert!(host.require_audio_endpoint_prepared(id).is_err());
        assert!(host.runtime.begin_route_start(id, 104).is_err());
        assert!(!host.audio_should_run(id, 104));
        assert!(receive_offer(&mut host, &peer, offer, "127.0.0.1".into(), 104).is_err());
        host.stop_audio_direction("microphone").unwrap();
    }
}
