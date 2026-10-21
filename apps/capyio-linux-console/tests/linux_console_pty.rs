#![cfg(all(target_os = "linux", feature = "paired-store"))]
//! Hosted-only acceptance through an actual console, its actual sibling Node,
//! and an isolated fake Secret Service. No user TTY, session bus, keyring, or
//! device is opened. These mandatory IPC tests must be compiled, not executed,
//! on restricted executors. Missing hosted prerequisites are failures, not skips.
use capyio_control_credentials::ControlCredentialBundle;
use capyio_core::NodeId;
use capyio_linux_pairing_store::{LinuxPairingStore, SecretServiceBackend};
use capyio_pairing::CertificateFingerprint;
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
#[path = "../../capyio-node/tests/support/enrollment_store.rs"]
mod enrollment_store;
use enrollment_store::{COLLECTION, KeyringState, PrivateService};

const BOUND: Duration = Duration::from_secs(20);
const HOST_NAME: &str = "Synthetic welcome console";
const GUEST_NAME: &str = "Synthetic joining console";
const PIN_PROMPT: &str = "PIN (8 digits; hidden): ";
const PAIRING_COMPLETE: &str = "Pairing completed; no resources granted.";

fn nonblocking(file: &impl AsRawFd) {
    // SAFETY: fcntl only inspects/updates flags on the borrowed, live descriptor.
    unsafe {
        let flags = libc::fcntl(file.as_raw_fd(), libc::F_GETFL);
        assert!(flags >= 0);
        assert_eq!(
            libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK),
            0
        );
    }
}
fn cloexec(file: &File) {
    // SAFETY: the file owns a valid descriptor; no ownership is transferred.
    assert_eq!(
        unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) },
        0
    );
}
fn termios(file: &File) -> libc::termios {
    // SAFETY: tcgetattr initializes the entire termios object on success.
    unsafe {
        let mut attributes = std::mem::zeroed();
        assert_eq!(libc::tcgetattr(file.as_raw_fd(), &mut attributes), 0);
        attributes
    }
}
fn drain(input: &mut impl Read, output: &mut Vec<u8>) {
    let mut bytes = [0; 4096];
    loop {
        match input.read(&mut bytes) {
            Ok(0) => break,
            Ok(n) => output.extend_from_slice(&bytes[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            // Linux PTY masters return EIO when their last slave closes.
            Err(e) if e.raw_os_error() == Some(libc::EIO) => break,
            Err(e) => panic!("synthetic process read failed: {e}"),
        }
        assert!(output.len() <= 1_048_576, "bounded test transcript");
    }
}
fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && bytes.windows(needle.len()).any(|window| window == needle)
}

struct TemporaryDirectory(PathBuf);
impl TemporaryDirectory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "capyio-console-pty-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        Self(root)
    }
}
impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Console {
    child: Child,
    master: Option<File>,
    slave: Option<File>,
    original: Option<libc::termios>,
    tty: Vec<u8>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    challenge_cursor: usize,
    node_pid: Option<u32>,
    invitation_paths: Vec<PathBuf>,
}
impl Console {
    fn start(args: &[String]) -> Self {
        let console = Path::new(env!("CARGO_BIN_EXE_capyio-linux-console"));
        let node = console.parent().unwrap().join("capyio-node");
        assert!(
            node.is_file(),
            "hosted gate must build the paired-store capyio-node sibling before this test"
        );
        Self::spawn(console, args, true, &[])
    }
    fn spawn(binary: &Path, args: &[String], with_tty: bool, prebuffer: &[u8]) -> Self {
        let (mut master, slave) = if with_tty {
            let mut master = -1;
            let mut slave = -1;
            // SAFETY: openpty initializes two distinct owned FDs. Null arguments
            // request a fresh synthetic PTY, never an existing user terminal.
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
            // SAFETY: each successful openpty descriptor is transferred once.
            let pair = unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) };
            cloexec(&pair.0);
            cloexec(&pair.1);
            nonblocking(&pair.0);
            (Some(pair.0), Some(pair.1))
        } else {
            (None, None)
        };
        let original = slave.as_ref().map(termios);
        if let Some(original) = &original {
            assert_ne!(
                original.c_lflag & libc::ECHO,
                0,
                "fixture starts with echo enabled so console must turn it off"
            );
        }
        if !prebuffer.is_empty() {
            master.as_mut().unwrap().write_all(prebuffer).unwrap();
        }
        let slave_fd = slave.as_ref().map(AsRawFd::as_raw_fd);
        let mut command = Command::new(binary);
        command
            .args(args)
            .env_clear()
            .env("RUST_BACKTRACE", "0")
            .env("RUST_LIB_BACKTRACE", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // SAFETY: the post-fork closure uses only async-signal-safe system calls.
        // It attaches only our openpty slave to this new session; no user TTY is
        // opened. The descriptor is live until spawn completes and then CLOEXEC.
        unsafe {
            command.pre_exec(move || {
                if libc::setsid() < 0 {
                    return Err(io::Error::last_os_error());
                }
                if let Some(fd) = slave_fd
                    && libc::ioctl(fd, libc::TIOCSCTTY, 0) < 0
                {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().unwrap();
        nonblocking(child.stdout.as_ref().unwrap());
        nonblocking(child.stderr.as_ref().unwrap());
        Self {
            child,
            master,
            slave,
            original,
            tty: vec![],
            stdout: vec![],
            stderr: vec![],
            challenge_cursor: 0,
            node_pid: None,
            invitation_paths: vec![],
        }
    }
    fn collect(&mut self) {
        if let Some(master) = &mut self.master {
            drain(master, &mut self.tty);
        }
        drain(self.child.stdout.as_mut().unwrap(), &mut self.stdout);
        drain(self.child.stderr.as_mut().unwrap(), &mut self.stderr);
    }
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.tty).replace('\r', "")
    }
    fn wait_text(&mut self, expected: &str) {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
            if self.text().contains(expected) {
                // begin_prompt also flushes input immediately after its write.
                thread::sleep(Duration::from_millis(25));
                return;
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "console exited before expected public prompt {expected:?}; stderr={}",
                String::from_utf8_lossy(&self.stderr)
            );
            assert!(
                Instant::now() < deadline,
                "bounded console prompt {expected:?}"
            );
            thread::sleep(Duration::from_millis(3));
        }
    }
    fn line(&mut self, prefix: &str) -> String {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
            let text = self.text();
            if let Some((_, rest)) = text.split_once(prefix)
                && let Some((value, _)) = rest.split_once('\n')
            {
                return value.to_owned();
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "console exited before invitation field"
            );
            assert!(
                Instant::now() < deadline,
                "bounded private invitation field"
            );
            thread::sleep(Duration::from_millis(3));
        }
    }
    fn send(&mut self, bytes: &[u8]) {
        let deadline = Instant::now() + BOUND;
        let mut offset = 0;
        while offset < bytes.len() {
            match self.master.as_mut().unwrap().write(&bytes[offset..]) {
                Ok(0) => panic!("closed synthetic terminal input"),
                Ok(n) => offset += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    assert!(Instant::now() < deadline, "bounded terminal input");
                    self.collect();
                    thread::sleep(Duration::from_millis(2));
                }
                Err(e) => panic!("synthetic terminal input failed: {e}"),
            }
        }
    }
    fn confirm(&mut self, challenge: &Challenge) {
        self.send(format!("{}\n", challenge.command).as_bytes());
    }
    fn challenge(&mut self, peer: NodeId, name: &str) -> Challenge {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
            let text = self.text();
            let tail = &text[self.challenge_cursor..];
            if let Some(at) = tail.find("Type confirm ")
                && let Some((command, _)) = tail[at + 5..].split_once(" or cancel: ")
            {
                let before = &tail[..at];
                let fields: Vec<_> = command.split_whitespace().collect();
                assert!(matches!(fields.len(), 3 | 4));
                assert_eq!(fields[0], "confirm");
                assert_eq!(fields[2], peer.to_string());
                assert!(before.contains(&format!("Peer: {peer}\n")));
                assert!(before.contains(&format!("Name: {name}\n")));
                assert!(before.contains(&format!("Request: {}\n", fields[1])));
                let digest = fields.get(3).map(|value| (*value).to_owned());
                if let Some(digest) = &digest {
                    assert_eq!(digest.len(), 64);
                    assert!(
                        digest
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    );
                    assert!(before.contains(&format!("Transaction: {digest}\n")));
                }
                let result = Challenge {
                    request: fields[1].to_owned(),
                    command: command.to_owned(),
                    digest,
                };
                self.challenge_cursor = text.len();
                self.assert_hidden();
                thread::sleep(Duration::from_millis(25));
                return result;
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "console exited before fresh challenge"
            );
            assert!(Instant::now() < deadline, "bounded exact challenge");
            thread::sleep(Duration::from_millis(3));
        }
    }
    fn assert_hidden(&self) {
        let attributes = termios(self.slave.as_ref().unwrap());
        assert_eq!(
            attributes.c_lflag & (libc::ECHO | libc::ECHONL),
            0,
            "terminal input must not echo"
        );
    }
    fn assert_actual_node_and_no_pin_in_processes(&mut self, pin: &str) {
        let path = format!(
            "/proc/{}/task/{}/children",
            self.child.id(),
            self.child.id()
        );
        let deadline = Instant::now() + BOUND;
        let pid = loop {
            let ids = fs::read_to_string(&path).unwrap();
            let children: Vec<u32> = ids
                .split_whitespace()
                .map(|id| id.parse().unwrap())
                .collect();
            if !children.is_empty() {
                assert_eq!(
                    children.len(),
                    1,
                    "one owned Node child, no shell or helpers"
                );
                break children[0];
            }
            assert!(Instant::now() < deadline, "bounded sibling child startup");
            thread::sleep(Duration::from_millis(3));
        };
        assert_eq!(
            fs::read_link(format!("/proc/{pid}/exe")).unwrap(),
            Path::new(env!("CARGO_BIN_EXE_capyio-linux-console"))
                .parent()
                .unwrap()
                .join("capyio-node")
                .canonicalize()
                .unwrap()
        );
        self.node_pid = Some(pid);
        for entry in fs::read_dir(format!("/proc/{}/fd", self.child.id())).unwrap() {
            let Ok(path) = fs::read_link(entry.unwrap().path()) else {
                continue;
            };
            if path
                .file_name()
                .is_some_and(|name| name == "invitation.fifo")
                && !self.invitation_paths.contains(&path)
            {
                let metadata = fs::symlink_metadata(&path).unwrap();
                assert!(metadata.file_type().is_fifo());
                assert_eq!(metadata.mode() & 0o7777, 0o600);
                assert_eq!(metadata.nlink(), 1);
                let parent = path.parent().unwrap();
                assert!(
                    parent
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("capyio-pairing-")
                );
                assert_eq!(fs::metadata(parent).unwrap().mode() & 0o7777, 0o700);
                self.invitation_paths.push(path);
            }
        }
        for pid in [self.child.id(), pid] {
            for file in ["cmdline", "environ"] {
                let bytes = fs::read(format!("/proc/{pid}/{file}")).unwrap();
                assert!(
                    !contains(&bytes, pin.as_bytes()),
                    "PIN must not enter controller or Node argv/environment"
                );
            }
        }
    }
    fn remain_pending(&mut self, duration: Duration) {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            self.collect();
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "fresh interaction remains pending"
            );
            assert!(!self.text().contains(PAIRING_COMPLETE));
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn finish(&mut self) -> ExitStatus {
        let deadline = Instant::now() + BOUND;
        loop {
            self.collect();
            if let Some(status) = self.child.try_wait().unwrap() {
                self.collect();
                if let (Some(slave), Some(original)) = (&self.slave, &self.original) {
                    let after = termios(slave);
                    assert_eq!(
                        after.c_lflag, original.c_lflag,
                        "terminal local flags restored on every exit"
                    );
                    assert_eq!(
                        after.c_iflag, original.c_iflag,
                        "terminal input flags restored on every exit"
                    );
                    assert_eq!(
                        after.c_oflag, original.c_oflag,
                        "terminal output flags restored on every exit"
                    );
                    assert_eq!(
                        after.c_cc, original.c_cc,
                        "terminal control characters restored on every exit"
                    );
                }
                if let Some(pid) = self.node_pid {
                    assert!(
                        !Path::new(&format!("/proc/{pid}")).exists(),
                        "owned Node must be reaped before controller exits"
                    );
                }
                for path in &self.invitation_paths {
                    assert!(!path.exists(), "owned invitation FIFO must be removed");
                    assert!(
                        !path.parent().unwrap().exists(),
                        "owned invitation directory must be removed"
                    );
                }
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "bounded console exit and child reap"
            );
            thread::sleep(Duration::from_millis(3));
        }
    }
    fn no_pin_logs(&self, pin: &str) {
        assert!(
            self.stdout.is_empty(),
            "ordinary stdout is never the secret or presentation channel"
        );
        assert!(
            !contains(&self.stderr, pin.as_bytes()),
            "PIN leaked to ordinary stderr"
        );
    }
}
impl Drop for Console {
    fn drop(&mut self) {
        // SAFETY: each fixture owns the new process group created by setsid;
        // negative PID targets only that group, including any surviving sibling.
        if self.child.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
struct Challenge {
    request: String,
    command: String,
    digest: Option<String>,
}
struct Invitation {
    endpoint: String,
    pin: String,
}
impl Invitation {
    fn assert_closed(&self) {
        let endpoint: SocketAddr = self.endpoint.parse().unwrap();
        assert!(
            TcpStream::connect_timeout(&endpoint, Duration::from_millis(250)).is_err(),
            "enrollment cannot leave a listener or silently restart"
        );
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
    fn args(&self, command: &[&str]) -> Vec<String> {
        let mut args = vec![
            "--pairing-state".into(),
            self.state_path.display().to_string(),
            "--pairing-node".into(),
            self.owner.to_string(),
            "--pairing-bus-address".into(),
            self.service.address.clone(),
            "--pairing-collection".into(),
            COLLECTION.into(),
        ];
        args.extend(command.iter().map(|value| (*value).to_owned()));
        args
    }
    fn offline_args(&self, command: &[&str]) -> Vec<String> {
        let mut args = self.args(command);
        let index = args
            .iter()
            .position(|arg| arg == "--pairing-bus-address")
            .unwrap()
            + 1;
        args[index] = format!("unix:path={}/absent", self.service.root.display());
        args
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
    fn counts(&self) -> (usize, usize, usize, usize, usize) {
        let state = self.service.state.lock().unwrap();
        (
            state.sessions,
            state.collection_property_reads,
            state.reads,
            state.creates,
            state.forbidden,
        )
    }
    fn no_credential_leaks(&self, process: &Console) {
        let state = self.service.state.lock().unwrap();
        if state.bytes.is_empty() {
            return;
        }
        let (_, key, _) = ControlCredentialBundle::decode(&state.bytes)
            .unwrap()
            .into_parts();
        let key_hex: String = key.iter().map(|byte| format!("{byte:02x}")).collect();
        for output in [&process.tty, &process.stdout, &process.stderr] {
            assert!(!contains(output, &state.bytes));
            assert!(!contains(output, &key));
            assert!(!contains(output, key_hex.as_bytes()));
        }
        for entry in fs::read_dir(&self.state_path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                let bytes = fs::read(entry.path()).unwrap();
                assert!(!contains(&bytes, &key));
                assert!(!contains(&bytes, key_hex.as_bytes()));
            }
        }
    }
}
fn welcome(fixture: &Fixture) -> (Console, Invitation) {
    let mut process =
        Console::start(&fixture.args(&["welcome", "--name", HOST_NAME, "--bind", "127.0.0.1:0"]));
    let endpoint = process.line("Invitation endpoint: ");
    let pin = process.line("Invitation PIN: ");
    let expiry = process.line("Expires in: ");
    let seconds: u64 = expiry.strip_suffix(" seconds").unwrap().parse().unwrap();
    assert!((1..180).contains(&seconds));
    assert_eq!(pin.len(), 8);
    assert!(pin.bytes().all(|byte| byte.is_ascii_digit()));
    let address: SocketAddr = endpoint.parse().unwrap();
    assert!(address.ip().is_loopback());
    assert_ne!(address.port(), 0);
    process.assert_actual_node_and_no_pin_in_processes(&pin);
    assert_eq!(
        process.invitation_paths.len(),
        1,
        "one private owned invitation FIFO"
    );
    fixture.empty();
    (process, Invitation { endpoint, pin })
}
fn join(fixture: &Fixture, invitation: &Invitation, extra: &str) -> Console {
    let mut process = Console::start(&fixture.args(&[
        "join",
        "--name",
        GUEST_NAME,
        "--endpoint",
        &invitation.endpoint,
    ]));
    process.wait_text(PIN_PROMPT);
    process.assert_hidden();
    process.send(format!("{}\n{extra}", invitation.pin).as_bytes());
    process
}
fn complete_pair(host: &Fixture, guest: &Fixture) -> (Console, Console, Invitation) {
    let (mut host_process, invitation) = welcome(host);
    let mut guest_process = join(guest, &invitation, "");
    let host_challenge = host_process.challenge(guest.owner, GUEST_NAME);
    let guest_challenge = guest_process.challenge(host.owner, HOST_NAME);
    assert_ne!(host_challenge.request, guest_challenge.request);
    assert!(host_challenge.digest.is_none());
    assert!(guest_challenge.digest.is_none());
    host.empty();
    guest.empty();
    guest_process.assert_actual_node_and_no_pin_in_processes(&invitation.pin);
    host_process.confirm(&host_challenge);
    guest_process.confirm(&guest_challenge);
    assert!(host_process.finish().success());
    assert!(guest_process.finish().success());
    assert!(host_process.text().contains(PAIRING_COMPLETE));
    assert!(guest_process.text().contains(PAIRING_COMPLETE));
    host_process.no_pin_logs(&invitation.pin);
    guest_process.no_pin_logs(&invitation.pin);
    assert!(
        !contains(&guest_process.tty, invitation.pin.as_bytes()),
        "entered PIN never echoes even to the private joining TTY"
    );
    assert_eq!(host_process.text().matches("Invitation PIN: ").count(), 1);
    invitation.assert_closed();
    (host_process, guest_process, invitation)
}

#[test]
fn ipc_console_bilateral_pairing_uses_actual_nodes_and_distinct_protected_identities() {
    let host = Fixture::new();
    let guest = Fixture::new();
    let (host_process, guest_process, _) = complete_pair(&host, &guest);
    let host_bytes = host.service.state.lock().unwrap().bytes.clone();
    let guest_bytes = guest.service.state.lock().unwrap().bytes.clone();
    let (host_certificates, host_key, _) = ControlCredentialBundle::decode(&host_bytes)
        .unwrap()
        .into_parts();
    let (guest_certificates, guest_key, _) = ControlCredentialBundle::decode(&guest_bytes)
        .unwrap()
        .into_parts();
    assert_ne!(host_key, guest_key);
    assert_ne!(host_certificates[0], guest_certificates[0]);
    for (fixture, peer, certificate, original) in [
        (&host, guest.owner, &guest_certificates[0], &host_bytes),
        (&guest, host.owner, &host_certificates[0], &guest_bytes),
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
        assert!(state.reads > 0);
        assert_eq!(state.forbidden, 0);
        assert!(!fixture.state_path.join("pending-v1.json").exists());
        drop(state);
        fixture.no_credential_leaks(&host_process);
        fixture.no_credential_leaks(&guest_process);
    }
}

#[test]
fn ipc_console_waiting_cancel_interrupt_and_eof_close_and_reap_without_persistence() {
    for input in [
        b"cancel\n".as_slice(),
        b"\x03".as_slice(),
        b"\x04".as_slice(),
    ] {
        let fixture = Fixture::new();
        let (mut process, invitation) = welcome(&fixture);
        process.wait_text("Type cancel to stop.");
        process.send(input);
        process.finish();
        fixture.empty();
        process.no_pin_logs(&invitation.pin);
        invitation.assert_closed();
        assert_eq!(process.text().matches("Invitation PIN: ").count(), 1);
    }
}

#[test]
fn ipc_console_one_sided_approval_cannot_persist_and_cancellation_reaps_both_nodes() {
    for host_only in [false, true] {
        let host = Fixture::new();
        let guest = Fixture::new();
        let (mut host_process, invitation) = welcome(&host);
        let mut guest_process = join(&guest, &invitation, "");
        let host_challenge = host_process.challenge(guest.owner, GUEST_NAME);
        let guest_challenge = guest_process.challenge(host.owner, HOST_NAME);
        guest_process.assert_actual_node_and_no_pin_in_processes(&invitation.pin);
        if host_only {
            host_process.confirm(&host_challenge);
        } else {
            guest_process.confirm(&guest_challenge);
        }
        host_process.remain_pending(Duration::from_millis(150));
        guest_process.remain_pending(Duration::from_millis(150));
        host.empty();
        guest.empty();
        if host_only {
            guest_process.send(b"cancel\n");
        } else {
            host_process.send(b"cancel\n");
        }
        host_process.finish();
        guest_process.finish();
        host.empty();
        guest.empty();
        host_process.no_pin_logs(&invitation.pin);
        guest_process.no_pin_logs(&invitation.pin);
        invitation.assert_closed();
    }
}

#[test]
fn ipc_console_rejects_yes_stale_request_wrong_peer_and_prebuffered_approval() {
    for case in ["yes", "stale", "wrong_peer", "prebuffered"] {
        let host = Fixture::new();
        let guest = Fixture::new();
        let (mut host_process, invitation) = welcome(&host);
        let prebuffer = if case == "prebuffered" {
            "yes\nconfirm 00000000-0000-4000-8000-000000000001 00000000-0000-4000-8000-000000000002\n"
        } else {
            ""
        };
        let mut guest_process = join(&guest, &invitation, prebuffer);
        if case == "prebuffered" {
            assert!(!guest_process.finish().success());
            host_process.send(b"\x03");
            host_process.finish();
            host.empty();
            guest.empty();
            host_process.no_pin_logs(&invitation.pin);
            guest_process.no_pin_logs(&invitation.pin);
            invitation.assert_closed();
            continue;
        }
        let _host_challenge = host_process.challenge(guest.owner, GUEST_NAME);
        let guest_challenge = guest_process.challenge(host.owner, HOST_NAME);
        guest_process.assert_actual_node_and_no_pin_in_processes(&invitation.pin);
        match case {
            "yes" => guest_process.send(b"yes\n"),
            "stale" => {
                guest_process.send(format!("confirm {} {}\n", NodeId::new(), host.owner).as_bytes())
            }
            "wrong_peer" => guest_process.send(
                format!("confirm {} {}\n", guest_challenge.request, NodeId::new()).as_bytes(),
            ),
            _ => unreachable!(),
        }
        assert!(
            !guest_process.finish().success(),
            "non-exact confirmation must fail closed"
        );
        host_process.finish();
        host.empty();
        guest.empty();
        host_process.no_pin_logs(&invitation.pin);
        guest_process.no_pin_logs(&invitation.pin);
        invitation.assert_closed();
    }
}

#[test]
fn ipc_console_missing_controlling_tty_refuses_before_protected_backend_access() {
    let fixture = Fixture::new();
    let before = fixture.counts();
    for args in [
        fixture.args(&["welcome", "--name", HOST_NAME, "--bind", "127.0.0.1:0"]),
        fixture.args(&["join", "--name", GUEST_NAME, "--endpoint", "127.0.0.1:9"]),
        fixture.args(&["list"]),
    ] {
        let mut process = Console::spawn(
            Path::new(env!("CARGO_BIN_EXE_capyio-linux-console")),
            &args,
            false,
            &[],
        );
        assert!(!process.finish().success());
        assert!(process.stdout.is_empty());
        assert!(process.tty.is_empty());
        assert_eq!(fixture.counts(), before);
        fixture.empty();
    }
}

#[test]
fn ipc_console_locked_backend_fails_before_invitation_or_networking() {
    let fixture = Fixture::new();
    fixture.service.state.lock().unwrap().locked = true;
    let mut welcome =
        Console::start(&fixture.args(&["welcome", "--name", HOST_NAME, "--bind", "127.0.0.1:0"]));
    assert!(!welcome.finish().success());
    assert!(!welcome.text().contains("Invitation PIN:"));
    assert!(!welcome.text().contains("Invitation endpoint:"));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = listener.local_addr().unwrap().to_string();
    let mut join =
        Console::start(&fixture.args(&["join", "--name", GUEST_NAME, "--endpoint", &endpoint]));
    join.wait_text(PIN_PROMPT);
    join.assert_hidden();
    join.send(b"12345678\n");
    assert!(!join.finish().success());
    assert!(matches!(listener.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock));
    join.no_pin_logs("12345678");
    assert!(!contains(&join.tty, b"12345678"));
    fixture.empty();
    let state = fixture.service.state.lock().unwrap();
    assert!(state.collection_property_reads >= 2);
    assert_eq!(state.reads, 0);
    assert_eq!(state.forbidden, 0);
}

#[test]
fn ipc_console_list_and_exact_revoke_are_metadata_only_and_keep_protected_identity() {
    let host = Fixture::new();
    let guest = Fixture::new();
    complete_pair(&host, &guest);
    host.service.state.lock().unwrap().locked = true;
    let before = host.counts();
    let original = fs::read(host.state_path.join("peers-v1.json")).unwrap();
    let credential = host.service.state.lock().unwrap().bytes.clone();
    let mut list = Console::start(&host.offline_args(&["list"]));
    assert!(list.finish().success());
    assert!(
        list.text()
            .contains(&format!("{} active {GUEST_NAME}", guest.owner))
    );
    assert_eq!(host.counts(), before);
    let peer = guest.owner.to_string();
    let mut cancelled = Console::start(&host.offline_args(&["revoke", "--peer", &peer]));
    let old_challenge = cancelled.challenge(guest.owner, GUEST_NAME);
    cancelled.send(b"cancel\n");
    cancelled.finish();
    assert_eq!(
        fs::read(host.state_path.join("peers-v1.json")).unwrap(),
        original
    );
    let mut stale = Console::start(&host.offline_args(&["revoke", "--peer", &peer]));
    let fresh = stale.challenge(guest.owner, GUEST_NAME);
    assert_ne!(old_challenge.request, fresh.request);
    stale.confirm(&old_challenge);
    assert!(!stale.finish().success());
    assert_eq!(
        fs::read(host.state_path.join("peers-v1.json")).unwrap(),
        original
    );
    let mut revoke = Console::start(&host.offline_args(&["revoke", "--peer", &peer]));
    let challenge = revoke.challenge(guest.owner, GUEST_NAME);
    assert!(challenge.digest.is_none());
    assert_eq!(
        fs::read(host.state_path.join("peers-v1.json")).unwrap(),
        original
    );
    revoke.confirm(&challenge);
    assert!(revoke.finish().success());
    assert!(
        revoke
            .text()
            .contains("Local trust revoked; protected credential retained.")
    );
    let mut list = Console::start(&host.offline_args(&["list"]));
    assert!(list.finish().success());
    assert!(
        list.text()
            .contains(&format!("{} revoked {GUEST_NAME}", guest.owner))
    );
    assert_eq!(host.counts(), before);
    assert_eq!(host.service.state.lock().unwrap().bytes, credential);
    host.service.state.lock().unwrap().locked = false;
    assert!(host.store().bootstrap(guest.owner).is_err());
    for process in [&list, &cancelled, &stale, &revoke] {
        host.no_credential_leaks(process);
    }
}

#[test]
fn ipc_console_recovery_binds_exact_digest_and_never_claims_remote_completion() {
    for stale_after_commit in [false, true] {
        let host = Fixture::new();
        let guest = Fixture::new();
        complete_pair(&host, &guest);
        let journal = fs::read(host.state_path.join("peers-v1.json")).unwrap();
        let credential = host.service.state.lock().unwrap().bytes.clone();
        let mut pending = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(host.state_path.join("pending-v1.json"))
            .unwrap();
        pending.write_all(&journal).unwrap();
        drop(pending);
        if !stale_after_commit {
            fs::remove_file(host.state_path.join("peers-v1.json")).unwrap();
        }
        let before = host.counts();
        for wrong_digest in [false, true] {
            let mut process = Console::start(&host.offline_args(&["recover"]));
            let challenge = process.challenge(guest.owner, GUEST_NAME);
            assert!(challenge.digest.is_some());
            assert_eq!(
                host.counts(),
                before,
                "inspection never accesses a protected backend"
            );
            if wrong_digest {
                process.send(
                    format!(
                        "confirm {} {} {}\n",
                        challenge.request,
                        guest.owner,
                        "0".repeat(64)
                    )
                    .as_bytes(),
                );
                assert!(!process.finish().success());
            } else {
                process.send(b"cancel\n");
                process.finish();
            }
            assert_eq!(
                fs::read(host.state_path.join("pending-v1.json")).unwrap(),
                journal
            );
            assert_eq!(host.counts(), before);
        }
        host.service.state.lock().unwrap().locked = true;
        let mut locked = Console::start(&host.args(&["recover"]));
        let locked_challenge = locked.challenge(guest.owner, GUEST_NAME);
        assert_eq!(host.counts(), before);
        locked.confirm(&locked_challenge);
        assert!(!locked.finish().success());
        assert_eq!(
            fs::read(host.state_path.join("pending-v1.json")).unwrap(),
            journal
        );
        assert_eq!(host.service.state.lock().unwrap().bytes, credential);
        assert_eq!(host.service.state.lock().unwrap().creates, before.3);
        host.service.state.lock().unwrap().locked = false;
        let before = host.counts();
        let mut recovery = Console::start(&host.args(&["recover"]));
        let challenge = recovery.challenge(guest.owner, GUEST_NAME);
        assert_eq!(host.counts(), before);
        recovery.confirm(&challenge);
        assert!(recovery.finish().success());
        assert!(
            recovery
                .text()
                .contains("Local recovery completed; remote completion is unconfirmed.")
        );
        assert!(!recovery.text().contains(PAIRING_COMPLETE));
        assert!(!host.state_path.join("pending-v1.json").exists());
        assert_eq!(
            fs::read(host.state_path.join("peers-v1.json")).unwrap(),
            journal
        );
        let state = host.service.state.lock().unwrap();
        assert_eq!(state.bytes, credential);
        assert_eq!(state.creates, before.3);
        assert_eq!(state.forbidden, before.4);
        assert!(state.reads > before.2);
        drop(state);
        host.no_credential_leaks(&recovery);
        assert!(host.store().bootstrap(guest.owner).is_ok());
    }
}

// The only synthetic sibling override is a physical copy of the controller in
// this test's private directory. Production has no environment/CLI injection.
const STUB: &str = r#"#!/usr/bin/python3
import json
import os
from pathlib import Path
import struct
import sys
import time

root = Path(__file__).parent
if sys.argv[1:] == ["linux-pairing", "--help"]:
    print("Usage: capyio-node linux-pairing [OPTIONS] --pairing-collection <COLLECTION>")
    raise SystemExit(0)
(root / "child.pid").write_text(str(os.getpid()))
def exact(n):
    result = b""
    while len(result) < n:
        part = os.read(0, n - len(result))
        if not part:
            raise SystemExit(2)
        result += part
    return result
request = json.loads(exact(struct.unpack(">I", exact(4))[0]))
assert request["action"] in ("list", "revoke")
case = (root / "case").read_text()
os.write(2, b"synthetic-child-private-detail\n")
def frame(value):
    raw = json.dumps(value, separators=(",", ":")).encode()
    os.write(1, struct.pack(">I", len(raw)) + raw)
if case == "partial_header":
    os.write(1, b"\x00\x00")
elif case == "partial_body":
    os.write(1, struct.pack(">I", 40) + b'{"version":')
elif case == "oversized":
    os.write(1, struct.pack(">I", 4097))
elif case == "zero":
    os.write(1, struct.pack(">I", 0))
elif case == "malformed":
    os.write(1, struct.pack(">I", 1) + b"{")
elif case == "unknown_fields":
    frame({"version":1,"event":"peers","peers":[],"pending_enrollment":False,"pending_peer":None,"unexpected":"synthetic-secret"})
elif case == "unexpected_completion":
    frame({"version":1,"event":"paired","local_saved":True,"remote_completion_confirmed":True,"control_endpoint_available":False})
elif case == "wrong_version":
    frame({"version":2,"event":"peers","peers":[],"pending_enrollment":False,"pending_peer":None})
elif case == "duplicate_terminal":
    event = {"version":1,"event":"peers","peers":[],"pending_enrollment":False,"pending_peer":None}
    frame(event)
    frame(event)
elif case == "prebuffered_exact":
    # Wait for the fixture to queue this *exact* current challenge response
    # before publishing the challenge. Neither a preplayed response nor idle
    # "yes" can count as a fresh user decision after the displayed challenge.
    deadline = time.monotonic() + 10
    while not (root / "release").exists():
        if time.monotonic() > deadline:
            raise SystemExit(3)
        time.sleep(0.005)
    frame({"version":1,"event":"revoke_confirmation_required",
           "request_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
           "peer_node_id":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
           "display_name":"Synthetic stub peer"})
    length = struct.unpack(">I", exact(4))[0]
    command = json.loads(exact(length))
    (root / "response.action").write_text(command["action"])
    raise SystemExit(0)
else:
    raise AssertionError(case)
# Keep partial frames and malformed output alive: the controller, not EOF,
# must enforce framing deadlines, cancel and reap this owned child.
time.sleep(30)
"#;

struct StubFixture {
    root: TemporaryDirectory,
    executable: PathBuf,
}
impl StubFixture {
    fn new() -> Self {
        let root = TemporaryDirectory::new();
        let executable = root.0.join("capyio-linux-console");
        fs::copy(env!("CARGO_BIN_EXE_capyio-linux-console"), &executable).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let node = root.0.join("capyio-node");
        fs::write(&node, STUB).unwrap();
        fs::set_permissions(node, fs::Permissions::from_mode(0o700)).unwrap();
        Self { root, executable }
    }
    fn args(&self, action: &str) -> Vec<String> {
        let mut args = vec![
            "--pairing-node".into(),
            "cccccccc-cccc-4ccc-8ccc-cccccccccccc".into(),
            "--pairing-state".into(),
            self.root.0.join("unused-state").display().to_string(),
            "--pairing-bus-address".into(),
            format!("unix:path={}/unused-bus", self.root.0.display()),
            "--pairing-collection".into(),
            COLLECTION.into(),
            action.into(),
        ];
        if action == "revoke" {
            args.extend([
                "--peer".into(),
                "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
            ]);
        }
        args
    }
    fn start(&self, case: &str, action: &str) -> Console {
        let marker = self.root.0.join("child.pid");
        if marker.exists() {
            fs::remove_file(&marker).unwrap();
        }
        fs::write(self.root.0.join("case"), case).unwrap();
        let mut process = Console::spawn(&self.executable, &self.args(action), true, &[]);
        let deadline = Instant::now() + BOUND;
        while !marker.exists() {
            process.collect();
            assert!(
                process.child.try_wait().unwrap().is_none(),
                "stub capability preflight and owned child startup must succeed"
            );
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(3));
        }
        // The existence of the file alone can precede its write.
        loop {
            if let Ok(pid) = fs::read_to_string(&marker).unwrap().parse() {
                process.node_pid = Some(pid);
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(3));
        }
        process
    }
}

#[test]
fn ipc_console_malformed_partial_and_out_of_sequence_child_events_fail_closed_and_reap() {
    let fixture = StubFixture::new();
    for case in [
        "partial_header",
        "partial_body",
        "oversized",
        "zero",
        "malformed",
        "unknown_fields",
        "unexpected_completion",
        "wrong_version",
        "duplicate_terminal",
    ] {
        let mut process = fixture.start(case, "list");
        assert!(!process.finish().success(), "{case} must fail closed");
        assert!(process.stdout.is_empty());
        for bytes in [&process.stdout, &process.stderr, &process.tty] {
            assert!(
                !contains(bytes, b"synthetic-child-private-detail"),
                "raw child errors must not be rendered"
            );
            assert!(!contains(bytes, b"synthetic-secret"));
        }
        assert!(!process.text().contains(PAIRING_COMPLETE));
        assert!(!fixture.root.0.join("unused-state").exists());
    }
}

#[test]
fn ipc_console_exact_preplayed_confirmation_is_flushed_before_fresh_challenge() {
    let fixture = StubFixture::new();
    let mut process = fixture.start("prebuffered_exact", "revoke");
    process.assert_hidden();
    process.send(
        b"confirm aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb\n",
    );
    fs::write(fixture.root.0.join("release"), b"ready").unwrap();
    let challenge = process.challenge(
        "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".parse().unwrap(),
        "Synthetic stub peer",
    );
    assert_eq!(challenge.request, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
    process.remain_pending(Duration::from_millis(200));
    assert!(
        !fixture.root.0.join("response.action").exists(),
        "preplayed exact response must not reach the child"
    );
    process.send(b"cancel\n");
    assert!(!process.finish().success());
    if let Ok(action) = fs::read_to_string(fixture.root.0.join("response.action")) {
        assert_eq!(
            action, "cancel",
            "only explicit cancellation may reach child"
        );
    }
    assert!(!fixture.root.0.join("unused-state").exists());
}
