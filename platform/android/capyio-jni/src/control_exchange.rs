//! Split transport I/O from brief Runtime access. The JNI registry supplies a
//! guard only for deterministic admission/preparation/application operations.

use std::{collections::BTreeSet, ops::DerefMut};

use capyio_control_credentials::ControlCredentialBundle;
use capyio_control_link::{CatalogControlLink, CatalogLinkError};
use capyio_control_tls::{
    AuthenticatedControlClientChannel, AuthenticatedControlServerChannel, ControlTlsClientIdentity,
    ControlTlsEndpoint, ControlTlsNodeBinding,
};
use capyio_core::{CapabilityId, NodeId, PortDirection};
use capyio_pairing::PairedPeerDirectory;

use crate::{AndroidCatalogRuntimeV1, AndroidControlDialError};

/// IP/port remain routing hints. Active pairing, exact leaf pinning and the
/// ordered Node/Session binding are checked before returning a channel.
pub fn authenticate_android_control_dial(
    local_id: NodeId,
    endpoint: ControlTlsEndpoint,
    directory_json: &[u8],
    encoded_credential: &[u8],
) -> Result<(AuthenticatedControlClientChannel, String), AndroidControlDialError> {
    let directory = PairedPeerDirectory::decode_json(local_id, directory_json)
        .map_err(AndroidControlDialError::Pairing)?;
    let mut active = directory.active_peers();
    let peer = active.next().ok_or(AndroidControlDialError::NoActivePeer)?;
    if active.next().is_some() {
        return Err(AndroidControlDialError::MultipleActivePeers);
    }
    let bundle = ControlCredentialBundle::decode(encoded_credential)
        .map_err(AndroidControlDialError::Credential)?;
    let identity = ControlTlsClientIdentity::from_bundle(peer.peer_certificate_sha256, bundle);
    let expected_nodes = ControlTlsNodeBinding::new(peer.node_id, local_id)
        .map_err(AndroidControlDialError::Transport)?;
    let channel = AuthenticatedControlClientChannel::connect_fresh_session(
        endpoint,
        &peer.server_name,
        identity,
        expected_nodes,
    )
    .map_err(AndroidControlDialError::Transport)?;
    Ok((channel, peer.display_name.clone()))
}

fn selected_sources(host: &AndroidCatalogRuntimeV1) -> BTreeSet<CapabilityId> {
    host.runtime
        .snapshot()
        .local_node
        .capabilities
        .iter()
        .filter(|(_, capability)| {
            capability
                .ports
                .values()
                .any(|port| port.direction == PortDirection::Source)
        })
        .map(|(id, _)| *id)
        .collect()
}

fn reconcile_failure<G>(link: &mut CatalogControlLink, access: &impl Fn() -> Result<G, String>)
where
    G: DerefMut<Target = AndroidCatalogRuntimeV1>,
{
    if let Ok(mut host) = access() {
        // A callback may have switched hosts after admission. Never reconcile
        // that replacement into an already registered bilateral Session.
        if host
            .check_unregistered_catalog_peer(link.peer_node_id())
            .is_err()
        {
            link.close();
            return;
        }
        let _ = link.close_and_reconcile(&mut host.runtime);
    } else {
        link.close();
    }
}

/// Receives before publishing, matching the established acceptor ordering.
/// The returned revision identifies the exact catalog prepared for the wire.
pub fn exchange_accepted_android_catalog<G>(
    channel: AuthenticatedControlServerChannel,
    display_name: String,
    access: impl Fn() -> Result<G, String>,
) -> Result<(CatalogControlLink, u64), String>
where
    G: DerefMut<Target = AndroidCatalogRuntimeV1>,
{
    let mut link = {
        let mut host = access()?;
        CatalogControlLink::admit_accepted_peer(&mut host.runtime, channel, display_name)
            .map_err(|error| error.to_string())?
    };
    let exchange = (|| {
        let incoming = link.receive_catalog().map_err(|error| error.to_string())?;
        let (outgoing, revision) = {
            let mut host = access()?;
            host.check_unregistered_catalog_peer(link.peer_node_id())?;
            link.apply_catalog(&mut host.runtime, incoming)
                .map_err(|error| error.to_string())?;
            let selected = selected_sources(&host);
            let outgoing = link
                .prepare_catalog(&host.runtime, &selected)
                .map_err(|error| error.to_string())?;
            (outgoing, host.publication_revision())
        };
        link.send_catalog(outgoing)
            .map_err(|error| error.to_string())?;
        Ok::<_, String>(revision)
    })();
    match exchange {
        Ok(revision) => Ok((link, revision)),
        Err(error) => {
            reconcile_failure(&mut link, &access);
            Err(error)
        }
    }
}

/// Publishes before receiving, matching the established dialer ordering.
pub fn exchange_dialed_android_catalog<G>(
    channel: AuthenticatedControlClientChannel,
    display_name: String,
    access: impl Fn() -> Result<G, String>,
) -> Result<(CatalogControlLink, u64), String>
where
    G: DerefMut<Target = AndroidCatalogRuntimeV1>,
{
    let mut link = {
        let mut host = access()?;
        CatalogControlLink::admit_dialed_peer(&mut host.runtime, channel, display_name)
            .map_err(|error| error.to_string())?
    };
    let exchange = (|| {
        let (outgoing, revision) = {
            let host = access()?;
            let selected = selected_sources(&host);
            let outgoing = link
                .prepare_catalog(&host.runtime, &selected)
                .map_err(|error| error.to_string())?;
            (outgoing, host.publication_revision())
        };
        link.send_catalog(outgoing)
            .map_err(|error| error.to_string())?;
        let incoming = link.receive_catalog().map_err(|error| error.to_string())?;
        {
            let mut host = access()?;
            host.check_unregistered_catalog_peer(link.peer_node_id())?;
            link.apply_catalog(&mut host.runtime, incoming)
                .map_err(|error: CatalogLinkError| error.to_string())?;
        }
        Ok::<_, String>(revision)
    })();
    match exchange {
        Ok(revision) => Ok((link, revision)),
        Err(error) => {
            reconcile_failure(&mut link, &access);
            Err(error)
        }
    }
}
