#![cfg(all(target_os = "linux", feature = "paired-store"))]
#![forbid(unsafe_code)]
//! Welcome and join use two actual node executables with separate protected stores.
//! Only synthetic identities, private buses and loopback sockets are used.
use capyio_control_credentials::ControlCredentialBundle;
use capyio_core::NodeId;
use capyio_linux_pairing_store::{LinuxPairingStore, SecretServiceBackend};
use capyio_pairing::CertificateFingerprint;
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    os::{
        fd::AsFd,
        unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt, symlink},
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
        if let Some(stdout) = self.child.stdout.as_mut() {
            drain(stdout, &mut self.out);
        }
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

/// Reader exists before launching the child. Never use RDWR: that would hide
/// the production requirement that a distinct invitation consumer is present.
struct InvitationPipe {
    path: PathBuf,
    reader: File,
    bytes: Vec<u8>,
}
impl InvitationPipe {
    fn new(fixture: &Fixture) -> Self {
        let path = fixture.service.root.join("invitation.pipe");
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            &path,
            rustix::fs::Mode::from_bits_truncate(0o600),
        )
        .unwrap();
        let reader = File::from(
            rustix::fs::open(
                &path,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::NONBLOCK
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )
            .unwrap(),
        );
        Self {
            path,
            reader,
            bytes: vec![],
        }
    }
    fn request(&self, bind: &str) -> Value {
        json!({"version":1,"action":"welcome","name":"Synthetic welcome node",
            "bind":bind,"invitation_pipe":self.path})
    }
    fn invitation(&mut self) -> Value {
        let deadline = Instant::now() + BOUND;
        loop {
            drain(&mut self.reader, &mut self.bytes);
            if self.bytes.len() >= 4 {
                let len = u32::from_be_bytes(self.bytes[..4].try_into().unwrap()) as usize;
                assert!((1..=4096).contains(&len));
                if self.bytes.len() >= 4 + len {
                    assert_eq!(self.bytes.len(), 4 + len, "one invitation only");
                    let invitation: Value = serde_json::from_slice(&self.bytes[4..]).unwrap();
                    assert_eq!(invitation["version"], 1);
                    assert_eq!(invitation["event"], "invitation");
                    assert!((1..180).contains(&invitation["valid_for_secs"].as_u64().unwrap()));
                    assert_eq!(invitation.as_object().unwrap().len(), 5);
                    let pin = invitation["pin"].as_str().unwrap();
                    assert_eq!(pin.len(), 8);
                    assert!(pin.bytes().all(|b| b.is_ascii_digit()));
                    let endpoint: SocketAddr =
                        invitation["endpoint"].as_str().unwrap().parse().unwrap();
                    assert!(endpoint.ip().is_loopback());
                    assert_ne!(endpoint.port(), 0);
                    return invitation;
                }
            }
            assert!(
                Instant::now() < deadline,
                "bounded private invitation delivery"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn no_more_invitation(&mut self) {
        let count = self.bytes.len();
        drain(&mut self.reader, &mut self.bytes);
        assert_eq!(self.bytes.len(), count, "must not issue a replacement PIN");
    }
}
fn approve(event: &Value) -> Value {
    json!({"version":1,"action":"approve","request_id":event["request_id"],
        "peer_node_id":event["peer_node_id"]})
}
fn start_welcome(fixture: &Fixture) -> (Process, InvitationPipe, Value) {
    let mut pipe = InvitationPipe::new(fixture);
    let mut process = fixture.start();
    process.command(pipe.request("127.0.0.1:0"));
    process.event("waiting");
    let invitation = pipe.invitation();
    fixture.empty();
    (process, pipe, invitation)
}
fn start_join(fixture: &Fixture, invitation: &Value) -> Process {
    let mut process = fixture.start();
    process.command(
        json!({"version":1,"action":"join","name":"Synthetic joining node",
        "endpoint":invitation["endpoint"],"pin":invitation["pin"]}),
    );
    process
}
fn challenge(process: &mut Process, peer: NodeId, name: &str) -> Value {
    let event = process.event("approval_required");
    assert_eq!(event["peer_node_id"], peer.to_string());
    assert_eq!(event["display_name"], name);
    assert!(!event["request_id"].as_str().unwrap().is_empty());
    event
}
fn no_process_secrets(process: &Process, invitation: &Value) {
    let pin = invitation["pin"].as_str().unwrap();
    for file in ["cmdline", "environ"] {
        let bytes = fs::read(format!("/proc/{}/{file}", process.child.id())).unwrap();
        assert!(!bytes.windows(pin.len()).any(|w| w == pin.as_bytes()));
    }
}
fn no_leaks(fixture: &Fixture, process: &Process, invitation: &Value) {
    fixture.no_leaks(
        process,
        invitation["pin"].as_str().unwrap(),
        invitation["endpoint"].as_str().unwrap(),
    );
}
fn assert_closed(invitation: &Value) {
    let endpoint: SocketAddr = invitation["endpoint"].as_str().unwrap().parse().unwrap();
    assert!(
        TcpStream::connect_timeout(&endpoint, Duration::from_millis(250)).is_err(),
        "enrollment must not turn into a resource/control endpoint or restart"
    );
}
fn paired(process: &mut Process) {
    let local = process.event("locally_paired");
    assert_eq!(local["local_saved"], true);
    assert_eq!(local["remote_completion_confirmed"], false);
    assert_eq!(local["control_endpoint_available"], false);
    let event = process.event("paired");
    assert_eq!(event["local_saved"], true);
    assert_eq!(event["remote_completion_confirmed"], true);
    assert_eq!(event["control_endpoint_available"], false);
    assert!(process.finish().success());
    assert_eq!(process.cursor, process.out.len());
}

#[test]
fn ipc_welcome_two_nodes_bilateral_approval_preserves_separate_protected_identities() {
    let host = Fixture::new();
    let guest = Fixture::new();
    let (mut welcome, mut pipe, invitation) = start_welcome(&host);
    let mut join = start_join(&guest, &invitation);
    join.event("authenticating");
    let host_challenge = challenge(&mut welcome, guest.owner, "Synthetic joining node");
    let guest_challenge = challenge(&mut join, host.owner, "Synthetic welcome node");
    assert_ne!(host_challenge["request_id"], guest_challenge["request_id"]);
    host.empty();
    guest.empty();
    no_process_secrets(&welcome, &invitation);
    no_process_secrets(&join, &invitation);
    welcome.command(approve(&host_challenge));
    join.command(approve(&guest_challenge));
    paired(&mut welcome);
    paired(&mut join);
    pipe.no_more_invitation();
    assert_closed(&invitation);
    no_leaks(&host, &welcome, &invitation);
    no_leaks(&guest, &join, &invitation);

    let host_original = host.service.state.lock().unwrap().bytes.clone();
    let guest_original = guest.service.state.lock().unwrap().bytes.clone();
    let (host_certs, host_key, _) = ControlCredentialBundle::decode(&host_original)
        .unwrap()
        .into_parts();
    let (guest_certs, guest_key, _) = ControlCredentialBundle::decode(&guest_original)
        .unwrap()
        .into_parts();
    assert_ne!(host_key, guest_key);
    assert_ne!(host_certs[0], guest_certs[0]);
    for (fixture, peer, certificate, original) in [
        (&host, guest.owner, &guest_certs[0], &host_original),
        (&guest, host.owner, &host_certs[0], &guest_original),
    ] {
        let store = fixture.store();
        let directory = store.load_directory().unwrap();
        let peers: Vec<_> = directory.active_peers().collect();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].node_id, peer);
        assert_eq!(
            peers[0].peer_certificate_sha256,
            CertificateFingerprint::from_certificate_der(certificate).unwrap()
        );
        assert_eq!(store.bootstrap(peer).unwrap().credential_bytes(), original);
        let state = fixture.service.state.lock().unwrap();
        assert_eq!(state.creates, 1);
        assert!(state.reads >= 1);
        assert_eq!(
            state.bytes, *original,
            "original identity must remain unchanged"
        );
        assert_eq!(state.forbidden, 0);
        assert!(!fixture.state_path.join("pending-v1.json").exists());
    }
    // Neither side may leak the other side's identity either.
    no_leaks(&host, &join, &invitation);
    no_leaks(&guest, &welcome, &invitation);
}

#[test]
fn ipc_welcome_waiting_cancel_and_eof_close_without_persistence() {
    for eof in [false, true] {
        let fixture = Fixture::new();
        let (mut process, mut pipe, invitation) = start_welcome(&fixture);
        if eof {
            drop(process.child.stdin.take());
        } else {
            process.command(json!({"version":1,"action":"cancel"}));
        }
        let status = process.finish();
        if eof {
            assert!(!status.success());
        }
        fixture.empty();
        no_leaks(&fixture, &process, &invitation);
        pipe.no_more_invitation();
        assert_closed(&invitation);
    }
}

#[test]
fn ipc_welcome_two_nodes_require_both_approvals_and_cancel_or_eof_never_persist() {
    for case in ["cancel", "eof", "host_only", "guest_only"] {
        let host = Fixture::new();
        let guest = Fixture::new();
        let (mut welcome, mut pipe, invitation) = start_welcome(&host);
        let mut join = start_join(&guest, &invitation);
        join.event("authenticating");
        let host_challenge = challenge(&mut welcome, guest.owner, "Synthetic joining node");
        let guest_challenge = challenge(&mut join, host.owner, "Synthetic welcome node");
        if case == "host_only" {
            welcome.command(approve(&host_challenge));
        }
        if case == "guest_only" {
            join.command(approve(&guest_challenge));
        }
        let until = Instant::now() + Duration::from_millis(250);
        while Instant::now() < until {
            assert!(welcome.child.try_wait().unwrap().is_none());
            assert!(join.child.try_wait().unwrap().is_none());
            host.empty();
            guest.empty();
            thread::sleep(Duration::from_millis(10));
        }
        if case == "eof" {
            drop(welcome.child.stdin.take());
        } else {
            welcome.command(json!({"version":1,"action":"cancel"}));
        }
        welcome.finish();
        assert!(!join.finish().success());
        host.empty();
        guest.empty();
        pipe.no_more_invitation();
        assert_closed(&invitation);
        no_leaks(&host, &welcome, &invitation);
        no_leaks(&guest, &join, &invitation);
    }
}

#[test]
fn ipc_welcome_rejects_wrong_peer_stale_nonce_and_pipelined_preapproval() {
    for case in ["wrong_peer", "stale_nonce", "preapproval", "replay"] {
        let host = Fixture::new();
        let guest = Fixture::new();
        if case == "preapproval" {
            let pipe = InvitationPipe::new(&host);
            let mut welcome = host.start();
            let request = serde_json::to_vec(&pipe.request("127.0.0.1:0")).unwrap();
            let approval = serde_json::to_vec(&json!({"version":1,"action":"approve",
                "request_id":NodeId::new(),"peer_node_id":guest.owner}))
            .unwrap();
            welcome.frames(&[&request, &approval]);
            assert!(!welcome.finish().success());
            host.empty();
            continue;
        }
        let (mut welcome, mut pipe, invitation) = start_welcome(&host);
        let mut join = start_join(&guest, &invitation);
        join.event("authenticating");
        let event = challenge(&mut welcome, guest.owner, "Synthetic joining node");
        challenge(&mut join, host.owner, "Synthetic welcome node");
        let mut response = approve(&event);
        match case {
            "wrong_peer" => response["peer_node_id"] = json!(NodeId::new()),
            "stale_nonce" => response["request_id"] = json!(NodeId::new()),
            "replay" => {}
            _ => unreachable!(),
        }
        if case == "replay" {
            let bytes = serde_json::to_vec(&response).unwrap();
            welcome.frames(&[&bytes, &bytes]);
        } else {
            welcome.command(response);
        }
        assert!(!welcome.finish().success());
        assert!(!join.finish().success());
        host.empty();
        guest.empty();
        pipe.no_more_invitation();
        assert_closed(&invitation);
        no_leaks(&host, &welcome, &invitation);
        no_leaks(&guest, &join, &invitation);
    }
}

#[test]
fn ipc_welcome_invalid_secret_channels_and_bind_fail_without_invitation() {
    for case in [
        "regular_file",
        "symlink",
        "hardlink",
        "parent_symlink",
        "wrong_mode",
        "parent_mode",
        "no_reader",
        "stdin",
        "stdout",
        "stderr",
        "relative",
        "unspecified_v4",
        "unspecified_v6",
        "multicast",
        "empty_name",
    ] {
        let fixture = Fixture::new();
        let mut pipe = InvitationPipe::new(&fixture);
        let mut request = pipe.request("127.0.0.1:0");
        // Keep an independent reader only where the negative case requires it.
        let mut reader = Some(pipe.reader.try_clone().unwrap());
        match case {
            "regular_file" => {
                let file = fixture.service.root.join("ordinary-file");
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&file)
                    .unwrap();
                request["invitation_pipe"] = json!(file);
            }
            "symlink" => {
                let link = fixture.service.root.join("invitation-link");
                symlink(&pipe.path, &link).unwrap();
                request["invitation_pipe"] = json!(link);
            }
            "hardlink" => {
                fs::hard_link(&pipe.path, fixture.service.root.join("second-name")).unwrap();
            }
            "parent_symlink" => {
                let link = fixture.service.root.join("parent-link");
                symlink(&fixture.service.root, &link).unwrap();
                request["invitation_pipe"] = json!(link.join("invitation.pipe"));
            }
            "wrong_mode" => {
                fs::set_permissions(&pipe.path, fs::Permissions::from_mode(0o644)).unwrap()
            }
            "parent_mode" => {
                fs::set_permissions(&fixture.service.root, fs::Permissions::from_mode(0o755))
                    .unwrap()
            }
            "no_reader" => {
                drop(reader.take());
                // Replacing the held FIFO descriptor with /dev/null closes the
                // final reader without creating any reader for the child.
                pipe.reader = File::open("/dev/null").unwrap();
            }
            "stdin" => request["invitation_pipe"] = json!("/proc/self/fd/0"),
            "stdout" => request["invitation_pipe"] = json!("/proc/self/fd/1"),
            "stderr" => request["invitation_pipe"] = json!("/proc/self/fd/2"),
            "relative" => request["invitation_pipe"] = json!("invitation.pipe"),
            "unspecified_v4" => request["bind"] = json!("0.0.0.0:0"),
            "unspecified_v6" => request["bind"] = json!("[::]:0"),
            "multicast" => request["bind"] = json!("224.0.0.1:0"),
            "empty_name" => request["name"] = json!(" "),
            _ => unreachable!(),
        }
        let mut process = fixture.start();
        process.command(request);
        assert!(!process.finish().success(), "{case} must fail closed");
        fs::set_permissions(&fixture.service.root, fs::Permissions::from_mode(0o700)).unwrap();
        drain(&mut pipe.reader, &mut pipe.bytes);
        assert!(pipe.bytes.is_empty(), "{case} must not emit an invitation");
        assert!(!String::from_utf8_lossy(&process.out).contains("waiting"));
        assert!(!String::from_utf8_lossy(&process.out).contains("approval_required"));
        fixture.empty();
    }
}

#[test]
fn ipc_welcome_locked_backend_fails_before_binding_or_secret_output() {
    let fixture = Fixture::new();
    fixture.service.state.lock().unwrap().locked = true;
    let mut pipe = InvitationPipe::new(&fixture);
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = reservation.local_addr().unwrap();
    // Holding this port also makes an accidental bind an observable failure;
    // the backend must be inspected even when that bind would have failed.
    let mut process = fixture.start();
    process.command(pipe.request(&endpoint.to_string()));
    assert!(!process.finish().success());
    drain(&mut pipe.reader, &mut pipe.bytes);
    assert!(pipe.bytes.is_empty());
    assert!(process.out.is_empty());
    let state = fixture.service.state.lock().unwrap();
    assert!(
        state.collection_property_reads > 0,
        "backend checked before bind"
    );
    assert_eq!(state.creates, 0);
    assert_eq!(state.forbidden, 0);
    drop(state);
    fixture.empty();
}

#[test]
fn ipc_welcome_wrong_pin_consumes_window_without_replacement_or_restart() {
    let host = Fixture::new();
    let guest = Fixture::new();
    let (mut welcome, mut pipe, invitation) = start_welcome(&host);
    let mut wrong = invitation.clone();
    let pin = invitation["pin"].as_str().unwrap();
    wrong["pin"] = json!(if pin == "00000000" {
        "11111111"
    } else {
        "00000000"
    });
    let mut join = start_join(&guest, &wrong);
    assert!(!join.finish().success());
    assert!(!welcome.finish().success());
    host.empty();
    guest.empty();
    pipe.no_more_invitation();
    assert_closed(&invitation);
    no_leaks(&host, &welcome, &invitation);
    no_leaks(&guest, &join, &invitation);
    no_leaks(&guest, &join, &wrong);
    let mut retry = start_join(&guest, &invitation);
    assert!(!retry.finish().success());
    guest.empty();
    pipe.no_more_invitation();
    no_leaks(&guest, &retry, &invitation);
}

#[test]
fn ipc_welcome_second_joiner_cannot_replace_selected_peer_or_reuse_window() {
    let host = Fixture::new();
    let guest = Fixture::new();
    let second = Fixture::new();
    let (mut welcome, mut pipe, invitation) = start_welcome(&host);
    let mut join = start_join(&guest, &invitation);
    join.event("authenticating");
    challenge(&mut welcome, guest.owner, "Synthetic joining node");
    challenge(&mut join, host.owner, "Synthetic welcome node");
    let mut intruder = start_join(&second, &invitation);
    assert!(!intruder.finish().success());
    // A second join cannot become an approval request on the occupied window.
    welcome.collect();
    assert_eq!(welcome.cursor, welcome.out.len());
    host.empty();
    guest.empty();
    second.empty();
    welcome.command(json!({"version":1,"action":"cancel"}));
    welcome.finish();
    assert!(!join.finish().success());
    host.empty();
    guest.empty();
    pipe.no_more_invitation();
    assert_closed(&invitation);
    no_leaks(&host, &welcome, &invitation);
    no_leaks(&guest, &join, &invitation);
    no_leaks(&second, &intruder, &invitation);
}

#[test]
fn ipc_welcome_fixed_three_minute_expiry_closes_without_approval_or_persistence() {
    let fixture = Fixture::new();
    let started = Instant::now();
    let (mut process, mut pipe, invitation) = start_welcome(&fixture);
    // Exercise the production deadline without a test override or a renewed
    // lifetime. Hosted CI gives this target a 420-second outer timeout.
    let deadline = started + Duration::from_secs(195);
    loop {
        process.collect();
        if let Some(status) = process.child.try_wait().unwrap() {
            process.collect();
            assert!(!status.success());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "fixed welcome deadline must terminate"
        );
        fixture.empty();
        thread::sleep(Duration::from_millis(100));
    }
    assert!(
        started.elapsed() >= Duration::from_secs(175),
        "must exercise the real three-minute window"
    );
    fixture.empty();
    pipe.no_more_invitation();
    assert_closed(&invitation);
    no_leaks(&fixture, &process, &invitation);
}

#[test]
fn ipc_welcome_rejects_same_fifo_as_ordinary_stdout_by_inode() {
    let fixture = Fixture::new();
    let mut pipe = InvitationPipe::new(&fixture);
    let ordinary_stdout = fs::OpenOptions::new().write(true).open(&pipe.path).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_capyio-node"))
        .args(fixture.args())
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::from(ordinary_stdout))
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    nonblocking(child.stdin.as_ref().unwrap());
    nonblocking(child.stderr.as_ref().unwrap());
    let mut process = Process {
        child,
        out: vec![],
        err: vec![],
        cursor: 0,
    };
    process.command(pipe.request("127.0.0.1:0"));
    assert!(!process.finish().success());
    drain(&mut pipe.reader, &mut pipe.bytes);
    assert!(
        pipe.bytes.is_empty(),
        "ordinary stdout must never receive invitation secrets"
    );
    fixture.empty();
}
