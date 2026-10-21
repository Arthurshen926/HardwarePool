//! Writable isolated Secret Service for first enrollment. Never opens a user bus.
// Integration targets deliberately use different subsets of this support API.
#![allow(dead_code)]
use capyio_control_credentials::ControlCredentialBundle;
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
    pub(crate) creates: usize,
    pub(crate) fail_after_create: bool,
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
        mut properties: HashMap<String, OwnedValue>,
        secret: (OwnedObjectPath, Vec<u8>, Vec<u8>, String),
        replace: bool,
    ) -> zbus::fdo::Result<(OwnedObjectPath, OwnedObjectPath)> {
        let mut s = self.0.lock().unwrap();
        let attributes = properties
            .remove("org.freedesktop.Secret.Item.Attributes")
            .and_then(|value| Attributes::try_from(value).ok());
        let label = properties.remove("org.freedesktop.Secret.Item.Label");
        if replace
            || !s.missing
            || s.locked
            || attributes.is_none()
            || label
                .as_ref()
                .and_then(|value| <&str>::try_from(value).ok())
                != Some("CapyIO control credential")
            || !properties.is_empty()
            || secret.0.as_str() != SESSION
            || !secret.1.is_empty()
            || secret.3 != "application/x-capyio-control-credential"
            || ControlCredentialBundle::decode(&secret.2).is_err()
        {
            s.forbidden += 1;
            return Err(zbus::fdo::Error::AccessDenied(
                "invalid synthetic create".into(),
            ));
        }
        s.attributes = attributes.unwrap();
        s.bytes = secret.2;
        s.missing = false;
        s.creates += 1;
        if s.fail_after_create {
            return Err(zbus::fdo::Error::Failed(
                "synthetic uncertain create".into(),
            ));
        }
        Ok((path(ITEM), path("/")))
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
            "capyio-node-enrollment-{}-{}",
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
