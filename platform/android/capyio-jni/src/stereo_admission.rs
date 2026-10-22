//! Shared exact stereo authority and bounded PCM accounting. No network or Runtime ownership.
use crate::secure_stereo_receiver::{MAX_STEREO_OPERATION, SecureStereoSinkAdmission};
use capyio_control_tls::ControlTlsBinding;
use capyio_core::{
    AdapterInstanceDescriptor, AdapterState, AuthorizationState, Availability,
    CapabilityDescriptor, FormatDescriptor, MessageId, OnlineState, Platform, PortDirection,
    PortRef, QosMode, Route, RouteBackend, RouteState, Session, SessionState,
};
use capyio_native_audio_lan::SECURE_STEREO_PCM_PAYLOAD_BYTES;
use capyio_protocol::{PortLeaseOutcome, SECURE_AUDIO_STEREO_FORMAT, ValidatedPortLeaseResponse};
use capyio_runtime::{NodeRuntime, PortLeaseClaim};
use std::{
    io,
    time::{Duration, Instant},
};
pub(crate) const FRAME_ALIGNMENT: usize = 4;
pub(crate) const MAX_WRITE_ATTEMPTS: u16 = 480;
fn error(reason: impl ToString) -> io::Error {
    io::Error::other(reason.to_string())
}
#[derive(Clone, Debug)]
pub(crate) struct EndpointAuthority {
    reference: PortRef,
    capability: CapabilityDescriptor,
    adapter: AdapterInstanceDescriptor,
}
impl EndpointAuthority {
    pub(crate) fn capture(runtime: &NodeRuntime, reference: PortRef) -> io::Result<Self> {
        let snapshot = runtime.snapshot();
        let node = if reference.node_id == snapshot.local_node.id {
            &snapshot.local_node
        } else {
            snapshot
                .peers
                .iter()
                .find(|n| n.id == reference.node_id)
                .ok_or_else(|| error("stereo peer absent"))?
        };
        let capability = node
            .capabilities
            .get(&reference.capability_id)
            .ok_or_else(|| error("stereo Capability absent"))?
            .clone();
        let adapter = node
            .adapter_instances
            .get(&capability.adapter_instance_id)
            .ok_or_else(|| error("stereo Adapter absent"))?
            .clone();
        Ok(Self {
            reference,
            capability,
            adapter,
        })
    }
    pub(crate) fn check(
        &self,
        runtime: &NodeRuntime,
        route: &Route,
        source_leased: bool,
    ) -> io::Result<()> {
        let snapshot = runtime.snapshot();
        let node = if self.reference.node_id == snapshot.local_node.id {
            &snapshot.local_node
        } else {
            snapshot
                .peers
                .iter()
                .find(|n| n.id == self.reference.node_id)
                .ok_or_else(|| error("stereo peer disappeared"))?
        };
        let mut fresh = Self::capture(runtime, self.reference)?;
        let expected_direction = if self.reference == route.source {
            PortDirection::Source
        } else {
            PortDirection::Sink
        };
        let port = fresh
            .capability
            .ports
            .get_mut(&self.reference.port_id)
            .ok_or_else(|| error("stereo Port absent"))?;
        let available = |value| {
            value == Availability::Available
                || (source_leased && self.reference == route.source && value == Availability::Busy)
        };
        if node.online_state != OnlineState::Online
            || !available(fresh.capability.availability)
            || !available(port.availability)
            || port.direction != expected_direction
            || port.profile != route.profile
            || port.formats != [FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT)]
            || port.qos_modes != std::collections::BTreeSet::from([QosMode::Interactive])
            || fresh.adapter.state != AdapterState::Ready
            || !fresh
                .adapter
                .supported_route_modes
                .contains(&RouteBackend::AdapterManaged)
        {
            return Err(error("stereo endpoint no longer exactly available"));
        }
        // Busy is the sole permitted remote Source catalog transition after its
        // independent lease. All IDs, metadata, permissions and settings freeze.
        fresh.capability.availability = self.capability.availability;
        port.availability = self.capability.ports[&self.reference.port_id].availability;
        if fresh.capability != self.capability || fresh.adapter != self.adapter {
            return Err(error("stereo original Capability or Adapter changed"));
        }
        Ok(())
    }
}

pub(crate) struct PendingPcm {
    pub(crate) bytes: [u8; SECURE_STEREO_PCM_PAYLOAD_BYTES],
    pub(crate) offset: usize,
    pub(crate) deadline: Instant,
    pub(crate) attempts: u16,
}
impl PendingPcm {
    pub(crate) fn new(bytes: Vec<u8>, now: Instant, end: Instant) -> io::Result<Self> {
        if bytes.len() != SECURE_STEREO_PCM_PAYLOAD_BYTES || now >= end {
            return Err(error("invalid stereo PCM frame"));
        }
        Ok(Self {
            bytes: bytes
                .try_into()
                .map_err(|_| error("invalid stereo PCM frame"))?,
            offset: 0,
            deadline: end.min(now + MAX_STEREO_OPERATION),
            attempts: 0,
        })
    }
    pub(crate) fn before_write(&mut self, now: Instant) -> io::Result<&[u8]> {
        self.before_write_checked(now).map_err(error)
    }
    /// The JNI boundary uses static errors and never allocates on failure.
    pub(crate) fn before_write_checked(&mut self, now: Instant) -> Result<&[u8], &'static str> {
        if now >= self.deadline
            || self.attempts >= MAX_WRITE_ATTEMPTS
            || self.offset >= self.bytes.len()
            || !self.offset.is_multiple_of(FRAME_ALIGNMENT)
        {
            return Err("stereo PCM operation deadline or write bound");
        }
        self.attempts += 1;
        self.bytes
            .get(self.offset..)
            .ok_or("stereo PCM offset out of bounds")
    }
    pub(crate) fn after_write(
        &mut self,
        result: io::Result<usize>,
        now: Instant,
    ) -> io::Result<(usize, bool)> {
        if now >= self.deadline {
            return Err(error("stereo PCM write exceeded absolute deadline"));
        }
        let accepted = match result {
            Ok(n) => n,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                0
            }
            Err(e) => return Err(e),
        };
        self.after_accept(accepted, now).map_err(error)
    }
    pub(crate) fn after_accept(
        &mut self,
        accepted: usize,
        now: Instant,
    ) -> Result<(usize, bool), &'static str> {
        if now >= self.deadline {
            return Err("stereo PCM write exceeded absolute deadline");
        }
        let remaining = self
            .bytes
            .len()
            .checked_sub(self.offset)
            .ok_or("stereo PCM offset overflow")?;
        if accepted > remaining || !accepted.is_multiple_of(FRAME_ALIGNMENT) {
            return Err("invalid stereo PCM partial write");
        }
        self.offset = self
            .offset
            .checked_add(accepted)
            .ok_or("stereo PCM offset overflow")?;
        Ok((accepted, self.offset == self.bytes.len()))
    }
}

pub(crate) fn expiry(route: &Route) -> io::Result<u64> {
    match route.authorization {
        AuthorizationState::Authorized {
            expires_at_ms: Some(end),
        } => Ok(end),
        _ => Err(error("finite local stereo consent required")),
    }
}
pub(crate) fn validate_pcm_enable(
    bound: bool,
    enabled: bool,
    now: Instant,
    original_end: Instant,
) -> io::Result<()> {
    if now >= original_end {
        return Err(crate::secure_stereo_receiver::terminal_error(
            "authority_expired",
        ));
    }
    if !bound || enabled {
        return Err(crate::secure_stereo_receiver::terminal_error(
            "invalid_pcm_enable",
        ));
    }
    Ok(())
}

pub(crate) fn validate_admission(admission: &SecureStereoSinkAdmission) -> io::Result<()> {
    if !(Duration::from_secs(1)..=Duration::from_secs(60)).contains(&admission.lifetime)
        || admission.lifetime.subsec_nanos() != 0
        || admission.local_address.is_unspecified()
        || admission.local_address.is_multicast()
        || admission.stream_id.as_uuid().is_nil()
    {
        return Err(error("invalid explicit finite stereo Sink admission"));
    }
    Ok(())
}
pub(crate) fn consent_end(
    started: Instant,
    lifetime: Duration,
    original_end: Instant,
    authority_ms: u64,
) -> io::Result<Instant> {
    let end = original_end.min(
        started
            .checked_add(lifetime.min(Duration::from_millis(authority_ms)))
            .ok_or_else(|| error("stereo deadline overflow"))?,
    );
    if end <= started {
        return Err(error("original stereo consent expired"));
    }
    Ok(end)
}
pub(crate) fn source_lease_ttl(remaining: Duration) -> io::Result<u64> {
    u64::try_from(remaining.as_millis())
        .ok()
        .and_then(|n| n.checked_sub(MAX_STEREO_OPERATION.as_millis() as u64))
        .filter(|n| *n >= capyio_protocol::MIN_PORT_LEASE_TTL_MS)
        .ok_or_else(|| error("insufficient stereo negotiation headroom"))
}
pub(crate) fn validate_route(
    runtime: &NodeRuntime,
    original: &Route,
    session: &Session,
    binding: ControlTlsBinding,
    now: u64,
) -> io::Result<()> {
    let snapshot = runtime.snapshot();
    let ordered_nodes = (session.local_node_id == binding.server_node_id
        && session.remote_node_id == binding.client_node_id)
        || (session.local_node_id == binding.client_node_id
            && session.remote_node_id == binding.server_node_id);
    if runtime.route(original.id).map_err(error)? != original
        || runtime.session(session.id).map_err(error)? != session
        || !matches!(original.state, RouteState::Prepared | RouteState::Starting)
        || session.state != SessionState::Ready
        || session.id != binding.session_id
        || original.session_id != session.id
        || !ordered_nodes
        || original.sink.node_id != session.local_node_id
        || original.source.node_id != session.remote_node_id
        || snapshot.local_node.id != session.local_node_id
        || snapshot.local_node.platform != Platform::Android
        || snapshot
            .routes
            .iter()
            .filter(|r| r.session_id == session.id)
            .count()
            != 1
        || original.profile != capyio_core::ProfileId::audio_frames_v1()
        || original.backend != RouteBackend::AdapterManaged
        || original.selected_format != Some(FormatDescriptor::new(SECURE_AUDIO_STEREO_FORMAT))
        || original.selected_qos_mode != Some(QosMode::Interactive)
        || original.epoch > i32::MAX as u64
        || expiry(original)? <= now
    {
        return Err(error(
            "stereo original Route, Session, profile or authority changed",
        ));
    }
    Ok(())
}
pub(crate) fn validate_sink_claim(
    runtime: &NodeRuntime,
    route: &Route,
    claim: PortLeaseClaim,
    now: u64,
) -> io::Result<()> {
    if runtime.route_port_lease_claim(route.id, route.sink) != Some(&claim)
        || claim.port != route.sink
        || claim.route_id != route.id
        || claim.session_id != route.session_id
        || claim.expires_at_ms <= now
        || claim.expires_at_ms > expiry(route)?
    {
        return Err(error("stereo original Sink claim changed or expired"));
    }
    Ok(())
}
pub(crate) fn validate_source_response(
    route: &Route,
    request: MessageId,
    response: ValidatedPortLeaseResponse,
    now: u64,
) -> io::Result<u64> {
    if response.request_message_id != request
        || response.session_id != route.session_id
        || response.route_id != route.id
        || response.port != route.source
        || response.outcome != PortLeaseOutcome::Acquired
    {
        return Err(error("unexpected stereo Source lease response"));
    }
    response
        .expires_at_ms
        .filter(|end| *end > now && *end <= expiry(route).unwrap_or(0))
        .ok_or_else(|| error("invalid stereo Source lease expiry"))
}
