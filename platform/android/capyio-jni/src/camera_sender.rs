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
use capyio_vcamdroid_adapter::AvcStreamKey;
mod admission;
pub(crate) use admission::CameraSenderProfile;
use admission::{Admission, AuthorityClock};
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
    records: Admission,
    profile: CameraSenderProfile,
    clock: AuthorityClock,
    binding: CameraTlsBinding,
    lease_deadline: u64,
    terminal: bool,
    completed_eos: bool,
}
/// Immutable native-only preparation. It owns no socket; prepare under brief
/// Runtime/control locks and call connect only after releasing those locks.
#[derive(Clone)]
pub(crate) struct PreparedCameraSender {
    address: SocketAddr,
    identity: ControlTlsClientIdentity,
    server_name: String,
    pub(crate) binding: CameraTlsBinding,
    records: Admission,
    profile: CameraSenderProfile,
    clock: AuthorityClock,
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
        let instant = Instant::now();
        let now = now.max(wall_time_ms()?);
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
        let lease_deadline = runtime
            .route_port_lease_claim(route_id, route.source)
            .ok_or("Source lease absent")?
            .expires_at_ms;
        let profile = CameraSenderProfile::from_selection(route.selected_format.as_ref())?;
        let clock = AuthorityClock::new(now, lease_deadline, instant)?;
        let records = Admission::new(
            AvcStreamKey {
                stream_id: *offer.stream_id.as_uuid().as_bytes(),
                epoch: route.epoch,
            },
            profile,
            lease_deadline - now,
            instant,
        )?;
        Ok(Self {
            address,
            identity,
            server_name,
            binding,
            records,
            profile,
            clock,
            lease_deadline,
            now,
        })
    }
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn profile(&self) -> CameraSenderProfile {
        self.profile
    }
    /// Compare immutable authority/selection, not a candidate's fresh clock.
    /// Equivalent retries must retain the original preparation rather than replace it.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn same_preparation(&self, other: &Self) -> bool {
        self.binding == other.binding
            && self.profile == other.profile
            && self.lease_deadline == other.lease_deadline
            && self.address == other.address
            && self.server_name == other.server_name
            && self.identity.expected_peer_certificate == other.identity.expected_peer_certificate
            && self.identity.certificate_chain_der == other.identity.certificate_chain_der
            && self.identity.private_key_der == other.identity.private_key_der
            && self.identity.server_ca_der == other.identity.server_ca_der
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
        check_original_source_lease(self.profile, runtime, route, self.lease_deadline)?;
        self.clock
            .clone()
            .check(now, self.lease_deadline, Instant::now())?;
        if !self
            .profile
            .matches_selection(route.selected_format.as_ref())
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
    pub(crate) fn connect(mut self) -> Result<SecureCameraSender, String> {
        let wall_now = wall_time_ms()?;
        self.clock
            .check(self.now.max(wall_now), self.lease_deadline, Instant::now())?;
        let channel = CameraTlsChannel::connect_until(
            ControlTlsEndpoint {
                address: self.address,
                io_timeout: Duration::from_millis(500),
            },
            &self.server_name,
            self.identity,
            self.binding,
            self.clock.deadline,
        )
        .map_err(|e| e.to_string())?;
        self.clock
            .check(wall_time_ms()?, self.lease_deadline, Instant::now())?;
        Ok(SecureCameraSender {
            channel,
            records: self.records,
            profile: self.profile,
            clock: self.clock,
            binding: self.binding,
            lease_deadline: self.lease_deadline,
            terminal: false,
            completed_eos: false,
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
        self.terminal = true;
        self.completed_eos = false;
        self.channel.close();
    }
    pub(crate) fn check(&mut self, runtime: &NodeRuntime, now: u64) -> Result<(), String> {
        if self.terminal {
            return Err("camera sender is terminal".into());
        }
        let result = self.check_authority(runtime, now);
        if result.is_err() {
            self.stop();
        }
        result
    }
    /// Fresh post-I/O snapshot check. A successfully written EOS may have closed
    /// its channel, but a failed record or explicit Stop can never use this path.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn check_after_send(
        &mut self,
        runtime: &NodeRuntime,
        now: u64,
    ) -> Result<(), String> {
        let result = if self.terminal && !self.completed_eos {
            Err("camera sender is terminal".into())
        } else {
            self.check_authority(runtime, now)
        };
        if result.is_err() {
            self.stop();
        }
        result
    }
    fn check_authority(&mut self, runtime: &NodeRuntime, now: u64) -> Result<(), String> {
        self.clock.check(now, self.lease_deadline, Instant::now())?;
        let route = runtime
            .route(self.binding.route_id)
            .map_err(|e| e.to_string())?;
        source_authority(runtime, route, now).ok_or("camera Source authority revoked")?;
        check_original_source_lease(self.profile, runtime, route, self.lease_deadline)?;
        let current =
            CameraTlsBinding::from_route(self.binding.session, route, self.binding.stream_id, now)
                .map_err(|e| e.to_string())?;
        if current != self.binding
            || !self
                .profile
                .matches_selection(route.selected_format.as_ref())
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
            let before = wall_time_ms()?;
            self.check(runtime, now.max(before))?;
            self.records.accept(bytes, Instant::now())?;
            self.channel.send_record(bytes).map_err(|e| e.to_string())?;
            let after = wall_time_ms()?;
            self.check(runtime, after)
        })();
        if result.is_err() {
            self.stop();
        } else if self.records.ended() {
            self.terminal = true;
            self.completed_eos = true;
            self.channel.close();
        }
        result
    }
}

fn check_original_source_lease(
    profile: CameraSenderProfile,
    runtime: &NodeRuntime,
    route: &Route,
    original_deadline: u64,
) -> Result<(), String> {
    if profile == CameraSenderProfile::Finite720p30V1
        && runtime
            .route_port_lease_claim(route.id, route.source)
            .is_none_or(|lease| lease.expires_at_ms != original_deadline)
    {
        return Err("finite camera original Source lease changed".into());
    }
    Ok(())
}

fn wall_time_ms() -> Result<u64, String> {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis(),
    )
    .map_err(|e| e.to_string())
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
    fn preparation_equivalence_pins_identity_but_retains_original_clock() {
        let (runtime, id) = pending();
        let route = runtime.route(id).unwrap();
        let instant = Instant::now();
        let stream_id = StreamId::new();
        // Deliberately inert synthetic credentials: this pure test opens no socket.
        let original = PreparedCameraSender {
            address: "127.0.0.1:1".parse().unwrap(),
            identity: ControlTlsClientIdentity {
                expected_peer_certificate: capyio_pairing::CertificateFingerprint::new([7; 32])
                    .unwrap(),
                certificate_chain_der: vec![vec![1]],
                private_key_der: vec![2],
                server_ca_der: vec![3],
            },
            server_name: "synthetic.invalid".into(),
            binding: CameraTlsBinding {
                session: capyio_control_tls::ControlTlsBinding::new(
                    route.session_id,
                    route.sink.node_id,
                    route.source.node_id,
                )
                .unwrap(),
                route_id: id,
                source: route.source,
                sink: route.sink,
                stream_id,
                epoch: 1,
                expires_at_ms: 2000,
            },
            records: Admission::new(
                AvcStreamKey {
                    stream_id: *stream_id.as_uuid().as_bytes(),
                    epoch: 1,
                },
                CameraSenderProfile::Finite720p30V1,
                1000,
                instant,
            )
            .unwrap(),
            profile: CameraSenderProfile::Finite720p30V1,
            clock: AuthorityClock::new(1000, 2000, instant).unwrap(),
            lease_deadline: 2000,
            now: 1000,
        };
        let mut later = original.clone();
        later.now = 1500;
        later.clock =
            AuthorityClock::new(1500, 2000, instant + Duration::from_millis(500)).unwrap();
        assert!(original.same_preparation(&later));
        assert_eq!(original.clock.deadline, original.clone().clock.deadline);
        let mut changed = original.clone();
        changed.profile = CameraSenderProfile::Legacy;
        assert!(!original.same_preparation(&changed));
        let mut changed = original.clone();
        changed.lease_deadline += 1;
        assert!(!original.same_preparation(&changed));
        let mut changed = original.clone();
        changed.binding.expires_at_ms += 1;
        assert!(!original.same_preparation(&changed));
        let mut changed = original.clone();
        changed.binding.epoch += 1;
        assert!(!original.same_preparation(&changed));
        let mut changed = original.clone();
        changed.binding.stream_id = StreamId::new();
        assert!(!original.same_preparation(&changed));
        let mut changed = original.clone();
        changed.address.set_port(2);
        assert!(!original.same_preparation(&changed));
        let mut changed = original.clone();
        changed.server_name = "other.invalid".into();
        assert!(!original.same_preparation(&changed));
        let mut changed = original.clone();
        changed.identity.expected_peer_certificate =
            capyio_pairing::CertificateFingerprint::new([8; 32]).unwrap();
        assert!(!original.same_preparation(&changed));
        let mut changed = original.clone();
        changed.identity.private_key_der = vec![4];
        assert!(!original.same_preparation(&changed));
    }
    #[test]
    fn finite_original_source_lease_rejects_narrowing_and_renewal() {
        let (mut runtime, id) = pending();
        let source = runtime.route(id).unwrap().source;
        let mut consent = TouchpadConsentGate::default();
        consent.enqueue(&mut runtime, id, 100).unwrap();
        consent
            .resolve_bounded(&mut runtime, id, true, 101, Some(3000))
            .unwrap();
        runtime
            .acquire_route_port_lease(id, source, 2000, 102)
            .unwrap();
        assert!(
            check_original_source_lease(
                CameraSenderProfile::Finite720p30V1,
                &runtime,
                runtime.route(id).unwrap(),
                2000
            )
            .is_ok()
        );
        for deadline in [1500, 2500] {
            runtime
                .acquire_route_port_lease(id, source, deadline, 103)
                .unwrap();
            // Both claims remain live and authorized; only exact original identity rejects them.
            assert!(source_authority(&runtime, runtime.route(id).unwrap(), 104).is_some());
            assert!(
                check_original_source_lease(
                    CameraSenderProfile::Finite720p30V1,
                    &runtime,
                    runtime.route(id).unwrap(),
                    2000
                )
                .is_err()
            );
            assert!(
                check_original_source_lease(
                    CameraSenderProfile::Legacy,
                    &runtime,
                    runtime.route(id).unwrap(),
                    2000
                )
                .is_ok()
            );
        }
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
