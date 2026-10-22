//! Finite, Rust-owned Android microphone Source sender for synthetic mono PCM.
//!
//! This is not AudioRecord integration and does not advertise a secure platform
//! capability. Call only on a media worker, never an audio callback. Credentials
//! remain in Rust and the transport never reconnects or falls back to UDP.
use capyio_audio::{AudioMediaStreamBinding, AudioStreamSpec};
use capyio_control_credentials::ControlCredentialBundle;
use capyio_control_link::CatalogControlLink;
use capyio_control_tls::{
    ControlTlsClientIdentity, ControlTlsEndpoint,
    audio::{AudioTlsBinding, AudioTlsChannel, AudioTlsStopHandle},
};
use capyio_core::*;
use capyio_native_audio_lan::{SecureMonoPcmAdmission, SecureMonoPcmRateBudget};
use capyio_pairing::PairedPeerDirectory;
use capyio_protocol::{SECURE_AUDIO_MONO_FORMAT, SecureAudioDataOffer};
use capyio_runtime::{NodeRuntime, PortLeaseClaim};
use std::time::{Duration, Instant};

const MAX_LIFETIME: Duration = Duration::from_secs(60);
const IO_BUDGET: Duration = Duration::from_millis(250);

/// Reuses the real Android audio grant/lease policy, not the camera policy.
/// A received offer neither grants microphone consent nor fabricates a lease.
pub fn source_authority(runtime: &NodeRuntime, route: &Route, now: u64) -> Option<u64> {
    crate::audio_negotiation::secure_source_authority(runtime, route, now)
}

/// The existing Android resource consent gate, restricted to microphone Sources.
/// Call `resolve` only in response to an explicit local approval or denial.
#[derive(Default)]
pub struct AudioSourceConsent(crate::touchpad_consent::TouchpadConsentGate);
impl AudioSourceConsent {
    pub fn enqueue(
        &mut self,
        runtime: &mut NodeRuntime,
        route: RouteId,
        now: u64,
    ) -> Result<(), String> {
        require_microphone(runtime, route)?;
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
        require_microphone(runtime, route)?;
        self.0
            .resolve_bounded(runtime, route, allow, now, Some(peer_deadline))
    }
}

fn require_microphone(runtime: &NodeRuntime, id: RouteId) -> Result<(), String> {
    let route = runtime.route(id).map_err(|e| e.to_string())?;
    if crate::audio_negotiation::microphone_source(runtime, route) {
        Ok(())
    } else {
        Err("secure audio requires a local Android microphone Source".into())
    }
}

/// Immutable original Source authority. Starting -> Active is allowed; a new
/// epoch, renewed/narrowed lease or grant, changed selection or Session is not.
struct PinnedSourceAuthority {
    binding: AudioTlsBinding,
    source_lease: PortLeaseClaim,
}
impl PinnedSourceAuthority {
    fn new(runtime: &NodeRuntime, binding: AudioTlsBinding, now: u64) -> Result<Self, String> {
        let route = runtime.route(binding.route_id).map_err(|e| e.to_string())?;
        let source_lease = *runtime
            .route_port_lease_claim(route.id, route.source)
            .ok_or("audio Source lease absent")?;
        let this = Self {
            binding,
            source_lease,
        };
        this.check(runtime, now)?;
        Ok(this)
    }

    fn check_route(&self, route: &Route, now: u64) -> Result<(), String> {
        if route.selected_format != Some(FormatDescriptor::new(SECURE_AUDIO_MONO_FORMAT))
            || route.selected_qos_mode != Some(QosMode::Interactive)
            || AudioTlsBinding::from_route(self.binding.session, route, self.binding.stream_id, now)
                .map_err(|e| e.to_string())?
                != self.binding
        {
            return Err("secure audio Route generation, grant, format or QoS changed".into());
        }
        Ok(())
    }

    fn check(&self, runtime: &NodeRuntime, now: u64) -> Result<(), String> {
        let route = runtime
            .route(self.binding.route_id)
            .map_err(|e| e.to_string())?;
        source_authority(runtime, route, now).ok_or("audio Source authority revoked")?;
        if runtime.route_port_lease_claim(route.id, route.source) != Some(&self.source_lease) {
            return Err("secure audio original Source lease changed".into());
        }
        let node = runtime.snapshot().local_node;
        let port = node
            .capabilities
            .get(&route.source.capability_id)
            .and_then(|cap| cap.ports.get(&route.source.port_id))
            .ok_or("secure audio Source catalog disappeared")?;
        if !port
            .formats
            .contains(&FormatDescriptor::new(SECURE_AUDIO_MONO_FORMAT))
            || !port.qos_modes.contains(&QosMode::Interactive)
            || port.interoperability_mode != InteroperabilityMode::AdapterManaged
        {
            return Err("secure audio Source catalog selection changed".into());
        }
        self.check_route(route, now)
    }
}

/// Independent wall and monotonic deadlines anchored before credential parsing
/// and TCP/TLS admission. Neither retries, timestamps nor packets renew them.
struct AuthorityClock {
    origin: Instant,
    deadline: Instant,
    wall_deadline: u64,
    last_wall: u64,
    last_instant: Instant,
}
impl AuthorityClock {
    fn new(now: u64, lease_deadline: u64, origin: Instant) -> Result<Self, String> {
        let remaining = lease_deadline
            .checked_sub(now)
            .filter(|n| *n > 0)
            .ok_or("audio Source lease expired")?;
        let lifetime = Duration::from_millis(remaining).min(MAX_LIFETIME);
        Ok(Self {
            origin,
            deadline: origin
                .checked_add(lifetime)
                .ok_or("audio deadline overflow")?,
            wall_deadline: now
                .checked_add(lifetime.as_millis() as u64)
                .ok_or("audio wall deadline overflow")?,
            last_wall: now,
            last_instant: origin,
        })
    }

    fn check(&mut self, wall: u64, instant: Instant) -> Result<(), String> {
        if wall < self.last_wall
            || wall >= self.wall_deadline
            || instant < self.last_instant
            || instant >= self.deadline
        {
            return Err("secure audio expired or clock regressed".into());
        }
        self.last_wall = wall;
        self.last_instant = instant;
        Ok(())
    }
}

/// One finite sender generation, admitted only by an actual authenticated offer.
/// Its independent Stop handle must be held by the control/lifecycle owner and
/// stopped on revoke, Stop, control EOF or lease loss. Snapshot checks detect
/// changes before/after I/O; they cannot themselves cancel a mid-write change.
pub struct SecureAudioSender {
    channel: AudioTlsChannel,
    records: SecureMonoPcmAdmission,
    rate: SecureMonoPcmRateBudget,
    authority: PinnedSourceAuthority,
    clock: AuthorityClock,
    terminal: bool,
}
impl SecureAudioSender {
    pub fn connect(
        runtime: &NodeRuntime,
        link: &mut CatalogControlLink,
        route_id: RouteId,
        offer: SecureAudioDataOffer,
        directory_json: &[u8],
        encoded_credential: &[u8],
        now: u64,
    ) -> Result<Self, String> {
        let origin = Instant::now();
        let now = now.max(wall_time_ms()?);
        let route = runtime.route(route_id).map_err(|e| e.to_string())?;
        // Consume before any attempt. A failure cannot re-arm this control-link
        // generation and caller-supplied structs are not authenticated receipts.
        link.take_received_secure_audio_offer(route, offer)
            .map_err(|e| e.to_string())?;
        source_authority(runtime, route, now).ok_or("audio Source consent or lease absent")?;
        offer.validate(route).map_err(|e| e.to_string())?;
        if link.peer_node_id() != route.sink.node_id {
            return Err("audio control peer differs from Sink".into());
        }
        let binding = AudioTlsBinding::from_route(link.tls_binding(), route, offer.stream_id, now)
            .map_err(|e| e.to_string())?;
        let authority = PinnedSourceAuthority::new(runtime, binding, now)?;
        let clock = AuthorityClock::new(now, authority.source_lease.expires_at_ms, origin)?;
        let rate = SecureMonoPcmRateBudget::new(clock.deadline.duration_since(origin))
            .map_err(|e| e.to_string())?;
        let records = SecureMonoPcmAdmission::new(AudioMediaStreamBinding {
            session_id: route.session_id,
            route_id,
            stream_id: offer.stream_id,
            stream_epoch: u32::try_from(route.epoch).map_err(|_| "audio epoch overflow")?,
            selected_spec: AudioStreamSpec::voice_interactive(),
        })
        .map_err(|e| e.to_string())?;
        let directory = PairedPeerDirectory::decode_json(route.source.node_id, directory_json)
            .map_err(|e| e.to_string())?;
        let peer = directory
            .active_peers()
            .find(|peer| peer.node_id == route.sink.node_id)
            .ok_or("audio Sink is not actively paired")?;
        if link
            .authenticated_peer_certificate()
            .map_err(|e| e.to_string())?
            != peer.peer_certificate_sha256
        {
            return Err("audio paired certificate differs from authenticated control peer".into());
        }
        let identity = ControlTlsClientIdentity::from_bundle(
            peer.peer_certificate_sha256,
            ControlCredentialBundle::decode(encoded_credential).map_err(|e| e.to_string())?,
        );
        let address = std::net::SocketAddr::new(
            link.authenticated_peer_addr()
                .map_err(|e| e.to_string())?
                .ip(),
            offer.port,
        );
        let channel = AudioTlsChannel::connect_until(
            ControlTlsEndpoint {
                address,
                io_timeout: IO_BUDGET,
            },
            &peer.server_name,
            identity,
            binding,
            clock.deadline,
        )
        .map_err(|e| e.to_string())?;
        let mut sender = Self {
            channel,
            records,
            rate,
            authority,
            clock,
            terminal: false,
        };
        sender.check(runtime, wall_time_ms()?)?;
        Ok(sender)
    }

    pub fn binding(&self) -> AudioTlsBinding {
        self.authority.binding
    }

    pub fn stop_handle(&self) -> Result<AudioTlsStopHandle, String> {
        self.channel.stop_handle().map_err(|e| e.to_string())
    }

    pub fn stop(&mut self) {
        self.terminal = true;
        self.channel.close();
    }

    pub fn check(&mut self, runtime: &NodeRuntime, now: u64) -> Result<(), String> {
        let result = (|| {
            if self.terminal {
                return Err("secure audio sender is terminal".into());
            }
            self.clock.check(now, Instant::now())?;
            self.authority.check(runtime, now)
        })();
        if result.is_err() {
            self.stop();
        }
        result
    }

    /// Send exactly one 1064-byte CPYA mono record, terminal on any failure.
    /// The runtime is stable during this synchronous call. Owners with mutable
    /// concurrent control use `send_record_with_snapshot` and the Stop handle.
    pub fn send_record(
        &mut self,
        runtime: &NodeRuntime,
        bytes: &[u8],
        now: u64,
    ) -> Result<(), String> {
        self.send_record_with_snapshot(|| Ok(runtime.clone()), bytes, now)
    }

    /// Acquire fresh Runtime snapshots without retaining a Runtime/control lock
    /// during network I/O. No packet can report success after failed revalidation.
    pub fn send_record_with_snapshot(
        &mut self,
        mut snapshot: impl FnMut() -> Result<NodeRuntime, String>,
        bytes: &[u8],
        now: u64,
    ) -> Result<(), String> {
        let result = (|| {
            self.check(&snapshot()?, now.max(wall_time_ms()?))?;
            let instant = Instant::now();
            self.rate
                .admit(instant.duration_since(self.clock.origin))
                .map_err(|e| e.to_string())?;
            self.records
                .admit_record(bytes)
                .map_err(|e| e.to_string())?;
            let deadline = instant
                .checked_add(IO_BUDGET)
                .ok_or("audio I/O deadline overflow")?
                .min(self.clock.deadline);
            self.channel
                .send_record_until(bytes, deadline)
                .map_err(|e| e.to_string())?;
            self.check(&snapshot()?, wall_time_ms()?)
        })();
        if result.is_err() {
            self.stop();
        }
        result
    }
}
impl Drop for SecureAudioSender {
    fn drop(&mut self) {
        self.stop();
    }
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
mod tests;
