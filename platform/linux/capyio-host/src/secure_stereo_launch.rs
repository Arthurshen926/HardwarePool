//! Explicit paired-store reverse stereo composition. No implicit grant or Source claim.
use super::*;
use capyio_control_tls::ControlTlsClientIdentity;
use capyio_linux_audio::{
    SecureStereoSourceEvent, SecureStereoSourceHandshake, SecureStereoSourceHandshakeEvent,
    SecureStereoSourceIntent, SecureStereoSourceOwner,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    io: &mut LauncherIo,
    config: &LaunchConfig,
    mut runtime: NodeRuntime,
    mut link: CatalogControlLink,
    source: PortRef,
    identity: ControlTlsServerIdentity,
    server_name: String,
) -> io::Result<()> {
    let catalog_end = Instant::now() + CATALOG;
    let setup = (|| {
        loop {
            check(catalog_end)?;
            if local_before_approval(io, "catalog")? {
                return Ok(false);
            }
            match sanitized(link.poll_control_message_until(catalog_end))? {
                Some(ReceivedControlMessage::Catalog(catalog)) => {
                    sanitized(link.apply_catalog(&mut runtime, catalog))?;
                    break;
                }
                Some(_) => return Err(fail()),
                None => thread::sleep(Duration::from_millis(2)),
            }
        }
        validate_sink(&runtime, config.source)?;
        let catalog =
            sanitized(link.prepare_catalog(&runtime, &BTreeSet::from([source.capability_id])))?;
        sanitized(link.send_catalog_until(catalog, catalog_end))?;
        // A complete or partial pre-input must not become a fresh approval.
        if io.poll_frame(LOCAL_FRAME)?.is_some() || io.has_pending_frame() {
            return Err(fail());
        }
        Ok(true)
    })();
    match setup {
        Ok(true) => {}
        Ok(false) => {
            sanitized(link.close_and_reconcile(&mut runtime))?;
            return stopped(io);
        }
        Err(_) => {
            let _ = link.close_and_reconcile(&mut runtime);
            return Err(fail());
        }
    }
    let session_id = link.session_id();
    let mut handshake = sanitized(SecureStereoSourceHandshake::propose(
        runtime,
        link,
        SecureStereoSourceIntent {
            source,
            sink: config.source,
            pipewire_target: config.target.clone().ok_or_else(fail)?,
            lifetime: Duration::from_secs(config.duration_secs),
        },
    ))?;
    let route_id = handshake.route_id().ok_or_else(fail)?;
    let request_id = MessageId::new();
    let consent_end = Instant::now() + CONSENT;
    io.send_json(
        &json!({"version":1,"event":"approval_required","request_id":request_id,
        "route_id":route_id,"session_id":session_id,"source":source,"sink":config.source,
        "target":config.target,"endpoint_mode":"stereo_capture","node_name":null,
        "format":capyio_protocol::SECURE_AUDIO_STEREO_FORMAT,"duration_secs":config.duration_secs,
        "approval_timeout_secs":CONSENT.as_secs(),"media_security":config.media_security.name()}),
        consent_end.min(Instant::now() + Duration::from_secs(1)),
    )?;
    let mut approved = false;
    loop {
        if !approved {
            check(consent_end)?;
        }
        if let Some(bytes) = io.poll_frame(LOCAL_FRAME)? {
            let command = command(bytes.as_slice())?;
            if command.simple("stop") {
                sanitized(handshake.cancel())?;
                return stopped(io);
            }
            if command.simple("status") {
                io.send_json(&json!({"version":1,"event":"status","stage":if approved {"awaiting_peer_approval"} else {"awaiting_local_approval"}}), Instant::now()+Duration::from_secs(1))?;
            } else if !approved
                && command.command == "approve"
                && command.request_id == Some(request_id)
                && command.route_id == Some(route_id)
            {
                sanitized(handshake.approve(true))?;
                approved = true;
                io.send_json(
                    &json!({"version":1,"event":"awaiting_peer_approval","route_id":route_id}),
                    Instant::now() + Duration::from_secs(1),
                )?;
            } else {
                return Err(fail());
            }
        }
        match sanitized(handshake.poll())? {
            SecureStereoSourceHandshakeEvent::Ready if approved => break,
            SecureStereoSourceHandshakeEvent::Terminal
            | SecureStereoSourceHandshakeEvent::Ready => return Err(fail()),
            _ => {}
        }
        thread::sleep(Duration::from_millis(2));
    }
    let identity = ControlTlsClientIdentity {
        expected_peer_certificate: identity.expected_peer_certificate,
        certificate_chain_der: identity.certificate_chain_der,
        private_key_der: identity.private_key_der,
        server_ca_der: identity.client_ca_der,
    };
    let mut owner = sanitized(handshake.into_owner(identity, server_name))?;
    let outcome = (|| {
        io.send_json(
            &json!({"version":1,"event":"awaiting_peer_offer","route_id":route_id}),
            Instant::now() + Duration::from_secs(1),
        )?;
        loop {
            if let Some(bytes) = io.poll_frame(LOCAL_FRAME)? {
                let command = command(bytes.as_slice())?;
                if command.simple("stop") {
                    sanitized(owner.request_stop())?;
                    break;
                }
                if !command.simple("status") {
                    return Err(fail());
                }
                let mut event = metrics(&owner);
                event["event"] = json!("status");
                event["route_id"] = json!(route_id);
                event["state"] = json!(sanitized(owner.runtime().route(route_id))?.state);
                event["media_security"] = json!(config.media_security.name());
                io.send_json(&event, Instant::now() + Duration::from_secs(1))?;
            }
            match sanitized(owner.poll())? {
                Some(SecureStereoSourceEvent::Stopped) => break,
                Some(SecureStereoSourceEvent::Bound) => io.send_json(
                    &json!({"version":1,"event":"bound","route_id":route_id}),
                    Instant::now() + Duration::from_secs(1),
                )?,
                _ => {}
            }
            thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    })();
    let cause = owner.terminal_cause().unwrap_or(if outcome.is_err() {
        "failure"
    } else {
        "local_stop"
    });
    let cleanup = owner.stop();
    let failed = outcome.is_err() || cleanup.is_err();
    let mut event = metrics(&owner);
    event["event"] = json!(if failed { "failed" } else { "stopped" });
    event["terminal_cause"] = json!(cause);
    if let Some(at) = owner.terminal_observed_at_ms() {
        event["terminal_observed_at_ms"] = json!(at);
    }
    io.send_json(&event, Instant::now() + Duration::from_secs(1))?;
    if failed {
        Err(io::Error::other(SecureTerminalReported))
    } else {
        Ok(())
    }
}

fn command(bytes: &[u8]) -> io::Result<LocalCommand> {
    if bytes.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{') {
        return Err(fail());
    }
    LocalCommand::parse(bytes)
}

fn metrics(owner: &SecureStereoSourceOwner) -> serde_json::Value {
    let metrics = owner.metrics();
    json!({"version":1,"packets_sent":owner.packets_sent(),"packets_captured":metrics.packets_captured,
        "bytes_read":metrics.bytes_read,"transport_bytes_sent":owner.packets_sent().saturating_mul(2128)})
}
fn validate_sink(runtime: &NodeRuntime, reference: PortRef) -> io::Result<()> {
    validate_remote_endpoint(
        runtime,
        reference,
        PortDirection::Sink,
        capyio_protocol::SECURE_AUDIO_STEREO_FORMAT,
    )?;
    let snapshot = runtime.snapshot();
    let peer = snapshot
        .peers
        .iter()
        .find(|peer| peer.id == reference.node_id)
        .ok_or_else(fail)?;
    let capability = peer
        .capabilities
        .get(&reference.capability_id)
        .ok_or_else(fail)?;
    let port = sanitized(peer.port(reference.capability_id, reference.port_id))?;
    if peer.platform != Platform::Android
        || peer.online_state != OnlineState::Online
        || capability.class != CapabilityClass::Speaker
        || capability.availability != Availability::Available
        || port.formats
            != [FormatDescriptor::new(
                capyio_protocol::SECURE_AUDIO_STEREO_FORMAT,
            )]
        || port.qos_modes != BTreeSet::from([QosMode::Interactive])
        || port.interoperability_mode != InteroperabilityMode::AdapterManaged
    {
        return Err(fail());
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../capyio-audio/tests/support/bilateral_fixture.rs"]
mod bilateral_fixture;

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> serde_json::Value {
        json!({"version":1,"node_id":NodeId::new(),"sink":PortRef{node_id:NodeId::new(),capability_id:CapabilityId::new(),port_id:PortId::new()},"listen":"127.0.0.1:0","media":"127.0.0.1:0","target":"explicit.selected.source","duration_secs":10,"media_security":"pinned_tls13_stereo_v1"})
    }
    #[test]
    fn local_stereo_commands_reject_positional_and_unknown_authority() {
        assert!(command(br#"[1,"approve",null,null]"#).is_err());
        assert!(command(br#"{"version":1,"command":"approve","peer":"forged"}"#).is_err());
        assert!(command(br#"{"version":1,"version":1,"command":"stop"}"#).is_err());
        assert!(
            command(br#"{"version":1,"command":"stop"}"#)
                .unwrap()
                .simple("stop")
        );
    }
    #[test]
    fn explicit_stereo_config_is_finite_paired_capture_only() {
        let value = config();
        let parsed = parse_capture(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(parsed.media_security, MediaSecurity::PinnedTls13StereoV1);
        assert!(validate_credential_source(&parsed, &CredentialSource::PrivatePipe).is_err());
        assert!(LaunchConfig::parse(&serde_json::to_vec(&value).unwrap()).is_err());
        for (key, bad) in [
            ("duration_secs", json!(0)),
            ("duration_secs", json!(61)),
            ("target", json!("auto")),
            ("target", json!("default")),
            ("target", json!("0")),
            ("target", json!(null)),
            ("media", json!("127.0.0.1:1234")),
            ("endpoint_mode", json!("virtual_speaker")),
            ("media_security", json!("pinned_tls13_mono_v1")),
            ("media_security", json!("pinned_tls13_mono_300s_v1")),
        ] {
            let mut invalid = value.clone();
            invalid[key] = bad;
            assert!(
                parse_capture(&serde_json::to_vec(&invalid).unwrap()).is_err(),
                "{invalid}"
            );
        }
        for duration in [1, 60] {
            let mut valid = value.clone();
            valid["duration_secs"] = json!(duration);
            assert!(parse_capture(&serde_json::to_vec(&valid).unwrap()).is_ok());
        }
    }
    #[test]
    fn remote_sink_is_exact_android_speaker_not_format_name_alone() {
        let (peer_runtime, reference) = local_runtime_profile(
            NodeId::new(),
            ProposalMode::LinuxLed,
            EndpointMode::Playback,
            MediaSecurity::TrustedLabUdp,
        )
        .unwrap();
        let mut peer = peer_runtime.snapshot().local_node;
        peer.platform = Platform::Android;
        peer.capabilities
            .get_mut(&reference.capability_id)
            .unwrap()
            .ports
            .get_mut(&reference.port_id)
            .unwrap()
            .formats = vec![FormatDescriptor::new(
            capyio_protocol::SECURE_AUDIO_STEREO_FORMAT,
        )];
        let check = |peer| {
            let (mut runtime, _) = local_runtime_profile(
                NodeId::new(),
                ProposalMode::LinuxLed,
                EndpointMode::StereoCapture,
                MediaSecurity::PinnedTls13StereoV1,
            )
            .unwrap();
            runtime.register_peer(peer, true).unwrap();
            validate_sink(&runtime, reference)
        };
        assert!(check(peer.clone()).is_ok());
        let mut wrong = peer.clone();
        wrong.platform = Platform::Linux;
        assert!(check(wrong).is_err());
        let mut wrong = peer.clone();
        wrong
            .capabilities
            .get_mut(&reference.capability_id)
            .unwrap()
            .class = CapabilityClass::Microphone;
        assert!(check(wrong).is_err());
        let mut wrong = peer.clone();
        wrong
            .capabilities
            .get_mut(&reference.capability_id)
            .unwrap()
            .ports
            .get_mut(&reference.port_id)
            .unwrap()
            .formats
            .push(FormatDescriptor::new("pcm-s16le-48000-stereo"));
        assert!(check(wrong).is_err());
        let mut wrong = peer.clone();
        wrong
            .capabilities
            .get_mut(&reference.capability_id)
            .unwrap()
            .ports
            .get_mut(&reference.port_id)
            .unwrap()
            .formats[0]
            .parameters
            .insert("unrecognized".into(), "value".into());
        assert!(check(wrong).is_err());
        let mut wrong = peer;
        wrong
            .capabilities
            .get_mut(&reference.capability_id)
            .unwrap()
            .ports
            .get_mut(&reference.port_id)
            .unwrap()
            .qos_modes
            .insert(QosMode::Basic);
        assert!(check(wrong).is_err());
    }
    #[test]
    fn actual_launcher_catalog_enters_bilateral_tls_pending_and_fresh_approval() {
        use capyio_control_tls::{
            AuthenticatedControlClientChannel, ControlTlsBinding, ControlTlsEndpoint,
        };
        let (mut runtime, source) = local_runtime_profile(
            NodeId::new(),
            ProposalMode::LinuxLed,
            EndpointMode::StereoCapture,
            MediaSecurity::PinnedTls13StereoV1,
        )
        .unwrap();
        let (mut android, _, _, sink) = bilateral_fixture::nodes();
        let cap = android.capabilities.get_mut(&sink.capability_id).unwrap();
        cap.metadata
            .insert("speaker.profile".into(), "secure_stereo_tls13_v1".into());
        cap.metadata
            .insert("speaker.selectionRevision".into(), "1".into());
        let mut peer_runtime = NodeRuntime::new(android).unwrap();
        let binding =
            ControlTlsBinding::new(SessionId::new(), source.node_id, sink.node_id).unwrap();
        let (_, android_identity, linux_identity, _) = bilateral_fixture::credentials();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = AuthenticatedControlServer::from_listener(
            listener,
            linux_identity,
            binding,
            Duration::from_secs(2),
        )
        .unwrap();
        let accepting = thread::spawn(move || server.accept().unwrap());
        let channel = AuthenticatedControlClientChannel::connect(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(2),
            },
            "localhost",
            android_identity,
            binding,
        )
        .unwrap();
        let mut peer_link =
            CatalogControlLink::admit_dialed_peer(&mut peer_runtime, channel, "Linux".into())
                .unwrap();
        let mut link = CatalogControlLink::admit_accepted_peer(
            &mut runtime,
            accepting.join().unwrap(),
            "Android".into(),
        )
        .unwrap();
        let catalog = peer_link
            .prepare_catalog(&peer_runtime, &BTreeSet::new())
            .unwrap();
        peer_link.send_catalog(catalog).unwrap();
        let ReceivedControlMessage::Catalog(catalog) = link.receive_control_message().unwrap()
        else {
            panic!("Android catalog")
        };
        link.apply_catalog(&mut runtime, catalog).unwrap();
        validate_sink(&runtime, sink).unwrap();
        let catalog = link
            .prepare_catalog(&runtime, &BTreeSet::from([source.capability_id]))
            .unwrap();
        link.send_catalog(catalog).unwrap();
        let ReceivedControlMessage::Catalog(catalog) = peer_link.receive_control_message().unwrap()
        else {
            panic!("actual Linux catalog")
        };
        peer_link.apply_catalog(&mut peer_runtime, catalog).unwrap();
        let mut handshake = SecureStereoSourceHandshake::propose(
            runtime,
            link,
            SecureStereoSourceIntent {
                source,
                sink,
                pipewire_target: "explicit.synthetic.source".into(),
                lifetime: Duration::from_secs(10),
            },
        )
        .unwrap();
        let ReceivedControlMessage::RouteStatus(pending) =
            peer_link.receive_control_message().unwrap()
        else {
            panic!("Pending")
        };
        assert_eq!(pending.route.authorization, AuthorizationState::Pending);
        assert_eq!(pending.route.state, RouteState::Draft);
        assert!(handshake.runtime().snapshot().port_leases.is_empty());
        handshake.approve(true).unwrap();
        let ReceivedControlMessage::RouteStatus(authorized) =
            peer_link.receive_control_message().unwrap()
        else {
            panic!("fresh Authorized Draft")
        };
        assert!(matches!(
            authorized.route.authorization,
            AuthorizationState::Authorized {
                expires_at_ms: Some(_)
            }
        ));
        assert!(handshake.runtime().snapshot().port_leases.is_empty());
        handshake.cancel().unwrap();
        let route = handshake.runtime().route(pending.route.id).unwrap();
        assert_eq!(route.authorization, AuthorizationState::Revoked);
        assert_eq!(route.state, RouteState::Failed);
    }
    #[test]
    fn stereo_publication_is_eligibility_without_routes_or_claims() {
        let (runtime, source) = local_runtime_profile(
            NodeId::new(),
            ProposalMode::LinuxLed,
            EndpointMode::StereoCapture,
            MediaSecurity::PinnedTls13StereoV1,
        )
        .unwrap();
        let snapshot = runtime.snapshot();
        let capability = &snapshot.local_node.capabilities[&source.capability_id];
        assert_eq!(
            capability.class,
            CapabilityClass::Custom("system_audio_capture".into())
        );
        assert_eq!(
            capability
                .metadata
                .get("capyio.secure_stereo.bilateral_consent")
                .map(String::as_str),
            Some("v1")
        );
        assert_eq!(
            capability.ports[&source.port_id].formats,
            [FormatDescriptor::new(
                capyio_protocol::SECURE_AUDIO_STEREO_FORMAT
            )]
        );
        assert!(snapshot.routes.is_empty());
        assert!(snapshot.port_leases.is_empty());
        for endpoint in [
            EndpointMode::Playback,
            EndpointMode::VirtualMicrophone,
            EndpointMode::VirtualSpeaker,
        ] {
            assert!(
                local_runtime_profile(
                    NodeId::new(),
                    ProposalMode::LinuxLed,
                    endpoint,
                    MediaSecurity::PinnedTls13StereoV1
                )
                .is_err()
            );
        }
    }
}
