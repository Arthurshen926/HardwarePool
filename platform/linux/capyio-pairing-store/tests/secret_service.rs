#![cfg(all(target_os = "linux", feature = "native-secret-service"))]
#![forbid(unsafe_code)]
//! Actual D-Bus wire tests against a deliberately small, synthetic Secret Service.
//! Every fixture owns its dbus-daemon and Unix socket. It never consults the
//! session-bus environment, activates a desktop service, or opens a user keyring.

use capyio_control_credentials::{ControlCredentialBundle, MAX_CONTROL_CREDENTIAL_BUNDLE_BYTES};
use capyio_core::NodeId;
use capyio_linux_pairing_store::{BackendError, CredentialBackend, SecretServiceBackend};
use capyio_pairing::CredentialHandle;
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

type Attributes = HashMap<String, String>;
type Secret = (OwnedObjectPath, Vec<u8>, Vec<u8>, String);
type Shared = Arc<Mutex<State>>;
const SERVICE: &str = "org.freedesktop.secrets";
const ROOT: &str = "/org/freedesktop/secrets";
const COLLECTION: &str = "/org/freedesktop/secrets/collection/capyio_test";
const SESSION: &str = "/org/freedesktop/secrets/session/capyio_test";
const PROMPT: &str = "/org/freedesktop/secrets/prompt/capyio_test";
const CONTENT_TYPE: &str = "application/x-capyio-control-credential";
const OPERATION_TIMEOUT: Duration = Duration::from_secs(2);

fn path(value: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(value).expect("fixture object path")
}
fn item_path(index: usize) -> OwnedObjectPath {
    path(&format!("{COLLECTION}/item{index}"))
}
fn attributes(owner: NodeId, handle: &CredentialHandle) -> Attributes {
    HashMap::from([
        ("application".into(), "dev.capyio.control".into()),
        ("schema".into(), "1".into()),
        ("local-node-id".into(), owner.to_string()),
        ("credential-handle".into(), handle.as_str().into()),
    ])
}
fn fixture_credential(tag: u8) -> (Vec<u8>, CredentialHandle) {
    // Synthetic format-valid components, deliberately not issued identities.
    let bytes = ControlCredentialBundle::new(
        vec![vec![0x30, 0x03, 0x01, tag]],
        vec![0x30, 0x03, 0x02, tag],
        vec![0x30, 0x03, 0x03, tag],
    )
    .unwrap()
    .encode()
    .unwrap();
    let handle =
        CredentialHandle::new(format!("linux-protected:{:x}", Sha256::digest(&bytes))).unwrap();
    (bytes, handle)
}

#[derive(Clone)]
struct StoredItem {
    attributes: Attributes,
    bytes: Vec<u8>,
    locked: bool,
    content_type: String,
    session: OwnedObjectPath,
    parameters: Vec<u8>,
}
impl StoredItem {
    fn new(owner: NodeId, handle: &CredentialHandle, bytes: &[u8]) -> Self {
        Self {
            attributes: attributes(owner, handle),
            bytes: bytes.to_vec(),
            locked: false,
            content_type: CONTENT_TYPE.into(),
            session: path(SESSION),
            parameters: Vec::new(),
        }
    }
}
#[derive(Clone, Copy, Default)]
enum CreateReply {
    #[default]
    Normal,
    Prompt,
    DelayAfterPersist,
    DelayBeforePersist,
    ErrorAfterPersist,
}
#[derive(Default)]
struct State {
    held_namespace: Option<String>,
    allow_delayed_commit: bool,
    collection_locked: bool,
    items: Vec<StoredItem>,
    search_override: Option<Vec<OwnedObjectPath>>,
    search_delay: Duration,
    create_reply: CreateReply,
    searches: Vec<Attributes>,
    open_sessions: usize,
    get_secrets: usize,
    create_calls: usize,
    replace_flags: Vec<bool>,
    created_attributes: Vec<Attributes>,
    created_content_types: Vec<String>,
    invalid_wire: usize,
    create_collection_calls: usize,
    unlock_calls: usize,
    prompt_calls: usize,
    delete_calls: usize,
    set_secret_calls: usize,
}

struct Service(Shared);
#[zbus::interface(name = "org.freedesktop.Secret.Service")]
impl Service {
    fn open_session(
        &self,
        algorithm: &str,
        input: OwnedValue,
    ) -> zbus::fdo::Result<(OwnedValue, OwnedObjectPath)> {
        let mut state = self.0.lock().unwrap();
        state.open_sessions += 1;
        if algorithm != "plain" || <&str>::try_from(&input).ok() != Some("") {
            state.invalid_wire += 1;
            return Err(zbus::fdo::Error::InvalidArgs(
                "fixture plain session".into(),
            ));
        }
        Ok((OwnedValue::from(Str::from("")), path(SESSION)))
    }
    fn create_collection(
        &self,
        _properties: HashMap<String, OwnedValue>,
        _alias: &str,
    ) -> zbus::fdo::Result<(OwnedObjectPath, OwnedObjectPath)> {
        self.0.lock().unwrap().create_collection_calls += 1;
        Err(zbus::fdo::Error::AccessDenied(
            "forbidden by fixture".into(),
        ))
    }
    fn unlock(
        &self,
        _objects: Vec<OwnedObjectPath>,
    ) -> zbus::fdo::Result<(Vec<OwnedObjectPath>, OwnedObjectPath)> {
        self.0.lock().unwrap().unlock_calls += 1;
        Err(zbus::fdo::Error::AccessDenied(
            "forbidden by fixture".into(),
        ))
    }
}
struct Collection(Shared);
#[zbus::interface(name = "org.freedesktop.Secret.Collection")]
impl Collection {
    #[zbus(property)]
    fn locked(&self) -> bool {
        self.0.lock().unwrap().collection_locked
    }
    async fn search_items(&self, attributes: Attributes) -> Vec<OwnedObjectPath> {
        let (delay, items) = {
            let mut state = self.0.lock().unwrap();
            state.searches.push(attributes.clone());
            let items = state.search_override.clone().unwrap_or_else(|| {
                state
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| {
                        attributes
                            .iter()
                            .all(|(key, value)| item.attributes.get(key) == Some(value))
                    })
                    .map(|(index, _)| item_path(index))
                    .collect()
            });
            (state.search_delay, items)
        };
        tokio::time::sleep(delay).await;
        items
    }
    async fn create_item(
        &self,
        mut properties: HashMap<String, OwnedValue>,
        secret: Secret,
        replace: bool,
    ) -> zbus::fdo::Result<(OwnedObjectPath, OwnedObjectPath)> {
        let (index, behavior, delayed_item) = {
            let mut state = self.0.lock().unwrap();
            state.create_calls += 1;
            state.replace_flags.push(replace);
            let attrs = properties
                .remove("org.freedesktop.Secret.Item.Attributes")
                .and_then(|value| Attributes::try_from(value).ok());
            let label = properties.remove("org.freedesktop.Secret.Item.Label");
            if attrs.is_none()
                || label
                    .as_ref()
                    .and_then(|value| <&str>::try_from(value).ok())
                    .is_none()
                || !properties.is_empty()
                || replace
                || secret.0.as_str() != SESSION
                || !secret.1.is_empty()
                || secret.3 != CONTENT_TYPE
                || ControlCredentialBundle::decode(&secret.2).is_err()
                || state.items.len() >= 2
            {
                state.invalid_wire += 1;
                return Err(zbus::fdo::Error::InvalidArgs(
                    "invalid fixture create".into(),
                ));
            }
            let attrs = attrs.unwrap();
            state.created_attributes.push(attrs.clone());
            state.created_content_types.push(secret.3.clone());
            let index = state.items.len();
            let item = StoredItem {
                attributes: attrs,
                bytes: secret.2,
                locked: false,
                content_type: secret.3,
                session: secret.0,
                parameters: secret.1,
            };
            let delayed_item = if matches!(state.create_reply, CreateReply::DelayBeforePersist) {
                Some(item)
            } else {
                state.items.push(item);
                None
            };
            (index, state.create_reply, delayed_item)
        };
        match behavior {
            CreateReply::Normal => Ok((item_path(index), path("/"))),
            CreateReply::Prompt => Ok((item_path(index), path(PROMPT))),
            CreateReply::DelayAfterPersist => {
                tokio::time::sleep(Duration::from_secs(2)).await;
                Ok((item_path(index), path("/")))
            }
            CreateReply::DelayBeforePersist => {
                tokio::time::sleep(Duration::from_millis(350)).await;
                // The gate makes the temporal assertion independent of host
                // scheduling: the client has timed out before we may commit.
                loop {
                    if self.0.lock().unwrap().allow_delayed_commit {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                self.0.lock().unwrap().items.push(delayed_item.unwrap());
                Ok((item_path(index), path("/")))
            }
            CreateReply::ErrorAfterPersist => Err(zbus::fdo::Error::Failed(
                "synthetic failure after committing item".into(),
            )),
        }
    }
    fn delete(&self) -> zbus::fdo::Result<OwnedObjectPath> {
        self.0.lock().unwrap().delete_calls += 1;
        Err(zbus::fdo::Error::AccessDenied(
            "forbidden by fixture".into(),
        ))
    }
}
struct Item {
    state: Shared,
    index: usize,
}
#[zbus::interface(name = "org.freedesktop.Secret.Item")]
impl Item {
    #[zbus(property)]
    fn locked(&self) -> zbus::fdo::Result<bool> {
        self.state
            .lock()
            .unwrap()
            .items
            .get(self.index)
            .map(|item| item.locked)
            .ok_or_else(|| zbus::fdo::Error::UnknownObject("missing fixture item".into()))
    }
    #[zbus(property)]
    fn attributes(&self) -> zbus::fdo::Result<Attributes> {
        self.state
            .lock()
            .unwrap()
            .items
            .get(self.index)
            .map(|item| item.attributes.clone())
            .ok_or_else(|| zbus::fdo::Error::UnknownObject("missing fixture item".into()))
    }
    fn get_secret(&self, session: OwnedObjectPath) -> zbus::fdo::Result<(Secret,)> {
        let mut state = self.state.lock().unwrap();
        state.get_secrets += 1;
        if session.as_str() != SESSION {
            state.invalid_wire += 1;
            return Err(zbus::fdo::Error::InvalidArgs("wrong session".into()));
        }
        let item = state
            .items
            .get(self.index)
            .ok_or_else(|| zbus::fdo::Error::UnknownObject("missing fixture item".into()))?;
        // One Secret struct result, not four top-level D-Bus arguments.
        Ok(((
            item.session.clone(),
            item.parameters.clone(),
            item.bytes.clone(),
            item.content_type.clone(),
        ),))
    }
    fn delete(&self) -> zbus::fdo::Result<OwnedObjectPath> {
        self.state.lock().unwrap().delete_calls += 1;
        Err(zbus::fdo::Error::AccessDenied(
            "forbidden by fixture".into(),
        ))
    }
    fn set_secret(&self, _secret: Secret) -> zbus::fdo::Result<()> {
        self.state.lock().unwrap().set_secret_calls += 1;
        Err(zbus::fdo::Error::AccessDenied(
            "forbidden by fixture".into(),
        ))
    }
}
struct Prompt(Shared);
#[zbus::interface(name = "org.freedesktop.Secret.Prompt")]
impl Prompt {
    fn prompt(&self, _window_id: &str) -> zbus::fdo::Result<()> {
        self.0.lock().unwrap().prompt_calls += 1;
        Err(zbus::fdo::Error::AccessDenied(
            "forbidden by fixture".into(),
        ))
    }
}
struct Session;
#[zbus::interface(name = "org.freedesktop.Secret.Session")]
impl Session {
    fn close(&self) {}
}

struct PrivateBus {
    root: PathBuf,
    address: String,
    child: Child,
}
impl PrivateBus {
    fn start() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "capyio-ss-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let socket = root.join("bus");
        let address = format!("unix:path={}", socket.display());
        let config = root.join("bus.conf");
        fs::write(
            &config,
            format!(
                "<busconfig><type>session</type><listen>{address}</listen>\
                 <auth>EXTERNAL</auth><policy context=\"default\">\
                 <allow user=\"*\"/><allow own=\"*\"/>\
                 <allow send_destination=\"*\"/><allow receive_sender=\"*\"/>\
                 </policy></busconfig>"
            ),
        )
        .unwrap();
        let child = Command::new("/usr/bin/dbus-daemon")
            .arg("--nofork")
            .arg("--nopidfile")
            .arg(format!("--config-file={}", config.display()))
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("private wire tests require /usr/bin/dbus-daemon; never skip them");
        let mut bus = Self {
            root,
            address,
            child,
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while !socket.exists() {
            assert!(
                bus.child.try_wait().unwrap().is_none(),
                "private dbus-daemon exited"
            );
            assert!(
                Instant::now() < deadline,
                "private dbus-daemon did not bind"
            );
            thread::sleep(Duration::from_millis(5));
        }
        bus
    }
    fn stop(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            self.child.kill().unwrap();
        }
        self.child.wait().unwrap();
    }
}
impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}
struct FakeService {
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}
impl FakeService {
    fn start(address: &str, state: Shared) -> Self {
        let address = address.to_owned();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let thread = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let held_namespace = state.lock().unwrap().held_namespace.clone();
                let builder = zbus::connection::Builder::address(address.as_str())
                    .unwrap()
                    .name(SERVICE)
                    .unwrap()
                    .serve_at(ROOT, Service(state.clone()))
                    .unwrap()
                    .serve_at(COLLECTION, Collection(state.clone()))
                    .unwrap()
                    .serve_at(
                        item_path(0),
                        Item {
                            state: state.clone(),
                            index: 0,
                        },
                    )
                    .unwrap()
                    .serve_at(
                        item_path(1),
                        Item {
                            state: state.clone(),
                            index: 1,
                        },
                    )
                    .unwrap()
                    .serve_at(PROMPT, Prompt(state))
                    .unwrap()
                    .serve_at(SESSION, Session)
                    .unwrap();
                let builder = match held_namespace {
                    Some(namespace) => builder.name(namespace).unwrap(),
                    None => builder,
                };
                let connection = builder
                    .build()
                    .await
                    .expect("connect fake service to its explicit private bus");
                ready_tx.send(()).unwrap();
                let _ = stopped.await;
                let _ = connection.close().await;
            });
        });
        let service = Self {
            stop: Some(stop),
            thread: Some(thread),
        };
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("fake Secret Service startup");
        service
    }
}
impl Drop for FakeService {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let result = thread.join();
            if !thread::panicking() {
                result.unwrap();
            }
        }
    }
}
struct Fixture {
    service: Option<FakeService>,
    bus: PrivateBus,
    state: Shared,
    owner: NodeId,
}
impl Fixture {
    fn new() -> Self {
        let bus = PrivateBus::start();
        let state = Arc::new(Mutex::new(State::default()));
        let service = Some(FakeService::start(&bus.address, state.clone()));
        Self {
            service,
            bus,
            state,
            owner: NodeId::new(),
        }
    }
    fn backend(&self) -> SecretServiceBackend {
        self.backend_with_timeout(OPERATION_TIMEOUT).unwrap()
    }
    fn backend_with_timeout(
        &self,
        timeout: Duration,
    ) -> Result<SecretServiceBackend, BackendError> {
        SecretServiceBackend::connect_unix(&self.bus.address, COLLECTION, self.owner, timeout)
    }
    fn seed(&self, handle: &CredentialHandle, bytes: &[u8]) {
        self.state
            .lock()
            .unwrap()
            .items
            .push(StoredItem::new(self.owner, handle, bytes));
    }
    fn restart_service(&mut self) {
        drop(self.service.take());
        self.service = Some(FakeService::start(&self.bus.address, self.state.clone()));
    }
    fn assert_policy(&self) {
        let state = self.state.lock().unwrap();
        assert_eq!(state.invalid_wire, 0, "invalid Secret Service wire request");
        assert_eq!(
            state.create_collection_calls, 0,
            "must not create collections"
        );
        assert_eq!(state.unlock_calls, 0, "must not unlock anything");
        assert_eq!(state.prompt_calls, 0, "must not invoke any Prompt");
        assert_eq!(state.delete_calls, 0, "must not delete anything");
        assert_eq!(state.set_secret_calls, 0, "must not replace secret bytes");
        assert!(state.replace_flags.iter().all(|replace| !replace));
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        drop(self.service.take());
        if !thread::panicking() {
            self.assert_policy();
        }
    }
}

#[test]
fn ipc_secret_service_create_exact_replay_and_restart_readback() {
    let mut fixture = Fixture::new();
    let (bytes, handle) = fixture_credential(1);
    let backend = fixture.backend();
    backend.put_exact(fixture.owner, &handle, &bytes).unwrap();
    assert_eq!(
        backend.load(fixture.owner, &handle).unwrap().as_slice(),
        bytes
    );
    backend.put_exact(fixture.owner, &handle, &bytes).unwrap();
    drop(backend);
    fixture.restart_service();
    let backend = fixture.backend();
    assert_eq!(
        backend.load(fixture.owner, &handle).unwrap().as_slice(),
        bytes
    );
    backend.put_exact(fixture.owner, &handle, &bytes).unwrap();
    let state = fixture.state.lock().unwrap();
    assert_eq!(
        state.create_calls, 1,
        "replay must never replace or recreate"
    );
    assert_eq!(
        state.created_attributes,
        vec![attributes(fixture.owner, &handle)]
    );
    assert_eq!(state.created_content_types, vec![CONTENT_TYPE]);
    assert!(
        state
            .searches
            .iter()
            .all(|found| *found == attributes(fixture.owner, &handle))
    );
    assert!(state.open_sessions >= 1);
    assert!(state.get_secrets >= 3);
}

#[test]
fn ipc_secret_service_missing_item_and_collection_never_provision() {
    let fixture = Fixture::new();
    let (_, handle) = fixture_credential(2);
    let backend = fixture.backend();
    assert!(matches!(
        backend.load(fixture.owner, &handle),
        Err(BackendError::Missing)
    ));
    assert!(
        SecretServiceBackend::connect_unix(
            &fixture.bus.address,
            "/org/freedesktop/secrets/collection/absent",
            fixture.owner,
            OPERATION_TIMEOUT,
        )
        .is_err()
    );
    assert_eq!(fixture.state.lock().unwrap().create_calls, 0);
}

#[test]
fn ipc_secret_service_locked_collection_and_item_never_unlock() {
    let fixture = Fixture::new();
    let (bytes, handle) = fixture_credential(3);
    fixture.state.lock().unwrap().collection_locked = true;
    assert!(matches!(
        fixture.backend_with_timeout(OPERATION_TIMEOUT),
        Err(BackendError::Locked)
    ));
    fixture.state.lock().unwrap().collection_locked = false;
    fixture.seed(&handle, &bytes);
    fixture.state.lock().unwrap().items[0].locked = true;
    let backend = fixture.backend();
    assert!(matches!(
        backend.load(fixture.owner, &handle),
        Err(BackendError::Locked)
    ));
    assert_eq!(
        backend.put_exact(fixture.owner, &handle, &bytes),
        Err(BackendError::Locked)
    );
    assert_eq!(fixture.state.lock().unwrap().create_calls, 0);
}

#[test]
fn ipc_secret_service_duplicates_are_conflicts_not_replaced() {
    let fixture = Fixture::new();
    let (bytes, handle) = fixture_credential(4);
    fixture.seed(&handle, &bytes);
    fixture.seed(&handle, &bytes);
    let backend = fixture.backend();
    assert!(matches!(
        backend.load(fixture.owner, &handle),
        Err(BackendError::Conflict)
    ));
    assert_eq!(
        backend.put_exact(fixture.owner, &handle, &bytes),
        Err(BackendError::Conflict)
    );
    let state = fixture.state.lock().unwrap();
    assert_eq!(state.create_calls, 0);
    assert_eq!(
        state.get_secrets, 0,
        "duplicate results must fail before secret retrieval"
    );
}

#[test]
fn ipc_secret_service_wrong_or_extra_item_attributes_are_rejected() {
    let fixture = Fixture::new();
    let (bytes, handle) = fixture_credential(5);
    fixture.seed(&handle, &bytes);
    fixture.state.lock().unwrap().search_override = Some(vec![item_path(0)]);
    let backend = fixture.backend();
    for key in [
        "application",
        "schema",
        "local-node-id",
        "credential-handle",
        "unexpected",
    ] {
        {
            let mut state = fixture.state.lock().unwrap();
            state.items[0].attributes = attributes(fixture.owner, &handle);
            state.items[0]
                .attributes
                .insert(key.into(), "not-the-requested-value".into());
        }
        assert!(
            matches!(
                backend.load(fixture.owner, &handle),
                Err(BackendError::Conflict)
            ),
            "accepted incorrect {key}"
        );
        assert_eq!(
            backend.put_exact(fixture.owner, &handle, &bytes),
            Err(BackendError::Conflict)
        );
    }
    let state = fixture.state.lock().unwrap();
    assert_eq!(state.create_calls, 0);
    assert_eq!(
        state.get_secrets, 0,
        "check namespace before reading secret"
    );
}

#[test]
fn ipc_secret_service_wrong_digest_content_type_session_and_parameters_fail_closed() {
    let fixture = Fixture::new();
    let (bytes, handle) = fixture_credential(6);
    fixture.seed(&handle, &bytes);
    let backend = fixture.backend();
    for case in 0..4 {
        {
            let mut state = fixture.state.lock().unwrap();
            state.items[0] = StoredItem::new(fixture.owner, &handle, &bytes);
            match case {
                0 => state.items[0].bytes = fixture_credential(7).0,
                1 => state.items[0].content_type = "text/plain".into(),
                2 => state.items[0].session = path("/org/freedesktop/secrets/session/other"),
                3 => state.items[0].parameters = vec![1],
                _ => unreachable!(),
            }
        }
        let expected = if case == 0 {
            BackendError::Conflict
        } else {
            BackendError::Invalid
        };
        assert!(
            matches!(backend.load(fixture.owner, &handle), Err(error) if error == expected),
            "accepted invalid secret case {case}"
        );
        assert_eq!(
            backend.put_exact(fixture.owner, &handle, &bytes),
            Err(expected)
        );
    }
    assert_eq!(fixture.state.lock().unwrap().create_calls, 0);
}

#[test]
fn ipc_secret_service_foreign_owner_and_wrong_put_digest_never_create() {
    let fixture = Fixture::new();
    let (bytes, handle) = fixture_credential(8);
    let backend = fixture.backend();
    assert!(backend.load(NodeId::new(), &handle).is_err());
    assert!(backend.put_exact(NodeId::new(), &handle, &bytes).is_err());
    assert!(
        backend
            .put_exact(fixture.owner, &handle, &fixture_credential(9).0)
            .is_err()
    );
    assert_eq!(fixture.state.lock().unwrap().create_calls, 0);
}

#[test]
fn ipc_secret_service_returned_prompt_is_never_invoked() {
    let fixture = Fixture::new();
    let (bytes, handle) = fixture_credential(10);
    fixture.state.lock().unwrap().create_reply = CreateReply::Prompt;
    let backend = fixture.backend();
    assert!(backend.put_exact(fixture.owner, &handle, &bytes).is_err());
    let state = fixture.state.lock().unwrap();
    assert_eq!(state.create_calls, 1);
    assert_eq!(state.prompt_calls, 0);
}

#[test]
fn ipc_secret_service_whole_operation_timeout_and_bus_disconnect_are_bounded() {
    let mut fixture = Fixture::new();
    let (_, handle) = fixture_credential(11);
    let backend = fixture
        .backend_with_timeout(Duration::from_millis(120))
        .unwrap();
    fixture.state.lock().unwrap().search_delay = Duration::from_secs(2);
    let start = Instant::now();
    assert!(matches!(
        backend.load(fixture.owner, &handle),
        Err(BackendError::Unavailable)
    ));
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "operation escaped its whole deadline"
    );
    fixture.bus.stop();
    let start = Instant::now();
    assert!(matches!(
        backend.load(fixture.owner, &handle),
        Err(BackendError::Unavailable)
    ));
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "bus loss exceeded the bound"
    );
    assert_eq!(fixture.state.lock().unwrap().create_calls, 0);
}

#[test]
fn ipc_secret_service_uncertain_create_is_not_retried_or_rolled_back() {
    let fixture = Fixture::new();
    let (bytes, handle) = fixture_credential(12);
    fixture.state.lock().unwrap().create_reply = CreateReply::DelayAfterPersist;
    let backend = fixture
        .backend_with_timeout(Duration::from_millis(120))
        .unwrap();
    let start = Instant::now();
    assert!(matches!(
        backend.put_exact(fixture.owner, &handle, &bytes),
        Err(BackendError::Unavailable)
    ));
    assert!(start.elapsed() < Duration::from_secs(1));
    drop(backend);
    {
        let mut state = fixture.state.lock().unwrap();
        assert_eq!(state.create_calls, 1);
        assert_eq!(
            state.items.len(),
            1,
            "fake committed before withholding its response"
        );
        state.create_reply = CreateReply::Normal;
    }
    let reopened = fixture.backend();
    assert_eq!(
        reopened.load(fixture.owner, &handle).unwrap().as_slice(),
        bytes
    );
    reopened.put_exact(fixture.owner, &handle, &bytes).unwrap();
    assert_eq!(fixture.state.lock().unwrap().create_calls, 1);
}

#[test]
fn ipc_secret_service_create_error_after_persist_requires_explicit_readback() {
    let fixture = Fixture::new();
    let (bytes, handle) = fixture_credential(13);
    fixture.state.lock().unwrap().create_reply = CreateReply::ErrorAfterPersist;
    let backend = fixture.backend();
    assert!(backend.put_exact(fixture.owner, &handle, &bytes).is_err());
    assert_eq!(fixture.state.lock().unwrap().create_calls, 1);
    assert_eq!(
        backend.load(fixture.owner, &handle).unwrap().as_slice(),
        bytes
    );
    backend.put_exact(fixture.owner, &handle, &bytes).unwrap();
    assert_eq!(fixture.state.lock().unwrap().create_calls, 1);
}

#[test]
fn ipc_secret_service_cooperative_namespace_conflict_fails_before_access() {
    let mut fixture = Fixture::new();
    let (bytes, handle) = fixture_credential(14);
    let backend = fixture.backend();
    fixture.state.lock().unwrap().held_namespace = Some(format!(
        "org.capyio.Pairing.n{}",
        fixture.owner.as_uuid().simple()
    ));
    fixture.restart_service();
    assert!(matches!(
        fixture.backend_with_timeout(OPERATION_TIMEOUT),
        Err(BackendError::Conflict)
    ));
    assert!(matches!(
        backend.load(fixture.owner, &handle),
        Err(BackendError::Conflict)
    ));
    assert_eq!(
        backend.put_exact(fixture.owner, &handle, &bytes),
        Err(BackendError::Conflict)
    );
    let state = fixture.state.lock().unwrap();
    assert_eq!(state.open_sessions, 0);
    assert_eq!(state.create_calls, 0);
    assert!(state.searches.is_empty());
}

#[test]
fn ipc_secret_service_oversized_received_secret_is_invalid() {
    let fixture = Fixture::new();
    let (bytes, handle) = fixture_credential(15);
    fixture.seed(&handle, &bytes);
    fixture.state.lock().unwrap().items[0].bytes = vec![0; MAX_CONTROL_CREDENTIAL_BUNDLE_BYTES + 1];
    let backend = fixture.backend();
    assert!(matches!(
        backend.load(fixture.owner, &handle),
        Err(BackendError::Invalid)
    ));
    assert_eq!(
        backend.put_exact(fixture.owner, &handle, &bytes),
        Err(BackendError::Invalid)
    );
    assert_eq!(fixture.state.lock().unwrap().create_calls, 0);
}

#[test]
fn ipc_secret_service_timed_out_create_may_commit_later_without_retry() {
    let fixture = Fixture::new();
    let (bytes, handle) = fixture_credential(16);
    fixture.state.lock().unwrap().create_reply = CreateReply::DelayBeforePersist;
    let backend = fixture
        .backend_with_timeout(Duration::from_millis(120))
        .unwrap();
    let start = Instant::now();
    assert!(matches!(
        backend.put_exact(fixture.owner, &handle, &bytes),
        Err(BackendError::Unavailable)
    ));
    assert!(start.elapsed() < Duration::from_secs(1));
    {
        let mut state = fixture.state.lock().unwrap();
        assert_eq!(state.create_calls, 1);
        assert!(
            state.items.is_empty(),
            "the first request has not committed yet"
        );
        // Absence after timeout is NOT authorization to issue another create.
        state.allow_delayed_commit = true;
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while fixture.state.lock().unwrap().items.is_empty() {
        assert!(
            Instant::now() < deadline,
            "delayed fake create did not commit"
        );
        thread::sleep(Duration::from_millis(5));
    }
    // Explicit read-only reconciliation observes the original request's late
    // mutation. There is no automatic retry, replacement, or delete rollback.
    let reopened = fixture.backend();
    assert_eq!(
        reopened.load(fixture.owner, &handle).unwrap().as_slice(),
        bytes
    );
    let state = fixture.state.lock().unwrap();
    assert_eq!(state.create_calls, 1);
    assert_eq!(state.items.len(), 1);
}

#[test]
fn secret_service_rejects_nonlocal_or_ambiguous_addresses_before_connecting() {
    let owner = NodeId::new();
    for address in [
        "tcp:host=127.0.0.1,port=1",
        "unix:abstract=capyio_fixture",
        "autolaunch:",
        "unix:path=relative",
        "unix:path=/tmp/capyio_fixture;unix:path=/tmp/other_fixture",
        "unix:path=/tmp/capyio%2ffixture",
        "unix:path=/tmp/capyio fixture",
        "unix:path=/tmp/capyio_fixture,guid=invalid",
        "unix:path=/tmp/capyio_fixture,other=unsupported",
        "unix:path=/tmp/capyio_fixture,guid=0123456789abcdef0123456789abcdef,extra=x",
    ] {
        assert!(
            matches!(
                SecretServiceBackend::connect_unix(address, COLLECTION, owner, OPERATION_TIMEOUT),
                Err(BackendError::Invalid)
            ),
            "accepted invalid explicit bus address {address}"
        );
    }
    // Parameter validation must not attempt the deliberately nonexistent bus.
    let address = "unix:path=/tmp/capyio_nonexistent_parameter_fixture";
    for collection in [
        "/org/freedesktop/secrets/collection/session",
        "/org/freedesktop/secrets/aliases/default",
        "/org/freedesktop/secrets/collection/not-valid",
        "/",
    ] {
        assert!(matches!(
            SecretServiceBackend::connect_unix(address, collection, owner, OPERATION_TIMEOUT),
            Err(BackendError::Invalid)
        ));
    }
    for timeout in [Duration::ZERO, Duration::from_secs(11)] {
        assert!(matches!(
            SecretServiceBackend::connect_unix(address, COLLECTION, owner, timeout),
            Err(BackendError::Invalid)
        ));
    }
    let nil_owner = "00000000-0000-0000-0000-000000000000".parse().unwrap();
    assert!(matches!(
        SecretServiceBackend::connect_unix(address, COLLECTION, nil_owner, OPERATION_TIMEOUT),
        Err(BackendError::Invalid)
    ));
}

#[test]
fn ipc_secret_service_absent_service_fails_closed() {
    let bus = PrivateBus::start();
    // This private bus has no service directories and no Secret Service owner.
    // No real desktop session, activation, or user keyring is available here.
    let start = Instant::now();
    assert!(matches!(
        SecretServiceBackend::connect_unix(
            &bus.address,
            COLLECTION,
            NodeId::new(),
            Duration::from_millis(120),
        ),
        Err(BackendError::Unavailable)
    ));
    assert!(start.elapsed() < Duration::from_secs(1));
}
