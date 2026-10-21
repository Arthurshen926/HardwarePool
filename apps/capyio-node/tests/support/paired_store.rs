//! Shared test-only protected-store fixture. Every bus and record is synthetic.
// Integration targets deliberately use different subsets of this support API.
#![allow(dead_code)]
use capyio_control_credentials::ControlCredentialBundle;
use capyio_core::NodeId;
use capyio_linux_pairing_store::{BackendError, CredentialBackend, LinuxPairingStore, SecretBytes};
use capyio_pairing::{CredentialHandle, PairedPeerDirectory, PairingInvitation};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    os::unix::fs::DirBuilderExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Str};
pub(crate) const COLLECTION: &str = "/org/freedesktop/secrets/collection/capyio";
const ITEM: &str = "/org/freedesktop/secrets/collection/capyio/item0";
const SESSION: &str = "/org/freedesktop/secrets/session/capyio";
const BOUND: Duration = Duration::from_secs(8);
type Attributes = HashMap<String, String>;
type Shared = Arc<Mutex<KeyringState>>;
fn path(s: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(s).unwrap()
}
#[derive(Default)]
pub(crate) struct KeyringState {
    pub(crate) attributes: Attributes,
    pub(crate) bytes: Vec<u8>,
    pub(crate) locked: bool,
    pub(crate) missing: bool,
    pub(crate) sessions: usize,
    pub(crate) collection_property_reads: usize,
    pub(crate) reads: usize,
    pub(crate) forbidden: usize,
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
pub(crate) struct PrivateService {
    pub(crate) root: PathBuf,
    pub(crate) address: String,
    bus: Child,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<thread::JoinHandle<()>>,
    pub(crate) state: Shared,
}
impl PrivateService {
    pub(crate) fn start(state: Shared) -> Self {
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
pub(crate) struct SeedBackend(pub(crate) Vec<u8>);
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

/// Owns the existing isolated protocol fixture plus committed synthetic metadata.
/// It never connects to a discovered bus or modifies a pre-existing keyring.
pub(crate) struct PersistedPairing {
    pub(crate) service: PrivateService,
    pub(crate) owner: NodeId,
    pub(crate) peer: NodeId,
    pub(crate) state_path: PathBuf,
    pub(crate) bundle: Vec<u8>,
    key: Vec<u8>,
    pub(crate) directory: Vec<u8>,
}
impl PersistedPairing {
    pub(crate) fn new(
        local: NodeId,
        directory_json: &[u8],
        credentials: &ControlCredentialBundle,
    ) -> Self {
        let source = PairedPeerDirectory::decode_json(local, directory_json).unwrap();
        assert_eq!(source.peers().len(), 1, "exact synthetic peer required");
        let mut peers = source.active_peers();
        let mut record = peers.next().expect("active synthetic peer").clone();
        assert!(peers.next().is_none());
        let bundle = credentials.encode().unwrap();
        let (_, key, _) = ControlCredentialBundle::decode(&bundle)
            .unwrap()
            .into_parts();
        let handle =
            CredentialHandle::new(format!("linux-protected:{:x}", Sha256::digest(&bundle)))
                .unwrap();
        record.credential_handle = handle.clone();
        let peer = record.node_id;
        let mut directory = PairedPeerDirectory::new(local).unwrap();
        directory.add(record).unwrap();
        let directory = directory.encode_json().unwrap();
        let state = Arc::new(Mutex::new(KeyringState {
            attributes: HashMap::from([
                ("application".into(), "dev.capyio.control".into()),
                ("schema".into(), "1".into()),
                ("local-node-id".into(), local.to_string()),
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
        let invitation = PairingInvitation::new(local, directory.clone(), bundle.clone())
            .unwrap()
            .encode()
            .unwrap();
        let mut store =
            LinuxPairingStore::open(&state_path, local, SeedBackend(bundle.clone())).unwrap();
        store.stage_enrollment(&invitation).unwrap();
        store.commit_pending_enrollment().unwrap();
        drop(store);
        Self {
            service,
            owner: local,
            peer,
            state_path,
            bundle,
            key,
            directory,
        }
    }
    pub(crate) fn args(&self, command: &str) -> Vec<String> {
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
    pub(crate) fn revoke(&self) {
        let mut store = LinuxPairingStore::open(
            &self.state_path,
            self.owner,
            SeedBackend(self.bundle.clone()),
        )
        .unwrap();
        store.revoke(self.peer, 2).unwrap();
    }
    pub(crate) fn assert_no_leak(&self, output: &[u8]) {
        for secret in [&self.bundle, &self.key] {
            assert!(
                !output.windows(secret.len()).any(|w| w == secret.as_slice()),
                "synthetic secret leaked in executable output"
            );
            let hex = secret
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            assert!(
                !String::from_utf8_lossy(output).contains(&hex),
                "synthetic secret hex leaked in executable output"
            );
        }
        for file in fs::read_dir(&self.state_path).unwrap() {
            let bytes = fs::read(file.unwrap().path()).unwrap();
            assert!(!bytes.windows(self.key.len()).any(|w| w == self.key));
        }
    }
}
