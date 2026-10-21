//! Explicit one-shot Linux touchpad composition. Uses the same inherited private
//! framing as audio, but never interprets an audio launch schema or grants capture.
use super::*;
use capyio_control_tls::{
    ControlTlsClientIdentity, ControlTlsEndpoint,
    touchpad::{TouchpadTlsBinding, TouchpadTlsChannel},
};
use capyio_linux_touchpad::RuntimeTouchpadWorker;
use capyio_protocol::RouteCommandAction;
use std::sync::{Arc, Mutex, mpsc};

const FORMAT: &str = "touchpad-frame-v1";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TouchpadConfig {
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
impl TouchpadConfig {
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

pub fn run_touchpad_stdio() -> io::Result<()> {
    run_stdio(Backend::Native, CredentialSource::PrivatePipe)
}

#[cfg(feature = "paired-store")]
pub fn run_touchpad_paired_store_stdio(selection: PairedStoreSelection) -> io::Result<()> {
    run_stdio(Backend::Native, CredentialSource::PairedStore(selection))
}

enum Backend {
    Native,
    #[cfg(feature = "touchpad-test-fixture")]
    Fixture(capyio_linux_touchpad::TouchpadFixtureMode),
}
#[cfg(feature = "touchpad-test-fixture")]
pub fn run_touchpad_fixture_stdio(mode: &str) -> io::Result<()> {
    run_fixture_stdio(mode, CredentialSource::PrivatePipe)
}
#[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
pub fn run_touchpad_fixture_paired_store_stdio(
    mode: &str,
    selection: PairedStoreSelection,
) -> io::Result<()> {
    run_fixture_stdio(mode, CredentialSource::PairedStore(selection))
}
#[cfg(feature = "touchpad-test-fixture")]
fn run_fixture_stdio(mode: &str, source: CredentialSource) -> io::Result<()> {
    use capyio_linux_touchpad::TouchpadFixtureMode::*;
    let mode = match mode {
        "normal" => Normal,
        "fail-create" => FailCreate,
        "partial-write" => PartialWrite,
        "delayed-commit" => DelayedCommit,
        "backpressure" => Backpressure,
        _ => return Err(fail()),
    };
    run_stdio(Backend::Fixture(mode), source)
}
fn run_stdio(backend: Backend, source: CredentialSource) -> io::Result<()> {
    let mut io = sanitized(LauncherIo::stdio())?;
    let result = launch_touchpad(&mut io, backend, source);
    if result.is_err() {
        let _ = io.send_json(
            &json!({"version":1,"event":"failed"}),
            Instant::now() + Duration::from_secs(1),
        );
    }
    result.map_err(|_| io::Error::other("Linux touchpad launch failed"))
}

fn touchpad_command(bytes: &[u8]) -> io::Result<LocalCommand> {
    if bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
        return Err(fail());
    }
    LocalCommand::parse(bytes)
}
fn touchpad_before_approval(io: &mut LauncherIo, stage: &str) -> io::Result<bool> {
    let Some(bytes) = io.poll_frame(LOCAL_FRAME)? else {
        return Ok(false);
    };
    let command = touchpad_command(bytes.as_slice())?;
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

fn launch_touchpad(
    io: &mut LauncherIo,
    backend: Backend,
    source: CredentialSource,
) -> io::Result<()> {
    let deadline = Instant::now() + BOOTSTRAP;
    let config = TouchpadConfig::parse(io.read_frame(LOCAL_FRAME, deadline)?.as_slice())?;
    let (directory, credentials) =
        launcher_credentials(io, config.node_id, config.source.node_id, deadline, source)?;
    let peer = directory.active_peers().next().ok_or_else(fail)?;
    let identity = ControlTlsServerIdentity::from_bundle(peer.peer_certificate_sha256, credentials);
    let client_identity = ControlTlsClientIdentity {
        expected_peer_certificate: identity.expected_peer_certificate,
        certificate_chain_der: identity.certificate_chain_der.clone(),
        private_key_der: identity.private_key_der.clone(),
        server_ca_der: identity.client_ca_der.clone(),
    };
    let (mut runtime, sink) = touchpad_runtime(config.node_id)?;
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
        if touchpad_before_approval(io, "listening")? {
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
    let prepared = prepare_touchpad(io, &config, &mut runtime, &mut link, sink);
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
    run_route(
        io,
        runtime,
        link,
        route_id,
        client_identity,
        &peer.server_name,
        backend,
    )
}

// Control remains on this owner while a bounded connect runs separately. No
// native opener is given to that thread: a late channel is dropped on Stop.
fn run_route(
    io: &mut LauncherIo,
    mut runtime: NodeRuntime,
    mut link: CatalogControlLink,
    route_id: RouteId,
    identity: ControlTlsClientIdentity,
    server_name: &str,
    backend: Backend,
) -> io::Result<()> {
    let prepared = sanitized(runtime.route(route_id))?.clone();
    let expiry = finite_expiry(&prepared)?;
    let result = (|| -> io::Result<Option<(TouchpadTlsBinding, std::net::SocketAddr, u64)>> {
        let deadline =
            negotiation_operation_deadline(Instant::now() + Duration::from_secs(1), expiry)?;
        let request = sanitized(link.send_port_lease_request_until(
            route_id,
            prepared.source,
            PortLeaseAction::Acquire,
            lease_ttl(expiry, now_ms()?)?,
            deadline,
        ))?;
        let source_expiry = loop {
            let operation = negotiation_operation_deadline(deadline, expiry)?;
            if touchpad_before_approval(io, "awaiting_source_lease")? {
                return Ok(None);
            }
            match sanitized(link.poll_control_message_until(operation))? {
                Some(ReceivedControlMessage::PortLeaseResponse(response)) => {
                    check(deadline)?;
                    if response.request_message_id != request
                        || response.session_id != prepared.session_id
                        || response.route_id != route_id
                        || response.port != prepared.source
                        || response.outcome != PortLeaseOutcome::Acquired
                    {
                        return Err(fail());
                    }
                    break response
                        .expires_at_ms
                        .filter(|e| *e > now_ms().unwrap_or(u64::MAX) && *e <= expiry)
                        .ok_or_else(fail)?;
                }
                Some(message) => {
                    handle_control(&mut runtime, &mut link, &prepared, message, operation)?
                }
                None => thread::sleep(Duration::from_millis(2)),
            }
        };
        // Mirror the shorter independently granted Source lease into our Sink
        // lease. The existing worker watchdog then enforces both even if the
        // control owner stalls. Never modify or extend the negotiated grant.
        sanitized(runtime.acquire_route_port_lease(
            route_id,
            prepared.sink,
            source_expiry,
            now_ms()?,
        ))?;
        let deadline = negotiation_operation_deadline(Instant::now() + CATALOG, source_expiry)?;
        sanitized(link.send_route_status_until(&prepared, deadline))?;
        sanitized(link.send_route_command_until(route_id, RouteCommandAction::Start, deadline))?;
        sanitized(runtime.begin_route_start(route_id, now_ms()?))?;
        let original = sanitized(runtime.route(route_id))?.clone();
        let deadline = Instant::now() + CATALOG;
        loop {
            let operation = negotiation_operation_deadline(deadline, source_expiry)?;
            if touchpad_before_approval(io, "awaiting_touchpad_offer")? {
                return Ok(None);
            }
            match sanitized(link.poll_control_message_until(operation))? {
                Some(ReceivedControlMessage::RouteStatus(status))
                    if status.touchpad_data.is_some() =>
                {
                    if status.session_id != original.session_id
                        || status.route != original
                        || status.native_audio_data.is_some()
                        || status.haptics_data.is_some()
                        || status.camera_data.is_some()
                        || status.secure_camera_data.is_some()
                    {
                        return Err(fail());
                    }
                    let offer = status.touchpad_data.ok_or_else(fail)?;
                    let binding = sanitized(TouchpadTlsBinding::from_route(
                        link.tls_binding(),
                        &original,
                        offer.stream_id,
                        now_ms()?,
                    ))?;
                    // The wire offer carries only a port. Its host comes solely
                    // from the admitted control socket, never a bootstrap/payload URL.
                    let address = std::net::SocketAddr::new(
                        sanitized(link.accepted_peer_addr())?.ip(),
                        offer.port,
                    );
                    return Ok(Some((binding, address, source_expiry)));
                }
                Some(message) => {
                    handle_control(&mut runtime, &mut link, &original, message, operation)?
                }
                None => thread::sleep(Duration::from_millis(2)),
            }
        }
    })();
    let (binding, address, source_expiry) = match result {
        Ok(Some(value)) => value,
        other => {
            let cleanup = sanitized(link.close_and_reconcile(&mut runtime));
            return match other {
                Ok(None) => {
                    cleanup?;
                    stopped(io)
                }
                Err(e) => Err(e),
                _ => unreachable!(),
            };
        }
    };
    let original = sanitized(runtime.route(route_id))?.clone();
    let end = Instant::now() + Duration::from_millis(source_expiry.saturating_sub(now_ms()?));
    let name = server_name.to_owned();
    let (tx, rx) = mpsc::sync_channel(1);
    let connect = thread::Builder::new()
        .name("capyio-touchpad-connect".into())
        .spawn(move || {
            let channel = TouchpadTlsChannel::connect_until(
                ControlTlsEndpoint {
                    address,
                    io_timeout: Duration::from_secs(1),
                },
                &name,
                identity,
                binding,
                end,
            );
            // On owner cancellation the receiver is dropped, closing any late channel.
            let _ = tx.send(channel);
        })?;
    let runtime = Arc::new(Mutex::new(runtime));
    let mut worker: Option<RuntimeTouchpadWorker> = None;
    let mut active_reported = false;
    let mut previous_wall = now_ms()?;
    let result = (|| -> io::Result<()> {
        let mut connected = false;
        loop {
            let now = now_ms()?;
            if now < previous_wall || now >= source_expiry || Instant::now() >= end {
                return Err(fail());
            }
            previous_wall = now;
            {
                let runtime = lock(&runtime)?;
                validate_live(&runtime, &original, source_expiry, now)?;
            }
            if let Some(bytes) = io.poll_frame(LOCAL_FRAME)? {
                let command = touchpad_command(bytes.as_slice())?;
                if command.simple("stop") {
                    break;
                }
                if !command.simple("status") {
                    return Err(fail());
                }
                touchpad_status(io, &runtime, route_id, "status")?;
            }
            let operation = end.min(Instant::now() + Duration::from_millis(250));
            if let Some(message) = sanitized(link.poll_control_message_until(operation))? {
                if let ReceivedControlMessage::Heartbeat(h) = message {
                    // Never retain Runtime authority locks across control I/O.
                    if !h.acknowledgement {
                        sanitized(link.send_heartbeat_until(h.sequence, true, operation))?;
                    }
                } else {
                    let mut runtime = lock(&runtime)?;
                    handle_control(&mut runtime, &mut link, &original, message, operation)?;
                    validate_live(&runtime, &original, source_expiry, now_ms()?)?;
                }
            }
            if !connected {
                match rx.try_recv() {
                    Ok(channel) => {
                        let channel = sanitized(channel)?;
                        validate_live(&*lock(&runtime)?, &original, source_expiry, now_ms()?)?;
                        touchpad_status(io, &runtime, route_id, "touchpad_bound")?;
                        worker = Some(start_worker(Arc::clone(&runtime), channel, &backend)?);
                        connected = true;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => return Err(fail()),
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
            if let Some(worker) = &mut worker {
                if !sanitized(worker.poll())? {
                    break;
                }
                if !active_reported
                    && sanitized(lock(&runtime)?.route(route_id))?.state == RouteState::Active
                {
                    touchpad_status(io, &runtime, route_id, "touchpad_active")?;
                    active_reported = true;
                }
            }
            thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    })();
    drop(rx);
    // Close authority first, then interrupt both worker channels before joining.
    let control = match lock(&runtime) {
        Ok(mut runtime) => sanitized(link.close_and_reconcile(&mut runtime)),
        Err(error) => {
            link.close();
            Err(error)
        }
    };
    let media = worker
        .as_mut()
        .map(RuntimeTouchpadWorker::stop)
        .unwrap_or(Ok(()));
    let join_deadline = Instant::now() + Duration::from_secs(3);
    while !connect.is_finished() && Instant::now() < join_deadline {
        thread::sleep(Duration::from_millis(2));
    }
    let connection = if connect.is_finished() {
        connect.join().map_err(|_| fail())
    } else {
        Err(fail())
    };
    // Even when cancellation interrupts a pending ACK, do not claim a clean
    // native release. The transport closes/destroys its owned helper regardless.
    result.and(control).and(sanitized(media)).and(connection)?;
    stopped(io)
}

fn start_worker(
    runtime: Arc<Mutex<NodeRuntime>>,
    channel: TouchpadTlsChannel,
    backend: &Backend,
) -> io::Result<RuntimeTouchpadWorker> {
    match backend {
        Backend::Native => {
            #[cfg(feature = "native-touchpad")]
            {
                RuntimeTouchpadWorker::start(runtime, channel)
            }
            #[cfg(not(feature = "native-touchpad"))]
            {
                let _ = (runtime, channel);
                Err(fail())
            }
        }
        #[cfg(feature = "touchpad-test-fixture")]
        Backend::Fixture(mode) => {
            // No path or inherited environment selects a device/helper. This
            // explicit fixture command passes only a duplicate of its own stderr.
            let trace = std::fs::File::from(rustix::io::dup(std::io::stderr())?);
            RuntimeTouchpadWorker::start_fixture(
                runtime,
                channel,
                capyio_linux_touchpad::TouchpadFixtureConfig { trace, mode: *mode },
            )
        }
    }
}
fn lock(runtime: &Mutex<NodeRuntime>) -> io::Result<std::sync::MutexGuard<'_, NodeRuntime>> {
    let deadline = Instant::now() + Duration::from_millis(100);
    loop {
        match runtime.try_lock() {
            Ok(value) => return Ok(value),
            Err(std::sync::TryLockError::WouldBlock) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(1))
            }
            Err(_) => return Err(fail()),
        }
    }
}
fn finite_expiry(route: &Route) -> io::Result<u64> {
    match route.authorization {
        AuthorizationState::Authorized {
            expires_at_ms: Some(e),
        } => Ok(e),
        _ => Err(fail()),
    }
}
fn validate_live(runtime: &NodeRuntime, original: &Route, expiry: u64, now: u64) -> io::Result<()> {
    let route = sanitized(runtime.route(original.id))?;
    let mut expected = original.clone();
    expected.state = route.state;
    let session = sanitized(runtime.session(original.session_id))?;
    let lease = runtime
        .route_port_lease_claim(original.id, original.sink)
        .ok_or_else(fail)?;
    if *route != expected
        || !matches!(route.state, RouteState::Starting | RouteState::Active)
        || session.state != SessionState::Ready
        || session.local_node_id != original.sink.node_id
        || session.remote_node_id != original.source.node_id
        || now >= expiry
        || now >= finite_expiry(route)?
        || lease.expires_at_ms != expiry
        || lease.session_id != original.session_id
    {
        return Err(fail());
    }
    validate_touchpad_source(runtime, original.source)
}
fn handle_control(
    runtime: &mut NodeRuntime,
    link: &mut CatalogControlLink,
    original: &Route,
    message: ReceivedControlMessage,
    deadline: Instant,
) -> io::Result<()> {
    match message {
        ReceivedControlMessage::Heartbeat(h) => {
            if !h.acknowledgement {
                sanitized(link.send_heartbeat_until(h.sequence, true, deadline))?;
            }
        }
        ReceivedControlMessage::Catalog(c) => {
            sanitized(link.apply_catalog(runtime, c))?;
            validate_touchpad_source(runtime, original.source)?;
        }
        ReceivedControlMessage::RouteStatus(s) => {
            let mut expected = original.clone();
            // Peer activation follows its first ACK; it cannot activate our Runtime.
            if original.state == RouteState::Starting && s.route.state == RouteState::Active {
                expected.state = RouteState::Active;
            }
            if s.session_id != original.session_id
                || s.route != expected
                || s.touchpad_data.is_some()
                || s.native_audio_data.is_some()
                || s.haptics_data.is_some()
                || s.camera_data.is_some()
                || s.secure_camera_data.is_some()
            {
                return Err(fail());
            }
        }
        // Includes Stop, revoke, duplicate lease replies and unexpected commands.
        _ => return Err(fail()),
    }
    Ok(())
}
fn touchpad_status(
    io: &mut LauncherIo,
    runtime: &Mutex<NodeRuntime>,
    route_id: RouteId,
    event: &str,
) -> io::Result<()> {
    let value = {
        let runtime = lock(runtime)?;
        json!({"version":1,"event":event,"route_id":route_id,
            "state":sanitized(runtime.route(route_id))?.state,"os_gestures_verified":false})
    };
    io.send_json(&value, Instant::now() + Duration::from_secs(1))
}

fn prepare_touchpad(
    io: &mut LauncherIo,
    config: &TouchpadConfig,
    runtime: &mut NodeRuntime,
    link: &mut CatalogControlLink,
    sink: PortRef,
) -> io::Result<Option<RouteId>> {
    let deadline = Instant::now() + CATALOG;
    loop {
        check(deadline)?;
        if touchpad_before_approval(io, "catalog")? {
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
    validate_touchpad_source(runtime, config.source)?;
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
        "max_contacts":5,"width_mm":120,"height_mm":70,"clickable":false,"pressure":true}), Instant::now()+Duration::from_secs(1))?;
    loop {
        check(deadline)?;
        if let Some(bytes) = io.poll_frame(LOCAL_FRAME)? {
            let command = touchpad_command(bytes.as_slice())?;
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
                validate_touchpad_source(runtime, config.source)?;
            }
            Some(_) => return Err(fail()),
            None => {}
        }
        thread::sleep(Duration::from_millis(2));
    }
    check(deadline)?;
    validate_touchpad_source(runtime, config.source)?;
    let approved_expiry = now_ms()?
        .checked_add(config.duration_secs * 1000)
        .ok_or_else(fail)?;
    let deadline = Instant::now() + CONSENT.min(Duration::from_secs(config.duration_secs));
    sanitized(link.send_route_status_until(
        sanitized(runtime.route(route))?,
        negotiation_operation_deadline(deadline, approved_expiry)?,
    ))?;
    let peer_expiry = loop {
        let Some(status) = touchpad_grant(
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
        let Some(status) =
            touchpad_grant(io, runtime, link, config.source, route, deadline, expiry)?
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
    // Leave enough time for the independently authorized Source lease.
    // Neither side starts the data plane before the exact lease response.
    lease_ttl(expiry, now)?;
    sanitized(runtime.set_local_port_lease_policy(sink, PortLeasePolicy::Exclusive))?;
    sanitized(runtime.acquire_route_port_lease(route, sink, expiry, now))?;
    sanitized(runtime.prepare_route(
        route,
        Some(FormatDescriptor::new(FORMAT)),
        QosMode::Interactive,
        now,
    ))?;
    Ok(Some(route))
}
fn touchpad_grant(
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
        if touchpad_before_approval(io, "awaiting_peer_consent")? {
            return Ok(None);
        }
        match sanitized(link.poll_control_message_until(operation))? {
            Some(ReceivedControlMessage::Catalog(c)) => {
                sanitized(link.apply_catalog(runtime, c))?;
                validate_touchpad_source(runtime, source)?;
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
fn validate_touchpad_source(runtime: &NodeRuntime, source: PortRef) -> io::Result<()> {
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
        || cap.class != CapabilityClass::Touchpad
        || cap.availability != Availability::Available
        || port.direction != PortDirection::Source
        || port.profile != ProfileId::touchpad_frames_v1()
        || port.availability != Availability::Available
        || port.interoperability_mode != InteroperabilityMode::AdapterManaged
        || !port.formats.contains(&FormatDescriptor::new(FORMAT))
        || !port.qos_modes.contains(&QosMode::Interactive)
    {
        return Err(fail());
    }
    Ok(())
}
fn touchpad_runtime(node_id: NodeId) -> io::Result<(NodeRuntime, PortRef)> {
    let mut node = NodeDescriptor::new(
        node_id,
        "LinuxTouchpadSink",
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
        adapter_type: "capyio.linux.touchpad".into(),
        display_name: "Owned finite Type-B touchpad".into(),
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
        display_name: "LinuxTouchpadSink".into(),
        class: CapabilityClass::Touchpad,
        availability: Availability::Available,
        permission_requirement: PermissionRequirement::UserConfirmation,
        metadata: BTreeMap::from([
            ("transport".into(), "capyio-touchpad/1 mTLS".into()),
            ("max_contacts".into(), "5".into()),
            ("width_mm".into(), "120".into()),
            ("height_mm".into(), "70".into()),
            ("clickable".into(), "false".into()),
            ("pressure".into(), "true".into()),
            (
                "evidence".into(),
                "native_projection_requires_runtime_open".into(),
            ),
        ]),
        ports: BTreeMap::from([(
            port,
            PortDescriptor {
                id: port,
                capability_id: capability,
                display_name: "Five-contact Type-B touchpad Sink".into(),
                direction: PortDirection::Sink,
                profile: ProfileId::touchpad_frames_v1(),
                schema_id: None,
                formats: vec![FormatDescriptor::new(FORMAT)],
                qos_modes: BTreeSet::from([QosMode::Interactive]),
                clock_domain: None,
                availability: Availability::Available,
                permission_requirement: PermissionRequirement::UserConfirmation,
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
    fn touchpad_bootstrap_requires_exact_source_and_finite_private_listener() {
        let value = config();
        assert!(TouchpadConfig::parse(&serde_json::to_vec(&value).unwrap()).is_ok());
        for seconds in [0, 1, 2, 61, u64::MAX] {
            let mut v = value.clone();
            v["duration_secs"] = json!(seconds);
            assert!(TouchpadConfig::parse(&serde_json::to_vec(&v).unwrap()).is_err());
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
            assert!(TouchpadConfig::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        }
        for field in ["target", "media", "helper", "password", "proposal_mode"] {
            let mut v = value.clone();
            v[field] = json!("forbidden");
            assert!(TouchpadConfig::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        }
        for field in ["node_id", "capability_id", "port_id"] {
            let mut v = value.clone();
            v["source"][field] = json!("00000000-0000-0000-0000-000000000000");
            assert!(TouchpadConfig::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        }
        let mut v = value.clone();
        v["source"]["node_id"] = v["node_id"].clone();
        assert!(TouchpadConfig::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        let mut v = value.clone();
        v["source"]["extra"] = json!(true);
        assert!(TouchpadConfig::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        let mut v = value.clone();
        let source = &v["source"];
        v["source"] = json!([
            source["node_id"],
            source["capability_id"],
            source["port_id"]
        ]);
        assert!(TouchpadConfig::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        assert!(TouchpadConfig::parse(b"[]").is_err());
        let encoded = serde_json::to_string(&value).unwrap();
        let duplicate = encoded.replacen('{', "{\"version\":1,", 1);
        assert!(TouchpadConfig::parse(duplicate.as_bytes()).is_err());
    }
    #[test]
    fn touchpad_catalog_is_a_real_single_linux_sink_without_fabricated_peer() {
        let (runtime, sink) = touchpad_runtime(NodeId::new()).unwrap();
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
        assert_eq!(port.profile, ProfileId::touchpad_frames_v1());
        assert_eq!(
            port.permission_requirement,
            PermissionRequirement::UserConfirmation
        );
        let capability = &snapshot.local_node.capabilities[&sink.capability_id];
        assert_eq!(capability.class, CapabilityClass::Touchpad);
        assert_eq!(capability.metadata["max_contacts"], "5");
        assert_eq!(capability.metadata["width_mm"], "120");
        assert_eq!(capability.metadata["height_mm"], "70");
        assert_eq!(capability.metadata["clickable"], "false");
        assert_eq!(capability.metadata["pressure"], "true");
        assert!(validate_touchpad_source(&runtime, sink).is_err());
    }
}
