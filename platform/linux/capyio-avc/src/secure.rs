//! Finite single-peer camera session. No pairing, enrollment or default listener.
use crate::{AvcProfile, FINITE_STREAM_FORMAT, MAX_LIFETIME, RuntimeAvcCamera};
use capyio_control_link::{CatalogControlLink, ReceivedControlMessage};
use capyio_control_tls::{
    ControlTlsBinding, ControlTlsServerIdentity,
    camera::{CameraTlsBinding, CameraTlsListener, CameraTlsStopHandle},
};
use capyio_core::{
    AuthorizationState, FormatDescriptor, MessageId, Route, RouteId, RouteState, SessionState,
    StreamId,
};
use capyio_linux_camera::{CameraBinding, SubmitOutcome};
use capyio_protocol::{
    PortLeaseAction, PortLeaseOutcome, RouteCommandAction, SecureCameraDataOffer,
    SecureCameraTransport,
};
use capyio_runtime::NodeRuntime;
use std::{
    io,
    net::{IpAddr, TcpListener},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
fn error(e: impl ToString) -> io::Error {
    io::Error::other(e.to_string())
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

/// Trusted local admission, not a peer-deserializable grant.
#[derive(Clone, Copy)]
pub struct SecureCameraAdmission {
    pub expected: ControlTlsBinding,
    pub route_id: RouteId,
    pub stream_id: StreamId,
    pub local_address: IpAddr,
    pub lifetime: Duration,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecureCameraEvent {
    Listening,
    Bound,
    Record,
    Stopped,
}
enum DataEvent {
    Bound,
    Record(Vec<u8>),
    Failed(io::Error),
}

/// Caller must continuously poll. A single transport thread keeps partial TLS
/// reads off the control owner; rendezvous admits only one complete record.
/// Decoder IPC supervises control too, so Stop/EOF cannot wait behind decoding.
pub struct SecureCameraOwner {
    runtime: Option<NodeRuntime>,
    camera: Option<RuntimeAvcCamera>,
    link: CatalogControlLink,
    admission: SecureCameraAdmission,
    original: Route,
    profile: AvcProfile,
    identity: Option<ControlTlsServerIdentity>,
    source_request: MessageId,
    source_request_deadline: Instant,
    source_expiry: Option<u64>,
    sink_expiry: u64,
    started: Instant,
    last_wall: u64,
    stop: Option<CameraTlsStopHandle>,
    worker: Option<JoinHandle<()>>,
    events: Option<Receiver<DataEvent>>,
    closed: bool,
}
impl SecureCameraOwner {
    /// Consumes an already authenticated, admitted exact Ready Session and a
    /// locally consented Starting Route. Authentication never manufactures consent.
    pub fn new(
        mut runtime: NodeRuntime,
        mut link: CatalogControlLink,
        identity: ControlTlsServerIdentity,
        admission: SecureCameraAdmission,
    ) -> io::Result<Self> {
        // Negotiation and TLS binding consume the same one-shot lifetime as
        // decoding. Neither handshake completion nor a profile budget renews it.
        let started = Instant::now();
        let now = now_ms()?;
        let original = runtime.route(admission.route_id).map_err(error)?.clone();
        let profile = camera_profile(&original)?;
        let sink_expiry = validate(&runtime, &original, admission.expected, now)?;
        if link.tls_binding() != admission.expected
            || link.peer_node_id() != original.source.node_id
            || link.accepted_peer_addr().is_err()
            || admission.lifetime.is_zero()
            || admission.lifetime > MAX_LIFETIME
            || admission.stream_id.as_uuid().is_nil()
            || admission.local_address.is_multicast()
        {
            return Err(error("invalid secure camera admission"));
        }
        let grant = expiry(&original)?;
        let deadline = (started + admission.lifetime)
            .min(started + Duration::from_millis((grant.min(sink_expiry) - now).min(1000)));
        // Lease TTL is relative to the Source's receipt time, not this clock.
        // Leave the bounded control budget inside all finite local deadlines.
        let source_ttl = source_lease_ttl(grant.min(sink_expiry), now, admission.lifetime)?;
        let source_request = match link.send_port_lease_request_until(
            original.id,
            original.source,
            PortLeaseAction::Acquire,
            source_ttl,
            deadline,
        ) {
            Ok(id) => id,
            Err(e) => {
                let _ = link.close_and_reconcile(&mut runtime);
                return Err(error(e));
            }
        };
        Ok(Self {
            runtime: Some(runtime),
            camera: None,
            link,
            admission,
            original,
            profile,
            identity: Some(identity),
            source_request,
            source_request_deadline: deadline,
            source_expiry: None,
            sink_expiry,
            started,
            last_wall: now,
            stop: None,
            worker: None,
            events: None,
            closed: false,
        })
    }
    pub fn runtime(&self) -> &NodeRuntime {
        self.camera
            .as_ref()
            .map(|c| c.runtime())
            .unwrap_or_else(|| self.runtime.as_ref().expect("owned runtime"))
    }
    pub fn runtime_mut(&mut self) -> &mut NodeRuntime {
        if let Some(c) = &mut self.camera {
            c.runtime_mut()
        } else {
            self.runtime.as_mut().expect("owned runtime")
        }
    }
    /// Actual authenticated lease deadline, which may precede the consent grant.
    pub fn source_lease_expires_at_ms(&self) -> Option<u64> {
        self.source_expiry
    }
    pub fn metrics(&self) -> Option<capyio_linux_camera::CameraMetrics> {
        self.camera.as_ref().map(|c| c.metrics())
    }
    pub fn helper_pid(&self) -> Option<u32> {
        self.camera.as_ref().and_then(|c| c.helper_pid())
    }
    pub fn decoder_pid(&self) -> Option<u32> {
        self.camera.as_ref().and_then(|c| c.decoder_pid())
    }
    pub fn node_name(&self) -> Option<&str> {
        self.camera.as_ref().and_then(|c| c.node_name())
    }
    pub fn poll(&mut self) -> io::Result<Option<SecureCameraEvent>> {
        let result = self.poll_inner();
        if result.is_err() {
            let _ = self.stop();
        }
        result
    }
    fn poll_inner(&mut self) -> io::Result<Option<SecureCameraEvent>> {
        if self.closed {
            return Err(error("secure camera closed"));
        }
        let now = now_ms()?;
        self.check(now)?;
        let deadline = self.deadline(now);
        if let Some(message) = self
            .link
            .poll_control_message_until(deadline)
            .map_err(error)?
        {
            if let ReceivedControlMessage::PortLeaseResponse(response) = message {
                let now = now_ms()?;
                self.check(now)?; // response completion must still fit the fixed negotiation budget
                if self.source_expiry.is_some()
                    || response.request_message_id != self.source_request
                    || response.session_id != self.original.session_id
                    || response.route_id != self.original.id
                    || response.port != self.original.source
                    || response.outcome != PortLeaseOutcome::Acquired
                {
                    return Err(error("unexpected camera Source lease response"));
                }
                let lease = response
                    .expires_at_ms
                    .filter(|e| *e > now && *e <= expiry(&self.original).unwrap_or(0))
                    .ok_or_else(|| error("invalid camera Source lease expiry"))?;
                self.source_expiry = Some(lease);
                self.listen(now)?;
                return Ok(Some(SecureCameraEvent::Listening));
            }
            let runtime = if let Some(c) = &mut self.camera {
                c.runtime_mut()
            } else {
                self.runtime.as_mut().expect("runtime")
            };
            handle(&mut self.link, runtime, &self.original, message, deadline)?;
        }
        self.check(now_ms()?)?;
        if let Some(camera) = &mut self.camera {
            camera.poll()?;
        }
        let event = match self.events.as_ref().map(|r| r.try_recv()) {
            None | Some(Err(TryRecvError::Empty)) => return Ok(None),
            Some(Err(TryRecvError::Disconnected)) => return Err(error("camera transport ended")),
            Some(Ok(event)) => event,
        };
        match event {
            DataEvent::Failed(e) => Err(e),
            DataEvent::Bound => {
                self.check(now_ms()?)?;
                let duration = self.deadline_lifetime()?;
                let binding = CameraBinding {
                    session_id: self.original.session_id,
                    route_id: self.original.id,
                    stream_id: self.admission.stream_id,
                    stream_epoch: self.original.epoch,
                    local_sink: self.original.sink,
                };
                self.camera = Some(RuntimeAvcCamera::start_secure_profile(
                    self.runtime.take().expect("runtime"),
                    binding,
                    duration,
                    self.profile,
                )?);
                // Helper startup also consumes the already pinned authority.
                // A completed spawn is not permission to outlive the handshake.
                self.check(now_ms()?)?;
                Ok(Some(SecureCameraEvent::Bound))
            }
            DataEvent::Record(bytes) => {
                let camera = self
                    .camera
                    .as_mut()
                    .ok_or_else(|| error("record before camera binding"))?;
                let link = &mut self.link;
                let original = &self.original;
                let authority_expiry = self
                    .source_expiry
                    .expect("leased")
                    .min(self.sink_expiry)
                    .min(expiry(original)?);
                let end = self.started + self.admission.lifetime;
                let mut last = self.last_wall;
                let outcome = camera.submit_record_supervised(&bytes, |runtime| {
                    let now = now_ms()?;
                    if now < last || now >= authority_expiry || Instant::now() >= end {
                        return Err(error("camera authority expired"));
                    }
                    last = now;
                    let sink = validate(runtime, original, link.tls_binding(), now)?;
                    let deadline = end.min(
                        Instant::now()
                            + Duration::from_millis((authority_expiry.min(sink) - now).min(1000)),
                    );
                    if let Some(message) =
                        link.poll_control_message_until(deadline).map_err(error)?
                    {
                        handle(link, runtime, original, message, deadline)?;
                    }
                    let fresh = now_ms()?;
                    if fresh < last
                        || fresh >= authority_expiry
                        || Instant::now() >= end
                        || link.is_closed()
                    {
                        return Err(error("camera authority expired during control handling"));
                    }
                    last = fresh;
                    validate(runtime, original, link.tls_binding(), fresh)?;
                    Ok(())
                })?;
                self.last_wall = last;
                if outcome == Some(SubmitOutcome::DroppedBackpressure) {
                    return Err(error("secure camera projection backpressure"));
                }
                // EOS has stopped the decoder. No reconnect or subsequent record.
                if camera.decoder_pid().is_none() {
                    self.stop()?;
                    return Ok(Some(SecureCameraEvent::Stopped));
                }
                Ok(Some(SecureCameraEvent::Record))
            }
        }
    }
    fn check(&mut self, now: u64) -> io::Result<()> {
        if self.source_expiry.is_none() && Instant::now() >= self.source_request_deadline {
            return Err(error("camera Source lease negotiation deadline"));
        }
        if now < self.last_wall
            || self.started.elapsed() >= self.admission.lifetime
            || self.link.is_closed()
            || now >= self.sink_expiry
            || self.source_expiry.is_some_and(|e| now >= e)
        {
            return Err(error("secure camera authority expired"));
        }
        validate(self.runtime(), &self.original, self.admission.expected, now)?;
        self.last_wall = now;
        Ok(())
    }
    fn deadline(&self, now: u64) -> Instant {
        let wall = expiry(&self.original)
            .unwrap_or(now)
            .min(self.sink_expiry)
            .min(self.source_expiry.unwrap_or(u64::MAX));
        let deadline = (self.started + self.admission.lifetime)
            .min(Instant::now() + Duration::from_millis(wall.saturating_sub(now).min(1000)));
        if self.source_expiry.is_none() {
            deadline.min(self.source_request_deadline)
        } else {
            deadline
        }
    }
    fn deadline_lifetime(&self) -> io::Result<Duration> {
        remaining_authority_lifetime(
            self.started + self.admission.lifetime,
            self.source_expiry
                .unwrap_or(0)
                .min(self.sink_expiry)
                .min(expiry(&self.original)?),
            now_ms()?,
            Instant::now(),
        )
    }
    fn listen(&mut self, now: u64) -> io::Result<()> {
        self.check(now)?;
        let binding = CameraTlsBinding::from_route(
            self.admission.expected,
            &self.original,
            self.admission.stream_id,
            now,
        )
        .map_err(error)?;
        let listener = CameraTlsListener::from_listener_until(
            TcpListener::bind((self.admission.local_address, 0))?,
            self.identity
                .take()
                .ok_or_else(|| error("identity already consumed"))?,
            binding,
            Duration::from_secs(5),
            Instant::now() + self.deadline_lifetime()?,
        )
        .map_err(error)?;
        let offer = SecureCameraDataOffer {
            port: listener.local_addr().map_err(error)?.port(),
            stream_id: self.admission.stream_id,
            transport: SecureCameraTransport::Tls13MtlsV1,
        };
        self.stop = Some(listener.stop_handle().map_err(error)?);
        self.link
            .send_secure_camera_data_offer_until(&self.original, offer, now, self.deadline(now))
            .map_err(error)?;
        let (tx, rx) = mpsc::sync_channel(0);
        self.events = Some(rx);
        let end = self.started + self.admission.lifetime;
        self.worker = Some(thread::spawn(move || {
            let result = (|| -> io::Result<()> {
                let mut channel = loop {
                    if Instant::now() >= end {
                        return Err(error("camera accept deadline"));
                    }
                    if let Some(channel) = listener.try_accept().map_err(error)? {
                        break channel;
                    }
                    thread::sleep(Duration::from_millis(2));
                };
                if tx.send(DataEvent::Bound).is_err() {
                    return Ok(());
                }
                loop {
                    let record = channel.receive_record().map_err(error)?;
                    if tx.send(DataEvent::Record(record)).is_err() {
                        return Ok(());
                    }
                }
            })();
            if let Err(e) = result {
                let _ = tx.send(DataEvent::Failed(e));
            }
        }));
        Ok(())
    }
    pub fn stop(&mut self) -> io::Result<()> {
        self.closed = true;
        if let Some(stop) = self.stop.take() {
            stop.stop();
        }
        self.events.take(); // releases a worker waiting to hand off a complete record
        let runtime = if let Some(c) = &mut self.camera {
            Some(c.runtime_mut())
        } else {
            self.runtime.as_mut()
        };
        let control = if let Some(runtime) = runtime {
            self.link.close_and_reconcile(runtime).map_err(error)
        } else {
            self.link.close();
            Ok(())
        };
        let media = self.camera.as_mut().map(|c| c.stop()).unwrap_or(Ok(()));
        let worker = if let Some(worker) = &self.worker {
            let deadline = Instant::now() + Duration::from_secs(1);
            while !worker.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(2));
            }
            if !worker.is_finished() {
                Err(error(
                    "camera transport join deadline; cancellation remains requested",
                ))
            } else {
                self.worker
                    .take()
                    .expect("worker")
                    .join()
                    .map_err(|_| error("camera transport panicked"))
            }
        } else {
            Ok(())
        };
        control.and(media).and(worker)
    }
}
impl Drop for SecureCameraOwner {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
// The frame budget may round up; time authority must not. Compute this only
// from the original monotonic end and pinned absolute grant/lease deadlines.
fn remaining_authority_lifetime(
    end: Instant,
    wall_deadline: u64,
    now_ms: u64,
    now: Instant,
) -> io::Result<Duration> {
    let remaining = end
        .saturating_duration_since(now)
        .min(Duration::from_millis(wall_deadline.saturating_sub(now_ms)));
    if remaining.as_millis() == 0 {
        return Err(error("camera authority elapsed before decoder start"));
    }
    Ok(remaining)
}

fn source_lease_ttl(deadline: u64, now: u64, lifetime: Duration) -> io::Result<u64> {
    deadline
        .checked_sub(now)
        .map(|remaining| remaining.min(lifetime.as_millis() as u64))
        .and_then(|remaining| remaining.checked_sub(1000))
        .filter(|ttl| *ttl >= capyio_protocol::MIN_PORT_LEASE_TTL_MS)
        .ok_or_else(|| error("insufficient finite camera lease negotiation headroom"))
}

fn expiry(route: &Route) -> io::Result<u64> {
    match route.authorization {
        AuthorizationState::Authorized {
            expires_at_ms: Some(e),
        } => Ok(e),
        _ => Err(error("finite camera consent required")),
    }
}
fn camera_profile(route: &Route) -> io::Result<AvcProfile> {
    match route.selected_format.as_ref() {
        Some(format) if format == &FormatDescriptor::new("cavc-avc-access-unit-v1") => {
            Ok(AvcProfile::Legacy)
        }
        Some(format) if format == &FormatDescriptor::new(FINITE_STREAM_FORMAT) => {
            Ok(AvcProfile::Finite720p30)
        }
        _ => Err(error(
            "secure camera requires an exact negotiated AVC profile",
        )),
    }
}

fn validate(
    runtime: &NodeRuntime,
    original: &Route,
    binding: ControlTlsBinding,
    now: u64,
) -> io::Result<u64> {
    // Determine the profile from the immutable original, then require the live
    // Route to remain identical. Never accept either supported format on polls.
    let profile = camera_profile(original)?;
    let route = runtime.route(original.id).map_err(error)?;
    let session = runtime.session(binding.session_id).map_err(error)?;
    if route != original
        || route.session_id != binding.session_id
        || session.state != SessionState::Ready
        || session.local_node_id != binding.server_node_id
        || session.remote_node_id != binding.client_node_id
        || route.sink.node_id != session.local_node_id
        || route.source.node_id != session.remote_node_id
        || route.state != RouteState::Starting
        || expiry(route)? <= now
    {
        return Err(error("secure camera Runtime binding changed"));
    }
    SecureCameraDataOffer {
        port: 1,
        stream_id: StreamId::new(),
        transport: SecureCameraTransport::Tls13MtlsV1,
    }
    .validate(route)
    .map_err(error)?;
    let lease = runtime
        .route_port_lease_claim(route.id, route.sink)
        .ok_or_else(|| error("camera Sink lease absent"))?;
    if lease.session_id != route.session_id
        || lease.expires_at_ms <= now
        || lease.expires_at_ms > expiry(route)?
    {
        return Err(error("camera Sink lease invalid"));
    }
    let binding = CameraBinding {
        session_id: route.session_id,
        route_id: route.id,
        stream_id: StreamId::new(),
        stream_epoch: route.epoch,
        local_sink: route.sink,
    };
    match profile {
        AvcProfile::Legacy => {
            capyio_linux_camera::validate_decoded_avc_authority(runtime, binding, now)?;
        }
        AvcProfile::Finite720p30 => {
            capyio_linux_camera::validate_decoded_avc_finite_authority(runtime, binding, now)?;
        }
    }
    Ok(lease.expires_at_ms)
}
fn validate_route_status(
    original: &Route,
    status: &capyio_protocol::ValidatedRouteStatus,
) -> io::Result<()> {
    if status.session_id != original.session_id
        || status.route != *original
        || status.secure_camera_data.is_some()
        || status.camera_data.is_some()
        || status.native_audio_data.is_some()
        || status.touchpad_data.is_some()
        || status.haptics_data.is_some()
    {
        return Err(error(
            "camera status changed the original Route or data offer",
        ));
    }
    Ok(())
}

fn handle(
    link: &mut CatalogControlLink,
    runtime: &mut NodeRuntime,
    original: &Route,
    message: ReceivedControlMessage,
    deadline: Instant,
) -> io::Result<()> {
    match message {
        ReceivedControlMessage::Heartbeat(h) => {
            if !h.acknowledgement {
                link.send_heartbeat_until(h.sequence, true, deadline)
                    .map_err(error)?;
            }
        }
        ReceivedControlMessage::Catalog(c) => {
            link.apply_catalog(runtime, c).map_err(error)?;
        }
        ReceivedControlMessage::RouteCommand(c)
            if c.session_id == original.session_id
                && c.route_id == original.id
                && c.action == RouteCommandAction::Stop =>
        {
            return Err(error("peer stopped camera"));
        }
        ReceivedControlMessage::RouteStatus(status) => {
            validate_route_status(original, &status)?;
        }
        _ => return Err(error("unexpected dedicated camera control message")),
    }
    validate(runtime, original, link.tls_binding(), now_ms()?)?;
    Ok(())
}

#[cfg(test)]
mod authority_tests {
    use super::*;
    use capyio_core::{InteroperabilityMode, NodeId, Platform, QosMode, RouteBackend, SessionId};
    use capyio_runtime::PortLeasePolicy;
    use capyio_video::VideoStreamSpec;
    fn fixture(encoded_avc: bool) -> (NodeRuntime, CameraBinding) {
        let format = if encoded_avc {
            capyio_core::FormatDescriptor::new("cavc-avc-access-unit-v1")
        } else {
            VideoStreamSpec::camera_720p30_nv12().format_descriptor()
        };
        fixture_format(format)
    }
    fn fixture_format(format: FormatDescriptor) -> (NodeRuntime, CameraBinding) {
        let lab = capyio_testkit::DemoLab::new().unwrap();
        let original = lab
            .runtime
            .route(lab.routes.phone_camera_to_panel)
            .unwrap()
            .clone();
        let snapshot = lab.runtime.snapshot();
        let mut local = snapshot.local_node;
        local.platform = Platform::Linux;
        let mut peer = snapshot.peers[0].clone();
        for node in [&mut local, &mut peer] {
            for adapter in node.adapter_instances.values_mut() {
                adapter
                    .supported_route_modes
                    .insert(RouteBackend::AdapterManaged);
            }
            for cap in node.capabilities.values_mut() {
                cap.availability = capyio_core::Availability::Available;
                for port in cap.ports.values_mut() {
                    port.availability = capyio_core::Availability::Available;
                    port.interoperability_mode = InteroperabilityMode::AdapterManaged;
                    if port.profile == capyio_video::video_frames_profile() {
                        port.formats = vec![format.clone()];
                        port.qos_modes.insert(QosMode::Interactive);
                    }
                }
            }
        }
        let peer_id = peer.id;
        let mut runtime = NodeRuntime::new(local).unwrap();
        runtime.register_peer(peer, true).unwrap();
        let session_id = runtime.open_session(peer_id).unwrap();
        let route_id = runtime
            .create_route(
                session_id,
                original.source,
                original.sink,
                RouteBackend::AdapterManaged,
            )
            .unwrap();
        runtime
            .set_local_port_lease_policy(original.sink, PortLeasePolicy::Exclusive)
            .unwrap();
        runtime.authorize_route(route_id, Some(10_000)).unwrap();
        runtime
            .acquire_route_port_lease(route_id, original.sink, 5_000, 1_000)
            .unwrap();
        runtime
            .prepare_route(route_id, Some(format), QosMode::Interactive, 1_000)
            .unwrap();
        runtime.begin_route_start(route_id, 1_000).unwrap();
        (
            runtime,
            CameraBinding {
                session_id,
                route_id,
                stream_id: StreamId::new(),
                stream_epoch: 1,
                local_sink: original.sink,
            },
        )
    }

    fn admitted() -> (NodeRuntime, Route, ControlTlsBinding) {
        let (runtime, binding) = fixture(true);
        let route = runtime.route(binding.route_id).unwrap().clone();
        let tls =
            ControlTlsBinding::new(route.session_id, route.sink.node_id, route.source.node_id)
                .unwrap();
        (runtime, route, tls)
    }
    #[test]
    fn only_exact_ready_cavc_route_and_live_sink_claim_are_admitted() {
        let (runtime, route, tls) = admitted();
        assert_eq!(validate(&runtime, &route, tls, 1000).unwrap(), 5000);
        assert!(validate(&runtime, &route, tls, 5000).is_err());
        assert!(validate(&runtime, &route, tls, 10000).is_err());
        for field in 0..3 {
            let mut wrong = tls;
            match field {
                0 => wrong.session_id = SessionId::new(),
                1 => wrong.server_node_id = NodeId::new(),
                _ => wrong.client_node_id = NodeId::new(),
            }
            assert!(validate(&runtime, &route, wrong, 1000).is_err());
        }
        for field in 0..4 {
            let mut wrong = route.clone();
            match field {
                0 => wrong.epoch += 1,
                1 => wrong.source.port_id = capyio_core::PortId::new(),
                2 => wrong.sink.capability_id = capyio_core::CapabilityId::new(),
                _ => {
                    wrong.authorization = AuthorizationState::Authorized {
                        expires_at_ms: Some(9999),
                    }
                }
            }
            assert!(validate(&runtime, &wrong, tls, 1000).is_err());
        }
    }
    #[test]
    fn lost_lease_stopped_route_closed_session_or_offline_peer_fail_admission() {
        for change in 0..4 {
            let (mut runtime, route, tls) = admitted();
            match change {
                0 => {
                    runtime
                        .release_route_port_lease(route.id, route.sink, route.session_id)
                        .unwrap();
                }
                1 => {
                    runtime.begin_route_stop(route.id).unwrap();
                    runtime.stop_route(route.id).unwrap();
                }
                2 => {
                    runtime.close_session(route.session_id).unwrap();
                }
                _ => {
                    runtime
                        .set_peer_online(route.source.node_id, false)
                        .unwrap();
                }
            }
            assert!(validate(&runtime, &route, tls, 1000).is_err());
        }
    }
    #[test]
    fn finite_profile_admits_only_its_original_exact_format() {
        let (runtime, binding) = fixture_format(FormatDescriptor::new(FINITE_STREAM_FORMAT));
        let original = runtime.route(binding.route_id).unwrap();
        let tls = ControlTlsBinding::new(
            original.session_id,
            original.sink.node_id,
            original.source.node_id,
        )
        .unwrap();
        assert_eq!(camera_profile(original).unwrap(), AvcProfile::Finite720p30);
        assert_eq!(validate(&runtime, original, tls, 1000).unwrap(), 5000);
        for format in [
            FormatDescriptor::new("cavc-avc-access-unit-v1"),
            FormatDescriptor::new("cavc-avc-finite-720p30-v2"),
        ] {
            let mut changed = original.clone();
            changed.selected_format = Some(format);
            assert!(validate(&runtime, &changed, tls, 1000).is_err());
        }
        for id in ["cavc-avc-access-unit-v1", FINITE_STREAM_FORMAT] {
            let mut changed = original.clone();
            let mut format = FormatDescriptor::new(id);
            format.parameters.insert("max_frames".into(), "1800".into());
            changed.selected_format = Some(format);
            assert!(camera_profile(&changed).is_err());
            assert!(validate(&runtime, &changed, tls, 1000).is_err());
        }
    }

    #[test]
    fn incoming_status_cannot_switch_between_valid_profiles() {
        for (original_id, changed_id) in [
            ("cavc-avc-access-unit-v1", FINITE_STREAM_FORMAT),
            (FINITE_STREAM_FORMAT, "cavc-avc-access-unit-v1"),
        ] {
            let (runtime, binding) = fixture_format(FormatDescriptor::new(original_id));
            let original = runtime.route(binding.route_id).unwrap();
            let bytes =
                capyio_protocol::encode_route_status(original.session_id, original).unwrap();
            let mut status = capyio_protocol::decode_route_status(&bytes).unwrap();
            assert!(validate_route_status(original, &status).is_ok());
            status.route.selected_format = Some(FormatDescriptor::new(changed_id));
            // Both are individually supported, but not interchangeable within
            // one owner. No generic two-format allowlist is used on later polls.
            assert!(camera_profile(&status.route).is_ok());
            assert!(validate_route_status(original, &status).is_err());
            status.route = original.clone();
            status
                .route
                .selected_format
                .as_mut()
                .unwrap()
                .parameters
                .insert("unknown".into(), "1".into());
            assert!(validate_route_status(original, &status).is_err());
        }
    }

    #[test]
    fn finite_owner_rejects_legacy_only_source_catalog() {
        let (mut runtime, binding) = fixture_format(FormatDescriptor::new(FINITE_STREAM_FORMAT));
        let original = runtime.route(binding.route_id).unwrap().clone();
        let tls = ControlTlsBinding::new(
            original.session_id,
            original.sink.node_id,
            original.source.node_id,
        )
        .unwrap();
        let mut peer = runtime
            .snapshot()
            .peers
            .into_iter()
            .find(|node| node.id == original.source.node_id)
            .unwrap();
        peer.capabilities
            .get_mut(&original.source.capability_id)
            .unwrap()
            .ports
            .get_mut(&original.source.port_id)
            .unwrap()
            .formats = vec![FormatDescriptor::new("cavc-avc-access-unit-v1")];
        runtime
            .accept_peer_catalog(original.session_id, peer)
            .unwrap();
        assert!(validate(&runtime, &original, tls, 1000).is_err());
    }

    #[test]
    fn raw_projection_format_is_not_a_secure_cavc_grant() {
        let (runtime, binding) = fixture(false);
        let route = runtime.route(binding.route_id).unwrap();
        let tls =
            ControlTlsBinding::new(route.session_id, route.sink.node_id, route.source.node_id)
                .unwrap();
        assert!(validate(&runtime, route, tls, 1000).is_err());
    }
}

#[cfg(test)]
mod lease_budget_tests {
    use super::*;
    #[test]
    fn post_handshake_lifetime_consumes_original_deadlines_without_rounding_up() {
        let start = Instant::now();
        let end = start + Duration::from_micros(34_500);
        let remaining = remaining_authority_lifetime(end, 5000, 1000, start).unwrap();
        assert_eq!(remaining, Duration::from_micros(34_500));
        assert_eq!(
            crate::FiniteStreamBudget::for_lifetime(remaining)
                .unwrap()
                .max_frames,
            2
        );
        assert!(remaining_authority_lifetime(end, 5000, 1000, end).is_err());
        assert!(remaining_authority_lifetime(end, 1000, 1000, start).is_err());
        assert!(remaining_authority_lifetime(end, 999, 1000, start).is_err());
        assert!(
            remaining_authority_lifetime(end, 5000, 1000, end - Duration::from_micros(999))
                .is_err()
        );
        assert_eq!(
            remaining_authority_lifetime(end, 1002, 1000, start).unwrap(),
            Duration::from_millis(2),
        );
        let after_handshake = start + Duration::from_millis(20);
        assert_eq!(
            remaining_authority_lifetime(end, 5000, 1020, after_handshake).unwrap(),
            Duration::from_micros(14_500),
        );
    }

    #[test]
    fn relative_source_ttl_leaves_receipt_budget_inside_all_authority() {
        assert_eq!(
            source_lease_ttl(5000, 1000, Duration::from_secs(60)).unwrap(),
            3000
        );
        assert_eq!(
            source_lease_ttl(10000, 1000, Duration::from_secs(4)).unwrap(),
            3000
        );
        for deadline in [999, 1000, 1999, 2999] {
            assert!(source_lease_ttl(deadline, 1000, Duration::from_secs(60)).is_err());
        }
        assert_eq!(
            source_lease_ttl(3000, 1000, Duration::from_secs(60)).unwrap(),
            1000
        );
    }
}
