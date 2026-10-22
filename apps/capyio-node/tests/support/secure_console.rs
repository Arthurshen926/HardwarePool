//! Explicit hosted-only production console test. Synthetic Rust PCM is not
//! evidence of Java AudioRecord, a physical phone, or consumer Active delivery.
use super::*;
use std::process::Child;
const PREFIX: &str = ".capyio-secure-console-";
const SCENARIOS: [&str; 11] = [
    "stop",
    "no_approval",
    "pretyped",
    "wrong_request",
    "wrong_route",
    "wrong_peer",
    "source_revoke",
    "expiry",
    "control_eof",
    "media_eof",
    "local_eof",
];
// The unchanged graph harness permits ten seconds from links-ready to the
// stopped checkpoint. Expiry must leave room for actual finite teardown, not
// ask the harness to wait less than the deliberately approved session itself.
fn consent_seconds(scenario: &str) -> u64 {
    if scenario == "expiry" { 8 } else { 12 }
}
#[test]
fn pure_secure_console_expiry_fits_existing_checkpoint_without_shortening_waveform() {
    let expiry = consent_seconds("expiry");
    assert_eq!(expiry, 8);
    assert!(expiry + 2 <= 10); // Existing checkpoint; two seconds for teardown.
    assert!(expiry >= 3 + 3); // Full three-second waveform plus setup reserve.
    for scenario in SCENARIOS.into_iter().filter(|name| *name != "expiry") {
        assert_eq!(consent_seconds(scenario), 12);
    }
}
fn negative(name: &str) -> bool {
    matches!(
        name,
        "no_approval" | "pretyped" | "wrong_request" | "wrong_route" | "wrong_peer"
    )
}
fn marker(root: &Path, name: &str) -> PathBuf {
    root.join(format!("{PREFIX}{name}"))
}
fn wait_marker(root: &Path, name: &str) {
    let until = Instant::now() + BOUND;
    while !marker(root, name).is_file() {
        assert!(
            Instant::now() < until,
            "independent checkpoint {name} absent"
        );
        thread::sleep(TICK);
    }
}
struct Process {
    child: Child,
    console: playback_console::Console,
    pairing: paired_store::PersistedPairing,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    owner: u32,
    endpoint: Option<Value>,
}
impl Process {
    fn collect(&mut self) {
        self.console
            .collect(&mut self.child, &mut self.stdout, &mut self.stderr);
        for bytes in [&self.console.tty, &self.stdout, &self.stderr] {
            self.pairing.assert_no_leak(bytes);
        }
    }
    fn event(&mut self, expected: &str) -> Value {
        let until = Instant::now() + BOUND;
        loop {
            self.collect();
            if let Some(value) = self.console.next_event() {
                assert_eq!(value["event"], expected, "unexpected production event");
                return value;
            }
            assert!(Instant::now() < until, "production event {expected} absent");
            thread::sleep(TICK);
        }
    }
    fn prompt(&mut self, approval: &Value) {
        let until = Instant::now() + BOUND;
        loop {
            self.collect();
            if self.console.prompt_seen(
                approval["request_id"].as_str().unwrap(),
                approval["route_id"].as_str().unwrap(),
            ) {
                self.console.assert_hidden();
                thread::sleep(Duration::from_millis(25));
                return;
            }
            assert!(Instant::now() < until && self.child.try_wait().unwrap().is_none());
            thread::sleep(TICK);
        }
    }
    fn status_prompt(&mut self) {
        let until = Instant::now() + BOUND;
        loop {
            self.collect();
            if self.console.status_prompt_seen() {
                self.console.assert_hidden();
                // begin_prompt flushes again immediately after displaying it.
                thread::sleep(Duration::from_millis(25));
                return;
            }
            assert!(Instant::now() < until && self.child.try_wait().unwrap().is_none());
            thread::sleep(TICK);
        }
    }
    fn children(&self) -> Vec<u32> {
        fs::read_to_string(format!("/proc/{}/task/{}/children", self.owner, self.owner))
            .unwrap()
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect()
    }
    fn publish(&self, root: &Path, scenario: &str, stage: &str, extra: Value) {
        let mut value = json!({"version":1,"scenario":scenario,"stage":stage,"format":SECURE_AUDIO_MONO_FORMAT,
            "surface":"production_console","owner_pid":self.owner,"console_pid":self.child.id(),"fixture_pid":std::process::id()});
        if let Some(endpoint) = &self.endpoint {
            value
                .as_object_mut()
                .unwrap()
                .extend(endpoint.as_object().unwrap().clone());
        }
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let path = marker(root, &format!("{stage}.json"));
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec(&value).unwrap()).unwrap();
        fs::rename(tmp, path).unwrap();
    }
    fn pending(&self, root: &Path, scenario: &str, stage: &str, runtime: &NodeRuntime) {
        assert!(self.children().is_empty());
        assert!(runtime.snapshot().port_leases.is_empty());
        self.publish(
            root,
            scenario,
            stage,
            json!({"helper_absent":true,"lease_count":0,"bytes_written":0}),
        );
    }
    fn finish(&mut self) -> bool {
        let until = Instant::now() + BOUND;
        loop {
            self.collect();
            if let Some(status) = self.child.try_wait().unwrap() {
                self.collect();
                self.console.assert_finished();
                let state = self.pairing.service.state.lock().unwrap();
                assert!(state.reads > 0);
                assert_eq!(state.forbidden, 0);
                return status.success();
            }
            assert!(Instant::now() < until, "console/Node cleanup bounded");
            thread::sleep(TICK);
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: this fixture created the console's private session/group.
        unsafe {
            libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
#[ignore = "requires mandatory hosted private secure-console graph and paired-store build"]
fn hosted_secure_audio_console_graph() {
    assert_eq!(
        std::env::var("CAPYIO_SECURE_CONSOLE_HOSTED").as_deref(),
        Ok("1")
    );
    assert_eq!(std::env::var("CAPYIO_PIPEWIRE_TEST").as_deref(), Ok("1"));
    assert_eq!(
        std::env::var("PIPEWIRE_REMOTE").as_deref(),
        Ok("capyio-test")
    );
    let root = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap());
    assert_eq!(
        fs::read_to_string(root.join(".capyio-pipewire-loopback")).unwrap(),
        "capyio-hosted-pipewire-loopback-v1\n"
    );
    let scenario = std::env::var("CAPYIO_SECURE_CONSOLE_SCENARIO").unwrap();
    assert!(SCENARIOS.contains(&scenario.as_str()));
    let endpoint_mode =
        std::env::var("CAPYIO_SECURE_CONSOLE_ENDPOINT_MODE").unwrap_or_else(|_| "playback".into());
    assert!(matches!(
        endpoint_mode.as_str(),
        "playback" | "virtual_microphone"
    ));
    let (mut runtime, source) = source_runtime();
    let local = NodeId::new();
    let (server, client) = identities();
    let mut directory = PairedPeerDirectory::new(local).unwrap();
    directory
        .add(
            PairedPeerRecord::new(
                source.node_id,
                "Synthetic Source",
                "synthetic-audio-client",
                CredentialHandle::new("ephemeral.console.audio").unwrap(),
                server.expected_peer_certificate,
                1,
            )
            .unwrap(),
        )
        .unwrap();
    let bundle = ControlCredentialBundle::new(
        server.certificate_chain_der,
        server.private_key_der,
        server.client_ca_der,
    )
    .unwrap();
    let pairing =
        paired_store::PersistedPairing::new(local, &directory.encode_json().unwrap(), &bundle);
    let mut config = json!({"version":1,"node_id":local,"source":source,"listen":"127.0.0.1:0","media":"127.0.0.1:0",
        "target":"capyio.test.null","endpoint_mode":"playback","proposal_mode":"linux_led","duration_secs":consent_seconds(&scenario),
        "media_security":"pinned_tls13_mono_v1"});
    config["endpoint_mode"] = json!(endpoint_mode);
    if endpoint_mode == "virtual_microphone" {
        config.as_object_mut().unwrap().remove("target");
    }
    let (child, console) = playback_console::Console::start(
        &pairing,
        &config,
        scenario == "pretyped",
        "playback",
        &[],
    );
    let mut process = Process {
        child,
        console,
        pairing,
        stdout: vec![],
        stderr: vec![],
        owner: 0,
        endpoint: None,
    };
    let listening = process.event("listening");
    process.owner = process.console.identify_node(&process.child);
    for pid in [process.child.id(), process.owner] {
        for name in ["cmdline", "environ"] {
            process
                .pairing
                .assert_no_leak(&fs::read(format!("/proc/{pid}/{name}")).unwrap());
        }
    }
    let nodes = ControlTlsNodeBinding::new(local, source.node_id).unwrap();
    let mut source_directory = PairedPeerDirectory::new(source.node_id).unwrap();
    source_directory
        .add(
            PairedPeerRecord::new(
                local,
                "Synthetic Linux",
                "localhost",
                CredentialHandle::new("ephemeral.console.source").unwrap(),
                client.expected_peer_certificate,
                1,
            )
            .unwrap(),
        )
        .unwrap();
    let mut credentials = ControlCredentialBundle::new(
        client.certificate_chain_der.clone(),
        client.private_key_der.clone(),
        client.server_ca_der.clone(),
    )
    .unwrap()
    .encode()
    .unwrap();
    let channel = AuthenticatedControlClientChannel::connect_fresh_session(
        ControlTlsEndpoint {
            address: listening["address"].as_str().unwrap().parse().unwrap(),
            io_timeout: Duration::from_secs(2),
        },
        "localhost",
        client,
        nodes,
    )
    .unwrap();
    let mut link =
        CatalogControlLink::admit_dialed_new_peer(&mut runtime, channel, "Synthetic Linux".into())
            .unwrap();
    let session = link.session_id();
    let catalog = link
        .prepare_catalog(&runtime, &BTreeSet::from([source.capability_id]))
        .unwrap();
    link.send_catalog(catalog).unwrap();
    let catalog = link.receive_catalog().unwrap();
    link.apply_catalog(&mut runtime, catalog).unwrap();
    let approval = process.event("approval_required");
    assert_eq!(approval["source"], serde_json::to_value(source).unwrap());
    assert_eq!(
        approval["sink"]["node_id"],
        serde_json::to_value(local).unwrap()
    );
    assert_eq!(
        approval["session_id"],
        serde_json::to_value(session).unwrap()
    );
    assert_eq!(approval["endpoint_mode"], endpoint_mode);
    if endpoint_mode == "virtual_microphone" {
        let sink_port = approval["sink"]["port_id"].as_str().unwrap();
        assert_eq!(
            approval["node_name"],
            format!("capyio.virtual-microphone.{sink_port}")
        );
        assert!(approval.get("target").is_none_or(Value::is_null));
        process.endpoint = Some(json!({"endpoint_mode":endpoint_mode,
            "sink_port_id":sink_port,"node_name":approval["node_name"]}));
    }
    process.prompt(&approval);
    assert_eq!(approval["format"], SECURE_AUDIO_MONO_FORMAT);
    assert_eq!(approval["media_security"], "pinned_tls13_mono_v1");
    process.console.assert_presentation(&approval);
    process.pending(&root, &scenario, "before_local", &runtime);
    wait_marker(&root, "local_checked");
    let mut helper = None;
    let mut expiry = None;
    let mut packets = 0;
    let mut bytes = 0;
    let mut sent = 0;
    let mut attempted = 0;
    let observed;
    let terminal_cause;
    if negative(&scenario) {
        terminal_cause = "not_started".to_owned();
        assert!(runtime.snapshot().routes.is_empty());
        if scenario.starts_with("wrong_") {
            process.console.send(
                format!(
                    "start {} {} {}\n",
                    if scenario == "wrong_request" {
                        MessageId::new().to_string()
                    } else {
                        approval["request_id"].as_str().unwrap().into()
                    },
                    if scenario == "wrong_route" {
                        RouteId::new().to_string()
                    } else {
                        approval["route_id"].as_str().unwrap().into()
                    },
                    if scenario == "wrong_peer" {
                        NodeId::new()
                    } else {
                        source.node_id
                    }
                )
                .as_bytes(),
            );
            observed = now_ms();
            assert!(!process.finish());
        } else {
            process.console.send(b"stop\n");
            process.event("stopped");
            observed = now_ms();
            assert!(process.finish());
        }
        assert!(
            link.receive_control_message_until(Instant::now() + BOUND)
                .is_err(),
            "no proposal without local consent"
        );
    } else {
        process.console.send(
            format!(
                "start {} {} {}\n",
                approval["request_id"].as_str().unwrap(),
                approval["route_id"].as_str().unwrap(),
                source.node_id
            )
            .as_bytes(),
        );
        let ReceivedControlMessage::RouteStatus(status) = link
            .receive_control_message_until(Instant::now() + BOUND)
            .unwrap()
        else {
            panic!("exact Draft proposal required")
        };
        assert_eq!(status.route.authorization, AuthorizationState::Pending);
        assert_eq!(status.route.state, RouteState::Draft);
        assert!(status.secure_audio_data.is_none() && status.native_audio_data.is_none());
        let route = status.route.id;
        assert_eq!(serde_json::to_value(route).unwrap(), approval["route_id"]);
        assert_eq!(status.route.source, source);
        assert_eq!(
            serde_json::to_value(status.route.sink).unwrap(),
            approval["sink"]
        );
        link.apply_route_status(&mut runtime, &status, false)
            .unwrap();
        let mut consent = AudioSourceConsent::default();
        consent.enqueue(&mut runtime, route, now_ms()).unwrap();
        link.send_route_status(runtime.route(route).unwrap())
            .unwrap();
        process.pending(&root, &scenario, "before_source", &runtime);
        wait_marker(&root, "source_checked");
        let now = now_ms();
        consent
            .resolve(
                &mut runtime,
                route,
                true,
                now,
                now + consent_seconds(&scenario) * 1000,
            )
            .unwrap();
        link.send_route_status(runtime.route(route).unwrap())
            .unwrap();
        let offer = receive_offer(&mut runtime, &mut link, route);
        process.event("awaiting_peer_offer");
        let original_expiry = runtime
            .route_port_lease_claim(route, source)
            .unwrap()
            .expires_at_ms;
        let AuthorizationState::Authorized {
            expires_at_ms: Some(grant_expiry),
        } = runtime.route(route).unwrap().authorization
        else {
            panic!("finite Source consent required")
        };
        assert!(
            original_expiry < grant_expiry,
            "retain actual narrowed Source lease, not the longer consent grant"
        );
        expiry = Some(original_expiry);
        let mut sender = SecureAudioSender::connect(
            &runtime,
            &mut link,
            route,
            offer,
            &source_directory.encode_json().unwrap(),
            &credentials,
            now_ms(),
        )
        .unwrap();
        credentials.fill(0);
        let mut samples = Samples::new(&sender);
        let ready = Instant::now() + Duration::from_secs(5);
        while helper.is_none() {
            samples.send(&mut sender, &runtime, 0).unwrap();
            let children = process.children();
            assert!(children.len() <= 1);
            if let Some(pid) = children.first() {
                helper = Some(*pid);
            }
            assert!(Instant::now() < ready);
        }
        process.publish(
            &root,
            &scenario,
            "bound",
            json!({"helper_pid":helper,"source_lease_expires_at_ms":expiry}),
        );
        while !marker(&root, "links_ready").is_file() {
            assert!(Instant::now() < ready);
            samples.send(&mut sender, &runtime, 0).unwrap();
        }
        for index in 0..300 {
            samples
                .send(
                    &mut sender,
                    &runtime,
                    if index < 120 {
                        1000
                    } else if index < 180 {
                        0
                    } else {
                        2000
                    },
                )
                .unwrap();
        }
        // Drain the complete waveform before injection, bounded below the
        // existing owner's 250ms media stall deadline (never relax that bound).
        let drained = Instant::now() + Duration::from_millis(150);
        let mut status_observed_at_ms;
        loop {
            process.console.send(b"status\n");
            let status = process.event("status");
            status_observed_at_ms = now_ms();
            process.status_prompt();
            assert_eq!(status["state"], "starting");
            assert_eq!(status["media_security"], "pinned_tls13_mono_v1");
            assert_eq!(status["packets_sent"], 0);
            packets = status["packets_written"].as_u64().unwrap();
            bytes = status["bytes_written"].as_u64().unwrap();
            if packets == samples.sequence {
                break;
            }
            assert!(packets < samples.sequence && Instant::now() < drained);
            thread::sleep(TICK);
        }
        assert!(packets >= 300 && bytes == packets * 960);
        assert!(now_ms() < original_expiry);
        if scenario == "local_eof" {
            // Ctrl-D is a real private-terminal cancellation. The controller
            // unwinds through best-effort Stop then input EOF, possibly forced
            // reap. It does NOT relay an owner terminal event in this path.
            let input_eof_at_ms = now_ms();
            assert!(input_eof_at_ms < original_expiry);
            process.console.send(&[4]);
            assert!(!process.finish(), "Ctrl-D must fail the active controller");
            assert!(!Path::new(&format!("/proc/{}", helper.unwrap())).exists());
            let failure =
                match link.receive_control_message_until(Instant::now() + Duration::from_secs(1)) {
                    Err(failure) => failure,
                    Ok(_) => panic!("actual authenticated control EOF required"),
                };
            let capyio_control_link::CatalogLinkError::Transport(
                capyio_control_tls::ControlTlsError::Io(error),
            ) = failure
            else {
                panic!(
                    "actual authenticated control EOF required, not an unrelated protocol failure"
                );
            };
            assert!(matches!(
                error.kind(),
                std::io::ErrorKind::UnexpectedEof
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
            ));
            // This proves authenticated control closure only. The public
            // sender API erases transport types, so do not infer media EOF or
            // manufacture an owner terminal event during controller unwind.
            let source_control_closed_observed_at_ms = now_ms();
            assert!(source_control_closed_observed_at_ms < original_expiry);
            sender.stop();
            credentials.fill(0);
            link.close_and_reconcile(&mut runtime).unwrap();
            assert_closed(&runtime, session);
            process.publish(&root, &scenario, "stopped", json!({
                "helper_pid":helper,"source_lease_expires_at_ms":expiry,
                "terminal_observed_at_ms":null,"terminal_cause":null,
                "counter_observation":"pre_input_eof_status",
                "status_packets_written":packets,"status_bytes_written":bytes,"status_observed_at_ms":status_observed_at_ms,
                "input_eof_at_ms":input_eof_at_ms,"source_control_closed_observed_at_ms":source_control_closed_observed_at_ms,
                "source_control_closed":true,
                "source_records_sent":samples.sequence,"source_records_attempted":samples.attempted,
                "injected":scenario,"verified":scenario,"session_closed":true,"leases_released":true,"helper_reaped":true,
                "console_reaped":true,"node_reaped":true,"protected_read":true,"secret_redacted":true}));
            wait_marker(&root, "stop_checked");
            eprintln!(
                "capyio_secure_audio_console=passed scenario={scenario} actual_tty=true actual_node=true fresh_consent=true protected_read=true secret_redacted=true node_reaped=true{}",
                if endpoint_mode == "virtual_microphone" {
                    " endpoint_mode=virtual_microphone"
                } else {
                    ""
                }
            );
            return;
        }
        match scenario.as_str() {
            "stop" => process.console.send(b"stop\n"),
            "source_revoke" => {
                runtime
                    .release_route_port_lease(route, source, session)
                    .unwrap();
                assert_eq!(
                    samples.send(&mut sender, &runtime, 0).unwrap_err(),
                    "audio Source authority revoked"
                );
            }
            "control_eof" => link.close(),
            "media_eof" => sender.stop(),
            "expiry" => loop {
                if samples.send(&mut sender, &runtime, 0).is_err() {
                    assert!(now_ms() >= original_expiry);
                    break;
                }
            },
            _ => unreachable!(),
        }
        sent = samples.sequence;
        attempted = samples.attempted;
        let terminal = process.event(if scenario == "stop" {
            "stopped"
        } else {
            "failed"
        });
        terminal_cause = terminal["terminal_cause"]
            .as_str()
            .expect("typed actual owner terminal cause")
            .to_owned();
        // The owner may cross the exact expiry boundary between its authority
        // check and observing queued TLS EOF/timeout. Those causes are valid
        // only at/after the original lease, never in the live-lease EOF cases.
        if scenario == "expiry" {
            assert!(matches!(
                terminal_cause.as_str(),
                "failure" | "media_timeout" | "media_closed"
            ));
        } else {
            assert_eq!(
                terminal_cause,
                match scenario.as_str() {
                    "stop" => "local_stop",
                    "source_revoke" | "media_eof" => "media_closed",
                    "control_eof" => "control_closed",
                    _ => unreachable!(),
                },
                "EOF must never be substituted by a media timeout or unrelated failure"
            );
        }
        observed = terminal["terminal_observed_at_ms"]
            .as_u64()
            .expect("owner timestamp before reaping required");
        if scenario == "expiry" {
            assert!(observed >= original_expiry);
        } else {
            assert!(observed < original_expiry);
        }
        // Terminal totals, not stale status snapshots or invented delivery counts.
        packets = terminal["packets_written"].as_u64().unwrap();
        bytes = terminal["bytes_written"].as_u64().unwrap();
        assert_eq!(terminal["packets_dropped_busy"], 0);
        assert_eq!(process.finish(), scenario == "stop");
        sender.stop();
        assert!(!Path::new(&format!("/proc/{}", helper.unwrap())).exists());
    }
    credentials.fill(0);
    link.close_and_reconcile(&mut runtime).unwrap();
    assert_closed(&runtime, session);
    process.publish(&root,&scenario,"stopped",json!({"helper_pid":helper,"source_lease_expires_at_ms":expiry,"terminal_observed_at_ms":observed,
        "bytes_written":bytes,"packets_written":packets,"packets_dropped_busy":0,"source_records_sent":sent,"source_records_attempted":attempted,
        "injected":scenario,"verified":scenario,"terminal_cause":terminal_cause,"session_closed":true,"leases_released":true,"helper_reaped":true,
        "console_reaped":true,"node_reaped":true,"protected_read":true,"secret_redacted":true}));
    wait_marker(&root, "stop_checked");
    eprintln!(
        "capyio_secure_audio_console=passed scenario={scenario} actual_tty=true actual_node=true fresh_consent=true protected_read=true secret_redacted=true node_reaped=true{}",
        if endpoint_mode == "virtual_microphone" {
            " endpoint_mode=virtual_microphone"
        } else {
            ""
        }
    );
}
