//! One-shot executable lab composition. No credential enrollment or persistence.
//! The launcher must exclusively own both inherited pipes and protect their
//! contents. A pipe is not authentication or protection from its invoker.
use crate::{LinuxPlaybackHost, launcher_io::LauncherIo};
use capyio_control_credentials::ControlCredentialBundle;
use capyio_control_link::{CatalogControlLink, ReceivedControlMessage};
use capyio_control_tls::{
    AuthenticatedControlServer, ControlTlsNodeBinding, ControlTlsServerIdentity,
};
use capyio_core::*;
use capyio_linux_audio::{
    AuthenticatedPlaybackOwner, EndpointMode, PlaybackAdmission, PlaybackConfig, PlaybackTarget,
};
use capyio_pairing::PairedPeerDirectory;
use capyio_protocol::{
    MIN_PORT_LEASE_TTL_MS, NativeAudioDataOffer, NativeAudioEndpoint, NativeAudioPreset,
    PortLeaseAction, PortLeaseOutcome, ValidatedRouteStatus,
};
use capyio_runtime::{NodeRuntime, PortLeasePolicy};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    net::{SocketAddrV4, TcpListener},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const LOCAL_FRAME: usize = 4096;
const BOOTSTRAP: Duration = Duration::from_secs(5);
const ACCEPT: Duration = Duration::from_secs(15);
const CATALOG: Duration = Duration::from_secs(5);
const CONSENT: Duration = Duration::from_secs(30);
fn fail() -> io::Error {
    io::Error::other("Linux lab playback launch failed")
}
fn sanitized<T>(value: Result<T, impl std::fmt::Display>) -> io::Result<T> {
    value.map_err(|_| fail())
}
fn now_ms() -> io::Result<u64> {
    sanitized(u64::try_from(
        sanitized(SystemTime::now().duration_since(UNIX_EPOCH))?.as_millis(),
    ))
}
fn check(deadline: Instant) -> io::Result<()> {
    if Instant::now() >= deadline {
        Err(fail())
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ProposalMode {
    #[default]
    LinuxLed,
    IncomingDraft,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LaunchConfig {
    version: u32,
    node_id: NodeId,
    source: PortRef,
    listen: SocketAddrV4,
    media: SocketAddrV4,
    #[serde(default)]
    target: Option<String>,
    #[serde(default, deserialize_with = "deserialize_endpoint_mode")]
    endpoint_mode: EndpointMode,
    duration_secs: u64,
    #[serde(default)]
    proposal_mode: ProposalMode,
}
/// Separate explicit CLI schema: the configured remote endpoint is a Sink.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureLaunchConfig {
    version: u32,
    node_id: NodeId,
    sink: PortRef,
    listen: SocketAddrV4,
    media: SocketAddrV4,
    target: String,
    duration_secs: u64,
}
fn parse_capture(bytes: &[u8]) -> io::Result<LaunchConfig> {
    let value: CaptureLaunchConfig = sanitized(serde_json::from_slice(bytes))?;
    if value.target.eq_ignore_ascii_case("auto") || value.target.eq_ignore_ascii_case("default") {
        return Err(fail());
    }
    // Reuse address/duration validation without accepting playback schema fields.
    let encoded = sanitized(serde_json::to_vec(
        &json!({"version":value.version,"node_id":value.node_id,"source":value.sink,"listen":value.listen,"media":value.media,"target":value.target,"duration_secs":value.duration_secs}),
    ))?;
    let mut config = LaunchConfig::parse(&encoded)?;
    config.endpoint_mode = EndpointMode::StereoCapture;
    Ok(config)
}

fn deserialize_endpoint_mode<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<EndpointMode, D::Error> {
    match String::deserialize(deserializer)?.as_str() {
        "playback" => Ok(EndpointMode::Playback),
        "virtual_microphone" => Ok(EndpointMode::VirtualMicrophone),
        _ => Err(serde::de::Error::custom("unknown endpoint mode")),
    }
}

impl LaunchConfig {
    fn output_target(&self, sink: PortRef) -> io::Result<String> {
        match self.endpoint_mode {
            EndpointMode::Playback | EndpointMode::StereoCapture => {
                self.target.clone().ok_or_else(fail)
            }
            EndpointMode::VirtualMicrophone => {
                Ok(capyio_linux_audio::virtual_microphone_name(sink.port_id))
            }
        }
    }

    fn parse(bytes: &[u8]) -> io::Result<Self> {
        let value: Self = sanitized(serde_json::from_slice(bytes))?;
        if value.version != 1
            || value.node_id.as_uuid().is_nil()
            || value.source.node_id == value.node_id
            || value.source.node_id.as_uuid().is_nil()
            || value.source.capability_id.as_uuid().is_nil()
            || value.source.port_id.as_uuid().is_nil()
            || value.listen.ip() != value.media.ip()
        {
            return Err(fail());
        }
        match value.endpoint_mode {
            EndpointMode::Playback if value.target.is_none() => return Err(fail()),
            EndpointMode::VirtualMicrophone if value.target.is_some() => return Err(fail()),
            _ => {}
        }
        // Reuse the existing address/duration policy; no hardware opens. Virtual
        // mode cannot target another node and its actual name is generated later.
        let mut listen = value.listen;
        if listen.port() == 0 {
            listen.set_port(1);
        }
        sanitized(PlaybackConfig::parse(&[
            "--trusted-lab".into(),
            listen.to_string(),
            value.media.to_string(),
            value.duration_secs.to_string(),
            value
                .target
                .clone()
                .unwrap_or_else(|| "capyio.validation.only".into()),
        ]))?;
        Ok(value)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalCommand {
    version: u32,
    command: String,
    #[serde(default)]
    request_id: Option<MessageId>,
    #[serde(default)]
    route_id: Option<RouteId>,
}
impl LocalCommand {
    fn parse(bytes: &[u8]) -> io::Result<Self> {
        let command: Self = sanitized(serde_json::from_slice(bytes))?;
        if command.version != 1 {
            return Err(fail());
        }
        Ok(command)
    }
    fn simple(&self, name: &str) -> bool {
        self.command == name && self.request_id.is_none() && self.route_id.is_none()
    }
}

/// Explicit command entry only. Requires exclusive inherited FIFO stdin/stdout;
/// never loads a path, environment variable, credential store or user device.
/// All returned errors are static and do not contain input/config/key material.
pub fn run_trusted_lab_playback_stdio() -> io::Result<()> {
    run_stdio(false)
}
pub fn run_trusted_lab_capture_stdio() -> io::Result<()> {
    run_stdio(true)
}
fn run_stdio(capture: bool) -> io::Result<()> {
    let mut io = sanitized(LauncherIo::stdio())?;
    let result = launch(&mut io, capture);
    if result.is_err() {
        // Do not echo parser/TLS/runtime errors or secrets even on malformed input.
        let _ = io.send_json(
            &json!({"version":1,"event":"failed"}),
            Instant::now() + Duration::from_secs(1),
        );
    }
    result.map_err(|_| fail())
}

fn launch(io: &mut LauncherIo, capture: bool) -> io::Result<()> {
    let bootstrap_deadline = Instant::now() + BOOTSTRAP;
    let bytes = io.read_frame(LOCAL_FRAME, bootstrap_deadline)?;
    let config = if capture {
        parse_capture(bytes.as_slice())?
    } else {
        LaunchConfig::parse(bytes.as_slice())?
    };
    let directory_bytes = io.read_frame(65_536, bootstrap_deadline)?;
    let directory = sanitized(PairedPeerDirectory::decode_json(
        config.node_id,
        directory_bytes.as_slice(),
    ))?;
    drop(directory_bytes);
    let mut peers = directory.active_peers();
    let peer = peers.next().ok_or_else(fail)?;
    if peer.node_id != config.source.node_id
        || peers.next().is_some()
        || directory.peers().len() != 1
    {
        return Err(fail());
    }
    let credential_bytes = io.read_frame(65_536, bootstrap_deadline)?;
    let credentials = sanitized(ControlCredentialBundle::decode(credential_bytes.as_slice()))?;
    drop(credential_bytes); // Clear the sole encoded secret buffer immediately.
    let (mut runtime, sink) =
        local_runtime_endpoint(config.node_id, config.proposal_mode, config.endpoint_mode)?;
    let expectation = sanitized(ControlTlsNodeBinding::new(config.node_id, peer.node_id))?;
    let listener = sanitized(TcpListener::bind(config.listen))?;
    let identity = ControlTlsServerIdentity::from_bundle(peer.peer_certificate_sha256, credentials);
    let server = sanitized(AuthenticatedControlServer::from_listener_fresh_session(
        listener,
        identity,
        expectation,
        Duration::from_secs(1),
    ))?;
    io.send_json(&json!({"version":1,"event":"listening","address":sanitized(server.local_addr())?.to_string()}), Instant::now() + Duration::from_secs(1))?;
    let deadline = Instant::now() + ACCEPT;
    let channel = loop {
        check(deadline)?;
        if local_before_approval(io, "listening")? {
            return stopped(io);
        }
        if let Some(channel) = sanitized(
            server.try_accept_until(deadline.min(Instant::now() + Duration::from_secs(1))),
        )? {
            break channel;
        }
        thread::sleep(Duration::from_millis(2));
    };
    drop(server); // No second client, persistent listener or automatic reconnect.
    let mut link = sanitized(CatalogControlLink::admit_accepted_new_peer(
        &mut runtime,
        channel,
        peer.display_name.clone(),
    ))?;
    // All failures from here reconcile the exact authenticated Session.
    let prepared = prepare(io, &config, &mut runtime, &mut link, sink);
    let (route_id, admission) = match prepared {
        Ok(Some(value)) => value,
        Ok(None) => {
            let _ = link.close_and_reconcile(&mut runtime);
            return stopped(io);
        }
        Err(_) => {
            let _ = link.close_and_reconcile(&mut runtime);
            return Err(fail());
        }
    };
    let owner = sanitized(AuthenticatedPlaybackOwner::from_admitted_link_trusted_lab(
        runtime,
        link,
        admission,
        PlaybackTarget {
            local: config.media,
            duration: Duration::from_secs(config.duration_secs),
            pipewire_target: config.output_target(sink)?,
            endpoint_mode: config.endpoint_mode,
        },
    ))?;
    let (mut host, _stop) = LinuxPlaybackHost::from_admitted_trusted_lab(owner);
    io.send_json(
        &json!({"version":1,"event":"awaiting_peer_offer","route_id":route_id}),
        Instant::now() + Duration::from_secs(1),
    )?;
    let outcome = host.run_with_local_control(|owner| {
        let Some(bytes) = io.poll_frame(LOCAL_FRAME)? else { return Ok(false); };
        let command = LocalCommand::parse(bytes.as_slice())?;
        if command.simple("stop") { return Ok(true); }
        if !command.simple("status") { return Err(fail()); }
        let snapshot = owner.snapshot();
        let route = snapshot.routes.iter().find(|route| route.id == route_id).ok_or_else(fail)?;
        io.send_json(&json!({"version":1,"event":"status","route_id":route_id,"state":route.state,"packets_written":owner.metrics().map(|metrics|metrics.packets_written).unwrap_or(0),"packets_sent":owner.metrics().map(|metrics|metrics.packets_sent).unwrap_or(0)}), Instant::now() + Duration::from_secs(1))?;
        Ok(false)
    });
    sanitized(outcome)?;
    stopped(io)
}
fn stopped(io: &mut LauncherIo) -> io::Result<()> {
    io.send_json(
        &json!({"version":1,"event":"stopped"}),
        Instant::now() + Duration::from_secs(1),
    )
}
fn local_before_approval(io: &mut LauncherIo, stage: &str) -> io::Result<bool> {
    let Some(bytes) = io.poll_frame(LOCAL_FRAME)? else {
        return Ok(false);
    };
    let command = LocalCommand::parse(bytes.as_slice())?;
    if command.simple("stop") {
        return Ok(true);
    }
    if !command.simple("status") {
        return Err(fail());
    }
    io.send_json(
        &json!({"version":1,"event":"status","stage":stage}),
        Instant::now() + Duration::from_secs(1),
    )?;
    Ok(false)
}
fn prepare(
    io: &mut LauncherIo,
    config: &LaunchConfig,
    runtime: &mut NodeRuntime,
    link: &mut CatalogControlLink,
    sink: PortRef,
) -> io::Result<Option<(RouteId, PlaybackAdmission)>> {
    let deadline = Instant::now() + CATALOG;
    loop {
        check(deadline)?;
        if local_before_approval(io, "catalog")? {
            return Ok(None);
        }
        match sanitized(link.poll_control_message_until(deadline))? {
            Some(ReceivedControlMessage::Catalog(catalog)) => {
                sanitized(link.apply_catalog(runtime, catalog))?;
                break;
            }
            Some(_) => return Err(fail()),
            None => thread::sleep(Duration::from_millis(2)),
        }
    }
    // This set opts Sources into publication; Sink-only capabilities are
    // already included. Selecting the Sink here is a NotSourceCapability error.
    let capture = config.endpoint_mode == EndpointMode::StereoCapture;
    let selected = if capture {
        BTreeSet::from([sink.capability_id])
    } else {
        BTreeSet::new()
    };
    let catalog = sanitized(link.prepare_catalog(runtime, &selected))?;
    sanitized(link.send_catalog_until(catalog, deadline))?;
    let route_id = match config.proposal_mode {
        ProposalMode::LinuxLed => {
            if capture {
                validate_remote_endpoint(
                    runtime,
                    config.source,
                    PortDirection::Sink,
                    "pcm-s16le-48000-stereo",
                )?;
                sanitized(runtime.create_route(
                    link.session_id(),
                    sink,
                    config.source,
                    RouteBackend::AdapterManaged,
                ))?
            } else {
                prepare_selected_route(runtime, link.session_id(), config.source, sink)?
            }
        }
        ProposalMode::IncomingDraft => {
            let Some(route_id) = receive_pending_proposal(io, runtime, link, config.source, sink)?
            else {
                return Ok(None);
            };
            route_id
        }
    };
    let (route_source, route_sink) = if capture {
        (sink, config.source)
    } else {
        (config.source, sink)
    };
    let format = if capture {
        "pcm-s16le-48000-stereo"
    } else {
        "pcm-s16le-48000-mono"
    };
    let request_id = MessageId::new();
    let consent_deadline = Instant::now() + CONSENT;
    io.send_json(&json!({"version":1,"event":"approval_required","request_id":request_id,"route_id":route_id,
        "session_id":link.session_id(),"source":route_source,"sink":route_sink,"target":config.target,
        "endpoint_mode":config.endpoint_mode.name(),"node_name":if config.endpoint_mode==EndpointMode::VirtualMicrophone { Some(config.output_target(sink)?) } else { None },
        "format":format,"duration_secs":config.duration_secs,"approval_timeout_secs":CONSENT.as_secs(),"media_security":"unauthenticated_udp_trusted_lab_only"}), Instant::now() + Duration::from_secs(1))?;
    loop {
        check(consent_deadline)?;
        if let Some(bytes) = io.poll_frame(LOCAL_FRAME)? {
            let command = LocalCommand::parse(bytes.as_slice())?;
            if command.simple("stop") {
                return Ok(None);
            }
            if command.simple("status") {
                io.send_json(
                    &json!({"version":1,"event":"status","stage":"awaiting_local_approval"}),
                    Instant::now() + Duration::from_secs(1),
                )?;
            } else if command.command == "approve"
                && command.request_id == Some(request_id)
                && command.route_id == Some(route_id)
            {
                break;
            } else {
                return Err(fail());
            }
        }
        match sanitized(link.poll_control_message_until(
            consent_deadline.min(Instant::now() + Duration::from_secs(1)),
        ))? {
            Some(ReceivedControlMessage::Heartbeat(heartbeat)) => {
                if !heartbeat.acknowledgement {
                    sanitized(link.send_heartbeat_until(
                        heartbeat.sequence,
                        true,
                        consent_deadline.min(Instant::now() + Duration::from_secs(1)),
                    ))?;
                }
            }
            Some(ReceivedControlMessage::RouteStatus(status))
                if config.proposal_mode == ProposalMode::IncomingDraft =>
            {
                validate_draft_peer(runtime, route_id, &status)?;
                if status.route.authorization != AuthorizationState::Pending {
                    return Err(fail());
                }
            }
            Some(_) => return Err(fail()),
            None => {}
        }
        thread::sleep(Duration::from_millis(2));
    }
    check(consent_deadline)?;
    let approved_at = now_ms()?;
    let approved_expiry = approved_at
        .checked_add(config.duration_secs * 1000)
        .ok_or_else(fail)?;
    // Approval is route-specific and finite. Neither proposal mode grants the
    // remote microphone permission or acquires any lease before reciprocity.
    let negotiation_deadline =
        (Instant::now() + CONSENT).min(Instant::now() + Duration::from_secs(config.duration_secs));
    let expiry = match config.proposal_mode {
        ProposalMode::LinuxLed => negotiate_linux_led(
            io,
            runtime,
            link,
            route_id,
            config.source,
            negotiation_deadline,
            approved_expiry,
        )?,
        ProposalMode::IncomingDraft => negotiate_incoming(
            io,
            runtime,
            link,
            route_id,
            config.source,
            negotiation_deadline,
            approved_expiry,
        )?,
    };
    let Some(expiry) = expiry else {
        return Ok(None);
    };
    // Same one-second transit headroom as the existing Windows lease flow.
    // Too little remaining lifetime fails closed rather than extending consent.
    let ttl = lease_ttl(expiry, now_ms()?)?;
    let request = sanitized(link.send_port_lease_request_until(
        route_id,
        config.source,
        PortLeaseAction::Acquire,
        ttl,
        negotiation_operation_deadline(negotiation_deadline, expiry)?,
    ))?;
    let lease_expiry = loop {
        let Some(message) = negotiation_message(
            io,
            runtime,
            link,
            config.source,
            negotiation_deadline,
            expiry,
        )?
        else {
            return Ok(None);
        };
        match message {
            ReceivedControlMessage::RouteStatus(status) => {
                validate_draft_peer(runtime, route_id, &status)?;
                if status.route.authorization != sanitized(runtime.route(route_id))?.authorization {
                    return Err(fail());
                }
            }
            ReceivedControlMessage::PortLeaseResponse(response) => {
                let lease_expiry = response.expires_at_ms.ok_or_else(fail)?;
                if response.request_message_id != request
                    || response.session_id != link.session_id()
                    || response.route_id != route_id
                    || response.port != config.source
                    || response.outcome != PortLeaseOutcome::Acquired
                    || lease_expiry <= now_ms()?
                    || lease_expiry > expiry
                {
                    return Err(fail());
                }
                break lease_expiry;
            }
            _ => return Err(fail()),
        }
    };
    let now = now_ms()?;
    check(negotiation_deadline)?;
    sanitized(runtime.set_local_port_lease_policy(sink, PortLeasePolicy::Exclusive))?;
    sanitized(runtime.acquire_route_port_lease(route_id, sink, lease_expiry, now))?;
    sanitized(runtime.prepare_route(
        route_id,
        Some(FormatDescriptor::new(format)),
        QosMode::Interactive,
        now,
    ))?;
    sanitized(runtime.begin_route_start(route_id, now))?;
    Ok(Some((
        route_id,
        PlaybackAdmission {
            expected: link.tls_binding(),
            route_id,
            local_offer: NativeAudioDataOffer {
                port: config.media.port(),
                stream_id: StreamId::new(),
                endpoint: if capture {
                    NativeAudioEndpoint::Source
                } else {
                    NativeAudioEndpoint::Sink
                },
                preset: if capture {
                    NativeAudioPreset::PcmS16le48000Stereo
                } else {
                    NativeAudioPreset::PcmS16le48000Mono
                },
            },
        },
    )))
}

/// This one-shot receiver never accepts an arbitrary Route or grants local use
/// while importing a peer proposal. A fresh launcher approval is still required.
fn validate_pending_proposal(
    runtime: &NodeRuntime,
    session_id: SessionId,
    source: PortRef,
    sink: PortRef,
    status: &ValidatedRouteStatus,
) -> io::Result<()> {
    validate_source(runtime, source)?;
    let session = sanitized(runtime.session(session_id))?;
    let route = &status.route;
    if session.state != SessionState::Ready
        || session.local_node_id != sink.node_id
        || session.remote_node_id != source.node_id
        || !runtime.snapshot().routes.is_empty()
        || status.session_id != session_id
        || route.session_id != session_id
        || route.source != source
        || route.sink != sink
        || route.profile != ProfileId::audio_frames_v1()
        || route.backend != RouteBackend::AdapterManaged
        || route.state != RouteState::Draft
        || route.authorization != AuthorizationState::Pending
        || route.epoch != 0
        || route.selected_format.is_some()
        || route.selected_qos_mode.is_some()
        || route.compatible_formats != [FormatDescriptor::new("pcm-s16le-48000-mono")]
        || route.compatible_qos_modes != BTreeSet::from([QosMode::Interactive])
        || !route.diagnostic_ids.is_empty()
        || status.native_audio_data.is_some()
        || status.touchpad_data.is_some()
        || status.haptics_data.is_some()
        || status.camera_data.is_some()
    {
        return Err(fail());
    }
    Ok(())
}

fn receive_pending_proposal(
    io: &mut LauncherIo,
    runtime: &mut NodeRuntime,
    link: &mut CatalogControlLink,
    source: PortRef,
    sink: PortRef,
) -> io::Result<Option<RouteId>> {
    let deadline = Instant::now() + CONSENT;
    loop {
        check(deadline)?;
        if local_before_approval(io, "awaiting_incoming_draft")? {
            return Ok(None);
        }
        let step = deadline.min(Instant::now() + Duration::from_secs(1));
        match sanitized(link.poll_control_message_until(step))? {
            Some(ReceivedControlMessage::RouteStatus(status)) => {
                validate_pending_proposal(runtime, link.session_id(), source, sink, &status)?;
                let route = sanitized(link.apply_route_status(runtime, &status, false))?;
                return Ok(Some(route.id));
            }
            Some(ReceivedControlMessage::Catalog(catalog)) => {
                sanitized(link.apply_catalog(runtime, catalog))?;
                validate_source(runtime, source)?;
            }
            Some(ReceivedControlMessage::Heartbeat(heartbeat)) => {
                if !heartbeat.acknowledgement {
                    sanitized(link.send_heartbeat_until(heartbeat.sequence, true, step))?;
                }
            }
            Some(_) => return Err(fail()),
            None => thread::sleep(Duration::from_millis(2)),
        }
    }
}

/// The Android initiator waits for this independently approved Linux grant.
/// Waiting for the Android grant first would deadlock the incoming mode.
fn negotiate_incoming(
    io: &mut LauncherIo,
    runtime: &mut NodeRuntime,
    link: &mut CatalogControlLink,
    route_id: RouteId,
    source: PortRef,
    deadline: Instant,
    approved_expiry: u64,
) -> io::Result<Option<u64>> {
    validate_source(runtime, source)?;
    sanitized(runtime.authorize_route(route_id, Some(approved_expiry)))?;
    sanitized(link.send_route_status_until(
        sanitized(runtime.route(route_id))?,
        negotiation_operation_deadline(deadline, approved_expiry)?,
    ))?;
    loop {
        let Some(message) =
            negotiation_message(io, runtime, link, source, deadline, approved_expiry)?
        else {
            return Ok(None);
        };
        let ReceivedControlMessage::RouteStatus(status) = message else {
            return Err(fail());
        };
        if let Some(expiry) =
            accept_incoming_grant(runtime, route_id, &status, approved_expiry, now_ms()?)?
        {
            if expiry < approved_expiry {
                // Acknowledge the peer's already-held narrower finite grant.
                sanitized(link.send_route_status_until(
                    sanitized(runtime.route(route_id))?,
                    negotiation_operation_deadline(deadline, expiry)?,
                ))?;
            }
            return Ok(Some(expiry));
        }
    }
}

fn accept_incoming_grant(
    runtime: &mut NodeRuntime,
    route_id: RouteId,
    status: &ValidatedRouteStatus,
    approved_expiry: u64,
    now: u64,
) -> io::Result<Option<u64>> {
    validate_draft_peer(runtime, route_id, status)?;
    if sanitized(runtime.route(route_id))?.authorization
        != (AuthorizationState::Authorized {
            expires_at_ms: Some(approved_expiry),
        })
        || now >= approved_expiry
    {
        return Err(fail());
    }
    match status.route.authorization {
        AuthorizationState::Pending => Ok(None),
        AuthorizationState::Authorized {
            expires_at_ms: Some(expiry),
        } if expiry > now && expiry <= approved_expiry => {
            if expiry < approved_expiry {
                sanitized(runtime.restrict_draft_authorization(route_id, expiry, now))?;
            }
            Ok(Some(expiry))
        }
        _ => Err(fail()),
    }
}

fn negotiate_linux_led(
    io: &mut LauncherIo,
    runtime: &mut NodeRuntime,
    link: &mut CatalogControlLink,
    route_id: RouteId,
    source: PortRef,
    negotiation_deadline: Instant,
    approved_expiry: u64,
) -> io::Result<Option<u64>> {
    sanitized(link.send_route_status_until(
        sanitized(runtime.route(route_id))?,
        negotiation_operation_deadline(negotiation_deadline, approved_expiry)?,
    ))?;
    let peer_expiry = loop {
        let Some(message) = negotiation_message(
            io,
            runtime,
            link,
            source,
            negotiation_deadline,
            approved_expiry,
        )?
        else {
            return Ok(None);
        };
        let ReceivedControlMessage::RouteStatus(status) = message else {
            return Err(fail());
        };
        validate_draft_peer(runtime, route_id, &status)?;
        match status.route.authorization {
            AuthorizationState::Pending => {}
            AuthorizationState::Authorized {
                expires_at_ms: Some(expiry),
            } if expiry > now_ms()? => break expiry,
            _ => return Err(fail()),
        }
    };
    let expiry = approved_expiry.min(peer_expiry);
    sanitized(runtime.authorize_route(route_id, Some(expiry)))?;
    sanitized(link.send_route_status_until(
        sanitized(runtime.route(route_id))?,
        negotiation_operation_deadline(negotiation_deadline, expiry)?,
    ))?;
    // Android retains its independently obtained consent and can only narrow
    // its Draft grant to this deadline. An acknowledgement is not a new grant.
    loop {
        let Some(message) =
            negotiation_message(io, runtime, link, source, negotiation_deadline, expiry)?
        else {
            return Ok(None);
        };
        let ReceivedControlMessage::RouteStatus(status) = message else {
            return Err(fail());
        };
        validate_draft_peer(runtime, route_id, &status)?;
        match status.route.authorization {
            AuthorizationState::Authorized {
                expires_at_ms: Some(peer),
            } if peer == expiry => break,
            AuthorizationState::Authorized {
                expires_at_ms: Some(peer),
            } if peer == peer_expiry => {}
            _ => return Err(fail()),
        }
    }
    Ok(Some(expiry))
}

fn lease_ttl(expiry: u64, now: u64) -> io::Result<u64> {
    let ttl = expiry.saturating_sub(now).saturating_sub(1_000);
    if ttl < MIN_PORT_LEASE_TTL_MS {
        return Err(fail());
    }
    Ok(ttl.min(capyio_protocol::MAX_PORT_LEASE_TTL_MS))
}

fn negotiation_operation_deadline(deadline: Instant, expiry: u64) -> io::Result<Instant> {
    check(deadline)?;
    let remaining = expiry
        .checked_sub(now_ms()?)
        .filter(|value| *value > 0)
        .ok_or_else(fail)?;
    Ok(deadline.min(Instant::now() + Duration::from_millis(remaining.min(1_000))))
}

fn validate_draft_peer(
    runtime: &NodeRuntime,
    route_id: RouteId,
    status: &ValidatedRouteStatus,
) -> io::Result<()> {
    let local = sanitized(runtime.route(route_id))?;
    let peer = &status.route;
    if status.session_id != local.session_id
        || peer.session_id != local.session_id
        || peer.id != local.id
        || peer.source != local.source
        || peer.sink != local.sink
        || peer.profile != local.profile
        || peer.backend != local.backend
        || peer.state != RouteState::Draft
        || peer.epoch != 0
        || peer.selected_format.is_some()
        || peer.selected_qos_mode.is_some()
        || status.native_audio_data.is_some()
        || status.touchpad_data.is_some()
        || status.haptics_data.is_some()
        || status.camera_data.is_some()
    {
        return Err(fail());
    }
    Ok(())
}

/// Progress one bounded authenticated exchange at a time, keeping local Stop
/// and finite consent live during peer interaction. Catalogs are applied through
/// the real transactional admission path, never treated as permission.
fn negotiation_message(
    io: &mut LauncherIo,
    runtime: &mut NodeRuntime,
    link: &mut CatalogControlLink,
    source: PortRef,
    deadline: Instant,
    expiry: u64,
) -> io::Result<Option<ReceivedControlMessage>> {
    loop {
        let operation_deadline = negotiation_operation_deadline(deadline, expiry)?;
        if local_before_approval(io, "awaiting_peer_consent_or_lease")? {
            return Ok(None);
        }
        match sanitized(link.poll_control_message_until(operation_deadline))? {
            Some(ReceivedControlMessage::Catalog(catalog)) => {
                sanitized(link.apply_catalog(runtime, catalog))?;
                validate_route_remote(runtime, source)?;
            }
            Some(ReceivedControlMessage::Heartbeat(heartbeat)) => {
                if !heartbeat.acknowledgement {
                    sanitized(link.send_heartbeat_until(
                        heartbeat.sequence,
                        true,
                        negotiation_operation_deadline(deadline, expiry)?,
                    ))?;
                }
            }
            Some(message) => return Ok(Some(message)),
            None => thread::sleep(Duration::from_millis(2)),
        }
    }
}

fn prepare_selected_route(
    runtime: &mut NodeRuntime,
    session_id: SessionId,
    source_ref: PortRef,
    sink: PortRef,
) -> io::Result<RouteId> {
    validate_source(runtime, source_ref)?;
    sanitized(runtime.create_route(session_id, source_ref, sink, RouteBackend::AdapterManaged))
}

fn validate_route_remote(runtime: &NodeRuntime, remote: PortRef) -> io::Result<()> {
    let snapshot = runtime.snapshot();
    let mut routes = snapshot
        .routes
        .iter()
        .filter(|r| r.source == remote || r.sink == remote);
    let route = routes.next().ok_or_else(fail)?;
    if routes.next().is_some() || remote.node_id == snapshot.local_node.id {
        return Err(fail());
    }
    if route.source == remote {
        validate_source(runtime, remote)
    } else {
        validate_remote_endpoint(
            runtime,
            remote,
            PortDirection::Sink,
            "pcm-s16le-48000-stereo",
        )
    }
}

fn validate_source(runtime: &NodeRuntime, source_ref: PortRef) -> io::Result<()> {
    validate_remote_endpoint(
        runtime,
        source_ref,
        PortDirection::Source,
        "pcm-s16le-48000-mono",
    )
}
fn validate_remote_endpoint(
    runtime: &NodeRuntime,
    source_ref: PortRef,
    direction: PortDirection,
    format: &str,
) -> io::Result<()> {
    // Real authenticated catalog descriptors, not launch-config/fixture ports.
    let snapshot = runtime.snapshot();
    let peer = snapshot
        .peers
        .iter()
        .find(|peer| peer.id == source_ref.node_id)
        .ok_or_else(fail)?;
    let source = sanitized(peer.port(source_ref.capability_id, source_ref.port_id))?;
    if source.direction != direction
        || source.profile != ProfileId::audio_frames_v1()
        || source.availability != Availability::Available
        || !source.formats.contains(&FormatDescriptor::new(format))
        || !source.qos_modes.contains(&QosMode::Interactive)
    {
        return Err(fail());
    }
    Ok(())
}

/// A single explicit software playback destination, not a hardware probe or
/// fabricated remote catalog. IDs are ephemeral per invocation and advertised.
#[cfg(test)]
fn local_runtime(node_id: NodeId) -> io::Result<(NodeRuntime, PortRef)> {
    local_runtime_mode(node_id, ProposalMode::LinuxLed)
}

#[cfg(test)]
fn local_runtime_mode(node_id: NodeId, mode: ProposalMode) -> io::Result<(NodeRuntime, PortRef)> {
    local_runtime_endpoint(node_id, mode, EndpointMode::Playback)
}

fn local_runtime_endpoint(
    node_id: NodeId,
    mode: ProposalMode,
    endpoint: EndpointMode,
) -> io::Result<(NodeRuntime, PortRef)> {
    if endpoint == EndpointMode::StereoCapture && mode != ProposalMode::LinuxLed {
        return Err(fail());
    }
    let capture = endpoint == EndpointMode::StereoCapture;
    let mut node = NodeDescriptor::new(
        node_id,
        if capture {
            "Linux explicit stereo capture"
        } else if endpoint == EndpointMode::VirtualMicrophone {
            "Linux virtual microphone"
        } else {
            "Linux lab playback"
        },
        Platform::Linux,
        "not probed",
        env!("CARGO_PKG_VERSION"),
        [ProtocolVersion::new(1, 1)],
    );
    let adapter = AdapterInstanceId::new();
    let capability = CapabilityId::new();
    let port = PortId::new();
    sanitized(
        node.add_adapter(AdapterInstanceDescriptor {
            id: adapter,
            adapter_type: "capyio.linux.lab-playback".into(),
            display_name: if endpoint == EndpointMode::VirtualMicrophone {
                "Owned PipeWire virtual microphone"
            } else {
                "Explicit lab PCM playback"
            }
            .into(),
            deployment_mode: AdapterDeploymentMode::Sidecar,
            version: env!("CARGO_PKG_VERSION").into(),
            state: AdapterState::Ready,
            health: AdapterHealth::Unknown,
            owned_capabilities: BTreeSet::new(),
            supported_route_modes: BTreeSet::from([RouteBackend::AdapterManaged]),
        }),
    )?;
    let descriptor = PortDescriptor {
        id: port,
        capability_id: capability,
        display_name: if capture {
            "Selected stereo capture Source"
        } else if endpoint == EndpointMode::VirtualMicrophone {
            "Virtual microphone network Sink"
        } else {
            "Explicit playback Sink"
        }
        .into(),
        direction: if capture {
            PortDirection::Source
        } else {
            PortDirection::Sink
        },
        profile: ProfileId::audio_frames_v1(),
        schema_id: None,
        formats: vec![FormatDescriptor::new(if capture {
            "pcm-s16le-48000-stereo"
        } else {
            "pcm-s16le-48000-mono"
        })],
        qos_modes: BTreeSet::from([QosMode::Interactive]),
        clock_domain: None,
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::None,
        interoperability_mode: InteroperabilityMode::AdapterManaged,
    };
    sanitized(
        node.add_capability(CapabilityDescriptor {
            id: capability,
            adapter_instance_id: adapter,
            display_name: if capture {
                "Selected PipeWire stereo capture"
            } else if endpoint == EndpointMode::VirtualMicrophone {
                "PipeWire virtual microphone"
            } else {
                "Explicit lab playback"
            }
            .into(),
            class: if endpoint == EndpointMode::VirtualMicrophone {
                CapabilityClass::Microphone
            } else {
                CapabilityClass::Speaker
            },
            availability: Availability::Available,
            permission_requirement: PermissionRequirement::None,
            metadata: if mode == ProposalMode::IncomingDraft {
                BTreeMap::from([(
                    capyio_protocol::NATIVE_AUDIO_INBOUND_DRAFT_METADATA_KEY.into(),
                    capyio_protocol::NATIVE_AUDIO_INBOUND_DRAFT_MONO_V1.into(),
                )])
            } else {
                BTreeMap::new()
            },
            ports: BTreeMap::from([(port, descriptor)]),
        }),
    )?;
    Ok((
        sanitized(NodeRuntime::new(node))?,
        PortRef {
            node_id,
            capability_id: capability,
            port_id: port,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> serde_json::Value {
        json!({"version":1,"node_id":NodeId::new(),"source":PortRef{node_id:NodeId::new(),capability_id:CapabilityId::new(),port_id:PortId::new()},"listen":"127.0.0.1:0","media":"127.0.0.1:40211","target":"explicit.test","duration_secs":10})
    }
    #[test]
    fn capture_launch_has_distinct_explicit_schema_and_published_stereo_source() {
        let original = config();
        assert!(parse_capture(&serde_json::to_vec(&original).unwrap()).is_err());
        let mut value = original.clone();
        value["sink"] = value["source"].clone();
        value.as_object_mut().unwrap().remove("source");
        let parsed = parse_capture(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(parsed.endpoint_mode, EndpointMode::StereoCapture);
        let (runtime, port) =
            local_runtime_endpoint(parsed.node_id, ProposalMode::LinuxLed, parsed.endpoint_mode)
                .unwrap();
        let snapshot = runtime.snapshot();
        let capability = &snapshot.local_node.capabilities[&port.capability_id];
        assert_eq!(capability.class, CapabilityClass::Speaker);
        assert_eq!(
            capability.ports[&port.port_id].direction,
            PortDirection::Source
        );
        assert!(
            capability.ports[&port.port_id]
                .formats
                .contains(&FormatDescriptor::new("pcm-s16le-48000-stereo"))
        );
        assert!(capability.metadata.is_empty());
        let published = capyio_control_host::project_publication_catalog(
            &snapshot.local_node,
            &BTreeSet::from([port.capability_id]),
        )
        .unwrap();
        assert!(published.capabilities.contains_key(&port.capability_id));
        assert!(snapshot.routes.is_empty());
        assert!(snapshot.port_leases.is_empty());
        assert!(
            local_runtime_endpoint(
                parsed.node_id,
                ProposalMode::IncomingDraft,
                EndpointMode::StereoCapture
            )
            .is_err()
        );
        for (key, bad) in [
            ("target", json!("auto")),
            ("target", json!("AUTO")),
            ("target", json!("default")),
            ("target", json!("Default")),
            ("target", json!("0")),
            ("target", json!(null)),
            ("duration_secs", json!(0)),
            ("proposal_mode", json!("incoming_draft")),
            ("endpoint_mode", json!("stereo_capture")),
        ] {
            let mut candidate = value.clone();
            candidate[key] = bad;
            assert!(parse_capture(&serde_json::to_vec(&candidate).unwrap()).is_err());
        }
        let mut candidate = original;
        candidate["endpoint_mode"] = json!("stereo_capture");
        assert!(LaunchConfig::parse(&serde_json::to_vec(&candidate).unwrap()).is_err());
    }

    #[test]
    fn virtual_microphone_mode_is_explicit_owned_network_sink_and_not_permission() {
        let mut value = config();
        assert_eq!(
            LaunchConfig::parse(&serde_json::to_vec(&value).unwrap())
                .unwrap()
                .endpoint_mode,
            EndpointMode::Playback
        );
        value["endpoint_mode"] = json!("virtual_microphone");
        assert!(LaunchConfig::parse(&serde_json::to_vec(&value).unwrap()).is_err());
        value.as_object_mut().unwrap().remove("target");
        for proposal in [ProposalMode::LinuxLed, ProposalMode::IncomingDraft] {
            let parsed = LaunchConfig::parse(&serde_json::to_vec(&value).unwrap()).unwrap();
            let (runtime, sink) =
                local_runtime_endpoint(parsed.node_id, proposal, parsed.endpoint_mode).unwrap();
            let (other, other_sink) =
                local_runtime_endpoint(parsed.node_id, proposal, parsed.endpoint_mode).unwrap();
            let snapshot = runtime.snapshot();
            assert_ne!(
                parsed.output_target(sink).unwrap(),
                parsed.output_target(other_sink).unwrap()
            );
            assert_eq!(
                parsed.output_target(sink).unwrap(),
                capyio_linux_audio::virtual_microphone_name(sink.port_id)
            );
            assert_eq!(
                snapshot.local_node.capabilities[&sink.capability_id].class,
                CapabilityClass::Microphone
            );
            assert_eq!(
                snapshot
                    .local_node
                    .port(sink.capability_id, sink.port_id)
                    .unwrap()
                    .direction,
                PortDirection::Sink
            );
            assert!(snapshot.routes.is_empty());
            assert!(snapshot.sessions.is_empty());
            assert!(snapshot.port_leases.is_empty());
            assert!(other.snapshot().routes.is_empty());
        }
        value["endpoint_mode"] = json!("unknown");
        assert!(LaunchConfig::parse(&serde_json::to_vec(&value).unwrap()).is_err());
        value["endpoint_mode"] = json!("playback");
        assert!(LaunchConfig::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn launch_config_is_explicit_finite_and_not_a_secret_error_or_authorization() {
        let valid = config();
        let parsed = LaunchConfig::parse(&serde_json::to_vec(&valid).unwrap()).unwrap();
        let (runtime, sink) = local_runtime(parsed.node_id).unwrap();
        let snapshot = runtime.snapshot();
        assert!(snapshot.routes.is_empty());
        assert!(snapshot.sessions.is_empty());
        assert!(snapshot.port_leases.is_empty());
        assert_eq!(sink.node_id, parsed.node_id);
        for (field, value) in [
            ("duration_secs", json!(0)),
            ("duration_secs", json!(301)),
            ("target", json!("auto")),
            ("listen", json!("0.0.0.0:40210")),
            ("media", json!("8.8.8.8:40211")),
            ("version", json!(2)),
            ("credential", json!("PRIVATE_SECRET_MARKER")),
        ] {
            let mut candidate = valid.clone();
            candidate[field] = value;
            let error = LaunchConfig::parse(&serde_json::to_vec(&candidate).unwrap())
                .err()
                .unwrap();
            assert_eq!(error.to_string(), "Linux lab playback launch failed");
            assert!(!error.to_string().contains("PRIVATE_SECRET_MARKER"));
        }
    }
    #[test]
    fn negotiation_lease_budget_never_extends_finite_local_approval() {
        assert!(lease_ttl(1_000, 1_000).is_err());
        assert!(lease_ttl(2_999, 1_000).is_err());
        assert_eq!(lease_ttl(3_000, 1_000).unwrap(), 1_000);
        assert_eq!(lease_ttl(301_000, 1_000).unwrap(), 299_000);
        assert!(negotiation_operation_deadline(Instant::now(), u64::MAX).is_err());
        assert!(
            negotiation_operation_deadline(Instant::now() + Duration::from_secs(1), 1).is_err()
        );
    }

    #[test]
    fn local_sink_catalog_requires_no_source_publication_selection() {
        let (runtime, sink) = local_runtime(NodeId::new()).unwrap();
        let local = runtime.snapshot().local_node;
        let published =
            capyio_control_host::project_publication_catalog(&local, &BTreeSet::new()).unwrap();
        assert_eq!(
            published
                .port(sink.capability_id, sink.port_id)
                .unwrap()
                .direction,
            PortDirection::Sink
        );
        assert!(matches!(
            capyio_control_host::project_publication_catalog(&local, &BTreeSet::from([sink.capability_id])),
            Err(capyio_control_host::PublicationError::NotSourceCapability(id)) if id == sink.capability_id
        ));
    }
    #[test]
    fn preconsent_requires_available_source_and_keeps_route_unauthorized() {
        let lab = capyio_testkit::DemoLab::new().unwrap();
        let source = lab
            .runtime
            .route(lab.routes.phone_microphone_to_windows)
            .unwrap()
            .source;
        let mut peer = lab
            .runtime
            .snapshot()
            .peers
            .into_iter()
            .find(|peer| peer.id == source.node_id)
            .unwrap();
        for adapter in peer.adapter_instances.values_mut() {
            adapter
                .supported_route_modes
                .insert(RouteBackend::AdapterManaged);
        }
        for capability in peer.capabilities.values_mut() {
            for port in capability.ports.values_mut() {
                port.interoperability_mode = InteroperabilityMode::AdapterManaged;
            }
        }
        assert_eq!(
            peer.port(source.capability_id, source.port_id)
                .unwrap()
                .availability,
            Availability::PermissionRequired
        );
        let (mut runtime, sink) = local_runtime(NodeId::new()).unwrap();
        runtime.register_peer(peer.clone(), true).unwrap();
        let session = runtime.open_session(peer.id).unwrap();
        assert!(prepare_selected_route(&mut runtime, session, source, sink).is_err());
        assert!(runtime.snapshot().routes.is_empty());
        assert!(runtime.snapshot().port_leases.is_empty());
        // Only this synthetic fixture simulates a remote granted capture state.
        let capability = peer.capabilities.get_mut(&source.capability_id).unwrap();
        capability.availability = Availability::Available;
        capability
            .ports
            .get_mut(&source.port_id)
            .unwrap()
            .availability = Availability::Available;
        let (mut runtime, sink) = local_runtime(NodeId::new()).unwrap();
        runtime.register_peer(peer, true).unwrap();
        let session = runtime.open_session(source.node_id).unwrap();
        let route = prepare_selected_route(&mut runtime, session, source, sink).unwrap();
        assert_eq!(runtime.route(route).unwrap().state, RouteState::Draft);
        assert_eq!(
            runtime.route(route).unwrap().authorization,
            AuthorizationState::Pending
        );
        assert!(runtime.snapshot().port_leases.is_empty());
        let draft = runtime.route(route).unwrap().clone();
        let bytes = capyio_protocol::encode_route_status(session, &draft).unwrap();
        let status = capyio_protocol::decode_route_status(&bytes).unwrap();
        validate_draft_peer(&runtime, route, &status).unwrap();
        for mutate in [0, 1, 2, 3, 4] {
            let mut wrong = status.clone();
            match mutate {
                0 => wrong.route.id = RouteId::new(),
                1 => wrong.session_id = SessionId::new(),
                2 => wrong.route.source.node_id = NodeId::new(),
                3 => wrong.route.epoch = 1,
                _ => wrong.route.state = RouteState::Starting,
            }
            assert!(validate_draft_peer(&runtime, route, &wrong).is_err());
        }
        // Existing explicit local approval operations must admit this exact pair.
        runtime
            .set_local_port_lease_policy(sink, PortLeasePolicy::Exclusive)
            .unwrap();
        runtime.authorize_route(route, Some(11_000)).unwrap();
        runtime
            .acquire_route_port_lease(route, sink, 11_000, 1_000)
            .unwrap();
        runtime
            .prepare_route(
                route,
                Some(FormatDescriptor::new("pcm-s16le-48000-mono")),
                QosMode::Interactive,
                1_000,
            )
            .unwrap();
        runtime.begin_route_start(route, 1_000).unwrap();
        assert_eq!(runtime.route(route).unwrap().state, RouteState::Starting);
    }
    #[test]
    fn local_command_rejects_unknown_fields_and_keeps_stop_separate_from_approval() {
        let request = MessageId::new();
        let route = RouteId::new();
        let approve = LocalCommand::parse(
            &serde_json::to_vec(
                &json!({"version":1,"command":"approve","request_id":request,"route_id":route}),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!approve.simple("stop"));
        assert_eq!(approve.request_id, Some(request));
        assert_eq!(approve.route_id, Some(route));
        let stop = LocalCommand::parse(br#"{"version":1,"command":"stop"}"#).unwrap();
        assert!(stop.simple("stop"));
        for input in [
            br#"{"version":1,"command":"stop","secret":"PRIVATE_SECRET_MARKER"}"#.as_slice(),
            br#"{"version":2,"command":"stop"}"#,
            br#"PRIVATE_SECRET_MARKER"#,
        ] {
            let error = LocalCommand::parse(input).err().unwrap();
            assert_eq!(error.to_string(), "Linux lab playback launch failed");
        }
    }
}

#[cfg(test)]
mod incoming_tests {
    use super::*;

    fn fixture() -> (NodeRuntime, PortRef, PortRef, ValidatedRouteStatus) {
        let lab = capyio_testkit::DemoLab::new().unwrap();
        let source = lab
            .runtime
            .route(lab.routes.phone_microphone_to_windows)
            .unwrap()
            .source;
        let mut peer = lab
            .runtime
            .snapshot()
            .peers
            .into_iter()
            .find(|peer| peer.id == source.node_id)
            .unwrap();
        for adapter in peer.adapter_instances.values_mut() {
            adapter
                .supported_route_modes
                .insert(RouteBackend::AdapterManaged);
        }
        let cap = peer.capabilities.get_mut(&source.capability_id).unwrap();
        cap.availability = Availability::Available;
        let port = cap.ports.get_mut(&source.port_id).unwrap();
        port.availability = Availability::Available;
        port.interoperability_mode = InteroperabilityMode::AdapterManaged;
        let (mut runtime, sink) =
            local_runtime_mode(NodeId::new(), ProposalMode::IncomingDraft).unwrap();
        runtime.register_peer(peer.clone(), true).unwrap();
        let session = runtime.open_session(peer.id).unwrap();
        let local = runtime.snapshot().local_node;
        let source_cap = &peer.capabilities[&source.capability_id];
        let sink_cap = &local.capabilities[&sink.capability_id];
        let route = Route::new(
            RouteId::new(),
            session,
            RouteEndpoint::new(
                source,
                &source_cap.ports[&source.port_id],
                &peer.adapter_instances[&source_cap.adapter_instance_id],
            ),
            RouteEndpoint::new(
                sink,
                &sink_cap.ports[&sink.port_id],
                &local.adapter_instances[&sink_cap.adapter_instance_id],
            ),
            RouteBackend::AdapterManaged,
        )
        .unwrap();
        let status = capyio_protocol::decode_route_status(
            &capyio_protocol::encode_route_status(session, &route).unwrap(),
        )
        .unwrap();
        (runtime, source, sink, status)
    }

    #[test]
    fn incoming_contract_is_explicit_and_not_default() {
        for mode in [ProposalMode::LinuxLed, ProposalMode::IncomingDraft] {
            let (runtime, sink) = local_runtime_mode(NodeId::new(), mode).unwrap();
            let snapshot = runtime.snapshot();
            let metadata = &snapshot.local_node.capabilities[&sink.capability_id].metadata;
            assert_eq!(
                metadata
                    .get(capyio_protocol::NATIVE_AUDIO_INBOUND_DRAFT_METADATA_KEY)
                    .map(String::as_str),
                (mode == ProposalMode::IncomingDraft)
                    .then_some(capyio_protocol::NATIVE_AUDIO_INBOUND_DRAFT_MONO_V1)
            );
            assert!(snapshot.routes.is_empty());
            assert!(snapshot.port_leases.is_empty());
        }
        let config = json!({"version":1,"node_id":NodeId::new(),"source":PortRef { node_id: NodeId::new(), capability_id: CapabilityId::new(), port_id: PortId::new() },"listen":"127.0.0.1:0","media":"127.0.0.1:40211","target":"explicit.test","duration_secs":10});
        assert!(
            LaunchConfig::parse(&serde_json::to_vec(&config).unwrap())
                .unwrap()
                .proposal_mode
                == ProposalMode::LinuxLed
        );
        let mut unknown = config.clone();
        unknown["proposal_mode"] = json!("automatic");
        assert!(LaunchConfig::parse(&serde_json::to_vec(&unknown).unwrap()).is_err());
    }

    #[test]
    fn incoming_proposal_is_exact_pending_fresh_and_does_not_authorize() {
        let (mut runtime, source, sink, status) = fixture();
        validate_pending_proposal(&runtime, status.session_id, source, sink, &status).unwrap();
        assert!(runtime.snapshot().routes.is_empty());
        assert!(runtime.snapshot().port_leases.is_empty());
        for change in 0..14 {
            let mut wrong = status.clone();
            match change {
                0 => wrong.session_id = SessionId::new(),
                1 => wrong.route.session_id = SessionId::new(),
                2 => wrong.route.source.port_id = PortId::new(),
                3 => wrong.route.sink.port_id = PortId::new(),
                4 => wrong.route.epoch = 1,
                5 => {
                    wrong.route.authorization = AuthorizationState::Authorized {
                        expires_at_ms: Some(u64::MAX),
                    }
                }
                6 => wrong.route.state = RouteState::Starting,
                7 => wrong.route.profile = ProfileId::video_frames_v1(),
                8 => wrong.route.backend = RouteBackend::CapyDataPlane,
                9 => {
                    wrong.route.selected_format =
                        Some(FormatDescriptor::new("pcm-s16le-48000-mono"))
                }
                10 => wrong.route.selected_qos_mode = Some(QosMode::Interactive),
                11 => wrong.route.compatible_formats.clear(),
                12 => wrong.route.compatible_qos_modes.clear(),
                _ => {
                    wrong.native_audio_data = Some(Box::new(NativeAudioDataOffer {
                        port: 40211,
                        stream_id: StreamId::new(),
                        endpoint: NativeAudioEndpoint::Source,
                        preset: NativeAudioPreset::PcmS16le48000Mono,
                    }))
                }
            }
            assert!(
                validate_pending_proposal(&runtime, status.session_id, source, sink, &wrong)
                    .is_err(),
                "mutation {change}"
            );
            assert!(runtime.snapshot().routes.is_empty());
            assert!(runtime.snapshot().port_leases.is_empty());
        }
        runtime
            .create_route_with_id(
                status.route.id,
                status.session_id,
                source,
                sink,
                RouteBackend::AdapterManaged,
            )
            .unwrap();
        // The one-shot receiver never imports a second proposal or replays one.
        assert!(
            validate_pending_proposal(&runtime, status.session_id, source, sink, &status).is_err()
        );
        assert_eq!(
            runtime.route(status.route.id).unwrap().authorization,
            AuthorizationState::Pending
        );
    }

    #[test]
    fn incoming_peer_grant_only_matches_or_narrows_existing_local_approval() {
        for peer_expiry in [None, Some(1_000), Some(11_001), Some(10_000), Some(11_000)] {
            let (mut runtime, source, sink, mut status) = fixture();
            runtime
                .create_route_with_id(
                    status.route.id,
                    status.session_id,
                    source,
                    sink,
                    RouteBackend::AdapterManaged,
                )
                .unwrap();
            runtime
                .authorize_route(status.route.id, Some(11_000))
                .unwrap();
            status.route.authorization = AuthorizationState::Authorized {
                expires_at_ms: peer_expiry,
            };
            let result =
                accept_incoming_grant(&mut runtime, status.route.id, &status, 11_000, 1_000);
            if matches!(peer_expiry, Some(10_000 | 11_000)) {
                assert_eq!(result.unwrap(), peer_expiry);
                assert_eq!(
                    runtime.route(status.route.id).unwrap().authorization,
                    status.route.authorization
                );
            } else {
                assert!(result.is_err());
            }
            assert!(runtime.snapshot().port_leases.is_empty());
            assert_eq!(
                runtime.route(status.route.id).unwrap().state,
                RouteState::Draft
            );
        }
        let (mut runtime, source, sink, status) = fixture();
        runtime
            .create_route_with_id(
                status.route.id,
                status.session_id,
                source,
                sink,
                RouteBackend::AdapterManaged,
            )
            .unwrap();
        // A peer acknowledgement is never a substitute for local approval.
        assert!(
            accept_incoming_grant(&mut runtime, status.route.id, &status, 11_000, 1_000).is_err()
        );
        runtime
            .authorize_route(status.route.id, Some(11_000))
            .unwrap();
        assert!(
            accept_incoming_grant(&mut runtime, status.route.id, &status, 11_000, 11_000).is_err()
        );
    }
}
