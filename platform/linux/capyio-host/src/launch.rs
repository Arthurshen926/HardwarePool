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
    AuthenticatedPlaybackOwner, PlaybackAdmission, PlaybackConfig, PlaybackTarget,
};
use capyio_pairing::PairedPeerDirectory;
use capyio_protocol::{NativeAudioDataOffer, NativeAudioEndpoint, NativeAudioPreset};
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LaunchConfig {
    version: u32,
    node_id: NodeId,
    source: PortRef,
    listen: SocketAddrV4,
    media: SocketAddrV4,
    target: String,
    duration_secs: u64,
}
impl LaunchConfig {
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
        // Reuse the existing exact target/address/duration policy; no hardware opens.
        let mut listen = value.listen;
        if listen.port() == 0 {
            listen.set_port(1);
        }
        sanitized(PlaybackConfig::parse(&[
            "--trusted-lab".into(),
            listen.to_string(),
            value.media.to_string(),
            value.duration_secs.to_string(),
            value.target.clone(),
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
    let mut io = sanitized(LauncherIo::stdio())?;
    let result = launch(&mut io);
    if result.is_err() {
        // Do not echo parser/TLS/runtime errors or secrets even on malformed input.
        let _ = io.send_json(
            &json!({"version":1,"event":"failed"}),
            Instant::now() + Duration::from_secs(1),
        );
    }
    result.map_err(|_| fail())
}

fn launch(io: &mut LauncherIo) -> io::Result<()> {
    let bootstrap_deadline = Instant::now() + BOOTSTRAP;
    let config = LaunchConfig::parse(io.read_frame(LOCAL_FRAME, bootstrap_deadline)?.as_slice())?;
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
    let (mut runtime, sink) = local_runtime(config.node_id)?;
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
            pipewire_target: config.target,
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
        io.send_json(&json!({"version":1,"event":"status","route_id":route_id,"state":route.state,"packets_written":owner.metrics().map(|metrics|metrics.packets_written).unwrap_or(0)}), Instant::now() + Duration::from_secs(1))?;
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
    let selected = BTreeSet::new();
    let catalog = sanitized(link.prepare_catalog(runtime, &selected))?;
    sanitized(link.send_catalog_until(catalog, deadline))?;
    let route_id = prepare_selected_route(runtime, link.session_id(), config.source, sink)?;
    let request_id = MessageId::new();
    let consent_deadline = Instant::now() + CONSENT;
    io.send_json(&json!({"version":1,"event":"approval_required","request_id":request_id,"route_id":route_id,
        "session_id":link.session_id(),"source":config.source,"sink":sink,"target":config.target,
        "format":"pcm-s16le-48000-mono","duration_secs":config.duration_secs,"approval_timeout_secs":CONSENT.as_secs(),"media_security":"unauthenticated_udp_trusted_lab_only"}), Instant::now() + Duration::from_secs(1))?;
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
            Some(_) => return Err(fail()),
            None => {}
        }
        thread::sleep(Duration::from_millis(2));
    }
    check(consent_deadline)?;
    let now = now_ms()?;
    let expiry = now
        .checked_add(config.duration_secs * 1000)
        .ok_or_else(fail)?;
    sanitized(runtime.set_local_port_lease_policy(sink, PortLeasePolicy::Exclusive))?;
    sanitized(runtime.authorize_route(route_id, Some(expiry)))?;
    sanitized(runtime.acquire_route_port_lease(route_id, sink, expiry, now))?;
    sanitized(runtime.prepare_route(
        route_id,
        Some(FormatDescriptor::new("pcm-s16le-48000-mono")),
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
                endpoint: NativeAudioEndpoint::Sink,
                preset: NativeAudioPreset::PcmS16le48000Mono,
            },
        },
    )))
}

fn prepare_selected_route(
    runtime: &mut NodeRuntime,
    session_id: SessionId,
    source_ref: PortRef,
    sink: PortRef,
) -> io::Result<RouteId> {
    // Real authenticated catalog descriptors, not launch-config/fixture ports.
    let snapshot = runtime.snapshot();
    let peer = snapshot
        .peers
        .iter()
        .find(|peer| peer.id == source_ref.node_id)
        .ok_or_else(fail)?;
    let source = sanitized(peer.port(source_ref.capability_id, source_ref.port_id))?;
    if source.direction != PortDirection::Source
        || source.profile != ProfileId::audio_frames_v1()
        || source.availability != Availability::Available
        || !source
            .formats
            .contains(&FormatDescriptor::new("pcm-s16le-48000-mono"))
        || !source.qos_modes.contains(&QosMode::Interactive)
    {
        return Err(fail());
    }
    sanitized(runtime.create_route(session_id, source_ref, sink, RouteBackend::AdapterManaged))
}

/// A single explicit software playback destination, not a hardware probe or
/// fabricated remote catalog. IDs are ephemeral per invocation and advertised.
fn local_runtime(node_id: NodeId) -> io::Result<(NodeRuntime, PortRef)> {
    let mut node = NodeDescriptor::new(
        node_id,
        "Linux lab playback",
        Platform::Linux,
        "not probed",
        env!("CARGO_PKG_VERSION"),
        [ProtocolVersion::new(1, 1)],
    );
    let adapter = AdapterInstanceId::new();
    let capability = CapabilityId::new();
    let port = PortId::new();
    sanitized(node.add_adapter(AdapterInstanceDescriptor {
        id: adapter,
        adapter_type: "capyio.linux.lab-playback".into(),
        display_name: "Explicit lab PCM playback".into(),
        deployment_mode: AdapterDeploymentMode::Sidecar,
        version: env!("CARGO_PKG_VERSION").into(),
        state: AdapterState::Ready,
        health: AdapterHealth::Unknown,
        owned_capabilities: BTreeSet::new(),
        supported_route_modes: BTreeSet::from([RouteBackend::AdapterManaged]),
    }))?;
    let descriptor = PortDescriptor {
        id: port,
        capability_id: capability,
        display_name: "Explicit playback Sink".into(),
        direction: PortDirection::Sink,
        profile: ProfileId::audio_frames_v1(),
        schema_id: None,
        formats: vec![FormatDescriptor::new("pcm-s16le-48000-mono")],
        qos_modes: BTreeSet::from([QosMode::Interactive]),
        clock_domain: None,
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::None,
        interoperability_mode: InteroperabilityMode::AdapterManaged,
    };
    sanitized(node.add_capability(CapabilityDescriptor {
        id: capability,
        adapter_instance_id: adapter,
        display_name: "Explicit lab playback".into(),
        class: CapabilityClass::Speaker,
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::None,
        metadata: BTreeMap::new(),
        ports: BTreeMap::from([(port, descriptor)]),
    }))?;
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
