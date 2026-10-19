//! Versioned Android composition boundary for private touchpad packets.
//!
//! The public Rust DTO deliberately contains only primitives that map to a
//! narrow JNI surface. Android framework objects never enter the core crates.

#![deny(unsafe_op_in_unsafe_fn)]

use std::{
    collections::BTreeSet,
    error::Error,
    fmt,
    net::{Ipv4Addr, SocketAddr, TcpListener},
    str::FromStr,
    time::Duration,
};

#[cfg(target_os = "android")]
use std::collections::BTreeMap;

use capyio_android_host::{
    AndroidCatalogProjectionError, AndroidMotionAction, AndroidMotionSample, AndroidPointerSample,
    AndroidToolType, AndroidTouchSurface, AndroidTouchpadCaptureError,
    AndroidTouchpadCaptureSession, decode_android_catalog_json,
};
use capyio_control_credentials::{ControlCredentialBundle, ControlCredentialError};
use capyio_control_link::{CatalogControlLink, CatalogLinkError};
use capyio_control_tls::{
    AuthenticatedControlServer, AuthenticatedControlServerChannel, ControlTlsEndpoint,
    ControlTlsError, ControlTlsNodeBinding, ControlTlsServerIdentity,
};
use capyio_core::{CapabilityId, NodeId, PortId, PortRef, RouteId, SessionId, StreamId};
use capyio_input::{
    InputStreamDescriptor, TouchpadButtonType, TouchpadDescriptor, TouchpadPhysicalSize,
};
use capyio_pairing::{PairedPeerDirectory, PairingError};
use capyio_remote_touchpad_adapter::{
    PrivateTouchpadPacketSource, PrivateTouchpadPacketSourceError, PrivateTouchpadPacketV1,
    PrivateTouchpadRouteBinding, PrivateTouchpadTransportCodecV1,
    PrivateTouchpadTransportRecordError, PrivateTouchpadTransportRecordV1,
};
use capyio_runtime::{NodeRuntime, RuntimeError, RuntimeSnapshot};

#[cfg(target_os = "android")]
mod android_ffi;
mod control_exchange;
mod display_catalog;
mod touchpad_consent;
pub use control_exchange::{
    authenticate_android_control_dial, exchange_accepted_android_catalog,
    exchange_dialed_android_catalog,
};

pub const ANDROID_TOUCHPAD_JNI_CONTRACT_VERSION: u32 = 1;
pub const ANDROID_ACTION_DOWN: i32 = 0;
pub const ANDROID_ACTION_UP: i32 = 1;
pub const ANDROID_ACTION_MOVE: i32 = 2;
pub const ANDROID_ACTION_CANCEL: i32 = 3;
pub const ANDROID_ACTION_POINTER_DOWN: i32 = 5;
pub const ANDROID_ACTION_POINTER_UP: i32 = 6;
pub const ANDROID_TOOL_TYPE_FINGER: i32 = 1;
/// Stable routing preference, not an authentication or authorization signal.
pub const ANDROID_CONTROL_PREFERRED_PORT: u16 = 43193;

fn bind_control_listener_at(
    address: Ipv4Addr,
    preferred_port: u16,
) -> std::io::Result<TcpListener> {
    match TcpListener::bind((address, preferred_port)) {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            TcpListener::bind((address, 0))
        }
        Err(error) => Err(error),
    }
}

pub fn bind_android_control_listener() -> std::io::Result<TcpListener> {
    bind_control_listener_at(Ipv4Addr::UNSPECIFIED, ANDROID_CONTROL_PREFERRED_PORT)
}

#[cfg(test)]
mod control_bind_tests {
    use super::*;

    #[test]
    fn occupied_preferred_port_falls_back_without_stealing_existing_listener() {
        let first = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let preferred = first.local_addr().unwrap().port();
        let fallback = bind_control_listener_at(Ipv4Addr::LOCALHOST, preferred).unwrap();
        assert_ne!(fallback.local_addr().unwrap().port(), preferred);
        assert_eq!(first.local_addr().unwrap().port(), preferred);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AndroidPairingDirectoryError {
    InvalidLocalNodeId,
    Directory(PairingError),
}

impl fmt::Display for AndroidPairingDirectoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLocalNodeId => formatter.write_str("local Node ID is invalid"),
            Self::Directory(error) => error.fmt(formatter),
        }
    }
}

impl Error for AndroidPairingDirectoryError {}

pub fn canonicalize_android_pairing_directory(
    local_node_id: &str,
    json: &[u8],
) -> Result<Vec<u8>, AndroidPairingDirectoryError> {
    let local_node_id = NodeId::from_str(local_node_id)
        .map_err(|_| AndroidPairingDirectoryError::InvalidLocalNodeId)?;
    PairedPeerDirectory::decode_json(local_node_id, json)
        .and_then(|directory| directory.encode_json())
        .map_err(AndroidPairingDirectoryError::Directory)
}

pub fn android_pairing_directory_peer_count(
    local_node_id: &str,
    json: &[u8],
) -> Result<usize, AndroidPairingDirectoryError> {
    let local_node_id = NodeId::from_str(local_node_id)
        .map_err(|_| AndroidPairingDirectoryError::InvalidLocalNodeId)?;
    PairedPeerDirectory::decode_json(local_node_id, json)
        .map(|directory| directory.peers().len())
        .map_err(AndroidPairingDirectoryError::Directory)
}

pub fn android_pairing_directory_active_credential_handles(
    local_node_id: &str,
    json: &[u8],
) -> Result<Vec<String>, AndroidPairingDirectoryError> {
    let local_node_id = NodeId::from_str(local_node_id)
        .map_err(|_| AndroidPairingDirectoryError::InvalidLocalNodeId)?;
    PairedPeerDirectory::decode_json(local_node_id, json)
        .map(|directory| {
            directory
                .active_peers()
                .map(|peer| peer.credential_handle.as_str().to_owned())
                .collect()
        })
        .map_err(AndroidPairingDirectoryError::Directory)
}

#[derive(Debug)]
pub enum AndroidControlListenerError {
    InvalidLocalNodeId,
    Pairing(PairingError),
    NoActivePeer,
    MultipleActivePeers,
    Credential(ControlCredentialError),
    Transport(ControlTlsError),
    Runtime(AndroidCatalogRuntimeError),
}

#[derive(Debug)]
pub enum AndroidControlDialError {
    Pairing(PairingError),
    NoActivePeer,
    MultipleActivePeers,
    Credential(ControlCredentialError),
    Transport(ControlTlsError),
    Runtime(AndroidCatalogRuntimeError),
}

impl fmt::Display for AndroidControlDialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pairing(error) => error.fmt(formatter),
            Self::NoActivePeer => formatter.write_str("no active paired peer is configured"),
            Self::MultipleActivePeers => {
                formatter.write_str("multiple active peers require an explicit selection")
            }
            Self::Credential(error) => error.fmt(formatter),
            Self::Transport(error) => error.fmt(formatter),
            Self::Runtime(error) => error.fmt(formatter),
        }
    }
}

impl Error for AndroidControlDialError {}

impl fmt::Display for AndroidControlListenerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLocalNodeId => formatter.write_str("local Node ID is invalid"),
            Self::Pairing(error) => error.fmt(formatter),
            Self::NoActivePeer => formatter.write_str("no active paired peer is configured"),
            Self::MultipleActivePeers => formatter
                .write_str("multiple active paired peers require a listener-selection policy"),
            Self::Credential(error) => error.fmt(formatter),
            Self::Transport(error) => error.fmt(formatter),
            Self::Runtime(error) => error.fmt(formatter),
        }
    }
}

impl Error for AndroidControlListenerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Pairing(error) => Some(error),
            Self::Credential(error) => Some(error),
            Self::Transport(error) => Some(error),
            Self::Runtime(error) => Some(error),
            _ => None,
        }
    }
}

/// Socket-owning Android control acceptor for the currently supported
/// single-active-peer policy. It never advertises itself; discovery starts only
/// after the platform Service has retained this object and its bound port.
pub struct AndroidControlListenerV1 {
    server: AuthenticatedControlServer,
    touchpad_identity: ControlTlsServerIdentity,
    paired_display_name: String,
}

impl AndroidControlListenerV1 {
    pub fn from_listener(
        listener: TcpListener,
        local_node_id: &str,
        directory_json: &[u8],
        encoded_credential: &[u8],
        io_timeout: Duration,
    ) -> Result<Self, AndroidControlListenerError> {
        let local_node_id = NodeId::from_str(local_node_id)
            .map_err(|_| AndroidControlListenerError::InvalidLocalNodeId)?;
        let directory = PairedPeerDirectory::decode_json(local_node_id, directory_json)
            .map_err(AndroidControlListenerError::Pairing)?;
        let mut active = directory.active_peers();
        let peer = active
            .next()
            .ok_or(AndroidControlListenerError::NoActivePeer)?;
        if active.next().is_some() {
            return Err(AndroidControlListenerError::MultipleActivePeers);
        }
        let expected_nodes = ControlTlsNodeBinding::new(local_node_id, peer.node_id)
            .map_err(AndroidControlListenerError::Transport)?;
        let bundle = ControlCredentialBundle::decode(encoded_credential)
            .map_err(AndroidControlListenerError::Credential)?;
        let identity = ControlTlsServerIdentity::from_bundle(peer.peer_certificate_sha256, bundle);
        let server = AuthenticatedControlServer::from_listener_fresh_session(
            listener,
            identity.clone(),
            expected_nodes,
            io_timeout,
        )
        .map_err(AndroidControlListenerError::Transport)?;
        Ok(Self {
            server,
            touchpad_identity: identity,
            paired_display_name: peer.display_name.clone(),
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, AndroidControlListenerError> {
        self.server
            .local_addr()
            .map_err(AndroidControlListenerError::Transport)
    }

    /// Performs only transport authentication. No Runtime lock is needed while
    /// waiting for a peer's TLS/Hello messages.
    pub fn try_accept_authenticated(
        &self,
    ) -> Result<Option<AuthenticatedControlServerChannel>, AndroidControlListenerError> {
        match self.server.accept() {
            Ok(channel) => Ok(Some(channel)),
            Err(ControlTlsError::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                Ok(None)
            }
            Err(error) => Err(AndroidControlListenerError::Transport(error)),
        }
    }

    pub fn paired_display_name(&self) -> &str {
        &self.paired_display_name
    }

    pub fn touchpad_listener(
        &self,
        binding: capyio_control_tls::touchpad::TouchpadTlsBinding,
    ) -> Result<capyio_control_tls::touchpad::TouchpadTlsListener, String> {
        let socket =
            TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, 0)).map_err(|e| e.to_string())?;
        capyio_control_tls::touchpad::TouchpadTlsListener::from_listener(
            socket,
            self.touchpad_identity.clone(),
            binding,
            Duration::from_secs(2),
        )
        .map_err(|e| e.to_string())
    }

    pub fn haptics_listener(
        &self,
        binding: capyio_control_tls::haptics::HapticsTlsBinding,
    ) -> Result<capyio_control_tls::haptics::HapticsTlsListener, String> {
        let socket = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).map_err(|e| e.to_string())?;
        capyio_control_tls::haptics::HapticsTlsListener::from_listener(
            socket,
            self.touchpad_identity.clone(),
            binding,
            Duration::from_secs(2),
        )
        .map_err(|e| e.to_string())
    }

    pub fn accept_and_exchange(
        &self,
        runtime: &mut AndroidCatalogRuntimeV1,
        selected_capabilities: &BTreeSet<CapabilityId>,
    ) -> Result<CatalogControlLink, AndroidControlListenerError> {
        let channel = self
            .server
            .accept()
            .map_err(AndroidControlListenerError::Transport)?;
        runtime
            .admit_accepted_catalog_exchange(
                channel,
                self.paired_display_name.clone(),
                selected_capabilities,
            )
            .map_err(AndroidControlListenerError::Runtime)
    }

    pub fn try_accept_and_exchange(
        &self,
        runtime: &mut AndroidCatalogRuntimeV1,
    ) -> Result<Option<CatalogControlLink>, AndroidControlListenerError> {
        let channel = match self.server.accept() {
            Ok(channel) => channel,
            Err(ControlTlsError::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                return Ok(None);
            }
            Err(error) => return Err(AndroidControlListenerError::Transport(error)),
        };
        let local = runtime.snapshot().local_node;
        let selected = local
            .capabilities
            .iter()
            .filter(|(_, capability)| {
                capability
                    .ports
                    .values()
                    .any(|port| port.direction == capyio_core::PortDirection::Source)
            })
            .map(|(id, _)| *id)
            .collect();
        runtime
            .admit_accepted_catalog_exchange(channel, self.paired_display_name.clone(), &selected)
            .map(Some)
            .map_err(AndroidControlListenerError::Runtime)
    }
}

#[derive(Debug)]
pub enum AndroidCatalogRuntimeError {
    Catalog(AndroidCatalogProjectionError),
    Runtime(RuntimeError),
    NodeIdentityChanged,
    ConnectedState,
    ControlLink(CatalogLinkError),
    PublicationRevisionExhausted,
}

impl fmt::Display for AndroidCatalogRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => error.fmt(formatter),
            Self::Runtime(error) => error.fmt(formatter),
            Self::NodeIdentityChanged => {
                formatter.write_str("replacement catalog changed the Android Node identity")
            }
            Self::ConnectedState => formatter.write_str(
                "the local catalog can only be replaced before peers, Sessions or Routes exist",
            ),
            Self::PublicationRevisionExhausted => {
                formatter.write_str("local catalog publication revision exhausted")
            }
            Self::ControlLink(error) => error.fmt(formatter),
        }
    }
}

impl Error for AndroidCatalogRuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Catalog(error) => Some(error),
            Self::Runtime(error) => Some(error),
            Self::ControlLink(error) => Some(error),
            _ => None,
        }
    }
}

impl From<AndroidCatalogProjectionError> for AndroidCatalogRuntimeError {
    fn from(value: AndroidCatalogProjectionError) -> Self {
        Self::Catalog(value)
    }
}

impl From<RuntimeError> for AndroidCatalogRuntimeError {
    fn from(value: RuntimeError) -> Self {
        Self::Runtime(value)
    }
}

impl From<CatalogLinkError> for AndroidCatalogRuntimeError {
    fn from(value: CatalogLinkError) -> Self {
        Self::ControlLink(value)
    }
}

#[cfg(target_os = "android")]
#[derive(Clone, Debug)]
struct AndroidAudioUse {
    peer_route: capyio_core::Route,
    offer: capyio_protocol::NativeAudioDataOffer,
    peer_address: String,
    local_port: Option<u16>,
    offer_pending: bool,
    observed_state: capyio_core::RouteState,
}

#[cfg(target_os = "android")]
#[derive(Clone, Debug)]
struct AndroidCameraUse {
    route_id: capyio_core::RouteId,
    offer: Option<(capyio_protocol::CameraDataOffer, String, u64)>,
    local_intent: bool,
}

#[derive(Clone, Debug)]
pub struct AndroidCatalogRuntimeV1 {
    display_use: display_catalog::DisplayUse,
    #[cfg(target_os = "android")]
    audio_uses: BTreeMap<capyio_core::RouteId, AndroidAudioUse>,
    #[cfg(target_os = "android")]
    camera_use: Option<AndroidCameraUse>,
    #[cfg(target_os = "android")]
    audio_local_intent: BTreeSet<capyio_core::RouteId>,
    audio_status_pending: BTreeSet<capyio_core::RouteId>,
    #[cfg(target_os = "android")]
    controller_use: Option<capyio_core::RouteId>,
    #[cfg(target_os = "android")]
    controller_local_intent: Option<capyio_core::RouteId>,
    #[cfg(target_os = "android")]
    controller_status_pending: Option<capyio_core::RouteId>,
    #[cfg(target_os = "android")]
    controller_observed_state: Option<(capyio_core::RouteId, capyio_core::RouteState)>,
    #[cfg(target_os = "android")]
    controller_settings: Option<capyio_protocol::v1::ControllerSettingsState>,
    #[cfg(target_os = "android")]
    controller_settings_request_origin: Option<(capyio_core::SessionId, u64)>,
    #[cfg(target_os = "android")]
    controller_settings_request: Option<(
        capyio_core::SessionId,
        capyio_protocol::v1::ControllerSettingsRequest,
    )>,
    #[cfg(target_os = "android")]
    controller_receiver: Option<(
        capyio_core::SessionId,
        capyio_protocol::v1::ControllerReceiverOffer,
    )>,
    runtime: NodeRuntime,
    touchpad_consent: touchpad_consent::TouchpadConsentGate,
    publication_revision: u64,
}

#[cfg(test)]
mod connected_catalog_tests {
    use super::*;
    const FIXTURE: &str = include_str!("../../fixtures/android-core-catalog-v1.json");

    #[test]
    fn controller_status_publication_does_not_select_pure_sinks() {
        let host = AndroidCatalogRuntimeV1::new(FIXTURE).unwrap();
        let selected = host.published_source_capabilities();
        let local = host.snapshot().local_node;
        assert!(!selected.is_empty());
        assert!(selected.len() < local.capabilities.len());
        for capability in local.capabilities.values() {
            assert_eq!(
                selected.contains(&capability.id),
                capability
                    .ports
                    .values()
                    .any(|port| port.direction == capyio_core::PortDirection::Source)
            );
        }
    }

    #[test]
    fn android_catalog_update_retains_an_existing_peer_and_session() {
        let mut host = AndroidCatalogRuntimeV1::new(FIXTURE).unwrap();
        let mut peer = host.snapshot().local_node;
        peer.id = NodeId::new();
        let peer_id = peer.id;
        host.runtime.register_peer(peer, true).unwrap();
        host.runtime.open_session(peer_id).unwrap();
        let before = host.snapshot();
        host.replace_catalog(&FIXTURE.replace("CapyIO Android", "Updated Android"))
            .unwrap();
        let after = host.snapshot();
        assert_eq!(after.local_node.display_name, "Updated Android");
        assert_eq!(after.peers, before.peers);
        assert_eq!(after.sessions, before.sessions);
        assert_eq!(after.routes, before.routes);
        assert!(host.replace_catalog("{}").is_err());
        assert_eq!(host.snapshot(), after);
        assert!(matches!(
            host.replace_disconnected_catalog(FIXTURE),
            Err(AndroidCatalogRuntimeError::ConnectedState)
        ));
    }
}

impl AndroidCatalogRuntimeV1 {
    pub fn close_control_link(
        &mut self,
        mut link: CatalogControlLink,
    ) -> Result<(), AndroidCatalogRuntimeError> {
        link.close_and_reconcile(&mut self.runtime)?;
        Ok(())
    }

    /// Android-side inverse of the accepted control path. The endpoint is only
    /// a routing hint; exact active pairing, pinned mTLS and Node binding
    /// remain mandatory before any catalog is applied.
    pub fn dial_paired_catalog_exchange(
        &mut self,
        endpoint: ControlTlsEndpoint,
        directory_json: &[u8],
        encoded_credential: &[u8],
        selected_capabilities: &BTreeSet<CapabilityId>,
    ) -> Result<CatalogControlLink, AndroidControlDialError> {
        let (channel, display_name) = authenticate_android_control_dial(
            self.runtime.snapshot().local_node.id,
            endpoint,
            directory_json,
            encoded_credential,
        )?;
        let mut link =
            CatalogControlLink::admit_dialed_peer(&mut self.runtime, channel, display_name)
                .map_err(|error| AndroidControlDialError::Runtime(error.into()))?;
        let exchange = (|| -> Result<(), CatalogLinkError> {
            let outgoing = link.prepare_catalog(&self.runtime, selected_capabilities)?;
            link.send_catalog(outgoing)?;
            let incoming = link.receive_catalog()?;
            link.apply_catalog(&mut self.runtime, incoming)?;
            Ok(())
        })();
        if let Err(error) = exchange {
            let _ = link.close_and_reconcile(&mut self.runtime);
            return Err(AndroidControlDialError::Runtime(error.into()));
        }
        Ok(link)
    }

    pub fn touchpad_requires_confirmation(&self, route: &capyio_core::Route) -> bool {
        touchpad_consent::is_local_touchpad(&self.runtime, route)
    }

    pub fn resource_requires_confirmation(&self, route: &capyio_core::Route) -> bool {
        touchpad_consent::TouchpadConsentGate::requires_confirmation(&self.runtime, route)
    }

    pub fn enqueue_touchpad_request(
        &mut self,
        route_id: RouteId,
        now_ms: u64,
    ) -> Result<(), String> {
        self.touchpad_consent
            .enqueue(&mut self.runtime, route_id, now_ms)
    }

    pub fn expire_touchpad_requests(&mut self, now_ms: u64) {
        self.touchpad_consent.expire(&mut self.runtime, now_ms);
    }

    pub fn resource_requests_json(&mut self, now_ms: u64) -> Result<Vec<u8>, String> {
        serde_json::to_vec(&self.touchpad_consent.requests(&mut self.runtime, now_ms))
            .map_err(|e| e.to_string())
    }

    pub fn resolve_touchpad_request(
        &mut self,
        route_id: RouteId,
        allow: bool,
        now_ms: u64,
    ) -> Result<(), String> {
        if self.display_use.route == route_id.to_string() {
            self.touchpad_consent.resolve_bounded(
                &mut self.runtime,
                route_id,
                allow,
                now_ms,
                Some(
                    self.display_use
                        .peer_deadline
                        .unwrap_or(now_ms.saturating_add(600_000)),
                ),
            )?;
        } else {
            self.touchpad_consent
                .resolve(&mut self.runtime, route_id, allow, now_ms)?;
        }
        // Consent may be resolved asynchronously by UI or remembered permission,
        // before any media offer/job exists. Publish the grant independently.
        self.audio_status_pending.insert(route_id);
        Ok(())
    }

    pub fn new(json: &str) -> Result<Self, AndroidCatalogRuntimeError> {
        let mut node = decode_android_catalog_json(json)?;
        display_catalog::install(&mut node);
        Ok(Self {
            display_use: display_catalog::DisplayUse::default(),
            runtime: NodeRuntime::new(node)?,
            #[cfg(target_os = "android")]
            audio_uses: BTreeMap::new(),
            #[cfg(target_os = "android")]
            camera_use: None,
            #[cfg(target_os = "android")]
            audio_local_intent: BTreeSet::new(),
            audio_status_pending: BTreeSet::new(),
            touchpad_consent: touchpad_consent::TouchpadConsentGate::default(),
            publication_revision: 0,
            #[cfg(target_os = "android")]
            controller_receiver: None,
            #[cfg(target_os = "android")]
            controller_use: None,
            #[cfg(target_os = "android")]
            controller_local_intent: None,
            #[cfg(target_os = "android")]
            controller_status_pending: None,
            #[cfg(target_os = "android")]
            controller_observed_state: None,
            #[cfg(target_os = "android")]
            controller_settings: None,
            #[cfg(target_os = "android")]
            controller_settings_request_origin: None,
            #[cfg(target_os = "android")]
            controller_settings_request: None,
        })
    }

    pub fn replace_disconnected_catalog(
        &mut self,
        json: &str,
    ) -> Result<(), AndroidCatalogRuntimeError> {
        let mut node = decode_android_catalog_json(json)?;
        display_catalog::install(&mut node);
        let current = self.runtime.snapshot();
        if node.id != current.local_node.id {
            return Err(AndroidCatalogRuntimeError::NodeIdentityChanged);
        }
        if !current.peers.is_empty() || !current.sessions.is_empty() || !current.routes.is_empty() {
            return Err(AndroidCatalogRuntimeError::ConnectedState);
        }
        let revision = self.next_publication_revision()?;
        self.runtime.replace_local_catalog(node)?;
        self.publication_revision = revision;
        Ok(())
    }

    /// Host-owned inventory updates remain valid while authenticated peers and
    /// Routes exist; Runtime reconciles only affected local endpoints.
    pub fn replace_catalog(&mut self, json: &str) -> Result<(), AndroidCatalogRuntimeError> {
        let mut node = decode_android_catalog_json(json)?;
        display_catalog::install(&mut node);
        if node.id != self.runtime.snapshot().local_node.id {
            return Err(AndroidCatalogRuntimeError::NodeIdentityChanged);
        }
        let revision = self.next_publication_revision()?;
        self.runtime.replace_local_catalog(node)?;
        self.publication_revision = revision;
        Ok(())
    }

    fn next_publication_revision(&self) -> Result<u64, AndroidCatalogRuntimeError> {
        self.publication_revision
            .checked_add(1)
            .ok_or(AndroidCatalogRuntimeError::PublicationRevisionExhausted)
    }

    pub fn publication_revision(&self) -> u64 {
        self.publication_revision
    }

    /// The publication selector names Sources only; compatible Sinks are
    /// automatically retained by the catalog encoder and must not be selected.
    pub fn published_source_capabilities(&self) -> BTreeSet<CapabilityId> {
        self.runtime
            .snapshot()
            .local_node
            .capabilities
            .iter()
            .filter(|(_, capability)| {
                capability
                    .ports
                    .values()
                    .any(|port| port.direction == capyio_core::PortDirection::Source)
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// Completes the first catalog exchange for an already mutually
    /// authenticated accepted channel. Acceptor ordering receives first, then
    /// sends the publication-filtered local catalog. Any failure closes and
    /// reconciles the exact admitted Session before returning.
    pub fn admit_accepted_catalog_exchange(
        &mut self,
        channel: AuthenticatedControlServerChannel,
        paired_display_name: String,
        selected_capabilities: &BTreeSet<CapabilityId>,
    ) -> Result<CatalogControlLink, AndroidCatalogRuntimeError> {
        let mut link = CatalogControlLink::admit_accepted_peer(
            &mut self.runtime,
            channel,
            paired_display_name,
        )?;
        let exchange = (|| -> Result<(), CatalogLinkError> {
            let incoming = link.receive_catalog()?;
            link.apply_catalog(&mut self.runtime, incoming)?;
            let outgoing = link.prepare_catalog(&self.runtime, selected_capabilities)?;
            link.send_catalog(outgoing)?;
            Ok(())
        })();
        if let Err(error) = exchange {
            let _ = link.close_and_reconcile(&mut self.runtime);
            return Err(AndroidCatalogRuntimeError::ControlLink(error));
        }
        Ok(link)
    }

    #[must_use]
    pub fn snapshot(&self) -> RuntimeSnapshot {
        self.runtime.snapshot()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AndroidTouchpadBridgeConfigV1 {
    pub stream_id: String,
    pub stream_epoch: u64,
    pub clock_domain_id: String,
    pub width_px: u32,
    pub height_px: u32,
    pub width_himetric: u32,
    pub height_himetric: u32,
    pub max_contacts: u8,
    pub reports_pressure: bool,
    pub first_sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AndroidTouchpadRouteConfigV1 {
    pub route_id: String,
    pub session_id: String,
    pub source_node_id: String,
    pub source_capability_id: String,
    pub source_port_id: String,
    pub sink_node_id: String,
    pub sink_capability_id: String,
    pub sink_port_id: String,
    pub authorization_expires_at_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AndroidMotionDtoV1<'a> {
    pub event_time_nanos: u64,
    pub action: i32,
    pub action_index: usize,
    pub pointer_ids: &'a [i32],
    pub tool_types: &'a [i32],
    pub x_px: &'a [f32],
    pub y_px: &'a [f32],
    /// Negative values mean absent; non-negative values carry Android pressure.
    pub pressure: &'a [f32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AndroidTouchpadBridgeError {
    InvalidConfig(String),
    InvalidMotion(String),
    Capture(AndroidTouchpadCaptureError),
    Packet(PrivateTouchpadPacketSourceError),
    Transport(PrivateTouchpadTransportRecordError),
}

impl fmt::Display for AndroidTouchpadBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(formatter, "invalid bridge config: {message}"),
            Self::InvalidMotion(message) => write!(formatter, "invalid motion DTO: {message}"),
            Self::Capture(error) => error.fmt(formatter),
            Self::Packet(error) => error.fmt(formatter),
            Self::Transport(error) => error.fmt(formatter),
        }
    }
}

impl Error for AndroidTouchpadBridgeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Capture(error) => Some(error),
            Self::Packet(error) => Some(error),
            Self::Transport(error) => Some(error),
            _ => None,
        }
    }
}

impl From<AndroidTouchpadCaptureError> for AndroidTouchpadBridgeError {
    fn from(value: AndroidTouchpadCaptureError) -> Self {
        Self::Capture(value)
    }
}

impl From<PrivateTouchpadPacketSourceError> for AndroidTouchpadBridgeError {
    fn from(value: PrivateTouchpadPacketSourceError) -> Self {
        Self::Packet(value)
    }
}

impl From<PrivateTouchpadTransportRecordError> for AndroidTouchpadBridgeError {
    fn from(value: PrivateTouchpadTransportRecordError) -> Self {
        Self::Transport(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AndroidTouchpadPacketSessionV1 {
    capture: AndroidTouchpadCaptureSession,
    source: PrivateTouchpadPacketSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AndroidTouchpadRecordSessionV1 {
    packets: AndroidTouchpadPacketSessionV1,
    transport: PrivateTouchpadTransportCodecV1,
}

impl AndroidTouchpadRecordSessionV1 {
    pub fn new(
        packet_config: AndroidTouchpadBridgeConfigV1,
        route_config: AndroidTouchpadRouteConfigV1,
    ) -> Result<Self, AndroidTouchpadBridgeError> {
        let epoch = packet_config.stream_epoch;
        let binding = PrivateTouchpadRouteBinding {
            route_id: parse_id::<RouteId>(&route_config.route_id, "route ID")?,
            session_id: parse_id::<SessionId>(&route_config.session_id, "session ID")?,
            source: parse_port(
                &route_config.source_node_id,
                &route_config.source_capability_id,
                &route_config.source_port_id,
                "source",
            )?,
            sink: parse_port(
                &route_config.sink_node_id,
                &route_config.sink_capability_id,
                &route_config.sink_port_id,
                "sink",
            )?,
            route_epoch: epoch,
            authorization_expires_at_ms: route_config.authorization_expires_at_ms,
        };
        Ok(Self {
            packets: AndroidTouchpadPacketSessionV1::new(packet_config)?,
            transport: PrivateTouchpadTransportCodecV1::new(binding),
        })
    }

    #[must_use]
    pub fn hello(&self) -> PrivateTouchpadTransportRecordV1 {
        self.transport.encode_hello()
    }

    pub fn start(
        &mut self,
        event_time_nanos: u64,
    ) -> Result<PrivateTouchpadTransportRecordV1, AndroidTouchpadBridgeError> {
        let packet = self.packets.start(event_time_nanos)?;
        Ok(self.transport.encode_data(&packet)?)
    }

    pub fn motion(
        &mut self,
        dto: AndroidMotionDtoV1<'_>,
    ) -> Result<PrivateTouchpadTransportRecordV1, AndroidTouchpadBridgeError> {
        let packet = self.packets.motion(dto)?;
        Ok(self.transport.encode_data(&packet)?)
    }

    pub fn stop(
        &mut self,
        event_time_nanos: u64,
    ) -> Result<Option<PrivateTouchpadTransportRecordV1>, AndroidTouchpadBridgeError> {
        self.packets
            .stop(event_time_nanos)?
            .map(|packet| self.transport.encode_data(&packet).map_err(Into::into))
            .transpose()
    }

    pub fn close(
        &mut self,
        event_time_nanos: u64,
    ) -> Result<Option<PrivateTouchpadTransportRecordV1>, AndroidTouchpadBridgeError> {
        self.packets
            .close(event_time_nanos)?
            .map(|packet| self.transport.encode_data(&packet).map_err(Into::into))
            .transpose()
    }

    #[must_use]
    pub fn close_record(&self) -> PrivateTouchpadTransportRecordV1 {
        self.transport.encode_close()
    }

    pub fn validate_ack(
        &self,
        bytes: &[u8],
        expected_sequence: u64,
    ) -> Result<(), AndroidTouchpadBridgeError> {
        Ok(self.transport.validate_ack(bytes, expected_sequence)?)
    }
}

fn parse_port(
    node: &str,
    capability: &str,
    port: &str,
    label: &str,
) -> Result<PortRef, AndroidTouchpadBridgeError> {
    Ok(PortRef {
        node_id: parse_id::<NodeId>(node, &format!("{label} node ID"))?,
        capability_id: parse_id::<CapabilityId>(capability, &format!("{label} capability ID"))?,
        port_id: parse_id::<PortId>(port, &format!("{label} port ID"))?,
    })
}

fn parse_id<T>(value: &str, label: &str) -> Result<T, AndroidTouchpadBridgeError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    value.parse().map_err(|error: T::Err| {
        AndroidTouchpadBridgeError::InvalidConfig(format!("{label}: {error}"))
    })
}

impl AndroidTouchpadPacketSessionV1 {
    pub fn new(config: AndroidTouchpadBridgeConfigV1) -> Result<Self, AndroidTouchpadBridgeError> {
        let stream_id = StreamId::from_str(&config.stream_id)
            .map_err(|error| AndroidTouchpadBridgeError::InvalidConfig(error.to_string()))?;
        let stream = InputStreamDescriptor {
            stream_id,
            stream_epoch: config.stream_epoch,
            clock_domain_id: config.clock_domain_id,
        };
        let descriptor = TouchpadDescriptor {
            physical_size: TouchpadPhysicalSize {
                width_himetric: config.width_himetric,
                height_himetric: config.height_himetric,
            },
            max_contacts: config.max_contacts,
            button_type: TouchpadButtonType::NonClickable,
            reports_contact_size: false,
            reports_pressure: config.reports_pressure,
        };
        let surface = AndroidTouchSurface {
            width_px: config.width_px,
            height_px: config.height_px,
            descriptor,
        };
        Ok(Self {
            capture: AndroidTouchpadCaptureSession::new(
                stream.clone(),
                surface,
                config.first_sequence,
            )?,
            source: PrivateTouchpadPacketSource::new(stream, descriptor, config.first_sequence)?,
        })
    }

    pub fn start(
        &mut self,
        event_time_nanos: u64,
    ) -> Result<PrivateTouchpadPacketV1, AndroidTouchpadBridgeError> {
        let frame = self.capture.start(event_time_nanos)?;
        Ok(self.source.encode(&frame)?)
    }

    pub fn motion(
        &mut self,
        dto: AndroidMotionDtoV1<'_>,
    ) -> Result<PrivateTouchpadPacketV1, AndroidTouchpadBridgeError> {
        let event = map_motion_dto(dto)?;
        let frame = self.capture.map_motion(&event)?;
        Ok(self.source.encode(&frame)?)
    }

    pub fn stop(
        &mut self,
        event_time_nanos: u64,
    ) -> Result<Option<PrivateTouchpadPacketV1>, AndroidTouchpadBridgeError> {
        self.capture
            .stop(event_time_nanos)?
            .map(|frame| self.source.encode(&frame).map_err(Into::into))
            .transpose()
    }

    pub fn close(
        &mut self,
        event_time_nanos: u64,
    ) -> Result<Option<PrivateTouchpadPacketV1>, AndroidTouchpadBridgeError> {
        let packet = self
            .capture
            .close(event_time_nanos)?
            .map(|frame| {
                self.source
                    .encode(&frame)
                    .map_err(AndroidTouchpadBridgeError::from)
            })
            .transpose()?;
        self.source.close()?;
        Ok(packet)
    }
}

fn map_motion_dto(
    dto: AndroidMotionDtoV1<'_>,
) -> Result<AndroidMotionSample, AndroidTouchpadBridgeError> {
    let count = dto.pointer_ids.len();
    if [
        dto.tool_types.len(),
        dto.x_px.len(),
        dto.y_px.len(),
        dto.pressure.len(),
    ]
    .into_iter()
    .any(|len| len != count)
    {
        return Err(AndroidTouchpadBridgeError::InvalidMotion(
            "all pointer arrays must have identical lengths".to_owned(),
        ));
    }
    let action = match dto.action {
        ANDROID_ACTION_DOWN => AndroidMotionAction::Down,
        ANDROID_ACTION_UP => AndroidMotionAction::Up {
            action_index: dto.action_index,
        },
        ANDROID_ACTION_MOVE => AndroidMotionAction::Move,
        ANDROID_ACTION_CANCEL => AndroidMotionAction::Cancel,
        ANDROID_ACTION_POINTER_DOWN => AndroidMotionAction::PointerDown {
            action_index: dto.action_index,
        },
        ANDROID_ACTION_POINTER_UP => AndroidMotionAction::PointerUp {
            action_index: dto.action_index,
        },
        actual => {
            return Err(AndroidTouchpadBridgeError::InvalidMotion(format!(
                "unsupported MotionEvent action {actual}"
            )));
        }
    };
    let pointers = (0..count)
        .map(|index| {
            let pointer_id = u32::try_from(dto.pointer_ids[index]).map_err(|_| {
                AndroidTouchpadBridgeError::InvalidMotion(
                    "pointer IDs must be non-negative".to_owned(),
                )
            })?;
            let tool_type = match dto.tool_types[index] {
                ANDROID_TOOL_TYPE_FINGER => AndroidToolType::Finger,
                2 => AndroidToolType::Stylus,
                3 => AndroidToolType::Mouse,
                4 => AndroidToolType::Eraser,
                _ => AndroidToolType::Unknown,
            };
            let pressure = (dto.pressure[index] >= 0.0).then_some(dto.pressure[index]);
            Ok(AndroidPointerSample {
                pointer_id,
                tool_type,
                x_px: dto.x_px[index],
                y_px: dto.y_px[index],
                pressure,
            })
        })
        .collect::<Result<Vec<_>, AndroidTouchpadBridgeError>>()?;
    Ok(AndroidMotionSample {
        event_time_nanos: dto.event_time_nanos,
        action,
        pointers,
    })
}
