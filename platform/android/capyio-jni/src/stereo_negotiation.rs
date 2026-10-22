//! Borrowed authoritative stereo negotiation. Preparation and commits never send
//! control bytes; the caller releases its Runtime guard before every action.
use crate::{
    secure_stereo_receiver::{MAX_STEREO_OPERATION, SecureStereoSinkAdmission},
    stereo_admission::*,
};
use capyio_control_link::{
    CatalogControlLink, PreparedPortLeaseRequest, PreparedPortLeaseResponse, ReceivedControlMessage,
};
use capyio_control_tls::{
    CertificateFingerprint, ControlTlsBinding, audio_stereo::StereoAudioTlsBinding,
};
use capyio_core::{
    FormatDescriptor, MessageId, Platform, QosMode, Route, RouteBackend, RouteId, RouteState,
    Session, SessionState,
};
use capyio_protocol::{
    PortLeaseAction, RouteCommandAction, SECURE_AUDIO_STEREO_FORMAT, ValidatedPortLeaseResponse,
};
use capyio_runtime::{NodeRuntime, PortLeaseClaim};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// Captured exactly when fresh local consent is resolved, before negotiation.
/// Clones share one consumed claim, including failed prepare attempts.
/// A broad legacy grant must be narrowed by its consent owner before capture.
#[derive(Clone, Debug)]
pub struct StereoConsentStamp {
    consumed: Arc<AtomicBool>,
    original: Route,
    session: Session,
    endpoints: [EndpointAuthority; 2],
    consent_wall: u64,
    end: Instant,
}
impl StereoConsentStamp {
    /// Exact opt-in detection only; never infer stereo from a generic Speaker.
    pub fn is_exact_stereo_sink(runtime: &NodeRuntime, route_id: RouteId) -> bool {
        let Ok(route) = runtime.route(route_id) else {
            return false;
        };
        let snapshot = runtime.snapshot();
        snapshot.local_node.platform == Platform::Android
            && snapshot.local_node.id == route.sink.node_id
            && route.profile == capyio_core::ProfileId::audio_frames_v1()
            && route.backend == RouteBackend::AdapterManaged
            && snapshot
                .local_node
                .capabilities
                .get(&route.sink.capability_id)
                .is_some_and(|cap| cap.class == capyio_core::CapabilityClass::Speaker)
            && [route.source, route.sink].into_iter().all(|reference| {
                EndpointAuthority::capture(runtime, reference)
                    .and_then(|endpoint| endpoint.check(runtime, route, false))
                    .is_ok()
            })
    }
    pub fn capture(
        runtime: &NodeRuntime,
        route_id: RouteId,
        now: u64,
        instant: Instant,
    ) -> Result<Self, String> {
        if !Self::is_exact_stereo_sink(runtime, route_id) {
            return Err("stereo consent requires an exact local Speaker Sink".into());
        }
        let original = runtime.route(route_id).map_err(|e| e.to_string())?.clone();
        let session = runtime
            .session(original.session_id)
            .map_err(|e| e.to_string())?
            .clone();
        let local = runtime.snapshot().local_node;
        let expires = expiry(&original).map_err(|e| e.to_string())?;
        let remaining = expires
            .checked_sub(now)
            .filter(|n| *n > 0 && *n <= 60_000)
            .ok_or("stereo fresh consent must be at most 60 seconds")?;
        if !matches!(original.state, RouteState::Draft | RouteState::Prepared)
            || local.platform != Platform::Android
            || local.id != original.sink.node_id
            || original.profile != capyio_core::ProfileId::audio_frames_v1()
            || original.backend != RouteBackend::AdapterManaged
            || session.state != SessionState::Ready
            || session.local_node_id != original.sink.node_id
            || session.remote_node_id != original.source.node_id
            || runtime
                .route_port_lease_claim(original.id, original.sink)
                .is_some()
        {
            return Err("stereo consent requires a fresh exact local Sink Route".into());
        }
        if original.state == RouteState::Prepared
            && (original.selected_format != Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT))
                || original.selected_qos_mode != Some(QosMode::Interactive))
        {
            return Err("stereo consent Prepared selection differs".into());
        }
        let endpoints = [
            EndpointAuthority::capture(runtime, original.source).map_err(|e| e.to_string())?,
            EndpointAuthority::capture(runtime, original.sink).map_err(|e| e.to_string())?,
        ];
        for endpoint in &endpoints {
            endpoint
                .check(runtime, &original, false)
                .map_err(|e| e.to_string())?;
        }
        Ok(Self {
            consumed: Arc::new(AtomicBool::new(false)),
            original,
            session,
            endpoints,
            consent_wall: now,
            end: instant + Duration::from_millis(remaining),
        })
    }
    pub fn route_id(&self) -> RouteId {
        self.original.id
    }
    pub fn monotonic_deadline(&self) -> Instant {
        self.end
    }
    pub fn prepare(
        self,
        runtime: &NodeRuntime,
        link: &CatalogControlLink,
        admission: SecureStereoSinkAdmission,
        control_generation: u64,
        now: u64,
        instant: Instant,
    ) -> Result<(StereoSinkNegotiation, PreparedPortLeaseRequest), String> {
        self.consumed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "stereo original consent already consumed")?;
        let mut expected = self.original;
        expected.state = RouteState::Prepared;
        expected.selected_format = Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT));
        expected.selected_qos_mode = Some(QosMode::Interactive);
        if now < self.consent_wall
            || instant >= self.end
            || admission.route_id != expected.id
            || runtime.route(expected.id).map_err(|e| e.to_string())? != &expected
            || runtime
                .session(self.session.id)
                .map_err(|e| e.to_string())?
                != &self.session
        {
            return Err("stereo original consent changed or expired before negotiation".into());
        }
        for endpoint in &self.endpoints {
            endpoint
                .check(runtime, &expected, false)
                .map_err(|e| e.to_string())?;
        }
        StereoSinkNegotiation::prepare(
            runtime,
            link,
            admission,
            self.end,
            control_generation,
            now,
            instant,
        )
    }
}

/// One outbound response at most. Send it outside the Runtime guard, then call
/// commit_sink_response. A failed send terminates the original generation.
pub enum StereoNegotiationAction {
    None,
    SendLeaseResponse(PreparedPortLeaseResponse),
    PeerStop,
}

/// Non-Clone: consumed by one media job. Full original authority, not a Runtime
/// clone, is retained. Its accessors cannot widen the original grant or claims.
pub struct StereoSinkSnapshot {
    pub(crate) admission: SecureStereoSinkAdmission,
    pub(crate) original: Route,
    pub(crate) session: Session,
    pub(crate) endpoints: [EndpointAuthority; 2],
    pub(crate) sink_claim: PortLeaseClaim,
    pub(crate) source_claim: ValidatedPortLeaseResponse,
    pub(crate) source_request: MessageId,
    pub(crate) control_generation: u64,
    pub(crate) peer_certificate: CertificateFingerprint,
    pub(crate) started: Instant,
    pub(crate) end: Instant,
    pub(crate) wall_floor: u64,
}
impl StereoSinkSnapshot {
    pub fn route(&self) -> &Route {
        &self.original
    }
    pub fn control_generation(&self) -> u64 {
        self.control_generation
    }
    pub fn control_binding(&self) -> ControlTlsBinding {
        self.admission.expected
    }
    pub fn monotonic_deadline(&self) -> Instant {
        self.end
    }
    pub fn wall_deadline_ms(&self) -> u64 {
        expiry(&self.original)
            .unwrap_or(0)
            .min(self.sink_claim.expires_at_ms)
            .min(self.source_claim.expires_at_ms.unwrap_or(0))
    }
    pub fn binding(&self, now: u64) -> Result<StereoAudioTlsBinding, String> {
        StereoAudioTlsBinding::from_route(
            self.admission.expected,
            &self.original,
            self.admission.stream_id,
            now,
        )
        .map_err(|e| e.to_string())
    }
    /// Call while borrowing the authoritative Runtime. Also pass the exact live
    /// admitted control generation; matching a stale cached TLS identity is not enough.
    pub fn validate(
        &self,
        runtime: &NodeRuntime,
        control_generation: u64,
        now: u64,
        instant: Instant,
    ) -> Result<(), String> {
        if control_generation != self.control_generation
            || now < self.wall_floor
            || instant >= self.end
            || now >= self.wall_deadline_ms()
        {
            return Err("stereo original generation or deadline changed".into());
        }
        validate_route(
            runtime,
            &self.original,
            &self.session,
            self.admission.expected,
            now,
        )
        .map_err(|e| e.to_string())?;
        validate_sink_claim(runtime, &self.original, self.sink_claim, now)
            .map_err(|e| e.to_string())?;
        validate_source_response(&self.original, self.source_request, self.source_claim, now)
            .map_err(|e| e.to_string())?;
        for endpoint in &self.endpoints {
            endpoint
                .check(runtime, &self.original, true)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

pub struct StereoSinkNegotiation {
    admission: SecureStereoSinkAdmission,
    original: Route,
    session: Session,
    endpoints: [EndpointAuthority; 2],
    control_generation: u64,
    peer_certificate: CertificateFingerprint,
    source_request: MessageId,
    source_sent: bool,
    sink_response_pending: bool,
    sink_response_sent: bool,
    source_claim: Option<ValidatedPortLeaseResponse>,
    sink_claim: Option<PortLeaseClaim>,
    authenticated_start: bool,
    started: Instant,
    end: Instant,
    local_consent_end: Instant,
    negotiation_end: Instant,
    last_wall: u64,
    terminal: bool,
}
impl StereoSinkNegotiation {
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        runtime: &NodeRuntime,
        link: &CatalogControlLink,
        admission: SecureStereoSinkAdmission,
        original_consent_end: Instant,
        control_generation: u64,
        now: u64,
        instant: Instant,
    ) -> Result<(Self, PreparedPortLeaseRequest), String> {
        validate_admission(&admission).map_err(|e| e.to_string())?;
        let original = runtime
            .route(admission.route_id)
            .map_err(|e| e.to_string())?
            .clone();
        let session = runtime
            .session(original.session_id)
            .map_err(|e| e.to_string())?
            .clone();
        validate_route(runtime, &original, &session, admission.expected, now)
            .map_err(|e| e.to_string())?;
        if control_generation == 0
            || control_generation > i64::MAX as u64
            || original.state != RouteState::Prepared
            || runtime
                .route_port_lease_claim(original.id, original.sink)
                .is_some()
            || link.is_closed()
            || link.tls_binding() != admission.expected
            || link.peer_node_id() != original.source.node_id
        {
            return Err(
                "stereo requires exact current Prepared control generation without a lease".into(),
            );
        }
        let peer_certificate = link
            .authenticated_peer_certificate()
            .map_err(|e| e.to_string())?;
        link.authenticated_peer_addr().map_err(|e| e.to_string())?;
        let endpoints = [
            EndpointAuthority::capture(runtime, original.source).map_err(|e| e.to_string())?,
            EndpointAuthority::capture(runtime, original.sink).map_err(|e| e.to_string())?,
        ];
        for endpoint in &endpoints {
            endpoint
                .check(runtime, &original, false)
                .map_err(|e| e.to_string())?;
        }
        let end = consent_end(
            instant,
            admission.lifetime,
            original_consent_end,
            expiry(&original)
                .map_err(|e| e.to_string())?
                .checked_sub(now)
                .ok_or("stereo consent expired")?,
        )
        .map_err(|e| e.to_string())?;
        let request = link
            .prepare_port_lease_request(
                original.id,
                original.source,
                PortLeaseAction::Acquire,
                source_lease_ttl(end.duration_since(instant)).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        Ok((
            Self {
                admission,
                original,
                session,
                endpoints,
                control_generation,
                peer_certificate,
                source_request: request.message_id(),
                source_sent: false,
                sink_response_pending: false,
                sink_response_sent: false,
                source_claim: None,
                sink_claim: None,
                authenticated_start: false,
                started: instant,
                end,
                local_consent_end: end,
                negotiation_end: end.min(instant + MAX_STEREO_OPERATION),
                last_wall: now,
                terminal: false,
            },
            request,
        ))
    }
    pub fn operation_deadline(&self) -> Instant {
        self.end.min(self.negotiation_end)
    }
    pub fn route(&self) -> &Route {
        &self.original
    }
    pub fn is_terminal(&self) -> bool {
        self.terminal
    }
    pub fn source_request(&self) -> MessageId {
        self.source_request
    }
    pub fn terminate(&mut self) {
        self.terminal = true;
    }
    pub fn commit_source_request(&mut self, sent: MessageId) -> Result<(), String> {
        if self.terminal
            || self.source_sent
            || sent != self.source_request
            || Instant::now() >= self.operation_deadline()
        {
            self.terminal = true;
            return Err("stale or duplicate stereo Source request commit".into());
        }
        self.source_sent = true;
        Ok(())
    }
    pub fn commit_sink_response(&mut self) -> Result<(), String> {
        if self.terminal
            || !self.sink_response_pending
            || self.sink_response_sent
            || Instant::now() >= self.operation_deadline()
        {
            self.terminal = true;
            return Err("stale or duplicate stereo Sink response commit".into());
        }
        self.sink_response_pending = false;
        self.sink_response_sent = true;
        Ok(())
    }
    pub fn validate(
        &mut self,
        runtime: &NodeRuntime,
        link: &CatalogControlLink,
        control_generation: u64,
        now: u64,
        instant: Instant,
    ) -> Result<(), String> {
        let result = self.validate_inner(runtime, link, control_generation, now, instant);
        if result.is_err() {
            self.terminal = true;
        } else {
            self.last_wall = now;
        }
        result
    }
    fn validate_inner(
        &self,
        runtime: &NodeRuntime,
        link: &CatalogControlLink,
        control_generation: u64,
        now: u64,
        instant: Instant,
    ) -> Result<(), String> {
        if self.terminal
            || control_generation != self.control_generation
            || link.is_closed()
            || link.tls_binding() != self.admission.expected
            || link.peer_node_id() != self.original.source.node_id
            || link
                .authenticated_peer_certificate()
                .map_err(|e| e.to_string())?
                != self.peer_certificate
            || now < self.last_wall
            || instant >= self.operation_deadline()
        {
            return Err("stereo negotiation authority ended".into());
        }
        validate_route(
            runtime,
            &self.original,
            &self.session,
            self.admission.expected,
            now,
        )
        .map_err(|e| e.to_string())?;
        match self.sink_claim {
            Some(claim) => validate_sink_claim(runtime, &self.original, claim, now)
                .map_err(|e| e.to_string())?,
            None if runtime
                .route_port_lease_claim(self.original.id, self.original.sink)
                .is_none() => {}
            None => return Err("unclaimed stereo Sink lease appeared".into()),
        }
        if let Some(claim) = self.source_claim {
            validate_source_response(&self.original, self.source_request, claim, now)
                .map_err(|e| e.to_string())?;
        }
        for endpoint in &self.endpoints {
            endpoint
                .check(runtime, &self.original, self.source_claim.is_some())
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn accept_control(
        &mut self,
        runtime: &mut NodeRuntime,
        link: &mut CatalogControlLink,
        message: ReceivedControlMessage,
        control_generation: u64,
        now: u64,
        instant: Instant,
    ) -> Result<StereoNegotiationAction, String> {
        let result = self.accept_inner(runtime, link, message, control_generation, now, instant);
        if result.is_err() {
            self.terminal = true;
        }
        result
    }
    fn accept_inner(
        &mut self,
        runtime: &mut NodeRuntime,
        link: &mut CatalogControlLink,
        message: ReceivedControlMessage,
        control_generation: u64,
        now: u64,
        instant: Instant,
    ) -> Result<StereoNegotiationAction, String> {
        self.validate_inner(runtime, link, control_generation, now, instant)?;
        if !self.source_sent || self.sink_response_pending {
            return Err("stereo action not committed".into());
        }
        let action = match message {
            ReceivedControlMessage::PortLeaseResponse(response) => {
                if self.source_claim.is_some() {
                    return Err("duplicate stereo Source claim".into());
                }
                let expiry =
                    validate_source_response(&self.original, self.source_request, response, now)
                        .map_err(|e| e.to_string())?;
                self.end = self.end.min(instant + Duration::from_millis(expiry - now));
                self.source_claim = Some(response);
                StereoNegotiationAction::None
            }
            ReceivedControlMessage::PortLeaseRequest(received) => {
                let request = received.request();
                if self.sink_claim.is_some()
                    || request.action != PortLeaseAction::Acquire
                    || request.session_id != self.original.session_id
                    || request.route_id != self.original.id
                    || request.port != self.original.sink
                    || now
                        .checked_add(request.requested_ttl_ms)
                        .is_none_or(|end| end > expiry(&self.original).unwrap_or(0))
                    || Duration::from_millis(request.requested_ttl_ms)
                        > self.local_consent_end.saturating_duration_since(instant)
                {
                    return Err("unexpected stereo Sink request or renewal".into());
                }
                let response = link
                    .prepare_port_lease_response(runtime, received, now)
                    .map_err(|e| e.to_string())?;
                let claim = *runtime
                    .route_port_lease_claim(self.original.id, self.original.sink)
                    .ok_or("stereo Sink lease denied")?;
                validate_sink_claim(runtime, &self.original, claim, now)
                    .map_err(|e| e.to_string())?;
                self.end = self
                    .end
                    .min(instant + Duration::from_millis(claim.expires_at_ms - now));
                self.sink_claim = Some(claim);
                self.sink_response_pending = true;
                StereoNegotiationAction::SendLeaseResponse(response)
            }
            ReceivedControlMessage::RouteCommand(command)
                if command.session_id == self.original.session_id
                    && command.route_id == self.original.id =>
            {
                match command.action {
                    RouteCommandAction::Stop => {
                        self.terminal = true;
                        return Ok(StereoNegotiationAction::PeerStop);
                    }
                    RouteCommandAction::Start if !self.authenticated_start => {
                        self.authenticated_start = true;
                        StereoNegotiationAction::None
                    }
                    _ => return Err("duplicate stereo Start".into()),
                }
            }
            ReceivedControlMessage::RouteStatus(status)
                if status.route == self.original
                    && status.session_id == self.original.session_id
                    && status.secure_audio_data.is_none()
                    && status.native_audio_data.is_none()
                    && status.camera_data.is_none()
                    && status.secure_camera_data.is_none()
                    && status.touchpad_data.is_none()
                    && status.haptics_data.is_none() =>
            {
                StereoNegotiationAction::None
            }
            _ => return Err("unexpected dedicated stereo negotiation message".into()),
        };
        self.validate_inner(runtime, link, control_generation, now, instant)?;
        self.last_wall = now;
        Ok(action)
    }
    pub fn ready(&self) -> bool {
        !self.terminal
            && self.source_sent
            && self.sink_response_sent
            && self.source_claim.is_some()
            && self.sink_claim.is_some()
            && self.authenticated_start
    }
    /// Exact transition and consume-once handoff. No listener or network I/O.
    pub fn finish(
        &mut self,
        runtime: &mut NodeRuntime,
        link: &CatalogControlLink,
        control_generation: u64,
        now: u64,
        instant: Instant,
    ) -> Result<StereoSinkSnapshot, String> {
        let result = (|| {
            self.validate_inner(runtime, link, control_generation, now, instant)?;
            if !self.ready() {
                return Err("stereo original claims and Start not complete".into());
            }
            let mut expected = self.original.clone();
            expected.state = RouteState::Starting;
            expected.epoch = expected
                .epoch
                .checked_add(1)
                .ok_or("stereo epoch overflow")?;
            runtime
                .begin_route_start(expected.id, now)
                .map_err(|e| e.to_string())?;
            if runtime.route(expected.id).map_err(|e| e.to_string())? != &expected {
                return Err("unexpected stereo Start transition".into());
            }
            Ok(StereoSinkSnapshot {
                admission: self.admission.clone(),
                original: expected,
                session: self.session.clone(),
                endpoints: self.endpoints.clone(),
                sink_claim: self.sink_claim.ok_or("missing stereo Sink claim")?,
                source_claim: self.source_claim.ok_or("missing stereo Source claim")?,
                source_request: self.source_request,
                control_generation: self.control_generation,
                peer_certificate: self.peer_certificate,
                started: self.started,
                end: self.end,
                wall_floor: now,
            })
        })();
        self.terminal = true;
        result
    }
}

#[cfg(test)]
#[path = "stereo_negotiation_tests.rs"]
pub(crate) mod tests;
