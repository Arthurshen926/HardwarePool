use std::{
    collections::BTreeSet,
    io::ErrorKind,
    net::{IpAddr, SocketAddr, TcpListener},
    str::FromStr,
    thread,
    time::{Duration, Instant},
};

use capyio_control_link::CatalogControlLink;
use capyio_control_tls::{
    AuthenticatedControlServer, ControlTlsError, ControlTlsNodeBinding, ControlTlsServerIdentity,
};
use capyio_core::NodeId;
use capyio_pairing::PairingState;
use capyio_runtime::NodeRuntime;
use capyio_windows_pairing_store::WindowsPairingStore;

use super::DesktopNodeState;

const ACCEPT_WINDOW: Duration = Duration::from_secs(20);
const CONTROL_IO_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn verify_once(
    target_node_id: &str,
    bind_host: &str,
    port: u16,
    on_listening: impl FnOnce(SocketAddr),
) -> Result<usize, String> {
    let peer_id = NodeId::from_str(target_node_id)
        .map_err(|_| "peer Node ID must be a canonical UUID".to_owned())?;
    let host = IpAddr::from_str(bind_host)
        .map_err(|_| "listen host must be a numeric local IP".to_owned())?;
    if port == 0 || host.is_unspecified() || host.is_multicast() {
        return Err("listen endpoint is invalid".to_owned());
    }
    let mut node = DesktopNodeState::production()?;
    let local_id = node.runtime.snapshot().local_node.id;
    let store =
        WindowsPairingStore::default_for_user(local_id).map_err(|error| error.to_string())?;
    let directory = store.load_directory().map_err(|error| error.to_string())?;
    let peer = directory
        .peers()
        .find(|candidate| candidate.node_id == peer_id)
        .ok_or_else(|| "inbound peer is not paired".to_owned())?;
    if !matches!(peer.state, PairingState::Active) {
        return Err("inbound peer pairing is revoked".to_owned());
    }
    let credential = store
        .load_control_credential(&peer.credential_handle)
        .map_err(|error| format!("paired control credential could not be loaded: {error}"))?;
    let identity = ControlTlsServerIdentity::from_bundle(peer.peer_certificate_sha256, credential);
    let expected =
        ControlTlsNodeBinding::new(local_id, peer_id).map_err(|error| error.to_string())?;
    let listener = TcpListener::bind(SocketAddr::new(host, port))
        .map_err(|error| format!("control listen bind failed: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("control listen setup failed: {error}"))?;
    let server = AuthenticatedControlServer::from_listener_fresh_session(
        listener,
        identity,
        expected,
        CONTROL_IO_TIMEOUT,
    )
    .map_err(|error| error.to_string())?;
    on_listening(server.local_addr().map_err(|error| error.to_string())?);
    accept_and_exchange(
        &server,
        &mut node.runtime,
        peer_id,
        &peer.display_name,
        ACCEPT_WINDOW,
    )
}

pub(crate) fn accept_and_exchange(
    server: &AuthenticatedControlServer,
    runtime: &mut NodeRuntime,
    peer_id: NodeId,
    peer_name: &str,
    window: Duration,
) -> Result<usize, String> {
    let deadline = Instant::now() + window;
    let channel = loop {
        if Instant::now() >= deadline {
            return Err("no authenticated paired peer connected before timeout".to_owned());
        }
        match server.accept() {
            Ok(channel) => break channel,
            Err(ControlTlsError::Io(error)) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(_) => {
                // A rejected handshake does not terminate the paired listener;
                // do not spin if a platform repeatedly returns an immediate error.
                thread::sleep(Duration::from_millis(25));
            }
        }
    };
    let mut link = CatalogControlLink::admit_accepted_peer(runtime, channel, peer_name.to_owned())
        .map_err(|error| error.to_string())?;
    let exchange = (|| -> Result<usize, String> {
        let incoming = link.receive_catalog().map_err(|error| error.to_string())?;
        link.apply_catalog(runtime, incoming)
            .map_err(|error| error.to_string())?;
        let prepared = link
            .prepare_catalog(runtime, &BTreeSet::new())
            .map_err(|error| error.to_string())?;
        link.send_catalog(prepared)
            .map_err(|error| error.to_string())?;
        let heartbeat = link
            .receive_heartbeat()
            .map_err(|error| error.to_string())?;
        if heartbeat.acknowledgement || heartbeat.sequence == 0 {
            return Err("peer heartbeat request is invalid".to_owned());
        }
        link.send_heartbeat(heartbeat.sequence, true)
            .map_err(|error| error.to_string())?;
        runtime
            .snapshot()
            .peers
            .iter()
            .find(|peer| peer.id == peer_id)
            .map(|peer| peer.capabilities.len())
            .ok_or_else(|| "authenticated peer catalog is absent".to_owned())
    })();
    let cleanup = link
        .close_and_reconcile(runtime)
        .map_err(|error| error.to_string());
    cleanup?;
    exchange
}
