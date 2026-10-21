//! Explicit, finite first-enrollment management over trusted inherited FIFOs.
//! No listener, discovery, recovery, credential export or resource grant exists here.

use std::{
    collections::BTreeSet,
    io,
    net::SocketAddr,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use capyio_core::{MessageId, NodeId};
use capyio_enrollment::EnrollmentManager;
use capyio_linux_pairing_store::{
    BackendError, CredentialBackend, LinuxPairingStore, SecretBytes, SecretServiceBackend,
};
use capyio_pairing::{CredentialHandle, PairedPeerDirectory, PairingState};
use serde::Deserialize;
use serde_json::json;

use crate::launcher_io::LauncherIo;

const MAX_FRAME: usize = 4096;
const LIFETIME: Duration = Duration::from_secs(180);
const BACKEND_TIMEOUT: Duration = Duration::from_secs(1);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// All selectors are public. The PIN is accepted only in the private first frame.
pub struct PairingManagementSelection {
    pub local_node: NodeId,
    pub state_path: PathBuf,
    pub bus_address: String,
    pub collection: String,
}

// Intentionally no Debug or Serialize on any secret-bearing input.
#[derive(Deserialize)]
#[serde(transparent)]
struct Pin(String);
impl Pin {
    fn take(&mut self) -> String {
        std::mem::take(&mut self.0)
    }
}
impl Drop for Pin {
    fn drop(&mut self) {
        // Best effort; serde/parser copies, swap and compiler erasure are not guaranteed.
        let mut bytes = std::mem::take(&mut self.0).into_bytes();
        bytes.fill(0);
    }
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Join {
        version: u8,
        name: String,
        endpoint: SocketAddr,
        pin: Pin,
    },
    List {
        version: u8,
    },
    Revoke {
        version: u8,
        peer_node_id: NodeId,
    },
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Approve {
        version: u8,
        request_id: MessageId,
        peer_node_id: NodeId,
    },
    ConfirmRevoke {
        version: u8,
        request_id: MessageId,
        peer_node_id: NodeId,
    },
    Cancel {
        version: u8,
    },
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid pairing management input",
    )
}
fn storage_error() -> io::Error {
    io::Error::other("pairing storage unavailable or conflicting")
}
fn check_deadline(deadline: Instant) -> io::Result<()> {
    if Instant::now() >= deadline {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "pairing management deadline expired",
        ))
    } else {
        Ok(())
    }
}
fn parse_request(bytes: &[u8]) -> io::Result<Request> {
    if bytes.len() > MAX_FRAME {
        return Err(invalid());
    }
    let request: Request = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let valid = match &request {
        Request::Join {
            version,
            name,
            endpoint,
            pin,
        } => {
            *version == 1
                && !name.trim().is_empty()
                && name.len() <= 128
                && !name.chars().any(char::is_control)
                && endpoint.port() != 0
                && !endpoint.ip().is_unspecified()
                && !endpoint.ip().is_multicast()
                && pin.0.len() == 8
                && pin.0.bytes().all(|b| b.is_ascii_digit())
        }
        Request::List { version } => *version == 1,
        Request::Revoke {
            version,
            peer_node_id,
        } => *version == 1 && !peer_node_id.as_uuid().is_nil(),
    };
    if valid { Ok(request) } else { Err(invalid()) }
}
fn parse_command(bytes: &[u8]) -> io::Result<Command> {
    if bytes.len() > MAX_FRAME {
        return Err(invalid());
    }
    let command: Command = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let version = match &command {
        Command::Approve { version, .. }
        | Command::ConfirmRevoke { version, .. }
        | Command::Cancel { version } => *version,
    };
    if version == 1 {
        Ok(command)
    } else {
        Err(invalid())
    }
}

/// A nonce is issued only after an authenticated peer (or exact existing revoke
/// target) is selected. Consuming it prevents replay, including a second approval.
struct Confirmation {
    request_id: MessageId,
    peer_node_id: NodeId,
    consumed: bool,
}
impl Confirmation {
    fn new(peer_node_id: NodeId) -> Self {
        Self {
            request_id: MessageId::new(),
            peer_node_id,
            consumed: false,
        }
    }
    fn consume(&mut self, request_id: MessageId, peer_node_id: NodeId) -> io::Result<()> {
        if self.consumed || request_id != self.request_id || peer_node_id != self.peer_node_id {
            return Err(invalid());
        }
        self.consumed = true;
        Ok(())
    }
}

// Reserve the full one-second backend budget before *each* protected operation,
// including multiple load/put calls inside a single store poll. No call can renew
// the overall management deadline. Filesystem scheduling is not a hard RT bound.
struct DeadlineBackend {
    inner: Option<SecretServiceBackend>,
    deadline: Instant,
}
impl DeadlineBackend {
    fn check(&self) -> Result<(), BackendError> {
        if self.deadline.saturating_duration_since(Instant::now()) < BACKEND_TIMEOUT {
            Err(BackendError::Unavailable)
        } else {
            Ok(())
        }
    }
}
impl CredentialBackend for DeadlineBackend {
    fn load(&self, owner: NodeId, handle: &CredentialHandle) -> Result<SecretBytes, BackendError> {
        self.check()?;
        self.inner
            .as_ref()
            .ok_or(BackendError::Unavailable)?
            .load(owner, handle)
    }
    fn put_exact(
        &self,
        owner: NodeId,
        handle: &CredentialHandle,
        bytes: &[u8],
    ) -> Result<(), BackendError> {
        self.check()?;
        self.inner
            .as_ref()
            .ok_or(BackendError::Unavailable)?
            .put_exact(owner, handle, bytes)
    }
}

pub fn run_pairing_management_stdio(selection: PairingManagementSelection) -> io::Result<()> {
    let deadline = Instant::now() + LIFETIME;
    let mut channel = LauncherIo::stdio()?;
    let first = channel.read_frame(MAX_FRAME, deadline)?;
    let request = parse_request(first.as_slice())?;
    drop(first);
    if selection.local_node.as_uuid().is_nil() {
        return Err(invalid());
    }
    if deadline.saturating_duration_since(Instant::now()) < BACKEND_TIMEOUT {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "pairing management deadline expired",
        ));
    }
    // Listing and revoking local trust must remain available with a locked or
    // unavailable keyring. Their metadata-only backend cannot read or write keys.
    let backend = if matches!(&request, Request::Join { .. }) {
        Some(
            SecretServiceBackend::connect_unix(
                &selection.bus_address,
                &selection.collection,
                selection.local_node,
                BACKEND_TIMEOUT,
            )
            .map_err(|_| storage_error())?,
        )
    } else {
        None
    };
    let mut store = LinuxPairingStore::open(
        &selection.state_path,
        selection.local_node,
        DeadlineBackend {
            inner: backend,
            deadline,
        },
    )
    .map_err(|_| storage_error())?;
    let directory = store.load_directory().map_err(|_| storage_error())?;
    let pending = store
        .has_pending_enrollment()
        .map_err(|_| storage_error())?;
    check_deadline(deadline)?;
    match request {
        Request::List { .. } => {
            let peers: Vec<_> = directory.peers().map(|peer| json!({
                "peer_node_id":peer.node_id, "display_name":peer.display_name,
                "state":match peer.state { PairingState::Active => "active", PairingState::Revoked { .. } => "revoked" },
            })).collect();
            channel.send_json(
                &json!({"version":1,"event":"peers","peers":peers,
                "pending_enrollment":pending}),
                deadline,
            )
        }
        Request::Revoke { peer_node_id, .. } => {
            revoke(&mut channel, &mut store, &directory, peer_node_id, deadline)
        }
        Request::Join {
            name,
            endpoint,
            mut pin,
            ..
        } => {
            if pending {
                return Err(storage_error());
            }
            // Include revoked tombstones: first enrollment must never replace an old pin.
            let paired = directory.peers().map(|p| p.node_id).collect();
            join(
                &mut channel,
                &mut store,
                selection.local_node,
                name,
                endpoint,
                pin.take(),
                paired,
                deadline,
            )
        }
    }
}

fn revoke<B: CredentialBackend>(
    channel: &mut LauncherIo,
    store: &mut LinuxPairingStore<B>,
    directory: &PairedPeerDirectory,
    peer: NodeId,
    deadline: Instant,
) -> io::Result<()> {
    let record = directory
        .peers()
        .find(|p| p.node_id == peer)
        .ok_or_else(invalid)?;
    // Reject already queued commands before issuing the fresh challenge.
    if channel.poll_frame(MAX_FRAME)?.is_some() {
        return Err(invalid());
    }
    let mut confirmation = Confirmation::new(peer);
    channel.send_json(&json!({"version":1,"event":"revoke_confirmation_required",
        "request_id":confirmation.request_id,"peer_node_id":peer,"display_name":record.display_name}), deadline)?;
    let frame = channel.read_frame(MAX_FRAME, deadline)?;
    match parse_command(frame.as_slice())? {
        Command::Cancel { .. } => channel.send_json(
            &json!({"version":1,"event":"cancelled","local_saved":false}),
            deadline,
        ),
        Command::ConfirmRevoke {
            request_id,
            peer_node_id,
            ..
        } => {
            confirmation.consume(request_id, peer_node_id)?;
            // Never treat buffered duplicate confirmations as a new request.
            if channel.poll_frame(MAX_FRAME)?.is_some() {
                return Err(invalid());
            }
            check_deadline(deadline)?;
            let at_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| invalid())?
                .as_millis()
                .try_into()
                .map_err(|_| invalid())?;
            store.revoke(peer, at_ms).map_err(|_| storage_error())?;
            check_deadline(deadline)?;
            channel.send_json(
                &json!({"version":1,"event":"revoked","peer_node_id":peer,
                "credential_retained":true}),
                deadline,
            )
        }
        _ => Err(invalid()),
    }
}

#[allow(clippy::too_many_arguments)]
fn join<B: CredentialBackend>(
    channel: &mut LauncherIo,
    store: &mut LinuxPairingStore<B>,
    local: NodeId,
    name: String,
    endpoint: SocketAddr,
    pin: String,
    paired: BTreeSet<NodeId>,
    deadline: Instant,
) -> io::Result<()> {
    // EnrollmentManager's Drop calls cancel on all exits, shutting down its socket
    // and erasing pending candidate bytes; no separate detached owner is retained.
    let mut manager = EnrollmentManager::default();
    manager
        .join(local, name, endpoint, pin, paired, "linux-protected:")
        .map_err(|_| invalid())?;
    // join resets its state. Defer AFTER join, and before any approval/persistence.
    manager.defer_control_ready();
    let mut confirmation: Option<Confirmation> = None;
    let mut local_saved = false;
    let result = (|| {
        channel.send_json(&json!({"version":1,"event":"authenticating"}), deadline)?;
        loop {
            check_deadline(deadline)?;
            // Input is checked before status/persistence, so early approval, EOF,
            // cancellation, replay and malformed commands fail closed.
            if let Some(frame) = channel.poll_frame(MAX_FRAME)? {
                match parse_command(frame.as_slice())? {
                    Command::Cancel { .. } => {
                        manager.cancel();
                        check_deadline(deadline)?;
                        let (saved, pending) =
                            persistence_status(store, confirmation.as_ref(), local_saved);
                        return channel.send_json(
                            &json!({"version":1,"event":"cancelled",
                            "local_saved":saved,"pending_enrollment":pending,"remote_completion_confirmed":false}),
                            deadline,
                        );
                    }
                    Command::Approve {
                        request_id,
                        peer_node_id,
                        ..
                    } => {
                        let gate = confirmation.as_mut().ok_or_else(invalid)?;
                        gate.consume(request_id, peer_node_id)?;
                        let status = manager.status();
                        if status.phase != "confirm"
                            || status.peer_node_id.as_deref()
                                != Some(peer_node_id.to_string().as_str())
                        {
                            return Err(invalid());
                        }
                        manager.approve().map_err(|_| invalid())?;
                        continue;
                    }
                    _ => return Err(invalid()),
                }
            }
            let status = manager.status();
            match status.phase.as_str() {
                "confirm" if confirmation.is_none() => {
                    let peer = status
                        .peer_node_id
                        .as_deref()
                        .ok_or_else(invalid)?
                        .parse::<NodeId>()
                        .map_err(|_| invalid())?;
                    let display_name = status.peer_name.as_deref().ok_or_else(invalid)?;
                    if peer.as_uuid().is_nil()
                        || peer == local
                        || display_name.len() > 128
                        || display_name.trim().is_empty()
                        || display_name.chars().any(char::is_control)
                    {
                        return Err(invalid());
                    }
                    let gate = Confirmation::new(peer);
                    channel.send_json(&json!({"version":1,"event":"approval_required",
                        "request_id":gate.request_id,"peer_node_id":peer,"display_name":display_name}), deadline)?;
                    confirmation = Some(gate);
                }
                "preparing" | "saving" => {
                    if !confirmation.as_ref().is_some_and(|gate| gate.consumed) {
                        return Err(invalid());
                    }
                    // Manager enforces BOTH authenticated protocol approvals and
                    // remote preparation; this entry never receives candidate bytes.
                    if store
                        .poll_enrollment(&manager)
                        .map_err(|_| storage_error())?
                    {
                        local_saved = true;
                        check_deadline(deadline)?;
                        channel.send_json(
                            &json!({"version":1,"event":"locally_paired",
                            "local_saved":true,"remote_completion_confirmed":false,
                            "control_endpoint_available":false}),
                            deadline,
                        )?;
                        // Existing protocol's explicit null-port handoff. This is
                        // NOT a listener, resource grant or claim of control readiness.
                        manager.publish_control_port(None);
                    }
                }
                "complete" => {
                    if !local_saved {
                        return Err(invalid());
                    }
                    return channel.send_json(
                        &json!({"version":1,"event":"paired",
                        "local_saved":true,"remote_completion_confirmed":true,
                        "control_endpoint_available":false}),
                        deadline,
                    );
                }
                "failed" | "cancelled" => {
                    return Err(io::Error::other("pairing enrollment did not complete"));
                }
                "authenticating" | "confirm" | "waiting_peer" | "waiting_prepared"
                | "waiting_receipt" => {}
                _ => return Err(invalid()),
            }
            std::thread::sleep(
                POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    })();
    manager.cancel();
    if result.is_err() && check_deadline(deadline).is_ok() {
        let (saved, pending) = persistence_status(store, confirmation.as_ref(), local_saved);
        // Best effort only: poisoned pipes and expired deadlines are not retried.
        // No raw manager/backend error or credential-bearing status is serialized.
        let _ = channel.send_json(
            &json!({"version":1,"event":"failed",
            "local_saved":saved,"pending_enrollment":pending,"remote_completion_confirmed":false,
            "control_endpoint_available":false}),
            deadline,
        );
    }
    result
}

// Metadata-only reconciliation handles partial filesystem failure (for example a
// committed directory followed by failed journal removal). Null means unknown,
// never a claim that cancellation rolled back a protected write or committed pair.
fn persistence_status<B: CredentialBackend>(
    store: &LinuxPairingStore<B>,
    confirmation: Option<&Confirmation>,
    known_saved: bool,
) -> (Option<bool>, Option<bool>) {
    let saved = if known_saved {
        Some(true)
    } else if let Some(gate) = confirmation {
        store.load_directory().ok().map(|directory| {
            directory
                .peers()
                .any(|peer| peer.node_id == gate.peer_node_id)
        })
    } else {
        Some(false)
    };
    (saved, store.has_pending_enrollment().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_initial_schema_and_secret_input_bounds() {
        assert!(parse_request(br#"{"version":1,"action":"list"}"#).is_ok());
        assert!(parse_request(br#"{"version":1,"action":"join","name":"Linux","endpoint":"127.0.0.1:1234","pin":"12345678"}"#).is_ok());
        for input in [
            br#"{"version":2,"action":"list"}"#.as_slice(),
            br#"{"version":1,"action":"list","pin":"12345678"}"#,
            br#"{"version":1,"action":"recover"}"#,
            br#"{"version":1,"action":"list","version":1}"#,
            br#"{"version":1,"action":"join","name":"Linux","endpoint":"127.0.0.1:0","pin":"12345678"}"#,
            br#"{"version":1,"action":"join","name":"Linux","endpoint":"example.org:1234","pin":"12345678"}"#,
            br#"{"version":1,"action":"join","name":"Linux","endpoint":"127.0.0.1:1234","pin":"password"}"#,
        ] { assert!(parse_request(input).is_err()); }
        assert!(parse_request(&vec![b' '; MAX_FRAME + 1]).is_err());
    }

    #[test]
    fn exact_fresh_peer_bound_confirmation_is_single_use() {
        let peer = NodeId::new();
        let mut gate = Confirmation::new(peer);
        let other = Confirmation::new(peer);
        assert_ne!(gate.request_id, other.request_id);
        assert!(gate.consume(other.request_id, peer).is_err());
        assert!(gate.consume(gate.request_id, NodeId::new()).is_err());
        assert!(!gate.consumed);
        assert!(gate.consume(gate.request_id, peer).is_ok());
        assert!(gate.consume(gate.request_id, peer).is_err());
    }

    #[test]
    fn followups_reject_unknown_missing_and_cross_action_fields() {
        assert!(parse_command(br#"{"version":1,"action":"cancel"}"#).is_ok());
        for input in [
            br#"{"version":1,"action":"approve"}"#.as_slice(),
            br#"{"version":1,"action":"cancel","request_id":null}"#,
            br#"{"version":2,"action":"cancel"}"#,
            br#"{"version":1,"action":"list"}"#,
            br#"{"version":1,"action":"cancel","endpoint":"private"}"#,
        ] {
            assert!(parse_command(input).is_err());
        }
    }

    #[test]
    fn metadata_only_backend_cannot_load_or_write_credentials() {
        let backend = DeadlineBackend {
            inner: None,
            deadline: Instant::now() + LIFETIME,
        };
        let handle = CredentialHandle::new("linux-protected:synthetic").unwrap();
        assert!(matches!(
            backend.load(NodeId::new(), &handle),
            Err(BackendError::Unavailable)
        ));
        assert_eq!(
            backend.put_exact(NodeId::new(), &handle, b"synthetic"),
            Err(BackendError::Unavailable)
        );
    }

    #[test]
    fn errors_are_fixed_and_deadline_is_not_renewable() {
        assert_eq!(invalid().to_string(), "invalid pairing management input");
        assert_eq!(
            storage_error().to_string(),
            "pairing storage unavailable or conflicting"
        );
        assert!(check_deadline(Instant::now()).is_err());
        assert!(check_deadline(Instant::now() + Duration::from_secs(1)).is_ok());
        assert_eq!(LIFETIME, Duration::from_secs(180));
        assert_eq!(BACKEND_TIMEOUT, Duration::from_secs(1));
    }
}
