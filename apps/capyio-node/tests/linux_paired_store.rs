#![cfg(all(target_os = "linux", feature = "paired-store"))]
#![forbid(unsafe_code)]
//! Actual finite node executable using persisted synthetic pairing metadata and
//! an owned private D-Bus service. Never opens a real keyring or media/input device.
use capyio_control_credentials::ControlCredentialBundle;
use capyio_core::{CapabilityId, NodeId, PortId};
use capyio_linux_pairing_store::{BackendError, CredentialBackend, LinuxPairingStore, SecretBytes};
use capyio_pairing::{
    CertificateFingerprint, CredentialHandle, PairedPeerDirectory, PairedPeerRecord,
    PairingInvitation,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::{self, Read, Write},
    os::{fd::AsFd, unix::fs::DirBuilderExt},
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Str};
const COLLECTION: &str = "/org/freedesktop/secrets/collection/capyio";
const ITEM: &str = "/org/freedesktop/secrets/collection/capyio/item0";
const SESSION: &str = "/org/freedesktop/secrets/session/capyio";
const BOUND: Duration = Duration::from_secs(8);
type Attributes = HashMap<String, String>;
type Shared = Arc<Mutex<KeyringState>>;
fn path(s: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(s).unwrap()
}
#[derive(Default)]
struct KeyringState {
    attributes: Attributes,
    bytes: Vec<u8>,
    locked: bool,
    missing: bool,
    sessions: usize,
    collection_property_reads: usize,
    reads: usize,
    forbidden: usize,
}
struct Service(Shared);
#[zbus::interface(name = "org.freedesktop.Secret.Service")]
impl Service {
    fn open_session(
        &self,
        algorithm: &str,
        input: OwnedValue,
    ) -> zbus::fdo::Result<(OwnedValue, OwnedObjectPath)> {
        assert_eq!(algorithm, "plain");
        assert_eq!(<&str>::try_from(&input).unwrap(), "");
        self.0.lock().unwrap().sessions += 1;
        Ok((OwnedValue::from(Str::from("")), path(SESSION)))
    }
    fn unlock(
        &self,
        _objects: Vec<OwnedObjectPath>,
    ) -> zbus::fdo::Result<(Vec<OwnedObjectPath>, OwnedObjectPath)> {
        self.0.lock().unwrap().forbidden += 1;
        Err(zbus::fdo::Error::AccessDenied(
            "forbidden synthetic operation".into(),
        ))
    }
    fn create_collection(
        &self,
        _properties: HashMap<String, OwnedValue>,
        _alias: &str,
    ) -> zbus::fdo::Result<(OwnedObjectPath, OwnedObjectPath)> {
        self.0.lock().unwrap().forbidden += 1;
        Err(zbus::fdo::Error::AccessDenied(
            "forbidden synthetic operation".into(),
        ))
    }
}
struct Collection(Shared);
#[zbus::interface(name = "org.freedesktop.Secret.Collection")]
impl Collection {
    #[zbus(property)]
    fn locked(&self) -> bool {
        let mut state = self.0.lock().unwrap();
        state.collection_property_reads += 1;
        state.locked
    }
    fn search_items(&self, attributes: Attributes) -> Vec<OwnedObjectPath> {
        let s = self.0.lock().unwrap();
        if !s.missing && attributes == s.attributes {
            vec![path(ITEM)]
        } else {
            vec![]
        }
    }
    fn create_item(
        &self,
        _properties: HashMap<String, OwnedValue>,
        _secret: (OwnedObjectPath, Vec<u8>, Vec<u8>, String),
        _replace: bool,
    ) -> zbus::fdo::Result<(OwnedObjectPath, OwnedObjectPath)> {
        self.0.lock().unwrap().forbidden += 1;
        Err(zbus::fdo::Error::AccessDenied("read only fixture".into()))
    }
}
struct Item(Shared);
#[zbus::interface(name = "org.freedesktop.Secret.Item")]
impl Item {
    #[zbus(property)]
    fn locked(&self) -> bool {
        self.0.lock().unwrap().locked
    }
    #[zbus(property)]
    fn attributes(&self) -> Attributes {
        self.0.lock().unwrap().attributes.clone()
    }
    #[allow(clippy::type_complexity)]
    fn get_secret(
        &self,
        session: OwnedObjectPath,
    ) -> ((OwnedObjectPath, Vec<u8>, Vec<u8>, String),) {
        assert_eq!(session.as_str(), SESSION);
        let mut s = self.0.lock().unwrap();
        s.reads += 1;
        ((
            session,
            vec![],
            s.bytes.clone(),
            "application/x-capyio-control-credential".into(),
        ),)
    }
    fn delete(&self) -> zbus::fdo::Result<OwnedObjectPath> {
        self.0.lock().unwrap().forbidden += 1;
        Err(zbus::fdo::Error::AccessDenied("read only fixture".into()))
    }
}
struct PrivateService {
    root: PathBuf,
    address: String,
    bus: Child,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<thread::JoinHandle<()>>,
    state: Shared,
}
impl PrivateService {
    fn start(state: Shared) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "capyio-node-paired-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let socket = root.join("bus");
        let address = format!("unix:path={}", socket.display());
        let config = root.join("bus.conf");
        fs::write(&config, format!("<busconfig><type>session</type><listen>{address}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow user=\"*\"/><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>")).unwrap();
        let bus = Command::new("/usr/bin/dbus-daemon")
            .args(["--nofork", "--nopidfile"])
            .arg(format!("--config-file={}", config.display()))
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("mandatory private-bus test requires dbus-daemon");
        let mut service = Self {
            root,
            address,
            bus,
            stop: None,
            task: None,
            state: state.clone(),
        };
        let deadline = Instant::now() + BOUND;
        while !socket.exists() {
            assert!(
                service.bus.try_wait().unwrap().is_none(),
                "private bus exited"
            );
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        let address = service.address.clone();
        let (ready, wait) = mpsc::sync_channel(1);
        let (stop, stopped) = tokio::sync::oneshot::channel();
        service.stop = Some(stop);
        service.task = Some(thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let connection = zbus::connection::Builder::address(address.as_str())
                        .unwrap()
                        .name("org.freedesktop.secrets")
                        .unwrap()
                        .serve_at("/org/freedesktop/secrets", Service(state.clone()))
                        .unwrap()
                        .serve_at(COLLECTION, Collection(state.clone()))
                        .unwrap()
                        .serve_at(ITEM, Item(state))
                        .unwrap()
                        .build()
                        .await
                        .unwrap();
                    ready.send(()).unwrap();
                    let _ = stopped.await;
                    let _ = connection.close().await;
                });
        }));
        wait.recv_timeout(BOUND)
            .expect("bounded fake service startup");
        service
    }
}
impl Drop for PrivateService {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            let result = task.join();
            if !thread::panicking() {
                result.unwrap();
            }
        }
        let _ = self.bus.kill();
        let _ = self.bus.wait();
        let _ = fs::remove_dir_all(&self.root);
        if !thread::panicking() {
            assert_eq!(self.state.lock().unwrap().forbidden, 0);
        }
    }
}
#[derive(Clone)]
struct SeedBackend(Vec<u8>);
impl CredentialBackend for SeedBackend {
    fn load(
        &self,
        _owner: NodeId,
        _handle: &CredentialHandle,
    ) -> Result<SecretBytes, BackendError> {
        SecretBytes::new(self.0.clone())
    }
    fn put_exact(
        &self,
        _owner: NodeId,
        _handle: &CredentialHandle,
        bytes: &[u8],
    ) -> Result<(), BackendError> {
        if bytes == self.0 {
            Ok(())
        } else {
            Err(BackendError::Conflict)
        }
    }
}
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
struct Fixture {
    service: PrivateService,
    owner: NodeId,
    peer: NodeId,
    state_path: PathBuf,
    bundle: Vec<u8>,
    key: Vec<u8>,
    directory: Vec<u8>,
}
impl Fixture {
    fn new() -> Self {
        let owner = NodeId::new();
        let peer = NodeId::new();
        let (bundle, key, pin) = credentials();
        let handle =
            CredentialHandle::new(format!("linux-protected:{:x}", Sha256::digest(&bundle)))
                .unwrap();
        let state = Arc::new(Mutex::new(KeyringState {
            attributes: HashMap::from([
                ("application".into(), "dev.capyio.control".into()),
                ("schema".into(), "1".into()),
                ("local-node-id".into(), owner.to_string()),
                ("credential-handle".into(), handle.as_str().into()),
            ]),
            bytes: bundle.clone(),
            ..Default::default()
        }));
        let service = PrivateService::start(state);
        let state_path = service.root.join("state");
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&state_path)
            .unwrap();
        let mut directory = PairedPeerDirectory::new(owner).unwrap();
        directory
            .add(
                PairedPeerRecord::new(peer, "Synthetic peer", "localhost", handle, pin, 1).unwrap(),
            )
            .unwrap();
        let directory = directory.encode_json().unwrap();
        let invitation = PairingInvitation::new(owner, directory.clone(), bundle.clone())
            .unwrap()
            .encode()
            .unwrap();
        let mut store =
            LinuxPairingStore::open(&state_path, owner, SeedBackend(bundle.clone())).unwrap();
        store.stage_enrollment(&invitation).unwrap();
        store.commit_pending_enrollment().unwrap();
        drop(store);
        Self {
            service,
            owner,
            peer,
            state_path,
            bundle,
            key,
            directory,
        }
    }
    fn config(&self) -> Value {
        json!({"version":1,"node_id":self.owner,"source":{"node_id":self.peer,"capability_id":CapabilityId::new(),"port_id":PortId::new()},"listen":"127.0.0.1:0","duration_secs":3})
    }
    fn args(&self, command: &str) -> Vec<String> {
        vec![
            command.into(),
            "--trusted-lab".into(),
            "--pairing-state".into(),
            self.state_path.display().to_string(),
            "--pairing-node".into(),
            self.owner.to_string(),
            "--pairing-peer".into(),
            self.peer.to_string(),
            "--pairing-bus-address".into(),
            self.service.address.clone(),
            "--pairing-collection".into(),
            COLLECTION.into(),
        ]
    }
    fn revoke(&self) {
        let mut store = LinuxPairingStore::open(
            &self.state_path,
            self.owner,
            SeedBackend(self.bundle.clone()),
        )
        .unwrap();
        store.revoke(self.peer, 2).unwrap();
    }
    fn no_leak(&self, process: &Process) {
        for secret in [&self.bundle, &self.key] {
            for output in [&process.out, &process.err] {
                assert!(
                    !output.windows(secret.len()).any(|w| w == secret.as_slice()),
                    "secret leaked in executable output"
                );
            }
        }
        let hex = self
            .key
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert!(!String::from_utf8_lossy(&process.out).contains(&hex));
        assert!(!String::from_utf8_lossy(&process.err).contains(&hex));
        for file in fs::read_dir(&self.state_path).unwrap() {
            let bytes = fs::read(file.unwrap().path()).unwrap();
            assert!(!bytes.windows(self.key.len()).any(|w| w == self.key));
        }
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
    for command in ["linux-camera", "linux-touchpad"] {
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
