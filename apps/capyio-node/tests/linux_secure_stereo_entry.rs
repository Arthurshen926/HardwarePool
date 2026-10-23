#![cfg(all(target_os = "linux", feature = "paired-store"))]
//! Actual Node FIFO + private synthetic Secret Service + pinned TLS handshake.
//! No PipeWire helper/media is opened: stop before receiver media admission.
#[allow(dead_code)]
#[path = "../../../platform/linux/capyio-audio/tests/support/bilateral_fixture.rs"]
mod credentials;
#[path = "support/paired_store.rs"]
mod paired_store;
use capyio_android_jni::{
    AndroidCatalogRuntimeV1,
    secure_stereo_receiver::SecureStereoSinkAdmission,
    stereo_supervisor::{StereoSinkSupervisor, StereoSupervisorEvent},
};
use capyio_control_credentials::ControlCredentialBundle;
use capyio_control_tls::{
    AuthenticatedControlClientChannel, ControlTlsBinding, ControlTlsEndpoint, ControlTlsNodeBinding,
};
use capyio_core::*;
use capyio_pairing::{CredentialHandle, PairedPeerDirectory, PairedPeerRecord};
use capyio_protocol::SECURE_AUDIO_STEREO_FORMAT;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io::{self, Read, Write},
    os::fd::AsFd,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const BOUND: Duration = Duration::from_secs(5);
fn nonblocking(fd: impl AsFd) {
    let flags = rustix::fs::fcntl_getfl(&fd).unwrap();
    rustix::fs::fcntl_setfl(fd, flags | rustix::fs::OFlags::NONBLOCK).unwrap();
}
fn drain(reader: &mut impl Read, bytes: &mut Vec<u8>) {
    let mut buffer = [0; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => panic!("output: {e}"),
        }
        assert!(bytes.len() <= 1_048_576);
    }
}
struct Process {
    child: Child,
    pairing: paired_store::PersistedPairing,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}
impl Process {
    fn new(pairing: paired_store::PersistedPairing) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_capyio-node"))
            .args(pairing.args("linux-capture"))
            .env_remove("RUST_LOG")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        nonblocking(child.stdin.as_ref().unwrap());
        nonblocking(child.stdout.as_ref().unwrap());
        nonblocking(child.stderr.as_ref().unwrap());
        Self {
            child,
            pairing,
            stdout: vec![],
            stderr: vec![],
        }
    }
    fn command(&mut self, value: Value) {
        let body = serde_json::to_vec(&value).unwrap();
        let mut frame = Vec::from(u32::try_from(body.len()).unwrap().to_be_bytes());
        frame.extend(body);
        let until = Instant::now() + BOUND;
        let mut n = 0;
        while n < frame.len() {
            assert!(Instant::now() < until);
            match self.child.stdin.as_mut().unwrap().write(&frame[n..]) {
                Ok(0) => panic!("input closed"),
                Ok(count) => n += count,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    thread::sleep(Duration::from_millis(2))
                }
                Err(e) => panic!("input: {e}"),
            }
        }
    }
    fn collect(&mut self) {
        drain(self.child.stdout.as_mut().unwrap(), &mut self.stdout);
        drain(self.child.stderr.as_mut().unwrap(), &mut self.stderr);
        self.pairing.assert_no_leak(&self.stdout);
        self.pairing.assert_no_leak(&self.stderr);
    }
    fn event(&mut self, expected: &str) -> Value {
        let until = Instant::now() + BOUND;
        loop {
            self.collect();
            if self.stdout.len() >= 4 {
                let n = u32::from_be_bytes(self.stdout[..4].try_into().unwrap()) as usize;
                assert!((1..=4096).contains(&n));
                if self.stdout.len() >= n + 4 {
                    let value: Value = serde_json::from_slice(&self.stdout[4..n + 4]).unwrap();
                    self.stdout.drain(..n + 4);
                    assert_eq!(value["version"], 1);
                    assert_eq!(value["event"], expected, "{value}");
                    return value;
                }
            }
            assert!(
                Instant::now() < until,
                "missing {expected}; stderr={}",
                String::from_utf8_lossy(&self.stderr)
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn no_helper(&self) {
        let children = std::fs::read_to_string(format!(
            "/proc/{}/task/{}/children",
            self.child.id(),
            self.child.id()
        ))
        .unwrap();
        assert!(
            children.trim().is_empty(),
            "no capture child before media binding"
        );
    }
    fn finish(&mut self) -> bool {
        let until = Instant::now() + BOUND;
        loop {
            self.collect();
            if let Some(status) = self.child.try_wait().unwrap() {
                self.collect();
                let state = self.pairing.service.state.lock().unwrap();
                assert!(state.reads > 0);
                assert_eq!(state.forbidden, 0);
                return status.success();
            }
            assert!(Instant::now() < until, "bounded process exit");
            thread::sleep(Duration::from_millis(2));
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn pending(seconds: u64) -> (Process, StereoSinkSupervisor, Value, ControlTlsBinding) {
    let mut catalog: Value = serde_json::from_str(include_str!(
        "../../../platform/android/fixtures/android-core-catalog-v1.json"
    ))
    .unwrap();
    let cap = catalog["capabilities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["class"] == "speaker")
        .unwrap();
    cap["speaker_selection"] = json!({"profile":"secure_stereo_tls13_v1","revision":1});
    cap["ports"][0]["formats"] = json!([SECURE_AUDIO_STEREO_FORMAT]);
    let mut host = AndroidCatalogRuntimeV1::new(&catalog.to_string()).unwrap();
    let android = host.snapshot().local_node;
    let speaker = android
        .capabilities
        .values()
        .find(|c| c.class == CapabilityClass::Speaker)
        .unwrap();
    let sink = PortRef {
        node_id: android.id,
        capability_id: speaker.id,
        port_id: *speaker.ports.keys().next().unwrap(),
    };
    let local = NodeId::new();
    let (_, android_client, linux_server, _) = credentials::credentials();
    let bundle = ControlCredentialBundle::new(
        linux_server.certificate_chain_der.clone(),
        linux_server.private_key_der.clone(),
        linux_server.client_ca_der.clone(),
    )
    .unwrap();
    let mut directory = PairedPeerDirectory::new(local).unwrap();
    directory
        .add(
            PairedPeerRecord::new(
                android.id,
                "synthetic stereo",
                "localhost",
                CredentialHandle::new("synthetic.stereo.only").unwrap(),
                linux_server.expected_peer_certificate,
                1,
            )
            .unwrap(),
        )
        .unwrap();
    let pairing =
        paired_store::PersistedPairing::new(local, &directory.encode_json().unwrap(), &bundle);
    let mut process = Process::new(pairing);
    process.command(json!({"version":1,"node_id":local,"sink":sink,"listen":"127.0.0.1:0","media":"127.0.0.1:0","target":"synthetic.never.opened","duration_secs":seconds,"media_security":"pinned_tls13_stereo_v1"}));
    process.command(json!({"version":1,"credential_source":"paired-store"}));
    let listening = process.event("listening");
    let client = AuthenticatedControlClientChannel::connect_fresh_session(
        ControlTlsEndpoint {
            address: listening["address"].as_str().unwrap().parse().unwrap(),
            io_timeout: BOUND,
        },
        "localhost",
        android_client,
        ControlTlsNodeBinding::new(local, android.id).unwrap(),
    )
    .unwrap();
    let link = host
        .admit_dialed_catalog_exchange(client, "synthetic linux".into(), &BTreeSet::new())
        .unwrap();
    let binding = link.tls_binding();
    let mut supervisor = StereoSinkSupervisor::new(host, link, 1).unwrap();
    let approval = process.event("approval_required");
    let id: RouteId = serde_json::from_value(approval["route_id"].clone()).unwrap();
    let until = Instant::now() + BOUND;
    loop {
        if let Some(event) = supervisor.poll().unwrap() {
            assert_eq!(
                event,
                StereoSupervisorEvent::PendingApproval { route_id: id }
            );
            break;
        }
        assert!(Instant::now() < until);
    }
    process.no_helper();
    assert!(
        supervisor
            .host_handle()
            .lock()
            .unwrap()
            .snapshot()
            .port_leases
            .is_empty()
    );
    assert_eq!(approval["format"], SECURE_AUDIO_STEREO_FORMAT);
    (process, supervisor, approval, binding)
}
fn approval(value: &Value) -> Value {
    json!({"version":1,"command":"approve","request_id":value["request_id"],"route_id":value["route_id"]})
}
fn admission(binding: ControlTlsBinding, route: RouteId) -> SecureStereoSinkAdmission {
    SecureStereoSinkAdmission {
        expected: binding,
        route_id: route,
        stream_id: StreamId::new(),
        local_address: "127.0.0.1".parse().unwrap(),
        lifetime: Duration::from_secs(10),
    }
}
fn peer_approved(process: &mut Process, supervisor: &mut StereoSinkSupervisor, prompt: &Value) {
    process.command(approval(prompt));
    process.event("awaiting_peer_approval");
    let until = Instant::now() + BOUND;
    loop {
        if let Some(event) = supervisor.poll().unwrap() {
            assert_eq!(
                event,
                StereoSupervisorEvent::PeerApproved {
                    route_id: supervisor.route_id().unwrap()
                }
            );
            break;
        }
        assert!(Instant::now() < until);
    }
    process.no_helper();
    assert!(
        supervisor
            .host_handle()
            .lock()
            .unwrap()
            .snapshot()
            .port_leases
            .is_empty()
    );
}
#[test]
fn node_stereo_requires_fresh_bilateral_approval_then_crosses_claims_and_start() {
    let (mut process, mut supervisor, prompt, binding) = pending(10);
    let route = supervisor.route_id().unwrap();
    assert!(
        supervisor
            .approve(route, true, admission(binding, route))
            .is_err(),
        "early Android click not queued"
    );
    peer_approved(&mut process, &mut supervisor, &prompt);
    assert_eq!(
        supervisor.host_handle().lock().unwrap().snapshot().routes[0].state,
        RouteState::Draft
    );
    supervisor
        .approve(route, true, admission(binding, route))
        .unwrap();
    process.event("awaiting_peer_offer");
    let until = Instant::now() + BOUND;
    loop {
        if let Some(StereoSupervisorEvent::Starting { route_id }) = supervisor.poll().unwrap() {
            assert_eq!(route_id, route);
            break;
        }
        assert!(Instant::now() < until);
    }
    process.no_helper();
    let snapshot = supervisor.host_handle().lock().unwrap().snapshot();
    assert_eq!(snapshot.routes[0].state, RouteState::Starting);
    assert_eq!(snapshot.routes[0].epoch, 1);
    assert!(!snapshot.port_leases.is_empty());
    process.command(json!({"version":1,"command":"stop"}));
    process.event("stopped");
    assert!(process.finish());
    supervisor.stop().unwrap();
    assert!(
        supervisor
            .host_handle()
            .lock()
            .unwrap()
            .snapshot()
            .port_leases
            .is_empty()
    );
}
#[test]
fn node_stereo_wrong_request_or_route_never_grants() {
    for field in ["request_id", "route_id"] {
        let (mut process, mut supervisor, prompt, _) = pending(10);
        let mut command = approval(&prompt);
        command[field] = json!(MessageId::new());
        process.command(command);
        process.event("failed");
        assert!(!process.finish());
        assert!(
            supervisor
                .host_handle()
                .lock()
                .unwrap()
                .snapshot()
                .port_leases
                .is_empty()
        );
        assert!(supervisor.poll().is_err());
    }
}
#[test]
fn node_stereo_stop_without_approval_and_terminal_eof_never_capture() {
    for eof in [false, true] {
        let (mut process, mut supervisor, _, _) = pending(10);
        if eof {
            drop(process.child.stdin.take());
        } else {
            process.command(json!({"version":1,"command":"stop"}));
            process.event("stopped");
        }
        assert_eq!(process.finish(), !eof);
        assert!(
            supervisor
                .host_handle()
                .lock()
                .unwrap()
                .snapshot()
                .port_leases
                .is_empty()
        );
        assert!(supervisor.poll().is_err());
    }
}
#[test]
fn node_stereo_android_denial_and_original_short_expiry_retire_without_media() {
    for deny in [false, true] {
        let (mut process, mut supervisor, prompt, binding) = pending(if deny { 10 } else { 1 });
        peer_approved(&mut process, &mut supervisor, &prompt);
        if deny {
            let route = supervisor.route_id().unwrap();
            assert!(
                supervisor
                    .approve(route, false, admission(binding, route))
                    .is_err()
            );
        }
        process.event("failed");
        assert!(!process.finish());
        assert!(
            supervisor
                .host_handle()
                .lock()
                .unwrap()
                .snapshot()
                .port_leases
                .is_empty()
        );
    }
}

#[test]
fn node_stereo_rejects_unpaired_and_invalid_configuration_before_credentials_or_network() {
    let base = json!({"version":1,"node_id":NodeId::new(),"sink":PortRef{node_id:NodeId::new(),capability_id:CapabilityId::new(),port_id:PortId::new()},"listen":"127.0.0.1:0","media":"127.0.0.1:0","target":"synthetic.never.opened","duration_secs":10,"media_security":"pinned_tls13_stereo_v1"});
    let mut cases = vec![base.clone()]; // Valid stereo still rejects private credential mode.
    for (key, value) in [
        ("duration_secs", json!(0)),
        ("duration_secs", json!(61)),
        ("endpoint_mode", json!("virtual_speaker")),
        ("target", json!("auto")),
        ("media", json!("127.0.0.1:9001")),
    ] {
        let mut invalid = base.clone();
        invalid[key] = value;
        cases.push(invalid);
    }
    for config in cases {
        let mut child = Command::new(env!("CARGO_BIN_EXE_capyio-node"))
            .args(["linux-capture", "--trusted-lab"])
            .env_remove("RUST_LOG")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let body = serde_json::to_vec(&config).unwrap();
        let input = child.stdin.as_mut().unwrap();
        input
            .write_all(&u32::try_from(body.len()).unwrap().to_be_bytes())
            .unwrap();
        input.write_all(&body).unwrap();
        // Keep input open: rejection must precede any credential read or EOF.
        let until = Instant::now() + BOUND;
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(!status.success());
                break;
            }
            if Instant::now() >= until {
                let _ = child.kill();
                let _ = child.wait();
                panic!("configuration validation waited for credentials");
            }
            thread::sleep(Duration::from_millis(2));
        }
        let mut output = vec![];
        child
            .stdout
            .take()
            .unwrap()
            .read_to_end(&mut output)
            .unwrap();
        assert!(output.len() >= 4);
        let size = u32::from_be_bytes(output[..4].try_into().unwrap()) as usize;
        assert_eq!(size + 4, output.len());
        let value: Value = serde_json::from_slice(&output[4..]).unwrap();
        assert_eq!(value, json!({"version":1,"event":"failed"}));
    }
}
