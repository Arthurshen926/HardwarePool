//! Reusable, finite secure-stereo Sink. This is deliberately NOT the Android JNI
//! dispatcher. The shared supervisor uses the same pure authority and PCM
//! helpers but borrows its authoritative Runtime and uses staged control actions.
//! Application playback still needs visible consent/lifecycle and a separately
//! implemented nonblocking AudioTrack boundary.
//! This API never advertises a capability, opens AudioTrack or reports Active.
use crate::stereo_admission::*;
use capyio_audio::{AudioMediaStreamBinding, AudioStreamSpec};
use capyio_control_link::{CatalogControlLink, CatalogLinkError, ReceivedControlMessage};
use capyio_control_tls::{
    ControlTlsBinding, ControlTlsError, ControlTlsServerIdentity,
    audio_stereo::{StereoAudioTlsBinding, StereoAudioTlsListener, StereoAudioTlsStopHandle},
    control_poll_timeout_deadline,
};
use capyio_core::{MessageId, Route, RouteId, RouteState, Session, StreamId};
use capyio_native_audio_lan::{SecureStereoPcmAdmission, SecureStereoPcmRateBudget};
use capyio_protocol::{
    PortLeaseAction, RouteCommandAction, SecureAudioDataOffer, SecureAudioTransport,
    ValidatedPortLeaseResponse,
};
#[cfg(test)]
use capyio_protocol::{PortLeaseOutcome, SECURE_AUDIO_STEREO_FORMAT};
use capyio_runtime::{NodeRuntime, PortLeaseClaim};
use std::{
    io,
    net::{IpAddr, SocketAddr, TcpListener},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug)]
struct TerminalFault(&'static str);
impl std::fmt::Display for TerminalFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for TerminalFault {}
pub(crate) fn terminal_error(cause: &'static str) -> io::Error {
    io::Error::other(TerminalFault(cause))
}
fn closed_io(kind: io::ErrorKind) -> bool {
    matches!(
        kind,
        io::ErrorKind::UnexpectedEof
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::BrokenPipe
    )
}
// The original authority always wins over the per-record write budget. Keep
// this check before control polling: an already expired PCM budget is not a
// control transport failure merely because it also limits that poll's deadline.
fn check_pending_pcm_budget(
    authority: io::Result<()>,
    pending_deadline: Option<Instant>,
    now: Instant,
) -> io::Result<()> {
    authority?;
    if pending_deadline.is_some_and(|deadline| now >= deadline) {
        return Err(terminal_error("sink_backpressure"));
    }
    Ok(())
}

fn control_poll_failure_cause(
    authority: io::Result<()>,
    failure: &CatalogLinkError,
    requested_deadline: Instant,
    pending_deadline: Option<Instant>,
    // Production obtains this only from control_poll_timeout_deadline, not from
    // error text or from the requested budget; retained partial frames may have
    // an earlier effective deadline.
    trusted_effective_deadline: Option<Instant>,
    now: Instant,
) -> io::Result<&'static str> {
    authority?;
    Ok(match failure {
        CatalogLinkError::Closed => "control_closed",
        CatalogLinkError::Transport(ControlTlsError::Io(e)) if closed_io(e.kind()) => {
            "control_closed"
        }
        CatalogLinkError::Transport(ControlTlsError::Io(e))
            if e.kind() == io::ErrorKind::TimedOut
                && pending_deadline.is_some_and(|pending| {
                    trusted_effective_deadline == Some(pending)
                        && requested_deadline == pending
                        && now >= pending
                }) =>
        {
            "sink_backpressure"
        }
        _ => "control_failure",
    })
}

fn media_error(error: ControlTlsError) -> io::Error {
    match error {
        ControlTlsError::Io(e) => e,
        _ => terminal_error("media_failure"),
    }
}
pub const MAX_STEREO_OPERATION: Duration = Duration::from_millis(250);
fn error(reason: impl ToString) -> io::Error {
    io::Error::other(reason.to_string())
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

/// Portable endpoint, not a realtime callback. Every method must be nonblocking.
/// Only complete stereo sample frames may be accepted. Zero, WouldBlock and
/// Interrupted retain the same absolute record deadline; errors are terminal.
/// The owner cannot forcibly interrupt a malicious blocking implementation.
pub trait PcmSink {
    fn try_write_pcm(&mut self, bytes: &[u8]) -> io::Result<usize>;
    fn stop(&mut self) -> io::Result<()>;
}

#[derive(Clone, Debug)]
pub struct SecureStereoSinkAdmission {
    pub expected: ControlTlsBinding,
    pub route_id: RouteId,
    pub stream_id: StreamId,
    pub local_address: IpAddr,
    /// Exact whole-second local choice, 1..=60. Never inferred from peer authority.
    pub lifetime: Duration,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecureStereoSinkEvent {
    Listening,
    /// Exact TLS ACK only. Not evidence of playback or physical audibility.
    Bound,
    /// A complete 1,920-byte record was accepted by the supplied generic Sink.
    RecordAccepted,
    Stopped,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SecureStereoSinkMetrics {
    pub records_admitted: u64,
    pub records_accepted: u64,
    pub pcm_bytes_accepted: u64,
}
#[derive(Clone, Default)]
pub struct SecureStereoSinkStopHandle {
    stopped: Arc<AtomicBool>,
    transport: Arc<Mutex<Option<StereoAudioTlsStopHandle>>>,
}
impl SecureStereoSinkStopHandle {
    /// Independently interrupts accept, handshake and record I/O. The host must
    /// still poll/stop the owner to reconcile Runtime and stop the generic Sink.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        if let Ok(transport) = self.transport.lock()
            && let Some(transport) = transport.as_ref()
        {
            transport.stop();
        }
    }
    fn register(&self, transport: StereoAudioTlsStopHandle) -> io::Result<()> {
        let mut slot = self
            .transport
            .lock()
            .map_err(|_| error("stereo cancellation poisoned"))?;
        if slot.is_some() {
            return Err(error("stereo transport already registered"));
        }
        if self.stopped.load(Ordering::Acquire) {
            transport.stop();
        }
        *slot = Some(transport);
        Ok(())
    }
}
enum DataEvent {
    Bound,
    Record(Vec<u8>, Instant),
    Failed(io::Error),
}

/// One-shot dedicated control/Runtime owner. Both endpoints start Prepared with
/// no lease: each independently requests the other's first Acquire. A duplicate,
/// renewal or replacement fails closed. Actual authenticated Start plus both
/// original claims precede the media offer. The host must continuously poll and
/// invoke Stop on shutdown; this is not an unattended watchdog service.
pub struct SecureStereoSinkOwner<S: PcmSink> {
    runtime: NodeRuntime,
    link: CatalogControlLink,
    admission: SecureStereoSinkAdmission,
    original: Route,
    session: Session,
    endpoints: [EndpointAuthority; 2],
    sink_claim: Option<PortLeaseClaim>,
    source_claim: Option<ValidatedPortLeaseResponse>,
    source_request: MessageId,
    authenticated_start: bool,
    identity: Option<ControlTlsServerIdentity>,
    started: Instant,
    end: Instant,
    local_consent_end: Instant,
    negotiation_end: Instant,
    binding_end: Option<Instant>,
    last_wall: u64,
    stop: SecureStereoSinkStopHandle,
    listener_address: Option<SocketAddr>,
    local_offer: Option<SecureAudioDataOffer>,
    worker: Option<JoinHandle<()>>,
    events: Option<Receiver<DataEvent>>,
    reads: Option<SyncSender<Instant>>,
    read_pending: bool,
    bound: bool,
    pcm_enabled: bool,
    parser: Option<SecureStereoPcmAdmission>,
    rate: SecureStereoPcmRateBudget,
    pending: Option<PendingPcm>,
    sink: S,
    sink_stopped: bool,
    closed: bool,
    terminal: Option<(&'static str, Option<u64>)>,
    metrics: SecureStereoSinkMetrics,
}
impl<S: PcmSink> SecureStereoSinkOwner<S> {
    /// `original_consent_end` must be recorded before catalog/consent negotiation.
    /// Media listener identity comes from the current paired identity, independent
    /// of whether Android accepted or dialed this exact control connection.
    pub fn new_until(
        mut runtime: NodeRuntime,
        mut link: CatalogControlLink,
        identity: ControlTlsServerIdentity,
        admission: SecureStereoSinkAdmission,
        original_consent_end: Instant,
        mut sink: S,
    ) -> io::Result<Self> {
        let started = Instant::now();
        let prepared = (|| {
            validate_admission(&admission)?;
            let now = now_ms()?;
            let original = runtime.route(admission.route_id).map_err(error)?.clone();
            let session = runtime.session(original.session_id).map_err(error)?.clone();
            validate_route(&runtime, &original, &session, admission.expected, now)?;
            if original.state != RouteState::Prepared
                || runtime
                    .route_port_lease_claim(original.id, original.sink)
                    .is_some()
            {
                return Err(error(
                    "stereo Sink requires Prepared consent and no existing lease",
                ));
            }
            if link.tls_binding() != admission.expected
                || link.peer_node_id() != original.source.node_id
                || identity.expected_peer_certificate
                    != link.authenticated_peer_certificate().map_err(error)?
            {
                return Err(error(
                    "stereo Sink requires exact current paired control link",
                ));
            }
            link.authenticated_peer_addr().map_err(error)?;
            let endpoints = [
                EndpointAuthority::capture(&runtime, original.source)?,
                EndpointAuthority::capture(&runtime, original.sink)?,
            ];
            for endpoint in &endpoints {
                endpoint.check(&runtime, &original, false)?;
            }
            let end = consent_end(
                started,
                admission.lifetime,
                original_consent_end,
                expiry(&original)?
                    .checked_sub(now)
                    .ok_or_else(|| error("stereo consent expired"))?,
            )?;
            let negotiation_end = end.min(started + MAX_STEREO_OPERATION);
            let ttl = source_lease_ttl(end.duration_since(started))?;
            let source_request = link
                .send_port_lease_request_until(
                    original.id,
                    original.source,
                    PortLeaseAction::Acquire,
                    ttl,
                    negotiation_end,
                )
                .map_err(error)?;
            let rate = SecureStereoPcmRateBudget::new(admission.lifetime).map_err(error)?;
            Ok((
                now,
                original,
                session,
                endpoints,
                end,
                negotiation_end,
                source_request,
                rate,
            ))
        })();
        let (now, original, session, endpoints, end, negotiation_end, source_request, rate) =
            match prepared {
                Ok(prepared) => prepared,
                Err(reason) => {
                    let _ = link.close_and_reconcile(&mut runtime);
                    let _ = sink.stop();
                    return Err(reason);
                }
            };
        Ok(Self {
            runtime,
            link,
            admission,
            original,
            session,
            endpoints,
            sink_claim: None,
            source_claim: None,
            source_request,
            authenticated_start: false,
            identity: Some(identity),
            started,
            end,
            local_consent_end: end,
            negotiation_end,
            binding_end: None,
            last_wall: now,
            stop: Default::default(),
            listener_address: None,
            local_offer: None,
            worker: None,
            events: None,
            reads: None,
            read_pending: false,
            bound: false,
            pcm_enabled: false,
            parser: None,
            rate,
            pending: None,
            sink,
            sink_stopped: false,
            closed: false,
            terminal: None,
            metrics: Default::default(),
        })
    }
    pub fn runtime(&self) -> &NodeRuntime {
        &self.runtime
    }
    /// Mutations are never renewal. The next poll/partial write checks every
    /// original claim, Route, Session, Capability, Adapter and monotonic cap.
    pub fn runtime_mut(&mut self) -> &mut NodeRuntime {
        &mut self.runtime
    }
    pub fn stop_handle(&self) -> SecureStereoSinkStopHandle {
        self.stop.clone()
    }
    pub fn source_lease_expires_at_ms(&self) -> Option<u64> {
        self.source_claim.and_then(|c| c.expires_at_ms)
    }
    pub fn sink_lease_expires_at_ms(&self) -> Option<u64> {
        self.sink_claim.map(|c| c.expires_at_ms)
    }
    pub fn local_offer(&self) -> Option<SecureAudioDataOffer> {
        self.local_offer
    }
    pub fn listener_address(&self) -> Option<SocketAddr> {
        self.listener_address
    }
    pub fn metrics(&self) -> SecureStereoSinkMetrics {
        self.metrics
    }
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    pub fn terminal_cause(&self) -> Option<&'static str> {
        self.terminal.map(|t| t.0)
    }
    pub fn terminal_observed_at_ms(&self) -> Option<u64> {
        self.terminal.and_then(|t| t.1)
    }
    fn observe(&mut self, reason: &'static str) {
        if self.terminal.is_none() {
            self.terminal = Some((reason, now_ms().ok()));
        }
    }
    /// One-shot local readiness, after the caller observed Bound and prepared
    /// its generic Sink. Before this call no read permit or PCM write is issued.
    /// Waiting remains under the original finite authority and control polling;
    /// enabling never refreshes consent, leases or the immutable session cap.
    /// A premature, repeated or expired call fails this generation terminally.
    pub fn enable_pcm(&mut self) -> io::Result<()> {
        let result = (|| {
            self.check()?;
            validate_pcm_enable(self.bound, self.pcm_enabled, Instant::now(), self.end)?;
            // Reconcile current authenticated control/transport events before
            // admitting the first operation, not merely a cached Runtime view.
            self.poll_inner()?;
            self.check()?;
            self.pcm_enabled = true;
            self.request_read()
        })();
        self.finish_result(result)
    }
    pub fn poll(&mut self) -> io::Result<Option<SecureStereoSinkEvent>> {
        let result = self.poll_inner();
        self.finish_result(result)
    }
    fn finish_result<T>(&mut self, result: io::Result<T>) -> io::Result<T> {
        if let Err(reason) = &result {
            let cause = reason
                .get_ref()
                .and_then(|e| e.downcast_ref::<TerminalFault>())
                .map_or("failure", |fault| fault.0);
            self.observe(cause);
            let _ = self.stop();
        }
        result
    }
    fn poll_inner(&mut self) -> io::Result<Option<SecureStereoSinkEvent>> {
        let pending_deadline = self.pending.as_ref().map(|pending| pending.deadline);
        check_pending_pcm_budget(self.check(), pending_deadline, Instant::now())?;
        let deadline = self.operation_deadline();
        let incoming = match self.link.poll_control_message_until(deadline) {
            Ok(incoming) => incoming,
            Err(failure) => {
                // poll_control_message_until closes its link on every error.
                // Ignore only that consequential flag while rechecking original
                // authority, so expiry/revocation cannot become control_closed.
                let effective_deadline = match &failure {
                    CatalogLinkError::Transport(ControlTlsError::Io(e)) => {
                        control_poll_timeout_deadline(e)
                    }
                    _ => None,
                };
                let cause = control_poll_failure_cause(
                    self.check_authority(true),
                    &failure,
                    deadline,
                    pending_deadline,
                    effective_deadline,
                    Instant::now(),
                )?;
                self.observe(cause);
                return Err(error(failure));
            }
        };
        // A successful nonblocking control step may cross the PCM deadline.
        // Recheck original authority first and never grant the write a new cap.
        check_pending_pcm_budget(self.check(), pending_deadline, Instant::now())?;
        if let Some(message) = incoming
            && self.handle_control(message, deadline)?
        {
            self.stop()?;
            return Ok(Some(SecureStereoSinkEvent::Stopped));
        }
        self.check()?;
        if self.local_offer.is_none()
            && self.authenticated_start
            && self.sink_claim.is_some()
            && self.source_claim.is_some()
        {
            self.runtime
                .begin_route_start(self.original.id, now_ms()?)
                .map_err(error)?;
            let mut expected = self.original.clone();
            expected.state = RouteState::Starting;
            expected.epoch = expected
                .epoch
                .checked_add(1)
                .ok_or_else(|| error("stereo epoch overflow"))?;
            if self.runtime.route(expected.id).map_err(error)? != &expected {
                return Err(error("unexpected stereo Start transition"));
            }
            self.original = expected;
            self.listen()?;
            return Ok(Some(SecureStereoSinkEvent::Listening));
        }
        let event = match self.events.as_ref().map(Receiver::try_recv) {
            None | Some(Err(TryRecvError::Empty)) => None,
            Some(Err(TryRecvError::Disconnected)) => return Err(error("stereo transport ended")),
            Some(Ok(event)) => Some(event),
        };
        let mut result = None;
        match event {
            Some(DataEvent::Failed(e)) => {
                let cause = if closed_io(e.kind()) {
                    "media_closed"
                } else if matches!(
                    e.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) {
                    "media_timeout"
                } else {
                    "media_failure"
                };
                self.observe(cause);
                return Err(e);
            }
            Some(DataEvent::Bound) => {
                if self.bound {
                    return Err(error("duplicate stereo binding"));
                }
                self.bound = true;
                self.binding_end = None;
                result = Some(SecureStereoSinkEvent::Bound);
            }
            Some(DataEvent::Record(bytes, record_end)) => {
                if !self.bound || !self.pcm_enabled || !self.read_pending || self.pending.is_some()
                {
                    return Err(error("stereo record without exclusive permit"));
                }
                self.read_pending = false;
                self.rate
                    .admit(self.started.elapsed())
                    .map_err(|_| terminal_error("media_rate"))?;
                let packet = self
                    .parser
                    .as_mut()
                    .ok_or_else(|| error("stereo parser absent"))?
                    .admit_record(&bytes)
                    .map_err(|_| terminal_error("media_invalid"))?;
                self.pending = Some(
                    PendingPcm::new(packet.payload, Instant::now(), self.end.min(record_end))
                        .map_err(|_| terminal_error("sink_backpressure"))?,
                );
                self.metrics.records_admitted = self
                    .metrics
                    .records_admitted
                    .checked_add(1)
                    .ok_or_else(|| error("stereo counter overflow"))?;
            }
            None => {}
        }
        if self.flush()? {
            result = Some(SecureStereoSinkEvent::RecordAccepted);
        }
        self.request_read()?;
        Ok(result)
    }
    fn handle_control(
        &mut self,
        message: ReceivedControlMessage,
        deadline: Instant,
    ) -> io::Result<bool> {
        match message {
            ReceivedControlMessage::PortLeaseResponse(response) => {
                if self.source_claim.is_some() {
                    return Err(error("duplicate stereo Source claim"));
                }
                let anchor = Instant::now();
                let now = now_ms()?;
                let expiry =
                    validate_source_response(&self.original, self.source_request, response, now)?;
                self.source_claim = Some(response);
                self.end = self.end.min(anchor + Duration::from_millis(expiry - now));
            }
            ReceivedControlMessage::PortLeaseRequest(received) => {
                let request = received.request();
                let anchor = Instant::now();
                let now = now_ms()?;
                if self.sink_claim.is_some()
                    || request.action != PortLeaseAction::Acquire
                    || request.session_id != self.original.session_id
                    || request.route_id != self.original.id
                    || request.port != self.original.sink
                    || self
                        .runtime
                        .route_port_lease_claim(self.original.id, self.original.sink)
                        .is_some()
                    || now
                        .checked_add(request.requested_ttl_ms)
                        .is_none_or(|end| end > expiry(&self.original).unwrap_or(0))
                    || Duration::from_millis(request.requested_ttl_ms)
                        > self
                            .local_consent_end
                            .saturating_duration_since(Instant::now())
                {
                    return Err(error("unexpected stereo Sink lease request or renewal"));
                }
                self.link
                    .answer_port_lease_request_until(&mut self.runtime, received, now, deadline)
                    .map_err(error)?;
                let claim = *self
                    .runtime
                    .route_port_lease_claim(self.original.id, self.original.sink)
                    .ok_or_else(|| error("stereo Sink lease denied"))?;
                self.end = self.end.min(
                    anchor
                        + Duration::from_millis(
                            claim
                                .expires_at_ms
                                .checked_sub(now)
                                .ok_or_else(|| error("stereo Sink claim expired"))?,
                        ),
                );
                self.sink_claim = Some(claim);
            }
            ReceivedControlMessage::RouteCommand(command)
                if command.session_id == self.original.session_id
                    && command.route_id == self.original.id =>
            {
                match command.action {
                    RouteCommandAction::Stop => {
                        self.observe("peer_stop");
                        return Ok(true);
                    }
                    RouteCommandAction::Start
                        if !self.authenticated_start
                            && self.original.state == RouteState::Prepared =>
                    {
                        self.authenticated_start = true
                    }
                    _ => return Err(error("duplicate stereo Start")),
                }
            }
            ReceivedControlMessage::Heartbeat(h) => {
                if !h.acknowledgement {
                    self.link
                        .send_heartbeat_until(h.sequence, true, deadline)
                        .map_err(error)?;
                }
            }
            ReceivedControlMessage::Catalog(catalog) => {
                self.link
                    .apply_catalog(&mut self.runtime, catalog)
                    .map_err(error)?;
            }
            ReceivedControlMessage::RouteStatus(status)
                if status.route == self.original
                    && status.session_id == self.original.session_id
                    && status.secure_audio_data.is_none()
                    && status.native_audio_data.is_none()
                    && status.camera_data.is_none()
                    && status.secure_camera_data.is_none()
                    && status.touchpad_data.is_none()
                    && status.haptics_data.is_none() => {}
            _ => return Err(error("unexpected dedicated stereo control message")),
        }
        self.check()?;
        Ok(false)
    }
    fn check(&mut self) -> io::Result<()> {
        self.check_authority(false)
    }
    fn check_authority(&mut self, ignore_failed_control_close: bool) -> io::Result<()> {
        let control_closed = !ignore_failed_control_close && self.link.is_closed();
        let now = now_ms()?;
        let instant = Instant::now();
        if self.closed
            || self.stop.stopped.load(Ordering::Acquire)
            || control_closed
            || now < self.last_wall
            || instant >= self.end
            || (self.local_offer.is_none() && instant >= self.negotiation_end)
            || self.binding_end.is_some_and(|end| instant >= end)
        {
            return Err(terminal_error(
                if self.stop.stopped.load(Ordering::Acquire) {
                    "local_stop"
                } else if control_closed {
                    "control_closed"
                } else if instant >= self.end {
                    "authority_expired"
                } else {
                    "operation_timeout"
                },
            ));
        }
        if now >= expiry(&self.original)?
            || self.sink_claim.is_some_and(|c| now >= c.expires_at_ms)
            || self
                .source_claim
                .and_then(|c| c.expires_at_ms)
                .is_some_and(|end| now >= end)
        {
            return Err(terminal_error("authority_expired"));
        }
        validate_route(
            &self.runtime,
            &self.original,
            &self.session,
            self.admission.expected,
            now,
        )
        .map_err(|_| terminal_error("route_authority_changed"))?;
        match self.sink_claim {
            Some(claim) => validate_sink_claim(&self.runtime, &self.original, claim, now)
                .map_err(|_| terminal_error("sink_lease_changed"))?,
            None if self
                .runtime
                .route_port_lease_claim(self.original.id, self.original.sink)
                .is_none() => {}
            None => return Err(error("unclaimed stereo Sink lease appeared")),
        }
        if let Some(claim) = self.source_claim {
            validate_source_response(&self.original, self.source_request, claim, now)
                .map_err(|_| terminal_error("source_lease_invalid"))?;
        }
        for endpoint in &self.endpoints {
            endpoint.check(&self.runtime, &self.original, self.source_claim.is_some())?;
        }
        let fresh = now_ms()?;
        if fresh < now
            || fresh >= expiry(&self.original)?
            || self.sink_claim.is_some_and(|c| fresh >= c.expires_at_ms)
            || self
                .source_claim
                .and_then(|c| c.expires_at_ms)
                .is_some_and(|end| fresh >= end)
            || Instant::now() >= self.end
        {
            return Err(terminal_error("authority_expired"));
        }
        self.last_wall = fresh;
        Ok(())
    }
    fn operation_deadline(&self) -> Instant {
        let mut end = self.end.min(Instant::now() + MAX_STEREO_OPERATION);
        if self.local_offer.is_none() {
            end = end.min(self.negotiation_end);
        }
        if let Some(binding) = self.binding_end {
            end = end.min(binding);
        }
        if let Some(pending) = &self.pending {
            end = end.min(pending.deadline);
        }
        end
    }
    fn flush(&mut self) -> io::Result<bool> {
        if self.pending.is_none() {
            return Ok(false);
        }
        if !self.pcm_enabled {
            return Err(terminal_error("invalid_pcm_enable"));
        }
        self.check()?;
        let bytes = self
            .pending
            .as_mut()
            .expect("checked PCM")
            .before_write(Instant::now())
            .map_err(|_| terminal_error("sink_backpressure"))?;
        let outcome = self.sink.try_write_pcm(bytes);
        // Never account success, release the permit or call the Sink again when
        // authority or the immutable operation deadline elapsed inside a write.
        self.check()?;
        let (accepted, complete) = self
            .pending
            .as_mut()
            .expect("owned PCM")
            .after_write(outcome, Instant::now())
            .map_err(|_| terminal_error("sink_backpressure"))?;
        self.metrics.pcm_bytes_accepted = self
            .metrics
            .pcm_bytes_accepted
            .checked_add(accepted as u64)
            .ok_or_else(|| error("stereo PCM counter overflow"))?;
        if complete {
            self.pending = None;
            self.metrics.records_accepted += 1;
        }
        Ok(complete)
    }
    fn request_read(&mut self) -> io::Result<()> {
        if !self.bound || !self.pcm_enabled || self.pending.is_some() || self.read_pending {
            return Ok(());
        }
        self.check()?;
        self.reads
            .as_ref()
            .ok_or_else(|| error("stereo reader absent"))?
            .try_send(self.operation_deadline())
            .map_err(error)?;
        self.read_pending = true;
        Ok(())
    }
    fn listen(&mut self) -> io::Result<()> {
        self.check()?;
        let now = now_ms()?;
        let binding = StereoAudioTlsBinding::from_route(
            self.admission.expected,
            &self.original,
            self.admission.stream_id,
            now,
        )
        .map_err(error)?;
        self.parser = Some(
            SecureStereoPcmAdmission::new(AudioMediaStreamBinding {
                session_id: self.original.session_id,
                route_id: self.original.id,
                stream_id: self.admission.stream_id,
                stream_epoch: u32::try_from(self.original.epoch).map_err(error)?,
                selected_spec: AudioStreamSpec::media_balanced(),
            })
            .map_err(error)?,
        );
        let listener = StereoAudioTlsListener::from_listener_until(
            TcpListener::bind((self.admission.local_address, 0))?,
            self.identity
                .take()
                .ok_or_else(|| error("stereo identity consumed"))?,
            binding,
            MAX_STEREO_OPERATION,
            self.end,
        )
        .map_err(error)?;
        let address = listener.local_addr().map_err(error)?;
        self.stop.register(listener.stop_handle().map_err(error)?)?;
        self.check()?;
        let offer = SecureAudioDataOffer {
            port: address.port(),
            stream_id: self.admission.stream_id,
            transport: SecureAudioTransport::Tls13MtlsStereoV1,
        };
        let accept_end = self.operation_deadline();
        self.link
            .send_secure_stereo_audio_data_offer_until(&self.original, offer, now, accept_end)
            .map_err(error)?;
        self.check()?;
        self.binding_end = Some(accept_end);
        self.listener_address = Some(address);
        self.local_offer = Some(offer);
        let (tx, rx) = mpsc::sync_channel(1);
        let (read_tx, read_rx) = mpsc::sync_channel::<Instant>(1);
        self.events = Some(rx);
        self.reads = Some(read_tx);
        self.worker = Some(
            thread::Builder::new()
                .name("capyio-stereo-sink".into())
                .spawn(move || {
                    let result = (|| -> io::Result<()> {
                        let mut channel = loop {
                            if Instant::now() >= accept_end {
                                return Err(error("stereo accept deadline"));
                            }
                            if let Some(channel) =
                                listener.try_accept_until(accept_end).map_err(error)?
                            {
                                break channel;
                            }
                            thread::sleep(Duration::from_millis(2));
                        };
                        if Instant::now() >= accept_end {
                            return Err(error("stereo binding deadline"));
                        }
                        if tx.send(DataEvent::Bound).is_err() {
                            return Ok(());
                        }
                        while let Ok(deadline) = read_rx.recv() {
                            if Instant::now() >= deadline {
                                return Err(error("stereo read permit expired"));
                            }
                            let bytes = channel
                                .receive_record_until(deadline)
                                .map_err(media_error)?;
                            if tx.send(DataEvent::Record(bytes, deadline)).is_err() {
                                break;
                            }
                        }
                        Ok(())
                    })();
                    if let Err(e) = result {
                        let _ = tx.send(DataEvent::Failed(e));
                    }
                })?,
        );
        Ok(())
    }
    pub fn stop_after_failure(&mut self) -> io::Result<()> {
        self.observe("failure");
        self.stop()
    }
    /// Idempotent terminal cleanup. Failed joins retain the owned JoinHandle for
    /// a later reap; no replacement generation or reconnection is possible.
    pub fn stop(&mut self) -> io::Result<()> {
        self.observe("local_stop");
        self.closed = true;
        self.stop.stop();
        self.pending = None;
        self.reads.take();
        self.events.take();
        let control = self
            .link
            .close_and_reconcile(&mut self.runtime)
            .map_err(error);
        let sink = if self.sink_stopped {
            Ok(())
        } else {
            let result = self.sink.stop();
            if result.is_ok() {
                self.sink_stopped = true;
            }
            result
        };
        let network = if let Some(worker) = self.worker.as_ref() {
            let deadline = Instant::now() + MAX_STEREO_OPERATION;
            while !worker.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(1));
            }
            if worker.is_finished() {
                self.worker
                    .take()
                    .expect("owned worker")
                    .join()
                    .map_err(|_| error("stereo worker panicked"))
            } else {
                Err(error("stereo worker cleanup pending"))
            }
        } else {
            Ok(())
        };
        control.and(sink).and(network)
    }
}
impl<S: PcmSink> Drop for SecureStereoSinkOwner<S> {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
#[path = "secure_stereo_receiver_tests.rs"]
mod tests;
