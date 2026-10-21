//! Host-testable Android Source policy and Rust-owned pinned camera sender.
//! This is not Camera2 capture and never falls back to the plaintext lab socket.
use capyio_control_credentials::ControlCredentialBundle;
use capyio_control_link::CatalogControlLink;
use capyio_control_tls::{
    ControlTlsClientIdentity, ControlTlsEndpoint,
    camera::{CameraTlsBinding, CameraTlsChannel, CameraTlsStopHandle},
};
use capyio_core::*;
use capyio_pairing::PairedPeerDirectory;
use capyio_protocol::SecureCameraDataOffer;
use capyio_runtime::NodeRuntime;
use capyio_vcamdroid_adapter::{AvcRecordGuard, AvcStreamKey, decode_record};
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

/// The same Source authority check used by the Android JNI camera lifecycle.
/// A peer's offer is never consent and cannot mint or extend a local lease.
pub fn source_authority(runtime: &NodeRuntime, route: &Route, now: u64) -> Option<u64> {
    let AuthorizationState::Authorized {
        expires_at_ms: Some(expiry),
    } = route.authorization
    else {
        return None;
    };
    let snapshot = runtime.snapshot();
    let node = &snapshot.local_node;
    let cap = node.capabilities.get(&route.source.capability_id)?;
    let port = cap.ports.get(&route.source.port_id)?;
    let session = runtime.session(route.session_id).ok()?;
    let lease = runtime.route_port_lease_claim(route.id, route.source)?;
    // AndroidCapabilityModules publishes Busy while this camera lifecycle owns
    // capture. Busy is not a fresh grant: only this exact Starting/Active Route's
    // live Source lease below permits continuation.
    let available_to_owner = |availability| {
        availability == Availability::Available
            || (availability == Availability::Busy
                && matches!(route.state, RouteState::Starting | RouteState::Active))
    };
    (node.platform == Platform::Android
        && route.source.node_id == node.id
        && cap.class == CapabilityClass::Camera
        && available_to_owner(cap.availability)
        && available_to_owner(port.availability)
        && port.direction == PortDirection::Source
        && port.profile == ProfileId::video_frames_v1()
        && route.profile == port.profile
        && session.state == SessionState::Ready
        && session.local_node_id == node.id
        && session.remote_node_id == route.sink.node_id
        && expiry > now
        && lease.session_id == route.session_id
        && lease.expires_at_ms > now
        && lease.expires_at_ms <= expiry)
        .then_some(expiry)
}

/// Host-side access to the same pending/finite consent gate used by Android JNI.
/// `resolve` is called only for an explicit local user's consent decision.
#[derive(Default)]
pub struct CameraSourceConsent(crate::touchpad_consent::TouchpadConsentGate);
impl CameraSourceConsent {
    pub fn enqueue(
        &mut self,
        runtime: &mut NodeRuntime,
        route: RouteId,
        now: u64,
    ) -> Result<(), String> {
        self.0.enqueue(runtime, route, now)
    }
    pub fn resolve(
        &mut self,
        runtime: &mut NodeRuntime,
        route: RouteId,
        allow: bool,
        now: u64,
        peer_deadline: u64,
    ) -> Result<(), String> {
        self.0
            .resolve_bounded(runtime, route, allow, now, Some(peer_deadline))
    }
}

/// Synchronous bounded sender, owned by a Service worker, never an audio/camera callback.
/// Callers must stop its independently usable stop handle on revoke/Stop/control EOF.
/// Each record additionally rechecks the live Runtime before and after the write.
pub struct SecureCameraSender {
    channel: CameraTlsChannel,
    records: AvcRecordGuard,
    binding: CameraTlsBinding,
    lease_deadline: u64,
    last_ms: u64,
}
/// Immutable native-only preparation. It owns no socket; prepare under brief
/// Runtime/control locks and call connect only after releasing those locks.
#[derive(Clone)]
pub(crate) struct PreparedCameraSender {
    address: SocketAddr,
    identity: ControlTlsClientIdentity,
    server_name: String,
    pub(crate) binding: CameraTlsBinding,
    records: AvcRecordGuard,
    lease_deadline: u64,
    now: u64,
}
impl PreparedCameraSender {
    pub(crate) fn new(
        runtime: &NodeRuntime,
        link: &CatalogControlLink,
        route_id: RouteId,
        offer: SecureCameraDataOffer,
        identity: ControlTlsClientIdentity,
        server_name: String,
        now: u64,
    ) -> Result<Self, String> {
        let route = runtime.route(route_id).map_err(|e| e.to_string())?;
        source_authority(runtime, route, now).ok_or("camera Source authority absent")?;
        offer.validate(route).map_err(|e| e.to_string())?;
        if link.peer_node_id() != route.sink.node_id {
            return Err("camera control peer mismatch".into());
        }
        let binding = CameraTlsBinding::from_route(link.tls_binding(), route, offer.stream_id, now)
            .map_err(|e| e.to_string())?;
        let address = SocketAddr::new(
            link.authenticated_peer_addr()
                .map_err(|e| e.to_string())?
                .ip(),
            offer.port,
        );
        let records = AvcRecordGuard::new(AvcStreamKey {
            stream_id: *offer.stream_id.as_uuid().as_bytes(),
            epoch: route.epoch,
        })
        .map_err(|e| e.to_string())?;
        let lease_deadline = runtime
            .route_port_lease_claim(route_id, route.source)
            .ok_or("Source lease absent")?
            .expires_at_ms;
        Ok(Self {
            address,
            identity,
            server_name,
            binding,
            records,
            lease_deadline,
            now,
        })
    }
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn lease_deadline(&self) -> u64 {
        self.lease_deadline
    }
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn validate_snapshot(&self, runtime: &NodeRuntime, now: u64) -> Result<(), String> {
        let route = runtime
            .route(self.binding.route_id)
            .map_err(|e| e.to_string())?;
        source_authority(runtime, route, now).ok_or("Source authority absent")?;
        if now >= self.lease_deadline
            || route.selected_format != Some(FormatDescriptor::new("cavc-avc-access-unit-v1"))
            || CameraTlsBinding::from_route(
                self.binding.session,
                route,
                self.binding.stream_id,
                now,
            )
            .map_err(|e| e.to_string())?
                != self.binding
        {
            return Err("camera preparation stale".into());
        }
        Ok(())
    }
    pub(crate) fn connect(self) -> Result<SecureCameraSender, String> {
        let wall_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis() as u64;
        let remaining = self
            .lease_deadline
            .checked_sub(self.now.max(wall_now))
            .filter(|ms| *ms > 0)
            .ok_or("Source lease expired before connect")?;
        let channel = CameraTlsChannel::connect_until(
            ControlTlsEndpoint {
                address: self.address,
                io_timeout: Duration::from_millis(500),
            },
            &self.server_name,
            self.identity,
            self.binding,
            Instant::now() + Duration::from_millis(remaining),
        )
        .map_err(|e| e.to_string())?;
        Ok(SecureCameraSender {
            channel,
            records: self.records,
            binding: self.binding,
            lease_deadline: self.lease_deadline,
            last_ms: self.now,
        })
    }
}
impl SecureCameraSender {
    pub fn connect(
        runtime: &NodeRuntime,
        link: &CatalogControlLink,
        route_id: RouteId,
        offer: SecureCameraDataOffer,
        directory_json: &[u8],
        encoded_credential: &[u8],
        now: u64,
    ) -> Result<Self, String> {
        let route = runtime.route(route_id).map_err(|e| e.to_string())?;
        source_authority(runtime, route, now).ok_or("camera Source consent or lease absent")?;
        offer.validate(route).map_err(|e| e.to_string())?;
        if link.peer_node_id() != route.sink.node_id {
            return Err("camera control peer differs from Sink".into());
        }
        let directory = PairedPeerDirectory::decode_json(route.source.node_id, directory_json)
            .map_err(|e| e.to_string())?;
        let peer = directory
            .active_peers()
            .find(|p| p.node_id == link.peer_node_id())
            .ok_or("camera peer is not actively paired")?;
        let identity = ControlTlsClientIdentity::from_bundle(
            peer.peer_certificate_sha256,
            ControlCredentialBundle::decode(encoded_credential).map_err(|e| e.to_string())?,
        );
        PreparedCameraSender::new(
            runtime,
            link,
            route_id,
            offer,
            identity,
            peer.server_name.clone(),
            now,
        )?
        .connect()
    }
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn ended(&self) -> bool {
        self.records.ended()
    }
    pub fn binding(&self) -> CameraTlsBinding {
        self.binding
    }
    pub fn stop_handle(&self) -> Result<CameraTlsStopHandle, String> {
        self.channel.stop_handle().map_err(|e| e.to_string())
    }
    pub fn stop(&mut self) {
        self.channel.close();
    }
    pub(crate) fn check(&mut self, runtime: &NodeRuntime, now: u64) -> Result<(), String> {
        if now < self.last_ms || now >= self.lease_deadline {
            return Err("camera Source lease expired or clock regressed".into());
        }
        self.last_ms = now;
        let route = runtime
            .route(self.binding.route_id)
            .map_err(|e| e.to_string())?;
        source_authority(runtime, route, now).ok_or("camera Source authority revoked")?;
        let current =
            CameraTlsBinding::from_route(self.binding.session, route, self.binding.stream_id, now)
                .map_err(|e| e.to_string())?;
        if current != self.binding
            || route.selected_format != Some(FormatDescriptor::new("cavc-avc-access-unit-v1"))
        {
            return Err("camera binding or format changed".into());
        }
        Ok(())
    }
    /// Accepts exactly one complete CAVC record from AvcWireSessionEncoder or a fixture.
    /// Partial/malformed/replayed records irreversibly close this sender.
    pub fn send_record(
        &mut self,
        runtime: &NodeRuntime,
        bytes: &[u8],
        now: u64,
    ) -> Result<(), String> {
        let result = (|| {
            let before = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis() as u64;
            self.check(runtime, now.max(before))?;
            let record = decode_record(bytes).map_err(|e| e.to_string())?;
            self.records.accept(&record).map_err(|e| e.to_string())?;
            self.channel.send_record(bytes).map_err(|e| e.to_string())?;
            let after = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis() as u64;
            self.check(runtime, after)
        })();
        if result.is_err() || self.records.ended() {
            self.stop();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::touchpad_consent::TouchpadConsentGate;

    fn pending() -> (NodeRuntime, RouteId) {
        let mut android = capyio_testkit::android_node();
        let mut sink_node = capyio_testkit::windows_node();
        for node in [&mut android, &mut sink_node] {
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
                }
            }
        }
        let cap = android
            .capabilities
            .values_mut()
            .find(|c| c.class == CapabilityClass::Camera)
            .unwrap();
        let port = cap.ports.values_mut().next().unwrap();
        port.formats = vec![FormatDescriptor::new("cavc-avc-access-unit-v1")];
        port.qos_modes.insert(QosMode::Interactive);
        let source = PortRef {
            node_id: android.id,
            capability_id: cap.id,
            port_id: port.id,
        };
        let sink_cap = sink_node.capabilities.values_mut().next().unwrap();
        sink_cap.class = CapabilityClass::Camera;
        let sink_port = sink_cap.ports.values_mut().next().unwrap();
        sink_port.direction = PortDirection::Sink;
        sink_port.profile = port.profile.clone();
        sink_port.formats = port.formats.clone();
        sink_port.qos_modes = port.qos_modes.clone();
        let sink = PortRef {
            node_id: sink_node.id,
            capability_id: sink_cap.id,
            port_id: sink_port.id,
        };
        let mut runtime = NodeRuntime::new(android).unwrap();
        runtime.register_peer(sink_node, true).unwrap();
        let session = runtime.open_session(sink.node_id).unwrap();
        let route = runtime
            .create_route(session, source, sink, RouteBackend::AdapterManaged)
            .unwrap();
        (runtime, route)
    }
    #[test]
    fn actual_consent_gate_and_source_lease_are_both_required() {
        let (mut runtime, id) = pending();
        let source = runtime.route(id).unwrap().source;
        let mut consent = TouchpadConsentGate::default();
        assert_eq!(
            source_authority(&runtime, runtime.route(id).unwrap(), 100),
            None
        );
        consent.enqueue(&mut runtime, id, 100).unwrap();
        assert!(
            runtime
                .acquire_route_port_lease(id, source, 1000, 101)
                .is_err()
        );
        consent
            .resolve_bounded(&mut runtime, id, true, 102, Some(3000))
            .unwrap();
        assert_eq!(
            source_authority(&runtime, runtime.route(id).unwrap(), 103),
            None
        );
        runtime
            .acquire_route_port_lease(id, source, 1000, 103)
            .unwrap();
        assert_eq!(
            source_authority(&runtime, runtime.route(id).unwrap(), 104),
            Some(3000)
        );
        assert_eq!(
            source_authority(&runtime, runtime.route(id).unwrap(), 1103),
            None
        );
        assert_eq!(
            source_authority(&runtime, runtime.route(id).unwrap(), 3000),
            None
        );
    }
    #[test]
    fn denial_never_authorizes_source() {
        let (mut runtime, id) = pending();
        let mut consent = TouchpadConsentGate::default();
        consent.enqueue(&mut runtime, id, 100).unwrap();
        consent.resolve(&mut runtime, id, false, 101).unwrap();
        assert_eq!(
            source_authority(&runtime, runtime.route(id).unwrap(), 102),
            None
        );
    }
    #[test]
    fn starting_and_stopping_use_real_source_lease_lifecycle() {
        let (mut runtime, id) = pending();
        let source = runtime.route(id).unwrap().source;
        let mut consent = TouchpadConsentGate::default();
        consent.enqueue(&mut runtime, id, 100).unwrap();
        consent
            .resolve_bounded(&mut runtime, id, true, 101, Some(3000))
            .unwrap();
        runtime
            .acquire_route_port_lease(id, source, 1000, 102)
            .unwrap();
        runtime
            .prepare_route(
                id,
                Some(FormatDescriptor::new("cavc-avc-access-unit-v1")),
                QosMode::Interactive,
                103,
            )
            .unwrap();
        runtime.begin_route_start(id, 104).unwrap();
        assert_eq!(
            source_authority(&runtime, runtime.route(id).unwrap(), 105),
            Some(3000)
        );
        runtime.begin_route_stop(id).unwrap();
        runtime.stop_route(id).unwrap();
        assert_eq!(
            source_authority(&runtime, runtime.route(id).unwrap(), 106),
            None
        );
    }
    #[test]
    fn owned_camera_busy_publication_preserves_only_live_route_authority() {
        let (mut runtime, id) = pending();
        let source = runtime.route(id).unwrap().source;
        let mut consent = TouchpadConsentGate::default();
        consent.enqueue(&mut runtime, id, 100).unwrap();
        consent
            .resolve_bounded(&mut runtime, id, true, 101, Some(3000))
            .unwrap();
        runtime
            .acquire_route_port_lease(id, source, 1000, 102)
            .unwrap();
        runtime
            .prepare_route(
                id,
                Some(FormatDescriptor::new("cavc-avc-access-unit-v1")),
                QosMode::Interactive,
                103,
            )
            .unwrap();
        runtime.begin_route_start(id, 104).unwrap();
        runtime.activate_route(id).unwrap();
        let mut catalog = runtime.snapshot().local_node;
        let cap = catalog.capabilities.get_mut(&source.capability_id).unwrap();
        cap.availability = Availability::Busy;
        cap.ports.get_mut(&source.port_id).unwrap().availability = Availability::Busy;
        runtime.replace_local_catalog(catalog).unwrap();
        assert_eq!(
            source_authority(&runtime, runtime.route(id).unwrap(), 105),
            Some(3000)
        );
        assert_eq!(
            source_authority(&runtime, runtime.route(id).unwrap(), 1102),
            None
        );
        runtime.begin_route_stop(id).unwrap();
        runtime.stop_route(id).unwrap();
        assert_eq!(
            source_authority(&runtime, runtime.route(id).unwrap(), 106),
            None
        );
    }
}
