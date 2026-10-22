//! Finite mono playback over a separate pinned TLS data connection. No default
//! listener, pairing, capture permission, UDP fallback or authority renewal.
use crate::{
    EndpointMode, MAX_STALL, PcmPlaybackSink, PipeWirePlayback, PlaybackMetrics, PlaybackPump,
};
use capyio_audio::{AudioMediaStreamBinding, AudioStreamSpec};
use capyio_control_link::{CatalogControlLink, ReceivedControlMessage};
use capyio_control_tls::{
    ControlTlsBinding, ControlTlsServerIdentity,
    audio::{AudioTlsBinding, AudioTlsListener, AudioTlsStopHandle},
};
use capyio_core::{
    AuthorizationState, Availability, MessageId, Platform, PortDirection, Route, RouteId,
    RouteState, SessionState, StreamId,
};
use capyio_native_audio_lan::{SecureMonoPcmAdmission, SecureMonoPcmRateBudget};
use capyio_protocol::{
    PortLeaseAction, PortLeaseOutcome, RouteCommandAction, SecureAudioDataOffer,
    SecureAudioTransport,
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
/// never remote metadata. `identity` passed to `new` must be the paired identity
/// used to admit the accepted control link.
#[derive(Clone, Debug)]
pub struct SecurePlaybackAdmission {
    pub expected: ControlTlsBinding,
    pub route_id: RouteId,
    pub stream_id: StreamId,
    pub local_address: IpAddr,
    /// Whole seconds in 1..=60; all negotiation consumes this same budget.
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
}
impl TerminalObservation {
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
    sink_claim: PortLeaseClaim,
    identity: Option<ControlTlsServerIdentity>,
    source_request: MessageId,
    source_request_deadline: Instant,
    source_expiry: Option<u64>,
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
        mut runtime: NodeRuntime,
        mut link: CatalogControlLink,
        identity: ControlTlsServerIdentity,
        admission: SecurePlaybackAdmission,
    ) -> io::Result<Self> {
        let started = Instant::now();
        let now = now_ms()?;
        let original = runtime.route(admission.route_id).map_err(error)?.clone();
        validate_target(&admission)?;
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
        let rate = SecureMonoPcmRateBudget::new(admission.lifetime).map_err(error)?;
        let authority_ms = expiry(&original)?.min(sink_claim.expires_at_ms);
        let end = started
            + admission
                .lifetime
                .min(Duration::from_millis(authority_ms - now));
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
            sink_claim,
            identity: Some(identity),
            source_request,
            source_request_deadline: deadline,
            source_expiry: None,
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
            // Do not replace the original failure with a cleanup error. An
            // uncompleted bounded join is retained for an explicit stop retry.
            let _ = self.stop();
        }
        result
    }
    fn poll_inner(&mut self) -> io::Result<Option<SecurePlaybackEvent>> {
        self.check(now_ms()?)?;
        let deadline = self.operation_deadline();
        if let Some(message) = self
            .link
            .poll_control_message_until(deadline)
            .map_err(error)?
        {
            if let ReceivedControlMessage::PortLeaseResponse(response) = message {
                let now = now_ms()?;
                self.check(now)?;
                if self.source_expiry.is_some() {
                    return Err(error("duplicate secure audio Source lease response"));
                }
                let expiry =
                    validate_source_response(&self.original, self.source_request, response, now)?;
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
            Some(DataEvent::Failed(reason)) => return Err(reason),
            Some(DataEvent::Bound) => {
                self.check(now_ms()?)?;
                if self.sink.is_some() {
                    return Err(error("duplicate secure audio binding"));
                }
                // The worker emits Bound only after the exact pinned TLS ACK.
                self.sink = Some(PipeWirePlayback::open(&self.admission.pipewire_target)?);
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
            source_expiry: self.source_expiry.unwrap_or(0),
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
                            let bytes = channel.receive_record_until(deadline).map_err(error)?;
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
    pub fn stop(&mut self) -> io::Result<()> {
        // Record before *any* cleanup. poll errors, authenticated peer Stop and
        // local Stop all converge here; cleanup latency is not observation time.
        self.terminal_observation.record(now_ms().ok());
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
    source_expiry: u64,
    end: Instant,
    last_wall: &'a mut u64,
}
impl<S: PcmPlaybackSink> GuardedSink<'_, S> {
    fn check(&mut self) -> io::Result<()> {
        let now = now_ms()?;
        if now < *self.last_wall || now >= self.source_expiry || Instant::now() >= self.end {
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
            || fresh >= self.source_expiry
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
fn validate_target(admission: &SecurePlaybackAdmission) -> io::Result<()> {
    crate::validate_endpoint_target(EndpointMode::Playback, &admission.pipewire_target)
        .map_err(error)?;
    if !(Duration::from_secs(1)..=Duration::from_secs(60)).contains(&admission.lifetime)
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
fn media_binding(route: &Route, stream_id: StreamId) -> io::Result<AudioMediaStreamBinding> {
    Ok(AudioMediaStreamBinding {
        session_id: route.session_id,
        route_id: route.id,
        stream_id,
        stream_epoch: u32::try_from(route.epoch).map_err(error)?,
        selected_spec: AudioStreamSpec::voice_interactive(),
    })
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

    fn fixture_at(now: u64) -> (NodeRuntime, Route, ControlTlsBinding, PortLeaseClaim) {
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
        let format = FormatDescriptor::new(capyio_protocol::SECURE_AUDIO_MONO_FORMAT);
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
                        port.qos_modes.insert(QosMode::Interactive);
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
        ] {
            let mut changed = admission.clone();
            changed.lifetime = lifetime;
            assert!(validate_target(&changed).is_err());
        }
        for address in ["0.0.0.0", "::", "224.0.0.1", "ff02::1"] {
            let mut changed = admission.clone();
            changed.local_address = address.parse().unwrap();
            assert!(validate_target(&changed).is_err());
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
                source_expiry: now + 5000,
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
            source_expiry: now + 5000,
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
            source_expiry: now + 5000,
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
