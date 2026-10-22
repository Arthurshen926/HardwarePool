//! Finite mono playback or owned virtual microphone over pinned TLS data. No default
//! listener, pairing, capture permission, UDP fallback or authority renewal.
use crate::{
    EndpointMode, MAX_STALL, PcmPlaybackSink, PipeWirePlayback, PlaybackMetrics, PlaybackPump,
};
use capyio_audio::{AudioMediaStreamBinding, AudioStreamSpec};
use capyio_control_link::{CatalogControlLink, CatalogLinkError, ReceivedControlMessage};
use capyio_control_tls::{
    ControlTlsBinding, ControlTlsError, ControlTlsServerIdentity,
    audio::{AudioTlsBinding, AudioTlsListener, AudioTlsStopHandle},
};
use capyio_core::{
    AuthorizationState, Availability, MessageId, Platform, PortDirection, Route, RouteId,
    RouteState, SessionState, StreamId,
};
use capyio_native_audio_lan::{SecureMonoPcmAdmission, SecureMonoPcmRateBudget};
use capyio_protocol::{
    PortLeaseAction, PortLeaseOutcome, RouteCommandAction, SecureAudioDataOffer,
    SecureAudioSessionProfile, SecureAudioTransport,
};
use capyio_runtime::{NodeRuntime, PortLeaseClaim};
use std::{
    io,
    net::{IpAddr, SocketAddr, TcpListener},
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

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

/// Trusted, exact local consent. Neither this type nor the peer offer creates a
/// grant or lease. `pipewire_target` is the locally selected node name/serial,
/// never remote metadata. Explicit virtual mode instead requires the exact name
/// derived from the original local Sink Port ID. `identity` must be the paired identity
/// used to admit the accepted control link.
#[derive(Clone, Debug)]
pub struct SecurePlaybackAdmission {
    pub expected: ControlTlsBinding,
    pub route_id: RouteId,
    pub stream_id: StreamId,
    pub local_address: IpAddr,
    /// Whole seconds within the explicitly selected profile; old constructors cap at 60.
    pub lifetime: Duration,
    pub pipewire_target: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecurePlaybackEvent {
    Listening,
    /// Exact TLS binding ACK completed and the owned playback helper opened.
    /// This is not evidence of audible consumption; the Route stays Starting.
    Bound,
    Record,
    Stopped,
}
enum DataEvent {
    Bound,
    Record(Vec<u8>),
    Failed(io::Error),
}

/// Preserve the first observation even if its wall clock was unavailable. A
/// retry after cleanup must never replace a missing timestamp with a later one.
#[derive(Default)]
struct TerminalObservation {
    recorded: bool,
    at_ms: Option<u64>,
    cause: Option<TerminalCause>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalCause {
    Failure,
    LocalStop,
    PeerStop,
    MediaClosed,
    MediaTimeout,
    MediaFailure,
    ControlClosed,
}
impl TerminalCause {
    fn name(self) -> &'static str {
        match self {
            Self::Failure => "failure",
            Self::LocalStop => "local_stop",
            Self::PeerStop => "peer_stop",
            Self::MediaClosed => "media_closed",
            Self::MediaTimeout => "media_timeout",
            Self::MediaFailure => "media_failure",
            Self::ControlClosed => "control_closed",
        }
    }
}
fn closed_kind(kind: io::ErrorKind) -> bool {
    matches!(
        kind,
        io::ErrorKind::UnexpectedEof
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
    )
}
fn media_cause(reason: &io::Error) -> TerminalCause {
    if closed_kind(reason.kind()) {
        TerminalCause::MediaClosed
    } else if matches!(
        reason.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    ) {
        TerminalCause::MediaTimeout
    } else {
        TerminalCause::MediaFailure
    }
}
fn control_cause(reason: &CatalogLinkError) -> TerminalCause {
    match reason {
        CatalogLinkError::Closed => TerminalCause::ControlClosed,
        CatalogLinkError::Transport(ControlTlsError::Io(reason)) if closed_kind(reason.kind()) => {
            TerminalCause::ControlClosed
        }
        _ => TerminalCause::Failure,
    }
}
fn media_error(reason: ControlTlsError) -> io::Error {
    match reason {
        ControlTlsError::Io(reason) => reason,
        _ => error("secure audio TLS failure"),
    }
}
impl TerminalObservation {
    fn observe(&mut self, cause: TerminalCause) {
        self.record(now_ms().ok());
        self.cause.get_or_insert(cause);
    }
    fn record(&mut self, at_ms: Option<u64>) {
        if !self.recorded {
            self.recorded = true;
            self.at_ms = at_ms;
        }
    }
}

/// The caller must continuously poll and call `stop` on owner shutdown. All
/// blocking network reads run on one independently cancellable worker; each
/// record requires a newly validated owner-issued read permit. One event slot,
/// one outstanding read and one PCM frame bound buffering. No real-time callback
/// performs network I/O. Stopping the caller's polling is not a watchdog service.
pub struct SecurePlaybackOwner {
    runtime: NodeRuntime,
    link: CatalogControlLink,
    admission: SecurePlaybackAdmission,
    original: Route,
    endpoint_mode: EndpointMode,
    sink_claim: PortLeaseClaim,
    identity: Option<ControlTlsServerIdentity>,
    source_request: MessageId,
    source_request_deadline: Instant,
    source_expiry: Option<u64>,
    source_claim: Option<capyio_protocol::ValidatedPortLeaseResponse>,
    started: Instant,
    end: Instant,
    last_wall: u64,
    binding_deadline: Option<Instant>,
    local_offer: Option<SecureAudioDataOffer>,
    listener_address: Option<SocketAddr>,
    transport_stop: Option<AudioTlsStopHandle>,
    worker: Option<JoinHandle<()>>,
    events: Option<Receiver<DataEvent>>,
    reads: Option<SyncSender<Instant>>,
    read_pending: bool,
    admission_check: SecureMonoPcmAdmission,
    pump: PlaybackPump,
    rate: SecureMonoPcmRateBudget,
    sink: Option<PipeWirePlayback>,
    closed: bool,
    terminal_observation: TerminalObservation,
}
impl SecurePlaybackOwner {
    /// Consume one already accepted pinned control link, exact Ready Session,
    /// locally authorized Starting Route, and preexisting local Sink lease.
    /// The remote Source lease is requested on that actual control link; only
    /// its matching successful response permits opening a data listener.
    pub fn new(
        runtime: NodeRuntime,
        link: CatalogControlLink,
        identity: ControlTlsServerIdentity,
        admission: SecurePlaybackAdmission,
    ) -> io::Result<Self> {
        Self::new_with_mode(runtime, link, identity, admission, EndpointMode::Playback)
    }

    /// Explicit local endpoint intent. Virtual mode accepts only the canonical
    /// name derived from this Route's original local Sink, never a physical target.
    pub fn new_with_mode(
        runtime: NodeRuntime,
        link: CatalogControlLink,
        identity: ControlTlsServerIdentity,
        admission: SecurePlaybackAdmission,
        endpoint_mode: EndpointMode,
    ) -> io::Result<Self> {
        // Validate finite lifetime before Instant arithmetic, including Duration::MAX.
        let original_end = admission_end(&admission)?;
        Self::new_until_with_mode(
            runtime,
            link,
            identity,
            admission,
            original_end,
            endpoint_mode,
        )
    }

    /// Preserve a caller's original consent-time monotonic deadline across
    /// catalog/grant negotiation. This can only shorten the owner's lifetime.
    pub fn new_until(
        runtime: NodeRuntime,
        link: CatalogControlLink,
        identity: ControlTlsServerIdentity,
        admission: SecurePlaybackAdmission,
        original_end: Instant,
    ) -> io::Result<Self> {
        Self::new_until_with_mode(
            runtime,
            link,
            identity,
            admission,
            original_end,
            EndpointMode::Playback,
        )
    }

    /// Explicit mode with the caller's original consent deadline. Neither mode
    /// selection nor a completed binding grants authority or renews any lease.
    pub fn new_until_with_mode(
        runtime: NodeRuntime,
        link: CatalogControlLink,
        identity: ControlTlsServerIdentity,
        admission: SecurePlaybackAdmission,
        original_end: Instant,
        endpoint_mode: EndpointMode,
    ) -> io::Result<Self> {
        Self::new_until_with_profile(
            runtime,
            link,
            identity,
            admission,
            original_end,
            endpoint_mode,
            SecureAudioSessionProfile::Finite60sV1,
        )
    }

    /// Explicit profile selection. Duration never implies the five-minute profile.
    /// The original consent deadline and exact authenticated Route remain immutable.
    #[allow(clippy::too_many_arguments)]
    pub fn new_until_with_profile(
        mut runtime: NodeRuntime,
        mut link: CatalogControlLink,
        identity: ControlTlsServerIdentity,
        admission: SecurePlaybackAdmission,
        original_end: Instant,
        endpoint_mode: EndpointMode,
        profile: SecureAudioSessionProfile,
    ) -> io::Result<Self> {
        let started = Instant::now();
        if original_end <= started {
            return Err(error("original secure playback consent elapsed"));
        }
        let now = now_ms()?;
        let original = runtime.route(admission.route_id).map_err(error)?.clone();
        validate_mode_target_profile(&admission, endpoint_mode, &original, profile)?;
        validate_profile_route(&original, profile)?;
        let sink_claim = *runtime
            .route_port_lease_claim(original.id, original.sink)
            .ok_or_else(|| error("secure playback requires an existing local Sink lease"))?;
        validate_authority(
            &runtime,
            &original,
            admission.expected,
            &sink_claim,
            now,
            false,
        )?;
        if link.tls_binding() != admission.expected
            || link.peer_node_id() != original.source.node_id
            || link.accepted_peer_addr().is_err()
            || identity.expected_peer_certificate
                != link.authenticated_peer_certificate().map_err(error)?
        {
            return Err(error(
                "secure playback requires the exact accepted paired control link",
            ));
        }
        let media = media_binding(&original, admission.stream_id)?;
        let admission_check = SecureMonoPcmAdmission::new(media.clone()).map_err(error)?;
        let pump = PlaybackPump::with_binding(admission.lifetime, media)?;
        let rate = match profile {
            SecureAudioSessionProfile::Finite60sV1 => {
                SecureMonoPcmRateBudget::new(admission.lifetime)
            }
            SecureAudioSessionProfile::Finite300sV1 => {
                SecureMonoPcmRateBudget::new_finite_300s(admission.lifetime)
            }
        }
        .map_err(error)?;
        let authority_ms = expiry(&original)?.min(sink_claim.expires_at_ms);
        let end = consent_end(
            started,
            admission.lifetime,
            original_end,
            authority_ms - now,
        )?;
        let deadline = end.min(started + MAX_STALL);
        let ttl = source_lease_ttl(authority_ms, now, admission.lifetime)?;
        let source_request = match link.send_port_lease_request_until(
            original.id,
            original.source,
            PortLeaseAction::Acquire,
            ttl,
            deadline,
        ) {
            Ok(id) => id,
            Err(reason) => {
                let _ = link.close_and_reconcile(&mut runtime);
                return Err(error(reason));
            }
        };
        Ok(Self {
            runtime,
            link,
            admission,
            original,
            endpoint_mode,
            sink_claim,
            identity: Some(identity),
            source_request,
            source_request_deadline: deadline,
            source_expiry: None,
            source_claim: None,
            started,
            end,
            last_wall: now,
            binding_deadline: None,
            local_offer: None,
            listener_address: None,
            transport_stop: None,
            worker: None,
            events: None,
            reads: None,
            read_pending: false,
            admission_check,
            pump,
            rate,
            sink: None,
            closed: false,
            terminal_observation: TerminalObservation::default(),
        })
    }
    pub fn runtime(&self) -> &NodeRuntime {
        &self.runtime
    }
    /// A mutation is never a renewal: the next poll/write requires the original
    /// exact Route and Sink claim, and rejects any changed generation or expiry.
    pub fn runtime_mut(&mut self) -> &mut NodeRuntime {
        &mut self.runtime
    }
    pub fn source_lease_expires_at_ms(&self) -> Option<u64> {
        self.source_expiry
    }
    /// Wall-clock observation at the first terminal transition, captured before
    /// control close, helper cleanup or worker join. This is diagnostic only and
    /// never grants authority. `None` means no terminal transition yet or that
    /// its clock read failed; later cleanup/retries cannot substitute a time.
    pub fn terminal_observed_at_ms(&self) -> Option<u64> {
        self.terminal_observation.at_ms
    }
    /// Closed diagnostic vocabulary; never includes peer text or TLS details.
    pub fn terminal_cause(&self) -> Option<&'static str> {
        self.terminal_observation.cause.map(TerminalCause::name)
    }
    pub fn listener_address(&self) -> Option<SocketAddr> {
        self.listener_address
    }
    pub fn local_offer(&self) -> Option<SecureAudioDataOffer> {
        self.local_offer
    }
    pub fn helper_pid(&self) -> Option<u32> {
        self.sink.as_ref().map(PipeWirePlayback::pid)
    }
    pub fn metrics(&self) -> PlaybackMetrics {
        self.pump.metrics()
    }

    pub fn poll(&mut self) -> io::Result<Option<SecurePlaybackEvent>> {
        let result = self.poll_inner();
        if result.is_err() {
            self.terminal_observation.observe(TerminalCause::Failure);
            // Do not replace the original failure with a cleanup error. An
            // uncompleted bounded join is retained for an explicit stop retry.
            let _ = self.stop();
        }
        result
    }
    fn poll_inner(&mut self) -> io::Result<Option<SecurePlaybackEvent>> {
        self.check(now_ms()?)?;
        let deadline = self.operation_deadline();
        let message = match self.link.poll_control_message_until(deadline) {
            Ok(message) => message,
            Err(reason) => {
                self.terminal_observation.observe(control_cause(&reason));
                return Err(error(reason));
            }
        };
        if let Some(message) = message {
            if let ReceivedControlMessage::PortLeaseResponse(response) = message {
                let now = now_ms()?;
                self.check(now)?;
                if self.source_expiry.is_some() {
                    return Err(error("duplicate secure audio Source lease response"));
                }
                let expiry =
                    validate_source_response(&self.original, self.source_request, response, now)?;
                self.source_claim = Some(response);
                self.source_expiry = Some(expiry);
                // Convert the peer's absolute expiry exactly once. Subsequent
                // wall-clock movement cannot extend the original monotonic end.
                self.end = self
                    .end
                    .min(Instant::now() + Duration::from_millis(expiry - now));
                self.listen()?;
                return Ok(Some(SecurePlaybackEvent::Listening));
            }
            if self.handle_control(message, deadline)? {
                self.stop()?;
                return Ok(Some(SecurePlaybackEvent::Stopped));
            }
        }
        self.check(now_ms()?)?;
        // Observe a queued transport failure before continuing even a partial
        // helper write. EOF/failure must not be hidden behind PCM flushing.
        let event = match self.events.as_ref().map(|events| events.try_recv()) {
            None | Some(Err(TryRecvError::Empty)) => None,
            Some(Err(TryRecvError::Disconnected)) => {
                return Err(error("secure audio transport ended"));
            }
            Some(Ok(event)) => Some(event),
        };
        let result = match event {
            Some(DataEvent::Failed(reason)) => {
                self.terminal_observation.observe(media_cause(&reason));
                return Err(reason);
            }
            Some(DataEvent::Bound) => {
                self.check(now_ms()?)?;
                if self.sink.is_some() {
                    return Err(error("duplicate secure audio binding"));
                }
                // The worker emits Bound only after the exact pinned TLS ACK.
                self.sink = Some(PipeWirePlayback::open_mode(
                    &self.admission.pipewire_target,
                    self.endpoint_mode,
                )?);
                self.binding_deadline = None;
                self.check(now_ms()?)?;
                self.flush()?;
                Some(SecurePlaybackEvent::Bound)
            }
            Some(DataEvent::Record(bytes)) => {
                self.check(now_ms()?)?;
                if !self.read_pending || self.sink.is_none() || self.pump.has_pending_pcm() {
                    return Err(error("unexpected secure audio record or queue overflow"));
                }
                self.read_pending = false;
                self.rate.admit(self.started.elapsed()).map_err(error)?;
                let packet = self.admission_check.admit_record(&bytes).map_err(error)?;
                self.pump.accept(&packet, self.started.elapsed())?;
                if !self.pump.has_pending_pcm() {
                    return Err(error(
                        "secure audio PCM admission did not queue the exact frame",
                    ));
                }
                self.flush()?;
                Some(SecurePlaybackEvent::Record)
            }
            None => {
                self.flush()?;
                None
            }
        };
        self.request_read()?;
        Ok(result)
    }
    fn request_read(&mut self) -> io::Result<()> {
        if self.sink.is_none() || self.read_pending || self.pump.has_pending_pcm() {
            return Ok(());
        }
        self.check(now_ms()?)?;
        let deadline = self.operation_deadline();
        match self
            .reads
            .as_ref()
            .ok_or_else(|| error("secure audio reader missing"))?
            .try_send(deadline)
        {
            Ok(()) => {
                self.read_pending = true;
                Ok(())
            }
            Err(TrySendError::Full(_)) => Err(error("secure audio read permit overflow")),
            Err(TrySendError::Disconnected(_)) => Err(error("secure audio reader ended")),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.check(now_ms()?)?;
        let Some(sink) = self.sink.as_mut() else {
            return Ok(());
        };
        let mut supervised = GuardedSink {
            sink,
            runtime: &self.runtime,
            original: &self.original,
            expected: self.admission.expected,
            sink_claim: &self.sink_claim,
            source_claim: self
                .source_claim
                .ok_or_else(|| error("secure audio Source claim absent"))?,
            end: self.end,
            last_wall: &mut self.last_wall,
        };
        self.pump.flush(&mut supervised, self.started.elapsed())
    }
    fn check(&mut self, now: u64) -> io::Result<()> {
        if self.closed
            || self.link.is_closed()
            || now < self.last_wall
            || Instant::now() >= self.end
            || self.source_expiry.is_some_and(|end| now >= end)
            || (self.source_expiry.is_none() && Instant::now() >= self.source_request_deadline)
            || self
                .binding_deadline
                .is_some_and(|end| Instant::now() >= end)
        {
            return Err(error(
                "secure audio authority or operation deadline elapsed",
            ));
        }
        if let Some(claim) = self.source_claim {
            if Some(validate_source_response(
                &self.original,
                self.source_request,
                claim,
                now,
            )?) != self.source_expiry
            {
                return Err(error("secure audio original Source claim changed"));
            }
        } else if self.source_expiry.is_some() {
            return Err(error("secure audio original Source claim absent"));
        }
        validate_authority(
            &self.runtime,
            &self.original,
            self.admission.expected,
            &self.sink_claim,
            now,
            self.source_expiry.is_some(),
        )?;
        let fresh = now_ms()?;
        if fresh < now
            || fresh >= self.sink_claim.expires_at_ms
            || fresh >= expiry(&self.original)?
            || self.source_expiry.is_some_and(|end| fresh >= end)
            || Instant::now() >= self.end
        {
            return Err(error("secure audio authority elapsed during preflight"));
        }
        self.last_wall = fresh;
        Ok(())
    }
    fn operation_deadline(&self) -> Instant {
        let mut deadline = self.end.min(Instant::now() + MAX_STALL);
        if self.source_expiry.is_none() {
            deadline = deadline.min(self.source_request_deadline);
        }
        if let Some(binding) = self.binding_deadline {
            deadline = deadline.min(binding);
        }
        deadline
    }
    fn listen(&mut self) -> io::Result<()> {
        let now = now_ms()?;
        self.check(now)?;
        let binding = AudioTlsBinding::from_route(
            self.admission.expected,
            &self.original,
            self.admission.stream_id,
            now,
        )
        .map_err(error)?;
        let listener = AudioTlsListener::from_listener_until(
            TcpListener::bind((self.admission.local_address, 0))?,
            self.identity
                .take()
                .ok_or_else(|| error("secure audio identity already consumed"))?,
            binding,
            MAX_STALL,
            self.end,
        )
        .map_err(error)?;
        let address = listener.local_addr().map_err(error)?;
        let offer = SecureAudioDataOffer {
            port: address.port(),
            stream_id: self.admission.stream_id,
            transport: SecureAudioTransport::Tls13MtlsV1,
        };
        self.transport_stop = Some(listener.stop_handle().map_err(error)?);
        let accept_end = self.operation_deadline();
        self.link
            .send_secure_audio_data_offer_until(&self.original, offer, now, accept_end)
            .map_err(error)?;
        self.check(now_ms()?)?;
        self.binding_deadline = Some(accept_end);
        self.local_offer = Some(offer);
        self.listener_address = Some(address);
        let (tx, rx) = mpsc::sync_channel(1);
        let (read_tx, read_rx) = mpsc::sync_channel::<Instant>(1);
        self.events = Some(rx);
        self.reads = Some(read_tx);
        self.worker = Some(
            thread::Builder::new()
                .name("capyio-secure-audio".into())
                .spawn(move || {
                    let outcome = (|| -> io::Result<()> {
                        let mut channel = loop {
                            if Instant::now() >= accept_end {
                                return Err(error("secure audio accept deadline"));
                            }
                            if let Some(channel) =
                                listener.try_accept_until(accept_end).map_err(error)?
                            {
                                break channel;
                            }
                            thread::sleep(Duration::from_millis(2));
                        };
                        if Instant::now() >= accept_end {
                            return Err(error("secure audio binding deadline"));
                        }
                        if tx.send(DataEvent::Bound).is_err() {
                            return Ok(());
                        }
                        while let Ok(deadline) = read_rx.recv() {
                            if Instant::now() >= deadline {
                                return Err(error("secure audio read permit expired"));
                            }
                            let bytes = channel
                                .receive_record_until(deadline)
                                .map_err(media_error)?;
                            if tx.send(DataEvent::Record(bytes)).is_err() {
                                return Ok(());
                            }
                        }
                        Ok(())
                    })();
                    if let Err(reason) = outcome {
                        let _ = tx.send(DataEvent::Failed(reason));
                    }
                })?,
        );
        Ok(())
    }
    fn handle_control(
        &mut self,
        message: ReceivedControlMessage,
        deadline: Instant,
    ) -> io::Result<bool> {
        match message {
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
            ReceivedControlMessage::RouteCommand(command)
                if command.session_id == self.original.session_id
                    && command.route_id == self.original.id
                    && command.action == RouteCommandAction::Stop =>
            {
                self.terminal_observation.observe(TerminalCause::PeerStop);
                return Ok(true);
            }
            ReceivedControlMessage::RouteStatus(status) => {
                validate_status(&self.original, &status)?
            }
            _ => return Err(error("unexpected dedicated secure audio control message")),
        }
        self.check(now_ms()?)?;
        Ok(false)
    }
    /// Terminal, idempotent close. Disconnect network I/O and drop both bounded
    /// channels before bounded worker join; always retain legacy helper cleanup.
    /// A caller failure is not mislabeled as a user Stop. Earlier causes win.
    pub fn stop_after_failure(&mut self) -> io::Result<()> {
        self.terminal_observation.observe(TerminalCause::Failure);
        self.stop()
    }
    pub fn stop(&mut self) -> io::Result<()> {
        // Record before *any* cleanup. poll errors, authenticated peer Stop and
        // local Stop all converge here; cleanup latency is not observation time.
        self.terminal_observation.observe(TerminalCause::LocalStop);
        self.closed = true;
        if let Some(stop) = self.transport_stop.take() {
            stop.stop();
        }
        self.reads.take();
        self.events.take();
        let control = self
            .link
            .close_and_reconcile(&mut self.runtime)
            .map_err(error);
        let media = self
            .sink
            .as_mut()
            .map(PipeWirePlayback::stop)
            .unwrap_or(Ok(()));
        let network = if let Some(worker) = self.worker.as_ref() {
            let deadline = Instant::now() + MAX_STALL;
            while !worker.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(2));
            }
            if worker.is_finished() {
                self.worker
                    .take()
                    .expect("owned worker")
                    .join()
                    .map_err(|_| error("secure audio worker panicked"))
            } else {
                Err(error(
                    "secure audio worker join deadline; cancellation remains requested",
                ))
            }
        } else {
            Ok(())
        };
        control.and(media).and(network)
    }
}
impl Drop for SecurePlaybackOwner {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Fresh authority is checked immediately before every nonblocking write,
/// including each continuation of a partially written 960-byte PCM frame.
struct GuardedSink<'a, S> {
    sink: &'a mut S,
    runtime: &'a NodeRuntime,
    original: &'a Route,
    expected: ControlTlsBinding,
    sink_claim: &'a PortLeaseClaim,
    source_claim: capyio_protocol::ValidatedPortLeaseResponse,
    end: Instant,
    last_wall: &'a mut u64,
}
impl<S: PcmPlaybackSink> GuardedSink<'_, S> {
    fn check(&mut self) -> io::Result<()> {
        let now = now_ms()?;
        let source_expiry = validate_source_response(
            self.original,
            self.source_claim.request_message_id,
            self.source_claim,
            now,
        )?;
        if now < *self.last_wall || now >= source_expiry || Instant::now() >= self.end {
            return Err(error("secure audio authority elapsed before PCM write"));
        }
        validate_authority(
            self.runtime,
            self.original,
            self.expected,
            self.sink_claim,
            now,
            true,
        )?;
        // Catalog validation can itself consume time. The borrowed Runtime
        // cannot mutate during this check; refresh time after it, immediately
        // before the nonblocking helper operation rather than using stale time.
        let fresh = now_ms()?;
        if fresh < now
            || fresh >= source_expiry
            || fresh >= self.sink_claim.expires_at_ms
            || fresh >= expiry(self.original)?
            || Instant::now() >= self.end
        {
            return Err(error("secure audio authority elapsed during PCM preflight"));
        }
        *self.last_wall = fresh;
        Ok(())
    }
}
impl<S: PcmPlaybackSink> PcmPlaybackSink for GuardedSink<'_, S> {
    fn check_alive(&mut self) -> io::Result<()> {
        self.check()?;
        self.sink.check_alive()
    }
    fn try_write_pcm(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.check()?;
        self.sink.try_write_pcm(bytes)
    }
}
fn expiry(route: &Route) -> io::Result<u64> {
    match route.authorization {
        AuthorizationState::Authorized {
            expires_at_ms: Some(expiry),
        } => Ok(expiry),
        _ => Err(error("finite local secure audio consent required")),
    }
}
fn validate_profile_route(route: &Route, profile: SecureAudioSessionProfile) -> io::Result<()> {
    if route.selected_format.as_ref() != Some(&profile.descriptor()) {
        return Err(error(
            "secure audio Route does not match explicit session profile",
        ));
    }
    Ok(())
}
fn validate_target(admission: &SecurePlaybackAdmission) -> io::Result<()> {
    validate_target_profile(admission, SecureAudioSessionProfile::Finite60sV1)
}
fn validate_target_profile(
    admission: &SecurePlaybackAdmission,
    profile: SecureAudioSessionProfile,
) -> io::Result<()> {
    crate::validate_endpoint_target(EndpointMode::Playback, &admission.pipewire_target)
        .map_err(error)?;
    if !(Duration::from_secs(1)..=profile.max_duration()).contains(&admission.lifetime)
        || admission.lifetime.subsec_nanos() != 0
        || admission.local_address.is_unspecified()
        || admission.local_address.is_multicast()
        || admission.stream_id.as_uuid().is_nil()
        || admission.pipewire_target.eq_ignore_ascii_case("default")
        || admission.pipewire_target.eq_ignore_ascii_case("auto")
    {
        return Err(error("invalid explicit finite secure playback target"));
    }
    Ok(())
}
/// Pure validation before any lease request, listener or helper can be created.
#[cfg(test)]
fn validate_mode_target(
    admission: &SecurePlaybackAdmission,
    mode: EndpointMode,
    original: &Route,
) -> io::Result<()> {
    validate_mode_target_profile(
        admission,
        mode,
        original,
        SecureAudioSessionProfile::Finite60sV1,
    )
}
fn validate_mode_target_profile(
    admission: &SecurePlaybackAdmission,
    mode: EndpointMode,
    original: &Route,
    profile: SecureAudioSessionProfile,
) -> io::Result<()> {
    validate_target_profile(admission, profile)?;
    match mode {
        EndpointMode::Playback => Ok(()),
        EndpointMode::VirtualMicrophone => {
            crate::validate_endpoint_target(mode, &admission.pipewire_target).map_err(error)?;
            if admission.pipewire_target != crate::virtual_microphone_name(original.sink.port_id) {
                return Err(error(
                    "secure virtual microphone requires the original local Sink identity",
                ));
            }
            Ok(())
        }
        _ => Err(error("unsupported secure audio endpoint mode")),
    }
}
fn media_binding(route: &Route, stream_id: StreamId) -> io::Result<AudioMediaStreamBinding> {
    Ok(AudioMediaStreamBinding {
        session_id: route.session_id,
        route_id: route.id,
        stream_id,
        stream_epoch: u32::try_from(route.epoch).map_err(error)?,
        selected_spec: AudioStreamSpec::voice_interactive(),
    })
}
fn admission_end(admission: &SecurePlaybackAdmission) -> io::Result<Instant> {
    validate_target(admission)?;
    Instant::now()
        .checked_add(admission.lifetime)
        .ok_or_else(|| error("invalid secure playback lifetime"))
}

fn consent_end(
    started: Instant,
    lifetime: Duration,
    original_end: Instant,
    authority_ms: u64,
) -> io::Result<Instant> {
    let end = original_end.min(
        started
            .checked_add(lifetime.min(Duration::from_millis(authority_ms)))
            .ok_or_else(|| error("invalid secure playback deadline"))?,
    );
    if end <= started {
        return Err(error("original secure playback consent elapsed"));
    }
    Ok(end)
}

fn source_lease_ttl(deadline: u64, now: u64, lifetime: Duration) -> io::Result<u64> {
    deadline
        .checked_sub(now)
        .map(|remaining| remaining.min(lifetime.as_millis() as u64))
        .and_then(|remaining| remaining.checked_sub(MAX_STALL.as_millis() as u64))
        .filter(|ttl| *ttl >= capyio_protocol::MIN_PORT_LEASE_TTL_MS)
        .ok_or_else(|| error("insufficient finite audio Source lease negotiation headroom"))
}
fn validate_authority(
    runtime: &NodeRuntime,
    original: &Route,
    binding: ControlTlsBinding,
    original_claim: &PortLeaseClaim,
    now: u64,
    source_leased: bool,
) -> io::Result<()> {
    let route = runtime.route(original.id).map_err(error)?;
    let session = runtime.session(binding.session_id).map_err(error)?;
    if route != original
        || route.session_id != binding.session_id
        || route.state != RouteState::Starting
        || session.state != SessionState::Ready
        || session.local_node_id != binding.server_node_id
        || session.remote_node_id != binding.client_node_id
        || route.sink.node_id != session.local_node_id
        || route.source.node_id != session.remote_node_id
        || expiry(route)? <= now
        || u32::try_from(route.epoch).is_err()
    {
        return Err(error("secure audio original Runtime binding changed"));
    }
    SecureAudioDataOffer {
        port: 1,
        stream_id: StreamId::new(),
        transport: SecureAudioTransport::Tls13MtlsV1,
    }
    .validate(route)
    .map_err(error)?;
    let claim = runtime
        .route_port_lease_claim(route.id, route.sink)
        .ok_or_else(|| error("secure audio Sink lease absent"))?;
    if claim != original_claim
        || claim.port != route.sink
        || claim.route_id != route.id
        || claim.session_id != route.session_id
        || claim.expires_at_ms <= now
        || claim.expires_at_ms > expiry(route)?
    {
        return Err(error("secure audio original Sink lease changed or expired"));
    }
    let snapshot = runtime.snapshot();
    if snapshot.local_node.platform != Platform::Linux
        || snapshot.local_node.id != binding.server_node_id
        || snapshot
            .routes
            .iter()
            .filter(|route| route.session_id == binding.session_id)
            .count()
            != 1
    {
        return Err(error(
            "secure audio requires a dedicated single-Route Linux Session",
        ));
    }
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
                .ok_or_else(|| error("secure audio peer catalog disappeared"))?
        };
        let capability = node
            .capabilities
            .get(&reference.capability_id)
            .ok_or_else(|| error("secure audio Capability disappeared"))?;
        let port = node
            .port(reference.capability_id, reference.port_id)
            .map_err(error)?;
        let available = |value| {
            value == Availability::Available
                || (source_leased && reference == route.source && value == Availability::Busy)
        };
        if node.online_state != capyio_core::OnlineState::Online
            || !available(capability.availability)
            || !available(port.availability)
            || port.direction != direction
            || port.profile != route.profile
            || (route
                .selected_format
                .as_ref()
                .and_then(SecureAudioSessionProfile::from_descriptor)
                == Some(SecureAudioSessionProfile::Finite300sV1)
                && (port.formats != [SecureAudioSessionProfile::Finite300sV1.descriptor()]
                    || port.qos_modes
                        != std::collections::BTreeSet::from([capyio_core::QosMode::Interactive])))
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
                "secure audio catalog no longer supports the exact endpoint",
            ));
        }
    }
    Ok(())
}
fn validate_source_response(
    original: &Route,
    request: MessageId,
    response: capyio_protocol::ValidatedPortLeaseResponse,
    now: u64,
) -> io::Result<u64> {
    if response.request_message_id != request
        || response.session_id != original.session_id
        || response.route_id != original.id
        || response.port != original.source
        || response.outcome != PortLeaseOutcome::Acquired
    {
        return Err(error("unexpected secure audio Source lease response"));
    }
    response
        .expires_at_ms
        .filter(|value| *value > now && *value <= expiry(original).unwrap_or(0))
        .ok_or_else(|| error("invalid secure audio Source lease expiry"))
}
fn validate_status(
    original: &Route,
    status: &capyio_protocol::ValidatedRouteStatus,
) -> io::Result<()> {
    if status.session_id != original.session_id
        || status.route != *original
        || status.secure_audio_data.is_some()
        || status.native_audio_data.is_some()
        || status.secure_camera_data.is_some()
        || status.camera_data.is_some()
        || status.touchpad_data.is_some()
        || status.haptics_data.is_some()
    {
        return Err(error(
            "secure audio peer status changed the exact Route or data offer",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_core::{
        FormatDescriptor, InteroperabilityMode, NodeId, QosMode, RouteBackend, SessionId,
    };
    use capyio_runtime::PortLeasePolicy;

    fn test_source_claim(
        route: &Route,
        expiry: u64,
    ) -> capyio_protocol::ValidatedPortLeaseResponse {
        capyio_protocol::ValidatedPortLeaseResponse {
            message_id: MessageId::new(),
            request_message_id: MessageId::new(),
            session_id: route.session_id,
            route_id: route.id,
            port: route.source,
            outcome: PortLeaseOutcome::Acquired,
            expires_at_ms: Some(expiry),
        }
    }

    fn fixture_at(now: u64) -> (NodeRuntime, Route, ControlTlsBinding, PortLeaseClaim) {
        fixture_profile(now, SecureAudioSessionProfile::Finite60sV1)
    }
    fn fixture_profile(
        now: u64,
        profile: SecureAudioSessionProfile,
    ) -> (NodeRuntime, Route, ControlTlsBinding, PortLeaseClaim) {
        let lab = capyio_testkit::DemoLab::new().unwrap();
        let original = lab
            .runtime
            .route(lab.routes.phone_microphone_to_windows)
            .unwrap()
            .clone();
        let snapshot = lab.runtime.snapshot();
        let mut local = snapshot.local_node;
        local.platform = Platform::Linux;
        let mut peer = snapshot.peers[0].clone();
        let format = profile.descriptor();
        for node in [&mut local, &mut peer] {
            for adapter in node.adapter_instances.values_mut() {
                adapter
                    .supported_route_modes
                    .insert(RouteBackend::AdapterManaged);
            }
            for cap in node.capabilities.values_mut() {
                cap.availability = Availability::Available;
                for port in cap.ports.values_mut() {
                    port.availability = Availability::Available;
                    port.interoperability_mode = InteroperabilityMode::AdapterManaged;
                    if port.profile == original.profile {
                        port.formats = vec![format.clone()];
                        port.qos_modes = std::collections::BTreeSet::from([QosMode::Interactive]);
                    }
                }
            }
        }
        let peer_id = peer.id;
        let mut runtime = NodeRuntime::new(local).unwrap();
        runtime.register_peer(peer, true).unwrap();
        let session = runtime.open_session(peer_id).unwrap();
        let route_id = runtime
            .create_route(
                session,
                original.source,
                original.sink,
                RouteBackend::AdapterManaged,
            )
            .unwrap();
        runtime
            .set_local_port_lease_policy(original.sink, PortLeasePolicy::Exclusive)
            .unwrap();
        runtime
            .authorize_route(route_id, Some(now + 10_000))
            .unwrap();
        runtime
            .acquire_route_port_lease(route_id, original.sink, now + 5_000, now)
            .unwrap();
        runtime
            .prepare_route(route_id, Some(format), QosMode::Interactive, now)
            .unwrap();
        runtime.begin_route_start(route_id, now).unwrap();
        let route = runtime.route(route_id).unwrap().clone();
        let binding =
            ControlTlsBinding::new(session, route.sink.node_id, route.source.node_id).unwrap();
        let claim = *runtime
            .route_port_lease_claim(route_id, route.sink)
            .unwrap();
        (runtime, route, binding, claim)
    }
    fn fixture() -> (NodeRuntime, Route, ControlTlsBinding, PortLeaseClaim) {
        fixture_at(1000)
    }

    #[test]
    fn secure_audio_exact_original_route_lease_format_and_qos_required() {
        let (runtime, route, binding, claim) = fixture();
        validate_authority(&runtime, &route, binding, &claim, 1000, false).unwrap();
        assert!(validate_authority(&runtime, &route, binding, &claim, 6000, false).is_err());
        for field in 0..8 {
            let mut changed = route.clone();
            match field {
                0 => changed.epoch += 1,
                1 => changed.source.port_id = capyio_core::PortId::new(),
                2 => changed.sink.capability_id = capyio_core::CapabilityId::new(),
                3 => changed.selected_format = Some(FormatDescriptor::new("pcm-s16le-48000-mono")),
                4 => {
                    changed
                        .selected_format
                        .as_mut()
                        .unwrap()
                        .parameters
                        .insert("extra".into(), "1".into());
                }
                5 => changed.selected_qos_mode = Some(QosMode::Basic),
                6 => {
                    changed.authorization = AuthorizationState::Authorized {
                        expires_at_ms: Some(12_000),
                    }
                }
                _ => changed.state = RouteState::Active,
            }
            assert!(
                validate_authority(&runtime, &changed, binding, &claim, 1000, false).is_err(),
                "field {field}"
            );
        }
        for field in 0..3 {
            let mut changed = binding;
            match field {
                0 => changed.session_id = SessionId::new(),
                1 => changed.server_node_id = NodeId::new(),
                _ => changed.client_node_id = NodeId::new(),
            }
            assert!(validate_authority(&runtime, &route, changed, &claim, 1000, false).is_err());
        }
    }
    #[test]
    fn secure_audio_authority_loss_and_renewal_never_rebind_original() {
        for action in 0..6 {
            let (mut runtime, route, binding, claim) = fixture();
            match action {
                0 => {
                    runtime
                        .release_route_port_lease(route.id, route.sink, route.session_id)
                        .unwrap();
                }
                1 => {
                    runtime
                        .acquire_route_port_lease(route.id, route.sink, 7000, 1001)
                        .unwrap();
                }
                2 => {
                    runtime
                        .renew_route_authorization(route.id, 12000, 1001)
                        .unwrap();
                }
                3 => {
                    runtime.begin_route_stop(route.id).unwrap();
                    runtime.stop_route(route.id).unwrap();
                }
                4 => {
                    runtime.close_session(route.session_id).unwrap();
                }
                _ => {
                    runtime
                        .set_peer_online(route.source.node_id, false)
                        .unwrap();
                }
            }
            assert!(
                validate_authority(&runtime, &route, binding, &claim, 1001, true).is_err(),
                "action {action}"
            );
        }
    }
    #[test]
    fn secure_audio_catalog_cannot_downgrade_negotiated_secure_profile() {
        let (mut runtime, route, binding, claim) = fixture();
        let mut peer = runtime
            .snapshot()
            .peers
            .into_iter()
            .find(|node| node.id == route.source.node_id)
            .unwrap();
        peer.capabilities
            .get_mut(&route.source.capability_id)
            .unwrap()
            .ports
            .get_mut(&route.source.port_id)
            .unwrap()
            .formats = vec![FormatDescriptor::new("pcm-s16le-48000-mono")];
        runtime.accept_peer_catalog(route.session_id, peer).unwrap();
        assert!(validate_authority(&runtime, &route, binding, &claim, 1001, true).is_err());
    }
    #[test]
    fn secure_audio_original_claim_is_exact_even_when_replacement_is_live() {
        let (runtime, route, binding, claim) = fixture();
        for field in 0..4 {
            let mut changed = claim;
            match field {
                0 => changed.expires_at_ms -= 1,
                1 => changed.route_id = RouteId::new(),
                2 => changed.session_id = SessionId::new(),
                _ => changed.port.port_id = capyio_core::PortId::new(),
            }
            assert!(validate_authority(&runtime, &route, binding, &changed, 1000, false).is_err());
        }
    }
    #[test]
    fn secure_audio_status_cannot_replace_route_or_offer() {
        let (_, route, _, _) = fixture();
        let encoded = capyio_protocol::encode_route_status(route.session_id, &route).unwrap();
        let mut status = capyio_protocol::decode_route_status(&encoded).unwrap();
        validate_status(&route, &status).unwrap();
        status.route.epoch += 1;
        assert!(validate_status(&route, &status).is_err());
        status.route = route.clone();
        status.secure_audio_data = Some(SecureAudioDataOffer {
            port: 12345,
            stream_id: StreamId::new(),
            transport: SecureAudioTransport::Tls13MtlsV1,
        });
        assert!(validate_status(&route, &status).is_err());
    }
    #[test]
    fn secure_audio_old_profile_retains_catalog_containment_but_not_route_mutation() {
        let (mut runtime, route, binding, claim) = fixture();
        let mut peer = runtime.snapshot().peers[0].clone();
        let port = peer
            .capabilities
            .get_mut(&route.source.capability_id)
            .unwrap()
            .ports
            .get_mut(&route.source.port_id)
            .unwrap();
        port.formats
            .push(SecureAudioSessionProfile::Finite300sV1.descriptor());
        port.qos_modes.insert(QosMode::Basic);
        runtime.accept_peer_catalog(route.session_id, peer).unwrap();
        validate_authority(&runtime, &route, binding, &claim, 1001, false).unwrap();
        let mut changed = route.clone();
        changed.selected_format = Some(SecureAudioSessionProfile::Finite300sV1.descriptor());
        assert!(validate_authority(&runtime, &changed, binding, &claim, 1001, false).is_err());
        assert!(validate_profile_route(&changed, SecureAudioSessionProfile::Finite60sV1).is_err());
    }

    #[test]
    fn secure_audio_long_profile_requires_singleton_catalog() {
        let (mut runtime, route, binding, claim) =
            fixture_profile(1000, SecureAudioSessionProfile::Finite300sV1);
        let mut peer = runtime.snapshot().peers[0].clone();
        let port = peer
            .capabilities
            .get_mut(&route.source.capability_id)
            .unwrap()
            .ports
            .get_mut(&route.source.port_id)
            .unwrap();
        port.formats
            .push(SecureAudioSessionProfile::Finite60sV1.descriptor());
        port.qos_modes.insert(QosMode::Basic);
        runtime.accept_peer_catalog(route.session_id, peer).unwrap();
        assert!(validate_authority(&runtime, &route, binding, &claim, 1001, false).is_err());
    }

    #[test]
    fn secure_audio_long_catalog_and_route_are_immutable() {
        let (mut runtime, route, binding, claim) =
            fixture_profile(1000, SecureAudioSessionProfile::Finite300sV1);
        validate_profile_route(&route, SecureAudioSessionProfile::Finite300sV1).unwrap();
        validate_authority(&runtime, &route, binding, &claim, 1001, false).unwrap();
        let mut peer = runtime.snapshot().peers[0].clone();
        peer.capabilities
            .get_mut(&route.source.capability_id)
            .unwrap()
            .ports
            .get_mut(&route.source.port_id)
            .unwrap()
            .formats = vec![SecureAudioSessionProfile::Finite60sV1.descriptor()];
        runtime.accept_peer_catalog(route.session_id, peer).unwrap();
        assert!(validate_authority(&runtime, &route, binding, &claim, 1001, false).is_err());
    }

    #[test]
    fn secure_audio_explicit_profile_boundaries_never_infer_from_duration() {
        let (_, mut route, expected, _) = fixture();
        let mut admission = SecurePlaybackAdmission {
            expected,
            route_id: route.id,
            stream_id: StreamId::new(),
            local_address: "127.0.0.1".parse().unwrap(),
            lifetime: Duration::from_secs(60),
            pipewire_target: "explicit.test".into(),
        };
        validate_target(&admission).unwrap();
        for seconds in [61, 300] {
            admission.lifetime = Duration::from_secs(seconds);
            assert!(validate_target(&admission).is_err());
            validate_target_profile(&admission, SecureAudioSessionProfile::Finite300sV1).unwrap();
        }
        for lifetime in [
            Duration::ZERO,
            Duration::from_secs(301),
            Duration::from_millis(1500),
            Duration::MAX,
        ] {
            admission.lifetime = lifetime;
            assert!(
                validate_target_profile(&admission, SecureAudioSessionProfile::Finite300sV1)
                    .is_err()
            );
        }
        assert!(validate_profile_route(&route, SecureAudioSessionProfile::Finite300sV1).is_err());
        route.selected_format = Some(SecureAudioSessionProfile::Finite300sV1.descriptor());
        validate_profile_route(&route, SecureAudioSessionProfile::Finite300sV1).unwrap();
        assert!(validate_profile_route(&route, SecureAudioSessionProfile::Finite60sV1).is_err());
        route
            .selected_format
            .as_mut()
            .unwrap()
            .parameters
            .insert("duration".into(), "300".into());
        assert!(validate_profile_route(&route, SecureAudioSessionProfile::Finite300sV1).is_err());
    }

    #[test]
    fn secure_audio_target_is_explicit_finite_and_never_default() {
        let (_, route, expected, _) = fixture();
        let admission = SecurePlaybackAdmission {
            expected,
            route_id: route.id,
            stream_id: StreamId::new(),
            local_address: "127.0.0.1".parse().unwrap(),
            lifetime: Duration::from_secs(10),
            pipewire_target: "capyio.secure.test".into(),
        };
        validate_target(&admission).unwrap();
        for target in [
            "",
            "default",
            "DEFAULT",
            "auto",
            "AUTO",
            "0",
            "-1",
            "-target",
            "has space",
        ] {
            let mut changed = admission.clone();
            changed.pipewire_target = target.into();
            assert!(validate_target(&changed).is_err());
        }
        for lifetime in [
            Duration::ZERO,
            Duration::from_millis(1500),
            Duration::from_secs(61),
            Duration::from_secs(300),
            Duration::MAX,
        ] {
            let mut changed = admission.clone();
            changed.lifetime = lifetime;
            assert!(validate_target(&changed).is_err());
            assert!(admission_end(&changed).is_err());
        }
        for address in ["0.0.0.0", "::", "224.0.0.1", "ff02::1"] {
            let mut changed = admission.clone();
            changed.local_address = address.parse().unwrap();
            assert!(validate_target(&changed).is_err());
        }
    }
    #[test]
    fn secure_virtual_microphone_requires_explicit_mode_and_original_sink_identity() {
        let (_, route, expected, _) = fixture();
        let admission = SecurePlaybackAdmission {
            expected,
            route_id: route.id,
            stream_id: StreamId::new(),
            local_address: "127.0.0.1".parse().unwrap(),
            lifetime: Duration::from_secs(10),
            pipewire_target: crate::virtual_microphone_name(route.sink.port_id),
        };
        validate_mode_target(&admission, EndpointMode::VirtualMicrophone, &route).unwrap();
        admission_end(&admission).unwrap();
        for target in [
            "physical.selected.target".into(),
            crate::virtual_microphone_name(route.source.port_id),
            crate::virtual_microphone_name(capyio_core::PortId::new()),
            "capyio.virtual-microphone.00000000-0000-0000-0000-000000000000".into(),
            "capyio.virtual-microphone.malformed".into(),
            crate::virtual_speaker_name(route.sink.port_id),
            admission.pipewire_target.to_uppercase(),
        ] {
            let mut changed = admission.clone();
            changed.pipewire_target = target;
            assert!(
                validate_mode_target(&changed, EndpointMode::VirtualMicrophone, &route).is_err()
            );
        }
        for mode in [EndpointMode::StereoCapture, EndpointMode::VirtualSpeaker] {
            assert!(validate_mode_target(&admission, mode, &route).is_err());
        }
        let mut physical = admission.clone();
        physical.pipewire_target = "physical.selected.target".into();
        validate_mode_target(&physical, EndpointMode::Playback, &route).unwrap();
        for lifetime in [
            Duration::MAX,
            Duration::from_secs(61),
            Duration::ZERO,
            Duration::from_millis(1500),
        ] {
            let mut changed = admission.clone();
            changed.lifetime = lifetime;
            assert!(
                validate_mode_target(&changed, EndpointMode::VirtualMicrophone, &route).is_err()
            );
            assert!(admission_end(&changed).is_err());
        }
    }
    #[test]
    fn secure_audio_source_response_requires_exact_acquired_tuple_and_finite_expiry() {
        let (_, route, _, _) = fixture();
        let request = MessageId::new();
        let response = capyio_protocol::ValidatedPortLeaseResponse {
            message_id: MessageId::new(),
            session_id: route.session_id,
            request_message_id: request,
            route_id: route.id,
            port: route.source,
            outcome: PortLeaseOutcome::Acquired,
            expires_at_ms: Some(5000),
        };
        assert_eq!(
            validate_source_response(&route, request, response, 1000).unwrap(),
            5000
        );
        for field in 0..9 {
            let mut changed = response;
            match field {
                0 => changed.request_message_id = MessageId::new(),
                1 => changed.session_id = SessionId::new(),
                2 => changed.route_id = RouteId::new(),
                3 => changed.port = route.sink,
                4 => changed.outcome = PortLeaseOutcome::Denied,
                5 => changed.expires_at_ms = None,
                6 => changed.expires_at_ms = Some(1000),
                7 => changed.expires_at_ms = Some(12000),
                _ => changed.port.port_id = capyio_core::PortId::new(),
            }
            assert!(
                validate_source_response(&route, request, changed, 1000).is_err(),
                "field {field}"
            );
        }
    }
    #[test]
    fn secure_audio_terminal_observation_is_not_replaced_after_cleanup() {
        let mut observation = TerminalObservation::default();
        assert_eq!(observation.at_ms, None);
        observation.record(Some(4_999));
        // A simulated reap or repeated Stop crosses the exact lease expiry.
        observation.record(Some(5_500));
        assert_eq!(observation.at_ms, Some(4_999));
        assert!(observation.at_ms.unwrap() < 5_000);
    }

    #[test]
    fn secure_audio_failed_terminal_clock_cannot_gain_postcleanup_timestamp() {
        let mut observation = TerminalObservation::default();
        observation.record(None);
        observation.record(Some(5_500));
        assert_eq!(observation.at_ms, None);
        assert!(observation.recorded);
    }

    #[test]
    fn secure_audio_source_request_reserves_fixed_negotiation_budget() {
        assert_eq!(
            source_lease_ttl(5000, 1000, Duration::from_secs(60)).unwrap(),
            3750
        );
        assert_eq!(
            source_lease_ttl(10000, 1000, Duration::from_secs(4)).unwrap(),
            3750
        );
        assert_eq!(
            source_lease_ttl(2250, 1000, Duration::from_secs(60)).unwrap(),
            1000
        );
        for deadline in [999, 1000, 2249] {
            assert!(source_lease_ttl(deadline, 1000, Duration::from_secs(60)).is_err());
        }
    }
    struct FakeSink {
        writes: usize,
        bytes: Vec<u8>,
        limit: usize,
    }
    impl PcmPlaybackSink for FakeSink {
        fn check_alive(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn try_write_pcm(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes += 1;
            let len = bytes.len().min(self.limit);
            self.bytes.extend_from_slice(&bytes[..len]);
            Ok(len)
        }
    }
    #[test]
    fn secure_audio_each_partial_pcm_write_rechecks_original_lease() {
        let now = now_ms().unwrap();
        let (mut runtime, route, expected, claim) = fixture_at(now);
        let media = media_binding(&route, StreamId::new()).unwrap();
        let mut pump = PlaybackPump::with_binding(Duration::from_secs(10), media.clone()).unwrap();
        let packet = capyio_audio::AudioMediaPacket {
            stream_id: media.stream_id,
            stream_epoch: media.stream_epoch,
            sequence: 0,
            source_timestamp_micros: 1,
            first_sample_index: 0,
            sample_count: 480,
            discontinuity: false,
            payload: vec![1; 960],
        };
        pump.accept(&packet, Duration::ZERO).unwrap();
        let mut sink = FakeSink {
            writes: 0,
            bytes: vec![],
            limit: 2,
        };
        let mut last_wall = now;
        let end = Instant::now() + Duration::from_secs(5);
        {
            let mut guarded = GuardedSink {
                sink: &mut sink,
                runtime: &runtime,
                original: &route,
                expected,
                sink_claim: &claim,
                source_claim: test_source_claim(&route, now + 5000),
                end,
                last_wall: &mut last_wall,
            };
            pump.flush(&mut guarded, Duration::ZERO).unwrap();
        }
        assert_eq!(sink.bytes.len(), 2);
        assert!(pump.has_pending_pcm());
        runtime
            .release_route_port_lease(route.id, route.sink, route.session_id)
            .unwrap();
        let mut guarded = GuardedSink {
            sink: &mut sink,
            runtime: &runtime,
            original: &route,
            expected,
            sink_claim: &claim,
            source_claim: test_source_claim(&route, now + 5000),
            end,
            last_wall: &mut last_wall,
        };
        assert!(pump.flush(&mut guarded, Duration::from_millis(1)).is_err());
        assert_eq!(sink.writes, 1);
        assert_eq!(sink.bytes.len(), 2);
    }
    #[test]
    fn secure_audio_expired_monotonic_authority_blocks_helper_write() {
        let now = now_ms().unwrap();
        let (runtime, route, expected, claim) = fixture_at(now);
        let mut sink = FakeSink {
            writes: 0,
            bytes: vec![],
            limit: 960,
        };
        let mut last_wall = now;
        let mut guarded = GuardedSink {
            sink: &mut sink,
            runtime: &runtime,
            original: &route,
            expected,
            sink_claim: &claim,
            source_claim: test_source_claim(&route, now + 5000),
            end: Instant::now(),
            last_wall: &mut last_wall,
        };
        assert!(guarded.try_write_pcm(&[1; 960]).is_err());
        assert_eq!(sink.writes, 0);
    }
    #[test]
    fn secure_audio_one_frame_queue_retains_absolute_stall_budget() {
        let (_, route, _, _) = fixture();
        let media = media_binding(&route, StreamId::new()).unwrap();
        let mut pump = PlaybackPump::with_binding(Duration::from_secs(10), media.clone()).unwrap();
        let packet = capyio_audio::AudioMediaPacket {
            stream_id: media.stream_id,
            stream_epoch: media.stream_epoch,
            sequence: 0,
            source_timestamp_micros: 1,
            first_sample_index: 0,
            sample_count: 480,
            discontinuity: false,
            payload: vec![1; 960],
        };
        pump.accept(&packet, Duration::ZERO).unwrap();
        let mut sink = FakeSink {
            writes: 0,
            bytes: vec![],
            limit: 1,
        };
        pump.flush(&mut sink, Duration::from_millis(200)).unwrap();
        assert!(pump.flush(&mut sink, MAX_STALL).is_err());
        assert_eq!(sink.writes, 1);
    }
}

#[cfg(test)]
mod consent_deadline_tests {
    use super::*;
    #[test]
    fn secure_launcher_negotiation_never_refreshes_original_monotonic_consent() {
        let consent = Instant::now();
        let original_end = consent + Duration::from_secs(10);
        let after_negotiation = consent + Duration::from_secs(7);
        assert_eq!(
            consent_end(
                after_negotiation,
                Duration::from_secs(10),
                original_end,
                10_000
            )
            .unwrap(),
            original_end
        );
        assert_eq!(
            consent_end(
                after_negotiation,
                Duration::from_secs(10),
                original_end,
                1_000
            )
            .unwrap(),
            after_negotiation + Duration::from_secs(1)
        );
        assert!(consent_end(original_end, Duration::from_secs(10), original_end, 10_000).is_err());
        assert!(consent_end(after_negotiation, Duration::from_secs(10), original_end, 0).is_err());
    }
}

#[cfg(test)]
mod terminal_cause_tests {
    use super::*;

    #[test]
    fn media_terminal_classification_uses_typed_io_not_error_text() {
        for (kind, expected) in [
            (io::ErrorKind::UnexpectedEof, TerminalCause::MediaClosed),
            (io::ErrorKind::ConnectionReset, TerminalCause::MediaClosed),
            (io::ErrorKind::ConnectionAborted, TerminalCause::MediaClosed),
            (io::ErrorKind::TimedOut, TerminalCause::MediaTimeout),
            (io::ErrorKind::WouldBlock, TerminalCause::MediaTimeout),
            (io::ErrorKind::BrokenPipe, TerminalCause::MediaFailure),
            (io::ErrorKind::Other, TerminalCause::MediaFailure),
            (io::ErrorKind::InvalidData, TerminalCause::MediaFailure),
        ] {
            let mapped = media_error(ControlTlsError::Io(io::Error::new(
                kind,
                "untrusted deadline EOF reset text",
            )));
            assert_eq!(mapped.kind(), kind);
            assert_eq!(media_cause(&mapped), expected);
        }
        let tls = media_error(ControlTlsError::ChannelFailed);
        assert_eq!(media_cause(&tls), TerminalCause::MediaFailure);
    }

    #[test]
    fn abrupt_tls_close_without_wrapper_prefix_retains_exact_io_cause() {
        let display = "peer closed connection without sending TLS close_notify";
        for kind in [
            io::ErrorKind::UnexpectedEof,
            io::ErrorKind::TimedOut,
            io::ErrorKind::WouldBlock,
            io::ErrorKind::Other,
        ] {
            let mapped = media_error(ControlTlsError::Io(io::Error::new(kind, display)));
            assert_eq!(mapped.to_string(), display);
            assert_eq!(mapped.kind(), kind);
            assert_eq!(
                media_cause(&mapped) == TerminalCause::MediaClosed,
                kind == io::ErrorKind::UnexpectedEof
            );
            let control = CatalogLinkError::Transport(ControlTlsError::Io(mapped));
            assert_eq!(
                control_cause(&control) == TerminalCause::ControlClosed,
                kind == io::ErrorKind::UnexpectedEof
            );
        }
    }

    #[test]
    fn control_terminal_classification_never_confuses_timeout_with_close() {
        assert_eq!(
            control_cause(&CatalogLinkError::Closed),
            TerminalCause::ControlClosed
        );
        for kind in [
            io::ErrorKind::UnexpectedEof,
            io::ErrorKind::ConnectionReset,
            io::ErrorKind::ConnectionAborted,
            io::ErrorKind::TimedOut,
            io::ErrorKind::WouldBlock,
            io::ErrorKind::BrokenPipe,
            io::ErrorKind::Other,
        ] {
            let reason = CatalogLinkError::Transport(ControlTlsError::Io(io::Error::new(
                kind,
                "EOF closed timeout",
            )));
            assert_eq!(
                control_cause(&reason),
                if closed_kind(kind) {
                    TerminalCause::ControlClosed
                } else {
                    TerminalCause::Failure
                }
            );
        }
        assert_eq!(
            control_cause(&CatalogLinkError::Transport(ControlTlsError::ChannelFailed)),
            TerminalCause::Failure
        );
    }

    #[test]
    fn first_terminal_cause_and_clock_survive_cleanup_and_later_failures() {
        for cause in [
            TerminalCause::Failure,
            TerminalCause::LocalStop,
            TerminalCause::PeerStop,
            TerminalCause::MediaClosed,
            TerminalCause::MediaTimeout,
            TerminalCause::MediaFailure,
            TerminalCause::ControlClosed,
        ] {
            let mut observation = TerminalObservation::default();
            // Model failed first clock read: later cleanup must not fill it in.
            observation.record(None);
            observation.observe(cause);
            observation.observe(TerminalCause::Failure);
            observation.observe(TerminalCause::LocalStop);
            assert_eq!(observation.cause, Some(cause));
            assert_eq!(observation.at_ms, None);
        }
    }
}
