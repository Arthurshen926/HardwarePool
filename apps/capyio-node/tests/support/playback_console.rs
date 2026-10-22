//! Test-only PTY transport for the existing finite media fixtures. The only input to
//! the real console is terminal text; tests never access its Node control pipes.
use super::{BOUND, NodeId, Value, drain, nonblocking};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{fs::OpenOptionsExt, process::CommandExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(super) struct Console {
    master: File,
    slave: File,
    original: libc::termios,
    pub(super) tty: Vec<u8>,
    event_cursor: usize,
    pub(super) node_pid: Option<u32>,
    node_binary: PathBuf,
    node_command: &'static str,
    node_fixture_mode: Option<String>,
    event_prefix: &'static str,
    pub(super) peer: NodeId,
}

fn attributes(file: &File) -> libc::termios {
    // SAFETY: tcgetattr initializes the object on success and borrows a live FD.
    unsafe {
        let mut value = std::mem::zeroed();
        assert_eq!(libc::tcgetattr(file.as_raw_fd(), &mut value), 0);
        value
    }
}

impl Console {
    #[allow(dead_code)]
    pub(super) fn start(
        pairing: &super::paired_store::PersistedPairing,
        config: &Value,
        pretyped: bool,
        operation: &str,
        environment: &[(&str, &str)],
    ) -> (Child, Self) {
        Self::start_inner(pairing, config, pretyped, operation, environment, None)
    }

    // Only the separately built, feature-gated fixture action can select the
    // fixed fake-syscall helper. This is not a production backend override.
    #[cfg(feature = "touchpad-test-fixture")]
    #[allow(dead_code)]
    pub(super) fn start_touchpad_fixture(
        pairing: &super::paired_store::PersistedPairing,
        config: &Value,
        pretyped: bool,
        mode: &str,
    ) -> (Child, Self) {
        assert!(
            fixture_mode_allowed(mode),
            "fixed software fixture mode required"
        );
        Self::start_inner(
            pairing,
            config,
            pretyped,
            "touchpad-fixture",
            &[("RUST_BACKTRACE", "0"), ("RUST_LIB_BACKTRACE", "0")],
            Some(mode),
        )
    }

    fn start_inner(
        pairing: &super::paired_store::PersistedPairing,
        config: &Value,
        pretyped: bool,
        operation: &str,
        environment: &[(&str, &str)],
        fixture_mode: Option<&str>,
    ) -> (Child, Self) {
        let (node_command, event_prefix) = match operation {
            "playback" => ("linux-playback", "Playback event: "),
            "capture" => ("linux-capture", "Capture event: "),
            "camera" => ("linux-camera", "Camera event: "),
            "touchpad-fixture" if fixture_mode.is_some_and(fixture_mode_allowed) => {
                ("linux-touchpad-fixture", "Touchpad event: ")
            }
            _ => panic!("exact supported media console operation required"),
        };
        let node = Path::new(env!("CARGO_BIN_EXE_capyio-node"))
            .canonicalize()
            .unwrap();
        let binary = node.parent().unwrap().join("capyio-linux-console");
        assert!(
            binary.is_file(),
            "hosted console gate must build the actual paired console sibling"
        );
        let config_path = pairing.service.root.join(format!("{operation}.json"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&config_path)
            .unwrap();
        serde_json::to_writer(&mut file, config).unwrap();
        drop(file);
        pairing.assert_no_leak(&fs::read(&config_path).unwrap());
        let mut master = -1;
        let mut slave = -1;
        // SAFETY: openpty allocates two new descriptors, never a user terminal.
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
        // SAFETY: each successful descriptor is transferred exactly once.
        let (master, slave) = unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) };
        for fd in [&master, &slave] {
            // SAFETY: fcntl only changes flags on the borrowed owned descriptor.
            assert_eq!(
                unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) },
                0
            );
        }
        nonblocking(&master);
        let original = attributes(&slave);
        assert_ne!(original.c_lflag & libc::ECHO, 0);
        let mut console = Self {
            master,
            slave,
            original,
            tty: vec![],
            event_cursor: 0,
            node_pid: None,
            node_binary: node,
            node_command,
            node_fixture_mode: fixture_mode.map(str::to_owned),
            event_prefix,
            peer: pairing.peer,
        };
        if pretyped {
            // This stale start is queued before the controller even opens its
            // terminal. It must be flushed, not consumed as a later approval.
            console.send(
                format!(
                    "start {} {} {}\n",
                    super::MessageId::new(),
                    super::RouteId::new(),
                    pairing.peer
                )
                .as_bytes(),
            );
        }
        let slave_fd = console.slave.as_raw_fd();
        let fixture_pid = std::process::id() as libc::pid_t;
        let mut command = Command::new(binary);
        command
            .args([
                "--pairing-node",
                &pairing.owner.to_string(),
                "--pairing-state",
                pairing.state_path.to_str().unwrap(),
                "--pairing-bus-address",
                &pairing.service.address,
                "--pairing-collection",
                super::paired_store::COLLECTION,
                operation,
                "--config",
                config_path.to_str().unwrap(),
                "--peer",
                &pairing.peer.to_string(),
            ])
            .env_remove("RUST_LOG")
            .envs(environment.iter().copied())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(mode) = fixture_mode {
            command.args(["--fixture-mode", mode]);
        }
        // SAFETY: only async-signal-safe calls run after fork. The fresh slave
        // is attached to this owned session and stays live through spawn.
        unsafe {
            command.pre_exec(move || {
                // If the outer hosted fixture is interrupted, the controller
                // still gets a graceful Stop/cleanup signal despite its new
                // PTY session. Reject the fork/parent-death race explicitly.
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) < 0
                    || libc::getppid() != fixture_pid
                    || libc::setsid() < 0
                    || libc::ioctl(slave_fd, libc::TIOCSCTTY, 0) < 0
                {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().unwrap();
        // Public owned-process identity only, written immediately so the outer
        // graph harness can clean up even before TLS reaches its checkpoint.
        let runtime = if fixture_mode.is_some() {
            // Each synthetic pairing owns a fresh private directory. Unlike the
            // graph gates, many touchpad scenarios run in a single test process.
            pairing.service.root.clone()
        } else {
            PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap())
        };
        let start = runtime.join(".capyio-console-start.json");
        let mut evidence = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(start)
            .unwrap();
        serde_json::to_writer(
            &mut evidence,
            &serde_json::json!({"console_pid": child.id()}),
        )
        .unwrap();
        nonblocking(child.stdout.as_ref().unwrap());
        nonblocking(child.stderr.as_ref().unwrap());
        (child, console)
    }

    pub(super) fn collect(
        &mut self,
        child: &mut Child,
        stdout: &mut Vec<u8>,
        stderr: &mut Vec<u8>,
    ) {
        let mut buffer = [0; 4096];
        loop {
            match self.master.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => self.tty.extend_from_slice(&buffer[..n]),
                Err(e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.raw_os_error() == Some(libc::EIO) =>
                {
                    break;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => panic!("private terminal read failed: {e}"),
            }
            assert!(self.tty.len() <= 1_048_576, "bounded console transcript");
        }
        drain(child.stdout.as_mut().unwrap(), stdout);
        drain(child.stderr.as_mut().unwrap(), stderr);
        assert!(
            stdout.is_empty(),
            "console presentation must stay on its private terminal"
        );
    }

    pub(super) fn next_event(&mut self) -> Option<Value> {
        let text = std::str::from_utf8(&self.tty).expect("ASCII/UTF-8 public terminal output");
        let rest = &text[self.event_cursor..];
        let start = rest.find(self.event_prefix)? + self.event_cursor;
        let line = &text[start..];
        let end = line.find('\n')?;
        let value =
            parse_event(&line[..end], self.event_prefix).expect("canonical public media event");
        self.event_cursor = start + end + 1;
        Some(value)
    }

    pub(super) fn prompt_seen(&self, request: &str, route: &str) -> bool {
        let prompt = format!("Type start {request} {route} {} or stop: ", self.peer);
        String::from_utf8_lossy(&self.tty[self.event_cursor..]).contains(&prompt)
    }

    #[allow(dead_code)]
    pub(super) fn status_prompt_seen(&self) -> bool {
        let text = String::from_utf8_lossy(&self.tty[self.event_cursor..]);
        text.contains("Type status, stop or cancel: ")
            || text
                .lines()
                .any(|line| line.starts_with("Type start ") && line.ends_with(" or stop: "))
    }

    pub(super) fn assert_presentation(&self, approval: &Value) {
        if self.node_command == "linux-touchpad-fixture" {
            let text = String::from_utf8_lossy(&self.tty).replace('\r', "");
            let source = &approval["source"];
            let sink = &approval["sink"];
            assert!(text.contains(&format!(
                "Remote Source Node {}, Capability {}, Port {}\nLocal touchpad Sink Node {}\n",
                source["node_id"].as_str().unwrap(),
                source["capability_id"].as_str().unwrap(),
                source["port_id"].as_str().unwrap(),
                sink["node_id"].as_str().unwrap(),
            )));
            assert!(text.contains(&format!(
                "Sink Capability {}, Port {}; Session {}; Route {}\n",
                sink["capability_id"].as_str().unwrap(),
                sink["port_id"].as_str().unwrap(),
                approval["session_id"].as_str().unwrap(),
                approval["route_id"].as_str().unwrap(),
            )));
            for line in [
                "Format: touchpad-frame-v1; media security: tls13_mtls_v1; duration: 10 seconds\n",
                "At most 5 contacts; 120x70 mm; non-clickable; pressure enabled.\n",
                "Experimental finite AdapterManaged touchpad; independent Source consent and both leases remain required.\n",
                "Node Active follows committed backend ACK; binding alone is not activation.\n",
                "OS gestures and physical input are not verified.\n",
            ] {
                assert!(
                    text.contains(line),
                    "exact secure finite touchpad presentation required"
                );
            }
            return;
        }
        if self.node_command == "linux-camera" {
            let text = String::from_utf8_lossy(&self.tty).replace('\r', "");
            let source = &approval["source"];
            let sink = &approval["sink"];
            assert!(text.contains(&format!(
                "Remote Source Node {}, Capability {}, Port {}\nLocal camera Sink Node {}\n",
                source["node_id"].as_str().unwrap(),
                source["capability_id"].as_str().unwrap(),
                source["port_id"].as_str().unwrap(),
                sink["node_id"].as_str().unwrap()
            )));
            assert!(text.contains(&format!(
                "Sink Capability {}, Port {}; Session {}; Route {}\n",
                sink["capability_id"].as_str().unwrap(),
                sink["port_id"].as_str().unwrap(),
                approval["session_id"].as_str().unwrap(),
                approval["route_id"].as_str().unwrap()
            )));
            for line in [
                "Format: cavc-avc-access-unit-v1; media security: tls13_mtls_v1; duration: 10 seconds\n",
                "1280x720, 30 fps, rotation 0; at most 120 frames and 33554432 encoded bytes.\n",
                "Experimental finite camera; independent Source consent and both leases remain required.\n",
                "A bound helper or queued frame is not consumer delivery; Route remains Starting.\n",
            ] {
                assert!(
                    text.contains(line),
                    "exact secure finite camera presentation required"
                );
            }
            return;
        }
        let text = String::from_utf8_lossy(&self.tty).replace('\r', "");
        let source = approval["source"]["node_id"].as_str().unwrap();
        let sink = approval["sink"]["node_id"].as_str().unwrap();
        let mode = approval["endpoint_mode"].as_str().unwrap();
        let device = if matches!(mode, "virtual_microphone" | "virtual_speaker") {
            approval["node_name"].as_str().unwrap()
        } else {
            approval["target"].as_str().unwrap()
        };
        let seconds = approval["duration_secs"].as_u64().unwrap();
        assert!(text.contains(&format!("Source {source} -> Sink {sink}\n")));
        assert!(text.contains(&format!(
            "Mode: {mode}; device: {device}; duration: {seconds} seconds\n"
        )));
        if approval["media_security"] == "pinned_tls13_mono_v1" {
            assert!(matches!(mode, "playback" | "virtual_microphone"));
            assert!(seconds <= 60);
            assert_eq!(
                approval["format"],
                capyio_protocol::SECURE_AUDIO_MONO_FORMAT
            );
            assert!(text.contains("Experimental pinned TLS 1.3 mono media; explicit endpoint, one finite session at most 60 seconds. No automatic downgrade or renewal.\n"));
            assert!(text.contains("Authenticated PCM writes do not prove audibility or application consumption; the Route remains Starting.\n"));
            assert!(!text.contains("WARNING: plaintext unauthenticated UDP media"));
        } else {
            assert!(text.contains(
                "WARNING: plaintext unauthenticated UDP media; trusted isolated lab only.\n"
            ));
        }
    }

    pub(super) fn send(&mut self, bytes: &[u8]) {
        let deadline = Instant::now() + BOUND;
        let mut offset = 0;
        while offset < bytes.len() {
            assert!(Instant::now() < deadline, "bounded private terminal input");
            match self.master.write(&bytes[offset..]) {
                Ok(0) => panic!("closed private terminal"),
                Ok(n) => offset += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    thread::sleep(Duration::from_millis(2))
                }
                Err(e) => panic!("private terminal write failed: {e}"),
            }
        }
    }

    pub(super) fn identify_node(&mut self, child: &Child) -> u32 {
        if let Some(pid) = self.node_pid {
            return pid;
        }
        let path = format!("/proc/{}/task/{}/children", child.id(), child.id());
        let ids: Vec<u32> = fs::read_to_string(path)
            .unwrap()
            .split_whitespace()
            .map(|id| id.parse().unwrap())
            .collect();
        assert_eq!(
            ids.len(),
            1,
            "actual console must own exactly one actual Node"
        );
        let pid = ids[0];
        assert_eq!(
            fs::read_link(format!("/proc/{pid}/exe")).unwrap(),
            self.node_binary
        );
        let args = fs::read(format!("/proc/{pid}/cmdline")).unwrap();
        assert!(
            args.split(|byte| *byte == 0)
                .any(|arg| arg == self.node_command.as_bytes())
        );
        if let Some(mode) = &self.node_fixture_mode {
            let args: Vec<_> = args.split(|byte| *byte == 0).collect();
            assert!(
                args.windows(2)
                    .any(|args| { args == [b"--fixture-mode".as_slice(), mode.as_bytes()] })
            );
            assert!(!args.contains(&b"linux-touchpad".as_slice()));
        }
        self.node_pid = Some(pid);
        pid
    }

    pub(super) fn assert_hidden(&self) {
        assert_eq!(
            attributes(&self.slave).c_lflag & (libc::ECHO | libc::ECHONL),
            0
        );
    }

    pub(super) fn assert_finished(&self) {
        let after = attributes(&self.slave);
        assert_eq!(
            after.c_lflag, self.original.c_lflag,
            "TTY local flags restored"
        );
        assert_eq!(
            after.c_iflag, self.original.c_iflag,
            "TTY input flags restored"
        );
        assert_eq!(
            after.c_oflag, self.original.c_oflag,
            "TTY output flags restored"
        );
        assert_eq!(after.c_cc, self.original.c_cc, "TTY controls restored");
        let pid = self.node_pid.expect("actual Node ownership was observed");
        assert!(
            !Path::new(&format!("/proc/{pid}")).exists(),
            "actual Node reaped before console exit"
        );
    }
}

fn fixture_mode_allowed(mode: &str) -> bool {
    matches!(
        mode,
        "normal" | "fail-create" | "partial-write" | "delayed-commit" | "backpressure"
    )
}

fn parse_event(line: &str, prefix: &str) -> Option<Value> {
    if !matches!(
        prefix,
        "Playback event: " | "Capture event: " | "Camera event: " | "Touchpad event: "
    ) {
        return None;
    }
    let json = line.strip_prefix(prefix)?.trim_end_matches('\r');
    if json.len() > 4096 {
        return None;
    }
    let value: Value = serde_json::from_str(json).ok()?;
    let event = value["event"].as_str()?;
    let accepted = if prefix == "Touchpad event: " {
        matches!(
            event,
            "listening"
                | "approval_required"
                | "touchpad_bound"
                | "touchpad_active"
                | "status"
                | "stopped"
                | "failed"
        )
    } else if prefix == "Camera event: " {
        matches!(
            event,
            "listening"
                | "approval_required"
                | "camera_bound"
                | "camera_record"
                | "status"
                | "stopped"
                | "failed"
        )
    } else {
        matches!(
            event,
            "listening"
                | "approval_required"
                | "awaiting_peer_offer"
                | "status"
                | "stopped"
                | "failed"
        )
    };
    if value["version"] != 1 || !accepted {
        return None;
    }
    Some(value)
}

#[test]
fn pure_console_public_event_parser_requires_exact_canonical_schema() {
    assert!(
        parse_event(
            "Playback event: {\"version\":1,\"event\":\"status\"}\r",
            "Playback event: "
        )
        .is_some()
    );
    for line in [
        "{\"version\":1,\"event\":\"status\"}",
        "Playback event: {\"version\":2,\"event\":\"status\"}",
        "Playback event: {\"version\":1,\"event\":\"credentials\"}",
        "Playback event: not json",
    ] {
        assert!(parse_event(line, "Playback event: ").is_none());
    }
}

#[test]
fn pure_capture_console_events_cannot_substitute_playback_or_unknown_prefix() {
    let line = "Capture event: {\"version\":1,\"event\":\"status\"}\r";
    assert!(parse_event(line, "Capture event: ").is_some());
    assert!(parse_event(line, "Playback event: ").is_none());
    assert!(parse_event(line, "Capture ").is_none());
    assert!(
        parse_event(
            "Playback event: {\"version\":1,\"event\":\"status\"}",
            "Capture event: "
        )
        .is_none()
    );
}

#[test]
fn pure_camera_console_events_cannot_substitute_audio_prefix_or_events() {
    for event in [
        "listening",
        "approval_required",
        "camera_bound",
        "camera_record",
        "status",
        "stopped",
        "failed",
    ] {
        let line = format!("Camera event: {{\"version\":1,\"event\":\"{event}\"}}\r");
        assert!(parse_event(&line, "Camera event: ").is_some());
        for prefix in ["Playback event: ", "Capture event: ", "Camera "] {
            assert!(parse_event(&line, prefix).is_none());
        }
    }
    for event in ["camera_bound", "camera_record"] {
        for prefix in ["Playback event: ", "Capture event: "] {
            let line = format!("{prefix}{{\"version\":1,\"event\":\"{event}\"}}");
            assert!(parse_event(&line, prefix).is_none());
        }
    }
    for event in ["awaiting_peer_offer", "credentials"] {
        let line = format!("Camera event: {{\"version\":1,\"event\":\"{event}\"}}");
        assert!(parse_event(&line, "Camera event: ").is_none());
    }
}

#[test]
fn pure_touchpad_console_events_require_exact_prefix_version_and_event() {
    for event in [
        "listening",
        "approval_required",
        "touchpad_bound",
        "touchpad_active",
        "status",
        "stopped",
        "failed",
    ] {
        let line = format!("Touchpad event: {{\"version\":1,\"event\":\"{event}\"}}\r");
        assert!(parse_event(&line, "Touchpad event: ").is_some());
        for prefix in [
            "Playback event: ",
            "Capture event: ",
            "Camera event: ",
            "Touchpad ",
        ] {
            assert!(parse_event(&line, prefix).is_none());
        }
    }
    for line in [
        "Touchpad event: {\"version\":2,\"event\":\"status\"}",
        "Touchpad event: {\"version\":1,\"event\":\"camera_record\"}",
        "Touchpad event: {\"version\":1,\"event\":\"awaiting_peer_offer\"}",
        "Touchpad event: {\"version\":1,\"event\":\"credentials\"}",
        "Touchpad event: []",
        "Touchpad event: not json",
    ] {
        assert!(parse_event(line, "Touchpad event: ").is_none());
    }
}

#[test]
fn pure_touchpad_console_event_frames_have_fixed_bound_and_no_audio_alias() {
    for event in ["touchpad_bound", "touchpad_active"] {
        for prefix in ["Playback event: ", "Capture event: ", "Camera event: "] {
            let line = format!("{prefix}{{\"version\":1,\"event\":\"{event}\"}}");
            assert!(parse_event(&line, prefix).is_none());
        }
    }
    let oversized = format!(
        "Touchpad event: {{\"version\":1,\"event\":\"status\",\"padding\":\"{}\"}}",
        "x".repeat(4096)
    );
    assert!(parse_event(&oversized, "Touchpad event: ").is_none());
}

#[test]
fn pure_touchpad_console_fixture_modes_are_closed_software_only_allowlist() {
    for mode in [
        "normal",
        "fail-create",
        "partial-write",
        "delayed-commit",
        "backpressure",
    ] {
        assert!(fixture_mode_allowed(mode));
    }
    for mode in [
        "",
        "native",
        "/dev/uinput",
        "/tmp/helper",
        "normal --native",
        "NORMAL",
    ] {
        assert!(!fixture_mode_allowed(mode));
    }
}
