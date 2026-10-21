#![cfg(target_os = "linux")]
//! Actual executable boundary: inherited private bootstrap, independent local
//! and Android consent, pinned control/data TLS, and the production input owner.
//! Default tests stop before native open. Explicit test-fixture builds use only
//! the fixed software helper, never /dev/uinput, an evdev device or input injection.
// This public wrapper invokes the same TouchpadConsentGate used by Android JNI.
// Its historical camera-specific name does not change the resource-specific gate.
use capyio_android_jni::camera_sender::CameraSourceConsent as AndroidSourceConsent;
use capyio_control_credentials::ControlCredentialBundle;
use capyio_control_link::{CatalogControlLink, ReceivedControlMessage};
use capyio_control_tls::{
    AuthenticatedControlClientChannel, ControlTlsClientIdentity, ControlTlsEndpoint,
    ControlTlsNodeBinding,
};
use capyio_core::*;
use capyio_pairing::{
    CertificateFingerprint, CredentialHandle, PairedPeerDirectory, PairedPeerRecord,
};
use capyio_protocol::RouteCommandAction;
use capyio_runtime::NodeRuntime;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io::{self, Read, Write},
    os::fd::AsFd,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
#[path = "support/paired_store.rs"]
mod paired_store;
#[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
#[path = "support/playback_console.rs"]
mod playback_console;

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
            Ok(count) => bytes.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => panic!("child output read failed: {error}"),
        }
        assert!(bytes.len() <= 1_048_576, "bounded executable output");
    }
}
struct Process {
    child: Child,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    #[cfg(feature = "touchpad-test-fixture")]
    helper_pids: std::collections::BTreeMap<u32, u64>,
    #[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
    paired_store: Option<paired_store::PersistedPairing>,
    #[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
    console: Option<playback_console::Console>,
}
impl Process {
    fn start(args: &[&str]) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_capyio-node"))
            .args(args)
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
            stdout: Vec::new(),
            stderr: Vec::new(),
            #[cfg(feature = "touchpad-test-fixture")]
            helper_pids: std::collections::BTreeMap::new(),
            #[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
            paired_store: None,
            #[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
            console: None,
        }
    }
    fn touchpad() -> Self {
        Self::start(&["linux-touchpad", "--trusted-lab"])
    }
    fn frame(&mut self, body: &[u8]) {
        assert!(
            !self.is_console(),
            "PTY acceptance never writes Node control pipes"
        );
        let mut frame = Vec::from(u32::try_from(body.len()).unwrap().to_be_bytes());
        frame.extend_from_slice(body);
        let deadline = Instant::now() + BOUND;
        let mut offset = 0;
        while offset < frame.len() {
            assert!(Instant::now() < deadline, "bounded bootstrap write");
            match self.child.stdin.as_mut().unwrap().write(&frame[offset..]) {
                Ok(0) => panic!("closed launcher input"),
                Ok(count) => offset += count,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    thread::sleep(Duration::from_millis(2))
                }
                Err(_) => panic!("launcher input write failed"),
            }
        }
        frame.fill(0);
    }
    fn command(&mut self, command: Value) {
        #[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
        if self.console.is_some() {
            match command["command"].as_str().unwrap() {
                "approve" => {
                    let request = command["request_id"].as_str().unwrap();
                    let route = command["route_id"].as_str().unwrap();
                    self.wait_console_prompt(request, route);
                    let console = self.console.as_mut().unwrap();
                    console.send(format!("start {request} {route} {}\n", console.peer).as_bytes());
                }
                "status" => self.console.as_mut().unwrap().send(b"status\n"),
                "stop" => self.console.as_mut().unwrap().send(b"stop\n"),
                _ => panic!("console fixture only accepts typed Start/status/Stop"),
            }
            return;
        }
        self.frame(&serde_json::to_vec(&command).unwrap());
    }
    fn is_console(&self) -> bool {
        #[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
        {
            self.console.is_some()
        }
        #[cfg(not(all(feature = "touchpad-test-fixture", feature = "paired-store")))]
        {
            false
        }
    }
    fn owner_pid(&self) -> u32 {
        #[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
        if let Some(console) = &self.console {
            return console.node_pid.expect("actual Node observed at listening");
        }
        self.child.id()
    }
    #[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
    fn wait_console_prompt(&mut self, request: &str, route: &str) {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
            let console = self.console.as_ref().unwrap();
            if console.prompt_seen(request, route) {
                console.assert_hidden();
                // The terminal performs a second flush after writing the prompt.
                thread::sleep(Duration::from_millis(25));
                return;
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "console exited before prompt"
            );
            assert!(Instant::now() < deadline, "bounded fresh consent prompt");
            thread::sleep(Duration::from_millis(2));
        }
    }
    #[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
    fn wait_console_status_prompt(&mut self) {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
            let console = self.console.as_ref().unwrap();
            if console.status_prompt_seen() {
                console.assert_hidden();
                thread::sleep(Duration::from_millis(25));
                return;
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "console exited before status prompt"
            );
            assert!(Instant::now() < deadline, "bounded status prompt");
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn close_input(&mut self) {
        #[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
        if let Some(console) = &mut self.console {
            // Terminal raw mode interprets Control-D as EOF/cancel. The fixture
            // cannot access the console's inherited Node-control descriptor.
            console.send(b"\x04");
            return;
        }
        drop(self.child.stdin.take());
    }
    fn collect(&mut self) {
        #[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
        if let Some(console) = &mut self.console {
            console.collect(&mut self.child, &mut self.stdout, &mut self.stderr);
        }
        #[cfg(feature = "touchpad-test-fixture")]
        let owner = {
            #[cfg(feature = "paired-store")]
            {
                self.console
                    .as_ref()
                    .map_or(Some(self.child.id()), |console| console.node_pid)
            }
            #[cfg(not(feature = "paired-store"))]
            {
                Some(self.child.id())
            }
        };
        #[cfg(feature = "touchpad-test-fixture")]
        for pid in owner.into_iter().flat_map(child_pids) {
            let Some(identity) = process_identity(pid).expect("owned helper process identity")
            else {
                continue;
            };
            assert_eq!(
                identity.parent,
                owner.unwrap(),
                "helper still belongs to owned Node"
            );
            if let Some(started) = self.helper_pids.get(&pid) {
                assert_eq!(*started, identity.started, "owned helper PID was reused");
            }
            let expected = std::path::Path::new(env!("CARGO_BIN_EXE_capyio-node"))
                .parent()
                .unwrap()
                .join("capyio-linux-touchpad-fixture-helper")
                .canonicalize()
                .unwrap();
            let verified = inspect_owned_helper(identity, &expected, |pid| {
                #[cfg(feature = "paired-store")]
                if let Some(store) = &self.paired_store {
                    for name in ["cmdline", "environ"] {
                        store.assert_no_leak(&std::fs::read(format!("/proc/{pid}/{name}"))?);
                    }
                }
                #[cfg(not(feature = "paired-store"))]
                let _ = pid;
                Ok(())
            })
            .expect("bounded fixed-helper identity and metadata inspection");
            if verified {
                self.helper_pids.insert(pid, identity.started);
            }
            // An ended or vanished helper retains prior verified evidence but
            // never earns new identity/redaction evidence from an incomplete scan.
        }
        if !self.is_console() {
            drain(self.child.stdout.as_mut().unwrap(), &mut self.stdout);
            drain(self.child.stderr.as_mut().unwrap(), &mut self.stderr);
        }
        #[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
        if let Some(store) = &self.paired_store {
            store.assert_no_leak(&self.stdout);
            store.assert_no_leak(&self.stderr);
            if let Some(console) = &self.console {
                store.assert_no_leak(&console.tty);
            }
        }
    }
    fn next_event(&mut self) -> Value {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
            #[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
            if let Some(console) = &mut self.console
                && let Some(value) = console.next_event()
            {
                if value["event"] == "listening" {
                    console.identify_node(&self.child);
                }
                if value["event"] == "status" {
                    // The event line precedes the controller's fresh prompt and
                    // final input flush. Never race the next typed command.
                    self.wait_console_status_prompt();
                }
                return value;
            }
            if self.stdout.len() >= 4 {
                let length = u32::from_be_bytes(self.stdout[..4].try_into().unwrap()) as usize;
                assert!((1..=4096).contains(&length), "bounded local event");
                if self.stdout.len() >= length + 4 {
                    let value: Value = serde_json::from_slice(&self.stdout[4..length + 4]).unwrap();
                    self.stdout.drain(..length + 4);
                    assert_eq!(value["version"], 1);
                    return value;
                }
            }
            assert!(
                Instant::now() < deadline,
                "missing bounded local event; stderr={}",
                String::from_utf8_lossy(&self.stderr)
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn event(&mut self, expected: &str) -> Value {
        let value = self.next_event();
        assert_eq!(value["event"], expected);
        value
    }
    fn finish(&mut self) -> ExitStatus {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
            if let Some(status) = self.child.try_wait().unwrap() {
                self.collect();
                #[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
                if let Some(console) = &self.console {
                    console.assert_finished();
                    let store = self.paired_store.as_ref().unwrap();
                    let state = store.service.state.lock().unwrap();
                    assert!(state.reads > 0, "actual protected-store read required");
                    assert_eq!(state.forbidden, 0, "read-only private Secret Service");
                }
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "executable must terminate promptly"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        // No blocking reader/writer threads survive assertions or timeout paths.
        #[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
        if self.console.is_some() {
            // SAFETY: only this test's private PTY session/process group is owned.
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
    ControlTlsClientIdentity,
    CertificateFingerprint,
    ControlTlsClientIdentity,
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
    let leaf = |name: &str| {
        let mut params = CertificateParams::new(vec![name.into()]).unwrap();
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        // The same enrolled leaf is control client/server and data server/client.
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ClientAuth,
            ExtendedKeyUsagePurpose::ServerAuth,
        ];
        let key = KeyPair::generate().unwrap();
        (
            params.signed_by(&key, &issuer).unwrap().der().to_vec(),
            key.serialize_der(),
        )
    };
    let (server, server_key) = leaf("localhost");
    let (client, client_key) = leaf("capyio-test-client");
    let (replacement, replacement_key) = leaf("capyio-test-client");
    let server_pin = CertificateFingerprint::from_certificate_der(&server).unwrap();
    let client_pin = CertificateFingerprint::from_certificate_der(&client).unwrap();
    (
        ControlCredentialBundle::new(vec![server, ca.clone()], server_key, ca.clone()).unwrap(),
        ControlTlsClientIdentity {
            expected_peer_certificate: server_pin,
            certificate_chain_der: vec![client, ca.clone()],
            private_key_der: client_key,
            server_ca_der: ca.clone(),
        },
        client_pin,
        ControlTlsClientIdentity {
            expected_peer_certificate: server_pin,
            certificate_chain_der: vec![replacement, ca.clone()],
            private_key_der: replacement_key,
            server_ca_der: ca,
        },
    )
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
fn child_pids(pid: u32) -> Vec<u32> {
    let mut children = Vec::new();
    // Read-only /proc scan also works on kernels without task/<pid>/children.
    for entry in std::fs::read_dir("/proc").unwrap().flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|b| b.is_ascii_digit())
        {
            continue;
        }
        let Ok(status) = std::fs::read_to_string(entry.path().join("status")) else {
            continue;
        };
        let parent = status
            .lines()
            .find_map(|line| line.strip_prefix("PPid:"))
            .and_then(|s| s.trim().parse::<u32>().ok());
        if parent == Some(pid) {
            children.push(entry.file_name().to_string_lossy().parse().unwrap());
        }
    }
    children
}
#[cfg(feature = "touchpad-test-fixture")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProcessIdentity {
    pid: u32,
    parent: u32,
    started: u64,
    state: u8,
}
#[cfg(feature = "touchpad-test-fixture")]
impl ProcessIdentity {
    fn parse(stat: &str) -> Option<Self> {
        let (pid, rest) = stat.split_once(" (")?;
        // The command may itself contain spaces and parentheses. State starts
        // after the final ') '; field 22 is the immutable process start time.
        let (_, fields) = rest.rsplit_once(") ")?;
        let fields: Vec<_> = fields.split_ascii_whitespace().collect();
        let state = *fields.first()?.as_bytes().first()?;
        if fields[0].len() != 1 || !b"RSDZTtXKWPI".contains(&state) {
            return None;
        }
        let identity = Self {
            pid: pid.parse().ok()?,
            parent: fields.get(1)?.parse().ok()?,
            started: fields.get(19)?.parse().ok()?,
            state,
        };
        (identity.pid != 0 && identity.started != 0).then_some(identity)
    }
    fn same_generation(self, other: Self) -> bool {
        self.pid == other.pid && self.parent == other.parent && self.started == other.started
    }
    fn ended(self) -> bool {
        matches!(self.state, b'Z' | b'X')
    }
}
#[cfg(feature = "touchpad-test-fixture")]
fn process_identity(pid: u32) -> io::Result<Option<ProcessIdentity>> {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => ProcessIdentity::parse(&stat)
            .filter(|identity| identity.pid == pid)
            .map(Some)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid process identity")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
#[cfg(feature = "touchpad-test-fixture")]
fn same_owned_process_ended(before: ProcessIdentity, after: Option<ProcessIdentity>) -> bool {
    after.is_none_or(|after| before.same_generation(after) && after.ended())
}

// Read-only /proc observation only: no control/data, lease, native-commit,
// terminal or reap deadline is extended by this fixed inspection budget.
#[cfg(feature = "touchpad-test-fixture")]
const HELPER_OBSERVATION_BUDGET: Duration = Duration::from_millis(100);

#[cfg(feature = "touchpad-test-fixture")]
#[derive(Clone, Copy)]
struct HelperObservation {
    identity: Option<ProcessIdentity>,
    inspected: bool,
}

#[cfg(feature = "touchpad-test-fixture")]
fn inspect_owned_helper(
    before: ProcessIdentity,
    expected: &std::path::Path,
    mut inspect_metadata: impl FnMut(u32) -> io::Result<()>,
) -> io::Result<bool> {
    let started = Instant::now();
    observe_owned_helper(
        before,
        || {
            let observation = probe_owned_helper(before, expected, &mut inspect_metadata)?;
            Ok((started.elapsed(), observation))
        },
        || thread::sleep(Duration::from_millis(2)),
    )
}

#[cfg(feature = "touchpad-test-fixture")]
fn probe_owned_helper(
    before: ProcessIdentity,
    expected: &std::path::Path,
    inspect_metadata: &mut impl FnMut(u32) -> io::Result<()>,
) -> io::Result<HelperObservation> {
    let identity = process_identity(before.pid)?;
    let pending = HelperObservation {
        identity,
        inspected: false,
    };
    let Some(current) = identity else {
        return Ok(pending);
    };
    if !before.same_generation(current) || current.ended() {
        return Ok(pending);
    }
    let inspection = (|| {
        let executable = std::fs::read_link(format!("/proc/{}/exe", before.pid))?;
        if executable != expected {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "fixed fixture helper executable mismatch",
            ));
        }
        inspect_metadata(before.pid)
    })();
    match inspection {
        Ok(()) => Ok(HelperObservation {
            // All successful reads are bracketed by the same live generation.
            // Exit or reuse here cannot earn successful inspection evidence.
            identity: process_identity(before.pid)?,
            inspected: true,
        }),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
            ) =>
        {
            Ok(pending)
        }
        Err(error) => Err(error),
    }
}

#[cfg(feature = "touchpad-test-fixture")]
fn observe_owned_helper(
    before: ProcessIdentity,
    mut observe: impl FnMut() -> io::Result<(Duration, HelperObservation)>,
    mut pause: impl FnMut(),
) -> io::Result<bool> {
    let mut first_incomplete = None;
    loop {
        let (observed_at, observation) = observe()?;
        if observation
            .identity
            .is_some_and(|after| !before.same_generation(after))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "owned helper generation changed",
            ));
        }
        if first_incomplete
            .is_some_and(|started| observed_at.saturating_sub(started) >= HELPER_OBSERVATION_BUDGET)
        {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "owned helper inspection stayed incomplete",
            ));
        }
        if same_owned_process_ended(before, observation.identity) {
            return Ok(false);
        }
        if observation.inspected {
            return Ok(true);
        }
        // exe/environ can disappear before Z/X appears. Retry the complete
        // exact proof, never treating bare live stat or permission denial as
        // success. The first unavailable read fixes the one shared budget.
        first_incomplete.get_or_insert(observed_at);
        pause();
    }
}

#[cfg(feature = "touchpad-test-fixture")]
#[test]
fn pure_helper_observation_retries_full_proof_with_one_generation_bound_budget() {
    fn run(
        before: ProcessIdentity,
        samples: &[(u64, HelperObservation)],
    ) -> (io::Result<bool>, usize) {
        let mut used = 0;
        let result = observe_owned_helper(
            before,
            || {
                let (elapsed, observation) = samples
                    .get(used)
                    .expect("must stop within scripted observations");
                used += 1;
                Ok((Duration::from_millis(*elapsed), *observation))
            },
            || {},
        );
        (result, used)
    }
    let live = ProcessIdentity {
        pid: 42,
        parent: 7,
        started: 12345,
        state: b'S',
    };
    let pending = HelperObservation {
        identity: Some(live),
        inspected: false,
    };
    let complete = HelperObservation {
        inspected: true,
        ..pending
    };
    let gone = HelperObservation {
        identity: None,
        inspected: false,
    };
    assert!(run(live, &[(0, complete)]).0.unwrap());
    assert!(
        run(live, &[(0, pending), (2, pending), (4, complete)])
            .0
            .unwrap()
    );
    for state in *b"ZX" {
        let ended = HelperObservation {
            identity: Some(ProcessIdentity { state, ..live }),
            inspected: false,
        };
        let (result, used) = run(live, &[(0, pending), (2, ended)]);
        assert!(
            !result.unwrap(),
            "ended process never earns new successful evidence"
        );
        assert_eq!(used, 2);
    }
    assert!(!run(live, &[(0, pending), (2, gone)]).0.unwrap());
    for last in [pending, complete, gone] {
        let (result, used) = run(live, &[(0, pending), (99, pending), (100, last)]);
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert_eq!(used, 3, "retries must not reset the original budget");
    }
    assert!(run(live, &[(23, pending), (122, complete)]).0.unwrap());
    assert_eq!(
        run(live, &[(23, pending), (123, complete)])
            .0
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    for changed in [
        ProcessIdentity { pid: 43, ..live },
        ProcessIdentity { parent: 8, ..live },
        ProcessIdentity {
            started: 12346,
            ..live
        },
    ] {
        for inspected in [false, true] {
            let replaced = HelperObservation {
                identity: Some(changed),
                inspected,
            };
            let (result, used) = run(live, &[(0, pending), (2, replaced), (4, gone)]);
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
            assert_eq!(
                used, 2,
                "identity change rejects before later disappearance"
            );
        }
    }
    let error = observe_owned_helper(
        live,
        || Err(io::Error::from(io::ErrorKind::PermissionDenied)),
        || panic!("stat read error must not be retried or treated as disappearance"),
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
}

#[cfg(feature = "touchpad-test-fixture")]
#[test]
fn pure_helper_proc_stat_requires_exact_generation_identity() {
    let stat = format!(
        "42 (helper (with) spaces) S 7 {} 12345 0 0",
        ["0"; 17].join(" ")
    );
    let identity = ProcessIdentity::parse(&stat).unwrap();
    assert_eq!(
        identity,
        ProcessIdentity {
            pid: 42,
            parent: 7,
            started: 12345,
            state: b'S'
        }
    );
    for malformed in [
        "",
        "42 helper S 7",
        "42 (helper) S 7 0",
        "42 (helper) SS 7 0",
    ] {
        assert!(ProcessIdentity::parse(malformed).is_none());
    }
    assert!(ProcessIdentity::parse(&stat.replacen(" S ", " ? ", 1)).is_none());
    assert!(ProcessIdentity::parse(&stat.replacen("12345", "0", 1)).is_none());
    assert!(ProcessIdentity::parse(&stat.replacen("42 (", "0 (", 1)).is_none());
}

#[cfg(feature = "touchpad-test-fixture")]
#[test]
fn pure_helper_metadata_denial_requires_proven_same_generation_exit() {
    let before = ProcessIdentity {
        pid: 42,
        parent: 7,
        started: 12345,
        state: b'S',
    };
    assert!(
        !same_owned_process_ended(before, Some(before)),
        "live permission denial fails closed"
    );
    assert!(
        same_owned_process_ended(before, None),
        "missing process is not live"
    );
    for state in *b"ZX" {
        let ended = ProcessIdentity { state, ..before };
        assert!(same_owned_process_ended(before, Some(ended)));
        for changed in [
            ProcessIdentity { pid: 43, ..ended },
            ProcessIdentity { parent: 8, ..ended },
            ProcessIdentity {
                started: 12346,
                ..ended
            },
        ] {
            assert!(
                !same_owned_process_ended(before, Some(changed)),
                "reparented or reused PID is not the verified helper"
            );
        }
    }
    for state in b"RSDTtKWPI" {
        assert!(!same_owned_process_ended(
            before,
            Some(ProcessIdentity {
                state: *state,
                ..before
            })
        ));
    }
}

fn no_children(process: &Process) {
    assert!(
        child_pids(process.owner_pid()).is_empty(),
        "no helper before both grants and authenticated binding"
    );
}
fn approval_command(approval: &Value) -> Value {
    json!({"version":1,"command":"approve","request_id":approval["request_id"],"route_id":approval["route_id"]})
}
struct Fixture {
    config: Value,
    directory: Vec<u8>,
    credentials: ControlCredentialBundle,
    client: ControlTlsClientIdentity,
    runtime: NodeRuntime,
    source: PortRef,
    #[cfg(feature = "touchpad-test-fixture")]
    replacement: ControlTlsClientIdentity,
}
impl Fixture {
    fn new() -> Self {
        // Synthetic capability descriptor only; Android consent is never simulated.
        // authorization and leases are ALWAYS production Android Rust policy.
        let mut android = capyio_testkit::android_node();
        for adapter in android.adapter_instances.values_mut() {
            adapter
                .supported_route_modes
                .insert(RouteBackend::AdapterManaged);
        }
        let cap = android
            .capabilities
            .values_mut()
            .find(|c| c.class == CapabilityClass::Touchpad)
            .unwrap();
        cap.availability = Availability::Available;
        let port = cap
            .ports
            .values_mut()
            .find(|p| p.direction == PortDirection::Source)
            .unwrap();
        port.availability = Availability::Available;
        port.interoperability_mode = InteroperabilityMode::AdapterManaged;
        port.formats = vec![FormatDescriptor::new("touchpad-frame-v1")];
        port.qos_modes.insert(QosMode::Interactive);
        let source = PortRef {
            node_id: android.id,
            capability_id: cap.id,
            port_id: port.id,
        };
        let local = NodeId::new();
        let (credentials, client, client_pin, _replacement) = identities();
        let mut directory = PairedPeerDirectory::new(local).unwrap();
        directory
            .add(
                PairedPeerRecord::new(
                    source.node_id,
                    "Synthetic Android",
                    "capyio-test-client",
                    CredentialHandle::new("ephemeral.test.only").unwrap(),
                    client_pin,
                    1,
                )
                .unwrap(),
            )
            .unwrap();
        Self {
            config: json!({"version":1,"node_id":local,"source":source,"listen":"127.0.0.1:0","duration_secs":10}),
            directory: directory.encode_json().unwrap(),
            credentials,
            client,
            runtime: NodeRuntime::new(android).unwrap(),
            source,
            #[cfg(feature = "touchpad-test-fixture")]
            replacement: _replacement,
        }
    }
    fn bootstrap(&self, process: &mut Process) {
        process.command(self.config.clone());
        process.frame(&self.directory);
        let mut encoded = self.credentials.encode().unwrap();
        process.frame(&encoded);
        encoded.fill(0);
    }
    fn dial(&mut self, process: &mut Process) -> CatalogControlLink {
        let event = process.event("listening");
        let nodes = ControlTlsNodeBinding::new(
            serde_json::from_value(self.config["node_id"].clone()).unwrap(),
            self.source.node_id,
        )
        .unwrap();
        let client = AuthenticatedControlClientChannel::connect_fresh_session(
            ControlTlsEndpoint {
                address: event["address"].as_str().unwrap().parse().unwrap(),
                io_timeout: BOUND,
            },
            "localhost",
            self.client.clone(),
            nodes,
        )
        .unwrap();
        let mut link = CatalogControlLink::admit_dialed_new_peer(
            &mut self.runtime,
            client,
            "Synthetic Linux".into(),
        )
        .unwrap();
        let source_capability =
            &self.runtime.snapshot().local_node.capabilities[&self.source.capability_id];
        let selected = if source_capability
            .ports
            .values()
            .any(|port| port.direction == PortDirection::Source)
        {
            BTreeSet::from([self.source.capability_id])
        } else {
            BTreeSet::new()
        };
        let catalog = link.prepare_catalog(&self.runtime, &selected).unwrap();
        link.send_catalog(catalog).unwrap();
        link
    }
    fn connect(mut self, process: &mut Process) -> Peer {
        let mut link = self.dial(process);
        let catalog = link.receive_catalog().unwrap();
        link.apply_catalog(&mut self.runtime, catalog).unwrap();
        Peer {
            runtime: self.runtime,
            link,
            consent: AndroidSourceConsent::default(),
            #[cfg(feature = "touchpad-test-fixture")]
            client: self.client,
            #[cfg(feature = "touchpad-test-fixture")]
            replacement: self.replacement,
        }
    }
}
struct Peer {
    runtime: NodeRuntime,
    link: CatalogControlLink,
    consent: AndroidSourceConsent,
    #[cfg(feature = "touchpad-test-fixture")]
    client: ControlTlsClientIdentity,
    #[cfg(feature = "touchpad-test-fixture")]
    replacement: ControlTlsClientIdentity,
}
impl Peer {
    fn proposal(&mut self, id: RouteId) {
        let ReceivedControlMessage::RouteStatus(status) = self
            .link
            .receive_control_message_until(Instant::now() + BOUND)
            .unwrap()
        else {
            panic!("Draft required")
        };
        assert_eq!(status.route.id, id);
        assert_eq!(status.route.state, RouteState::Draft);
        assert_eq!(status.route.authorization, AuthorizationState::Pending);
        assert!(status.touchpad_data.is_none());
        self.link
            .apply_route_status(&mut self.runtime, &status, false)
            .unwrap();
        self.consent
            .enqueue(&mut self.runtime, id, now_ms())
            .unwrap();
        assert!(self.runtime.snapshot().port_leases.is_empty());
        let source = self.runtime.route(id).unwrap().source;
        assert!(
            self.runtime
                .acquire_route_port_lease(id, source, now_ms() + 5000, now_ms())
                .is_err()
        );
        self.link
            .send_route_status(self.runtime.route(id).unwrap())
            .unwrap();
    }
    fn grant(&mut self, id: RouteId, allow: bool, ttl: u64) {
        let now = now_ms();
        self.consent
            .resolve(&mut self.runtime, id, allow, now, now + ttl)
            .unwrap();
        self.link
            .send_route_status(self.runtime.route(id).unwrap())
            .unwrap();
    }
    fn start_route(&mut self, id: RouteId) {
        let mut narrowed = false;
        let mut leased = false;
        let mut prepared = false;
        loop {
            match self
                .link
                .receive_control_message_until(Instant::now() + BOUND)
                .unwrap()
            {
                ReceivedControlMessage::RouteStatus(status) => {
                    assert_eq!(status.route.id, id);
                    assert!(
                        status.touchpad_data.is_none(),
                        "Source alone offers touchpad data"
                    );
                    if status.route.state == RouteState::Prepared {
                        assert!(narrowed && leased && !prepared);
                        assert_eq!(
                            status.route.selected_format,
                            Some(FormatDescriptor::new("touchpad-frame-v1"))
                        );
                        assert_eq!(status.route.selected_qos_mode, Some(QosMode::Interactive));
                        prepared = true;
                        continue;
                    }
                    assert_eq!(status.route.state, RouteState::Draft);
                    let AuthorizationState::Authorized {
                        expires_at_ms: Some(expiry),
                    } = status.route.authorization
                    else {
                        panic!("finite narrowing required")
                    };
                    self.runtime
                        .restrict_draft_authorization(id, expiry, now_ms())
                        .unwrap();
                    assert_eq!(
                        self.runtime.route(id).unwrap().authorization,
                        status.route.authorization
                    );
                    self.link
                        .send_route_status(self.runtime.route(id).unwrap())
                        .unwrap();
                    narrowed = true;
                }
                ReceivedControlMessage::PortLeaseRequest(request) => {
                    assert!(narrowed && !leased);
                    self.link
                        .answer_port_lease_request(&mut self.runtime, request, now_ms())
                        .unwrap();
                    leased = true;
                }
                ReceivedControlMessage::RouteCommand(command) => {
                    assert!(narrowed && leased && prepared);
                    assert_eq!(command.route_id, id);
                    assert_eq!(command.action, RouteCommandAction::Start);
                    let route = self
                        .link
                        .stage_route_command(&mut self.runtime, &command, now_ms())
                        .unwrap();
                    assert_eq!(route.state, RouteState::Starting);
                    assert_ne!(route.epoch, 0);
                    return;
                }
                _ => panic!("unexpected negotiation message"),
            }
        }
    }
    fn idle(&mut self, process: &Process) {
        let deadline = Instant::now() + Duration::from_millis(100);
        while Instant::now() < deadline {
            no_children(process);
            assert!(
                self.link
                    .poll_control_message_until(Instant::now() + BOUND)
                    .unwrap()
                    .is_none(),
                "no lease/offer before independent consent"
            );
            assert!(self.runtime.snapshot().port_leases.is_empty());
            thread::sleep(Duration::from_millis(2));
        }
    }
}
fn pending() -> (Process, Peer, Value) {
    let fixture = Fixture::new();
    let mut process = Process::touchpad();
    fixture.bootstrap(&mut process);
    let mut peer = fixture.connect(&mut process);
    let approval = process.event("approval_required");
    assert_eq!(approval["media_security"], "tls13_mtls_v1");
    assert_eq!(approval["format"], "touchpad-frame-v1");
    assert_eq!(approval["max_contacts"], 5);
    assert_eq!(approval["width_mm"], 120);
    assert_eq!(approval["height_mm"], 70);
    assert_eq!(approval["clickable"], false);
    assert_eq!(approval["pressure"], true);
    peer.idle(&process);
    (process, peer, approval)
}
fn failed(process: &mut Process) {
    process.event("failed");
    assert_eq!(process.finish().code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&process.stderr).trim(),
        "Error: Linux touchpad launch failed"
    );
}
#[test]
fn ipc_linux_touchpad_requires_exact_approval_and_independent_android_consent() {
    let (mut process, mut peer, approval) = pending();
    process.command(approval_command(&approval));
    let id = serde_json::from_value(approval["route_id"].clone()).unwrap();
    peer.proposal(id);
    peer.idle(&process);
    process.command(json!({"version":1,"command":"stop"}));
    process.event("stopped");
    assert!(process.finish().success());
}
#[test]
fn ipc_linux_touchpad_wrong_request_or_route_fails_closed() {
    for field in ["request_id", "route_id"] {
        let (mut process, mut peer, approval) = pending();
        let mut command = approval_command(&approval);
        command[field] = json!(MessageId::new());
        no_children(&process);
        process.command(command);
        failed(&mut process);
        assert!(
            peer.link
                .receive_control_message_until(Instant::now() + BOUND)
                .is_err()
        );
    }
}
#[test]
fn ipc_linux_touchpad_android_denial_fails_before_media_children() {
    let (mut process, mut peer, approval) = pending();
    process.command(approval_command(&approval));
    let id = serde_json::from_value(approval["route_id"].clone()).unwrap();
    peer.proposal(id);
    no_children(&process);
    peer.grant(id, false, 5000);
    failed(&mut process);
    assert!(peer.runtime.snapshot().port_leases.is_empty());
}
#[test]
fn ipc_linux_touchpad_bad_config_and_source_fail_closed() {
    for (field, value) in [
        ("version", json!(2)),
        ("duration_secs", json!(0)),
        ("duration_secs", json!(2)),
        ("duration_secs", json!(61)),
        ("listen", json!("0.0.0.0:0")),
        ("media", json!("127.0.0.1:9876")),
        ("helper", json!("/tmp/not-trusted")),
        (
            "source",
            json!({"node_id":NodeId::new(),"capability_id":CapabilityId::new(),"port_id":PortId::new()}),
        ),
    ] {
        let mut fixture = Fixture::new();
        fixture.config[field] = value;
        let mut process = Process::touchpad();
        // Config errors can close input immediately. Write all three in one syscall.
        let mut bytes = Vec::new();
        for frame in [
            serde_json::to_vec(&fixture.config).unwrap(),
            fixture.directory.clone(),
            fixture.credentials.encode().unwrap(),
        ] {
            bytes.extend_from_slice(&(frame.len() as u32).to_be_bytes());
            bytes.extend(frame);
        }
        process
            .child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(&bytes)
            .unwrap();
        bytes.fill(0);
        failed(&mut process);
    }
}
#[test]
fn ipc_linux_touchpad_start_waits_for_source_data_offer_without_native_children() {
    let (mut process, mut peer, approval) = pending();
    process.command(approval_command(&approval));
    let id = serde_json::from_value(approval["route_id"].clone()).unwrap();
    peer.proposal(id);
    peer.grant(id, true, 5000);
    peer.start_route(id);
    no_children(&process);
    process.command(json!({"version":1,"command":"status"}));
    let status = process.event("status");
    assert_eq!(status["stage"], "awaiting_touchpad_offer");
    process.command(json!({"version":1,"command":"stop"}));
    process.event("stopped");
    assert!(process.finish().success());
}

#[test]
fn ipc_linux_touchpad_preapproval_eof_is_terminal() {
    let (mut process, mut peer, _) = pending();
    no_children(&process);
    process.close_input();
    assert_eq!(process.finish().code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&process.stderr).trim(),
        "Error: Linux touchpad launch failed"
    );
    assert!(
        peer.link
            .receive_control_message_until(Instant::now() + BOUND)
            .is_err()
    );
}
#[test]
fn ipc_linux_touchpad_wrong_certificate_pin_fails_without_children() {
    let mut fixture = Fixture::new();
    fixture.client.expected_peer_certificate = identities().1.expected_peer_certificate;
    let mut process = Process::touchpad();
    fixture.bootstrap(&mut process);
    let listening = process.event("listening");
    no_children(&process);
    let nodes = ControlTlsNodeBinding::new(
        serde_json::from_value(fixture.config["node_id"].clone()).unwrap(),
        fixture.source.node_id,
    )
    .unwrap();
    assert!(
        AuthenticatedControlClientChannel::connect_fresh_session(
            ControlTlsEndpoint {
                address: listening["address"].as_str().unwrap().parse().unwrap(),
                io_timeout: BOUND
            },
            "localhost",
            fixture.client,
            nodes
        )
        .is_err()
    );
    failed(&mut process);
}

#[test]
fn ipc_linux_touchpad_nested_unknown_duplicate_and_nonobject_config_are_rejected() {
    let fixture = Fixture::new();
    let config = serde_json::to_string(&fixture.config).unwrap();
    let duplicate_top = config.replacen("{", "{\"version\":1,", 1);
    let source = serde_json::to_string(&fixture.config["source"]).unwrap();
    let duplicate_source = config.replace(
        &source,
        &source.replacen(
            "{",
            &format!("{{\"port_id\":{:?},", fixture.source.port_id.to_string()),
            1,
        ),
    );
    let mut unknown_source = fixture.config.clone();
    unknown_source["source"]["helper"] = json!("/tmp/untrusted");
    for bytes in [
        b"[]".to_vec(),
        duplicate_top.into_bytes(),
        duplicate_source.into_bytes(),
        serde_json::to_vec(&unknown_source).unwrap(),
    ] {
        let mut process = Process::touchpad();
        process.frame(&bytes);
        failed(&mut process);
        no_children(&process);
    }
}

#[test]
fn ipc_linux_touchpad_authenticated_catalog_source_is_validated_exactly() {
    for defect in [
        "missing",
        "platform",
        "class",
        "direction",
        "profile",
        "availability",
        "format",
    ] {
        let mut fixture = Fixture::new();
        let mut android = fixture.runtime.snapshot().local_node;
        if defect == "missing" {
            fixture.config["source"]["port_id"] = json!(PortId::new());
        } else if defect == "platform" {
            android.platform = Platform::Linux;
        } else {
            let cap = android
                .capabilities
                .get_mut(&fixture.source.capability_id)
                .unwrap();
            let port = cap.ports.get_mut(&fixture.source.port_id).unwrap();
            match defect {
                "class" => cap.class = CapabilityClass::Gamepad,
                "direction" => port.direction = PortDirection::Sink,
                "profile" => port.profile = ProfileId::gamepad_state_v1(),
                "availability" => port.availability = Availability::Offline,
                "format" => port.formats = vec![FormatDescriptor::new("relative-mouse")],
                _ => unreachable!(),
            }
        }
        fixture.runtime = NodeRuntime::new(android).unwrap();
        let mut process = Process::touchpad();
        fixture.bootstrap(&mut process);
        let mut link = fixture.dial(&mut process);
        failed(&mut process);
        no_children(&process);
        assert!(
            link.receive_catalog().is_err(),
            "invalid Source cannot receive Linux catalog or consent"
        );
    }
}

#[test]
fn ipc_linux_touchpad_denied_or_missing_source_lease_never_starts_data() {
    for answer in [false, true] {
        let (mut process, mut peer, approval) = pending();
        process.command(approval_command(&approval));
        let id = serde_json::from_value(approval["route_id"].clone()).unwrap();
        peer.proposal(id);
        peer.grant(id, true, 5000);
        let request = loop {
            match peer
                .link
                .receive_control_message_until(Instant::now() + BOUND)
                .unwrap()
            {
                ReceivedControlMessage::RouteStatus(status) => {
                    assert_eq!(status.route.id, id);
                    assert_eq!(status.route.state, RouteState::Draft);
                    let AuthorizationState::Authorized {
                        expires_at_ms: Some(expiry),
                    } = status.route.authorization
                    else {
                        panic!("finite grant required")
                    };
                    peer.runtime
                        .restrict_draft_authorization(id, expiry, now_ms())
                        .unwrap();
                    peer.link
                        .send_route_status(peer.runtime.route(id).unwrap())
                        .unwrap();
                }
                ReceivedControlMessage::PortLeaseRequest(request) => break request,
                _ => panic!("lease must precede Start"),
            }
        };
        assert_eq!(request.request().route_id, id);
        if answer {
            // An independently consented existing owner holds the exclusive Source.
            let route = peer.runtime.route(id).unwrap().clone();
            let competing = peer
                .runtime
                .create_route(route.session_id, route.source, route.sink, route.backend)
                .unwrap();
            let now = now_ms();
            peer.consent
                .enqueue(&mut peer.runtime, competing, now)
                .unwrap();
            peer.consent
                .resolve(&mut peer.runtime, competing, true, now, now + 5000)
                .unwrap();
            peer.runtime
                .acquire_route_port_lease(competing, route.source, now + 4000, now)
                .unwrap();
            peer.link
                .answer_port_lease_request(&mut peer.runtime, request, now_ms())
                .unwrap();
            assert!(
                peer.runtime
                    .route_port_lease_claim(id, route.source)
                    .is_none()
            );
        }
        // Missing responses have the same bounded closed outcome, never Start.
        failed(&mut process);
        no_children(&process);
    }
}

#[cfg(feature = "touchpad-test-fixture")]
mod software_fixture {
    use super::*;
    use capyio_android_host::{
        AndroidMotionAction, AndroidMotionSample, AndroidPointerSample, AndroidToolType,
        AndroidTouchSurface, AndroidTouchpadCaptureSession,
    };
    use capyio_control_tls::{
        ControlTlsServerIdentity,
        touchpad::{TouchpadTlsBinding, TouchpadTlsChannel, TouchpadTlsListener},
    };
    use capyio_input::TouchpadFrame;
    use capyio_linux_touchpad::{InputEvent, linux_touchpad_descriptor};
    use capyio_protocol::TouchpadDataOffer;
    use capyio_remote_touchpad_adapter::{
        PrivateTouchpadPacketCodecV1, PrivateTouchpadTransportCodecV1,
        secure_receiver::{touchpad_record_binding, touchpad_tls_stream},
    };
    use std::{collections::BTreeMap, net::TcpListener, sync::mpsc};

    fn fixture_pending(mode: &str) -> (Process, Peer, Value) {
        fixture_pending_input(mode, false)
    }

    fn fixture_pending_input(mode: &str, pretyped: bool) -> (Process, Peer, Value) {
        let fixture = Fixture::new();
        let console_mode = match std::env::var("CAPYIO_TOUCHPAD_CONSOLE") {
            Ok(value) => {
                assert_eq!(value, "1", "explicit console fixture mode is 1 only");
                true
            }
            Err(std::env::VarError::NotPresent) => false,
            Err(_) => panic!("invalid console fixture environment"),
        };
        assert!(
            !pretyped || console_mode,
            "pretyped input requires actual console"
        );
        let mut process = match std::env::var("CAPYIO_TOUCHPAD_PAIRED_STORE") {
            Ok(value) => {
                assert_eq!(value, "1", "explicit paired fixture mode is 1 only");
                #[cfg(feature = "paired-store")]
                {
                    if console_mode {
                        console_process(&fixture, mode, pretyped)
                    } else {
                        paired_process(&fixture, mode, false)
                    }
                }
                #[cfg(not(feature = "paired-store"))]
                {
                    panic!("paired-store fixture mode requires the paired-store feature")
                }
            }
            Err(std::env::VarError::NotPresent) => {
                assert!(
                    !console_mode,
                    "console requires explicit protected paired-store fixture"
                );
                let mut process = Process::start(&[
                    "linux-touchpad-fixture",
                    "--trusted-lab",
                    "--fixture-mode",
                    mode,
                ]);
                fixture.bootstrap(&mut process);
                process
            }
            Err(_) => panic!("invalid paired-store fixture environment"),
        };
        let mut peer = fixture.connect(&mut process);
        let approval = process.event("approval_required");
        #[cfg(feature = "paired-store")]
        if process.is_console() {
            assert_console_consent(&mut process, &approval);
        }
        peer.idle(&process);
        assert_no_open(&mut process);
        (process, peer, approval)
    }

    #[cfg(feature = "paired-store")]
    fn console_process(fixture: &Fixture, mode: &str, pretyped: bool) -> Process {
        let local = serde_json::from_value(fixture.config["node_id"].clone()).unwrap();
        let store =
            paired_store::PersistedPairing::new(local, &fixture.directory, &fixture.credentials);
        let (child, console) = playback_console::Console::start_touchpad_fixture(
            &store,
            &fixture.config,
            pretyped,
            mode,
        );
        Process {
            child,
            console: Some(console),
            stdout: vec![],
            stderr: vec![],
            helper_pids: std::collections::BTreeMap::new(),
            paired_store: Some(store),
        }
    }

    #[cfg(feature = "paired-store")]
    fn assert_console_consent(process: &mut Process, approval: &Value) {
        process.wait_console_prompt(
            approval["request_id"].as_str().unwrap(),
            approval["route_id"].as_str().unwrap(),
        );
        let console = process.console.as_ref().unwrap();
        let store = process.paired_store.as_ref().unwrap();
        console.assert_hidden();
        assert_eq!(approval["source"]["node_id"], json!(store.peer));
        assert_eq!(approval["sink"]["node_id"], json!(store.owner));
        assert_eq!(approval["format"], "touchpad-frame-v1");
        assert_eq!(approval["media_security"], "tls13_mtls_v1");
        assert_eq!(approval["duration_secs"], 10);
        assert_eq!(approval["approval_timeout_secs"], 30);
        assert_eq!(approval["max_contacts"], 5);
        assert_eq!(approval["width_mm"], 120);
        assert_eq!(approval["height_mm"], 70);
        assert_eq!(approval["clickable"], false);
        assert_eq!(approval["pressure"], true);
        console.assert_presentation(approval);
        assert!(
            String::from_utf8_lossy(&console.tty)
                .contains("SOFTWARE-ONLY TOUCHPAD FIXTURE: no OS device or gesture acceptance.")
        );
        for pid in [process.child.id(), process.owner_pid()] {
            for name in ["cmdline", "environ"] {
                store.assert_no_leak(&std::fs::read(format!("/proc/{pid}/{name}")).unwrap());
            }
        }
    }

    #[cfg(feature = "paired-store")]
    pub(super) fn console_fresh_exact_terminal_consent() {
        for scenario in [
            "no_approval",
            "pretyped",
            "mismatched_request",
            "mismatched_route",
            "mismatched_peer",
        ] {
            let (mut process, mut peer, approval) =
                fixture_pending_input("normal", scenario == "pretyped");
            assert!(process.is_console(), "actual console is mandatory");
            assert!(peer.runtime.snapshot().routes.is_empty());
            assert!(peer.runtime.snapshot().port_leases.is_empty());
            if scenario.starts_with("mismatched_") {
                let request = if scenario == "mismatched_request" {
                    MessageId::new().to_string()
                } else {
                    approval["request_id"].as_str().unwrap().to_owned()
                };
                let route = if scenario == "mismatched_route" {
                    RouteId::new().to_string()
                } else {
                    approval["route_id"].as_str().unwrap().to_owned()
                };
                let console = process.console.as_mut().unwrap();
                let peer = if scenario == "mismatched_peer" {
                    NodeId::new()
                } else {
                    console.peer
                };
                let typed = format!("start {request} {route} {peer}");
                let before = console.tty.len();
                console.send(format!("{typed}\n").as_bytes());
                assert!(
                    !process.finish().success(),
                    "stale/wrong terminal authority fails closed"
                );
                assert!(
                    !String::from_utf8_lossy(&process.console.as_ref().unwrap().tty[before..])
                        .contains(&typed),
                    "typed input must not echo"
                );
            } else {
                process.command(json!({"version":1,"command":"status"}));
                assert_eq!(process.event("status")["stage"], "awaiting_local_approval");
                process.wait_console_prompt(
                    approval["request_id"].as_str().unwrap(),
                    approval["route_id"].as_str().unwrap(),
                );
                peer.idle(&process);
                process.command(json!({"version":1,"command":"stop"}));
                terminal(&mut process, "stopped");
            }
            assert_no_open(&mut process);
            no_children(&process);
            assert!(
                peer.link
                    .receive_control_message_until(Instant::now() + BOUND)
                    .is_err(),
                "no proposal before fresh exact terminal consent"
            );
            assert!(peer.runtime.snapshot().routes.is_empty());
            assert!(peer.runtime.snapshot().port_leases.is_empty());
            eprintln!(
                "touchpad_console_consent_case={scenario} actual_tty=true unapproved=true helper_open=false owned_cleanup=true"
            );
        }
    }

    #[cfg(feature = "paired-store")]
    fn paired_process(fixture: &Fixture, mode: &str, revoked: bool) -> Process {
        let local = serde_json::from_value(fixture.config["node_id"].clone()).unwrap();
        let store =
            paired_store::PersistedPairing::new(local, &fixture.directory, &fixture.credentials);
        if revoked {
            store.revoke();
        }
        let mut args = store.args("linux-touchpad-fixture");
        args.extend(["--fixture-mode".into(), mode.into()]);
        let borrowed: Vec<_> = args.iter().map(String::as_str).collect();
        let mut process = Process::start(&borrowed);
        process.paired_store = Some(store);
        process.command(fixture.config.clone());
        process.command(json!({"version":1,"credential_source":"paired-store"}));
        process
    }

    #[cfg(feature = "paired-store")]
    #[test]
    fn ipc_linux_touchpad_paired_store_approval_pin_and_revocation_boundary() {
        // Real persisted key loading and pinned mTLS still confer no resource
        // authority: no local approval, Source grant, lease or native helper.
        let fixture = Fixture::new();
        let mut process = paired_process(&fixture, "normal", false);
        let mut peer = fixture.connect(&mut process);
        let approval = process.event("approval_required");
        assert_eq!(approval["media_security"], "tls13_mtls_v1");
        peer.idle(&process);
        assert_no_open(&mut process);
        process.command(json!({"version":1,"command":"stop"}));
        terminal(&mut process, "stopped");
        assert_no_open(&mut process);

        // Both the stored peer pin and the remote endpoint pin remain exact.
        for stored_peer_pin in [false, true] {
            let mut fixture = Fixture::new();
            if stored_peer_pin {
                let local = serde_json::from_value(fixture.config["node_id"].clone()).unwrap();
                let original = PairedPeerDirectory::decode_json(local, &fixture.directory).unwrap();
                let mut record = original.active_peers().next().unwrap().clone();
                record.peer_certificate_sha256 = identities().2;
                let mut directory = PairedPeerDirectory::new(local).unwrap();
                directory.add(record).unwrap();
                fixture.directory = directory.encode_json().unwrap();
            } else {
                fixture.client.expected_peer_certificate = identities().1.expected_peer_certificate;
            }
            let mut process = paired_process(&fixture, "normal", false);
            let listening = process.event("listening");
            let nodes = ControlTlsNodeBinding::new(
                serde_json::from_value(fixture.config["node_id"].clone()).unwrap(),
                fixture.source.node_id,
            )
            .unwrap();
            assert!(
                AuthenticatedControlClientChannel::connect_fresh_session(
                    ControlTlsEndpoint {
                        address: listening["address"].as_str().unwrap().parse().unwrap(),
                        io_timeout: BOUND
                    },
                    "localhost",
                    fixture.client,
                    nodes,
                )
                .is_err()
            );
            failed(&mut process);
            assert_no_open(&mut process);
        }

        // Revocation already committed before launch blocks even the listener.
        let fixture = Fixture::new();
        let mut process = paired_process(&fixture, "normal", true);
        failed(&mut process);
        assert_no_open(&mut process);
    }

    fn approve(process: &mut Process, peer: &mut Peer, approval: &Value, ttl: u64) -> RouteId {
        process.command(approval_command(approval));
        let id = serde_json::from_value(approval["route_id"].clone()).unwrap();
        peer.proposal(id);
        peer.idle(process);
        assert_no_open(process);
        peer.grant(id, true, ttl);
        peer.start_route(id);
        no_children(process);
        assert_no_open(process);
        id
    }

    fn listener(peer: &Peer, id: RouteId) -> (TouchpadTlsListener, TouchpadTlsBinding) {
        listener_identity(peer, id, &peer.client)
    }
    fn listener_identity(
        peer: &Peer,
        id: RouteId,
        identity: &ControlTlsClientIdentity,
    ) -> (TouchpadTlsListener, TouchpadTlsBinding) {
        let binding = TouchpadTlsBinding::from_route(
            peer.link.tls_binding(),
            peer.runtime.route(id).unwrap(),
            StreamId::new(),
            now_ms(),
        )
        .unwrap();
        let identity = ControlTlsServerIdentity {
            expected_peer_certificate: identity.expected_peer_certificate,
            certificate_chain_der: identity.certificate_chain_der.clone(),
            private_key_der: identity.private_key_der.clone(),
            client_ca_der: identity.server_ca_der.clone(),
        };
        let listener = TouchpadTlsListener::from_listener(
            TcpListener::bind("127.0.0.1:0").unwrap(),
            identity,
            binding,
            Duration::from_secs(1),
        )
        .unwrap();
        (listener, binding)
    }

    fn bind_data(
        process: &mut Process,
        peer: &mut Peer,
        id: RouteId,
    ) -> (TouchpadTlsChannel, TouchpadTlsBinding) {
        let (listener, binding) = listener(peer, id);
        peer.link
            .send_touchpad_data_offer(
                peer.runtime.route(id).unwrap(),
                TouchpadDataOffer {
                    port: listener.local_addr().unwrap().port(),
                    stream_id: binding.stream_id,
                },
            )
            .unwrap();
        let deadline = Instant::now() + BOUND;
        let sender = loop {
            if let Some(channel) = listener.try_accept().unwrap() {
                break channel;
            }
            assert!(Instant::now() < deadline, "data connection not admitted");
            thread::sleep(Duration::from_millis(2));
        };
        let bound = process.event("touchpad_bound");
        assert_eq!(bound["state"], "starting");
        assert_eq!(bound["os_gestures_verified"], false);
        #[cfg(feature = "paired-store")]
        if process.is_console() {
            process.command(json!({"version":1,"command":"status"}));
            let status = process.event("status");
            assert_eq!(
                status["state"], "starting",
                "binding is not committed backend ACK"
            );
            assert_eq!(status["os_gestures_verified"], false);
            assert!(
                !String::from_utf8_lossy(&process.console.as_ref().unwrap().tty)
                    .contains("\"event\":\"touchpad_active\"")
            );
        }
        no_children(process);
        assert_no_open(process);
        (sender, binding)
    }

    fn assert_no_open(process: &mut Process) {
        process.collect();
        let trace = String::from_utf8_lossy(&process.stderr);
        assert!(
            !trace
                .lines()
                .any(|line| line == "CREATE" || line.starts_with("OPEN ")),
            "unexpected native open: {trace}"
        );
    }

    fn hello(sender: &mut TouchpadTlsChannel, binding: TouchpadTlsBinding) {
        sender
            .send_record(
                PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
                    .encode_hello()
                    .as_bytes(),
            )
            .unwrap();
    }

    fn capture(binding: TouchpadTlsBinding) -> AndroidTouchpadCaptureSession {
        AndroidTouchpadCaptureSession::new(
            touchpad_tls_stream(binding),
            AndroidTouchSurface {
                width_px: 1200,
                height_px: 700,
                descriptor: linux_touchpad_descriptor(),
            },
            0,
        )
        .unwrap()
    }

    fn finger(id: u32, x: f32) -> AndroidPointerSample {
        AndroidPointerSample {
            pointer_id: id,
            tool_type: AndroidToolType::Finger,
            x_px: x,
            y_px: 100.0,
            pressure: Some(0.5),
        }
    }
    fn motion(
        action: AndroidMotionAction,
        points: Vec<AndroidPointerSample>,
        nanos: u64,
    ) -> AndroidMotionSample {
        AndroidMotionSample {
            action,
            pointers: points,
            event_time_nanos: nanos,
        }
    }
    fn send_frame(
        sender: &mut TouchpadTlsChannel,
        binding: TouchpadTlsBinding,
        frame: &TouchpadFrame,
    ) {
        let packet = PrivateTouchpadPacketCodecV1::new(
            touchpad_tls_stream(binding),
            linux_touchpad_descriptor(),
        )
        .unwrap()
        .encode(frame)
        .unwrap();
        sender
            .send_record(
                PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
                    .encode_data(&packet)
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
    }
    fn ack(sender: &mut TouchpadTlsChannel, binding: TouchpadTlsBinding, sequence: u64) {
        PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
            .validate_ack(&sender.receive_record().unwrap(), sequence)
            .unwrap();
    }
    fn submit(sender: &mut TouchpadTlsChannel, binding: TouchpadTlsBinding, frame: &TouchpadFrame) {
        send_frame(sender, binding, frame);
        ack(sender, binding, frame.header.sequence);
    }
    fn down(
        process: &mut Process,
        sender: &mut TouchpadTlsChannel,
        binding: TouchpadTlsBinding,
        capture: &mut AndroidTouchpadCaptureSession,
    ) {
        submit(sender, binding, &capture.start(1).unwrap());
        submit(
            sender,
            binding,
            &capture
                .map_motion(&motion(
                    AndroidMotionAction::Down,
                    vec![finger(42, 100.0)],
                    2,
                ))
                .unwrap(),
        );
        process.collect();
    }
    fn has(events: &[InputEvent], event_type: u16, code: u16, value: i32) -> bool {
        events.contains(&InputEvent {
            event_type,
            code,
            value,
        })
    }
    fn events(trace: &str) -> Vec<InputEvent> {
        trace
            .lines()
            .filter_map(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields.first() != Some(&"EVENT") {
                    return None;
                }
                assert_eq!(fields.len(), 4, "bounded exact native trace");
                Some(InputEvent {
                    event_type: fields[1].parse().unwrap(),
                    code: fields[2].parse().unwrap(),
                    value: fields[3].parse().unwrap(),
                })
            })
            .collect()
    }
    fn batches(process: &mut Process) -> BTreeMap<usize, Vec<InputEvent>> {
        process.collect();
        let trace = String::from_utf8_lossy(&process.stderr);
        let mut result = BTreeMap::new();
        let mut pending = None;
        for line in trace.lines() {
            if let Some(number) = line.strip_prefix("FRAME_BEGIN ") {
                pending = Some((number.parse::<usize>().unwrap(), Vec::new()));
            } else if let Some(number) = line.strip_prefix("FRAME_COMMITTED ") {
                let (index, batch) = pending.take().expect("commit without frame begin");
                assert_eq!(index, number.parse::<usize>().unwrap());
                assert!(
                    result.insert(index, batch).is_none(),
                    "no duplicate native commit"
                );
            } else if let Some((_, batch)) = pending.as_mut() {
                batch.extend(events(line));
            }
        }
        result
    }
    fn wait_trace(process: &mut Process, expected: &str) {
        let deadline = Instant::now() + BOUND;
        loop {
            process.collect();
            if String::from_utf8_lossy(&process.stderr)
                .lines()
                .any(|line| line == expected)
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "missing trace {expected}: {}",
                String::from_utf8_lossy(&process.stderr)
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn terminal(process: &mut Process, expected: &str) {
        terminal_not_before(process, expected, None);
    }
    fn terminal_not_before(process: &mut Process, expected: &str, lease_expiry: Option<u64>) {
        loop {
            let event = process.next_event();
            let observed_at_ms = now_ms();
            if event["event"] == "touchpad_active" {
                continue;
            }
            assert_eq!(event["event"], expected);
            if let Some(expiry) = lease_expiry {
                // Capture when the actual terminal event is read, before any
                // controller/Node/helper reap wait can conceal early failure.
                assert!(
                    observed_at_ms >= expiry,
                    "console reported terminal failure before exact Source lease expiry"
                );
            }
            break;
        }
        assert_eq!(
            process.finish().code(),
            Some(if expected == "stopped" { 0 } else { 1 })
        );
        let trace = String::from_utf8_lossy(&process.stderr);
        let errors: Vec<_> = trace
            .lines()
            .filter(|line| line.starts_with("Error:"))
            .collect();
        if expected == "failed" {
            assert_eq!(errors, ["Error: Linux touchpad launch failed"]);
        } else {
            assert!(errors.is_empty());
        }
        #[cfg(feature = "paired-store")]
        if process.is_console() {
            let console_errors: Vec<_> = trace
                .lines()
                .filter(|line| line.starts_with("Linux console "))
                .collect();
            if expected == "failed" {
                assert_eq!(
                    console_errors,
                    [
                        "Linux console operation failed; no automatic retry. Check explicit selectors, paired-store build and interactive terminal. Pairing persistence may require inspection."
                    ]
                );
            } else {
                assert!(console_errors.is_empty());
            }
        }
        no_children(process);
    }
    fn destroyed(process: &mut Process) {
        process.collect();
        let trace = String::from_utf8_lossy(&process.stderr);
        assert_eq!(trace.lines().filter(|line| *line == "CREATE").count(), 1);
        assert_eq!(trace.lines().filter(|line| *line == "DESTROY").count(), 1);
        let (_, after) = trace.split_once("DESTROY\n").unwrap();
        assert!(
            !after.contains("EVENT "),
            "no events after device destruction"
        );
        let cleanup = trace
            .rsplit_once("CLEANUP_BEGIN\n")
            .expect("teardown phase must be observed")
            .1;
        let cleanup = cleanup.split_once("DESTROY\n").unwrap().0;
        let mut expected = Vec::new();
        for code in [0x14a, 0x145, 0x14d, 0x14e, 0x14f, 0x148] {
            expected.push(InputEvent {
                event_type: 1,
                code,
                value: 0,
            });
        }
        for slot in 0..5 {
            expected.push(InputEvent {
                event_type: 3,
                code: 0x2f,
                value: slot,
            });
            expected.push(InputEvent {
                event_type: 3,
                code: 0x39,
                value: -1,
            });
        }
        expected.push(InputEvent {
            event_type: 3,
            code: 0x18,
            value: 0,
        });
        expected.push(InputEvent {
            event_type: 0,
            code: 0,
            value: 0,
        });
        assert_eq!(
            events(cleanup),
            expected,
            "final cleanup must release all slots, touch/tool keys and pressure before SYN_REPORT"
        );
        assert_eq!(
            process.helper_pids.len(),
            1,
            "observe exactly one owned helper before teardown"
        );
        for pid in process.helper_pids.keys() {
            assert!(
                !std::path::Path::new(&format!("/proc/{pid}")).exists(),
                "owned helper {pid} must be reaped, not orphaned or zombied"
            );
        }
        let before = process.stderr.clone();
        thread::sleep(Duration::from_millis(40));
        process.collect();
        assert_eq!(
            process.stderr, before,
            "no delayed helper work after terminal outcome"
        );
    }

    #[test]
    pub(super) fn ipc_linux_touchpad_five_slots_tracking_cancel_gap_and_android_policy() {
        let (mut process, mut peer, approval) = fixture_pending("normal");
        let id = approve(&mut process, &mut peer, &approval, 10_000);
        let (mut sender, binding) = bind_data(&mut process, &mut peer, id);
        hello(&mut sender, binding);
        if process.is_console() {
            wait_trace(&mut process, "CREATE");
            process.command(json!({"version":1,"command":"status"}));
            let status = process.event("status");
            assert_eq!(
                status["state"], "starting",
                "Hello/helper creation is not backend delivery"
            );
            assert!(
                batches(&mut process).is_empty(),
                "no native frame commit before payload"
            );
        }
        let mut capture = capture(binding);
        submit(&mut sender, binding, &capture.start(1).unwrap());
        assert!(
            batches(&mut process).contains_key(&1),
            "first authenticated ACK follows complete native commit"
        );
        if process.is_console() {
            let active = process.event("touchpad_active");
            assert_eq!(active["state"], "active");
            assert_eq!(active["os_gestures_verified"], false);
        }
        for count in 1..=5 {
            let action = if count == 1 {
                AndroidMotionAction::Down
            } else {
                AndroidMotionAction::PointerDown {
                    action_index: count - 1,
                }
            };
            let points = (0..count)
                .map(|i| finger(i as u32, 100.0 + i as f32 * 50.0))
                .collect();
            submit(
                &mut sender,
                binding,
                &capture
                    .map_motion(&motion(action, points, count as u64 + 1))
                    .unwrap(),
            );
        }
        let points = || (0..5).map(|i| finger(i, 110.0 + i as f32 * 50.0)).collect();
        let moved = capture
            .map_motion(&motion(AndroidMotionAction::Move, points(), 7))
            .unwrap();
        assert_eq!(
            moved.contacts[0].position.x_himetric, 1070,
            "real recommended Android 3+-contact gain"
        );
        submit(&mut sender, binding, &moved);
        let committed = batches(&mut process);
        let five = &committed[&6];
        assert!(has(five, 1, 0x148, 1) && has(five, 1, 0x14a, 1));
        assert!(has(five, 3, 0x3a, 32768));
        let slots: BTreeSet<_> = five
            .iter()
            .filter(|e| e.event_type == 3 && e.code == 0x2f)
            .map(|e| e.value)
            .collect();
        assert_eq!(slots, BTreeSet::from([0, 1, 2, 3, 4]));
        let tracking: BTreeSet<_> = committed
            .values()
            .flat_map(|b| b.iter())
            .filter(|e| e.event_type == 3 && e.code == 0x39 && e.value >= 0)
            .map(|e| e.value)
            .collect();
        assert_eq!(tracking, BTreeSet::from([0, 1, 2, 3, 4]));
        assert!(has(&committed[&7], 3, 0x35, 1070));
        assert!(
            !committed[&7]
                .iter()
                .any(|e| e.event_type == 3 && e.code == 0x39),
            "motion preserves tracking lifetimes"
        );

        // Drop one genuine Android-produced sequence. The complete next snapshot
        // cannot resurrect contacts until a real producer CancelAll arrives.
        let _dropped = capture
            .map_motion(&motion(AndroidMotionAction::Move, points(), 8))
            .unwrap();
        submit(
            &mut sender,
            binding,
            &capture
                .map_motion(&motion(AndroidMotionAction::Move, points(), 9))
                .unwrap(),
        );
        submit(
            &mut sender,
            binding,
            &capture
                .map_motion(&motion(AndroidMotionAction::Move, points(), 10))
                .unwrap(),
        );
        let committed = batches(&mut process);
        assert_eq!(
            committed[&8]
                .iter()
                .filter(|e| e.event_type == 3 && e.code == 0x39 && e.value == -1)
                .count(),
            5
        );
        assert!(has(&committed[&9], 1, 0x14a, 0));
        assert!(
            !committed[&9]
                .iter()
                .any(|e| e.event_type == 3 && e.code == 0x39 && e.value >= 0)
        );
        submit(
            &mut sender,
            binding,
            &capture
                .map_motion(&motion(AndroidMotionAction::Cancel, Vec::new(), 11))
                .unwrap(),
        );
        submit(
            &mut sender,
            binding,
            &capture
                .map_motion(&motion(
                    AndroidMotionAction::Down,
                    vec![finger(0, 200.0)],
                    12,
                ))
                .unwrap(),
        );
        assert!(
            has(&batches(&mut process)[&11], 3, 0x39, 5),
            "reused Android pointer gets fresh Linux tracking ID"
        );
        submit(
            &mut sender,
            binding,
            &capture
                .map_motion(&motion(AndroidMotionAction::Cancel, Vec::new(), 13))
                .unwrap(),
        );
        let cancelled = &batches(&mut process)[&12];
        assert!(has(cancelled, 1, 0x14a, 0));
        assert_eq!(
            cancelled
                .iter()
                .filter(|e| e.event_type == 3 && e.code == 0x39 && e.value == -1)
                .count(),
            1
        );
        submit(
            &mut sender,
            binding,
            &capture
                .map_motion(&motion(
                    AndroidMotionAction::Down,
                    vec![finger(0, 200.0)],
                    14,
                ))
                .unwrap(),
        );
        assert!(has(&batches(&mut process)[&13], 3, 0x39, 6));
        if !process.is_console() {
            process.event("touchpad_active");
        }
        process.command(json!({"version":1,"command":"status"}));
        let status = process.event("status");
        assert_eq!(status["state"], "active");
        assert_eq!(status["os_gestures_verified"], false);
        process.command(json!({"version":1,"command":"stop"}));
        // Explicit interruption is not reported as a successful native cleanup.
        terminal(&mut process, "failed");
        destroyed(&mut process);
        for batch in batches(&mut process).values() {
            assert!((1..=64).contains(&batch.len()));
            assert_eq!(
                batch.last(),
                Some(&InputEvent {
                    event_type: 0,
                    code: 0,
                    value: 0
                })
            );
        }
    }

    #[test]
    pub(super) fn ipc_linux_touchpad_mismatched_hello_never_opens_helper() {
        let (mut process, mut peer, approval) = fixture_pending("normal");
        let id = approve(&mut process, &mut peer, &approval, 5000);
        let (mut sender, binding) = bind_data(&mut process, &mut peer, id);
        let mut wrong = touchpad_record_binding(binding);
        wrong.route_epoch += 1;
        sender
            .send_record(
                PrivateTouchpadTransportCodecV1::new(wrong)
                    .encode_hello()
                    .as_bytes(),
            )
            .unwrap();
        terminal(&mut process, "failed");
        assert_no_open(&mut process);
    }

    #[test]
    pub(super) fn ipc_linux_touchpad_native_failure_and_backpressure_never_acknowledge_uncertain_commit()
     {
        for mode in ["partial-write", "backpressure"] {
            let (mut process, mut peer, approval) = fixture_pending(mode);
            let id = approve(&mut process, &mut peer, &approval, 5000);
            let (mut sender, binding) = bind_data(&mut process, &mut peer, id);
            hello(&mut sender, binding);
            let mut capture = capture(binding);
            down(&mut process, &mut sender, binding, &mut capture);
            let frame = capture
                .map_motion(&motion(
                    AndroidMotionAction::Move,
                    vec![finger(42, 120.0)],
                    3,
                ))
                .unwrap();
            let started = Instant::now();
            send_frame(&mut sender, binding, &frame);
            assert!(
                sender.receive_record().is_err(),
                "uncertain native commit cannot ACK"
            );
            terminal(&mut process, "failed");
            assert!(started.elapsed() < BOUND);
            assert!(!batches(&mut process).contains_key(&3));
            destroyed(&mut process);
        }
    }

    #[test]
    pub(super) fn ipc_linux_touchpad_ack_waits_for_complete_native_commit() {
        let (mut process, mut peer, approval) = fixture_pending("delayed-commit");
        let id = approve(&mut process, &mut peer, &approval, 5000);
        let (mut sender, binding) = bind_data(&mut process, &mut peer, id);
        hello(&mut sender, binding);
        let mut capture = capture(binding);
        down(&mut process, &mut sender, binding, &mut capture);
        let frame = capture
            .map_motion(&motion(
                AndroidMotionAction::Move,
                vec![finger(42, 120.0)],
                3,
            ))
            .unwrap();
        send_frame(&mut sender, binding, &frame);
        wait_trace(&mut process, "FRAME_BEGIN 3");
        let (send, receive) = mpsc::channel();
        let stop = sender.stop_handle().unwrap();
        let reader = thread::spawn(move || {
            let result = sender.receive_record();
            let _ = send.send(result);
            sender
        });
        let early = receive.recv_timeout(Duration::from_millis(25));
        let no_early = matches!(early, Err(mpsc::RecvTimeoutError::Timeout));
        if !no_early {
            stop.stop();
        }
        let result = if no_early {
            receive.recv_timeout(BOUND).unwrap()
        } else {
            early.unwrap()
        };
        let mut sender = reader.join().unwrap();
        assert!(no_early, "ACK arrived while backend commit was blocked");
        PrivateTouchpadTransportCodecV1::new(touchpad_record_binding(binding))
            .validate_ack(&result.unwrap(), frame.header.sequence)
            .unwrap();
        assert!(batches(&mut process).contains_key(&3));
        process.command(json!({"version":1,"command":"stop"}));
        // Explicit interruption is not reported as a successful native cleanup.
        terminal(&mut process, "failed");
        destroyed(&mut process);
        assert!(sender.receive_record().is_err());
    }

    #[test]
    pub(super) fn ipc_linux_touchpad_create_failure_has_no_native_open_or_ack() {
        let (mut process, mut peer, approval) = fixture_pending("fail-create");
        let id = approve(&mut process, &mut peer, &approval, 5000);
        let (mut sender, binding) = bind_data(&mut process, &mut peer, id);
        hello(&mut sender, binding);
        assert!(sender.receive_record().is_err());
        terminal(&mut process, "failed");
        assert_no_open(&mut process);
    }

    #[test]
    pub(super) fn ipc_linux_touchpad_control_eof_data_eof_local_eof_and_expiry_release_owned_contacts()
     {
        for cause in [
            "control-stop",
            "source-revoke",
            "control-eof",
            "data-eof",
            "local-eof",
            "expiry",
        ] {
            let (mut process, mut peer, approval) = fixture_pending("normal");
            let id = approve(
                &mut process,
                &mut peer,
                &approval,
                if cause == "expiry" { 3000 } else { 5000 },
            );
            let (mut sender, binding) = bind_data(&mut process, &mut peer, id);
            hello(&mut sender, binding);
            let mut capture = capture(binding);
            down(&mut process, &mut sender, binding, &mut capture);
            process.event("touchpad_active");
            match cause {
                "control-stop" => peer
                    .link
                    .send_route_command_until(id, RouteCommandAction::Stop, Instant::now() + BOUND)
                    .unwrap(),
                "source-revoke" => {
                    peer.runtime
                        .release_route_port_lease(id, binding.source, binding.session.session_id)
                        .unwrap();
                    peer.runtime.begin_route_stop(id).unwrap();
                    peer.runtime.stop_route(id).unwrap();
                    peer.link
                        .send_route_status(peer.runtime.route(id).unwrap())
                        .unwrap();
                }
                "control-eof" => peer.link.close(),
                "data-eof" => sender.close(),
                "local-eof" => process.close_input(),
                "expiry" => {
                    let lease = peer
                        .runtime
                        .route_port_lease_claim(id, binding.source)
                        .unwrap()
                        .expires_at_ms;
                    let mut nanos = 3;
                    // Refresh within the channel's one-second I/O bound, then
                    // stop before finite authority expires. Idle cannot explain
                    // terminal cleanup less than one second after this last ACK.
                    while lease.saturating_sub(now_ms()) > 400 {
                        thread::sleep(Duration::from_millis(100));
                        submit(
                            &mut sender,
                            binding,
                            &capture
                                .map_motion(&motion(
                                    AndroidMotionAction::Move,
                                    vec![finger(42, 120.0)],
                                    nanos,
                                ))
                                .unwrap(),
                        );
                        nanos += 1;
                        process.collect();
                    }
                }
                _ => unreachable!(),
            }
            if cause == "local-eof" {
                // Legacy closes LauncherIo's input. The actual console instead
                // receives typed Control-D and owns Stop/EOF plus bounded reap.
                assert_eq!(process.finish().code(), Some(1));
                let diagnostics: Vec<_> = String::from_utf8_lossy(&process.stderr)
                    .lines()
                    .filter(|line| line.starts_with("Error:") || line.starts_with("Linux console "))
                    .map(str::to_owned)
                    .collect();
                if process.is_console() {
                    let controller = "Linux console operation failed; no automatic retry. Check explicit selectors, paired-store build and interactive terminal. Pairing persistence may require inspection.";
                    // Drop may complete Stop before EOF or interrupt the Node's
                    // input path. Both exact outcomes still require one static
                    // console failure and all the same contact/reap assertions.
                    assert!(
                        diagnostics == [controller]
                            || diagnostics == ["Error: Linux touchpad launch failed", controller],
                        "unexpected console EOF diagnostics: {diagnostics:?}"
                    );
                } else {
                    assert_eq!(diagnostics, ["Error: Linux touchpad launch failed"]);
                }
                no_children(&process);
            } else if cause == "expiry" && process.is_console() {
                let expiry = peer
                    .runtime
                    .route_port_lease_claim(id, binding.source)
                    .unwrap()
                    .expires_at_ms;
                terminal_not_before(&mut process, "failed", Some(expiry));
            } else {
                terminal(&mut process, "failed");
            }
            if cause == "expiry" {
                let lease = peer
                    .runtime
                    .route_port_lease_claim(id, binding.source)
                    .unwrap();
                assert!(lease.expires_at_ms < binding.expires_at_ms);
                assert!(
                    now_ms() >= lease.expires_at_ms,
                    "finite Source lease, not idle, ends this owner"
                );
            }
            destroyed(&mut process);
            assert!(sender.receive_record().is_err());
        }
    }
    #[test]
    pub(super) fn ipc_linux_touchpad_wrong_data_pin_and_changed_route_offer_fail_before_open() {
        for wrong_pin in [false, true] {
            let (mut process, mut peer, approval) = fixture_pending("normal");
            let id = approve(&mut process, &mut peer, &approval, 5000);
            let (listener, binding) = if wrong_pin {
                assert_eq!(peer.client.server_ca_der, peer.replacement.server_ca_der);
                assert_ne!(
                    peer.client.certificate_chain_der[0],
                    peer.replacement.certificate_chain_der[0]
                );
                // Same trusted issuer and DNS name, but not the paired exact leaf.
                listener_identity(&peer, id, &peer.replacement)
            } else {
                listener(&peer, id)
            };
            let mut route = peer.runtime.route(id).unwrap().clone();
            if !wrong_pin {
                route.epoch += 1;
            }
            peer.link
                .send_touchpad_data_offer(
                    &route,
                    TouchpadDataOffer {
                        port: listener.local_addr().unwrap().port(),
                        stream_id: binding.stream_id,
                    },
                )
                .unwrap();
            if wrong_pin {
                let deadline = Instant::now() + BOUND;
                loop {
                    match listener.try_accept() {
                        Err(_) => break,
                        Ok(None) => {}
                        Ok(Some(_)) => panic!("replacement unpaired leaf was accepted"),
                    }
                    assert!(Instant::now() < deadline);
                    thread::sleep(Duration::from_millis(2));
                }
            }
            terminal(&mut process, "failed");
            assert_no_open(&mut process);
        }
    }

    #[test]
    pub(super) fn ipc_linux_touchpad_local_stop_interrupts_partial_data_handshake_before_native_open()
     {
        let (mut process, mut peer, approval) = fixture_pending("normal");
        let id = approve(&mut process, &mut peer, &approval, 5000);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        peer.link
            .send_touchpad_data_offer(
                peer.runtime.route(id).unwrap(),
                TouchpadDataOffer {
                    port: listener.local_addr().unwrap().port(),
                    stream_id: StreamId::new(),
                },
            )
            .unwrap();
        let deadline = Instant::now() + BOUND;
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => panic!("private data accept failed: {error}"),
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut tls_prefix = [0; 5];
        socket.read_exact(&mut tls_prefix).unwrap();
        assert_eq!(
            tls_prefix[0], 22,
            "actual TLS ClientHello, not merely a bound port"
        );
        no_children(&process);
        assert_no_open(&mut process);
        let started = Instant::now();
        process.command(json!({"version":1,"command":"stop"}));
        terminal(&mut process, "stopped");
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_no_open(&mut process);
    }
}

#[cfg(all(feature = "touchpad-test-fixture", feature = "paired-store"))]
mod console_fixture {
    use super::software_fixture;

    fn require_actual_console() {
        for name in ["CAPYIO_TOUCHPAD_CONSOLE", "CAPYIO_TOUCHPAD_PAIRED_STORE"] {
            assert_eq!(
                std::env::var(name).as_deref(),
                Ok("1"),
                "explicit hosted PTY and protected-store modes required"
            );
        }
    }

    fn verified(name: &str) {
        eprintln!(
            "capyio_touchpad_console=passed test={name} actual_tty=true paired_store=true software_helper=true owned_cleanup=true"
        );
    }

    #[test]
    #[ignore = "requires explicit disposable actual-console PTY/private Secret Service/software-helper harness"]
    fn hosted_touchpad_console_fresh_exact_terminal_consent() {
        require_actual_console();
        software_fixture::console_fresh_exact_terminal_consent();
        verified("hosted_touchpad_console_fresh_exact_terminal_consent");
    }

    #[test]
    #[ignore = "requires explicit disposable actual-console PTY/private Secret Service/software-helper harness"]
    fn hosted_touchpad_console_five_slots_tracking_cancel_gap_and_android_policy() {
        require_actual_console();
        software_fixture::ipc_linux_touchpad_five_slots_tracking_cancel_gap_and_android_policy();
        verified("hosted_touchpad_console_five_slots_tracking_cancel_gap_and_android_policy");
    }

    #[test]
    #[ignore = "requires explicit disposable actual-console PTY/private Secret Service/software-helper harness"]
    fn hosted_touchpad_console_mismatched_hello_never_opens_helper() {
        require_actual_console();
        software_fixture::ipc_linux_touchpad_mismatched_hello_never_opens_helper();
        verified("hosted_touchpad_console_mismatched_hello_never_opens_helper");
    }

    #[test]
    #[ignore = "requires explicit disposable actual-console PTY/private Secret Service/software-helper harness"]
    fn hosted_touchpad_console_native_failure_and_backpressure_never_acknowledge_uncertain_commit()
    {
        require_actual_console();
        software_fixture::ipc_linux_touchpad_native_failure_and_backpressure_never_acknowledge_uncertain_commit();
        verified(
            "hosted_touchpad_console_native_failure_and_backpressure_never_acknowledge_uncertain_commit",
        );
    }

    #[test]
    #[ignore = "requires explicit disposable actual-console PTY/private Secret Service/software-helper harness"]
    fn hosted_touchpad_console_ack_waits_for_complete_native_commit() {
        require_actual_console();
        software_fixture::ipc_linux_touchpad_ack_waits_for_complete_native_commit();
        verified("hosted_touchpad_console_ack_waits_for_complete_native_commit");
    }

    #[test]
    #[ignore = "requires explicit disposable actual-console PTY/private Secret Service/software-helper harness"]
    fn hosted_touchpad_console_create_failure_has_no_native_open_or_ack() {
        require_actual_console();
        software_fixture::ipc_linux_touchpad_create_failure_has_no_native_open_or_ack();
        verified("hosted_touchpad_console_create_failure_has_no_native_open_or_ack");
    }

    #[test]
    #[ignore = "requires explicit disposable actual-console PTY/private Secret Service/software-helper harness"]
    fn hosted_touchpad_console_control_eof_data_eof_local_eof_and_expiry_release_owned_contacts() {
        require_actual_console();
        software_fixture::ipc_linux_touchpad_control_eof_data_eof_local_eof_and_expiry_release_owned_contacts();
        verified(
            "hosted_touchpad_console_control_eof_data_eof_local_eof_and_expiry_release_owned_contacts",
        );
    }

    #[test]
    #[ignore = "requires explicit disposable actual-console PTY/private Secret Service/software-helper harness"]
    fn hosted_touchpad_console_wrong_data_pin_and_changed_route_offer_fail_before_open() {
        require_actual_console();
        software_fixture::ipc_linux_touchpad_wrong_data_pin_and_changed_route_offer_fail_before_open();
        verified("hosted_touchpad_console_wrong_data_pin_and_changed_route_offer_fail_before_open");
    }

    #[test]
    #[ignore = "requires explicit disposable actual-console PTY/private Secret Service/software-helper harness"]
    fn hosted_touchpad_console_local_stop_interrupts_partial_data_handshake_before_native_open() {
        require_actual_console();
        software_fixture::ipc_linux_touchpad_local_stop_interrupts_partial_data_handshake_before_native_open();
        verified(
            "hosted_touchpad_console_local_stop_interrupts_partial_data_handshake_before_native_open",
        );
    }
}
