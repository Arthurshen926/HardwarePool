//! Explicit one-shot Linux camera composition. Uses the same inherited private
//! framing as audio, but never interprets an audio launch schema or grants capture.
use super::*;
use capyio_linux_avc::secure::{SecureCameraAdmission, SecureCameraEvent, SecureCameraOwner};

const FORMAT: &str = "cavc-avc-access-unit-v1";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CameraConfig {
    version: u32,
    node_id: NodeId,
    #[serde(deserialize_with = "deserialize_source")]
    source: PortRef,
    listen: SocketAddrV4,
    duration_secs: u64,
}
fn deserialize_source<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<PortRef, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Source {
        node_id: NodeId,
        capability_id: CapabilityId,
        port_id: PortId,
    }
    let source = Source::deserialize(deserializer)?;
    Ok(PortRef {
        node_id: source.node_id,
        capability_id: source.capability_id,
        port_id: source.port_id,
    })
}
impl CameraConfig {
    fn parse(bytes: &[u8]) -> io::Result<Self> {
        // Require an object, retaining duplicate/unknown-field rejection.
        if bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
            return Err(fail());
        }
        // Shape-only inspection does not replace decoding the original bytes:
        // the typed decode below still rejects duplicate fields at both levels.
        let shape: serde_json::Value = sanitized(serde_json::from_slice(bytes))?;
        if !shape
            .get("source")
            .is_some_and(serde_json::Value::is_object)
        {
            return Err(fail());
        }
        let config: Self = sanitized(serde_json::from_slice(bytes))?;
        let ip = config.listen.ip();
        if config.version != 1
            || config.node_id.as_uuid().is_nil()
            || config.source.node_id == config.node_id
            || config.source.node_id.as_uuid().is_nil()
            || config.source.capability_id.as_uuid().is_nil()
            || config.source.port_id.as_uuid().is_nil()
            || !(3..=60).contains(&config.duration_secs)
            || !(ip.is_loopback() || ip.is_private() || ip.is_link_local())
            || ip.is_broadcast()
            || ip.is_unspecified()
        {
            return Err(fail());
        }
        Ok(config)
    }
}

pub fn run_camera_stdio() -> io::Result<()> {
    let mut io = sanitized(LauncherIo::stdio())?;
    let result = launch_camera(&mut io);
    if result.is_err() {
        let _ = io.send_json(
            &json!({"version":1,"event":"failed"}),
            Instant::now() + Duration::from_secs(1),
        );
    }
    result.map_err(|_| io::Error::other("Linux camera launch failed"))
}

fn camera_command(bytes: &[u8]) -> io::Result<LocalCommand> {
    if bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
        return Err(fail());
    }
    LocalCommand::parse(bytes)
}
fn camera_before_approval(io: &mut LauncherIo, stage: &str) -> io::Result<bool> {
    let Some(bytes) = io.poll_frame(LOCAL_FRAME)? else {
        return Ok(false);
    };
    let command = camera_command(bytes.as_slice())?;
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

fn launch_camera(io: &mut LauncherIo) -> io::Result<()> {
    let deadline = Instant::now() + BOOTSTRAP;
    let config = CameraConfig::parse(io.read_frame(LOCAL_FRAME, deadline)?.as_slice())?;
    let bytes = io.read_frame(65_536, deadline)?;
    let directory = sanitized(PairedPeerDirectory::decode_json(
        config.node_id,
        bytes.as_slice(),
    ))?;
    drop(bytes);
    let mut peers = directory.active_peers();
    let peer = peers.next().ok_or_else(fail)?;
    if peer.node_id != config.source.node_id
        || peers.next().is_some()
        || directory.peers().len() != 1
    {
        return Err(fail());
    }
    let bytes = io.read_frame(65_536, deadline)?;
    let credentials = sanitized(ControlCredentialBundle::decode(bytes.as_slice()))?;
    drop(bytes);
    let identity = ControlTlsServerIdentity::from_bundle(peer.peer_certificate_sha256, credentials);
    let (mut runtime, sink) = camera_runtime(config.node_id)?;
    let expectation = sanitized(ControlTlsNodeBinding::new(config.node_id, peer.node_id))?;
    let server = sanitized(AuthenticatedControlServer::from_listener_fresh_session(
        sanitized(TcpListener::bind(config.listen))?,
        identity.clone(),
        expectation,
        Duration::from_secs(1),
    ))?;
    io.send_json(&json!({"version":1,"event":"listening","address":sanitized(server.local_addr())?.to_string()}), Instant::now() + Duration::from_secs(1))?;
    let deadline = Instant::now() + ACCEPT;
    let channel = loop {
        check(deadline)?;
        if camera_before_approval(io, "listening")? {
            return stopped(io);
        }
        if let Some(channel) = sanitized(
            server.try_accept_until(deadline.min(Instant::now() + Duration::from_secs(1))),
        )? {
            break channel;
        }
        thread::sleep(Duration::from_millis(2));
    };
    drop(server);
    let mut link = sanitized(CatalogControlLink::admit_accepted_new_peer(
        &mut runtime,
        channel,
        peer.display_name.clone(),
    ))?;
    let prepared = prepare_camera(io, &config, &mut runtime, &mut link, sink);
    let route_id = match prepared {
        Ok(Some(id)) => id,
        Ok(None) => {
            let _ = link.close_and_reconcile(&mut runtime);
            return stopped(io);
        }
        Err(_) => {
            let _ = link.close_and_reconcile(&mut runtime);
            return Err(fail());
        }
    };
    let admission = SecureCameraAdmission {
        expected: link.tls_binding(),
        route_id,
        stream_id: StreamId::new(),
        local_address: (*config.listen.ip()).into(),
        lifetime: Duration::from_secs(config.duration_secs),
    };
    let mut owner = sanitized(SecureCameraOwner::new(runtime, link, identity, admission))?;
    let result = (|| -> io::Result<()> {
        loop {
            if let Some(bytes) = io.poll_frame(LOCAL_FRAME)? {
                let command = camera_command(bytes.as_slice())?;
                if command.simple("stop") {
                    break;
                }
                if !command.simple("status") {
                    return Err(fail());
                }
                camera_status(io, &owner, route_id, "status")?;
            }
            match sanitized(owner.poll())? {
                Some(SecureCameraEvent::Stopped) => break,
                Some(SecureCameraEvent::Bound) => {
                    camera_status(io, &owner, route_id, "camera_bound")?
                }
                Some(SecureCameraEvent::Record) => {
                    camera_status(io, &owner, route_id, "camera_record")?
                }
                Some(SecureCameraEvent::Listening) | None => {}
            }
            thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    })();
    let cleanup = sanitized(owner.stop());
    result.and(cleanup)?;
    stopped(io)
}
fn camera_status(
    io: &mut LauncherIo,
    owner: &SecureCameraOwner,
    route_id: RouteId,
    event: &str,
) -> io::Result<()> {
    io.send_json(&json!({"version":1,"event":event,"route_id":route_id,
        "state":sanitized(owner.runtime().route(route_id))?.state,
        "queued":owner.metrics().map(|m|m.queued).unwrap_or(0),
        "helper_pid":owner.helper_pid(),"decoder_pid":owner.decoder_pid(),"node_name":owner.node_name(),
        "consumer_delivery_verified":false}), Instant::now() + Duration::from_secs(1))
}

fn prepare_camera(
    io: &mut LauncherIo,
    config: &CameraConfig,
    runtime: &mut NodeRuntime,
    link: &mut CatalogControlLink,
    sink: PortRef,
) -> io::Result<Option<RouteId>> {
    let deadline = Instant::now() + CATALOG;
    loop {
        check(deadline)?;
        if camera_before_approval(io, "catalog")? {
            return Ok(None);
        }
        match sanitized(
            link.poll_control_message_until(deadline.min(Instant::now() + Duration::from_secs(1))),
        )? {
            Some(ReceivedControlMessage::Catalog(catalog)) => {
                sanitized(link.apply_catalog(runtime, catalog))?;
                break;
            }
            Some(_) => return Err(fail()),
            None => thread::sleep(Duration::from_millis(2)),
        }
    }
    validate_camera_source(runtime, config.source)?;
    let catalog = sanitized(link.prepare_catalog(runtime, &BTreeSet::new()))?;
    sanitized(link.send_catalog_until(catalog, deadline))?;
    let route = sanitized(runtime.create_route(
        link.session_id(),
        config.source,
        sink,
        RouteBackend::AdapterManaged,
    ))?;
    let request_id = MessageId::new();
    let deadline = Instant::now() + CONSENT;
    io.send_json(&json!({"version":1,"event":"approval_required","request_id":request_id,"route_id":route,
        "session_id":link.session_id(),"source":config.source,"sink":sink,"format":FORMAT,
        "duration_secs":config.duration_secs,"approval_timeout_secs":CONSENT.as_secs(),"media_security":"tls13_mtls_v1",
        "width":1280,"height":720,"fps":30,"rotation":0,"max_frames":120,"max_input_bytes":33554432}), Instant::now()+Duration::from_secs(1))?;
    loop {
        check(deadline)?;
        if let Some(bytes) = io.poll_frame(LOCAL_FRAME)? {
            let command = camera_command(bytes.as_slice())?;
            if command.simple("stop") {
                return Ok(None);
            }
            if command.command == "approve"
                && command.request_id == Some(request_id)
                && command.route_id == Some(route)
            {
                break;
            }
            if !command.simple("status") {
                return Err(fail());
            }
            io.send_json(
                &json!({"version":1,"event":"status","stage":"awaiting_local_approval"}),
                Instant::now() + Duration::from_secs(1),
            )?;
        }
        match sanitized(
            link.poll_control_message_until(deadline.min(Instant::now() + Duration::from_secs(1))),
        )? {
            Some(ReceivedControlMessage::Heartbeat(h)) => {
                if !h.acknowledgement {
                    sanitized(link.send_heartbeat_until(
                        h.sequence,
                        true,
                        deadline.min(Instant::now() + Duration::from_secs(1)),
                    ))?;
                }
            }
            Some(ReceivedControlMessage::Catalog(c)) => {
                sanitized(link.apply_catalog(runtime, c))?;
                validate_camera_source(runtime, config.source)?;
            }
            Some(_) => return Err(fail()),
            None => {}
        }
        thread::sleep(Duration::from_millis(2));
    }
    check(deadline)?;
    validate_camera_source(runtime, config.source)?;
    let approved_expiry = now_ms()?
        .checked_add(config.duration_secs * 1000)
        .ok_or_else(fail)?;
    let deadline = Instant::now() + CONSENT.min(Duration::from_secs(config.duration_secs));
    sanitized(link.send_route_status_until(
        sanitized(runtime.route(route))?,
        negotiation_operation_deadline(deadline, approved_expiry)?,
    ))?;
    let peer_expiry = loop {
        let Some(status) = camera_grant(
            io,
            runtime,
            link,
            config.source,
            route,
            deadline,
            approved_expiry,
        )?
        else {
            return Ok(None);
        };
        match status.route.authorization {
            AuthorizationState::Pending => {}
            AuthorizationState::Authorized {
                expires_at_ms: Some(e),
            } if e > now_ms()? => break e,
            _ => return Err(fail()),
        }
    };
    let expiry = approved_expiry.min(peer_expiry);
    sanitized(runtime.authorize_route(route, Some(expiry)))?;
    sanitized(link.send_route_status_until(
        sanitized(runtime.route(route))?,
        negotiation_operation_deadline(deadline, expiry)?,
    ))?;
    loop {
        let Some(status) = camera_grant(io, runtime, link, config.source, route, deadline, expiry)?
        else {
            return Ok(None);
        };
        match status.route.authorization {
            AuthorizationState::Authorized {
                expires_at_ms: Some(e),
            } if e == expiry => break,
            AuthorizationState::Authorized {
                expires_at_ms: Some(e),
            } if e == peer_expiry => {}
            _ => return Err(fail()),
        }
    }
    let now = now_ms()?;
    // Leave enough time for the existing owner to acquire the independently
    // authorized Source lease; no data listener/helper exists before that reply.
    lease_ttl(expiry, now)?;
    sanitized(runtime.set_local_port_lease_policy(sink, PortLeasePolicy::Exclusive))?;
    sanitized(runtime.acquire_route_port_lease(route, sink, expiry, now))?;
    sanitized(runtime.prepare_route(
        route,
        Some(FormatDescriptor::new(FORMAT)),
        QosMode::Interactive,
        now,
    ))?;
    sanitized(runtime.begin_route_start(route, now))?;
    Ok(Some(route))
}
fn camera_grant(
    io: &mut LauncherIo,
    runtime: &mut NodeRuntime,
    link: &mut CatalogControlLink,
    source: PortRef,
    route: RouteId,
    deadline: Instant,
    expiry: u64,
) -> io::Result<Option<ValidatedRouteStatus>> {
    loop {
        let operation = negotiation_operation_deadline(deadline, expiry)?;
        if camera_before_approval(io, "awaiting_peer_consent")? {
            return Ok(None);
        }
        match sanitized(link.poll_control_message_until(operation))? {
            Some(ReceivedControlMessage::Catalog(c)) => {
                sanitized(link.apply_catalog(runtime, c))?;
                validate_camera_source(runtime, source)?;
            }
            Some(ReceivedControlMessage::Heartbeat(h)) => {
                if !h.acknowledgement {
                    sanitized(link.send_heartbeat_until(h.sequence, true, operation))?;
                }
            }
            Some(ReceivedControlMessage::RouteStatus(s)) => {
                validate_draft_peer(runtime, route, &s)?;
                let local = sanitized(runtime.route(route))?;
                if s.secure_camera_data.is_some()
                    || s.route.compatible_formats != local.compatible_formats
                    || s.route.compatible_qos_modes != local.compatible_qos_modes
                    || !s.route.diagnostic_ids.is_empty()
                {
                    return Err(fail());
                }
                return Ok(Some(*s));
            }
            Some(_) => return Err(fail()),
            None => thread::sleep(Duration::from_millis(2)),
        }
    }
}
fn validate_camera_source(runtime: &NodeRuntime, source: PortRef) -> io::Result<()> {
    let snapshot = runtime.snapshot();
    let peer = snapshot
        .peers
        .iter()
        .find(|p| p.id == source.node_id)
        .ok_or_else(fail)?;
    let cap = peer
        .capabilities
        .get(&source.capability_id)
        .ok_or_else(fail)?;
    let port = sanitized(peer.port(source.capability_id, source.port_id))?;
    if peer.platform != Platform::Android
        || cap.class != CapabilityClass::Camera
        || cap.availability != Availability::Available
        || port.direction != PortDirection::Source
        || port.profile != ProfileId::video_frames_v1()
        || port.availability != Availability::Available
        || port.interoperability_mode != InteroperabilityMode::AdapterManaged
        || !port.formats.contains(&FormatDescriptor::new(FORMAT))
        || !port.qos_modes.contains(&QosMode::Interactive)
    {
        return Err(fail());
    }
    Ok(())
}
fn camera_runtime(node_id: NodeId) -> io::Result<(NodeRuntime, PortRef)> {
    let mut node = NodeDescriptor::new(
        node_id,
        "LinuxCameraSink",
        Platform::Linux,
        "not probed",
        env!("CARGO_PKG_VERSION"),
        [ProtocolVersion::new(
            1,
            capyio_protocol::PROTOCOL_MINOR as u16,
        )],
    );
    let adapter = AdapterInstanceId::new();
    let capability = CapabilityId::new();
    let port = PortId::new();
    sanitized(node.add_adapter(AdapterInstanceDescriptor {
        id: adapter,
        adapter_type: "capyio.linux.camera".into(),
        display_name: "Owned finite PipeWire camera".into(),
        deployment_mode: AdapterDeploymentMode::Sidecar,
        version: env!("CARGO_PKG_VERSION").into(),
        state: AdapterState::Ready,
        health: AdapterHealth::Unknown,
        owned_capabilities: BTreeSet::new(),
        supported_route_modes: BTreeSet::from([RouteBackend::AdapterManaged]),
    }))?;
    sanitized(node.add_capability(CapabilityDescriptor {
        id: capability,
        adapter_instance_id: adapter,
        display_name: "LinuxCameraSink".into(),
        class: CapabilityClass::Camera,
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::None,
        metadata: BTreeMap::new(),
        ports: BTreeMap::from([(
            port,
            PortDescriptor {
                id: port,
                capability_id: capability,
                display_name: "Canonical AVC camera Sink".into(),
                direction: PortDirection::Sink,
                profile: ProfileId::video_frames_v1(),
                schema_id: None,
                formats: vec![FormatDescriptor::new(FORMAT)],
                qos_modes: BTreeSet::from([QosMode::Interactive]),
                clock_domain: None,
                availability: Availability::Available,
                permission_requirement: PermissionRequirement::None,
                interoperability_mode: InteroperabilityMode::AdapterManaged,
            },
        )]),
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
        json!({"version":1,"node_id":NodeId::new(),"source":PortRef {node_id:NodeId::new(),capability_id:CapabilityId::new(),port_id:PortId::new()},"listen":"127.0.0.1:0","duration_secs":60})
    }
    #[test]
    fn camera_bootstrap_requires_exact_source_and_finite_private_listener() {
        let value = config();
        assert!(CameraConfig::parse(&serde_json::to_vec(&value).unwrap()).is_ok());
        for seconds in [0, 1, 2, 61, u64::MAX] {
            let mut v = value.clone();
            v["duration_secs"] = json!(seconds);
            assert!(CameraConfig::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        }
        for address in [
            "0.0.0.0:0",
            "8.8.8.8:1234",
            "224.0.0.1:1234",
            "255.255.255.255:1",
            "[::1]:1",
        ] {
            let mut v = value.clone();
            v["listen"] = json!(address);
            assert!(CameraConfig::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        }
        for field in ["target", "media", "helper", "password", "proposal_mode"] {
            let mut v = value.clone();
            v[field] = json!("forbidden");
            assert!(CameraConfig::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        }
        for field in ["node_id", "capability_id", "port_id"] {
            let mut v = value.clone();
            v["source"][field] = json!("00000000-0000-0000-0000-000000000000");
            assert!(CameraConfig::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        }
        let mut v = value.clone();
        v["source"]["node_id"] = v["node_id"].clone();
        assert!(CameraConfig::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        let mut v = value.clone();
        v["source"]["extra"] = json!(true);
        assert!(CameraConfig::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        let mut v = value.clone();
        let source = &v["source"];
        v["source"] = json!([
            source["node_id"],
            source["capability_id"],
            source["port_id"]
        ]);
        assert!(CameraConfig::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        assert!(CameraConfig::parse(b"[]").is_err());
        let encoded = serde_json::to_string(&value).unwrap();
        let duplicate = encoded.replacen('{', "{\"version\":1,", 1);
        assert!(CameraConfig::parse(duplicate.as_bytes()).is_err());
    }
    #[test]
    fn camera_catalog_is_a_real_single_linux_sink_without_fabricated_peer() {
        let (runtime, sink) = camera_runtime(NodeId::new()).unwrap();
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.local_node.platform, Platform::Linux);
        assert!(snapshot.peers.is_empty());
        assert!(snapshot.sessions.is_empty());
        assert!(snapshot.routes.is_empty());
        assert_eq!(snapshot.local_node.capabilities.len(), 1);
        let port = snapshot
            .local_node
            .port(sink.capability_id, sink.port_id)
            .unwrap();
        assert_eq!(port.direction, PortDirection::Sink);
        assert_eq!(port.formats, [FormatDescriptor::new(FORMAT)]);
        assert_eq!(port.profile, ProfileId::video_frames_v1());
        assert!(validate_camera_source(&runtime, sink).is_err());
    }
}
