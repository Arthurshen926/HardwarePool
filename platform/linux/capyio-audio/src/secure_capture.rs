//! Finite selected-source stereo capture over a separate pinned TLS channel.
//! No import, publication or peer request grants capture. The caller supplies
//! explicit local consent, the current paired identity and one Prepared Route.
//! This reusable owner is not a Node/console launcher or a physical-device claim.
use crate::{CaptureEndpoint, CaptureMetrics, CapturePump, MAX_STALL, PipeWireCapture};
use capyio_audio::{AudioMediaStreamBinding, AudioStreamSpec};
use capyio_control_link::{CatalogControlLink, CatalogLinkError, ReceivedControlMessage};
use capyio_control_tls::{
    ControlTlsBinding, ControlTlsClientIdentity, ControlTlsEndpoint, ControlTlsError,
    audio_stereo::{StereoAudioTlsBinding, StereoAudioTlsChannel, StereoAudioTlsStopHandle},
};
use capyio_core::*;
use capyio_native_audio_lan::{SecureStereoPcmAdmission, SecureStereoPcmRateBudget};
use capyio_protocol::{
    PortLeaseAction, PortLeaseOutcome, RouteCommandAction, SECURE_AUDIO_STEREO_FORMAT,
    SecureAudioDataOffer, SecureAudioTransport, ValidatedPortLeaseResponse,
};
use capyio_runtime::{NodeRuntime, PortLeaseClaim, PortLeasePolicy};
use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
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

/// Trusted local intent, never populated from peer-provided target metadata.
/// The deadline passed to `new_until` must originate at the original consent.
#[derive(Clone, Debug)]
pub struct SecureStereoSourceAdmission {
    pub expected: ControlTlsBinding,
    pub route_id: RouteId,
    pub stream_id: StreamId,
    pub lifetime: Duration,
    pub pipewire_target: String,
}
/// Trusted local intent for a receiver-selected StreamId. This admission has no
/// placeholder ID: only the first exact authenticated stereo offer selects it.
/// Selection is not TLS readiness, Route activation, or physical audibility.
#[derive(Clone, Debug)]
pub struct SecureStereoReceiverAssignedSourceAdmission {
    pub expected: ControlTlsBinding,
    pub route_id: RouteId,
    pub lifetime: Duration,
    pub pipewire_target: String,
}

#[derive(Clone, Copy)]
enum StreamSelection {
    Strict(StreamId),
    ReceiverAssigned,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecureStereoSourceEvent {
    Starting,
    Bound,
    RecordSent,
    Stopped,
}

/// Cancellation is independently usable while the worker is in connect/TLS/write.
/// A Stop racing socket registration is latched. TCP connect itself remains
/// bounded by the original operation deadline; no reconnect is attempted.
#[derive(Clone, Default)]
pub struct SecureStereoSourceStopHandle {
    stopped: Arc<AtomicBool>,
    media: Arc<Mutex<Option<StereoAudioTlsStopHandle>>>,
}
impl SecureStereoSourceStopHandle {
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        if let Ok(media) = self.media.lock()
            && let Some(media) = media.as_ref()
        {
            media.stop();
        }
    }
    fn install(&self, stop: StereoAudioTlsStopHandle) {
        if let Ok(mut media) = self.media.lock() {
            if self.stopped.load(Ordering::Acquire) {
                stop.stop();
            }
            *media = Some(stop);
        } else {
            stop.stop();
        }
    }
}

enum DataEvent {
    Bound,
    Sent,
    Failed(io::Error),
}
struct SendPermit {
    record: [u8; 2128],
    deadline: Instant,
}

/// One control owner, one one-shot TLS worker, one capture assembly and one
/// outstanding record. Poll continuously on a non-real-time lifecycle worker.
/// No Runtime/control lock is held during media I/O. The owner never renews a
/// lease, reconnects, substitutes a default source, or falls back to UDP.
pub struct SecureStereoSourceOwner {
    runtime: NodeRuntime,
    link: CatalogControlLink,
    admission: SecureStereoReceiverAssignedSourceAdmission,
    stream_selection: StreamSelection,
    selected_stream_id: Option<StreamId>,
    original: Route,
    session: Session,
    source_catalog: CapabilityDescriptor,
    source_adapter: AdapterInstanceDescriptor,
    sink_catalog: CapabilityDescriptor,
    sink_adapter: AdapterInstanceDescriptor,
    source_claim: Option<PortLeaseClaim>,
    sink_claim: Option<ValidatedPortLeaseResponse>,
    sink_request: MessageId,
    local_consent_end: Instant,
    local_wall_end: u64,
    end: Instant,
    started: Instant,
    negotiation_end: Instant,
    last_wall: u64,
    identity: Option<ControlTlsClientIdentity>,
    server_name: String,
    stop: SecureStereoSourceStopHandle,
    worker: Option<JoinHandle<()>>,
    events: Option<Receiver<DataEvent>>,
    writes: Option<SyncSender<SendPermit>>,
    write_pending: bool,
    pending_end: Option<Instant>,
    input: Option<PipeWireCapture>,
    pump: Option<CapturePump>,
    encoder: Option<SecureStereoPcmAdmission>,
    rate: SecureStereoPcmRateBudget,
    packets_sent: u64,
    closed: bool,
    terminal_cause: Option<&'static str>,
    terminal_at: Option<u64>,
    terminal_recorded: bool,
}
impl SecureStereoSourceOwner {
    /// Both peers must independently approve the exact finite Prepared Route.
    /// No initial local claim is accepted: the first authenticated peer Acquire
    /// creates it once. Subsequent Acquire/Renew cannot silently refresh it.
    pub fn new_until(
        runtime: NodeRuntime,
        link: CatalogControlLink,
        identity: ControlTlsClientIdentity,
        server_name: String,
        admission: SecureStereoSourceAdmission,
        original_consent_end: Instant,
    ) -> io::Result<Self> {
        validate_admission(&admission)?;
        let selection = StreamSelection::Strict(admission.stream_id);
        Self::new_with_selection_until(
            runtime,
            link,
            identity,
            server_name,
            SecureStereoReceiverAssignedSourceAdmission {
                expected: admission.expected,
                route_id: admission.route_id,
                lifetime: admission.lifetime,
                pipewire_target: admission.pipewire_target,
            },
            original_consent_end,
            selection,
        )
    }

    /// Learn the receiver's nonnil StreamId exactly once from its authenticated
    /// offer, after validating the original finite authority. No retry/rebinding.
    pub fn new_receiver_assigned_until(
        runtime: NodeRuntime,
        link: CatalogControlLink,
        identity: ControlTlsClientIdentity,
        server_name: String,
        admission: SecureStereoReceiverAssignedSourceAdmission,
        original_consent_end: Instant,
    ) -> io::Result<Self> {
        Self::new_with_selection_until(
            runtime,
            link,
            identity,
            server_name,
            admission,
            original_consent_end,
            StreamSelection::ReceiverAssigned,
        )
    }

    fn new_with_selection_until(
        mut runtime: NodeRuntime,
        mut link: CatalogControlLink,
        identity: ControlTlsClientIdentity,
        server_name: String,
        admission: SecureStereoReceiverAssignedSourceAdmission,
        original_consent_end: Instant,
        stream_selection: StreamSelection,
    ) -> io::Result<Self> {
        validate_source_intent(&admission.pipewire_target, admission.lifetime)?;
        let started = Instant::now();
        let now = now_ms()?;
        let original = runtime.route(admission.route_id).map_err(error)?.clone();
        validate_prepared(&runtime, &original, admission.expected, now)?;
        if link.tls_binding() != admission.expected
            || link.peer_node_id() != original.sink.node_id
            || link.authenticated_peer_certificate().map_err(error)?
                != identity.expected_peer_certificate
            || server_name.is_empty()
            || server_name.len() > 253
            || runtime
                .route_port_lease_claim(original.id, original.source)
                .is_some()
        {
            return Err(error(
                "secure stereo requires the exact paired control and a fresh Source claim",
            ));
        }
        let local_consent_end = original_consent_end
            .min(started + admission.lifetime)
            .min(started + Duration::from_millis(expiry(&original)? - now));
        let remaining = local_consent_end
            .checked_duration_since(started)
            .filter(|remaining| *remaining > MAX_STALL)
            .ok_or_else(|| error("original stereo Source consent elapsed"))?;
        let local_wall_end = now
            .checked_add(remaining.as_millis() as u64)
            .ok_or_else(|| error("stereo Source deadline overflow"))?;
        let session = runtime
            .session(admission.expected.session_id)
            .map_err(error)?
            .clone();
        let (source_catalog, source_adapter, sink_catalog, sink_adapter) =
            catalogs(&runtime, &original)?;
        let rate = SecureStereoPcmRateBudget::new(admission.lifetime).map_err(error)?;
        runtime
            .set_local_port_lease_policy(original.source, PortLeasePolicy::Exclusive)
            .map_err(error)?;
        let negotiation_end = local_consent_end.min(started + MAX_STALL);
        let ttl = lease_ttl(local_wall_end, now)?;
        let sink_request = match link.send_port_lease_request_until(
            original.id,
            original.sink,
            PortLeaseAction::Acquire,
            ttl,
            negotiation_end,
        ) {
            Ok(request) => request,
            Err(reason) => {
                let _ = link.close_and_reconcile(&mut runtime);
                return Err(error(reason));
            }
        };
        Ok(Self {
            runtime,
            link,
            admission,
            stream_selection,
            selected_stream_id: None,
            original,
            session,
            source_catalog,
            source_adapter,
            sink_catalog,
            sink_adapter,
            source_claim: None,
            sink_claim: None,
            sink_request,
            local_consent_end,
            local_wall_end,
            end: local_consent_end,
            started,
            negotiation_end,
            last_wall: now,
            identity: Some(identity),
            server_name,
            stop: SecureStereoSourceStopHandle::default(),
            worker: None,
            events: None,
            writes: None,
            write_pending: false,
            pending_end: None,
            input: None,
            pump: None,
            encoder: None,
            rate,
            packets_sent: 0,
            closed: false,
            terminal_cause: None,
            terminal_at: None,
            terminal_recorded: false,
        })
    }
    pub fn runtime(&self) -> &NodeRuntime {
        &self.runtime
    }
    /// Mutations never renew this owner: every later boundary requires the
    /// original Route/catalog/claim, or the single owned Prepared -> Starting.
    pub fn runtime_mut(&mut self) -> &mut NodeRuntime {
        &mut self.runtime
    }
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    /// Diagnostic only: a selected offer can still fail TLS. This never implies
    /// Bound, helper creation, Active, PCM acceptance, or physical audibility.
    pub fn selected_stream_id(&self) -> Option<StreamId> {
        self.selected_stream_id
    }
    pub fn helper_pid(&self) -> Option<u32> {
        self.input.as_ref().map(PipeWireCapture::pid)
    }
    pub fn source_lease_expires_at_ms(&self) -> Option<u64> {
        self.source_claim.map(|c| c.expires_at_ms)
    }
    pub fn sink_lease_expires_at_ms(&self) -> Option<u64> {
        self.sink_claim.and_then(|c| c.expires_at_ms)
    }
    pub fn terminal_cause(&self) -> Option<&'static str> {
        self.terminal_cause
    }
    pub fn terminal_observed_at_ms(&self) -> Option<u64> {
        self.terminal_at
    }
    pub fn metrics(&self) -> CaptureMetrics {
        self.pump
            .as_ref()
            .map(CapturePump::metrics)
            .unwrap_or_default()
    }
    pub fn packets_sent(&self) -> u64 {
        self.packets_sent
    }
    pub fn stop_handle(&self) -> SecureStereoSourceStopHandle {
        self.stop.clone()
    }
    fn observe(&mut self, cause: &'static str) {
        if !self.terminal_recorded {
            self.terminal_recorded = true;
            self.terminal_at = now_ms().ok();
            self.terminal_cause = Some(cause);
        }
    }
    fn fail(&mut self, cause: &'static str, reason: &'static str) -> io::Result<()> {
        self.observe(cause);
        Err(error(reason))
    }
    fn operation_end(&self) -> Instant {
        self.end.min(Instant::now() + MAX_STALL)
    }
    fn check(&mut self) -> io::Result<()> {
        let now = now_ms()?;
        if self.closed || self.stop.stopped.load(Ordering::Acquire) {
            return self.fail("local_stop", "secure stereo Source stopped");
        }
        if now < self.last_wall {
            return self.fail("clock_regressed", "stereo Source clock regressed");
        }
        if Instant::now() >= self.end
            || now >= self.local_wall_end
            || now >= expiry(&self.original)?
            || self.source_claim.is_some_and(|c| now >= c.expires_at_ms)
            || self
                .sink_claim
                .and_then(|c| c.expires_at_ms)
                .is_some_and(|end| now >= end)
        {
            return self.fail("expired", "original secure stereo authority expired");
        }
        if self.link.is_closed() {
            return self.fail("control_closed", "stereo Source control closed");
        }
        if validate_source_claim(&self.runtime, &self.original, self.source_claim, now).is_err() {
            return self.fail(
                "source_revoked",
                "original stereo Source claim changed or absent",
            );
        }
        if self.runtime.session(self.session.id).map_err(error)? != &self.session {
            return self.fail("session_changed", "original stereo control Session changed");
        }
        validate_current(&self.runtime, &self.original, self.admission.expected, now)?;
        let current = catalogs(&self.runtime, &self.original)?;
        if current
            != (
                self.source_catalog.clone(),
                self.source_adapter.clone(),
                self.sink_catalog.clone(),
                self.sink_adapter.clone(),
            )
        {
            return self.fail(
                "catalog_changed",
                "original stereo catalog or Adapter changed",
            );
        }
        if self.original.state == RouteState::Prepared && Instant::now() >= self.negotiation_end {
            return self.fail(
                "negotiation_timeout",
                "stereo lease negotiation deadline elapsed",
            );
        }
        if self.pending_end.is_some_and(|end| Instant::now() >= end) {
            return self.fail("media_timeout", "stereo media operation deadline elapsed");
        }
        self.last_wall = now;
        Ok(())
    }
    pub fn poll(&mut self) -> io::Result<Option<SecureStereoSourceEvent>> {
        let result = self.poll_inner();
        if result.is_err() {
            self.observe("failure");
            let _ = self.stop();
        }
        result
    }
    fn poll_inner(&mut self) -> io::Result<Option<SecureStereoSourceEvent>> {
        self.check()?;
        let deadline = self.operation_end();
        let message = match self.link.poll_control_message_until(deadline) {
            Ok(message) => message,
            Err(reason) => {
                self.observe(control_cause(&reason));
                return Err(error(reason));
            }
        };
        if let Some(message) = message
            && self.handle_control(message)?
        {
            self.stop()?;
            return Ok(Some(SecureStereoSourceEvent::Stopped));
        }
        self.check()?;
        if self.original.state == RouteState::Prepared
            && self.source_claim.is_some()
            && self.sink_claim.is_some()
        {
            self.runtime
                .begin_route_start(self.original.id, now_ms()?)
                .map_err(error)?;
            let mut expected = self.original.clone();
            expected.state = RouteState::Starting;
            expected.epoch = expected
                .epoch
                .checked_add(1)
                .ok_or_else(|| error("stereo Source epoch overflow"))?;
            if self.runtime.route(expected.id).map_err(error)? != &expected {
                return Err(error("unexpected stereo Source Start transition"));
            }
            self.original = expected;
            self.check()?;
            self.link
                .send_route_command_until(
                    self.original.id,
                    RouteCommandAction::Start,
                    self.operation_end(),
                )
                .map_err(error)?;
            // A missing offer is terminal, not an invitation to reconnect later.
            self.pending_end = Some(self.operation_end());
            return Ok(Some(SecureStereoSourceEvent::Starting));
        }
        let event = match self.events.as_ref().map(Receiver::try_recv) {
            None | Some(Err(TryRecvError::Empty)) => None,
            Some(Ok(event)) => Some(event),
            Some(Err(TryRecvError::Disconnected)) => {
                return self
                    .fail("media_closed", "stereo media worker ended")
                    .map(|_| None);
            }
        };
        match event {
            Some(DataEvent::Failed(reason)) => {
                self.observe(media_cause(&reason));
                return Err(reason);
            }
            Some(DataEvent::Bound) => {
                self.check()?;
                if self.input.is_some() {
                    return Err(error("duplicate stereo TLS readiness"));
                }
                let endpoint =
                    CaptureEndpoint::SelectedSource(self.admission.pipewire_target.clone());
                self.input = Some(PipeWireCapture::open_checked(&endpoint, &mut || {
                    self.check()
                })?);
                self.pending_end = None;
                self.check()?;
                return Ok(Some(SecureStereoSourceEvent::Bound));
            }
            Some(DataEvent::Sent) => {
                self.check()?;
                if !self.write_pending {
                    return Err(error("unexpected stereo send completion"));
                }
                self.write_pending = false;
                self.pending_end = None;
                self.packets_sent += 1;
                return Ok(Some(SecureStereoSourceEvent::RecordSent));
            }
            None => {}
        }
        if self.input.is_some() && !self.write_pending {
            self.check()?;
            let captured = self
                .pump
                .as_mut()
                .ok_or_else(|| error("stereo capture pump absent"))?
                .poll(
                    self.input.as_mut().expect("checked capture"),
                    self.started.elapsed(),
                );
            let packet = match captured {
                Ok(packet) => packet,
                Err(reason) => {
                    self.observe(if closed_kind(reason.kind()) {
                        "capture_closed"
                    } else if reason.kind() == io::ErrorKind::TimedOut {
                        "capture_timeout"
                    } else {
                        "capture_failure"
                    });
                    return Err(reason);
                }
            };
            self.check()?;
            if let Some(packet) = packet {
                self.rate.admit(self.started.elapsed()).map_err(error)?;
                let record = self
                    .encoder
                    .as_mut()
                    .ok_or_else(|| error("stereo encoder absent"))?
                    .encode_record(&packet)
                    .map_err(error)?;
                self.check()?;
                let deadline = self.operation_end();
                self.writes
                    .as_ref()
                    .ok_or_else(|| error("stereo sender absent"))?
                    .try_send(SendPermit { record, deadline })
                    .map_err(error)?;
                self.write_pending = true;
                self.pending_end = Some(deadline);
                self.check()?;
            }
        }
        Ok(None)
    }
    fn handle_control(&mut self, message: ReceivedControlMessage) -> io::Result<bool> {
        self.check()?;
        match message {
            ReceivedControlMessage::PortLeaseRequest(received) => {
                let request = received.request();
                let claim_anchor = Instant::now();
                let now = now_ms()?;
                let requested_end =
                    Instant::now().checked_add(Duration::from_millis(request.requested_ttl_ms));
                if self.source_claim.is_some()
                    || request.session_id != self.original.session_id
                    || request.route_id != self.original.id
                    || request.port != self.original.source
                    || request.action != PortLeaseAction::Acquire
                    || request.requested_ttl_ms > 60_000
                    || requested_end.is_none_or(|end| end > self.local_consent_end)
                    || now
                        .checked_add(request.requested_ttl_ms)
                        .is_none_or(|end| end > self.local_wall_end)
                {
                    return Err(error(
                        "only one exact finite initial Source Acquire is permitted",
                    ));
                }
                let deadline = self.operation_end();
                self.link
                    .answer_port_lease_request_until(&mut self.runtime, received, now, deadline)
                    .map_err(error)?;
                let claim = *self
                    .runtime
                    .route_port_lease_claim(self.original.id, self.original.source)
                    .ok_or_else(|| error("initial stereo Source Acquire denied"))?;
                if claim.port != self.original.source
                    || claim.route_id != self.original.id
                    || claim.session_id != self.original.session_id
                    || claim.expires_at_ms <= now
                    || claim.expires_at_ms > self.local_wall_end
                {
                    return Err(error("invalid original stereo Source claim"));
                }
                self.source_claim = Some(claim);
                self.end = self
                    .end
                    .min(claim_anchor + Duration::from_millis(claim.expires_at_ms - now));
            }
            ReceivedControlMessage::PortLeaseResponse(response) => {
                let claim_anchor = Instant::now();
                let now = now_ms()?;
                if self.sink_claim.is_some() {
                    return Err(error("duplicate stereo Sink lease response"));
                }
                let expiry =
                    validate_sink_response(&self.original, self.sink_request, response, now)?;
                self.sink_claim = Some(response);
                self.end = self
                    .end
                    .min(claim_anchor + Duration::from_millis(expiry - now));
            }
            ReceivedControlMessage::RouteStatus(status) => {
                if status.session_id != self.original.session_id
                    || status.route != self.original
                    || status.native_audio_data.is_some()
                    || status.secure_camera_data.is_some()
                    || status.camera_data.is_some()
                    || status.touchpad_data.is_some()
                    || status.haptics_data.is_some()
                {
                    return Err(error(
                        "stereo peer status changed the exact Route or transport",
                    ));
                }
                if let Some(offer) = status.secure_audio_data {
                    self.connect(offer)?;
                }
            }
            ReceivedControlMessage::RouteCommand(command)
                if command.session_id == self.original.session_id
                    && command.route_id == self.original.id
                    && command.action == RouteCommandAction::Stop =>
            {
                self.observe("peer_stop");
                return Ok(true);
            }
            ReceivedControlMessage::Heartbeat(heartbeat) => {
                if !heartbeat.acknowledgement {
                    self.link
                        .send_heartbeat_until(heartbeat.sequence, true, self.operation_end())
                        .map_err(error)?;
                }
            }
            ReceivedControlMessage::Catalog(catalog) => {
                self.link
                    .apply_catalog(&mut self.runtime, catalog)
                    .map_err(error)?;
            }
            _ => return Err(error("unexpected dedicated stereo Source control message")),
        }
        self.check()?;
        Ok(false)
    }
    fn connect(&mut self, offer: SecureAudioDataOffer) -> io::Result<()> {
        self.check()?;
        if self.source_claim.is_none()
            || self.sink_claim.is_none()
            || self.original.state != RouteState::Starting
            || matches!(self.stream_selection, StreamSelection::Strict(id) if offer.stream_id != id)
            || self.selected_stream_id.is_some()
            || offer.stream_id.as_uuid().is_nil()
            || offer.transport != SecureAudioTransport::Tls13MtlsStereoV1
            || self.worker.is_some()
            || self.identity.is_none()
        {
            return Err(error(
                "stereo offer precedes exact authority or is repeated",
            ));
        }
        // The authenticated receipt is consumed even if a subsequent step fails.
        self.link
            .take_received_secure_stereo_audio_offer(&self.original, offer)
            .map_err(error)?;
        let binding = StereoAudioTlsBinding::from_route(
            self.admission.expected,
            &self.original,
            offer.stream_id,
            now_ms()?,
        )
        .map_err(error)?;
        // Receipt, exact Route/profile binding and original claims are all
        // validated before latching. Never clear this diagnostic on failure/Stop.
        self.check()?;
        self.selected_stream_id = Some(offer.stream_id);
        let media = AudioMediaStreamBinding {
            session_id: self.original.session_id,
            route_id: self.original.id,
            stream_id: offer.stream_id,
            stream_epoch: self.original.epoch.try_into().map_err(error)?,
            selected_spec: AudioStreamSpec::media_balanced(),
        };
        self.pump = Some(CapturePump::with_binding(
            self.admission.lifetime,
            media.clone(),
        )?);
        self.encoder = Some(SecureStereoPcmAdmission::new(media).map_err(error)?);
        let address = std::net::SocketAddr::new(
            self.link.authenticated_peer_addr().map_err(error)?.ip(),
            offer.port,
        );
        let identity = self
            .identity
            .take()
            .ok_or_else(|| error("stereo identity consumed"))?;
        let server_name = self.server_name.clone();
        let operation_end = self
            .pending_end
            .unwrap_or(self.end)
            .min(self.operation_end());
        let media_end = self.end;
        self.pending_end = Some(operation_end);
        let stop = self.stop.clone();
        let (events_tx, events_rx) = mpsc::sync_channel(1);
        let (writes_tx, writes_rx) = mpsc::sync_channel::<SendPermit>(1);
        self.events = Some(events_rx);
        self.writes = Some(writes_tx);
        self.worker = Some(
            thread::Builder::new()
                .name("capyio-stereo-source".into())
                .spawn(move || {
                    let result = (|| -> io::Result<()> {
                        let mut channel = StereoAudioTlsChannel::connect_until_with_stop(
                            ControlTlsEndpoint {
                                address,
                                io_timeout: MAX_STALL,
                            },
                            &server_name,
                            identity,
                            binding,
                            media_end,
                            |handle| stop.install(handle),
                        )
                        .map_err(media_error)?;
                        if events_tx.send(DataEvent::Bound).is_err() {
                            return Ok(());
                        }
                        while let Ok(permit) = writes_rx.recv() {
                            if stop.stopped.load(Ordering::Acquire)
                                || Instant::now() >= permit.deadline
                            {
                                return Err(io::Error::new(
                                    io::ErrorKind::TimedOut,
                                    "stereo send permit expired or stopped",
                                ));
                            }
                            channel
                                .send_record_until(&permit.record, permit.deadline)
                                .map_err(media_error)?;
                            if events_tx.send(DataEvent::Sent).is_err() {
                                return Ok(());
                            }
                        }
                        channel.close();
                        Ok(())
                    })();
                    if let Err(reason) = result {
                        let _ = events_tx.send(DataEvent::Failed(reason));
                    }
                })?,
        );
        Ok(())
    }
    /// Explicit local Stop delivered as an authenticated command before closing.
    /// A failed command still tears down all owned resources; it is never retried.
    pub fn request_stop(&mut self) -> io::Result<()> {
        self.observe("local_stop");
        // Cancel media immediately, even if the bounded control Stop write stalls.
        self.stop.stop();
        let sent = if !self.closed {
            self.link
                .send_route_command_until(
                    self.original.id,
                    RouteCommandAction::Stop,
                    self.operation_end(),
                )
                .map_err(error)
        } else {
            Ok(())
        };
        let stopped = self.stop();
        sent.and(stopped)
    }
    /// Record the first terminal observation before closing or helper cleanup.
    /// Drop bounded channels before joining so no worker can remain queue-blocked.
    pub fn stop(&mut self) -> io::Result<()> {
        self.observe("local_stop");
        self.closed = true;
        self.stop.stop();
        self.writes.take();
        self.events.take();
        if let Some(pump) = &mut self.pump {
            pump.discard();
        }
        let control = self
            .link
            .close_and_reconcile(&mut self.runtime)
            .map_err(error);
        let capture = self
            .input
            .as_mut()
            .map(PipeWireCapture::stop)
            .unwrap_or(Ok(()));
        let network = if let Some(worker) = self.worker.as_ref() {
            let deadline = Instant::now() + MAX_STALL;
            while !worker.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(2));
            }
            if worker.is_finished() {
                self.worker
                    .take()
                    .expect("owned stereo worker")
                    .join()
                    .map_err(|_| error("stereo worker panicked"))
            } else {
                Err(error("stereo worker cleanup deadline elapsed"))
            }
        } else {
            Ok(())
        };
        control.and(capture).and(network)
    }
}
impl Drop for SecureStereoSourceOwner {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn validate_admission(admission: &SecureStereoSourceAdmission) -> io::Result<()> {
    if admission.stream_id.as_uuid().is_nil() {
        return Err(error("exact finite stereo Source consent required"));
    }
    validate_source_intent(&admission.pipewire_target, admission.lifetime)
}
fn validate_source_intent(target: &str, lifetime: Duration) -> io::Result<()> {
    CaptureEndpoint::SelectedSource(target.to_owned())
        .validate()
        .map_err(error)?;
    if !(Duration::from_secs(1)..=Duration::from_secs(60)).contains(&lifetime)
        || lifetime.subsec_nanos() != 0
    {
        return Err(error("exact finite stereo Source consent required"));
    }
    Ok(())
}
fn expiry(route: &Route) -> io::Result<u64> {
    match route.authorization {
        AuthorizationState::Authorized {
            expires_at_ms: Some(end),
        } if end > 0 => Ok(end),
        _ => Err(error("finite stereo Source authorization absent")),
    }
}
fn validate_prepared(
    runtime: &NodeRuntime,
    route: &Route,
    binding: ControlTlsBinding,
    now: u64,
) -> io::Result<()> {
    if route.state != RouteState::Prepared {
        return Err(error("fresh Prepared stereo Route required"));
    }
    validate_current(runtime, route, binding, now)
}
fn validate_current(
    runtime: &NodeRuntime,
    original: &Route,
    binding: ControlTlsBinding,
    now: u64,
) -> io::Result<()> {
    let route = runtime.route(original.id).map_err(error)?;
    let session = runtime.session(binding.session_id).map_err(error)?;
    let snapshot = runtime.snapshot();
    if route != original
        || route.session_id != binding.session_id
        || session.state != SessionState::Ready
        || !matches!(route.state, RouteState::Prepared | RouteState::Starting)
        || session.local_node_id != route.source.node_id
        || session.remote_node_id != route.sink.node_id
        || ![binding.server_node_id, binding.client_node_id].contains(&session.local_node_id)
        || ![binding.server_node_id, binding.client_node_id].contains(&session.remote_node_id)
        || snapshot.local_node.id != route.source.node_id
        || snapshot.local_node.platform != Platform::Linux
        || snapshot
            .routes
            .iter()
            .filter(|r| r.session_id == binding.session_id)
            .count()
            != 1
        || expiry(route)? <= now
        || route.epoch > i32::MAX as u64
        || route.profile != ProfileId::audio_frames_v1()
        || route.backend != RouteBackend::AdapterManaged
        || route.selected_format != Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT))
        || route.selected_qos_mode != Some(QosMode::Interactive)
    {
        return Err(error(
            "original exact stereo Source Runtime binding changed",
        ));
    }
    catalogs(runtime, route)?;
    Ok(())
}
fn catalogs(
    runtime: &NodeRuntime,
    route: &Route,
) -> io::Result<(
    CapabilityDescriptor,
    AdapterInstanceDescriptor,
    CapabilityDescriptor,
    AdapterInstanceDescriptor,
)> {
    let snapshot = runtime.snapshot();
    let get = |reference: PortRef, direction| {
        let node = if reference.node_id == snapshot.local_node.id {
            &snapshot.local_node
        } else {
            snapshot
                .peers
                .iter()
                .find(|node| node.id == reference.node_id)
                .ok_or_else(|| error("stereo peer catalog absent"))?
        };
        let capability = node
            .capabilities
            .get(&reference.capability_id)
            .ok_or_else(|| error("stereo Capability absent"))?;
        let adapter = node
            .adapter_instances
            .get(&capability.adapter_instance_id)
            .ok_or_else(|| error("stereo Adapter absent"))?;
        let port = capability
            .ports
            .get(&reference.port_id)
            .ok_or_else(|| error("stereo Port absent"))?;
        if node.online_state != OnlineState::Online
            || capability.availability != Availability::Available
            || port.availability != Availability::Available
            || port.direction != direction
            || port.profile != route.profile
            || port.interoperability_mode != InteroperabilityMode::AdapterManaged
            || adapter.state != AdapterState::Ready
            || !adapter
                .supported_route_modes
                .contains(&RouteBackend::AdapterManaged)
            || !adapter.owned_capabilities.contains(&capability.id)
            || port.formats != [FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)]
            || port.qos_modes != std::collections::BTreeSet::from([QosMode::Interactive])
        {
            return Err(error("catalog no longer supports exact secure stereo"));
        }
        Ok((capability.clone(), adapter.clone()))
    };
    let (source, source_adapter) = get(route.source, PortDirection::Source)?;
    let (sink, sink_adapter) = get(route.sink, PortDirection::Sink)?;
    Ok((source, source_adapter, sink, sink_adapter))
}
fn lease_ttl(end: u64, now: u64) -> io::Result<u64> {
    end.checked_sub(now)
        .and_then(|ms| ms.checked_sub(MAX_STALL.as_millis() as u64))
        .filter(|ttl| *ttl >= capyio_protocol::MIN_PORT_LEASE_TTL_MS && *ttl <= 60_000)
        .ok_or_else(|| error("insufficient stereo lease negotiation headroom"))
}
fn validate_source_claim(
    runtime: &NodeRuntime,
    route: &Route,
    original: Option<PortLeaseClaim>,
    now: u64,
) -> io::Result<()> {
    if runtime
        .route_port_lease_claim(route.id, route.source)
        .copied()
        != original
        || original.is_some_and(|claim| {
            claim.port != route.source
                || claim.route_id != route.id
                || claim.session_id != route.session_id
                || claim.expires_at_ms <= now
                || claim.expires_at_ms > expiry(route).unwrap_or(0)
        })
    {
        return Err(error("original stereo Source claim changed or expired"));
    }
    Ok(())
}
fn validate_sink_response(
    route: &Route,
    request: MessageId,
    response: ValidatedPortLeaseResponse,
    now: u64,
) -> io::Result<u64> {
    if response.request_message_id != request
        || response.session_id != route.session_id
        || response.route_id != route.id
        || response.port != route.sink
        || response.outcome != PortLeaseOutcome::Acquired
    {
        return Err(error("unexpected stereo Sink lease response"));
    }
    response
        .expires_at_ms
        .filter(|end| *end > now && *end <= expiry(route).unwrap_or(0))
        .ok_or_else(|| error("invalid stereo Sink claim expiry"))
}
fn media_error(reason: ControlTlsError) -> io::Error {
    match reason {
        ControlTlsError::Io(reason) => reason,
        _ => error("stereo TLS failure"),
    }
}
fn closed_kind(kind: io::ErrorKind) -> bool {
    matches!(
        kind,
        io::ErrorKind::UnexpectedEof
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::BrokenPipe
    )
}
fn media_cause(reason: &io::Error) -> &'static str {
    if closed_kind(reason.kind()) {
        "media_closed"
    } else if matches!(
        reason.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    ) {
        "media_timeout"
    } else {
        "media_failure"
    }
}
fn control_cause(reason: &CatalogLinkError) -> &'static str {
    match reason {
        CatalogLinkError::Closed => "control_closed",
        CatalogLinkError::Transport(ControlTlsError::Io(reason)) if closed_kind(reason.kind()) => {
            "control_closed"
        }
        _ => "control_failure",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (NodeRuntime, Route, ControlTlsBinding) {
        let mut local = capyio_testkit::windows_node();
        local.platform = Platform::Linux;
        let mut peer = capyio_testkit::android_node();
        for node in [&mut local, &mut peer] {
            for adapter in node.adapter_instances.values_mut() {
                adapter.state = AdapterState::Ready;
                adapter
                    .supported_route_modes
                    .insert(RouteBackend::AdapterManaged);
            }
            for capability in node.capabilities.values_mut() {
                capability.availability = Availability::Available;
                for port in capability.ports.values_mut() {
                    port.availability = Availability::Available;
                    port.interoperability_mode = InteroperabilityMode::AdapterManaged;
                    if port.profile == ProfileId::audio_frames_v1() {
                        port.formats = vec![FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)];
                        port.qos_modes = std::collections::BTreeSet::from([QosMode::Interactive]);
                    }
                }
            }
        }
        let endpoint = |node: &NodeDescriptor, class: CapabilityClass, direction| {
            let capability = node
                .capabilities
                .values()
                .find(|c| c.class == class)
                .unwrap();
            let port = capability
                .ports
                .values()
                .find(|p| p.direction == direction)
                .unwrap();
            PortRef {
                node_id: node.id,
                capability_id: capability.id,
                port_id: port.id,
            }
        };
        let source = endpoint(
            &local,
            CapabilityClass::Custom("system_audio_capture".into()),
            PortDirection::Source,
        );
        let sink = endpoint(&peer, CapabilityClass::Speaker, PortDirection::Sink);
        let mut runtime = NodeRuntime::new(local).unwrap();
        runtime.register_peer(peer, true).unwrap();
        let session = runtime.open_session(sink.node_id).unwrap();
        let id = runtime
            .create_route(session, source, sink, RouteBackend::AdapterManaged)
            .unwrap();
        runtime.authorize_route(id, Some(11_000)).unwrap();
        runtime
            .set_local_port_lease_policy(source, PortLeasePolicy::Exclusive)
            .unwrap();
        runtime
            .prepare_route(
                id,
                Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)),
                QosMode::Interactive,
                1000,
            )
            .unwrap();
        let route = runtime.route(id).unwrap().clone();
        let binding = ControlTlsBinding::new(session, source.node_id, sink.node_id).unwrap();
        (runtime, route, binding)
    }
    #[test]
    fn pure_stereo_source_consent_requires_explicit_target_and_60s_profile() {
        let (_, route, binding) = fixture();
        let admission = SecureStereoSourceAdmission {
            expected: binding,
            route_id: route.id,
            stream_id: StreamId::new(),
            lifetime: Duration::from_secs(60),
            pipewire_target: "selected.synthetic.source".into(),
        };
        validate_admission(&admission).unwrap();
        for lifetime in [
            Duration::ZERO,
            Duration::from_millis(1001),
            Duration::from_secs(61),
            Duration::MAX,
        ] {
            let mut changed = admission.clone();
            changed.lifetime = lifetime;
            assert!(validate_admission(&changed).is_err());
        }
        for target in ["", "auto", "default", "--target=anything", "bad target"] {
            let mut changed = admission.clone();
            changed.pipewire_target = target.into();
            assert!(validate_admission(&changed).is_err(), "{target}");
        }
    }
    #[test]
    fn pure_stereo_source_freezes_route_profile_generation_qos_and_control_roles() {
        let (runtime, route, binding) = fixture();
        validate_prepared(&runtime, &route, binding, 1000).unwrap();
        let reversed = ControlTlsBinding::new(
            binding.session_id,
            binding.client_node_id,
            binding.server_node_id,
        )
        .unwrap();
        validate_prepared(&runtime, &route, reversed, 1000).unwrap();
        for change in [
            (|route: &mut Route| route.epoch += 1) as fn(&mut Route),
            |route| route.state = RouteState::Active,
            |route| route.source.port_id = PortId::new(),
            |route| route.sink.port_id = PortId::new(),
            |route| route.selected_qos_mode = Some(QosMode::Basic),
            |route| {
                route.selected_format = Some(FormatDescriptor::new(
                    capyio_protocol::SECURE_AUDIO_MONO_FORMAT,
                ))
            },
            |route| {
                route
                    .selected_format
                    .as_mut()
                    .unwrap()
                    .parameters
                    .insert("extra".into(), "1".into());
            },
            |route| {
                route.authorization = AuthorizationState::Authorized {
                    expires_at_ms: Some(12_000),
                }
            },
        ] {
            let mut altered = route.clone();
            change(&mut altered);
            assert!(validate_current(&runtime, &altered, binding, 1000).is_err());
        }
        assert!(validate_current(&runtime, &route, binding, 11_000).is_err());
        let wrong =
            ControlTlsBinding::new(binding.session_id, NodeId::new(), binding.client_node_id)
                .unwrap();
        assert!(validate_current(&runtime, &route, wrong, 1000).is_err());
    }
    #[test]
    fn pure_stereo_source_owned_start_is_the_only_admitted_route_transition() {
        let (mut runtime, prepared, binding) = fixture();
        runtime
            .acquire_route_port_lease(prepared.id, prepared.source, 5000, 1000)
            .unwrap();
        runtime.begin_route_start(prepared.id, 1000).unwrap();
        assert!(validate_current(&runtime, &prepared, binding, 1000).is_err());
        let starting = runtime.route(prepared.id).unwrap().clone();
        validate_current(&runtime, &starting, binding, 1000).unwrap();
        runtime.activate_route(prepared.id).unwrap();
        assert!(validate_current(&runtime, &starting, binding, 1000).is_err());
    }
    #[test]
    fn pure_stereo_source_claim_cannot_be_acquired_twice_renewed_released_or_replaced() {
        for mutation in 0..5 {
            let (mut runtime, route, _) = fixture();
            validate_source_claim(&runtime, &route, None, 1000).unwrap();
            runtime
                .acquire_route_port_lease(route.id, route.source, 5000, 1000)
                .unwrap();
            assert!(validate_source_claim(&runtime, &route, None, 1000).is_err());
            let claim = *runtime
                .route_port_lease_claim(route.id, route.source)
                .unwrap();
            validate_source_claim(&runtime, &route, Some(claim), 1000).unwrap();
            match mutation {
                0 => {
                    runtime
                        .release_route_port_lease(route.id, route.source, route.session_id)
                        .unwrap();
                }
                1 => {
                    runtime
                        .acquire_route_port_lease(route.id, route.source, 6000, 1001)
                        .unwrap();
                }
                2 => {
                    runtime
                        .acquire_route_port_lease(route.id, route.source, 4000, 1001)
                        .unwrap();
                }
                3 => {
                    runtime.close_session(route.session_id).unwrap();
                }
                _ => {
                    assert!(validate_source_claim(&runtime, &route, Some(claim), 5000).is_err());
                    continue;
                }
            }
            assert!(validate_source_claim(&runtime, &route, Some(claim), 1001).is_err());
        }
    }
    #[test]
    fn pure_stereo_sink_claim_rejects_wrong_request_fields_renewal_and_expiry() {
        let (_, route, _) = fixture();
        let response = ValidatedPortLeaseResponse {
            message_id: MessageId::new(),
            request_message_id: MessageId::new(),
            session_id: route.session_id,
            route_id: route.id,
            port: route.sink,
            outcome: PortLeaseOutcome::Acquired,
            expires_at_ms: Some(5000),
        };
        validate_sink_response(&route, response.request_message_id, response, 1000).unwrap();
        for change in [
            (|c: &mut ValidatedPortLeaseResponse| c.request_message_id = MessageId::new())
                as fn(&mut ValidatedPortLeaseResponse),
            |c| c.session_id = SessionId::new(),
            |c| c.route_id = RouteId::new(),
            |c| c.port.port_id = PortId::new(),
            |c| c.outcome = PortLeaseOutcome::Renewed,
            |c| c.expires_at_ms = None,
            |c| c.expires_at_ms = Some(1000),
            |c| c.expires_at_ms = Some(12_000),
        ] {
            let mut altered = response;
            change(&mut altered);
            assert!(
                validate_sink_response(&route, response.request_message_id, altered, 1000).is_err()
            );
        }
    }
    #[test]
    fn pure_stereo_source_catalog_pins_metadata_and_adapter() {
        let (mut runtime, route, _) = fixture();
        let before = catalogs(&runtime, &route).unwrap();
        let mut peer = runtime.snapshot().peers[0].clone();
        peer.capabilities
            .get_mut(&route.sink.capability_id)
            .unwrap()
            .metadata
            .insert("revision".into(), "2".into());
        runtime.accept_peer_catalog(route.session_id, peer).unwrap();
        assert_ne!(catalogs(&runtime, &route).unwrap(), before);
    }
    #[test]
    fn pure_stereo_source_lease_headroom_never_renews_original_clock() {
        assert_eq!(lease_ttl(11_000, 1000).unwrap(), 9750);
        assert_eq!(lease_ttl(11_000, 1250).unwrap(), 9500);
        for now in [11_000, 11_001, u64::MAX] {
            assert!(lease_ttl(11_000, now).is_err());
        }
        assert!(lease_ttl(u64::MAX, 0).is_err());
    }
    #[test]
    fn pure_stereo_source_stop_before_registration_is_latched() {
        let stop = SecureStereoSourceStopHandle::default();
        let clone = stop.clone();
        stop.stop();
        assert!(clone.stopped.load(Ordering::Acquire));
        assert!(clone.media.lock().unwrap().is_none());
    }
}
