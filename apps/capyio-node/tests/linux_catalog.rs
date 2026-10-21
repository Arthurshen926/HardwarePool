#![cfg(all(target_os = "linux", feature = "paired-store"))]
//! Actual Node and console boundaries, synthetic pinned mTLS and a private fake
//! Secret Service only. No user bus, persistent identity, resource or device is used.
use capyio_control_credentials::ControlCredentialBundle;
use capyio_control_tls::{
    AuthenticatedControlServer, AuthenticatedControlServerChannel, ControlTlsError,
    ControlTlsNodeBinding, ControlTlsServerIdentity,
};
use capyio_core::*;
use capyio_pairing::{
    CertificateFingerprint, CredentialHandle, PairedPeerDirectory, PairedPeerRecord,
};
use capyio_protocol::{
    PortLeaseAction, RouteCommandAction, decode_catalog_snapshot, encode_catalog_snapshot,
    encode_control_heartbeat, encode_envelope_checked, encode_port_lease_request,
    encode_route_command, new_envelope, v1,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener},
    os::{
        fd::{AsFd, AsRawFd, FromRawFd},
        unix::{
            fs::{DirBuilderExt, PermissionsExt, symlink},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};
#[path = "support/paired_store.rs"]
mod paired_store;
use paired_store::PersistedPairing;

const BOUND: Duration = Duration::from_secs(14);
const OMITTED: &str = "PRIVATE-CATALOG-METADATA-MUST-NOT-LEAK";
const HOSTILE: &str = "x\u{1b}]52;c;injected\u{7}\n\r\t\\\u{202e}é";
const SERVER_NAME: &str = "catalog.synthetic.test";

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
        assert!(output.len() <= 65_536, "bounded public output");
    }
}
fn no_children(pid: u32) {
    let children = fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")).unwrap();
    assert!(
        children.trim().is_empty(),
        "catalog inspection spawned a resource helper"
    );
}
fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fs::read_dir(root)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (path.file_name().unwrap().into(), fs::read(path).unwrap())
        })
        .collect()
}

struct Pty {
    master: File,
    slave: File,
    original: libc::termios,
    bytes: Vec<u8>,
    node_pid: Option<u32>,
}
fn attributes(file: &File) -> libc::termios {
    // SAFETY: tcgetattr initializes the value on success and borrows a live FD.
    unsafe {
        let mut value = std::mem::zeroed();
        assert_eq!(libc::tcgetattr(file.as_raw_fd(), &mut value), 0);
        value
    }
}
impl Pty {
    fn new() -> Self {
        let (mut master, mut slave) = (-1, -1);
        // SAFETY: openpty creates two fresh owned descriptors, not a user TTY.
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            },
            0
        );
        // SAFETY: each successfully created descriptor is transferred once.
        let (master, slave) = unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) };
        for fd in [&master, &slave] {
            // SAFETY: only descriptor flags on these fresh owned FDs are changed.
            assert_eq!(
                unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) },
                0
            );
        }
        nonblocking(&master);
        let original = attributes(&slave);
        Self {
            master,
            slave,
            original,
            bytes: vec![],
            node_pid: None,
        }
    }
    fn collect(&mut self) {
        let mut buffer = [0; 4096];
        loop {
            match self.master.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => self.bytes.extend_from_slice(&buffer[..n]),
                Err(e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.raw_os_error() == Some(libc::EIO) =>
                {
                    break;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => panic!("private terminal read: {e}"),
            }
            assert!(
                self.bytes.len() <= 65_536,
                "bounded private terminal output"
            );
        }
    }
    fn assert_restored(&self) {
        let after = attributes(&self.slave);
        assert_eq!(after.c_lflag, self.original.c_lflag);
        assert_eq!(after.c_iflag, self.original.c_iflag);
        assert_eq!(after.c_oflag, self.original.c_oflag);
        assert_eq!(after.c_cc, self.original.c_cc);
        if let Some(pid) = self.node_pid {
            assert!(
                !Path::new(&format!("/proc/{pid}")).exists(),
                "console must reap actual Node"
            );
        }
    }
}
struct Process {
    child: Child,
    out: Vec<u8>,
    err: Vec<u8>,
    pty: Option<Pty>,
    started: Instant,
}
impl Process {
    fn node(args: &[String]) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_capyio-node"));
        command.args(args).stdin(Stdio::piped());
        Self::spawn(command, None)
    }
    fn console(pairing: &PersistedPairing, endpoint: SocketAddr) -> Self {
        Self::console_command(pairing, endpoint, "catalog", &[])
    }
    fn prepare_camera(pairing: &PersistedPairing, endpoint: SocketAddr, output: &Path) -> Self {
        Self::console_command(
            pairing,
            endpoint,
            "prepare-camera",
            &[
                "--output".into(),
                output.to_str().unwrap().into(),
                "--listen".into(),
                "127.0.0.1:43197".into(),
                "--duration-secs".into(),
                "7".into(),
            ],
        )
    }
    fn console_command(
        pairing: &PersistedPairing,
        endpoint: SocketAddr,
        action: &str,
        extra: &[String],
    ) -> Self {
        let node = Path::new(env!("CARGO_BIN_EXE_capyio-node"))
            .canonicalize()
            .unwrap();
        let console = node.parent().unwrap().join("capyio-linux-console");
        assert!(
            console.is_file(),
            "mandatory gate must build actual paired console sibling"
        );
        let pty = Pty::new();
        let slave = pty.slave.as_raw_fd();
        let parent = std::process::id() as libc::pid_t;
        let mut command = Command::new(console);
        command
            .args([
                "--pairing-node",
                &pairing.owner.to_string(),
                "--pairing-state",
                pairing.state_path.to_str().unwrap(),
                "--pairing-bus-address",
                &pairing.service.address,
                "--pairing-collection",
                paired_store::COLLECTION,
                action,
                "--peer",
                &pairing.peer.to_string(),
                "--endpoint",
                &endpoint.to_string(),
            ])
            .args(extra)
            .stdin(Stdio::null());
        // SAFETY: only async-signal-safe operations after fork; the owned slave
        // remains live, and this child receives a new private session.
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) < 0
                    || libc::getppid() != parent
                    || libc::setsid() < 0
                    || libc::ioctl(slave, libc::TIOCSCTTY, 0) < 0
                {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Self::spawn(command, Some(pty))
    }
    fn spawn(mut command: Command, pty: Option<Pty>) -> Self {
        let started = Instant::now();
        let child = command
            .env_clear()
            .env("RUST_BACKTRACE", "0")
            .env("RUST_LIB_BACKTRACE", "0")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(stdin) = &child.stdin {
            nonblocking(stdin);
        }
        nonblocking(child.stdout.as_ref().unwrap());
        nonblocking(child.stderr.as_ref().unwrap());
        Self {
            child,
            out: vec![],
            err: vec![],
            pty,
            started,
        }
    }
    fn frames(&mut self, values: &[Vec<u8>]) {
        let mut bytes = Vec::new();
        for value in values {
            bytes.extend_from_slice(&(value.len() as u32).to_be_bytes());
            bytes.extend_from_slice(value);
        }
        assert!(bytes.len() <= 4096, "atomic PIPE_BUF fixture bootstrap");
        let mut offset = 0;
        while offset < bytes.len() {
            assert!(self.started.elapsed() < BOUND, "bounded bootstrap write");
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
    fn collect(&mut self) {
        drain(self.child.stdout.as_mut().unwrap(), &mut self.out);
        drain(self.child.stderr.as_mut().unwrap(), &mut self.err);
        if let Some(pty) = &mut self.pty {
            pty.collect();
            assert!(
                self.out.is_empty(),
                "console output belongs only on private TTY"
            );
        }
    }
    fn identify_node(&mut self, endpoint: SocketAddr, pairing: &PersistedPairing) -> u32 {
        let pid = if let Some(pty) = &mut self.pty {
            let children = fs::read_to_string(format!(
                "/proc/{}/task/{}/children",
                self.child.id(),
                self.child.id()
            ))
            .unwrap();
            let ids: Vec<u32> = children
                .split_whitespace()
                .map(|s| s.parse().unwrap())
                .collect();
            assert_eq!(
                ids.len(),
                1,
                "one real sibling Node required, no mock wrapper"
            );
            let pid = ids[0];
            assert_eq!(
                fs::read_link(format!("/proc/{pid}/exe")).unwrap(),
                Path::new(env!("CARGO_BIN_EXE_capyio-node"))
                    .canonicalize()
                    .unwrap()
            );
            pty.node_pid = Some(pid);
            assert_eq!(
                attributes(&pty.slave).c_lflag & (libc::ECHO | libc::ECHONL),
                0
            );
            pid
        } else {
            self.child.id()
        };
        let args = fs::read(format!("/proc/{pid}/cmdline")).unwrap();
        assert!(args.split(|b| *b == 0).any(|s| s == b"linux-catalog"));
        for file in ["cmdline", "environ"] {
            let bytes = fs::read(format!("/proc/{pid}/{file}")).unwrap();
            pairing.assert_no_leak(&bytes);
            assert!(
                !String::from_utf8_lossy(&bytes).contains(&endpoint.to_string()),
                "private Node endpoint must not become an argument or environment entry"
            );
        }
        no_children(pid);
        pid
    }
    fn finish(&mut self) -> ExitStatus {
        loop {
            self.collect();
            if let Some(status) = self.child.try_wait().unwrap() {
                self.collect();
                if let Some(pty) = &self.pty {
                    pty.assert_restored();
                }
                return status;
            }
            assert!(
                self.started.elapsed() < BOUND,
                "absolute bounded executable exit; stderr={}",
                String::from_utf8_lossy(&self.err)
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn events(&self) -> Vec<Value> {
        assert!(self.pty.is_none());
        let mut bytes = self.out.as_slice();
        let mut events = Vec::new();
        while !bytes.is_empty() {
            assert!(bytes.len() >= 4, "complete framed output");
            let n = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
            assert!((1..=4096).contains(&n));
            assert!(bytes.len() >= n + 4);
            let event: Value = serde_json::from_slice(&bytes[4..4 + n]).unwrap();
            assert_eq!(event["version"], 1);
            events.push(event);
            bytes = &bytes[n + 4..];
        }
        events
    }
    fn failed(&mut self) {
        self.failed_with_optional_event(false);
    }
    fn failed_with_optional_event(&mut self, deadline_may_have_expired: bool) {
        assert!(!self.finish().success());
        let events = self.events();
        if !deadline_may_have_expired || !events.is_empty() {
            assert_eq!(
                events,
                [json!({"version":1,"event":"failed"})],
                "no partial catalog may escape on failure"
            );
        }
        assert_eq!(
            String::from_utf8_lossy(&self.err).trim(),
            "Error: Linux catalog inspection failed",
            "only fixed redacted runtime error text"
        );
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if self.pty.is_some() {
            // SAFETY: only the new private child session owned by this fixture.
            unsafe {
                libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn identities() -> (
    ControlCredentialBundle,
    ControlTlsServerIdentity,
    CertificateFingerprint,
) {
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
    let leaf = |name: &str, usage| {
        let mut params = CertificateParams::new(vec![name.into()]).unwrap();
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![usage];
        let key = KeyPair::generate().unwrap();
        (
            params.signed_by(&key, &issuer).unwrap().der().to_vec(),
            key.serialize_der(),
        )
    };
    let (server, server_key) = leaf(SERVER_NAME, ExtendedKeyUsagePurpose::ServerAuth);
    let (client, client_key) = leaf(
        "synthetic-catalog-client",
        ExtendedKeyUsagePurpose::ClientAuth,
    );
    let server_pin = CertificateFingerprint::from_certificate_der(&server).unwrap();
    let client_pin = CertificateFingerprint::from_certificate_der(&client).unwrap();
    (
        ControlCredentialBundle::new(vec![client, ca.clone()], client_key, ca.clone()).unwrap(),
        ControlTlsServerIdentity {
            expected_peer_certificate: client_pin,
            certificate_chain_der: vec![server, ca.clone()],
            private_key_der: server_key,
            client_ca_der: ca,
        },
        server_pin,
    )
}
fn catalog(count: usize) -> NodeDescriptor {
    let mut node = NodeDescriptor::new(
        NodeId::new(),
        OMITTED,
        Platform::Linux,
        OMITTED,
        "0.1.0",
        [ProtocolVersion::new(1, 0)],
    );
    let adapter = AdapterInstanceId::new();
    node.add_adapter(AdapterInstanceDescriptor {
        id: adapter,
        adapter_type: "synthetic.catalog".into(),
        display_name: OMITTED.into(),
        deployment_mode: AdapterDeploymentMode::Sidecar,
        version: "0.1.0".into(),
        state: AdapterState::Ready,
        health: AdapterHealth::Healthy,
        owned_capabilities: BTreeSet::new(),
        supported_route_modes: BTreeSet::from([
            RouteBackend::CapyDataPlane,
            RouteBackend::AdapterManaged,
        ]),
    })
    .unwrap();
    for index in 0..count {
        let cap = CapabilityId::new();
        let port_id = PortId::new();
        let direction = [
            PortDirection::Source,
            PortDirection::Sink,
            PortDirection::Control,
        ][index % 3];
        let port = PortDescriptor {
            id: port_id,
            capability_id: cap,
            display_name: OMITTED.into(),
            direction,
            profile: if direction == PortDirection::Control {
                ProfileId::new("capyio.control.commands", 1)
            } else {
                ProfileId::audio_frames_v1()
            },
            schema_id: None,
            formats: vec![
                FormatDescriptor::new(if direction == PortDirection::Source {
                    "pcm-s16le-48000-mono"
                } else {
                    "pcm-s16le-48000-stereo"
                }),
                FormatDescriptor {
                    id: "pcm-s16le".into(),
                    parameters: BTreeMap::from([
                        ("channels".into(), "2".into()),
                        ("sample_rate".into(), "48000".into()),
                    ]),
                },
            ],
            qos_modes: BTreeSet::from([QosMode::Interactive]),
            clock_domain: None,
            availability: [
                Availability::Available,
                Availability::Busy,
                Availability::PermissionRequired,
            ][index % 3],
            permission_requirement: PermissionRequirement::UserConfirmation,
            interoperability_mode: InteroperabilityMode::AdapterManaged,
        };
        node.add_capability(CapabilityDescriptor {
            id: cap,
            adapter_instance_id: adapter,
            display_name: OMITTED.into(),
            class: CapabilityClass::Custom("synthetic".into()),
            availability: Availability::Available,
            permission_requirement: PermissionRequirement::UserConfirmation,
            metadata: BTreeMap::from([("private".into(), OMITTED.into())]),
            ports: BTreeMap::from([(port_id, port)]),
        })
        .unwrap();
    }
    node.validate().unwrap();
    node
}
struct Fixture {
    pairing: PersistedPairing,
    server: AuthenticatedControlServer,
    endpoint: SocketAddr,
    node: NodeDescriptor,
    original: BTreeMap<PathBuf, Vec<u8>>,
    server_key: Vec<u8>,
}
impl Fixture {
    fn new(count: usize) -> Self {
        Self::with_fault(count, "none")
    }
    fn with_fault(count: usize, fault: &str) -> Self {
        let local = NodeId::new();
        let node = catalog(count);
        let (bundle, identity, mut pin) = identities();
        if fault == "pin" {
            pin = CertificateFingerprint::from_certificate_der(&identity.client_ca_der).unwrap();
        }
        let mut directory = PairedPeerDirectory::new(local).unwrap();
        directory
            .add(
                PairedPeerRecord::new(
                    node.id,
                    "Synthetic catalog peer",
                    SERVER_NAME,
                    CredentialHandle::new("synthetic.catalog.only").unwrap(),
                    pin,
                    1,
                )
                .unwrap(),
            )
            .unwrap();
        let pairing = PersistedPairing::new(local, &directory.encode_json().unwrap(), &bundle);
        let original = files(&pairing.state_path);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap();
        let binding = ControlTlsNodeBinding::new(
            if fault == "binding" {
                NodeId::new()
            } else {
                node.id
            },
            local,
        )
        .unwrap();
        let server_key = identity.private_key_der.clone();
        let server = AuthenticatedControlServer::from_listener_fresh_session(
            listener, identity, binding, BOUND,
        )
        .unwrap();
        Self {
            pairing,
            server,
            endpoint,
            node,
            original,
            server_key,
        }
    }
    fn config(&self) -> Value {
        json!({"version":1,"local_node_id":self.pairing.owner,"peer_node_id":self.pairing.peer,"endpoint":self.endpoint.to_string()})
    }
    fn start(&self) -> Process {
        let mut process = Process::node(&self.pairing.args("linux-catalog"));
        process.frames(&[serde_json::to_vec(&self.config()).unwrap(), marker()]);
        process
    }
    fn accept_result(
        &self,
        process: &mut Process,
    ) -> Result<AuthenticatedControlServerChannel, ControlTlsError> {
        let deadline = process.started + BOUND;
        loop {
            process.collect();
            match self.server.try_accept_until(deadline)? {
                Some(channel) => return Ok(channel),
                None => {
                    assert!(
                        process.child.try_wait().unwrap().is_none(),
                        "Node exited before dial"
                    );
                    thread::sleep(Duration::from_millis(2));
                }
            }
        }
    }
    fn accept_empty(&self, process: &mut Process) -> AuthenticatedControlServerChannel {
        let mut channel = self.accept_result(process).unwrap();
        process.identify_node(self.endpoint, &self.pairing);
        let bytes = channel
            .receive_envelope_until(process.started + BOUND)
            .unwrap();
        let snapshot = decode_catalog_snapshot(&bytes).unwrap();
        assert_eq!(snapshot.session_id, channel.binding().session_id);
        assert_eq!(snapshot.node.id, self.pairing.owner);
        assert!(
            snapshot.node.capabilities.is_empty(),
            "inspection never publishes local resources"
        );
        assert!(
            snapshot.node.adapter_instances.is_empty(),
            "inspection never constructs local adapters"
        );
        channel
    }
    fn send(&self, channel: &mut AuthenticatedControlServerChannel, process: &Process) {
        channel
            .send_envelope_until(
                &encode_catalog_snapshot(channel.binding().session_id, &self.node).unwrap(),
                process.started + BOUND,
            )
            .unwrap();
    }
    fn unchanged(&self, process: &Process) {
        assert_eq!(
            files(&self.pairing.state_path),
            self.original,
            "read-only inspection changed persisted pairing state"
        );
        let state = self.pairing.service.state.lock().unwrap();
        assert_eq!(state.forbidden, 0);
        assert_eq!(
            state.bytes, self.pairing.bundle,
            "protected identity changed"
        );
        drop(state);
        let mut outputs = vec![(&process.out, false), (&process.err, false)];
        if let Some(pty) = &process.pty {
            outputs.push((&pty.bytes, true));
        }
        for (output, terminal) in outputs {
            self.pairing.assert_no_leak(output);
            assert!(
                !output
                    .windows(self.server_key.len())
                    .any(|w| w == self.server_key)
            );
            let text = String::from_utf8_lossy(output);
            assert!(
                !text.contains(OMITTED),
                "unselected peer names or metadata leaked"
            );
            if !terminal {
                assert!(
                    !text.contains(&self.endpoint.to_string()),
                    "private Node endpoint leaked to public output"
                );
            }
            assert!(
                !text.contains(SERVER_NAME),
                "stored TLS name leaked to public output"
            );
            assert!(!text.contains("linux-protected:"));
            assert!(
                !text.contains(
                    &self
                        .server_key
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                )
            );
        }
    }
}
fn marker() -> Vec<u8> {
    serde_json::to_vec(&json!({"version":1,"credential_source":"paired-store"})).unwrap()
}
fn closed_without_resource_commands(
    channel: &mut AuthenticatedControlServerChannel,
    deadline: Instant,
) {
    match channel.receive_envelope_until(deadline) {
        Err(ControlTlsError::Io(error))
            if matches!(
                error.kind(),
                io::ErrorKind::UnexpectedEof
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::BrokenPipe
                    | io::ErrorKind::NotConnected
            ) => {}
        Ok(_) => panic!(
            "inspection emitted another envelope: resource/Route/lease/control commands are forbidden"
        ),
        Err(error) => panic!("expected prompt peer EOF, not timeout or protocol failure: {error}"),
    }
}
fn assert_public_catalog(fixture: &Fixture, events: &[Value]) {
    let count: usize = fixture
        .node
        .capabilities
        .values()
        .map(|c| c.ports.len())
        .sum();
    assert_eq!(events.len(), count + 2);
    assert_eq!(
        events[0],
        json!({"version":1,"event":"catalog_begin","peer_node_id":fixture.node.id,"port_count":count,"text_encoding":"ascii_escaped"})
    );
    assert_eq!(
        events[count + 1],
        json!({"version":1,"event":"catalog_complete","peer_node_id":fixture.node.id,"port_count":count})
    );
    let mut actual = BTreeMap::new();
    for event in &events[1..count + 1] {
        assert_eq!(event["event"], "catalog_port");
        assert_eq!(event["peer_node_id"], fixture.node.id.to_string());
        let cap: CapabilityId = serde_json::from_value(event["capability_id"].clone()).unwrap();
        let port: PortId = serde_json::from_value(event["port_id"].clone()).unwrap();
        assert!(
            actual.insert((cap, port), event).is_none(),
            "no duplicate rows"
        );
        assert_eq!(
            event.as_object().unwrap().len(),
            11,
            "public projection must not grow secret-bearing fields"
        );
    }
    for capability in fixture.node.capabilities.values() {
        for port in capability.ports.values() {
            let event = actual[&(capability.id, port.id)];
            assert_eq!(
                event["direction"],
                serde_json::to_value(port.direction).unwrap()
            );
            assert_eq!(
                event["profile"],
                serde_json::to_value(&port.profile).unwrap()
            );
            assert_eq!(
                event["formats"],
                serde_json::to_value(&port.formats).unwrap()
            );
            assert_eq!(
                event["capability_availability"],
                serde_json::to_value(capability.availability).unwrap()
            );
            assert_eq!(
                event["port_availability"],
                serde_json::to_value(port.availability).unwrap()
            );
            assert!(matches!(
                event["compatibility"]["status"].as_str(),
                Some("needs_local_selection" | "not_supported")
            ));
            assert!(
                !event["compatibility"]["reason"]
                    .as_str()
                    .unwrap()
                    .is_empty()
            );
            if port.direction == PortDirection::Source {
                assert_eq!(
                    event["compatibility"],
                    json!({"status":"needs_local_selection","reason":"audio_playback_needs_local_selection"})
                );
            } else {
                assert_eq!(event["compatibility"]["status"], "not_supported");
            }
        }
    }
}

#[test]
fn ipc_catalog_authenticated_ids_directions_formats_read_only() {
    for count in [0, 3] {
        let fixture = Fixture::new(count);
        let mut process = fixture.start();
        let mut peer = fixture.accept_empty(&mut process);
        fixture.send(&mut peer, &process);
        closed_without_resource_commands(&mut peer, process.started + BOUND);
        assert!(process.finish().success());
        assert_public_catalog(&fixture, &process.events());
        assert!(fixture.pairing.service.state.lock().unwrap().reads > 0);
        fixture.unchanged(&process);
    }
}
#[test]
fn ipc_catalog_pin_and_node_binding_fail_closed() {
    for fault in ["pin", "binding", "catalog-node", "catalog-session"] {
        let mut fixture = Fixture::with_fault(3, fault);
        let mut process = fixture.start();
        if matches!(fault, "pin" | "binding") {
            assert!(
                fixture.accept_result(&mut process).is_err(),
                "wrong paired TLS identity must not authenticate"
            );
        } else {
            let mut peer = fixture.accept_empty(&mut process);
            if fault == "catalog-node" {
                fixture.node.id = NodeId::new();
            }
            let session = if fault == "catalog-session" {
                SessionId::new()
            } else {
                peer.binding().session_id
            };
            peer.send_envelope_until(
                &encode_catalog_snapshot(session, &fixture.node).unwrap(),
                process.started + BOUND,
            )
            .unwrap();
            closed_without_resource_commands(&mut peer, process.started + BOUND);
        }
        process.failed();
        fixture.unchanged(&process);
    }
}
#[test]
fn ipc_catalog_store_failures_never_dial_or_mutate() {
    for fault in [
        "revoked",
        "locked",
        "missing",
        "digest",
        "missing-directory",
    ] {
        let mut fixture = Fixture::new(3);
        match fault {
            "revoked" => fixture.pairing.revoke(),
            "locked" => fixture.pairing.service.state.lock().unwrap().locked = true,
            "missing" => fixture.pairing.service.state.lock().unwrap().missing = true,
            "digest" => fixture.pairing.service.state.lock().unwrap().bytes[0] ^= 1,
            "missing-directory" => {}
            _ => unreachable!(),
        }
        fixture.original = files(&fixture.pairing.state_path);
        let mut args = fixture.pairing.args("linux-catalog");
        let absent = fixture.pairing.service.root.join("must-remain-absent");
        if fault == "missing-directory" {
            args[3] = absent.display().to_string();
        }
        let mut process = Process::node(&args);
        process.frames(&[serde_json::to_vec(&fixture.config()).unwrap(), marker()]);
        process.failed();
        assert!(
            fixture
                .server
                .try_accept_until(Instant::now() + Duration::from_millis(100))
                .unwrap()
                .is_none(),
            "failed store must never dial"
        );
        let state = fixture.pairing.service.state.lock().unwrap();
        assert_eq!(state.forbidden, 0);
        assert_eq!(state.reads > 0, fault == "digest");
        drop(state);
        if fault == "digest" {
            fixture.pairing.service.state.lock().unwrap().bytes[0] ^= 1;
        }
        assert!(
            !absent.exists(),
            "inspection must not create a missing store directory"
        );
        fixture.unchanged(&process);
    }
}
#[test]
fn ipc_catalog_config_and_marker_reject_before_store_access() {
    for fault in [
        "local",
        "peer",
        "dns",
        "ipv6",
        "zero-port",
        "unknown",
        "marker",
        "extra",
    ] {
        let fixture = Fixture::new(3);
        let mut config = fixture.config();
        let mut second = marker();
        match fault {
            "local" => config["local_node_id"] = json!(NodeId::new()),
            "peer" => config["peer_node_id"] = json!(NodeId::new()),
            "dns" => config["endpoint"] = json!("localhost:443"),
            "ipv6" => config["endpoint"] = json!("[::1]:443"),
            "zero-port" => config["endpoint"] = json!("127.0.0.1:0"),
            "unknown" => config["server_name"] = json!("attacker.invalid"),
            "marker" => second = fixture.pairing.directory.clone(),
            "extra" => {}
            _ => unreachable!(),
        }
        let mut process = Process::node(&fixture.pairing.args("linux-catalog"));
        let mut frames = vec![serde_json::to_vec(&config).unwrap(), second];
        if fault == "extra" {
            frames.push(serde_json::to_vec(&json!({"version":1,"command":"start"})).unwrap());
        }
        process.frames(&frames);
        process.failed();
        assert!(
            fixture
                .server
                .try_accept_until(Instant::now() + Duration::from_millis(100))
                .unwrap()
                .is_none()
        );
        let state = fixture.pairing.service.state.lock().unwrap();
        if fault != "extra" {
            assert_eq!(state.sessions, 0, "{fault} before protected access");
            assert_eq!(state.reads, 0);
            assert_eq!(state.collection_property_reads, 0);
        }
        drop(state);
        fixture.unchanged(&process);
    }
}
#[test]
fn ipc_catalog_malformed_and_oversized_catalog_has_no_partial_output() {
    for fault in [
        "missing-node",
        "direction",
        "ports",
        "formats",
        "parameters",
        "string",
        "event",
        "aggregate",
    ] {
        let mut fixture = Fixture::new(match fault {
            "ports" => 65,
            "aggregate" => 64,
            _ => 3,
        });
        let port = fixture
            .node
            .capabilities
            .values_mut()
            .next()
            .unwrap()
            .ports
            .values_mut()
            .next()
            .unwrap();
        match fault {
            "formats" => {
                port.formats = (0..17)
                    .map(|i| FormatDescriptor::new(format!("format-{i}")))
                    .collect()
            }
            "parameters" => {
                port.formats[0].parameters =
                    (0..17).map(|i| (format!("key-{i}"), "v".into())).collect()
            }
            "string" => port.formats[0].id = "x".repeat(129),
            "event" => {
                port.formats = (0..16)
                    .map(|i| FormatDescriptor {
                        id: format!("{i:03}{}", "x".repeat(125)),
                        parameters: BTreeMap::from([("k".repeat(128), "v".repeat(128))]),
                    })
                    .collect();
            }
            _ => {}
        }
        if fault == "aggregate" {
            for capability in fixture.node.capabilities.values_mut() {
                for port in capability.ports.values_mut() {
                    port.formats = (0..8)
                        .map(|i| FormatDescriptor::new(format!("{i:03}{}", "x".repeat(125))))
                        .collect();
                }
            }
        }
        let mut process = fixture.start();
        let mut peer = fixture.accept_empty(&mut process);
        let mut wire = v1::NodeDescriptor::try_from(&fixture.node).unwrap();
        if fault == "direction" {
            wire.capabilities[0].ports[0].direction = 777;
        }
        let envelope = new_envelope(
            Some(peer.binding().session_id),
            v1::envelope::Payload::CatalogSnapshot(v1::CatalogSnapshot {
                node: if fault == "missing-node" {
                    None
                } else {
                    Some(wire)
                },
            }),
        );
        // The generic transport admits these bounded envelopes; catalog semantics
        // and the narrower inspection projection must reject them, not the fixture.
        peer.send_envelope_until(
            &encode_envelope_checked(&envelope).unwrap(),
            process.started + BOUND,
        )
        .unwrap();
        closed_without_resource_commands(&mut peer, process.started + BOUND);
        process.failed();
        fixture.unchanged(&process);
    }
}
#[test]
fn ipc_catalog_route_and_control_payloads_close_without_response() {
    for payload in ["route", "lease", "heartbeat"] {
        let fixture = Fixture::new(3);
        let mut process = fixture.start();
        let mut peer = fixture.accept_empty(&mut process);
        let session = peer.binding().session_id;
        let bytes = match payload {
            "route" => {
                encode_route_command(session, RouteId::new(), RouteCommandAction::Start).unwrap()
            }
            "lease" => encode_port_lease_request(
                session,
                RouteId::new(),
                PortRef {
                    node_id: fixture.pairing.owner,
                    capability_id: CapabilityId::new(),
                    port_id: PortId::new(),
                },
                PortLeaseAction::Acquire,
                1_000,
            )
            .unwrap(),
            "heartbeat" => encode_control_heartbeat(session, 1, false).unwrap(),
            _ => unreachable!(),
        };
        peer.send_envelope_until(&bytes, process.started + BOUND)
            .unwrap();
        closed_without_resource_commands(&mut peer, process.started + BOUND);
        process.failed();
        fixture.unchanged(&process);
    }
}
#[test]
fn ipc_catalog_peer_eof_and_absolute_timeout_are_bounded() {
    for failure in ["peer-eof", "local-eof", "timeout"] {
        let fixture = Fixture::new(3);
        let mut process = fixture.start();
        let mut peer = fixture.accept_empty(&mut process);
        if failure == "peer-eof" {
            drop(peer);
        } else {
            if failure == "local-eof" {
                drop(process.child.stdin.take());
            }
            closed_without_resource_commands(&mut peer, process.started + BOUND);
        }
        process.failed_with_optional_event(failure != "peer-eof");
        if failure == "timeout" {
            assert!(
                process.started.elapsed() >= Duration::from_secs(8),
                "must exercise the absolute operation timeout, not an early EOF"
            );
        }
        fixture.unchanged(&process);
    }
}
#[test]
fn ipc_catalog_untrusted_fields_are_ascii_escaped_and_metadata_omitted() {
    let mut fixture = Fixture::new(3);
    let port = fixture
        .node
        .capabilities
        .values_mut()
        .next()
        .unwrap()
        .ports
        .values_mut()
        .next()
        .unwrap();
    port.profile.name = HOSTILE.into();
    port.formats[0].id = HOSTILE.into();
    port.formats[0].parameters = BTreeMap::from([(HOSTILE.into(), HOSTILE.into())]);
    let mut process = fixture.start();
    let mut peer = fixture.accept_empty(&mut process);
    fixture.send(&mut peer, &process);
    closed_without_resource_commands(&mut peer, process.started + BOUND);
    assert!(process.finish().success());
    let events = process.events();
    let hostile = events
        .iter()
        .find(|e| {
            e["profile"]["name"]
                .as_str()
                .is_some_and(|s| s.starts_with('x'))
        })
        .unwrap();
    for text in [
        hostile["profile"]["name"].as_str().unwrap(),
        hostile["formats"][0]["id"].as_str().unwrap(),
    ] {
        assert!(text.bytes().all(|b| (0x20..=0x7e).contains(&b)));
        assert_eq!(
            text,
            HOSTILE
                .chars()
                .flat_map(char::escape_default)
                .collect::<String>()
        );
        assert!(
            text.contains("\\"),
            "escaping must remain visible, not silently discard peer bytes"
        );
    }
    let params = hostile["formats"][0]["parameters"].as_object().unwrap();
    assert_eq!(params.len(), 1);
    for (key, value) in params {
        for text in [key.as_str(), value.as_str().unwrap()] {
            assert!(text.bytes().all(|b| (0x20..=0x7e).contains(&b)));
        }
    }
    fixture.unchanged(&process);
}
#[test]
fn pty_catalog_actual_console_and_node_show_verified_public_ids() {
    let mut fixture = Fixture::new(3);
    let port = fixture
        .node
        .capabilities
        .values_mut()
        .next()
        .unwrap()
        .ports
        .values_mut()
        .next()
        .unwrap();
    port.formats[0].id = HOSTILE.into();
    let mut process = Process::console(&fixture.pairing, fixture.endpoint);
    let mut peer = fixture.accept_empty(&mut process);
    fixture.send(&mut peer, &process);
    closed_without_resource_commands(&mut peer, process.started + BOUND);
    assert!(process.finish().success());
    let tty = &process.pty.as_ref().unwrap().bytes;
    let text = std::str::from_utf8(tty).unwrap();
    assert!(text.contains(&fixture.node.id.to_string()));
    for capability in fixture.node.capabilities.values() {
        assert!(text.contains(&capability.id.to_string()));
        for port in capability.ports.values() {
            assert!(text.contains(&port.id.to_string()));
            assert!(text.to_ascii_lowercase().contains(match port.direction {
                PortDirection::Source => "source",
                PortDirection::Sink => "sink",
                PortDirection::Control => "control",
            }));
        }
    }
    assert!(
        !tty.contains(&0x1b),
        "remote ANSI/OSC injection reached the real terminal"
    );
    assert!(!text.contains('\u{202e}'));
    assert!(!text.contains(HOSTILE));
    assert!(
        !text.contains("Type start"),
        "catalog must not offer resource consent"
    );
    fixture.unchanged(&process);
}
#[test]
fn pty_catalog_actual_console_rejects_incomplete_or_wrong_peer_catalog() {
    for fault in ["eof", "wrong-node", "cancel", "terminal-eof"] {
        let mut fixture = Fixture::new(3);
        let mut process = Process::console(&fixture.pairing, fixture.endpoint);
        let mut peer = fixture.accept_empty(&mut process);
        if fault == "eof" {
            drop(peer);
        } else if matches!(fault, "cancel" | "terminal-eof") {
            // The real controller already displayed/armed its cancel-only
            // prompt before spawning Node. No peer catalog is supplied.
            let input: &[u8] = if fault == "cancel" {
                b"cancel\n"
            } else {
                b"\x04"
            };
            process
                .pty
                .as_mut()
                .unwrap()
                .master
                .write_all(input)
                .unwrap();
            closed_without_resource_commands(&mut peer, process.started + BOUND);
        } else {
            fixture.node.id = NodeId::new();
            fixture.send(&mut peer, &process);
            closed_without_resource_commands(&mut peer, process.started + BOUND);
        }
        assert!(!process.finish().success());
        let text = String::from_utf8_lossy(&process.pty.as_ref().unwrap().bytes);
        for capability in fixture.node.capabilities.values() {
            assert!(
                !text.contains(&capability.id.to_string()),
                "failed snapshot must never present port rows"
            );
        }
        assert!(!text.contains("Type start"));
        fixture.unchanged(&process);
    }
}

// Manifest preparation uses the same actual sibling Node, synthetic pinned peer
// and private Secret Service as catalog inspection. It never opens the manifest
// with a launcher, creates a media listener or invokes a camera/device helper.
const CAMERA_FORMAT: &str = "cavc-avc-access-unit-v1";
const SELECT_END: &str = "Selection (30 seconds; cancel, Ctrl-C or Ctrl-D abort): ";
const SAVE_END: &str = "Save (30 seconds; cancel, Ctrl-C or Ctrl-D abort): ";

fn camera_fixture(count: usize) -> Fixture {
    let mut fixture = Fixture::new(count);
    fixture.node.platform = Platform::Android;
    for capability in fixture.node.capabilities.values_mut() {
        capability.class = CapabilityClass::Camera;
        capability.availability = Availability::Available;
        for port in capability.ports.values_mut() {
            port.direction = PortDirection::Source;
            port.profile = ProfileId::video_frames_v1();
            port.formats = vec![FormatDescriptor::new(CAMERA_FORMAT)];
            port.availability = Availability::Available;
        }
    }
    fixture.node.validate().unwrap();
    fixture
}

fn camera_rows(fixture: &Fixture) -> Vec<(CapabilityId, PortId)> {
    fixture
        .node
        .capabilities
        .values()
        .flat_map(|capability| {
            capability
                .ports
                .keys()
                .map(move |port| (capability.id, *port))
        })
        .collect()
}

fn terminal_text(process: &Process) -> &str {
    std::str::from_utf8(&process.pty.as_ref().unwrap().bytes).unwrap()
}

fn type_camera(process: &mut Process, text: &[u8]) {
    assert!(text.len() <= 512, "bounded private terminal fixture input");
    process
        .pty
        .as_mut()
        .unwrap()
        .master
        .write_all(text)
        .unwrap();
}

fn camera_prompt(process: &mut Process, marker: &str, end: &str) -> MessageId {
    loop {
        process.collect();
        if terminal_text(process).contains(end) {
            let text = terminal_text(process);
            let request = text
                .rsplit_once(marker)
                .unwrap()
                .1
                .split_whitespace()
                .next()
                .unwrap()
                .parse::<MessageId>()
                .unwrap();
            assert!(!request.as_uuid().is_nil());
            let node = process.pty.as_ref().unwrap().node_pid.unwrap();
            assert!(
                !Path::new(&format!("/proc/{node}")).exists(),
                "actual catalog Node must be reaped before either local prompt"
            );
            no_children(process.child.id());
            // begin_prompt flushes after writing. Do not race that final flush
            // with a fixture answer to an already visible prompt.
            thread::sleep(Duration::from_millis(20));
            return request;
        }
        assert!(
            process.child.try_wait().unwrap().is_none(),
            "console exited before {marker}; terminal={}; stderr={}",
            terminal_text(process),
            String::from_utf8_lossy(&process.err)
        );
        assert!(process.started.elapsed() < BOUND, "bounded manifest prompt");
        thread::sleep(Duration::from_millis(2));
    }
}

fn begin_camera(fixture: &Fixture, output: &Path) -> (Process, MessageId) {
    let mut process = Process::prepare_camera(&fixture.pairing, fixture.endpoint, output);
    let mut peer = fixture.accept_empty(&mut process);
    fixture.send(&mut peer, &process);
    closed_without_resource_commands(&mut peer, process.started + BOUND);
    let request = camera_prompt(&mut process, "Selection request: ", SELECT_END);
    (process, request)
}

fn selection_line(fixture: &Fixture, request: MessageId, row: (CapabilityId, PortId)) -> String {
    format!("select {request} {} {} {}\n", fixture.node.id, row.0, row.1)
}

fn select_camera(
    fixture: &Fixture,
    process: &mut Process,
    request: MessageId,
    row: (CapabilityId, PortId),
) -> MessageId {
    type_camera(process, selection_line(fixture, request, row).as_bytes());
    let save = camera_prompt(process, "Save request: ", SAVE_END);
    assert_ne!(save, request, "output needs independent fresh confirmation");
    save
}

fn save_line(request: MessageId, output: &Path) -> String {
    format!("save {request} {}\n", output.display())
}

fn camera_finished(fixture: &Fixture, process: &mut Process, success: bool) {
    assert_eq!(process.finish().success(), success);
    let text = terminal_text(process);
    assert!(!text.contains("Type start"));
    assert!(!text.contains("Route approval"));
    assert_eq!(
        text.contains("Camera manifest saved; no camera session started."),
        success
    );
    assert!(
        fixture
            .server
            .try_accept_until(Instant::now() + Duration::from_millis(50))
            .unwrap()
            .is_none(),
        "manifest preparation must not reconnect or start a camera session"
    );
    fixture.unchanged(process);
}

fn expected_camera(fixture: &Fixture, row: (CapabilityId, PortId)) -> Value {
    json!({
        "version": 1,
        "node_id": fixture.pairing.owner,
        "source": {
            "node_id": fixture.node.id,
            "capability_id": row.0,
            "port_id": row.1,
        },
        "listen": "127.0.0.1:43197",
        "duration_secs": 7,
    })
}

#[test]
fn pty_prepare_camera_exports_only_explicit_current_selection() {
    let fixture = camera_fixture(2);
    let output = fixture.pairing.service.root.join("camera.json");
    let rows = camera_rows(&fixture);
    let (mut process, selection) = begin_camera(&fixture, &output);
    assert!(
        !output.exists(),
        "catalog visibility must not create a manifest"
    );
    let candidates: Vec<Value> = terminal_text(&process)
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Camera candidate: "))
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(candidates.len(), 2, "never silently choose the first row");
    for row in &rows {
        assert!(candidates.iter().any(|candidate| {
            candidate["capability_id"] == row.0.to_string()
                && candidate["port_id"] == row.1.to_string()
                && candidate["peer_node_id"] == fixture.node.id.to_string()
                && candidate["compatibility"]
                    == json!({"status":"needs_local_selection","reason":"camera_needs_local_selection"})
        }));
    }
    let selected = rows[1];
    let save = select_camera(&fixture, &mut process, selection, selected);
    assert!(!output.exists(), "selection is not file-write confirmation");
    let displayed: Value = terminal_text(&process)
        .lines()
        .find_map(|line| line.trim().strip_prefix("Camera config: "))
        .map(|line| serde_json::from_str(line).unwrap())
        .unwrap();
    assert_eq!(displayed, expected_camera(&fixture, selected));
    type_camera(&mut process, save_line(save, &output).as_bytes());
    camera_finished(&fixture, &mut process, true);
    let metadata = fs::symlink_metadata(&output).unwrap();
    assert!(metadata.file_type().is_file());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    let bytes = fs::read(&output).unwrap();
    assert!(!bytes.is_empty() && bytes.len() <= 4096);
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap(),
        expected_camera(&fixture, selected),
        "exact closed nonsecret launcher schema, not the peer catalog or credentials"
    );
    fixture.pairing.assert_no_leak(&bytes);
    let text = std::str::from_utf8(&bytes).unwrap();
    for forbidden in [
        OMITTED,
        SERVER_NAME,
        "linux-protected:",
        &fixture.endpoint.to_string(),
    ] {
        assert!(!text.contains(forbidden));
    }
    // Deliberately stop at inspecting bytes: no launcher, media or device use.
}

#[test]
fn pty_prepare_camera_rejects_unsupported_catalog_rows() {
    for fault in [
        "sink",
        "profile",
        "format",
        "parameters",
        "port-busy",
        "cap-busy",
        "platform",
        "class",
    ] {
        let mut fixture = camera_fixture(1);
        let capability = fixture.node.capabilities.values_mut().next().unwrap();
        let port = capability.ports.values_mut().next().unwrap();
        match fault {
            "sink" => port.direction = PortDirection::Sink,
            "profile" => port.profile.major = 2,
            "format" => port.formats[0].id = "not-camera-v1".into(),
            "parameters" => {
                port.formats[0]
                    .parameters
                    .insert("unrequested".into(), "1".into());
            }
            "port-busy" => port.availability = Availability::Busy,
            "cap-busy" => capability.availability = Availability::Busy,
            "platform" => fixture.node.platform = Platform::Linux,
            "class" => capability.class = CapabilityClass::Custom("not-camera".into()),
            _ => unreachable!(),
        }
        let output = fixture.pairing.service.root.join("camera.json");
        let mut process = Process::prepare_camera(&fixture.pairing, fixture.endpoint, &output);
        let mut peer = fixture.accept_empty(&mut process);
        fixture.send(&mut peer, &process);
        closed_without_resource_commands(&mut peer, process.started + BOUND);
        camera_finished(&fixture, &mut process, false);
        assert!(
            !output.exists(),
            "unsupported {fault} must never create output"
        );
        assert!(!terminal_text(&process).contains("Camera candidate: "));
        assert!(!terminal_text(&process).contains("Selection request: "));
    }
}

#[test]
fn pty_prepare_camera_rejects_mismatched_stale_multiple_and_pretyped_selection() {
    let mut stale = None;
    for fault in [
        "seed",
        "stale",
        "peer",
        "capability",
        "port",
        "crossed",
        "unsupported",
        "multiple",
        "pretyped",
    ] {
        let mut fixture = camera_fixture(2);
        if fault == "unsupported" {
            fixture
                .node
                .capabilities
                .values_mut()
                .next_back()
                .unwrap()
                .ports
                .values_mut()
                .next()
                .unwrap()
                .availability = Availability::Busy;
        }
        let output = fixture.pairing.service.root.join("camera.json");
        if fault == "pretyped" {
            let mut process = Process::prepare_camera(&fixture.pairing, fixture.endpoint, &output);
            let mut peer = fixture.accept_empty(&mut process);
            // Still waiting for the sole snapshot: no selection prompt exists.
            type_camera(
                &mut process,
                selection_line(&fixture, stale.unwrap(), camera_rows(&fixture)[0]).as_bytes(),
            );
            closed_without_resource_commands(&mut peer, process.started + BOUND);
            camera_finished(&fixture, &mut process, false);
            assert!(!terminal_text(&process).contains("Save request: "));
            assert!(!output.exists());
            continue;
        }
        let (mut process, request) = begin_camera(&fixture, &output);
        let rows = camera_rows(&fixture);
        let line = match fault {
            "seed" => {
                stale = Some(request);
                "cancel\n".into()
            }
            "stale" => {
                assert_ne!(request, stale.unwrap());
                selection_line(&fixture, stale.unwrap(), rows[0])
            }
            "peer" => format!(
                "select {request} {} {} {}\n",
                NodeId::new(),
                rows[0].0,
                rows[0].1
            ),
            "capability" => selection_line(&fixture, request, (CapabilityId::new(), rows[0].1)),
            "port" => selection_line(&fixture, request, (rows[0].0, PortId::new())),
            "crossed" => selection_line(&fixture, request, (rows[0].0, rows[1].1)),
            "unsupported" => selection_line(&fixture, request, rows[1]),
            "multiple" => format!(
                "{}{}",
                selection_line(&fixture, request, rows[0]),
                selection_line(&fixture, request, rows[1])
            ),
            _ => unreachable!(),
        };
        assert!(!output.exists());
        type_camera(&mut process, line.as_bytes());
        camera_finished(&fixture, &mut process, false);
        assert!(
            !output.exists(),
            "invalid {fault} selection must not create output"
        );
        assert!(!terminal_text(&process).contains("Save request: "));
    }
}

#[test]
fn pty_prepare_camera_requires_fresh_exact_output_confirmation() {
    let mut stale = None;
    for fault in [
        "seed",
        "stale",
        "selection-request",
        "path",
        "multiple",
        "pretyped",
    ] {
        let fixture = camera_fixture(1);
        let output = fixture.pairing.service.root.join("camera.json");
        let alternate = fixture.pairing.service.root.join("other.json");
        let (mut process, selection) = begin_camera(&fixture, &output);
        let row = camera_rows(&fixture)[0];
        if fault == "pretyped" {
            type_camera(
                &mut process,
                format!(
                    "{}{}",
                    selection_line(&fixture, selection, row),
                    save_line(stale.unwrap(), &output)
                )
                .as_bytes(),
            );
        } else {
            let save = select_camera(&fixture, &mut process, selection, row);
            assert!(!output.exists(), "save prompt must precede creation");
            let line = match fault {
                "seed" => {
                    stale = Some(save);
                    "cancel\n".into()
                }
                "stale" => {
                    assert_ne!(save, stale.unwrap());
                    save_line(stale.unwrap(), &output)
                }
                "selection-request" => save_line(selection, &output),
                "path" => save_line(save, &alternate),
                "multiple" => format!("{}{}", save_line(save, &output), save_line(save, &output)),
                _ => unreachable!(),
            };
            type_camera(&mut process, line.as_bytes());
        }
        camera_finished(&fixture, &mut process, false);
        assert!(
            !output.exists(),
            "invalid {fault} save must not create output"
        );
        assert!(!alternate.exists());
    }
}

#[test]
fn pty_prepare_camera_cancel_and_eof_never_create_output() {
    for stage in ["selection", "save"] {
        for input in [b"cancel\n".as_slice(), b"\x04".as_slice()] {
            let fixture = camera_fixture(1);
            let output = fixture.pairing.service.root.join("camera.json");
            let (mut process, selection) = begin_camera(&fixture, &output);
            if stage == "save" {
                select_camera(&fixture, &mut process, selection, camera_rows(&fixture)[0]);
            }
            assert!(!output.exists());
            type_camera(&mut process, input);
            camera_finished(&fixture, &mut process, false);
            assert!(
                !output.exists(),
                "{stage} cancellation must not create output"
            );
        }
    }
}

#[test]
fn pty_prepare_camera_refuses_existing_and_symlink_outputs() {
    const SENTINEL: &[u8] = b"existing synthetic file must remain unchanged";
    for fault in [
        "existing",
        "file-symlink",
        "parent-symlink",
        "file-race",
        "parent-race",
    ] {
        let fixture = camera_fixture(1);
        let root = &fixture.pairing.service.root;
        let original = root.join("original");
        let parent = root.join("exports");
        let substitute = root.join("substitute");
        for directory in [&parent, &substitute] {
            fs::DirBuilder::new().mode(0o700).create(directory).unwrap();
        }
        let output = parent.join("camera.json");
        let sentinel = root.join("sentinel");
        fs::write(&sentinel, SENTINEL).unwrap();
        fs::set_permissions(&sentinel, fs::Permissions::from_mode(0o640)).unwrap();
        match fault {
            "existing" => {
                fs::write(&output, SENTINEL).unwrap();
                fs::set_permissions(&output, fs::Permissions::from_mode(0o640)).unwrap();
            }
            "file-symlink" => symlink(&sentinel, &output).unwrap(),
            "parent-symlink" => {
                fs::rename(&parent, &original).unwrap();
                symlink(&substitute, &parent).unwrap();
            }
            _ => {}
        }
        let (mut process, selection) = begin_camera(&fixture, &output);
        let row = camera_rows(&fixture)[0];
        if fault == "parent-symlink" {
            type_camera(
                &mut process,
                selection_line(&fixture, selection, row).as_bytes(),
            );
        } else {
            let save = select_camera(&fixture, &mut process, selection, row);
            match fault {
                "file-race" => {
                    fs::write(&output, SENTINEL).unwrap();
                    fs::set_permissions(&output, fs::Permissions::from_mode(0o640)).unwrap();
                }
                "parent-race" => {
                    fs::rename(&parent, &original).unwrap();
                    symlink(&substitute, &parent).unwrap();
                }
                _ => {}
            }
            type_camera(&mut process, save_line(save, &output).as_bytes());
        }
        camera_finished(&fixture, &mut process, false);
        assert_eq!(fs::read(&sentinel).unwrap(), SENTINEL);
        assert_eq!(
            fs::metadata(&sentinel).unwrap().permissions().mode() & 0o777,
            0o640
        );
        if matches!(fault, "existing" | "file-race") {
            assert_eq!(fs::read(&output).unwrap(), SENTINEL);
            assert_eq!(
                fs::metadata(&output).unwrap().permissions().mode() & 0o777,
                0o640
            );
        } else if fault == "file-symlink" {
            assert!(
                fs::symlink_metadata(&output)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
        } else {
            assert!(!original.join("camera.json").exists());
            assert!(!substitute.join("camera.json").exists());
            assert!(!output.exists());
        }
    }
}
