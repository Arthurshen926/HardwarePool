//! Explicit, portable bilateral stereo control owner. Existing JNI dispatch does
//! not construct this supervisor. Approval is never implied by a proposal.
use crate::{
    AndroidCatalogRuntimeError, AndroidCatalogRuntimeV1,
    secure_stereo_receiver::{MAX_STEREO_OPERATION, SecureStereoSinkAdmission},
    stereo_job::{StereoAdmissionGate, StereoReceiveJob},
    stereo_negotiation::{StereoNegotiationAction, StereoSinkNegotiation, StereoSinkSnapshot},
};
use capyio_control_link::{
    CatalogControlLink, ReceivedControlMessage, StereoConsentControlIdentity,
};
use capyio_control_tls::ControlTlsServerIdentity;
use capyio_core::{AuthorizationState, PortRef, RouteBackend, RouteId, RouteState, SessionId};
use capyio_protocol::RouteCommandAction;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Registration is dormant unless a supervisor explicitly stages a Route.
/// Cloned hosts share revocation, not a fresh authority or independent job gate.
#[derive(Clone, Debug, Default)]
pub(crate) struct BilateralRegistry(
    Arc<Mutex<BTreeMap<RouteId, (SessionId, StereoAdmissionGate)>>>,
);
impl BilateralRegistry {
    pub(crate) fn contains(&self, route: RouteId) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains_key(&route)
    }
    pub(crate) fn owns_session(&self, session: SessionId) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .any(|(id, _)| *id == session)
    }
    pub(crate) fn invalidate(&self, route: RouteId) {
        let m = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if let Some((_, gate)) = m.get(&route) {
            gate.invalidate();
        }
    }
    pub(crate) fn register(
        &self,
        route: RouteId,
        session: SessionId,
    ) -> Result<StereoAdmissionGate, String> {
        let mut m = self.0.lock().map_err(|_| "bilateral registry poisoned")?;
        if m.contains_key(&route) || m.len() >= 16 {
            return Err("bilateral registration consumed or full".into());
        }
        let gate = StereoAdmissionGate::new();
        m.insert(route, (session, gate.clone()));
        Ok(gate)
    }
    fn session_routes(&self, session: SessionId) -> Result<Vec<RouteId>, String> {
        let m = self.0.lock().map_err(|_| "bilateral registry poisoned")?;
        Ok(m.iter()
            .filter_map(|(id, (s, g))| {
                if *s == session {
                    g.invalidate();
                    Some(*id)
                } else {
                    None
                }
            })
            .collect())
    }
}
impl AndroidCatalogRuntimeV1 {
    /// Catalog-exchange callbacks must not switch to a host whose peer is
    /// reserved by an existing bilateral supervisor after admitting elsewhere.
    pub(crate) fn check_unregistered_catalog_peer(
        &self,
        peer: capyio_core::NodeId,
    ) -> Result<(), String> {
        if self.runtime.snapshot().sessions.iter().any(|session| {
            session.remote_node_id == peer
                && session.state != capyio_core::SessionState::Closed
                && self.bilateral_stereo.owns_session(session.id)
        }) {
            return Err("registered bilateral peer belongs to its original supervisor".into());
        }
        Ok(())
    }
    pub(crate) fn invalidate_bilateral_pending_expiry(&self, now: u64) {
        for route in self.runtime.snapshot().routes {
            if self.bilateral_stereo.contains(route.id)
                && self
                    .touchpad_consent
                    .pending_deadline(route.id)
                    .is_some_and(|end| now >= end)
            {
                self.bilateral_stereo.invalidate(route.id);
            }
        }
    }
    pub(crate) fn invalidate_bilateral_stereo_expiry(&mut self, now: u64) -> Result<(), String> {
        let routes = self.runtime.snapshot().routes;
        for route in routes {
            if self.bilateral_stereo.contains(route.id)
                && (matches!(route.authorization,AuthorizationState::Authorized{expires_at_ms:Some(end)} if now>=end)
                    || self
                        .runtime
                        .route_port_lease_claim(route.id, route.sink)
                        .is_some_and(|claim| now >= claim.expires_at_ms))
            {
                self.bilateral_stereo.invalidate(route.id);
                self.runtime
                    .retire_route_authorization(route.id)
                    .map_err(|e| e.to_string())?;
                self.stereo_consents.remove(&route.id);
                self.audio_status_pending.insert(route.id);
            }
        }
        Ok(())
    }
    /// Only registered B2 Routes are affected. Other flows keep their existing
    /// lifecycle policy. A clone invalidates the shared gate before mutation.
    pub fn invalidate_bilateral_stereo_session(
        &mut self,
        session: SessionId,
    ) -> Result<(), AndroidCatalogRuntimeError> {
        let routes = self
            .bilateral_stereo
            .session_routes(session)
            .map_err(|_| AndroidCatalogRuntimeError::ConnectedState)?;
        for id in routes {
            if self.runtime.route(id).is_ok() {
                self.runtime.retire_route_authorization(id)?;
            }
            self.stereo_consents.remove(&id);
            self.audio_status_pending.insert(id);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StereoSupervisorEvent {
    PendingApproval { route_id: RouteId },
    PeerApproved { route_id: RouteId },
    Starting { route_id: RouteId },
}

/// Owns the only control reader throughout approval, crossed leases and media.
/// The shared host permits ordinary inventory updates; registration makes those
/// updates revoke this supervisor's fast-path gates before publishing changes.
pub struct StereoSinkSupervisor {
    host: Arc<Mutex<AndroidCatalogRuntimeV1>>,
    link: CatalogControlLink,
    identity: StereoConsentControlIdentity,
    generation: u64,
    route: Option<RouteId>,
    gate: Option<StereoAdmissionGate>,
    peer_approved: bool,
    negotiation: Option<StereoSinkNegotiation>,
    snapshot: Option<StereoSinkSnapshot>,
    job: Option<Arc<StereoReceiveJob>>,
    end: Instant,
    last_wall: Arc<Mutex<u64>>,
    terminal: bool,
    cleanup_error: Arc<Mutex<Option<String>>>,
}
fn now_ms() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis()
        .try_into()
        .map_err(|_| "wall overflow".into())
}
// Control and worker-side authority only, never PCM read/ACK: serialize the
// SAMPLE as well as the comparison so concurrent observers cannot miss a real
// rollback or mistake a delayed old sample for one. Release before Runtime/I/O.
fn observe_wall(floor: &Mutex<u64>) -> Result<u64, String> {
    observe_wall_with(floor, now_ms)
}
fn observe_wall_with(
    floor: &Mutex<u64>,
    sample: impl FnOnce() -> Result<u64, String>,
) -> Result<u64, String> {
    let mut previous = floor.lock().map_err(|_| "bilateral wall clock poisoned")?;
    let now = sample()?;
    if now < *previous {
        return Err("bilateral wall regressed".into());
    }
    *previous = now;
    Ok(now)
}
fn dedicated(
    host: &AndroidCatalogRuntimeV1,
    session: SessionId,
    route: Option<RouteId>,
) -> Result<(), String> {
    let routes = host.runtime.snapshot().routes;
    let selected: Vec<_> = routes.iter().filter(|r| r.session_id == session).collect();
    if match route {
        Some(id) => selected.len() != 1 || selected[0].id != id,
        None => !selected.is_empty(),
    } {
        return Err("bilateral stereo requires a dedicated one-Route Session".into());
    }
    Ok(())
}
impl StereoSinkSupervisor {
    pub fn new(
        mut host: AndroidCatalogRuntimeV1,
        link: CatalogControlLink,
        control_generation: u64,
    ) -> Result<Self, String> {
        dedicated(&host, link.session_id(), None)?;
        host.bind_stereo_consent_control(&link, control_generation)?;
        let identity = link
            .stereo_consent_control_identity()
            .map_err(|e| e.to_string())?;
        let instant = Instant::now();
        let wall = now_ms()?;
        Ok(Self {
            host: Arc::new(Mutex::new(host)),
            link,
            identity,
            generation: control_generation,
            route: None,
            gate: None,
            peer_approved: false,
            negotiation: None,
            snapshot: None,
            job: None,
            end: instant + Duration::from_secs(60),
            last_wall: Arc::new(Mutex::new(wall)),
            terminal: false,
            cleanup_error: Arc::new(Mutex::new(None)),
        })
    }
    pub fn host_handle(&self) -> Arc<Mutex<AndroidCatalogRuntimeV1>> {
        self.host.clone()
    }
    pub fn route_id(&self) -> Option<RouteId> {
        self.route
    }
    pub fn is_terminal(&self) -> bool {
        self.terminal
    }
    pub fn cleanup_error(&self) -> Option<String> {
        match self.cleanup_error.lock() {
            Ok(error) => error.clone(),
            Err(_) => Some("bilateral cleanup record poisoned".into()),
        }
    }
    fn check(&mut self) -> Result<u64, String> {
        let now = observe_wall(&self.last_wall)?;
        if self.terminal
            || Instant::now() >= self.end
            || self.gate.as_ref().is_some_and(|g| !g.is_open())
        {
            return Err("bilateral original authority expired or revoked".into());
        }
        self.link
            .validate_stereo_consent_control_identity(&self.identity)
            .map_err(|e| e.to_string())?;
        let host = self.host.lock().map_err(|_| "bilateral host poisoned")?;
        dedicated(&host, self.link.session_id(), self.route)?;
        Ok(now)
    }
    fn fail(&mut self, reason: String) -> String {
        self.terminal = true;
        if let Some(gate) = &self.gate {
            gate.invalidate();
        }
        if let Some(job) = &self.job {
            job.fail();
        }
        let mut cleanup = Vec::new();
        if let Some(error) = self.cleanup_error() {
            cleanup.push(error);
        }
        match self.host.lock() {
            Ok(mut host) => {
                if let Some(id) = self.route {
                    if let Err(e) = host.runtime.retire_route_authorization(id) {
                        cleanup.push(e.to_string());
                    }
                    host.stereo_consents.remove(&id);
                    host.audio_status_pending.insert(id);
                }
                if let Err(e) = self.link.close_and_reconcile(&mut host.runtime) {
                    cleanup.push(e.to_string());
                }
            }
            Err(_) => {
                self.link.close();
                cleanup.push("bilateral host poisoned".into());
            }
        }
        if cleanup.is_empty() {
            reason
        } else {
            format!("{reason}; cleanup: {}", cleanup.join("; "))
        }
    }
    fn stage(&mut self, id: RouteId) -> Result<(), String> {
        // Remember the target before any staging mutation, so failures retire it.
        self.route = Some(id);
        let mut host = self.host.lock().map_err(|_| "bilateral host poisoned")?;
        dedicated(&host, self.link.session_id(), Some(id))?;
        self.gate = Some(host.bilateral_stereo.register(id, self.link.session_id())?);
        host.stage_stereo_consent(&self.link, id)
    }
    /// Receiver-originated Pending proposal. Source still approves first.
    pub fn propose(&mut self, source: PortRef, sink: PortRef) -> Result<RouteId, String> {
        let result = (|| {
            self.check()?;
            if self.route.is_some() {
                return Err("bilateral proposal already exists".into());
            }
            let id = {
                let mut host = self.host.lock().map_err(|_| "bilateral host poisoned")?;
                host.runtime
                    .create_route(
                        self.link.session_id(),
                        source,
                        sink,
                        RouteBackend::AdapterManaged,
                    )
                    .map_err(|e| e.to_string())?
            };
            self.stage(id)?;
            let route = self
                .host
                .lock()
                .map_err(|_| "bilateral host poisoned")?
                .runtime
                .route(id)
                .map_err(|e| e.to_string())?
                .clone();
            self.link
                .send_route_status_until(
                    &route,
                    self.end.min(Instant::now() + MAX_STEREO_OPERATION),
                )
                .map_err(|e| e.to_string())?;
            Ok(id)
        })();
        result.map_err(|e| self.fail(e))
    }
    /// Polls only this actual admitted link. No caller-created status is accepted.
    pub fn poll(&mut self) -> Result<Option<StereoSupervisorEvent>, String> {
        let result = self.poll_inner();
        result.map_err(|e| self.fail(e))
    }
    fn poll_inner(&mut self) -> Result<Option<StereoSupervisorEvent>, String> {
        self.check()?;
        if let Some(job) = &self.job
            && matches!(
                job.state(),
                crate::stereo_job::FAILED | crate::stereo_job::STOPPED
            )
        {
            return Err("bilateral media ended".into());
        }
        let deadline = self
            .negotiation
            .as_ref()
            .map(|n| n.operation_deadline())
            .unwrap_or(self.end)
            .min(self.end)
            .min(Instant::now() + MAX_STEREO_OPERATION);
        let Some(message) = self
            .link
            .poll_control_message_until(deadline)
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let now = self.check()?;
        if let Some(mut negotiation) = self.negotiation.take() {
            let action = {
                let mut host = self.host.lock().map_err(|_| "bilateral host poisoned")?;
                negotiation.accept_control(
                    &mut host.runtime,
                    &mut self.link,
                    message,
                    self.generation,
                    now,
                    Instant::now(),
                )?
            };
            match action {
                StereoNegotiationAction::None => {}
                StereoNegotiationAction::PeerStop => return Err("bilateral peer stopped".into()),
                StereoNegotiationAction::SendLeaseResponse(response) => {
                    self.check()?;
                    self.link
                        .send_prepared_port_lease_response_until(response, deadline)
                        .map_err(|e| e.to_string())?;
                    self.check()?;
                    negotiation.commit_sink_response()?;
                }
            }
            if negotiation.ready() {
                let now = self.check()?;
                let snapshot = {
                    let mut host = self.host.lock().map_err(|_| "bilateral host poisoned")?;
                    negotiation.finish(
                        &mut host.runtime,
                        &self.link,
                        self.generation,
                        now,
                        Instant::now(),
                    )?
                };
                #[cfg(test)]
                tests::fail_at(3)?;
                self.end = self.end.min(snapshot.monotonic_deadline());
                self.snapshot = Some(snapshot);
                self.negotiation = None;
                self.check()?;
                return Ok(Some(StereoSupervisorEvent::Starting {
                    route_id: self.route.expect("staged"),
                }));
            }
            self.negotiation = Some(negotiation);
            return Ok(None);
        }
        match message {
            ReceivedControlMessage::RouteStatus(status) if self.route.is_none() => {
                if status.route.state != RouteState::Draft
                    || status.route.authorization != AuthorizationState::Pending
                    || status.touchpad_data.is_some()
                    || status.haptics_data.is_some()
                    || status.native_audio_data.is_some()
                    || status.camera_data.is_some()
                    || status.secure_camera_data.is_some()
                    || status.secure_audio_data.is_some()
                {
                    return Err("bilateral initial proposal must be Pending Draft".into());
                }
                let id = status.route.id;
                {
                    let mut host = self.host.lock().map_err(|_| "bilateral host poisoned")?;
                    self.link
                        .apply_route_status(&mut host.runtime, &status, false)
                        .map_err(|e| e.to_string())?;
                    self.route = Some(id);
                    if host.runtime.route(id).map_err(|e| e.to_string())? != &status.route {
                        return Err("bilateral proposal differs from exact local Route".into());
                    }
                }
                self.stage(id)?;
                Ok(Some(StereoSupervisorEvent::PendingApproval {
                    route_id: id,
                }))
            }
            ReceivedControlMessage::RouteStatus(status)
                if self.snapshot.is_none() && self.job.is_none() =>
            {
                let id = self.route.ok_or("bilateral route absent")?;
                if status.route.id != id {
                    return Err("bilateral different Route".into());
                }
                self.host
                    .lock()
                    .map_err(|_| "bilateral host poisoned")?
                    .receive_stereo_consent(&mut self.link, &status)?;
                self.peer_approved = true;
                Ok(Some(StereoSupervisorEvent::PeerApproved { route_id: id }))
            }
            ReceivedControlMessage::Heartbeat(h) => {
                if !h.acknowledgement {
                    self.link
                        .send_heartbeat_until(h.sequence, true, deadline)
                        .map_err(|e| e.to_string())?;
                }
                Ok(None)
            }
            ReceivedControlMessage::RouteCommand(c)
                if Some(c.route_id) == self.route && c.action == RouteCommandAction::Stop =>
            {
                Err("bilateral peer stopped".into())
            }
            _ => Err("unexpected dedicated bilateral control message".into()),
        }
    }
    /// An early allow is rejected without being queued or consuming approval.
    /// Caller must make another fresh call after PeerApproved.
    pub fn approve(
        &mut self,
        route: RouteId,
        allow: bool,
        admission: SecureStereoSinkAdmission,
    ) -> Result<(), String> {
        if allow && !self.peer_approved && self.route == Some(route) && !self.terminal {
            return Err("approve again after authenticated source approval".into());
        }
        let result = (|| {
            let now = self.check()?;
            if self.route != Some(route) || self.negotiation.is_some() || self.snapshot.is_some() {
                return Err("bilateral approval already resolved or wrong Route".into());
            }
            let stamp = {
                let mut host = self.host.lock().map_err(|_| "bilateral host poisoned")?;
                crate::stereo_admission::validate_admission(&admission)
                    .map_err(|e| e.to_string())?;
                if admission.route_id != route || admission.expected != self.link.tls_binding() {
                    return Err("bilateral admission changed Route or binding".into());
                }
                host.resolve_bilateral_stereo_consent(route, allow, admission.lifetime)?;
                if !allow {
                    return Err("bilateral approval denied".into());
                }
                host.stereo_consents
                    .remove(&route)
                    .ok_or("bilateral bounded stamp absent")?
            };
            self.end = self.end.min(stamp.monotonic_deadline());
            let prepare_now = observe_wall(&self.last_wall)?;
            if prepare_now < now {
                return Err("bilateral clock regressed during approval".into());
            }
            let (negotiation, request, prepared) = {
                let host = self.host.lock().map_err(|_| "bilateral host poisoned")?;
                let (negotiation, request) = stamp.prepare(
                    &host.runtime,
                    &self.link,
                    admission,
                    self.generation,
                    prepare_now,
                    Instant::now(),
                )?;
                (
                    negotiation,
                    request,
                    host.runtime
                        .route(route)
                        .map_err(|e| e.to_string())?
                        .clone(),
                )
            };
            self.check()?;
            let deadline = negotiation.operation_deadline().min(self.end);
            self.link
                .send_route_status_until(&prepared, deadline)
                .map_err(|e| e.to_string())?;
            #[cfg(test)]
            tests::fail_at(1)?;
            self.check()?;
            let sent = self
                .link
                .send_prepared_port_lease_request_until(request, deadline)
                .map_err(|e| e.to_string())?;
            #[cfg(test)]
            tests::fail_at(2)?;
            self.check()?;
            let mut negotiation = negotiation;
            negotiation.commit_source_request(sent)?;
            self.negotiation = Some(negotiation);
            self.check()?;
            Ok(())
        })();
        result.map_err(|e| self.fail(e))
    }
    /// Creates a receiver job only after both claims and authenticated Start.
    /// The returned job retains the shared revocation gate and Runtime callback.
    pub fn start_media(
        &mut self,
        identity: ControlTlsServerIdentity,
        runtime_id: u64,
        job_generation: u64,
    ) -> Result<Arc<StereoReceiveJob>, String> {
        let result = (|| {
            self.check()?;
            if self.job.is_some() {
                return Err("bilateral media already started".into());
            }
            let snapshot = self
                .snapshot
                .take()
                .ok_or("bilateral negotiation not complete")?;
            let gate = self.gate.clone().ok_or("bilateral gate absent")?;
            let route = self.route.ok_or("bilateral route absent")?;
            self.host
                .lock()
                .map_err(|_| "bilateral host poisoned")?
                .stereo_generations
                .get_mut(&route)
                .ok_or("bilateral generation absent")?
                .reserve()
                .map_err(str::to_owned)?;
            let host = self.host.clone();
            let authority_gate = gate.clone();
            let generation = self.generation;
            let wall = self.last_wall.clone();
            let identity_token = self.identity.clone();
            let authority = Arc::new(move |snapshot: &StereoSinkSnapshot| {
                if !authority_gate.is_open() || !identity_token.is_live() {
                    return Err("bilateral live authority revoked".into());
                }
                let now = observe_wall(&wall)?;
                let host = host.lock().map_err(|_| "bilateral host poisoned")?;
                snapshot.validate(&host.runtime, generation, now, Instant::now())
            });
            let host = self.host.clone();
            let failure_gate = gate.clone();
            let cleanup = self.cleanup_error.clone();
            let failure = Arc::new(move || {
                failure_gate.invalidate();
                let result = host
                    .lock()
                    .map_err(|_| "bilateral host poisoned".to_string())
                    .and_then(|mut host| {
                        let result = host
                            .runtime
                            .retire_route_authorization(route)
                            .map_err(|e| e.to_string());
                        host.audio_status_pending.insert(route);
                        result
                    });
                if let Err(error) = result
                    && let Ok(mut slot) = cleanup.lock()
                {
                    *slot = Some(error);
                }
            });
            let job = StereoReceiveJob::start(
                runtime_id,
                job_generation,
                snapshot,
                identity,
                self.generation,
                gate,
                authority,
                failure,
            )?;
            self.job = Some(job.clone());
            self.check()?;
            let now = self.check()?;
            self.link
                .send_secure_stereo_audio_data_offer_until(
                    job.snapshot().route(),
                    job.offer(),
                    now,
                    job.operation_deadline(),
                )
                .map_err(|e| e.to_string())?;
            self.check()?;
            job.offer_committed()?;
            Ok(job)
        })();
        result.map_err(|e| self.fail(e))
    }
    pub fn stop(&mut self) -> Result<(), String> {
        if self.terminal {
            return Ok(());
        }
        let reason = self.fail("bilateral supervisor stopped".into());
        if reason.contains("; cleanup:") {
            Err(reason)
        } else {
            Ok(())
        }
    }
}
impl Drop for StereoSinkSupervisor {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
#[path = "stereo_supervisor_tests.rs"]
mod tests;
