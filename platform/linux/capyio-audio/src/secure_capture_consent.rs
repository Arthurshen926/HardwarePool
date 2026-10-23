//! Dormant bilateral source consent. No CLI, JNI or default dispatcher uses this.
//! Owns the actual authenticated link; public status DTOs are never admission.
use crate::{MAX_STALL, SecureStereoReceiverAssignedSourceAdmission, SecureStereoSourceOwner};
use capyio_control_link::{
    CatalogControlLink, ReceivedControlMessage, StereoConsentControlIdentity, StereoPreparedReceipt,
};
use capyio_control_tls::{CertificateFingerprint, ControlTlsClientIdentity};
use capyio_core::*;
use capyio_protocol::{SECURE_AUDIO_STEREO_FORMAT, ValidatedRouteStatus};
use capyio_runtime::NodeRuntime;
use std::{
    fmt, io,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const CONSENT: Duration = Duration::from_secs(60);
type Catalogs = (
    CapabilityDescriptor,
    AdapterInstanceDescriptor,
    CapabilityDescriptor,
    AdapterInstanceDescriptor,
);
fn error(reason: impl ToString) -> io::Error {
    io::Error::other(reason.to_string())
}
fn clock() -> io::Result<(u64, Instant)> {
    let instant = Instant::now();
    let wall = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(error)?
        .as_millis()
        .try_into()
        .map_err(error)?;
    Ok((wall, instant))
}

/// Trusted exact local source intent, never inferred from remote target metadata.
#[derive(Clone, Debug)]
pub struct SecureStereoSourceIntent {
    pub source: PortRef,
    pub sink: PortRef,
    pub pipewire_target: String,
    pub lifetime: Duration,
}

/// A failed ownership transfer returns the authoritative, already-retired Runtime.
/// Cleanup errors are retained separately and never represented as clean success.
pub struct SecureStereoSourceFailure {
    pub error: io::Error,
    pub cleanup_errors: Vec<String>,
    pub runtime: NodeRuntime,
}
impl fmt::Debug for SecureStereoSourceFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecureStereoSourceFailure")
            .field("error", &self.error)
            .field("cleanup_errors", &self.cleanup_errors)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for SecureStereoSourceFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for SecureStereoSourceFailure {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecureStereoSourceHandshakeEvent {
    Terminal,
    AwaitingProposal,
    PendingApproval { route_id: RouteId },
    AwaitingPeer,
    Ready,
}

/// One attempt on one dedicated Session. Drop, denial and any failed phase close
/// this attempt; no raw link or mutable Runtime escape can mint another approval.
pub struct SecureStereoSourceHandshake {
    runtime: Option<NodeRuntime>,
    link: Option<CatalogControlLink>,
    intent: SecureStereoSourceIntent,
    session: Session,
    connection: StereoConsentControlIdentity,
    certificate: CertificateFingerprint,
    catalogs: Option<Catalogs>,
    original: Option<Route>,
    pending: Option<Route>,
    receipt: Option<StereoPreparedReceipt>,
    request_end: Instant,
    request_wall_end: u64,
    approved_end: Option<Instant>,
    effective_end: Option<Instant>,
    last_wall: u64,
    last_instant: Instant,
    ready: bool,
    terminal: bool,
    cleanup_errors: Vec<String>,
}
impl SecureStereoSourceHandshake {
    pub fn propose(
        runtime: NodeRuntime,
        link: CatalogControlLink,
        intent: SecureStereoSourceIntent,
    ) -> Result<Self, Box<SecureStereoSourceFailure>> {
        Self::begin(runtime, link, intent, true)
    }
    pub fn await_proposal(
        runtime: NodeRuntime,
        link: CatalogControlLink,
        intent: SecureStereoSourceIntent,
    ) -> Result<Self, Box<SecureStereoSourceFailure>> {
        Self::begin(runtime, link, intent, false)
    }
    fn begin(
        mut runtime: NodeRuntime,
        mut link: CatalogControlLink,
        intent: SecureStereoSourceIntent,
        propose: bool,
    ) -> Result<Self, Box<SecureStereoSourceFailure>> {
        let mut target = None;
        let prepared = (|| -> io::Result<_> {
            crate::secure_capture::validate_source_intent(
                &intent.pipewire_target,
                intent.lifetime,
            )?;
            let (wall, instant) = clock()?;
            let session = runtime.session(link.session_id()).map_err(error)?.clone();
            if session.state != SessionState::Ready
                || session.local_node_id != intent.source.node_id
                || session.remote_node_id != intent.sink.node_id
                || link.peer_node_id() != intent.sink.node_id
                || runtime
                    .snapshot()
                    .routes
                    .iter()
                    .any(|r| r.session_id == session.id)
            {
                return Err(error(
                    "bilateral stereo requires an empty dedicated source Session",
                ));
            }
            let certificate = link.authenticated_peer_certificate().map_err(error)?;
            let connection = link.stereo_consent_control_identity().map_err(error)?;
            let route = if propose {
                let id = runtime
                    .create_route(
                        session.id,
                        intent.source,
                        intent.sink,
                        RouteBackend::AdapterManaged,
                    )
                    .map_err(error)?;
                target = Some(id);
                let route = runtime.route(id).map_err(error)?.clone();
                validate_pending(&runtime, &route, &intent)?;
                link.send_route_status_until(&route, instant + MAX_STALL)
                    .map_err(error)?;
                Some(route)
            } else {
                None
            };
            let catalogs = route
                .as_ref()
                .map(|r| crate::secure_capture::catalogs(&runtime, r))
                .transpose()?;
            Ok((
                wall,
                instant,
                session,
                certificate,
                connection,
                route,
                catalogs,
            ))
        })();
        match prepared {
            Ok((wall, instant, session, certificate, connection, route, catalogs)) => Ok(Self {
                runtime: Some(runtime),
                link: Some(link),
                intent,
                session,
                certificate,
                connection,
                catalogs,
                pending: route.clone(),
                original: route,
                receipt: None,
                request_end: instant + CONSENT,
                request_wall_end: wall.saturating_add(60_000),
                approved_end: None,
                effective_end: None,
                last_wall: wall,
                last_instant: instant,
                ready: false,
                terminal: false,
                cleanup_errors: Vec::new(),
            }),
            Err(reason) => {
                let cleanup_errors = retire_and_close(&mut runtime, &mut link, target);
                Err(Box::new(SecureStereoSourceFailure {
                    error: reason,
                    cleanup_errors,
                    runtime,
                }))
            }
        }
    }
    pub fn runtime(&self) -> &NodeRuntime {
        self.runtime.as_ref().expect("owned handshake Runtime")
    }
    pub fn route_id(&self) -> Option<RouteId> {
        self.original.as_ref().map(|r| r.id)
    }
    pub fn approved_end(&self) -> Option<Instant> {
        self.approved_end
    }
    pub fn is_terminal(&self) -> bool {
        self.terminal
    }
    pub fn cleanup_errors(&self) -> &[String] {
        &self.cleanup_errors
    }
    /// Explicit bounded cancellation. Unlike Drop, reports retirement or control
    /// cleanup failure; callers must not acknowledge a clean Stop after an error.
    /// The authoritative Runtime and cleanup diagnostics remain inspectable.
    pub fn cancel(&mut self) -> io::Result<()> {
        if !self.terminal {
            let _ = self.fail(error("bilateral source explicitly cancelled"));
        }
        if self.cleanup_errors.is_empty() {
            Ok(())
        } else {
            Err(error("bilateral source cancellation cleanup failed"))
        }
    }
    pub fn state(&self) -> SecureStereoSourceHandshakeEvent {
        if self.terminal {
            SecureStereoSourceHandshakeEvent::Terminal
        } else if self.ready {
            SecureStereoSourceHandshakeEvent::Ready
        } else if self.approved_end.is_some() {
            SecureStereoSourceHandshakeEvent::AwaitingPeer
        } else if let Some(route_id) = self.route_id() {
            SecureStereoSourceHandshakeEvent::PendingApproval { route_id }
        } else {
            SecureStereoSourceHandshakeEvent::AwaitingProposal
        }
    }
    fn check_at(&mut self, wall: u64, instant: Instant) -> io::Result<()> {
        if self.terminal || wall < self.last_wall || instant < self.last_instant {
            return Err(error(
                "bilateral stereo authority retired or clock regressed",
            ));
        }
        let end = self.effective_end.unwrap_or(self.request_end);
        if instant >= end || (self.approved_end.is_none() && wall >= self.request_wall_end) {
            return Err(error("bilateral stereo original deadline elapsed"));
        }
        let link = self.link.as_ref().expect("owned link");
        link.validate_stereo_consent_control_identity(&self.connection)
            .map_err(error)?;
        if link.authenticated_peer_certificate().map_err(error)? != self.certificate
            || self.runtime().session(self.session.id).map_err(error)? != &self.session
        {
            return Err(error("bilateral stereo current control changed"));
        }
        let routes = self
            .runtime()
            .snapshot()
            .routes
            .into_iter()
            .filter(|r| r.session_id == self.session.id)
            .count();
        if routes != usize::from(self.original.is_some()) {
            return Err(error("bilateral stereo dedicated Session changed"));
        }
        if let Some(original) = &self.original {
            if self.runtime().route(original.id).map_err(error)? != original
                || Some(crate::secure_capture::catalogs(self.runtime(), original)?) != self.catalogs
                || self
                    .runtime()
                    .route_port_lease_claim(original.id, original.source)
                    .is_some()
                || self
                    .runtime()
                    .route_port_lease_claim(original.id, original.sink)
                    .is_some()
            {
                return Err(error("bilateral stereo original Route or catalog changed"));
            }
            validate_endpoints(self.runtime(), original)?;
            if let AuthorizationState::Authorized {
                expires_at_ms: Some(expiry),
            } = original.authorization
                && wall >= expiry
            {
                return Err(error("bilateral stereo grant elapsed"));
            }
        }
        if let Some(receipt) = &self.receipt {
            link.validate_stereo_prepared_receipt(receipt)
                .map_err(error)?;
            if wall < receipt.received_wall_ms()
                || instant < receipt.received_instant()
                || instant >= receipt.monotonic_deadline()
            {
                return Err(error("bilateral stereo original receipt clock invalid"));
            }
        }
        self.last_wall = wall;
        self.last_instant = instant;
        Ok(())
    }
    fn fail(&mut self, reason: io::Error) -> io::Error {
        self.terminal = true;
        let target = self.route_id();
        if let (Some(runtime), Some(link)) = (&mut self.runtime, &mut self.link) {
            self.cleanup_errors
                .extend(retire_and_close(runtime, link, target));
        }
        reason
    }
    /// Explicit fresh local source decision. There is no remembered approval.
    pub fn approve(&mut self, allow: bool) -> io::Result<()> {
        let result = (|| {
            let (wall, instant) = clock()?;
            self.check_at(wall, instant)?;
            if self.approved_end.is_some() || self.ready {
                return Err(error("source approval already consumed"));
            }
            let id = self
                .route_id()
                .ok_or_else(|| error("source proposal absent"))?;
            if !allow {
                return Err(error("source consent denied"));
            }
            let expiry = wall
                .checked_add(self.intent.lifetime.as_millis() as u64)
                .ok_or_else(|| error("source consent clock overflow"))?;
            self.approved_end = Some(instant + self.intent.lifetime);
            self.effective_end = self.approved_end;
            self.runtime
                .as_mut()
                .unwrap()
                .authorize_route(id, Some(expiry))
                .map_err(error)?;
            let route = self.runtime().route(id).map_err(error)?.clone();
            self.original = Some(route.clone());
            self.link
                .as_mut()
                .unwrap()
                .send_route_status_until(
                    &route,
                    (instant + MAX_STALL).min(self.approved_end.unwrap()),
                )
                .map_err(error)?;
            Ok(())
        })();
        result.map_err(|reason| self.fail(reason))
    }
    /// Reads at most one authenticated message. Once Ready it does not read the
    /// queued peer Source Acquire, which belongs to the actual Source owner.
    pub fn poll(&mut self) -> io::Result<SecureStereoSourceHandshakeEvent> {
        let result = self.poll_inner();
        result.map_err(|reason| self.fail(reason))
    }
    fn poll_inner(&mut self) -> io::Result<SecureStereoSourceHandshakeEvent> {
        let (wall, instant) = clock()?;
        self.check_at(wall, instant)?;
        if self.ready {
            return Ok(self.state());
        }
        let deadline = self
            .effective_end
            .unwrap_or(self.request_end)
            .min(instant + MAX_STALL);
        let message = self
            .link
            .as_mut()
            .unwrap()
            .poll_control_message_until(deadline)
            .map_err(error)?;
        if let Some(message) = message {
            match message {
                ReceivedControlMessage::RouteStatus(status) => self.receive_status(*status)?,
                ReceivedControlMessage::Heartbeat(heartbeat) => {
                    if !heartbeat.acknowledgement {
                        self.link
                            .as_mut()
                            .unwrap()
                            .send_heartbeat_until(heartbeat.sequence, true, deadline)
                            .map_err(error)?;
                    }
                }
                ReceivedControlMessage::Catalog(catalog) => {
                    self.link
                        .as_mut()
                        .unwrap()
                        .apply_catalog(self.runtime.as_mut().unwrap(), catalog)
                        .map_err(error)?;
                }
                _ => return Err(error("unexpected bilateral source consent message")),
            }
        }
        let (wall, instant) = clock()?;
        self.check_at(wall, instant)?;
        Ok(self.state())
    }
    fn receive_status(&mut self, status: ValidatedRouteStatus) -> io::Result<()> {
        if !no_offer(&status) || status.session_id != self.session.id {
            return Err(error("invalid bilateral source proposal fields"));
        }
        if self.original.is_none() {
            if status.route.source != self.intent.source || status.route.sink != self.intent.sink {
                return Err(error("unexpected bilateral source proposal endpoints"));
            }
            self.link
                .as_ref()
                .unwrap()
                .apply_route_status(self.runtime.as_mut().unwrap(), &status, false)
                .map_err(error)?;
            // Record the owned target immediately so every subsequent failure retires it.
            self.original = Some(
                self.runtime()
                    .route(status.route.id)
                    .map_err(error)?
                    .clone(),
            );
            let route = self.original.as_ref().unwrap();
            validate_pending(self.runtime(), route, &self.intent)?;
            if route != &status.route {
                return Err(error("bilateral Pending proposal is not exact"));
            }
            self.catalogs = Some(crate::secure_capture::catalogs(self.runtime(), route)?);
            self.pending = Some(route.clone());
            return Ok(());
        }
        if self.pending.as_ref() == Some(&status.route) {
            return Ok(());
        }
        if self.approved_end.is_none() {
            return Err(error("peer Prepared preceded local source consent"));
        }
        let receipt = self
            .link
            .as_mut()
            .unwrap()
            .take_received_stereo_prepared(&status)
            .map_err(error)?;
        let (wall, instant) = clock()?;
        self.check_at(wall, instant)?;
        let original = self.original.as_ref().unwrap();
        let AuthorizationState::Authorized {
            expires_at_ms: Some(local_expiry),
        } = original.authorization
        else {
            return Err(error("original source grant absent"));
        };
        let AuthorizationState::Authorized {
            expires_at_ms: Some(peer_expiry),
        } = receipt.status().route.authorization
        else {
            return Err(error("peer finite Prepared grant absent"));
        };
        let mut expected = original.clone();
        expected.authorization = AuthorizationState::Authorized {
            expires_at_ms: Some(peer_expiry),
        };
        expected.state = RouteState::Prepared;
        expected.selected_format = Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT));
        expected.selected_qos_mode = Some(QosMode::Interactive);
        if expected != receipt.status().route
            || expected.epoch != 0
            || peer_expiry > local_expiry
            || peer_expiry <= wall
            || receipt.binding() != self.link.as_ref().unwrap().tls_binding()
            || receipt.peer_certificate() != self.certificate
            || receipt.received_wall_ms() > wall
            || receipt.received_instant() > instant
            || receipt.monotonic_deadline() <= instant
        {
            return Err(error("peer Prepared changed exact bilateral authority"));
        }
        let id = expected.id;
        self.effective_end = Some(self.approved_end.unwrap().min(receipt.monotonic_deadline()));
        self.receipt = Some(receipt);
        let runtime = self.runtime.as_mut().unwrap();
        runtime
            .restrict_draft_authorization(id, peer_expiry, wall)
            .map_err(error)?;
        runtime
            .prepare_route(
                id,
                expected.selected_format.clone(),
                QosMode::Interactive,
                wall,
            )
            .map_err(error)?;
        self.original = Some(expected);
        self.ready = true;
        // Deliberately no Prepared echo: the peer retains its original Draft receipt.
        Ok(())
    }
    pub fn into_owner(
        mut self,
        identity: ControlTlsClientIdentity,
        server_name: String,
    ) -> Result<SecureStereoSourceOwner, Box<SecureStereoSourceFailure>> {
        let validated = (|| {
            let (wall, instant) = clock()?;
            self.check_at(wall, instant)?;
            if !self.ready {
                return Err(error("bilateral source is not Prepared"));
            }
            Ok(())
        })();
        if let Err(reason) = validated {
            let reason = self.fail(reason);
            return Err(Box::new(SecureStereoSourceFailure {
                error: reason,
                cleanup_errors: std::mem::take(&mut self.cleanup_errors),
                runtime: self.runtime.take().unwrap(),
            }));
        }
        let admission = SecureStereoReceiverAssignedSourceAdmission {
            expected: self.link.as_ref().unwrap().tls_binding(),
            route_id: self.route_id().unwrap(),
            lifetime: self.intent.lifetime,
            pipewire_target: self.intent.pipewire_target.clone(),
        };
        self.terminal = true;
        SecureStereoSourceOwner::new_bilateral_until(
            self.runtime.take().unwrap(),
            self.link.take().unwrap(),
            identity,
            server_name,
            admission,
            self.effective_end.unwrap(),
            self.last_wall,
        )
    }
}
impl Drop for SecureStereoSourceHandshake {
    fn drop(&mut self) {
        if !self.terminal && self.runtime.is_some() && self.link.is_some() {
            let _ = self.fail(error("bilateral source handshake dropped"));
        }
    }
}

pub(crate) fn retire_and_close(
    runtime: &mut NodeRuntime,
    link: &mut CatalogControlLink,
    target: Option<RouteId>,
) -> Vec<String> {
    let mut errors = Vec::new();
    if let Some(id) = target {
        let retirement = match runtime.route(id) {
            Ok(route)
                if route.state == RouteState::Draft
                    && matches!(
                        route.authorization,
                        AuthorizationState::Pending | AuthorizationState::Denied
                    ) =>
            {
                runtime.deny_pending_route(id)
            }
            Ok(_) => runtime.retire_route_authorization(id),
            Err(reason) => Err(reason),
        };
        if let Err(reason) = retirement {
            errors.push(reason.to_string());
        }
    }
    if let Err(reason) = link.close_and_reconcile(runtime) {
        errors.push(reason.to_string());
    }
    errors
}
fn no_offer(status: &ValidatedRouteStatus) -> bool {
    status.touchpad_data.is_none()
        && status.haptics_data.is_none()
        && status.native_audio_data.is_none()
        && status.camera_data.is_none()
        && status.secure_camera_data.is_none()
        && status.secure_audio_data.is_none()
}
fn validate_pending(
    runtime: &NodeRuntime,
    route: &Route,
    intent: &SecureStereoSourceIntent,
) -> io::Result<()> {
    if route.source != intent.source
        || route.sink != intent.sink
        || route.state != RouteState::Draft
        || route.epoch != 0
        || route.authorization != AuthorizationState::Pending
        || route.profile != ProfileId::audio_frames_v1()
        || route.backend != RouteBackend::AdapterManaged
        || route.selected_format.is_some()
        || route.selected_qos_mode.is_some()
        || route.compatible_formats != [FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)]
        || route.compatible_qos_modes != [QosMode::Interactive].into_iter().collect()
    {
        return Err(error("exact Pending stereo source proposal required"));
    }
    crate::secure_capture::catalogs(runtime, route)?;
    validate_endpoints(runtime, route)
}
fn validate_endpoints(runtime: &NodeRuntime, route: &Route) -> io::Result<()> {
    let snapshot = runtime.snapshot();
    let source = &snapshot.local_node;
    let sink = snapshot
        .peers
        .iter()
        .find(|n| n.id == route.sink.node_id)
        .ok_or_else(|| error("Android sink absent"))?;
    let source_cap = source
        .capabilities
        .get(&route.source.capability_id)
        .ok_or_else(|| error("source Capability absent"))?;
    let sink_cap = sink
        .capabilities
        .get(&route.sink.capability_id)
        .ok_or_else(|| error("sink Capability absent"))?;
    let revision = sink_cap
        .metadata
        .get("speaker.selectionRevision")
        .ok_or_else(|| error("speaker selection absent"))?;
    let value = revision.parse::<u64>().map_err(error)?;
    if source.id != route.source.node_id
        || source.platform != Platform::Linux
        || sink.platform != Platform::Android
        || source_cap.class != CapabilityClass::Custom("system_audio_capture".into())
        || sink_cap.class != CapabilityClass::Speaker
        || sink_cap.metadata.get("speaker.profile").map(String::as_str)
            != Some("secure_stereo_tls13_v1")
        || value == 0
        || value > 9_007_199_254_740_991
        || value.to_string() != *revision
    {
        return Err(error(
            "explicit current Linux Source and Android speaker selection required",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "secure_capture_consent_tests.rs"]
mod tests;
