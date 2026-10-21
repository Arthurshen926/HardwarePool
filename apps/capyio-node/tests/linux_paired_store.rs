#![cfg(all(target_os = "linux", feature = "paired-store"))]
#![forbid(unsafe_code)]
//! Actual finite node executable using persisted synthetic pairing metadata and
//! an owned private D-Bus service. Never opens a real keyring or media/input device.
use capyio_control_credentials::ControlCredentialBundle;
use capyio_core::{CapabilityId, NodeId, PortId};
use capyio_linux_pairing_store::LinuxPairingStore;
use capyio_pairing::{
    CertificateFingerprint, CredentialHandle, PairedPeerDirectory, PairedPeerRecord,
    PairingInvitation,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    os::fd::AsFd,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};
#[path = "support/paired_store.rs"]
mod paired_store;
use paired_store::{PersistedPairing, SeedBackend};
const BOUND: Duration = Duration::from_secs(8);
fn credentials() -> (Vec<u8>, Vec<u8>, CertificateFingerprint) {
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
        KeyUsagePurpose,
    };
    let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];
    let key = KeyPair::generate().unwrap();
    let ca = params.self_signed(&key).unwrap().der().to_vec();
    let issuer = Issuer::new(params, key);
    let mut params = CertificateParams::new(vec!["localhost".into()]).unwrap();
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let key = KeyPair::generate().unwrap();
    let cert = params.signed_by(&key, &issuer).unwrap().der().to_vec();
    let key = key.serialize_der();
    let pin = CertificateFingerprint::from_certificate_der(&cert).unwrap();
    (
        ControlCredentialBundle::new(vec![cert, ca.clone()], key.clone(), ca)
            .unwrap()
            .encode()
            .unwrap(),
        key,
        pin,
    )
}
struct Fixture(PersistedPairing);
impl std::ops::Deref for Fixture {
    type Target = PersistedPairing;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Fixture {
    fn new() -> Self {
        let owner = NodeId::new();
        let peer = NodeId::new();
        let (bundle, _key, pin) = credentials();
        let mut directory = PairedPeerDirectory::new(owner).unwrap();
        directory
            .add(
                PairedPeerRecord::new(
                    peer,
                    "Synthetic peer",
                    "localhost",
                    CredentialHandle::new("synthetic.fixture.only").unwrap(),
                    pin,
                    1,
                )
                .unwrap(),
            )
            .unwrap();
        Self(PersistedPairing::new(
            owner,
            &directory.encode_json().unwrap(),
            &ControlCredentialBundle::decode(&bundle).unwrap(),
        ))
    }
    fn config(&self) -> Value {
        json!({"version":1,"node_id":self.owner,"source":{"node_id":self.peer,"capability_id":CapabilityId::new(),"port_id":PortId::new()},"listen":"127.0.0.1:0","duration_secs":3})
    }
    fn audio_config(&self, command: &str) -> Value {
        let mut config = self.config();
        config["media"] = json!("127.0.0.1:40211");
        config["target"] = json!("synthetic.test");
        if command == "linux-capture" {
            let source = config.as_object_mut().unwrap().remove("source").unwrap();
            config["sink"] = source;
        } else {
            assert_eq!(command, "linux-playback");
        }
        config
    }
    fn no_leak(&self, process: &Process) {
        self.assert_no_leak(&process.out);
        self.assert_no_leak(&process.err);
    }
}
fn nonblocking(fd: impl AsFd) {
    let flags = rustix::fs::fcntl_getfl(&fd).unwrap();
    rustix::fs::fcntl_setfl(fd, flags | rustix::fs::OFlags::NONBLOCK).unwrap();
}
fn drain(input: &mut impl Read, output: &mut Vec<u8>) {
    let mut bytes = [0; 4096];
    loop {
        match input.read(&mut bytes) {
            Ok(0) => break,
            Ok(n) => output.extend_from_slice(&bytes[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => panic!("fixture read: {e}"),
        }
        assert!(output.len() < 1_048_576);
    }
}
struct Process {
    child: Child,
    out: Vec<u8>,
    err: Vec<u8>,
    cursor: usize,
}
impl Process {
    fn start(args: &[String]) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_capyio-node"))
            .args(args)
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .env_remove("DBUS_SYSTEM_BUS_ADDRESS")
            .env_remove("RUST_LOG")
            .env("RUST_BACKTRACE", "0")
            .env("RUST_LIB_BACKTRACE", "0")
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
            out: vec![],
            err: vec![],
            cursor: 0,
        }
    }
    fn frame(&mut self, value: &[u8]) {
        self.frames(&[value]);
    }
    fn frames(&mut self, values: &[&[u8]]) {
        let mut bytes = Vec::new();
        for value in values {
            bytes.extend_from_slice(&(value.len() as u32).to_be_bytes());
            bytes.extend_from_slice(value);
        }
        // Linux FIFO writes <= PIPE_BUF are atomic: early config rejection cannot
        // race a separate marker write and cause an irrelevant fixture EPIPE.
        assert!(bytes.len() <= 4096);
        let mut offset = 0;
        let deadline = Instant::now() + BOUND;
        while offset < bytes.len() {
            assert!(Instant::now() < deadline);
            match self.child.stdin.as_mut().unwrap().write(&bytes[offset..]) {
                Ok(0) => panic!("closed child input"),
                Ok(n) => offset += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    thread::sleep(Duration::from_millis(2))
                }
                Err(e) => panic!("fixture write: {e}"),
            }
        }
        bytes.fill(0);
    }
    fn command(&mut self, value: Value) {
        self.frame(&serde_json::to_vec(&value).unwrap());
    }
    fn collect(&mut self) {
        drain(self.child.stdout.as_mut().unwrap(), &mut self.out);
        drain(self.child.stderr.as_mut().unwrap(), &mut self.err);
    }
    fn event(&mut self, expected: &str) {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
            if self.out.len() >= self.cursor + 4 {
                let n =
                    u32::from_be_bytes(self.out[self.cursor..self.cursor + 4].try_into().unwrap())
                        as usize;
                assert!((1..=4096).contains(&n));
                if self.out.len() >= self.cursor + 4 + n {
                    let value: Value =
                        serde_json::from_slice(&self.out[self.cursor + 4..self.cursor + 4 + n])
                            .unwrap();
                    self.cursor += 4 + n;
                    assert_eq!(value["event"], expected);
                    return;
                }
            }
            assert!(
                Instant::now() < deadline,
                "expected bounded event {expected}; stderr={}",
                String::from_utf8_lossy(&self.err)
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn finish(&mut self) -> ExitStatus {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
            if let Some(status) = self.child.try_wait().unwrap() {
                self.collect();
                return status;
            }
            assert!(Instant::now() < deadline, "bounded executable exit");
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn no_helper(&self) {
        for entry in fs::read_dir("/proc").unwrap().flatten() {
            let Ok(status) = fs::read_to_string(entry.path().join("status")) else {
                continue;
            };
            let ppid = status
                .lines()
                .find_map(|s| s.strip_prefix("PPid:"))
                .and_then(|s| s.trim().parse::<u32>().ok());
            assert_ne!(
                ppid,
                Some(self.child.id()),
                "no helper before fresh per-launch approval"
            );
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn marker() -> Value {
    json!({"version":1,"credential_source":"paired-store"})
}
#[test]
fn ipc_paired_store_camera_and_touchpad_listen_without_grant_and_stop_cleanly() {
    for command in ["linux-camera", "linux-touchpad"] {
        let fixture = Fixture::new();
        let mut process = Process::start(&fixture.args(command));
        process.command(fixture.config());
        process.command(marker());
        process.event("listening");
        process.no_helper();
        process.command(json!({"version":1,"command":"stop"}));
        process.event("stopped");
        assert!(process.finish().success());
        fixture.no_leak(&process);
        assert!(fixture.service.state.lock().unwrap().reads > 0);
    }
}
#[test]
fn ipc_paired_store_invalid_peer_or_backend_fails_before_listening_or_helper() {
    for command in ["linux-camera", "linux-touchpad"] {
        for failure in [
            "revoked",
            "missing",
            "locked",
            "digest",
            "config-node",
            "config-peer",
            "metadata-owner",
            "missing-peer",
            "duration-zero",
            "duration-over-max",
            "pending-journal",
            "corrupt-journal",
        ] {
            let fixture = Fixture::new();
            let mut args = fixture.args(command);
            let mut config = fixture.config();
            match failure {
                "revoked" => fixture.revoke(),
                "pending-journal" => {
                    let invitation = PairingInvitation::new(
                        fixture.owner,
                        fixture.directory.clone(),
                        fixture.bundle.clone(),
                    )
                    .unwrap()
                    .encode()
                    .unwrap();
                    let mut store = LinuxPairingStore::open(
                        &fixture.state_path,
                        fixture.owner,
                        SeedBackend(fixture.bundle.clone()),
                    )
                    .unwrap();
                    store.stage_enrollment(&invitation).unwrap();
                }
                "corrupt-journal" => {
                    use std::os::unix::fs::OpenOptionsExt;
                    fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .mode(0o600)
                        .open(fixture.state_path.join("pending-v1.json"))
                        .unwrap()
                        .write_all(b"{}")
                        .unwrap();
                }
                "missing" => fixture.service.state.lock().unwrap().missing = true,
                "locked" => fixture.service.state.lock().unwrap().locked = true,
                "digest" => fixture.service.state.lock().unwrap().bytes[0] ^= 1,
                "duration-zero" => config["duration_secs"] = json!(0),
                "duration-over-max" => config["duration_secs"] = json!(61),
                "config-node" => config["node_id"] = json!(NodeId::new()),
                "config-peer" => config["source"]["node_id"] = json!(NodeId::new()),
                "metadata-owner" => {
                    let owner = NodeId::new();
                    args[5] = owner.to_string();
                    config["node_id"] = json!(owner);
                }
                "missing-peer" => {
                    let peer = NodeId::new();
                    args[7] = peer.to_string();
                    config["source"]["node_id"] = json!(peer);
                }
                _ => unreachable!(),
            }
            let mut process = Process::start(&args);
            process.frames(&[
                &serde_json::to_vec(&config).unwrap(),
                &serde_json::to_vec(&marker()).unwrap(),
            ]);
            process.event("failed");
            assert!(!process.finish().success(), "{command}: {failure}");
            assert!(!String::from_utf8_lossy(&process.out).contains("listening"));
            fixture.no_leak(&process);
            if matches!(
                failure,
                "config-node" | "config-peer" | "duration-zero" | "duration-over-max"
            ) {
                assert_eq!(
                    fixture
                        .service
                        .state
                        .lock()
                        .unwrap()
                        .collection_property_reads,
                    0,
                    "{failure} must fail before keyring preflight"
                );
            }
            if failure != "digest" {
                assert_eq!(
                    fixture.service.state.lock().unwrap().reads,
                    0,
                    "{failure} must fail before protected reads"
                );
            }
        }
    }
}
#[test]
fn ipc_paired_store_rejects_legacy_credential_frames_in_selected_mode() {
    for command in ["linux-camera", "linux-touchpad"] {
        let fixture = Fixture::new();
        let mut process = Process::start(&fixture.args(command));
        process.frames(&[
            &serde_json::to_vec(&fixture.config()).unwrap(),
            &fixture.directory,
        ]);
        process.event("failed");
        assert!(!process.finish().success());
        fixture.no_leak(&process);
        let state = fixture.service.state.lock().unwrap();
        assert_eq!(state.sessions, 0);
        assert_eq!(state.collection_property_reads, 0);
    }
}
#[test]
fn paired_store_cli_rejects_incomplete_selector_set_without_access() {
    for command in [
        "linux-camera",
        "linux-touchpad",
        "linux-playback",
        "linux-capture",
    ] {
        let mut process = Process::start(&[
            command.into(),
            "--trusted-lab".into(),
            "--pairing-node".into(),
            NodeId::new().to_string(),
        ]);
        assert!(!process.finish().success());
        assert!(!String::from_utf8_lossy(&process.out).contains("listening"));
    }
}

// These cases deliberately stop before peer admission: persisted credentials
// permit a listener, never media, a PipeWire endpoint, or fresh Route consent.
#[test]
fn ipc_paired_store_audio_loads_credentials_without_helpers_and_stops() {
    for (command, virtual_endpoint, incoming_draft) in [
        ("linux-playback", false, false),
        ("linux-playback", false, true),
        ("linux-playback", true, true),
        ("linux-capture", false, false),
        ("linux-capture", true, false),
    ] {
        let fixture = Fixture::new();
        let mut config = fixture.audio_config(command);
        if virtual_endpoint {
            config.as_object_mut().unwrap().remove("target");
            config["endpoint_mode"] = json!(if command == "linux-playback" {
                "virtual_microphone"
            } else {
                "virtual_speaker"
            });
        }
        if incoming_draft {
            config["proposal_mode"] = json!("incoming_draft");
        }
        let mut process = Process::start(&fixture.args(command));
        process.frames(&[
            &serde_json::to_vec(&config).unwrap(),
            &serde_json::to_vec(&marker()).unwrap(),
        ]);
        process.event("listening");
        process.no_helper();
        process.command(json!({"version":1,"command":"stop"}));
        process.event("stopped");
        assert!(process.finish().success());
        fixture.no_leak(&process);
        let state = fixture.service.state.lock().unwrap();
        assert!(
            state.reads > 0,
            "stored credentials must actually be loaded"
        );
        assert_eq!(state.forbidden, 0);
        assert!(!String::from_utf8_lossy(&process.out).contains("approval_required"));
    }
}

#[test]
fn ipc_paired_store_audio_binding_and_backend_fail_before_listener() {
    for command in ["linux-playback", "linux-capture"] {
        for failure in [
            "config-node",
            "config-peer",
            "revoked",
            "locked",
            "missing",
            "digest",
        ] {
            let fixture = Fixture::new();
            let mut config = fixture.audio_config(command);
            match failure {
                "config-node" => config["node_id"] = json!(NodeId::new()),
                "config-peer" => {
                    let remote = if command == "linux-playback" {
                        "source"
                    } else {
                        "sink"
                    };
                    config[remote]["node_id"] = json!(NodeId::new());
                }
                "revoked" => fixture.revoke(),
                "locked" => fixture.service.state.lock().unwrap().locked = true,
                "missing" => fixture.service.state.lock().unwrap().missing = true,
                "digest" => fixture.service.state.lock().unwrap().bytes[0] ^= 1,
                _ => unreachable!(),
            }
            let mut process = Process::start(&fixture.args(command));
            process.frames(&[
                &serde_json::to_vec(&config).unwrap(),
                &serde_json::to_vec(&marker()).unwrap(),
            ]);
            process.event("failed");
            assert!(!process.finish().success(), "{command}: {failure}");
            assert!(!String::from_utf8_lossy(&process.out).contains("listening"));
            fixture.no_leak(&process);
            let state = fixture.service.state.lock().unwrap();
            assert_eq!(state.forbidden, 0);
            if matches!(failure, "config-node" | "config-peer") {
                assert_eq!(
                    state.collection_property_reads, 0,
                    "binding before keyring preflight"
                );
                assert_eq!(state.sessions, 0);
            }
            if failure == "digest" {
                assert!(
                    state.reads > 0,
                    "changed stored bytes must actually be checked"
                );
            } else {
                assert_eq!(
                    state.reads, 0,
                    "{failure} before protected credential reads"
                );
            }
        }
    }
}

#[test]
fn ipc_paired_store_audio_rejects_mixed_bootstrap_without_keyring_access() {
    for command in ["linux-playback", "linux-capture"] {
        for selected_store in [false, true] {
            let fixture = Fixture::new();
            let args = if selected_store {
                fixture.args(command)
            } else {
                vec![command.into(), "--trusted-lab".into()]
            };
            let mut process = Process::start(&args);
            let store_marker = serde_json::to_vec(&marker()).unwrap();
            process.frames(&[
                &serde_json::to_vec(&fixture.audio_config(command)).unwrap(),
                if selected_store {
                    &fixture.directory
                } else {
                    &store_marker
                },
                &fixture.bundle,
            ]);
            // Supply a complete credential frame and keep stdin open: this must
            // reject the mixed mode, not merely fail on missing input or EOF.
            process.event("failed");
            assert!(!process.finish().success());
            assert!(!String::from_utf8_lossy(&process.out).contains("listening"));
            fixture.no_leak(&process);
            let state = fixture.service.state.lock().unwrap();
            assert_eq!(state.sessions, 0);
            assert_eq!(state.collection_property_reads, 0);
            assert_eq!(state.reads, 0);
            assert_eq!(state.forbidden, 0);
        }
    }
}
