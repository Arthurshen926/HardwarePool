#![cfg(all(target_os = "linux", feature = "paired-store"))]
#![forbid(unsafe_code)]
//! Mandatory actual-executable first-enrollment tests. All identities are newly
//! generated, all networking is loopback/private D-Bus, and no hardware is used.
use capyio_control_credentials::ControlCredentialBundle;
use capyio_core::NodeId;
use capyio_enrollment::EnrollmentManager;
use capyio_linux_pairing_store::{LinuxPairingStore, SecretServiceBackend};
use capyio_pairing::{CertificateFingerprint, PairedPeerDirectory, PairingInvitation};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::{self, Read, Write},
    net::TcpListener,
    os::{
        fd::AsFd,
        unix::fs::{DirBuilderExt, OpenOptionsExt},
    },
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
#[path = "support/enrollment_store.rs"]
mod enrollment_store;
use enrollment_store::{COLLECTION, KeyringState, PrivateService};
const BOUND: Duration = Duration::from_secs(12);
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
            .env_clear()
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
    fn event(&mut self, expected: &str) -> Value {
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
                    return value;
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
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
struct Fixture {
    service: PrivateService,
    owner: NodeId,
    state_path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let service = PrivateService::start(Arc::new(Mutex::new(KeyringState {
            missing: true,
            ..Default::default()
        })));
        let state_path = service.root.join("state");
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&state_path)
            .unwrap();
        Self {
            service,
            owner: NodeId::new(),
            state_path,
        }
    }
    fn args(&self) -> Vec<String> {
        vec![
            "linux-pairing".into(),
            "--trusted-lab".into(),
            "--pairing-state".into(),
            self.state_path.display().to_string(),
            "--pairing-node".into(),
            self.owner.to_string(),
            "--pairing-bus-address".into(),
            self.service.address.clone(),
            "--pairing-collection".into(),
            COLLECTION.into(),
        ]
    }
    fn start(&self) -> Process {
        Process::start(&self.args())
    }
    fn store(&self) -> LinuxPairingStore<SecretServiceBackend> {
        let backend = SecretServiceBackend::connect_unix(
            &self.service.address,
            COLLECTION,
            self.owner,
            Duration::from_secs(3),
        )
        .unwrap();
        LinuxPairingStore::open(&self.state_path, self.owner, backend).unwrap()
    }
    fn empty(&self) {
        assert_eq!(self.service.state.lock().unwrap().creates, 0);
        assert!(!self.state_path.join("peers-v1.json").exists());
        assert!(!self.state_path.join("pending-v1.json").exists());
    }
    fn no_leaks(&self, process: &Process, pin: &str, endpoint: &str) {
        let state = self.service.state.lock().unwrap();
        for output in [&process.out, &process.err] {
            assert!(
                !output
                    .windows(endpoint.len())
                    .any(|w| w == endpoint.as_bytes()),
                "endpoint leaked"
            );
            assert!(
                !output.windows(pin.len()).any(|w| w == pin.as_bytes()),
                "PIN leaked"
            );
            if !state.bytes.is_empty() {
                let (_, key, _) = ControlCredentialBundle::decode(&state.bytes)
                    .unwrap()
                    .into_parts();
                for secret in [&state.bytes, &key] {
                    assert!(!output.windows(secret.len()).any(|w| w == secret));
                    let hex = secret
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>();
                    assert!(!String::from_utf8_lossy(output).contains(&hex));
                }
                for entry in fs::read_dir(&self.state_path).unwrap() {
                    let path = entry.unwrap().path();
                    if path.is_file() {
                        let bytes = fs::read(path).unwrap();
                        assert!(!bytes.windows(key.len()).any(|w| w == key));
                    }
                }
            }
        }
    }
}
struct Welcome {
    manager: EnrollmentManager,
    peer: NodeId,
    endpoint: String,
    pin: String,
}
impl Welcome {
    fn new() -> Self {
        Self::with_peer(NodeId::new())
    }
    fn with_peer(peer: NodeId) -> Self {
        let mut manager = EnrollmentManager::default();
        let status = manager
            .welcome(
                peer,
                "Synthetic welcome peer".into(),
                "127.0.0.1".parse().unwrap(),
                0,
                BTreeSet::new(),
                "synthetic-memory:",
            )
            .unwrap();
        Self {
            manager,
            peer,
            endpoint: status.endpoint.unwrap(),
            pin: status.pin.unwrap(),
        }
    }
    fn request(&self) -> Value {
        json!({"version":1,"action":"join","name":"Synthetic Linux node",
            "endpoint":self.endpoint,"pin":self.pin})
    }
    fn confirm(&self) {
        let deadline = Instant::now() + BOUND;
        while self.manager.status().phase != "confirm" {
            assert!(Instant::now() < deadline, "bounded remote confirmation");
            thread::sleep(Duration::from_millis(5));
        }
    }
}
fn approval(process: &mut Process, welcome: &Welcome) -> Value {
    process.event("authenticating");
    let event = process.event("approval_required");
    assert_eq!(event["peer_node_id"], welcome.peer.to_string());
    assert_eq!(event["display_name"], "Synthetic welcome peer");
    let id = event["request_id"]
        .as_str()
        .expect("fresh confirmation request ID");
    assert!(!id.is_empty());
    welcome.confirm();
    event
}
fn approve(event: &Value) -> Value {
    json!({"version":1,"action":"approve","request_id":event["request_id"],
        "peer_node_id":event["peer_node_id"]})
}
fn join(fixture: &Fixture) -> (Welcome, Process, Value) {
    let welcome = Welcome::new();
    let mut process = fixture.start();
    process.command(welcome.request());
    let event = approval(&mut process, &welcome);
    // The PIN is sent only through the inherited private input pipe, never argv/env.
    for file in ["cmdline", "environ"] {
        let bytes = fs::read(format!("/proc/{}/{file}", process.child.id())).unwrap();
        assert!(
            !bytes
                .windows(welcome.pin.len())
                .any(|w| w == welcome.pin.as_bytes())
        );
    }
    fixture.empty();
    (welcome, process, event)
}
fn complete(fixture: &Fixture) -> (NodeId, Vec<u8>) {
    let (welcome, mut process, event) = join(fixture);
    process.command(approve(&event));
    welcome.manager.approve().unwrap();
    let deadline = Instant::now() + BOUND;
    let mut remote_saved = None;
    loop {
        welcome.manager.stage_with(|_| Ok(())).unwrap();
        welcome
            .manager
            .persist_with(|bytes| {
                remote_saved = Some(bytes.to_vec());
                Ok(())
            })
            .unwrap();
        process.collect();
        if process.child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "bounded bilateral persistence; remote={}",
            welcome.manager.status().phase
        );
        thread::sleep(Duration::from_millis(5));
    }
    let local = process.event("locally_paired");
    assert_eq!(local["local_saved"], true);
    assert_eq!(local["remote_completion_confirmed"], false);
    assert_eq!(local["control_endpoint_available"], false);
    let paired = process.event("paired");
    assert_eq!(paired["remote_completion_confirmed"], true);
    assert_eq!(paired["control_endpoint_available"], false);
    assert!(process.finish().success());
    fixture.no_leaks(&process, &welcome.pin, &welcome.endpoint);
    let state = fixture.service.state.lock().unwrap();
    assert_eq!(state.creates, 1);
    assert!(
        state.reads >= 1,
        "write must be read back from private backend"
    );
    drop(state);
    (
        welcome.peer,
        remote_saved.expect("remote explicit approval persisted its own identity"),
    )
}
#[test]
fn ipc_pairing_bilateral_enrollment_lists_bootstraps_and_explicitly_revokes() {
    let fixture = Fixture::new();
    let (peer, remote_saved) = complete(&fixture);
    let remote = PairingInvitation::decode_for(peer, &remote_saved).unwrap();
    let remote_directory = PairedPeerDirectory::decode_json(peer, remote.directory_json()).unwrap();
    let store = fixture.store();
    let material = store.bootstrap(peer).unwrap();
    let local_bundle = ControlCredentialBundle::decode(material.credential_bytes()).unwrap();
    let (local_certs, local_key, _) = local_bundle.into_parts();
    let remote_bundle = ControlCredentialBundle::decode(remote.credential_bundle()).unwrap();
    let (remote_certs, remote_key, _) = remote_bundle.into_parts();
    assert_ne!(local_key, remote_key, "private identity remains local");
    assert_eq!(
        remote_directory.active_peers().next().unwrap().node_id,
        fixture.owner
    );
    assert_eq!(
        store
            .load_directory()
            .unwrap()
            .active_peers()
            .next()
            .unwrap()
            .node_id,
        peer
    );
    assert_ne!(local_certs[0], remote_certs[0]);
    assert_eq!(
        remote_directory
            .active_peers()
            .next()
            .unwrap()
            .peer_certificate_sha256,
        CertificateFingerprint::from_certificate_der(&local_certs[0]).unwrap()
    );
    assert_eq!(
        store
            .load_directory()
            .unwrap()
            .active_peers()
            .next()
            .unwrap()
            .peer_certificate_sha256,
        CertificateFingerprint::from_certificate_der(&remote_certs[0]).unwrap()
    );
    drop(store);
    let counters_before_management = {
        let mut state = fixture.service.state.lock().unwrap();
        state.locked = true;
        (
            state.sessions,
            state.collection_property_reads,
            state.reads,
            state.creates,
        )
    };
    let mut metadata_args = fixture.args();
    let bus_index = metadata_args
        .iter()
        .position(|arg| arg == "--pairing-bus-address")
        .unwrap()
        + 1;
    metadata_args[bus_index] = format!("unix:path={}/absent", fixture.service.root.display());
    let mut list = Process::start(&metadata_args);
    list.command(json!({"version":1,"action":"list"}));
    let peers = list.event("peers");
    assert_eq!(peers["pending_enrollment"], false);
    assert_eq!(peers["peers"].as_array().unwrap().len(), 1);
    assert_eq!(peers["peers"][0]["peer_node_id"], peer.to_string());
    assert_eq!(peers["peers"][0]["state"], "active");
    assert!(list.finish().success());
    let before_revoke = fs::read(fixture.state_path.join("peers-v1.json")).unwrap();
    let mut revoke = Process::start(&metadata_args);
    revoke.command(json!({"version":1,"action":"revoke","peer_node_id":peer}));
    let event = revoke.event("revoke_confirmation_required");
    assert_eq!(
        fs::read(fixture.state_path.join("peers-v1.json")).unwrap(),
        before_revoke,
        "request alone does not revoke"
    );
    revoke.command(json!({"version":1,"action":"confirm_revoke",
        "request_id":event["request_id"],"peer_node_id":peer}));
    let event = revoke.event("revoked");
    assert_eq!(event["credential_retained"], true);
    assert!(revoke.finish().success());
    let mut list = Process::start(&metadata_args);
    list.command(json!({"version":1,"action":"list"}));
    assert_eq!(list.event("peers")["peers"][0]["state"], "revoked");
    assert!(list.finish().success());
    {
        let mut state = fixture.service.state.lock().unwrap();
        assert_eq!(
            (
                state.sessions,
                state.collection_property_reads,
                state.reads,
                state.creates
            ),
            counters_before_management,
            "metadata-only management must not contact protected storage"
        );
        state.locked = false;
    }
    assert!(fixture.store().bootstrap(peer).is_err());
    assert_eq!(fixture.service.state.lock().unwrap().creates, 1);
}
#[test]
fn ipc_pairing_rejects_preapproval_wrong_peer_stale_request_and_replay() {
    for case in ["preapprove", "wrong_peer", "stale_request", "replay"] {
        let fixture = Fixture::new();
        let welcome = Welcome::new();
        let mut process = fixture.start();
        if case == "preapprove" {
            let request = serde_json::to_vec(&welcome.request()).unwrap();
            let premature = serde_json::to_vec(&json!({"version":1,"action":"approve",
                "request_id":NodeId::new(),"peer_node_id":welcome.peer}))
            .unwrap();
            process.frames(&[&request, &premature]);
        } else {
            process.command(welcome.request());
            let event = approval(&mut process, &welcome);
            let mut response = approve(&event);
            match case {
                "wrong_peer" => response["peer_node_id"] = json!(NodeId::new()),
                "stale_request" => response["request_id"] = json!(NodeId::new()),
                "replay" => {
                    let bytes = serde_json::to_vec(&response).unwrap();
                    process.frames(&[&bytes, &bytes]);
                }
                _ => unreachable!(),
            }
            if case != "replay" {
                process.command(response);
            }
        }
        assert!(!process.finish().success(), "{case} must fail closed");
        fixture.empty();
        fixture.no_leaks(&process, &welcome.pin, &welcome.endpoint);
    }
}
#[test]
fn ipc_pairing_requires_peer_approval_and_cancel_or_eof_never_persists() {
    for case in ["cancel", "eof", "local_approval_only"] {
        let fixture = Fixture::new();
        let (welcome, mut process, event) = join(&fixture);
        if case == "local_approval_only" {
            process.command(approve(&event));
            let deadline = Instant::now() + Duration::from_millis(250);
            while Instant::now() < deadline {
                assert!(process.child.try_wait().unwrap().is_none());
                fixture.empty();
                thread::sleep(Duration::from_millis(10));
            }
        }
        if case == "eof" {
            drop(process.child.stdin.take());
        } else {
            process.command(json!({"version":1,"action":"cancel"}));
        }
        process.finish();
        fixture.empty();
        fixture.no_leaks(&process, &welcome.pin, &welcome.endpoint);
    }
}
#[test]
fn ipc_pairing_locked_or_missing_backend_fails_before_networking() {
    for case in ["locked", "missing_bus", "missing_collection"] {
        let fixture = Fixture::new();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut args = fixture.args();
        match case {
            "locked" => fixture.service.state.lock().unwrap().locked = true,
            "missing_bus" => {
                let index = args
                    .iter()
                    .position(|s| s == "--pairing-bus-address")
                    .unwrap()
                    + 1;
                args[index] = format!("unix:path={}/absent", fixture.service.root.display());
            }
            "missing_collection" => *args.last_mut().unwrap() = format!("{COLLECTION}_absent"),
            _ => unreachable!(),
        }
        let mut process = Process::start(&args);
        process.command(json!({"version":1,"action":"join","name":"Synthetic",
            "endpoint":listener.local_addr().unwrap().to_string(),"pin":"12345678"}));
        assert!(!process.finish().success());
        assert!(matches!(listener.accept(), Err(e) if e.kind() == io::ErrorKind::WouldBlock));
        assert_eq!(fixture.service.state.lock().unwrap().creates, 0);
        assert_eq!(fixture.service.state.lock().unwrap().reads, 0);
        fixture.no_leaks(
            &process,
            "12345678",
            &listener.local_addr().unwrap().to_string(),
        );
    }
}
#[test]
fn ipc_pairing_revoke_rejects_stale_wrong_peer_replay_cancel_and_eof() {
    let fixture = Fixture::new();
    let (peer, _) = complete(&fixture);
    let original = fs::read(fixture.state_path.join("peers-v1.json")).unwrap();
    let mut stale = Value::Null;
    for case in ["cancel", "stale_request", "wrong_peer", "replay", "eof"] {
        let mut process = fixture.start();
        process.command(json!({"version":1,"action":"revoke","peer_node_id":peer}));
        let event = process.event("revoke_confirmation_required");
        assert_ne!(event["request_id"], stale, "fresh request ID per operation");
        let mut response = json!({"version":1,"action":"confirm_revoke",
            "request_id":event["request_id"],"peer_node_id":peer});
        match case {
            "cancel" => process.command(json!({"version":1,"action":"cancel"})),
            "stale_request" => {
                response["request_id"] = stale.clone();
                process.command(response);
            }
            "wrong_peer" => {
                response["peer_node_id"] = json!(NodeId::new());
                process.command(response);
            }
            "replay" => {
                let bytes = serde_json::to_vec(&response).unwrap();
                process.frames(&[&bytes, &bytes]);
            }
            "eof" => drop(process.child.stdin.take()),
            _ => unreachable!(),
        }
        let status = process.finish();
        if case != "cancel" {
            assert!(!status.success());
        }
        assert_eq!(
            fs::read(fixture.state_path.join("peers-v1.json")).unwrap(),
            original
        );
        assert!(fixture.store().bootstrap(peer).is_ok());
        assert_eq!(fixture.service.state.lock().unwrap().creates, 1);
        stale = event["request_id"].clone();
    }
}
#[test]
fn ipc_pairing_pending_or_corrupt_journal_blocks_before_networking() {
    for valid in [false, true] {
        let fixture = Fixture::new();
        let journal = if valid {
            complete(&fixture);
            fs::read(fixture.state_path.join("peers-v1.json")).unwrap()
        } else {
            b"corrupt synthetic journal".to_vec()
        };
        let creates = fixture.service.state.lock().unwrap().creates;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(fixture.state_path.join("pending-v1.json"))
            .unwrap();
        file.write_all(&journal).unwrap();
        drop(file);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut process = fixture.start();
        process.command(json!({"version":1,"action":"join","name":"Synthetic",
            "endpoint":listener.local_addr().unwrap().to_string(),"pin":"12345678"}));
        assert!(!process.finish().success());
        assert!(matches!(listener.accept(), Err(e) if e.kind() == io::ErrorKind::WouldBlock));
        assert_eq!(fixture.service.state.lock().unwrap().creates, creates);
        assert_eq!(
            fs::read(fixture.state_path.join("pending-v1.json")).unwrap(),
            journal
        );
        fixture.no_leaks(
            &process,
            "12345678",
            &listener.local_addr().unwrap().to_string(),
        );
        if valid {
            let mut list = fixture.start();
            list.command(json!({"version":1,"action":"list"}));
            assert_eq!(list.event("peers")["pending_enrollment"], true);
            assert!(list.finish().success());
        }
    }
}
#[test]
fn ipc_pairing_existing_and_revoked_peer_cannot_replace_identity() {
    let fixture = Fixture::new();
    let (peer, _) = complete(&fixture);
    for revoked in [false, true] {
        if revoked {
            let mut store = fixture.store();
            let paired_at = store
                .load_directory()
                .unwrap()
                .peers()
                .next()
                .unwrap()
                .paired_at_ms;
            store.revoke(peer, paired_at + 1).unwrap();
        }
        let original = fs::read(fixture.state_path.join("peers-v1.json")).unwrap();
        let welcome = Welcome::with_peer(peer);
        let mut process = fixture.start();
        process.command(welcome.request());
        assert!(!process.finish().success());
        assert_eq!(
            fs::read(fixture.state_path.join("peers-v1.json")).unwrap(),
            original
        );
        assert_eq!(fixture.service.state.lock().unwrap().creates, 1);
        assert!(!fixture.state_path.join("pending-v1.json").exists());
        fixture.no_leaks(&process, &welcome.pin, &welcome.endpoint);
    }
}
#[test]
fn ipc_pairing_uncertain_create_retains_item_without_claiming_saved_or_retrying() {
    let fixture = Fixture::new();
    fixture.service.state.lock().unwrap().fail_after_create = true;
    let (welcome, mut process, event) = join(&fixture);
    process.command(approve(&event));
    welcome.manager.approve().unwrap();
    let deadline = Instant::now() + BOUND;
    loop {
        welcome.manager.stage_with(|_| Ok(())).unwrap();
        assert!(
            !welcome
                .manager
                .persist_with(|_| panic!("peer cannot commit without local durable preparation"))
                .unwrap()
        );
        process.collect();
        if process.child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(Instant::now() < deadline, "bounded create failure");
        thread::sleep(Duration::from_millis(5));
    }
    let failure = process.event("failed");
    assert_eq!(failure["local_saved"], false);
    assert_eq!(failure["remote_completion_confirmed"], false);
    assert!(!process.finish().success());
    assert!(!fixture.state_path.join("pending-v1.json").exists());
    assert!(!fixture.state_path.join("peers-v1.json").exists());
    let state = fixture.service.state.lock().unwrap();
    assert_eq!(state.creates, 1, "uncertain create must not be retried");
    assert!(
        !state.missing,
        "uncertain protected item must not be deleted"
    );
    assert!(!state.bytes.is_empty());
    assert_eq!(state.forbidden, 0);
    drop(state);
    fixture.no_leaks(&process, &welcome.pin, &welcome.endpoint);
}
