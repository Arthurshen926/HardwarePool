//! Accepted-channel composition only. No listener, pairing store or credentials.
//! Existing local grants are consumed, never inferred from authenticated offers.
use crate::{PlaybackConfig, PlaybackMetrics, RuntimePlayback};
use capyio_audio::AudioMediaStreamBinding;
#[cfg(test)]
use capyio_audio::AudioStreamSpec;
use capyio_control_link::{CatalogControlLink, ReceivedControlMessage};
use capyio_control_tls::{AuthenticatedControlServerChannel, ControlTlsBinding};
use capyio_core::{AuthorizationState, Availability, PortDirection, Route, RouteId, RouteState};
use capyio_protocol::{NativeAudioDataOffer, RouteCommandAction};
#[cfg(test)]
use capyio_protocol::{NativeAudioEndpoint, NativeAudioPreset};
use capyio_runtime::{NodeRuntime, RuntimeSnapshot};
use std::{
    io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn error(value: impl ToString) -> io::Error {
    io::Error::other(value.to_string())
}
fn now_ms() -> io::Result<u64> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(error)?
            .as_millis(),
    )
    .map_err(error)
}

/// Explicit local destination only. The peer IP is never accepted from config
/// or an offer; it is obtained from the accepted, pinned TLS socket.
#[derive(Clone, Debug)]
pub struct PlaybackTarget {
    pub local: SocketAddrV4,
    pub duration: Duration,
    pub pipewire_target: String,
    pub endpoint_mode: crate::EndpointMode,
}
/// Trusted host intent, not deserializable peer input. The expected binding must
/// come from the existing paired-peer admission flow, not from an incoming offer.
#[derive(Clone, Copy, Debug)]
pub struct PlaybackAdmission {
    pub expected: ControlTlsBinding,
    pub route_id: RouteId,
    pub local_offer: NativeAudioDataOffer,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackOwnerEvent {
    PlaybackReady,
    Heartbeat,
    /// Authenticated catalog or ordinary status accepted; not media readiness.
    ControlUpdated,
    Stopped,
}

/// Owns one already-admitted, locally authorized playback Route on a dedicated
/// authenticated Session. Construction consumes a real TLS channel and checks
/// its complete expected Session/server/client identity before any send/open.
/// The caller must have configured that channel with the paired certificate pin.
///
/// Caller-driven: prefer `poll` for fair incremental control/media supervision.
/// `receive_control` pauses media polling but has an absolute
/// deadline bounded by one second, owner lifetime, grant and lease. A cancellable
/// socket watcher interrupts trickled frames too. Timeout/disconnect is terminal, not a
/// retryable idle tick. Poll media between control operations; no persistent
/// reader or between-call supervisor is promised. No public API accepts a
/// fabricated peer status.
/// `stop`/Drop close owned media and control resources. UDP is still insecure.
pub struct AuthenticatedPlaybackOwner {
    runtime: NodeRuntime,
    link: CatalogControlLink,
    admission: PlaybackAdmission,
    target: PlaybackTarget,
    peer_ip: Ipv4Addr,
    worker: Option<RuntimePlayback>,
    offered: bool,
    closed: bool,
    created: Instant,
    last_wall_ms: u64,
    peer_port: Option<u16>,
}

impl AuthenticatedPlaybackOwner {
    pub fn from_accepted_trusted_lab(
        runtime: NodeRuntime,
        channel: AuthenticatedControlServerChannel,
        admission: PlaybackAdmission,
        target: PlaybackTarget,
    ) -> io::Result<Self> {
        Self::from_admitted_link_trusted_lab(
            runtime,
            CatalogControlLink::from_accepted(channel),
            admission,
            target,
        )
    }
    /// Preserve an existing authenticated catalog/admission link. This does not
    /// create authority: the same exact binding, current grant and lease checks
    /// apply as for a newly accepted channel.
    pub fn from_admitted_link_trusted_lab(
        runtime: NodeRuntime,
        link: CatalogControlLink,
        admission: PlaybackAdmission,
        target: PlaybackTarget,
    ) -> io::Result<Self> {
        let now = now_ms()?;
        validate_admission(&runtime, link.tls_binding(), admission, &target, now)?;
        let SocketAddr::V4(peer) = link.accepted_peer_addr().map_err(error)? else {
            return Err(error(
                "Linux lab playback requires an IPv4 authenticated peer",
            ));
        };
        // Check peer address scope now, even before the peer offers its UDP port.
        crate::parse_lab_endpoint(&peer.to_string()).map_err(error)?;
        Ok(Self {
            runtime,
            link,
            admission,
            target,
            peer_ip: *peer.ip(),
            worker: None,
            offered: false,
            closed: false,
            created: Instant::now(),
            last_wall_ms: now,
            peer_port: None,
        })
    }
    /// Sends the local reciprocal offer once. This consumes an existing grant
    /// and lease; neither authentication nor this call authorizes new capture.
    pub fn send_local_offer(&mut self) -> io::Result<()> {
        self.require_open()?;
        if self.offered {
            return Ok(());
        }
        let result = (|| {
            let deadline = self.operation_deadline()?;
            let now = now_ms()?;
            crate::route::validate_start(&self.runtime, self.media(), now)?;
            let route = self.runtime.route(self.admission.route_id).map_err(error)?;
            self.link
                .send_native_audio_data_offer_until(
                    route,
                    self.admission.local_offer,
                    now,
                    deadline,
                )
                .map_err(error)
        })();
        match result {
            Ok(()) => {
                self.offered = true;
                Ok(())
            }
            Err(reason) => self.fail(reason),
        }
    }
    /// Read one actual authenticated frame. No peer-status injection method is
    /// exposed. Wrong offers, unexpected commands and framing errors close this
    /// dedicated Session and media before returning failure.
    pub fn receive_control(&mut self) -> io::Result<PlaybackOwnerEvent> {
        self.receive_with(RuntimePlayback::open_trusted_lab)
    }
    fn receive_with(
        &mut self,
        open: impl FnOnce(
            &mut NodeRuntime,
            AudioMediaStreamBinding,
            &PlaybackConfig,
        ) -> io::Result<RuntimePlayback>,
    ) -> io::Result<PlaybackOwnerEvent> {
        self.require_open()?;
        if !self.offered {
            return Err(error(
                "send the local offer before receiving playback control",
            ));
        }
        let outcome = (|| {
            let deadline = self.operation_deadline()?;
            let message = self
                .link
                .receive_control_message_until(deadline)
                .map_err(error)?;
            self.handle_control(message, deadline, open)
        })();
        match outcome {
            Ok(event) => Ok(event),
            Err(reason) => self.fail(reason),
        }
    }
    /// Fair event-loop step: one bounded incremental TLS read, at most one
    /// authenticated control message, then one bounded media poll. Idle control
    /// never blocks playback. Partial frame expiry and authority expiry are
    /// terminal. Replies still have the same <=1 second absolute write bound.
    pub fn poll(&mut self) -> io::Result<Option<PlaybackOwnerEvent>> {
        self.poll_with(RuntimePlayback::open_trusted_lab)
    }
    fn poll_with(
        &mut self,
        open: impl FnOnce(
            &mut NodeRuntime,
            AudioMediaStreamBinding,
            &PlaybackConfig,
        ) -> io::Result<RuntimePlayback>,
    ) -> io::Result<Option<PlaybackOwnerEvent>> {
        self.require_open()?;
        if !self.offered {
            return Err(error(
                "send the local offer before polling playback control",
            ));
        }
        let outcome = (|| {
            let deadline = self.operation_deadline()?;
            let event = self
                .link
                .poll_control_message_until(deadline)
                .map_err(error)?
                .map(|message| self.handle_control(message, deadline, open))
                .transpose()?;
            if event != Some(PlaybackOwnerEvent::Stopped) {
                self.validate_live()?;
                if let Some(worker) = self.worker.as_mut() {
                    worker.poll(&mut self.runtime)?;
                }
            }
            Ok(event)
        })();
        match outcome {
            Ok(event) => Ok(event),
            Err(reason) => self.fail(reason),
        }
    }
    fn handle_control(
        &mut self,
        message: ReceivedControlMessage,
        deadline: Instant,
        open: impl FnOnce(
            &mut NodeRuntime,
            AudioMediaStreamBinding,
            &PlaybackConfig,
        ) -> io::Result<RuntimePlayback>,
    ) -> io::Result<PlaybackOwnerEvent> {
        self.validate_live()?;
        match message {
            ReceivedControlMessage::Heartbeat(heartbeat) => {
                if !heartbeat.acknowledgement {
                    self.link
                        .send_heartbeat_until(heartbeat.sequence, true, deadline)
                        .map_err(error)?;
                }
                Ok(PlaybackOwnerEvent::Heartbeat)
            }
            ReceivedControlMessage::RouteCommand(command)
                if command.session_id == self.admission.expected.session_id
                    && command.route_id == self.admission.route_id
                    && command.action == RouteCommandAction::Stop =>
            {
                self.stop()?;
                Ok(PlaybackOwnerEvent::Stopped)
            }
            ReceivedControlMessage::Catalog(catalog) => {
                self.link
                    .apply_catalog(&mut self.runtime, catalog)
                    .map_err(error)?;
                validate_catalog_endpoints(
                    &self.runtime,
                    self.admission.route_id,
                    self.worker.is_some(),
                )?;
                self.validate_live()?;
                Ok(PlaybackOwnerEvent::ControlUpdated)
            }
            ReceivedControlMessage::RouteStatus(remote) => {
                let local = self.runtime.route(self.admission.route_id).map_err(error)?;
                let terminal = validate_peer_status(local, &remote.route)?;
                if terminal {
                    self.stop()?;
                    return Ok(PlaybackOwnerEvent::Stopped);
                }
                if remote.native_audio_data.is_none() {
                    return Ok(PlaybackOwnerEvent::ControlUpdated);
                }
                let preparation = self
                    .link
                    .prepare_native_audio_worker(
                        &self.runtime,
                        self.admission.route_id,
                        self.admission.local_offer,
                        &remote,
                        true,
                        now_ms()?,
                    )
                    .map_err(error)?;
                if preparation.local_endpoint != self.target.endpoint_mode.endpoint()
                    || preparation.preset != self.target.endpoint_mode.preset()
                {
                    return Err(error("only the admitted mono playback Sink is supported"));
                }
                if self.worker.is_some() {
                    if self.peer_port != Some(preparation.peer_port) {
                        return Err(error("peer changed its active UDP endpoint"));
                    }
                    return Ok(PlaybackOwnerEvent::PlaybackReady); // Exact duplicate, no restart.
                }
                let config = PlaybackConfig {
                    local: self.target.local,
                    peer: SocketAddrV4::new(self.peer_ip, preparation.peer_port),
                    duration: self.target.duration,
                    target: self.target.pipewire_target.clone(),
                    endpoint_mode: self.target.endpoint_mode,
                };
                // Same full policy validation as the real worker, before the test seam too.
                PlaybackConfig::parse(&[
                    "--trusted-lab".into(),
                    config.local.to_string(),
                    config.peer.to_string(),
                    config.duration.as_secs().to_string(),
                    config.target.clone(),
                ])
                .map_err(error)?;
                let media = self.media();
                self.worker = Some(open(&mut self.runtime, media, &config)?);
                self.peer_port = Some(preparation.peer_port);
                // Android prepares its endpoint before this command but captures only
                // after Start. Send once, after our real Sink has opened, and share
                // the same bounded operation budget with the status below.
                self.link
                    .send_route_command_until(
                        self.admission.route_id,
                        RouteCommandAction::Start,
                        deadline,
                    )
                    .map_err(error)?;
                // Report readiness only after the actual worker opened. A
                // failed status send closes media and the Session below.
                self.link
                    .send_route_status_until(
                        self.runtime.route(self.admission.route_id).map_err(error)?,
                        deadline,
                    )
                    .map_err(error)?;
                Ok(PlaybackOwnerEvent::PlaybackReady)
            }
            _ => Err(error(
                "unexpected control message on dedicated playback Session",
            )),
        }
    }
    pub fn poll_media(&mut self) -> io::Result<()> {
        self.require_open()?;
        if let Err(reason) = self.validate_live() {
            return self.fail(reason);
        }
        let Some(worker) = self.worker.as_mut() else {
            return Err(error("peer playback offer is not ready"));
        };
        match worker.poll(&mut self.runtime) {
            Ok(()) => Ok(()),
            Err(reason) => self.fail(reason),
        }
    }
    /// Stop is terminal for this dedicated Session. Reconnect requires a fresh
    /// authenticated channel, Session/Route admission and local consent owner.
    pub fn stop(&mut self) -> io::Result<()> {
        self.closed = true;
        let media = if let Some(worker) = self.worker.as_mut() {
            worker.stop(&mut self.runtime)
        } else {
            let id = self.admission.route_id;
            (|| {
                if self.runtime.route(id).map_err(error)?.state == RouteState::Starting {
                    self.runtime.begin_route_stop(id).map_err(error)?;
                }
                if self.runtime.route(id).map_err(error)?.state == RouteState::Stopping {
                    self.runtime.stop_route(id).map_err(error)?;
                }
                Ok(())
            })()
        };
        let control = self
            .link
            .close_and_reconcile(&mut self.runtime)
            .map_err(error);
        // Close control and revoke its leases even if media cleanup reports failure.
        media.and(control)
    }
    pub fn snapshot(&self) -> RuntimeSnapshot {
        self.runtime.snapshot()
    }
    pub fn metrics(&self) -> Option<PlaybackMetrics> {
        self.worker.as_ref().map(RuntimePlayback::metrics)
    }
    fn media(&self) -> AudioMediaStreamBinding {
        AudioMediaStreamBinding {
            session_id: self.admission.expected.session_id,
            route_id: self.admission.route_id,
            stream_id: self.admission.local_offer.stream_id,
            stream_epoch: self
                .runtime
                .route(self.admission.route_id)
                .expect("owned Route exists")
                .epoch as u32,
            selected_spec: self.target.endpoint_mode.spec(),
        }
    }
    fn operation_deadline(&mut self) -> io::Result<Instant> {
        self.validate_live()?;
        let route = self.runtime.route(self.admission.route_id).map_err(error)?;
        let capyio_core::AuthorizationState::Authorized {
            expires_at_ms: Some(grant),
        } = route.authorization
        else {
            return Err(error("finite local grant required"));
        };
        let lease = self
            .runtime
            .route_port_lease_claim(
                route.id,
                if self.target.endpoint_mode.capture() {
                    route.source
                } else {
                    route.sink
                },
            )
            .ok_or_else(|| error("local Sink lease missing"))?
            .expires_at_ms;
        let remaining_ms = grant
            .min(lease)
            .checked_sub(self.last_wall_ms)
            .filter(|remaining| *remaining > 0)
            .ok_or_else(|| error("local authority expired"))?;
        Ok((self.created + self.target.duration)
            .min(Instant::now() + Duration::from_millis(remaining_ms).min(Duration::from_secs(1))))
    }
    fn validate_live(&mut self) -> io::Result<()> {
        if self.created.elapsed() >= self.target.duration {
            return Err(error("playback owner lifetime expired"));
        }
        let now = now_ms()?;
        if now < self.last_wall_ms {
            return Err(error("playback owner wall clock regressed"));
        }
        self.last_wall_ms = now;
        crate::route::validate_live(&self.runtime, self.media(), now)
    }
    fn require_open(&self) -> io::Result<()> {
        if self.closed || self.link.is_closed() {
            Err(error("playback control owner is closed"))
        } else {
            Ok(())
        }
    }
    fn fail<T>(&mut self, reason: io::Error) -> io::Result<T> {
        self.closed = true;
        // Invalidate Session/epoch before cleanup; never leave authority active
        // because a child failed to exit. Other Sessions are not touched.
        let control = self
            .link
            .close_and_reconcile(&mut self.runtime)
            .map_err(error);
        let media = self
            .worker
            .as_mut()
            .map(|worker| worker.stop(&mut self.runtime))
            .unwrap_or(Ok(()));
        media.and(control).and(Err(reason))
    }
}
impl Drop for AuthenticatedPlaybackOwner {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Ordinary peer status is not a reciprocal offer, consent, or readiness.
/// Android's Draft acknowledgements retain epoch zero until Start; never copy
/// them over the negotiated local Route or offer.
fn validate_peer_status(local: &Route, peer: &Route) -> io::Result<bool> {
    if peer.id != local.id
        || peer.session_id != local.session_id
        || peer.source != local.source
        || peer.sink != local.sink
        || peer.profile != local.profile
        || peer.backend != local.backend
    {
        return Err(error("peer status differs from admitted playback Route"));
    }
    let terminal = matches!(
        peer.state,
        RouteState::Stopped | RouteState::Offline | RouteState::Failed
    ) || matches!(
        peer.authorization,
        AuthorizationState::Denied | AuthorizationState::Revoked
    );
    if terminal {
        // Terminal status can only stop this exact owned Route, never grant or open.
        return Ok(true);
    }
    if peer.authorization != local.authorization {
        return Err(error("peer status differs from the exact finite grant"));
    }
    let draft_ack = peer.state == RouteState::Draft
        && peer.epoch == 0
        && peer.selected_format.is_none()
        && peer.selected_qos_mode.is_none();
    let running_ack = matches!(peer.state, RouteState::Starting | RouteState::Active)
        && peer.epoch == local.epoch
        && peer.selected_format == local.selected_format
        && peer.selected_qos_mode == local.selected_qos_mode;
    if !draft_ack && !running_ack {
        return Err(error(
            "peer status has an unexpected playback phase or binding",
        ));
    }
    Ok(false)
}

fn validate_catalog_endpoints(
    runtime: &NodeRuntime,
    route_id: RouteId,
    started_peer: bool,
) -> io::Result<()> {
    let route = runtime.route(route_id).map_err(error)?;
    let snapshot = runtime.snapshot();
    for (reference, direction) in [
        (route.source, PortDirection::Source),
        (route.sink, PortDirection::Sink),
    ] {
        let node = if reference.node_id == snapshot.local_node.id {
            &snapshot.local_node
        } else {
            snapshot
                .peers
                .iter()
                .find(|node| node.id == reference.node_id)
                .ok_or_else(|| error("playback catalog Node disappeared"))?
        };
        let capability = node
            .capabilities
            .get(&reference.capability_id)
            .ok_or_else(|| error("playback catalog Capability disappeared"))?;
        let port = node
            .port(reference.capability_id, reference.port_id)
            .map_err(error)?;
        // AndroidCapabilityModules.audio availability maps an owned foreground
        // lifecycle (starting/active capture) to BUSY. Only our already-opened
        // exact Route may retain that remote endpoint after Start was sent. BUSY
        // is never admission, permission, or proof that another use is ours.
        let owned_remote = started_peer && reference.node_id != snapshot.local_node.id;
        let available = |value| {
            value == Availability::Available || (owned_remote && value == Availability::Busy)
        };
        if !available(capability.availability)
            || !available(port.availability)
            || port.direction != direction
            || port.profile != route.profile
            || !route
                .selected_format
                .as_ref()
                .is_some_and(|format| port.formats.contains(format))
            || !route
                .selected_qos_mode
                .as_ref()
                .is_some_and(|qos| port.qos_modes.contains(qos))
        {
            return Err(error(
                "playback catalog no longer supports the admitted endpoint",
            ));
        }
    }
    Ok(())
}

fn validate_admission(
    runtime: &NodeRuntime,
    actual: ControlTlsBinding,
    admission: PlaybackAdmission,
    target: &PlaybackTarget,
    now: u64,
) -> io::Result<()> {
    ControlTlsBinding::new(
        admission.expected.session_id,
        admission.expected.server_node_id,
        admission.expected.client_node_id,
    )
    .map_err(error)?;
    if actual != admission.expected {
        return Err(error(
            "authenticated channel differs from expected paired identity or Session",
        ));
    }
    crate::parse_lab_endpoint(&target.local.to_string()).map_err(error)?;
    crate::validate_endpoint_target(target.endpoint_mode, &target.pipewire_target)
        .map_err(error)?;
    if !(Duration::from_secs(1)..=Duration::from_secs(300)).contains(&target.duration)
        || target.duration.subsec_nanos() != 0
        || admission.local_offer.endpoint != target.endpoint_mode.endpoint()
        || admission.local_offer.preset != target.endpoint_mode.preset()
        || admission.local_offer.port != target.local.port()
    {
        return Err(error("invalid explicit Linux playback admission"));
    }
    let snapshot = runtime.snapshot();
    if snapshot.local_node.id != actual.server_node_id
        || snapshot
            .routes
            .iter()
            .filter(|route| route.session_id == actual.session_id)
            .count()
            != 1
    {
        return Err(error(
            "playback owner requires its exact local Node and a dedicated single-Route Session",
        ));
    }
    let route = runtime.route(admission.route_id).map_err(error)?;
    if target.endpoint_mode == crate::EndpointMode::VirtualMicrophone
        && target.pipewire_target != crate::virtual_microphone_name(route.sink.port_id)
    {
        return Err(error(
            "virtual microphone identity differs from admitted local Sink",
        ));
    }
    let (local, peer) = if target.endpoint_mode.capture() {
        (route.source, route.sink)
    } else {
        (route.sink, route.source)
    };
    if route.session_id != actual.session_id
        || peer.node_id != actual.client_node_id
        || local.node_id != actual.server_node_id
    {
        return Err(error("playback Route differs from paired channel binding"));
    }
    crate::route::validate_start(
        runtime,
        AudioMediaStreamBinding {
            session_id: actual.session_id,
            route_id: route.id,
            stream_id: admission.local_offer.stream_id,
            stream_epoch: u32::try_from(route.epoch).map_err(error)?,
            selected_spec: target.endpoint_mode.spec(),
        },
        now,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_control_tls::{
        AuthenticatedControlClientChannel, AuthenticatedControlServer, ControlTlsClientIdentity,
        ControlTlsEndpoint, ControlTlsServerIdentity,
    };
    use capyio_core::{NodeId, SessionId, SessionState};
    use capyio_native_audio_lan::{NativeLanEndpointConfig, NativeLanUdpEndpoint};
    use std::{
        net::{TcpListener, UdpSocket},
        thread,
    };

    fn fixture() -> (NodeRuntime, PlaybackAdmission, PlaybackTarget) {
        let (runtime, media) = crate::route::tests::fixture();
        let route = runtime.route(media.route_id).unwrap();
        let admission = PlaybackAdmission {
            expected: ControlTlsBinding::new(
                media.session_id,
                route.sink.node_id,
                route.source.node_id,
            )
            .unwrap(),
            route_id: media.route_id,
            local_offer: NativeAudioDataOffer {
                port: 40211,
                stream_id: media.stream_id,
                endpoint: NativeAudioEndpoint::Sink,
                preset: NativeAudioPreset::PcmS16le48000Mono,
            },
        };
        (
            runtime,
            admission,
            PlaybackTarget {
                local: "127.0.0.1:40211".parse().unwrap(),
                duration: Duration::from_secs(10),
                pipewire_target: "synthetic.test".into(),
                endpoint_mode: crate::EndpointMode::Playback,
            },
        )
    }
    #[test]
    fn capture_owner_requires_source_offer_and_exact_stereo_binding() {
        let (mut runtime, media) = crate::route::tests::fixture_for(true);
        let route = runtime.route(media.route_id).unwrap().clone();
        let admission = PlaybackAdmission {
            expected: ControlTlsBinding::new(
                media.session_id,
                route.source.node_id,
                route.sink.node_id,
            )
            .unwrap(),
            route_id: route.id,
            local_offer: NativeAudioDataOffer {
                port: 40211,
                stream_id: media.stream_id,
                endpoint: NativeAudioEndpoint::Source,
                preset: NativeAudioPreset::PcmS16le48000Stereo,
            },
        };
        let target = PlaybackTarget {
            local: "127.0.0.1:40211".parse().unwrap(),
            duration: Duration::from_secs(10),
            pipewire_target: "exact.approved.source".into(),
            endpoint_mode: crate::EndpointMode::StereoCapture,
        };
        validate_admission(&runtime, admission.expected, admission, &target, 1_000).unwrap();
        for case in 0..3 {
            let mut wrong = admission;
            match case {
                0 => wrong.local_offer.endpoint = NativeAudioEndpoint::Sink,
                1 => wrong.local_offer.preset = NativeAudioPreset::PcmS16le48000Mono,
                _ => wrong.expected.session_id = SessionId::new(),
            }
            assert!(
                validate_admission(&runtime, admission.expected, wrong, &target, 1_000).is_err()
            );
        }
        assert!(
            validate_admission(&runtime, admission.expected, admission, &target, 5_000).is_err()
        );
        let mut peer = runtime.snapshot().peers[0].clone();
        peer.capabilities
            .get_mut(&route.sink.capability_id)
            .unwrap()
            .availability = Availability::Busy;
        runtime.accept_peer_catalog(media.session_id, peer).unwrap();
        assert!(validate_catalog_endpoints(&runtime, route.id, false).is_err());
        validate_catalog_endpoints(&runtime, route.id, true).unwrap();
    }

    #[test]
    fn ordinary_peer_status_preserves_exact_authority_without_media_readiness() {
        let (runtime, admission, _) = fixture();
        let local = runtime.route(admission.route_id).unwrap();
        let mut peer = local.clone();
        peer.state = RouteState::Draft;
        peer.epoch = 0;
        peer.selected_format = None;
        peer.selected_qos_mode = None;
        assert!(!validate_peer_status(local, &peer).unwrap());
        let original = peer.clone();
        for change in 0..8 {
            let mut peer = original.clone();
            match change {
                0 => peer.id = RouteId::new(),
                1 => peer.session_id = SessionId::new(),
                2 => peer.source.node_id = NodeId::new(),
                3 => peer.sink.node_id = NodeId::new(),
                4 => peer.authorization = AuthorizationState::Pending,
                5 => {
                    peer.authorization = AuthorizationState::Authorized {
                        expires_at_ms: Some(999_999),
                    }
                }
                6 => peer.epoch = 1,
                _ => peer.selected_format = local.selected_format.clone(),
            }
            assert!(
                validate_peer_status(local, &peer).is_err(),
                "change {change}"
            );
        }
        assert_eq!(runtime.route(admission.route_id).unwrap(), local);
    }

    #[test]
    fn terminal_peer_status_only_stops_exact_owned_route() {
        let (runtime, admission, _) = fixture();
        let local = runtime.route(admission.route_id).unwrap();
        for terminal in 0..5 {
            let mut peer = local.clone();
            match terminal {
                0 => peer.state = RouteState::Stopped,
                1 => peer.state = RouteState::Offline,
                2 => peer.state = RouteState::Failed,
                3 => peer.authorization = AuthorizationState::Denied,
                _ => peer.authorization = AuthorizationState::Revoked,
            }
            assert!(validate_peer_status(local, &peer).unwrap());
            peer.id = RouteId::new();
            assert!(validate_peer_status(local, &peer).is_err());
        }
    }

    #[test]
    fn catalog_metadata_must_retain_available_exact_playback_endpoints() {
        let (mut runtime, admission, _) = fixture();
        let snapshot = runtime.snapshot();
        let mut local = snapshot.local_node;
        let mut peer = snapshot.peers[0].clone();
        for node in [&mut local, &mut peer] {
            for capability in node.capabilities.values_mut() {
                capability.availability = Availability::Available;
                for port in capability.ports.values_mut() {
                    port.availability = Availability::Available;
                }
            }
        }
        runtime.replace_local_catalog(local).unwrap();
        runtime
            .accept_peer_catalog(admission.expected.session_id, peer.clone())
            .unwrap();
        validate_catalog_endpoints(&runtime, admission.route_id, false).unwrap();
        validate_catalog_endpoints(&runtime, admission.route_id, true).unwrap();
        let source = runtime.route(admission.route_id).unwrap().source;
        for availability in [
            Availability::Busy,
            Availability::PermissionRequired,
            Availability::Offline,
        ] {
            let mut candidate = runtime.clone();
            let capability = peer.capabilities.get_mut(&source.capability_id).unwrap();
            capability.availability = availability;
            capability
                .ports
                .get_mut(&source.port_id)
                .unwrap()
                .availability = availability;
            candidate
                .accept_peer_catalog(admission.expected.session_id, peer.clone())
                .unwrap();
            assert!(validate_catalog_endpoints(&candidate, admission.route_id, false).is_err());
            assert_eq!(
                validate_catalog_endpoints(&candidate, admission.route_id, true).is_ok(),
                availability == Availability::Busy
            );
            if availability == Availability::Busy {
                assert_eq!(
                    candidate.route(admission.route_id).unwrap().state,
                    RouteState::Starting
                );
                crate::route::validate_live(
                    &candidate,
                    capyio_audio::AudioMediaStreamBinding {
                        session_id: admission.expected.session_id,
                        route_id: admission.route_id,
                        stream_id: admission.local_offer.stream_id,
                        stream_epoch: 1,
                        selected_spec: AudioStreamSpec::voice_interactive(),
                    },
                    1_001,
                )
                .unwrap();
            }
        }
        // Local Sink BUSY is not accepted by this remote capture exception.
        let mut local = runtime.snapshot().local_node;
        let sink = runtime.route(admission.route_id).unwrap().sink;
        local
            .capabilities
            .get_mut(&sink.capability_id)
            .unwrap()
            .availability = Availability::Busy;
        runtime.replace_local_catalog(local).unwrap();
        assert!(validate_catalog_endpoints(&runtime, admission.route_id, true).is_err());
    }

    #[test]
    fn virtual_microphone_admission_binds_owned_name_and_retains_finite_lease() {
        let (mut runtime, admission, mut target) = fixture();
        let sink = runtime.route(admission.route_id).unwrap().sink;
        target.endpoint_mode = crate::EndpointMode::VirtualMicrophone;
        target.pipewire_target = crate::virtual_microphone_name(capyio_core::PortId::new());
        assert!(
            validate_admission(&runtime, admission.expected, admission, &target, 1_000).is_err()
        );
        target.pipewire_target = crate::virtual_microphone_name(sink.port_id);
        validate_admission(&runtime, admission.expected, admission, &target, 1_000).unwrap();
        assert!(
            validate_admission(&runtime, admission.expected, admission, &target, 10_000).is_err()
        );
        runtime
            .release_route_port_lease(admission.route_id, sink, admission.expected.session_id)
            .unwrap();
        assert!(
            validate_admission(&runtime, admission.expected, admission, &target, 1_000).is_err()
        );
    }

    #[test]
    fn admission_requires_exact_paired_session_and_existing_local_lease() {
        let (mut runtime, admission, target) = fixture();
        validate_admission(&runtime, admission.expected, admission, &target, 1_000).unwrap();
        for actual in [
            ControlTlsBinding {
                session_id: SessionId::new(),
                ..admission.expected
            },
            ControlTlsBinding {
                client_node_id: NodeId::new(),
                ..admission.expected
            },
            ControlTlsBinding {
                server_node_id: NodeId::new(),
                ..admission.expected
            },
        ] {
            assert!(validate_admission(&runtime, actual, admission, &target, 1_000).is_err());
        }
        assert!(
            validate_admission(&runtime, admission.expected, admission, &target, 10_000).is_err()
        );
        let sink = runtime.route(admission.route_id).unwrap().sink;
        runtime
            .release_route_port_lease(admission.route_id, sink, admission.expected.session_id)
            .unwrap();
        assert!(
            validate_admission(&runtime, admission.expected, admission, &target, 1_000).is_err()
        );
    }
    #[test]
    fn admission_rejects_implicit_target_public_address_and_wrong_offer() {
        for case in 0..6 {
            let (runtime, mut admission, mut target) = fixture();
            match case {
                0 => target.pipewire_target = "auto".into(),
                1 => target.local = "8.8.8.8:40211".parse().unwrap(),
                2 => target.duration = Duration::ZERO,
                3 => admission.local_offer.port += 1,
                4 => admission.local_offer.endpoint = NativeAudioEndpoint::Source,
                _ => admission.local_offer.preset = NativeAudioPreset::PcmS16le48000Stereo,
            }
            assert!(
                validate_admission(&runtime, admission.expected, admission, &target, 1_000)
                    .is_err()
            );
        }
    }

    #[test]
    fn admission_rejects_shared_session_but_preserves_other_sessions() {
        let (mut runtime, admission, target) = fixture();
        let route = runtime.route(admission.route_id).unwrap().clone();
        let other_session = runtime.open_session(route.source.node_id).unwrap();
        runtime
            .create_route(other_session, route.source, route.sink, route.backend)
            .unwrap();
        validate_admission(&runtime, admission.expected, admission, &target, 1_000).unwrap();
        runtime
            .create_route(
                admission.expected.session_id,
                route.source,
                route.sink,
                route.backend,
            )
            .unwrap();
        assert!(
            validate_admission(&runtime, admission.expected, admission, &target, 1_000).is_err()
        );
        assert_eq!(
            runtime.session(other_session).unwrap().state,
            SessionState::Ready
        );
    }

    // Hosted-only fixtures use ephemeral in-memory test certificates, real TLS,
    // real UDP and an owned cat process. No credentials are persisted/provisioned.
    fn connected(timeout: Duration) -> (AuthenticatedPlaybackOwner, CatalogControlLink, UdpSocket) {
        let (mut runtime, mut admission, mut target) = fixture();
        let now = now_ms().unwrap();
        runtime
            .renew_route_authorization(admission.route_id, now + 60_000, 1_001)
            .unwrap();
        let sink = runtime.route(admission.route_id).unwrap().sink;
        runtime
            .acquire_route_port_lease(admission.route_id, sink, now + 30_000, now)
            .unwrap();
        target.duration = Duration::from_secs(30);
        // Reserve then release only the known local test port before the worker opens.
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let SocketAddr::V4(local) = receiver.local_addr().unwrap() else {
            panic!("IPv4")
        };
        target.local = local;
        admission.local_offer.port = local.port();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let (server_identity, client_identity) = identities();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = AuthenticatedControlServer::from_listener(
            listener,
            server_identity,
            admission.expected,
            timeout,
        )
        .unwrap();
        let worker = thread::spawn(move || server.accept().unwrap());
        let client = AuthenticatedControlClientChannel::connect(
            ControlTlsEndpoint {
                address,
                io_timeout: timeout,
            },
            "localhost",
            client_identity,
            admission.expected,
        )
        .unwrap();
        let server = worker.join().unwrap();
        let owner = AuthenticatedPlaybackOwner::from_accepted_trusted_lab(
            runtime, server, admission, target,
        )
        .unwrap();
        drop(receiver);
        (owner, CatalogControlLink::from_dialed(client), sender)
    }
    fn reciprocal(
        owner: &mut AuthenticatedPlaybackOwner,
        peer: &mut CatalogControlLink,
        port: u16,
        wrong_stream: bool,
    ) {
        owner.send_local_offer().unwrap();
        let ReceivedControlMessage::RouteStatus(local) = peer.receive_control_message().unwrap()
        else {
            panic!("local offer")
        };
        let mut offer = *local.native_audio_data.unwrap();
        offer.endpoint = NativeAudioEndpoint::Source;
        offer.port = port;
        if wrong_stream {
            offer.stream_id = capyio_core::StreamId::new();
        }
        peer.send_native_audio_data_offer(&local.route, offer, now_ms().unwrap())
            .unwrap();
    }
    fn active() -> (
        AuthenticatedPlaybackOwner,
        CatalogControlLink,
        NativeLanUdpEndpoint,
    ) {
        let (mut owner, mut peer, sender) = connected(Duration::from_secs(2));
        reciprocal(
            &mut owner,
            &mut peer,
            sender.local_addr().unwrap().port(),
            false,
        );
        assert_eq!(
            owner.receive_with(RuntimePlayback::open_synthetic).unwrap(),
            PlaybackOwnerEvent::PlaybackReady
        );
        let ReceivedControlMessage::RouteCommand(start) = peer.receive_control_message().unwrap()
        else {
            panic!("Start before readiness status");
        };
        assert_eq!(start.action, RouteCommandAction::Start);
        assert_eq!(start.route_id, owner.admission.route_id);
        let ReceivedControlMessage::RouteStatus(ready) = peer.receive_control_message().unwrap()
        else {
            panic!("readiness status")
        };
        assert_eq!(ready.route.state, RouteState::Active);
        let endpoint = NativeLanUdpEndpoint::from_socket(
            sender,
            NativeLanEndpointConfig {
                peer: owner.target.local.into(),
                read_timeout: Duration::from_millis(10),
                inflight_packet_capacity: 2,
            },
            owner.media(),
        )
        .unwrap();
        (owner, peer, endpoint)
    }
    #[test]
    fn ipc_ordinary_status_never_opens_worker_and_duplicate_offer_never_restarts_peer() {
        let (mut owner, mut peer, sender) = connected(Duration::from_secs(2));
        owner.send_local_offer().unwrap();
        let ReceivedControlMessage::RouteStatus(local) = peer.receive_control_message().unwrap()
        else {
            panic!("offer");
        };
        let mut draft = local.route.clone();
        draft.state = RouteState::Draft;
        draft.epoch = 0;
        draft.selected_format = None;
        draft.selected_qos_mode = None;
        peer.send_route_status(&draft).unwrap();
        assert_eq!(
            owner
                .receive_with(|_, _, _| panic!("ordinary status cannot open"))
                .unwrap(),
            PlaybackOwnerEvent::ControlUpdated
        );
        assert!(owner.worker.is_none());
        let mut offer = *local.native_audio_data.unwrap();
        offer.endpoint = NativeAudioEndpoint::Source;
        offer.port = sender.local_addr().unwrap().port();
        peer.send_native_audio_data_offer(&local.route, offer, now_ms().unwrap())
            .unwrap();
        assert_eq!(
            owner.receive_with(RuntimePlayback::open_synthetic).unwrap(),
            PlaybackOwnerEvent::PlaybackReady
        );
        let ReceivedControlMessage::RouteCommand(start) = peer.receive_control_message().unwrap()
        else {
            panic!("Start");
        };
        assert_eq!(start.action, RouteCommandAction::Start);
        assert!(matches!(
            peer.receive_control_message().unwrap(),
            ReceivedControlMessage::RouteStatus(_)
        ));
        peer.send_native_audio_data_offer(&local.route, offer, now_ms().unwrap())
            .unwrap();
        assert_eq!(
            owner
                .receive_with(|_, _, _| panic!("duplicate must not reopen"))
                .unwrap(),
            PlaybackOwnerEvent::PlaybackReady
        );
        // A heartbeat roundtrip is an ordering fence: any second Start would
        // arrive before its acknowledgement and fail this exact assertion.
        peer.send_heartbeat(42, false).unwrap();
        assert_eq!(
            owner.receive_with(|_, _, _| panic!("heartbeat")).unwrap(),
            PlaybackOwnerEvent::Heartbeat
        );
        let ReceivedControlMessage::Heartbeat(ack) = peer.receive_control_message().unwrap() else {
            panic!("duplicate offer emitted an unexpected command");
        };
        assert_eq!(ack.sequence, 42);
        assert!(ack.acknowledgement);
        owner.stop().unwrap();
    }

    #[test]
    fn ipc_authenticated_owner_offer_drives_dynamic_pcm_and_stop() {
        let (mut owner, mut peer, mut sender) = active();
        let media = owner.media();
        sender
            .send_packet(&capyio_audio::AudioMediaPacket {
                stream_id: media.stream_id,
                stream_epoch: media.stream_epoch,
                sequence: 1,
                source_timestamp_micros: 10_000,
                first_sample_index: 0,
                sample_count: 480,
                discontinuity: false,
                payload: vec![3; 960],
            })
            .unwrap();
        for _ in 0..4 {
            owner.poll_media().unwrap();
            if owner.metrics().unwrap().packets_written == 1 {
                break;
            }
        }
        assert_eq!(owner.metrics().unwrap().bytes_written, 960);
        peer.send_route_command(owner.admission.route_id, RouteCommandAction::Stop)
            .unwrap();
        assert_eq!(
            owner.receive_control().unwrap(),
            PlaybackOwnerEvent::Stopped
        );
        assert!(owner.poll_media().is_err());
        assert_eq!(
            owner.runtime.route(owner.admission.route_id).unwrap().state,
            RouteState::Stopped
        );
        assert_eq!(
            owner.runtime.session(media.session_id).unwrap().state,
            SessionState::Closed
        );
        assert!(owner.snapshot().port_leases.is_empty());
        assert!(UdpSocket::bind(owner.target.local).is_ok());
    }
    #[test]
    fn ipc_authenticated_owner_wrong_stream_never_opens_media() {
        let (mut owner, mut peer, sender) = connected(Duration::from_secs(2));
        reciprocal(
            &mut owner,
            &mut peer,
            sender.local_addr().unwrap().port(),
            true,
        );
        assert!(
            owner
                .receive_with(|_, _, _| panic!("must not open media"))
                .is_err()
        );
        assert!(owner.worker.is_none());
        assert!(owner.snapshot().port_leases.is_empty());
        assert_eq!(
            owner
                .runtime
                .session(owner.admission.expected.session_id)
                .unwrap()
                .state,
            SessionState::Closed
        );
    }
    #[test]
    fn ipc_authenticated_owner_disconnect_closes_media_and_lease() {
        let (mut owner, peer, _sender) = active();
        drop(peer);
        assert!(owner.receive_control().is_err());
        assert!(owner.poll_media().is_err());
        assert_eq!(
            owner.runtime.route(owner.admission.route_id).unwrap().state,
            RouteState::Offline
        );
        assert!(owner.snapshot().port_leases.is_empty());
        assert!(UdpSocket::bind(owner.target.local).is_ok());
    }
    #[test]
    fn ipc_authenticated_owner_timeout_is_terminal_not_idle_retry() {
        let (mut owner, mut peer, _sender) = connected(Duration::from_millis(500));
        owner.send_local_offer().unwrap();
        peer.receive_control_message().unwrap();
        let start = Instant::now();
        assert!(owner.receive_control().is_err());
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(owner.snapshot().port_leases.is_empty());
        assert_eq!(
            owner
                .runtime
                .session(owner.admission.expected.session_id)
                .unwrap()
                .state,
            SessionState::Closed
        );
        assert!(owner.receive_control().is_err());
    }

    #[test]
    fn ipc_authenticated_owner_drop_closes_control_and_udp() {
        let (owner, mut peer, _sender) = active();
        let local = owner.target.local;
        drop(owner);
        assert!(UdpSocket::bind(local).is_ok());
        assert!(peer.receive_control_message().is_err());
    }

    #[test]
    fn ipc_authenticated_owner_read_deadline_is_clamped_to_lease() {
        let (mut owner, mut peer, _sender) = connected(Duration::from_secs(2));
        owner.send_local_offer().unwrap();
        peer.receive_control_message().unwrap();
        let id = owner.admission.route_id;
        let sink = owner.runtime.route(id).unwrap().sink;
        let now = now_ms().unwrap();
        owner
            .runtime
            .acquire_route_port_lease(id, sink, now + 150, now)
            .unwrap();
        let start = Instant::now();
        assert!(owner.receive_control().is_err());
        assert!(start.elapsed() < Duration::from_millis(750));
        assert!(owner.snapshot().port_leases.is_empty());
        assert_eq!(
            owner
                .runtime
                .session(owner.admission.expected.session_id)
                .unwrap()
                .state,
            SessionState::Closed
        );
    }

    #[test]
    fn ipc_poll_idle_control_keeps_media_and_remote_stop_live() {
        let (mut owner, mut peer, mut sender) = active();
        // No control traffic is required for ongoing playback.
        assert_eq!(owner.poll().unwrap(), None);
        let media = owner.media();
        sender
            .send_packet(&capyio_audio::AudioMediaPacket {
                stream_id: media.stream_id,
                stream_epoch: media.stream_epoch,
                sequence: 1,
                source_timestamp_micros: 10_000,
                first_sample_index: 0,
                sample_count: 480,
                discontinuity: false,
                payload: vec![7; 960],
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while owner.metrics().unwrap().packets_written == 0 {
            assert!(Instant::now() < deadline);
            assert_eq!(owner.poll().unwrap(), None);
        }
        assert_eq!(owner.metrics().unwrap().bytes_written, 960);
        peer.send_route_command(owner.admission.route_id, RouteCommandAction::Stop)
            .unwrap();
        loop {
            assert!(Instant::now() < deadline);
            if owner.poll().unwrap() == Some(PlaybackOwnerEvent::Stopped) {
                break;
            }
        }
        assert!(owner.snapshot().port_leases.is_empty());
        assert!(UdpSocket::bind(owner.target.local).is_ok());
    }
    #[test]
    fn ipc_poll_handshake_uses_authenticated_offer_and_does_not_restart() {
        let (mut owner, mut peer, sender) = connected(Duration::from_secs(2));
        reciprocal(
            &mut owner,
            &mut peer,
            sender.local_addr().unwrap().port(),
            false,
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            assert!(Instant::now() < deadline);
            if owner.poll_with(RuntimePlayback::open_synthetic).unwrap()
                == Some(PlaybackOwnerEvent::PlaybackReady)
            {
                break;
            }
        }
        let ReceivedControlMessage::RouteCommand(start) = peer.receive_control_message().unwrap()
        else {
            panic!("Start before readiness status");
        };
        assert_eq!(start.action, RouteCommandAction::Start);
        assert_eq!(start.route_id, owner.admission.route_id);
        let ReceivedControlMessage::RouteStatus(ready) = peer.receive_control_message().unwrap()
        else {
            panic!("ready");
        };
        assert_eq!(ready.route.state, RouteState::Active);
        peer.send_heartbeat(1, true).unwrap();
        loop {
            assert!(Instant::now() < deadline);
            if owner
                .poll_with(|_, _, _| panic!("must not reopen"))
                .unwrap()
                == Some(PlaybackOwnerEvent::Heartbeat)
            {
                break;
            }
        }
        owner.stop().unwrap();
    }
    #[test]
    fn ipc_poll_idle_lease_expiry_and_disconnect_cleanup() {
        for disconnect in [false, true] {
            let (mut owner, peer, _sender) = active();
            let id = owner.admission.route_id;
            if disconnect {
                drop(peer);
            } else {
                let now = now_ms().unwrap();
                let sink = owner.runtime.route(id).unwrap().sink;
                owner
                    .runtime
                    .acquire_route_port_lease(id, sink, now + 100, now)
                    .unwrap();
                // Keep the peer alive until the owner notices local expiry.
                let deadline = Instant::now() + Duration::from_secs(2);
                while owner.poll().is_ok() {
                    assert!(Instant::now() < deadline);
                }
                drop(peer);
            }
            if disconnect {
                let deadline = Instant::now() + Duration::from_secs(2);
                while owner.poll().is_ok() {
                    assert!(Instant::now() < deadline);
                }
            }
            assert!(owner.snapshot().port_leases.is_empty());
            assert_eq!(owner.runtime.route(id).unwrap().state, RouteState::Offline);
            assert!(UdpSocket::bind(owner.target.local).is_ok());
        }
    }

    fn identities() -> (ControlTlsServerIdentity, ControlTlsClientIdentity) {
        use rcgen::{
            BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
            KeyUsagePurpose,
        };
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
        let key = KeyPair::generate().unwrap();
        let ca = params.self_signed(&key).unwrap().der().to_vec();
        let issuer = Issuer::new(params, key);
        let leaf = |name: &str, usage| {
            let mut params = CertificateParams::new(vec![name.into()]).unwrap();
            params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
            params.extended_key_usages = vec![usage];
            let key = KeyPair::generate().unwrap();
            (
                params.signed_by(&key, &issuer).unwrap().der().to_vec(),
                key.serialize_der(),
            )
        };
        let (server, server_key) = leaf("localhost", ExtendedKeyUsagePurpose::ServerAuth);
        let (client, client_key) = leaf("capyio-test-client", ExtendedKeyUsagePurpose::ClientAuth);
        let server_pin =
            capyio_control_tls::CertificateFingerprint::from_certificate_der(&server).unwrap();
        let client_pin =
            capyio_control_tls::CertificateFingerprint::from_certificate_der(&client).unwrap();
        (
            ControlTlsServerIdentity {
                expected_peer_certificate: client_pin,
                certificate_chain_der: vec![server, ca.clone()],
                private_key_der: server_key,
                client_ca_der: ca.clone(),
            },
            ControlTlsClientIdentity {
                expected_peer_certificate: server_pin,
                certificate_chain_der: vec![client, ca.clone()],
                private_key_der: client_key,
                server_ca_der: ca,
            },
        )
    }
}
