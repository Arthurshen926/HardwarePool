mod media;

use crate::{invitation::InvitationFifo, terminal::Terminal};
use capyio_core::{MessageId, NodeId};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    net::SocketAddr,
    os::{fd::AsRawFd, unix::fs::MetadataExt},
    path::PathBuf,
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
const MAX_FRAME: usize = 4096;
const LIFETIME: Duration = Duration::from_secs(180);
static INTERRUPTED: AtomicBool = AtomicBool::new(false);
extern "C" fn interrupted(_: libc::c_int) {
    INTERRUPTED.store(true, Ordering::Relaxed);
}
fn invalid() -> io::Error {
    io::Error::other("invalid console operation")
}
pub(crate) fn check(deadline: Instant) -> io::Result<()> {
    if INTERRUPTED.load(Ordering::Relaxed) || Instant::now() >= deadline {
        Err(invalid())
    } else {
        Ok(())
    }
}
struct Signals(Vec<(libc::c_int, libc::sigaction)>);
impl Signals {
    fn install() -> io::Result<Self> {
        let mut result = Self(Vec::new());
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            // Only an atomic flag is touched by the handler; cleanup stays on the main thread.
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = interrupted as *const () as usize;
            unsafe {
                libc::sigemptyset(&mut action.sa_mask);
            }
            let mut old = unsafe { std::mem::zeroed() };
            if unsafe { libc::sigaction(signal, &action, &mut old) } != 0 {
                return Err(invalid());
            }
            result.0.push((signal, old));
        }
        Ok(result)
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        for (signal, old) in &self.0 {
            unsafe {
                libc::sigaction(*signal, old, std::ptr::null_mut());
            }
        }
    }
}

#[derive(Parser)]
#[command(
    name = "capyio-linux-console",
    about = "Explicit finite Linux pairing and playback controller (no automatic resource grants)."
)]
struct Args {
    #[arg(long)]
    pairing_node: NodeId,
    #[arg(long)]
    pairing_state: PathBuf,
    #[arg(long)]
    pairing_bus_address: String,
    #[arg(long)]
    pairing_collection: String,
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    Welcome {
        #[arg(long)]
        name: String,
        #[arg(long)]
        bind: SocketAddr,
    },
    Join {
        #[arg(long)]
        name: String,
        #[arg(long)]
        endpoint: SocketAddr,
    },
    List,
    Recover,
    Revoke {
        #[arg(long)]
        peer: NodeId,
    },
    /// One finite paired-store playback session; nonsecret explicit manifest only.
    Playback {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        peer: NodeId,
    },
    /// Starts no transaction. In any active prompt, use cancel or Ctrl-C.
    Cancel,
}
fn validate(args: &Args) -> io::Result<()> {
    if args.pairing_node.as_uuid().is_nil()
        || !args.pairing_state.is_absolute()
        || args.pairing_bus_address.len() > 4096
        || args.pairing_collection.len() > 1024
    {
        return Err(invalid());
    }
    match &args.action {
        Action::Welcome { name, bind }
        | Action::Join {
            name,
            endpoint: bind,
        } => {
            if name.trim().is_empty()
                || name.len() > 128
                || name.chars().any(char::is_control)
                || bind.ip().is_unspecified()
                || bind.ip().is_multicast()
                || (matches!(args.action, Action::Join { .. }) && bind.port() == 0)
            {
                return Err(invalid());
            }
        }
        Action::Revoke { peer } if peer.as_uuid().is_nil() => return Err(invalid()),
        Action::Playback { config, peer }
            if !config.is_absolute() || peer.as_uuid().is_nil() || *peer == args.pairing_node =>
        {
            return Err(invalid());
        }
        _ => {}
    }
    Ok(())
}
fn nonblocking(fd: &impl AsRawFd) -> io::Result<()> {
    let old = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
    if old < 0 || unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, old | libc::O_NONBLOCK) } < 0
    {
        Err(invalid())
    } else {
        Ok(())
    }
}
struct Secret(Vec<u8>);
impl Drop for Secret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}
struct Frames {
    bytes: Vec<u8>,
    count: usize,
    limit: usize,
    eof: bool,
}
impl Default for Frames {
    fn default() -> Self {
        Self {
            bytes: Vec::new(),
            count: 0,
            limit: 64,
            eof: false,
        }
    }
}
impl Frames {
    fn poll(&mut self, input: &mut impl Read) -> io::Result<Option<Value>> {
        let mut chunk = [0; MAX_FRAME + 4];
        if self.bytes.len() < 4 || self.bytes.len() < 4 + self.length()? {
            match input.read(&mut chunk) {
                Ok(0) => self.eof = true,
                Ok(n) => {
                    self.bytes.extend_from_slice(&chunk[..n]);
                    if self.bytes.len() > 2 * (MAX_FRAME + 4) {
                        return Err(invalid());
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => return Err(invalid()),
            }
        }
        if self.bytes.len() >= 4 {
            let length = self.length()?;
            if self.bytes.len() >= 4 + length {
                self.count += 1;
                if self.count > self.limit {
                    return Err(invalid());
                }
                let result =
                    serde_json::from_slice(&self.bytes[4..4 + length]).map_err(|_| invalid())?;
                self.bytes.drain(..4 + length);
                return Ok(Some(result));
            }
        }
        if self.eof && !self.bytes.is_empty() {
            return Err(invalid());
        }
        Ok(None)
    }
    fn length(&self) -> io::Result<usize> {
        if self.bytes.len() < 4 {
            return Ok(0);
        }
        let n = u32::from_be_bytes(self.bytes[..4].try_into().map_err(|_| invalid())?) as usize;
        if n == 0 || n > MAX_FRAME {
            Err(invalid())
        } else {
            Ok(n)
        }
    }
}
fn sibling() -> io::Result<PathBuf> {
    let path = std::env::current_exe()?
        .parent()
        .ok_or_else(invalid)?
        .join("capyio-node");
    let owner = unsafe { libc::geteuid() };
    for parent in path.ancestors().skip(1) {
        let metadata = fs::symlink_metadata(parent)?;
        // Root-owned sticky /tmp is safe for an already-owned private child.
        let sticky_root = metadata.uid() == 0 && metadata.mode() & 0o1000 != 0;
        if !metadata.is_dir()
            || (metadata.uid() != owner && metadata.uid() != 0)
            || (metadata.mode() & 0o022 != 0 && !sticky_root)
        {
            return Err(invalid());
        }
    }
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_file()
        || metadata.mode() & 0o022 != 0
        || metadata.mode() & 0o111 == 0
        || (metadata.uid() != owner && metadata.uid() != 0)
    {
        return Err(invalid());
    }
    Ok(path)
}

fn require_node_capability() -> io::Result<()> {
    require_node_capability_for("linux-pairing")
}
fn require_node_capability_for(command: &str) -> io::Result<()> {
    if !["linux-pairing", "linux-playback"].contains(&command) {
        return Err(invalid());
    }
    let mut child = Command::new(sibling()?)
        .args([command, "--help"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let result = (|| {
        let mut output = child.stdout.take().ok_or_else(invalid)?;
        let mut error = child.stderr.take().ok_or_else(invalid)?;
        nonblocking(&output)?;
        nonblocking(&error)?;
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut bytes = Vec::new();
        let mut errors = 0;
        let mut output_eof = false;
        let mut error_eof = false;
        loop {
            check(deadline)?;
            let mut chunk = [0; 2048];
            for (input, is_output) in [
                (&mut output as &mut dyn Read, true),
                (&mut error as &mut dyn Read, false),
            ] {
                loop {
                    match input.read(&mut chunk) {
                        Ok(0) => {
                            if is_output {
                                output_eof = true;
                            } else {
                                error_eof = true;
                            }
                            break;
                        }
                        Ok(n) => {
                            if is_output {
                                bytes.extend_from_slice(&chunk[..n]);
                            } else {
                                errors += n;
                            }
                            if bytes.len() + errors > 16384 {
                                return Err(invalid());
                            }
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                        Err(_) => return Err(invalid()),
                    }
                }
            }
            if let Some(status) = child.try_wait()? {
                if !output_eof || !error_eof {
                    continue;
                }
                if !status.success() || errors != 0 {
                    return Err(invalid());
                }
                let help = std::str::from_utf8(&bytes).map_err(|_| invalid())?;
                if !help.contains(&format!("Usage: capyio-node {command} "))
                    || !help.contains("--pairing-collection")
                {
                    return Err(invalid());
                }
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
    })();
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
    result
}

struct Node {
    child: Child,
    input: Option<ChildStdin>,
    output: ChildStdout,
    error: ChildStderr,
    frames: Frames,
    error_bytes: usize,
    reaped: bool,
    stop_on_drop: bool,
}
impl Node {
    fn spawn(args: &Args) -> io::Result<Self> {
        Self::spawn_for(args, "linux-pairing", None)
    }
    fn spawn_for(args: &Args, command: &str, peer: Option<NodeId>) -> io::Result<Self> {
        if !matches!(
            (command, peer),
            ("linux-pairing", None) | ("linux-playback", Some(_))
        ) {
            return Err(invalid());
        }
        let path = sibling()?;
        let mut process = Command::new(path);
        process
            .args([command, "--trusted-lab", "--pairing-node"])
            .arg(args.pairing_node.to_string())
            .arg("--pairing-state")
            .arg(&args.pairing_state)
            .arg("--pairing-bus-address")
            .arg(&args.pairing_bus_address)
            .arg("--pairing-collection")
            .arg(&args.pairing_collection)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(peer) = peer {
            process.arg("--pairing-peer").arg(peer.to_string());
        }
        let mut child = process.spawn()?;
        let input = child.stdin.take().ok_or_else(invalid)?;
        let output = child.stdout.take().ok_or_else(invalid)?;
        let error = child.stderr.take().ok_or_else(invalid)?;
        let result = Self {
            child,
            input: Some(input),
            output,
            error,
            frames: Frames::default(),
            error_bytes: 0,
            reaped: false,
            stop_on_drop: command == "linux-playback",
        };
        nonblocking(result.input.as_ref().ok_or_else(invalid)?)?;
        nonblocking(&result.output)?;
        nonblocking(&result.error)?;
        Ok(result)
    }
    fn set_stop_on_drop(&mut self) {
        self.stop_on_drop = true;
    }
    fn set_frame_limit(&mut self, limit: usize) -> io::Result<()> {
        if !(64..=2048).contains(&limit) || self.frames.count != 0 {
            return Err(invalid());
        }
        self.frames.limit = limit;
        Ok(())
    }
    fn send(&mut self, value: &impl Serialize, deadline: Instant) -> io::Result<()> {
        let mut payload = Secret(serde_json::to_vec(value).map_err(|_| invalid())?);
        if payload.0.is_empty() || payload.0.len() > MAX_FRAME {
            return Err(invalid());
        }
        let mut frame = Secret(Vec::with_capacity(payload.0.len() + 4));
        frame
            .0
            .extend_from_slice(&(payload.0.len() as u32).to_be_bytes());
        frame.0.append(&mut payload.0);
        let mut at = 0;
        while at < frame.0.len() {
            check(deadline)?;
            match self
                .input
                .as_mut()
                .ok_or_else(invalid)?
                .write(&frame.0[at..])
            {
                Ok(0) => return Err(invalid()),
                Ok(n) => at += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(_) => return Err(invalid()),
            }
        }
        Ok(())
    }
    fn poll(&mut self) -> io::Result<Option<Value>> {
        let mut discard = [0; 1024];
        loop {
            match self.error.read(&mut discard) {
                Ok(0) => break,
                Ok(n) => {
                    self.error_bytes += n;
                    if self.error_bytes > 8192 {
                        return Err(invalid());
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(invalid()),
            }
        }
        discard.fill(0);
        self.frames.poll(&mut self.output)
    }
}
fn shutdown_frame(media: bool) -> &'static [u8] {
    if media {
        b"\0\0\0\x1e{\"version\":1,\"command\":\"stop\"}"
    } else {
        b"\0\0\0\x1f{\"version\":1,\"action\":\"cancel\"}"
    }
}
impl Drop for Node {
    fn drop(&mut self) {
        if self.reaped {
            return;
        }
        // Best effort cancel, then EOF. Always reap, escalating only this child.
        if let Some(input) = &mut self.input {
            let frame = shutdown_frame(self.stop_on_drop);
            let _ = input.write_all(frame);
        }
        self.input.take();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                self.reaped = true;
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn text<'a>(value: &'a Value, key: &str) -> io::Result<&'a str> {
    value.get(key).and_then(Value::as_str).ok_or_else(invalid)
}
fn exact_keys(value: &Value, keys: &[&str]) -> io::Result<()> {
    let object = value.as_object().ok_or_else(invalid)?;
    if object.len() != keys.len() || object.keys().any(|k| !keys.contains(&k.as_str())) {
        return Err(invalid());
    }
    Ok(())
}
fn clean(value: &str) -> String {
    value
        .chars()
        .take(128)
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '?'
            }
        })
        .collect()
}
fn node_id(value: &Value, key: &str) -> io::Result<NodeId> {
    let id: NodeId = serde_json::from_value(value.get(key).ok_or_else(invalid)?.clone())
        .map_err(|_| invalid())?;
    if id.as_uuid().is_nil() {
        Err(invalid())
    } else {
        Ok(id)
    }
}
struct Confirmation {
    expected: Secret,
    response: Value,
}
fn challenge(
    value: &Value,
    action: &Action,
    tty: &mut Terminal,
    deadline: Instant,
) -> io::Result<Confirmation> {
    let event = text(value, "event")?;
    let recovery = event == "recovery_confirmation_required";
    let command = match (event, action) {
        ("approval_required", Action::Join { .. } | Action::Welcome { .. }) => "approve",
        ("revoke_confirmation_required", Action::Revoke { peer })
            if *peer == node_id(value, "peer_node_id")? =>
        {
            "confirm_revoke"
        }
        ("recovery_confirmation_required", Action::Recover) => "confirm_recover",
        _ => return Err(invalid()),
    };
    if recovery {
        exact_keys(
            value,
            &[
                "version",
                "event",
                "request_id",
                "peer_node_id",
                "display_name",
                "transaction_digest",
                "remote_completion_confirmed",
                "control_endpoint_available",
            ],
        )?;
        if value["remote_completion_confirmed"] != false
            || value["control_endpoint_available"] != false
        {
            return Err(invalid());
        }
    } else {
        exact_keys(
            value,
            &[
                "version",
                "event",
                "request_id",
                "peer_node_id",
                "display_name",
            ],
        )?;
    }
    let peer = node_id(value, "peer_node_id")?;
    let request: MessageId =
        serde_json::from_value(value["request_id"].clone()).map_err(|_| invalid())?;
    if request.as_uuid().is_nil() {
        return Err(invalid());
    }
    let name = text(value, "display_name")?;
    if name.len() > 128 {
        return Err(invalid());
    }
    let mut expected = format!("confirm {request} {peer}");
    let mut display = format!(
        "Peer: {peer}\r\nName: {}\r\nRequest: {request}\r\n",
        clean(name)
    );
    let mut response =
        json!({"version":1,"action":command,"request_id":request,"peer_node_id":peer});
    if recovery {
        let digest = text(value, "transaction_digest")?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid());
        }
        display.push_str(&format!("Transaction: {digest}\r\n"));
        expected.push(' ');
        expected.push_str(digest);
        response["transaction_digest"] = json!(digest);
    }
    display.push_str(&format!("Type {expected} or cancel: "));
    tty.begin_prompt(&display, deadline)?;
    Ok(Confirmation {
        expected: Secret(expected.into_bytes()),
        response,
    })
}
fn persisted(value: &Value, remote: bool) -> io::Result<()> {
    exact_keys(
        value,
        &[
            "version",
            "event",
            "local_saved",
            "remote_completion_confirmed",
            "control_endpoint_available",
        ],
    )?;
    if value["local_saved"] != true
        || value["remote_completion_confirmed"] != remote
        || value["control_endpoint_available"] != false
    {
        Err(invalid())
    } else {
        Ok(())
    }
}
fn peers(value: &Value, tty: &mut Terminal, deadline: Instant) -> io::Result<()> {
    exact_keys(
        value,
        &[
            "version",
            "event",
            "peers",
            "pending_enrollment",
            "pending_peer",
        ],
    )?;
    let peers = value["peers"].as_array().ok_or_else(invalid)?;
    let pending = value["pending_enrollment"].as_bool().ok_or_else(invalid)?;
    tty.write("Peers:\r\n", deadline)?;
    for peer in peers {
        exact_keys(peer, &["peer_node_id", "state", "display_name"])?;
        let id = node_id(peer, "peer_node_id")?;
        let state = text(peer, "state")?;
        if !["active", "revoked"].contains(&state) {
            return Err(invalid());
        }
        tty.write(
            &format!("{id} {state} {}\r\n", clean(text(peer, "display_name")?)),
            deadline,
        )?;
    }
    if pending {
        let p = &value["pending_peer"];
        exact_keys(p, &["peer_node_id", "display_name", "transaction_digest"])?;
        let id = node_id(p, "peer_node_id")?;
        let digest = text(p, "transaction_digest")?;
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        tty.write(
            &format!("Pending local repair: {id} {digest}\r\n"),
            deadline,
        )?;
    } else if !value["pending_peer"].is_null() {
        return Err(invalid());
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Invitation {
    version: u8,
    event: String,
    endpoint: SocketAddr,
    pin: String,
    valid_for_secs: u64,
}
impl Drop for Invitation {
    fn drop(&mut self) {
        let mut bytes = std::mem::take(&mut self.pin).into_bytes();
        bytes.fill(0);
    }
}

pub fn run() -> io::Result<()> {
    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            error.print().map_err(|_| invalid())?;
            return Ok(());
        }
        Err(_) => return Err(invalid()),
    };
    if !cfg!(feature = "paired-store") {
        return Err(invalid());
    }
    validate(&args)?;
    if let Action::Playback { config, peer } = &args.action {
        return media::run(&args, config, *peer);
    }
    let _signals = Signals::install()?;
    let deadline = Instant::now() + LIFETIME;
    require_node_capability()?;
    let mut tty = Terminal::open()?;
    if matches!(args.action, Action::Cancel) {
        tty.write(
            "No transaction started. Use cancel or Ctrl-C in the active console.\r\n",
            deadline,
        )?;
        return Ok(());
    }
    let mut fifo = if matches!(args.action, Action::Welcome { .. }) {
        Some(InvitationFifo::create()?)
    } else {
        None
    };
    // PIN stays in an owned wiping terminal line and a wiping serialized frame.
    // The request is never placed in argv, environment, a regular file or logs.
    let mut node = Node::spawn(&args)?;
    match &args.action {
        Action::Join{name,endpoint}=>{
            tty.begin_prompt("PIN (8 digits; hidden): ",deadline)?;
            let pin=loop {
                check(deadline)?;
                if node.child.try_wait()?.is_some(){return Err(invalid());}
                if let Some(line)=tty.poll_line()?{break line;}
                thread::sleep(Duration::from_millis(10));
            };
            let pin=pin.as_str()?;
            if pin.len()!=8||!pin.bytes().all(|b|b.is_ascii_digit()){return Err(invalid());}
            #[derive(Serialize)]struct Join<'a>{version:u8,action:&'static str,name:&'a str,endpoint:SocketAddr,pin:&'a str}
            node.send(&Join{version:1,action:"join",name,endpoint:*endpoint,pin},deadline)?;
        }
        Action::Welcome{name,bind}=>node.send(&json!({"version":1,"action":"welcome","name":name,"bind":bind,"invitation_pipe":fifo.as_ref().ok_or_else(invalid)?.path()}),deadline)?,
        Action::List=>node.send(&json!({"version":1,"action":"list"}),deadline)?,
        Action::Recover=>node.send(&json!({"version":1,"action":"recover"}),deadline)?,
        Action::Revoke{peer}=>node.send(&json!({"version":1,"action":"revoke","peer_node_id":peer}),deadline)?,
        Action::Cancel | Action::Playback { .. } =>unreachable!(),
    }
    let mut confirmation: Option<Confirmation> = None;
    let mut challenged = false;
    let mut approved = false;
    let mut started = false;
    let mut saved = false;
    let mut final_message = None;
    let mut final_deadline = None;
    let mut invitation_seen = false;
    let mut partial_since = None;
    loop {
        check(deadline)?;
        if let Some(pipe) = &mut fifo
            && let Some(bytes) = pipe.poll()?
        {
            if invitation_seen {
                return Err(invalid());
            }
            let invitation: Invitation =
                serde_json::from_slice(bytes.as_bytes()).map_err(|_| invalid())?;
            if invitation.version != 1
                || invitation.event != "invitation"
                || invitation.pin.len() != 8
                || !invitation.pin.bytes().all(|b| b.is_ascii_digit())
                || !(1..180).contains(&invitation.valid_for_secs)
                || invitation.endpoint.port() == 0
                || invitation.endpoint.ip().is_unspecified()
                || invitation.endpoint.ip().is_multicast()
            {
                return Err(invalid());
            }
            let mut display = Secret(
                format!(
                    "Invitation endpoint: {}\r\nInvitation PIN: {}\r\nExpires in: {} seconds\r\n",
                    invitation.endpoint, invitation.pin, invitation.valid_for_secs
                )
                .into_bytes(),
            );
            tty.write(
                std::str::from_utf8(&display.0).map_err(|_| invalid())?,
                deadline,
            )?;
            display.0.fill(0);
            invitation_seen = true;
        }
        if let Some(line) = tty.poll_line()? {
            if line.as_bytes() == b"cancel" {
                return Err(invalid());
            }
            let gate = confirmation.take().ok_or_else(invalid)?;
            if line.as_bytes() != gate.expected.0 {
                return Err(invalid());
            }
            // Reject a second already-buffered line before issuing a trust decision.
            if tty.poll_line()?.is_some() {
                return Err(invalid());
            }
            node.send(&gate.response, deadline)?;
            approved = true;
        }
        let mut received_event = false;
        if let Some(value) = node.poll()? {
            received_event = true;
            if final_message.is_some() || value["version"] != 1 {
                return Err(invalid());
            }
            match text(&value, "event")? {
                "waiting" | "authenticating" => {
                    exact_keys(&value, &["version", "event"])?;
                    if started
                        || !matches!(args.action, Action::Join { .. } | Action::Welcome { .. })
                    {
                        return Err(invalid());
                    }
                    started = true;
                    tty.begin_prompt(
                        "Waiting for authenticated peer; no trust saved. Type cancel to stop.\r\n",
                        deadline,
                    )?;
                }
                "approval_required"
                | "revoke_confirmation_required"
                | "recovery_confirmation_required" => {
                    if challenged {
                        return Err(invalid());
                    }
                    if matches!(args.action, Action::Join { .. } | Action::Welcome { .. })
                        && !started
                    {
                        return Err(invalid());
                    }
                    confirmation = Some(challenge(&value, &args.action, &mut tty, deadline)?);
                    challenged = true;
                }
                "locally_paired" => {
                    if !approved
                        || saved
                        || !matches!(args.action, Action::Join { .. } | Action::Welcome { .. })
                    {
                        return Err(invalid());
                    }
                    persisted(&value, false)?;
                    saved = true;
                    tty.write(
                        "Local trust saved; waiting for remote completion.\r\n",
                        deadline,
                    )?;
                }
                "paired" => {
                    if !saved {
                        return Err(invalid());
                    }
                    persisted(&value, true)?;
                    final_message = Some("Pairing completed; no resources granted.\r\n");
                }
                "locally_recovered" => {
                    if !approved || !matches!(args.action, Action::Recover) {
                        return Err(invalid());
                    }
                    persisted(&value, false)?;
                    final_message =
                        Some("Local recovery completed; remote completion is unconfirmed.\r\n");
                }
                "revoked" => {
                    exact_keys(
                        &value,
                        &["version", "event", "peer_node_id", "credential_retained"],
                    )?;
                    if !approved
                        || value["credential_retained"] != true
                        || !matches!(args.action,Action::Revoke{peer} if peer==node_id(&value,"peer_node_id")?)
                    {
                        return Err(invalid());
                    }
                    final_message = Some("Local trust revoked; protected credential retained.\r\n");
                }
                "peers" => {
                    if !matches!(args.action, Action::List) {
                        return Err(invalid());
                    }
                    peers(&value, &mut tty, deadline)?;
                    final_message = Some("Metadata inspection completed.\r\n");
                }
                _ => return Err(invalid()),
            }
            if final_message.is_some() {
                final_deadline = Some(Instant::now() + Duration::from_secs(2));
            }
        }
        if node.frames.bytes.is_empty() {
            partial_since = None;
        } else {
            let since = partial_since.get_or_insert_with(Instant::now);
            if since.elapsed() > Duration::from_secs(2) {
                return Err(invalid());
            }
        }
        if received_event {
            continue;
        }
        if let Some(status) = node.child.try_wait()? {
            node.reaped = true;
            // Drain and reject trailing or partial frames before reporting success.
            if node.poll()?.is_some() || !node.frames.bytes.is_empty() || !status.success() {
                return Err(invalid());
            }
            tty.write(final_message.ok_or_else(invalid)?, deadline)?;
            return Ok(());
        }
        if final_deadline.is_some_and(|end| Instant::now() >= end) {
            return Err(invalid());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shutdown_frames_keep_pairing_cancel_separate_from_media_stop() {
        for (media, key, command) in [(false, "action", "cancel"), (true, "command", "stop")] {
            let frame = shutdown_frame(media);
            let length = u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize;
            assert_eq!(length, frame.len() - 4);
            let value: Value = serde_json::from_slice(&frame[4..]).unwrap();
            assert_eq!(value, json!({"version":1,key:command}));
        }
    }
    #[test]
    fn frame_bounds_and_partial_eof_fail_closed() {
        for bytes in [
            vec![0, 0, 0, 0],
            vec![0, 0, 16, 1],
            vec![0, 0, 0],
            vec![0, 0, 0, 3, b'{'],
        ] {
            let mut frames = Frames::default();
            let mut input = bytes.as_slice();
            let first = frames.poll(&mut input);
            assert!(first.is_err() || frames.poll(&mut input).is_err());
        }
    }
    #[test]
    fn frame_parser_preserves_coalesced_events() {
        let payload = br#"{"version":1,"event":"waiting"}"#;
        let mut bytes = Vec::new();
        for _ in 0..2 {
            bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            bytes.extend_from_slice(payload);
        }
        let mut frames = Frames::default();
        let mut input = bytes.as_slice();
        assert_eq!(
            frames.poll(&mut input).unwrap().unwrap()["event"],
            "waiting"
        );
        assert_eq!(
            frames.poll(&mut input).unwrap().unwrap()["event"],
            "waiting"
        );
    }
    #[test]
    fn metadata_cannot_emit_terminal_controls() {
        assert_eq!(clean("a\x1b[31m\r\n\u{202e}b"), "a?[31m???b");
    }
    #[test]
    fn unknown_event_fields_fail_closed() {
        assert!(
            exact_keys(
                &json!({"version":1,"event":"waiting","pin":"secret"}),
                &["version", "event"]
            )
            .is_err()
        );
    }
}
