#![forbid(unsafe_code)]

//! Mutually authenticated TLS 1.3 framing for CapyIO control Envelopes.
//! Credential enrollment and persistence are deliberately caller-owned.

pub mod display;
pub mod haptics;
mod route_tls;
pub mod touchpad;

use std::{
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::Arc,
    time::{Duration, Instant},
};

use capyio_control_credentials::ControlCredentialBundle;
use capyio_core::{NodeId, SessionId};
pub use capyio_pairing::CertificateFingerprint;
use capyio_protocol::{MAX_CONTROL_ENVELOPE_BYTES, decode_envelope};
use rustls::{
    ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
    pki_types::{CertificateDer, PrivateKeyDer, ServerName},
    server::WebPkiClientVerifier,
    version::TLS13,
};
use thiserror::Error;

pub const CONTROL_TLS_ALPN: &[u8] = b"capyio-control/1";
pub const CONTROL_BIND_BYTES: usize = 64;
const BIND_MAGIC: [u8; 8] = *b"CPYCTRL1";
const BIND_VERSION: u8 = 1;
const BIND_REQUEST: u8 = 1;
const BIND_ACK: u8 = 2;
const BIND_BODY_BYTES: u32 = 48;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlTlsBinding {
    pub session_id: SessionId,
    pub server_node_id: NodeId,
    pub client_node_id: NodeId,
}

/// Expected Node identities for a fresh Session proposed inside an already
/// mutually authenticated TLS connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlTlsNodeBinding {
    pub server_node_id: NodeId,
    pub client_node_id: NodeId,
}

impl ControlTlsNodeBinding {
    pub fn new(server_node_id: NodeId, client_node_id: NodeId) -> Result<Self, ControlTlsError> {
        if server_node_id.as_uuid().is_nil()
            || client_node_id.as_uuid().is_nil()
            || server_node_id == client_node_id
        {
            return Err(ControlTlsError::InvalidBinding);
        }
        Ok(Self {
            server_node_id,
            client_node_id,
        })
    }

    pub fn with_session(self, session_id: SessionId) -> Result<ControlTlsBinding, ControlTlsError> {
        ControlTlsBinding::new(session_id, self.server_node_id, self.client_node_id)
    }

    fn admits(self, binding: ControlTlsBinding) -> bool {
        binding.server_node_id == self.server_node_id
            && binding.client_node_id == self.client_node_id
    }
}

impl ControlTlsBinding {
    pub fn new(
        session_id: SessionId,
        server_node_id: NodeId,
        client_node_id: NodeId,
    ) -> Result<Self, ControlTlsError> {
        if session_id.as_uuid().is_nil()
            || server_node_id.as_uuid().is_nil()
            || client_node_id.as_uuid().is_nil()
            || server_node_id == client_node_id
        {
            return Err(ControlTlsError::InvalidBinding);
        }
        Ok(Self {
            session_id,
            server_node_id,
            client_node_id,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlTlsEndpoint {
    pub address: SocketAddr,
    pub io_timeout: Duration,
}

impl ControlTlsEndpoint {
    pub fn validate(self) -> Result<(), ControlTlsError> {
        if self.address.ip().is_unspecified()
            || self.address.ip().is_multicast()
            || self.address.port() == 0
        {
            return Err(ControlTlsError::InvalidEndpoint);
        }
        if !(Duration::from_millis(100)..=Duration::from_secs(30)).contains(&self.io_timeout) {
            return Err(ControlTlsError::InvalidTimeout);
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct ControlTlsServerIdentity {
    /// Exact paired client leaf certificate, independent of its issuing root.
    pub expected_peer_certificate: CertificateFingerprint,
    pub certificate_chain_der: Vec<Vec<u8>>,
    pub private_key_der: Vec<u8>,
    /// Trust root dedicated to the already-paired client identity.
    pub client_ca_der: Vec<u8>,
}

#[derive(Clone)]
pub struct ControlTlsClientIdentity {
    /// Exact paired server leaf certificate, independent of its issuing root.
    pub expected_peer_certificate: CertificateFingerprint,
    pub certificate_chain_der: Vec<Vec<u8>>,
    pub private_key_der: Vec<u8>,
    /// Trust root dedicated to the already-paired server identity.
    pub server_ca_der: Vec<u8>,
}

impl ControlTlsServerIdentity {
    #[must_use]
    pub fn from_bundle(
        expected_peer_certificate: CertificateFingerprint,
        bundle: ControlCredentialBundle,
    ) -> Self {
        let (certificate_chain_der, private_key_der, client_ca_der) = bundle.into_parts();
        Self {
            expected_peer_certificate,
            certificate_chain_der,
            private_key_der,
            client_ca_der,
        }
    }
}

impl ControlTlsClientIdentity {
    #[must_use]
    pub fn from_bundle(
        expected_peer_certificate: CertificateFingerprint,
        bundle: ControlCredentialBundle,
    ) -> Self {
        let (certificate_chain_der, private_key_der, server_ca_der) = bundle.into_parts();
        Self {
            expected_peer_certificate,
            certificate_chain_der,
            private_key_der,
            server_ca_der,
        }
    }
}

pub struct AuthenticatedControlServer {
    expected_peer_certificate: CertificateFingerprint,
    listener: TcpListener,
    config: Arc<ServerConfig>,
    expected: ServerBindingExpectation,
    io_timeout: Duration,
}

#[derive(Clone, Copy)]
enum ServerBindingExpectation {
    Exact(ControlTlsBinding),
    FreshSession(ControlTlsNodeBinding),
}

pub struct AuthenticatedControlServerChannel {
    stream: FramedControl<StreamOwned<ServerConnection, TcpStream>>,
    binding: ControlTlsBinding,
}

pub struct AuthenticatedControlClientChannel {
    stream: FramedControl<StreamOwned<ClientConnection, TcpStream>>,
    binding: ControlTlsBinding,
}

impl AuthenticatedControlServer {
    pub fn bind(
        endpoint: ControlTlsEndpoint,
        identity: ControlTlsServerIdentity,
        expected: ControlTlsBinding,
    ) -> Result<Self, ControlTlsError> {
        endpoint.validate()?;
        let listener = TcpListener::bind(endpoint.address)?;
        Self::from_listener(listener, identity, expected, endpoint.io_timeout)
    }

    pub fn from_listener(
        listener: TcpListener,
        identity: ControlTlsServerIdentity,
        expected: ControlTlsBinding,
        io_timeout: Duration,
    ) -> Result<Self, ControlTlsError> {
        validate_timeout(io_timeout)?;
        Ok(Self {
            listener,
            expected_peer_certificate: identity.expected_peer_certificate,
            config: build_server_config(identity)?,
            expected: ServerBindingExpectation::Exact(expected),
            io_timeout,
        })
    }

    pub fn bind_fresh_session(
        endpoint: ControlTlsEndpoint,
        identity: ControlTlsServerIdentity,
        expected_nodes: ControlTlsNodeBinding,
    ) -> Result<Self, ControlTlsError> {
        endpoint.validate()?;
        let listener = TcpListener::bind(endpoint.address)?;
        Self::from_listener_fresh_session(listener, identity, expected_nodes, endpoint.io_timeout)
    }

    pub fn from_listener_fresh_session(
        listener: TcpListener,
        identity: ControlTlsServerIdentity,
        expected_nodes: ControlTlsNodeBinding,
        io_timeout: Duration,
    ) -> Result<Self, ControlTlsError> {
        validate_timeout(io_timeout)?;
        Ok(Self {
            listener,
            expected_peer_certificate: identity.expected_peer_certificate,
            config: build_server_config(identity)?,
            expected: ServerBindingExpectation::FreshSession(expected_nodes),
            io_timeout,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, ControlTlsError> {
        Ok(self.listener.local_addr()?)
    }

    pub fn accept(&self) -> Result<AuthenticatedControlServerChannel, ControlTlsError> {
        let (socket, _) = self.listener.accept()?;
        // A nonblocking accept loop may yield nonblocking sockets on Windows.
        // TLS framing uses bounded blocking reads/writes after acceptance.
        socket.set_nonblocking(false)?;
        socket.set_read_timeout(Some(self.io_timeout))?;
        socket.set_write_timeout(Some(self.io_timeout))?;
        socket.set_nodelay(true)?;
        let connection = ServerConnection::new(self.config.clone())?;
        let mut stream = StreamOwned::new(connection, socket);
        while stream.conn.is_handshaking() {
            stream.conn.complete_io(&mut stream.sock)?;
        }
        require_tls13_alpn(stream.conn.protocol_version(), stream.conn.alpn_protocol())?;
        verify_peer_certificate(
            stream.conn.peer_certificates(),
            self.expected_peer_certificate,
        )?;
        let mut request = [0_u8; CONTROL_BIND_BYTES];
        stream.read_exact(&mut request)?;
        require_tls13_alpn(stream.conn.protocol_version(), stream.conn.alpn_protocol())?;
        let binding = match self.expected {
            ServerBindingExpectation::Exact(expected) => {
                verify_bind(&request, expected, BIND_REQUEST)?;
                expected
            }
            ServerBindingExpectation::FreshSession(expected_nodes) => {
                let proposed = decode_bind(&request, BIND_REQUEST)?;
                if !expected_nodes.admits(proposed) {
                    return Err(ControlTlsError::UnauthorizedBinding);
                }
                proposed
            }
        };
        stream.write_all(&encode_bind(binding, BIND_ACK))?;
        stream.flush()?;
        Ok(AuthenticatedControlServerChannel {
            stream: FramedControl::new(stream),
            binding,
        })
    }
}

impl AuthenticatedControlClientChannel {
    pub fn connect_fresh_session(
        endpoint: ControlTlsEndpoint,
        server_name: &str,
        identity: ControlTlsClientIdentity,
        expected_nodes: ControlTlsNodeBinding,
    ) -> Result<Self, ControlTlsError> {
        let binding = expected_nodes.with_session(SessionId::new())?;
        Self::connect(endpoint, server_name, identity, binding)
    }

    pub fn connect(
        endpoint: ControlTlsEndpoint,
        server_name: &str,
        identity: ControlTlsClientIdentity,
        expected: ControlTlsBinding,
    ) -> Result<Self, ControlTlsError> {
        endpoint.validate()?;
        if server_name.is_empty() || server_name.len() > 253 {
            return Err(ControlTlsError::InvalidEndpoint);
        }
        let socket = TcpStream::connect_timeout(&endpoint.address, endpoint.io_timeout)?;
        socket.set_read_timeout(Some(endpoint.io_timeout))?;
        socket.set_write_timeout(Some(endpoint.io_timeout))?;
        socket.set_nodelay(true)?;
        let server_name = ServerName::try_from(server_name.to_owned())
            .map_err(|_| ControlTlsError::InvalidEndpoint)?;
        let expected_peer_certificate = identity.expected_peer_certificate;
        let connection = ClientConnection::new(build_client_config(identity)?, server_name)?;
        let mut stream = StreamOwned::new(connection, socket);
        while stream.conn.is_handshaking() {
            stream.conn.complete_io(&mut stream.sock)?;
        }
        require_tls13_alpn(stream.conn.protocol_version(), stream.conn.alpn_protocol())?;
        verify_peer_certificate(stream.conn.peer_certificates(), expected_peer_certificate)?;
        stream.write_all(&encode_bind(expected, BIND_REQUEST))?;
        stream.flush()?;
        let mut acknowledgement = [0_u8; CONTROL_BIND_BYTES];
        stream.read_exact(&mut acknowledgement)?;
        require_tls13_alpn(stream.conn.protocol_version(), stream.conn.alpn_protocol())?;
        verify_bind(&acknowledgement, expected, BIND_ACK)?;
        Ok(Self {
            stream: FramedControl::new(stream),
            binding: expected,
        })
    }

    #[must_use]
    pub const fn binding(&self) -> ControlTlsBinding {
        self.binding
    }

    pub fn send_envelope(&mut self, bytes: &[u8]) -> Result<(), ControlTlsError> {
        let result = self.stream.send(bytes);
        if self.stream.failed {
            let _ = self.stream.io.sock.shutdown(Shutdown::Both);
        }
        result
    }

    /// Absolute whole-operation bound, including trickled TLS/framing bytes.
    /// Deadline or any I/O error closes this channel; partial frames are never retried.
    pub fn receive_envelope_until(
        &mut self,
        deadline: Instant,
    ) -> Result<Vec<u8>, ControlTlsError> {
        let socket = self.stream.io.sock.try_clone()?;
        let result = with_socket_deadline(socket, deadline, || self.receive_envelope());
        if result.is_err() {
            self.stream.failed = true;
            let _ = self.stream.io.sock.shutdown(Shutdown::Both);
        }
        result
    }

    /// Deadline-bounded counterpart for an entire framed write.
    pub fn send_envelope_until(
        &mut self,
        bytes: &[u8],
        deadline: Instant,
    ) -> Result<(), ControlTlsError> {
        let socket = self.stream.io.sock.try_clone()?;
        let result = with_socket_deadline(socket, deadline, || self.send_envelope(bytes));
        if result.is_err() {
            self.stream.failed = true;
            let _ = self.stream.io.sock.shutdown(Shutdown::Both);
        }
        result
    }

    pub fn receive_envelope(&mut self) -> Result<Vec<u8>, ControlTlsError> {
        let result = self.stream.receive();
        if self.stream.failed {
            let _ = self.stream.io.sock.shutdown(Shutdown::Both);
        }
        result
    }
}

impl AuthenticatedControlServerChannel {
    pub fn peer_addr(&self) -> Result<SocketAddr, ControlTlsError> {
        self.stream.io.sock.peer_addr().map_err(ControlTlsError::Io)
    }

    #[must_use]
    pub const fn binding(&self) -> ControlTlsBinding {
        self.binding
    }

    pub fn send_envelope(&mut self, bytes: &[u8]) -> Result<(), ControlTlsError> {
        let result = self.stream.send(bytes);
        if self.stream.failed {
            let _ = self.stream.io.sock.shutdown(Shutdown::Both);
        }
        result
    }

    /// Absolute whole-operation bound, including trickled TLS/framing bytes.
    /// Deadline or any I/O error closes this channel; partial frames are never retried.
    pub fn receive_envelope_until(
        &mut self,
        deadline: Instant,
    ) -> Result<Vec<u8>, ControlTlsError> {
        let socket = self.stream.io.sock.try_clone()?;
        let result = with_socket_deadline(socket, deadline, || self.receive_envelope());
        if result.is_err() {
            self.stream.failed = true;
            let _ = self.stream.io.sock.shutdown(Shutdown::Both);
        }
        result
    }

    /// Deadline-bounded counterpart for an entire framed write.
    pub fn send_envelope_until(
        &mut self,
        bytes: &[u8],
        deadline: Instant,
    ) -> Result<(), ControlTlsError> {
        let socket = self.stream.io.sock.try_clone()?;
        let result = with_socket_deadline(socket, deadline, || self.send_envelope(bytes));
        if result.is_err() {
            self.stream.failed = true;
            let _ = self.stream.io.sock.shutdown(Shutdown::Both);
        }
        result
    }

    pub fn receive_envelope(&mut self) -> Result<Vec<u8>, ControlTlsError> {
        let result = self.stream.receive();
        if self.stream.failed {
            let _ = self.stream.io.sock.shutdown(Shutdown::Both);
        }
        result
    }
}

/// A bounded watcher shuts down only a clone of the same owned connection.
/// Completion cancels and joins it, so successful operations leave no sleeper.
/// This is additive: legacy per-syscall-timeout methods are unchanged.
fn with_socket_deadline<T>(
    socket: TcpStream,
    deadline: Instant,
    operation: impl FnOnce() -> Result<T, ControlTlsError>,
) -> Result<T, ControlTlsError> {
    use std::sync::mpsc::{self, RecvTimeoutError};
    if deadline <= Instant::now() {
        let _ = socket.shutdown(Shutdown::Both);
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "control operation deadline expired",
        )
        .into());
    }
    let (cancel, cancelled) = mpsc::channel();
    let watcher = std::thread::Builder::new()
        .name("capyio-control-deadline".into())
        .spawn(move || {
            let expired = matches!(
                cancelled.recv_timeout(deadline.saturating_duration_since(Instant::now())),
                Err(RecvTimeoutError::Timeout)
            );
            if expired {
                let _ = socket.shutdown(Shutdown::Both);
            }
            expired
        })?;
    let result = operation();
    let _ = cancel.send(());
    let shutdown_fired = watcher
        .join()
        .map_err(|_| std::io::Error::other("control deadline watcher failed"))?;
    // The operation can finish before the deadline but lose the cancellation
    // race while this thread is descheduled. Never report success on the socket
    // its watcher already shut down; sample time only after the joined outcome.
    if shutdown_fired || Instant::now() >= deadline {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "control operation deadline expired",
        )
        .into());
    }
    result
}

/// Any failure after touching the wire is terminal: partial I/O cannot be
/// retried without losing frame boundaries or duplicating a control command.
struct FramedControl<S> {
    io: S,
    failed: bool,
}

impl<S: Read + Write> FramedControl<S> {
    fn new(io: S) -> Self {
        Self { io, failed: false }
    }

    fn send(&mut self, bytes: &[u8]) -> Result<(), ControlTlsError> {
        if self.failed {
            return Err(ControlTlsError::ChannelFailed);
        }
        // A local validation error writes nothing and does not poison a peer.
        validate_envelope_bytes(bytes)?;
        let result = send_envelope(&mut self.io, bytes);
        self.failed = result.is_err();
        result
    }

    fn receive(&mut self) -> Result<Vec<u8>, ControlTlsError> {
        if self.failed {
            return Err(ControlTlsError::ChannelFailed);
        }
        let result = receive_envelope(&mut self.io);
        self.failed = result.is_err();
        result
    }
}

fn send_envelope(stream: &mut impl Write, bytes: &[u8]) -> Result<(), ControlTlsError> {
    validate_envelope_bytes(bytes)?;
    let length = u32::try_from(bytes.len()).map_err(|_| ControlTlsError::RecordTooLarge)?;
    stream.write_all(&length.to_be_bytes())?;
    stream.write_all(bytes)?;
    stream.flush()?;
    Ok(())
}

fn receive_envelope(stream: &mut impl Read) -> Result<Vec<u8>, ControlTlsError> {
    let mut length = [0_u8; 4];
    stream.read_exact(&mut length)?;
    let length =
        usize::try_from(u32::from_be_bytes(length)).map_err(|_| ControlTlsError::RecordTooLarge)?;
    if length == 0 || length > MAX_CONTROL_ENVELOPE_BYTES {
        return Err(ControlTlsError::RecordTooLarge);
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes)?;
    validate_envelope_bytes(&bytes)?;
    Ok(bytes)
}

fn validate_envelope_bytes(bytes: &[u8]) -> Result<(), ControlTlsError> {
    if bytes.is_empty() || bytes.len() > MAX_CONTROL_ENVELOPE_BYTES {
        return Err(ControlTlsError::RecordTooLarge);
    }
    decode_envelope(bytes).map_err(|error| ControlTlsError::InvalidEnvelope(error.to_string()))?;
    Ok(())
}

fn encode_bind(binding: ControlTlsBinding, kind: u8) -> [u8; CONTROL_BIND_BYTES] {
    let mut bytes = [0_u8; CONTROL_BIND_BYTES];
    bytes[..8].copy_from_slice(&BIND_MAGIC);
    bytes[8] = BIND_VERSION;
    bytes[9] = kind;
    bytes[12..16].copy_from_slice(&BIND_BODY_BYTES.to_be_bytes());
    bytes[16..32].copy_from_slice(binding.session_id.as_uuid().as_bytes());
    bytes[32..48].copy_from_slice(binding.server_node_id.as_uuid().as_bytes());
    bytes[48..64].copy_from_slice(binding.client_node_id.as_uuid().as_bytes());
    bytes
}

fn verify_bind(
    bytes: &[u8; CONTROL_BIND_BYTES],
    expected: ControlTlsBinding,
    kind: u8,
) -> Result<(), ControlTlsError> {
    if bytes == &encode_bind(expected, kind) {
        Ok(())
    } else {
        Err(ControlTlsError::UnauthorizedBinding)
    }
}

fn decode_bind(
    bytes: &[u8; CONTROL_BIND_BYTES],
    expected_kind: u8,
) -> Result<ControlTlsBinding, ControlTlsError> {
    if bytes[..8] != BIND_MAGIC
        || bytes[8] != BIND_VERSION
        || bytes[9] != expected_kind
        || bytes[10..12] != [0; 2]
        || bytes[12..16] != BIND_BODY_BYTES.to_be_bytes()
    {
        return Err(ControlTlsError::UnauthorizedBinding);
    }
    let session_id = SessionId::from_uuid(
        uuid::Uuid::from_slice(&bytes[16..32]).map_err(|_| ControlTlsError::UnauthorizedBinding)?,
    );
    let server_node_id = NodeId::from_uuid(
        uuid::Uuid::from_slice(&bytes[32..48]).map_err(|_| ControlTlsError::UnauthorizedBinding)?,
    );
    let client_node_id = NodeId::from_uuid(
        uuid::Uuid::from_slice(&bytes[48..64]).map_err(|_| ControlTlsError::UnauthorizedBinding)?,
    );
    ControlTlsBinding::new(session_id, server_node_id, client_node_id)
        .map_err(|_| ControlTlsError::UnauthorizedBinding)
}

fn validate_timeout(timeout: Duration) -> Result<(), ControlTlsError> {
    if (Duration::from_millis(100)..=Duration::from_secs(30)).contains(&timeout) {
        Ok(())
    } else {
        Err(ControlTlsError::InvalidTimeout)
    }
}

fn verify_peer_certificate(
    certificates: Option<&[CertificateDer<'_>]>,
    expected: CertificateFingerprint,
) -> Result<(), ControlTlsError> {
    let actual = certificates
        .and_then(|chain| chain.first())
        .and_then(|leaf| CertificateFingerprint::from_certificate_der(leaf.as_ref()));
    if actual == Some(expected) {
        Ok(())
    } else {
        Err(ControlTlsError::PeerCertificateMismatch)
    }
}

fn require_tls13_alpn(
    protocol_version: Option<rustls::ProtocolVersion>,
    alpn_protocol: Option<&[u8]>,
) -> Result<(), ControlTlsError> {
    if protocol_version == Some(rustls::ProtocolVersion::TLSv1_3)
        && alpn_protocol == Some(CONTROL_TLS_ALPN)
    {
        Ok(())
    } else {
        Err(ControlTlsError::WrongApplicationProtocol)
    }
}

fn build_server_config(
    identity: ControlTlsServerIdentity,
) -> Result<Arc<ServerConfig>, ControlTlsError> {
    if identity.certificate_chain_der.is_empty()
        || identity.private_key_der.is_empty()
        || identity.client_ca_der.is_empty()
    {
        return Err(ControlTlsError::InvalidCredential);
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(identity.client_ca_der))
        .map_err(|_| ControlTlsError::InvalidCredential)?;
    let verifier = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider.clone())
        .build()
        .map_err(|_| ControlTlsError::InvalidCredential)?;
    let certificates = identity
        .certificate_chain_der
        .into_iter()
        .map(CertificateDer::from)
        .collect();
    let key = PrivateKeyDer::try_from(identity.private_key_der)
        .map_err(|_| ControlTlsError::InvalidCredential)?;
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&TLS13])
        .map_err(|_| ControlTlsError::InvalidCredential)?
        .with_client_cert_verifier(verifier)
        .with_single_cert(certificates, key)
        .map_err(|_| ControlTlsError::InvalidCredential)?;
    config.alpn_protocols = vec![CONTROL_TLS_ALPN.to_vec()];
    config.max_early_data_size = 0;
    config.send_tls13_tickets = 0;
    Ok(Arc::new(config))
}

fn build_client_config(
    identity: ControlTlsClientIdentity,
) -> Result<Arc<ClientConfig>, ControlTlsError> {
    if identity.certificate_chain_der.is_empty()
        || identity.private_key_der.is_empty()
        || identity.server_ca_der.is_empty()
    {
        return Err(ControlTlsError::InvalidCredential);
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(identity.server_ca_der))
        .map_err(|_| ControlTlsError::InvalidCredential)?;
    let certificates = identity
        .certificate_chain_der
        .into_iter()
        .map(CertificateDer::from)
        .collect();
    let key = PrivateKeyDer::try_from(identity.private_key_der)
        .map_err(|_| ControlTlsError::InvalidCredential)?;
    let mut config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&TLS13])
        .map_err(|_| ControlTlsError::InvalidCredential)?
        .with_root_certificates(roots)
        .with_client_auth_cert(certificates, key)
        .map_err(|_| ControlTlsError::InvalidCredential)?;
    config.alpn_protocols = vec![CONTROL_TLS_ALPN.to_vec()];
    config.enable_early_data = false;
    Ok(Arc::new(config))
}

#[derive(Debug, Error)]
pub enum ControlTlsError {
    #[error("control TLS peer certificate does not match the paired identity")]
    PeerCertificateMismatch,
    #[error("control TLS channel failed; a fresh authenticated connection is required")]
    ChannelFailed,
    #[error("control TLS endpoint is invalid")]
    InvalidEndpoint,
    #[error("control TLS timeout must be inside 100ms..=30s")]
    InvalidTimeout,
    #[error("control TLS binding is invalid")]
    InvalidBinding,
    #[error("control TLS credential is invalid")]
    InvalidCredential,
    #[error("control TLS I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("control TLS handshake failed: {0}")]
    Tls(#[from] rustls::Error),
    #[error("control TLS peer did not negotiate TLS 1.3 and capyio-control/1")]
    WrongApplicationProtocol,
    #[error("control TLS Session/Node binding is unauthorized")]
    UnauthorizedBinding,
    #[error("control TLS record is empty or exceeds the control-envelope bound")]
    RecordTooLarge,
    #[error("control TLS record is not a valid Envelope: {0}")]
    InvalidEnvelope(String),
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, net::Ipv4Addr, thread};

    use capyio_control_host::{AdmittedPeerBinding, ControlMessageOutcome, PeerCatalogControlHost};
    use capyio_protocol::{
        MIN_PORT_LEASE_TTL_MS, PortLeaseAction, PortLeaseOutcome, decode_port_lease_response,
        encode_envelope, encode_port_lease_request, new_envelope, v1,
    };
    use capyio_runtime::PortLeasePolicy;
    use capyio_testkit::DemoLab;
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
        KeyUsagePurpose,
    };

    use super::*;

    #[test]
    fn typed_bundle_maps_to_both_control_identity_roles() {
        let fingerprint = CertificateFingerprint::new([7; 32]).expect("fingerprint");
        let server = ControlTlsServerIdentity::from_bundle(
            fingerprint,
            ControlCredentialBundle::new(vec![vec![1, 2]], vec![3, 4], vec![5, 6]).expect("bundle"),
        );
        assert_eq!(server.certificate_chain_der, vec![vec![1, 2]]);
        assert_eq!(server.private_key_der, vec![3, 4]);
        assert_eq!(server.client_ca_der, vec![5, 6]);

        let client = ControlTlsClientIdentity::from_bundle(
            fingerprint,
            ControlCredentialBundle::new(vec![vec![9]], vec![8], vec![7]).expect("bundle"),
        );
        assert_eq!(client.certificate_chain_der, vec![vec![9]]);
        assert_eq!(client.private_key_der, vec![8]);
        assert_eq!(client.server_ca_der, vec![7]);
    }

    #[test]
    fn bind_record_is_exact_and_identity_changes_fail_closed() {
        let binding = binding();
        let record = encode_bind(binding, BIND_REQUEST);
        assert_eq!(record.len(), CONTROL_BIND_BYTES);
        assert_eq!(&record[..10], b"CPYCTRL1\x01\x01");
        verify_bind(&record, binding, BIND_REQUEST).expect("request binding");
        assert!(matches!(
            verify_bind(&record, binding, BIND_ACK),
            Err(ControlTlsError::UnauthorizedBinding)
        ));
        let changed =
            ControlTlsBinding::new(binding.session_id, binding.server_node_id, NodeId::new())
                .expect("changed binding");
        assert!(matches!(
            verify_bind(&record, changed, BIND_REQUEST),
            Err(ControlTlsError::UnauthorizedBinding)
        ));
        assert_eq!(
            decode_bind(&record, BIND_REQUEST).expect("decoded binding"),
            binding
        );
        let mut reserved = record;
        reserved[10] = 1;
        assert!(matches!(
            decode_bind(&reserved, BIND_REQUEST),
            Err(ControlTlsError::UnauthorizedBinding)
        ));
    }

    #[test]
    fn mutually_authenticated_nodes_establish_one_fresh_shared_session() {
        let (server_identity, client_identity) = test_identities();
        let original = binding();
        let nodes = ControlTlsNodeBinding::new(original.server_node_id, original.client_node_id)
            .expect("node binding");
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("listener");
        let address = listener.local_addr().expect("address");
        let server = AuthenticatedControlServer::from_listener_fresh_session(
            listener,
            server_identity,
            nodes,
            Duration::from_secs(5),
        )
        .expect("server");
        let server_thread = thread::spawn(move || server.accept().expect("fresh client").binding());

        let client = AuthenticatedControlClientChannel::connect_fresh_session(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(5),
            },
            "localhost",
            client_identity,
            nodes,
        )
        .expect("fresh session");
        let server_binding = server_thread.join().expect("server thread");
        assert_eq!(client.binding(), server_binding);
        assert_eq!(client.binding().server_node_id, original.server_node_id);
        assert_eq!(client.binding().client_node_id, original.client_node_id);
        assert!(!client.binding().session_id.as_uuid().is_nil());
        assert_ne!(client.binding().session_id, original.session_id);
    }

    #[test]
    fn fresh_session_server_rejects_changed_node_identity_after_mtls() {
        let (server_identity, client_identity) = test_identities();
        let original = binding();
        let nodes = ControlTlsNodeBinding::new(original.server_node_id, original.client_node_id)
            .expect("node binding");
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("listener");
        let address = listener.local_addr().expect("address");
        let server = AuthenticatedControlServer::from_listener_fresh_session(
            listener,
            server_identity,
            nodes,
            Duration::from_secs(5),
        )
        .expect("server");
        let server_thread = thread::spawn(move || server.accept());
        let changed =
            ControlTlsBinding::new(SessionId::new(), original.server_node_id, NodeId::new())
                .expect("changed binding");

        assert!(
            AuthenticatedControlClientChannel::connect(
                ControlTlsEndpoint {
                    address,
                    io_timeout: Duration::from_secs(5),
                },
                "localhost",
                client_identity,
                changed,
            )
            .is_err()
        );
        assert!(matches!(
            server_thread.join().expect("server thread"),
            Err(ControlTlsError::UnauthorizedBinding)
        ));
    }

    #[test]
    fn framing_rejects_empty_oversized_and_invalid_envelopes() {
        assert!(matches!(
            send_envelope(&mut Vec::new(), &[]),
            Err(ControlTlsError::RecordTooLarge)
        ));
        assert!(matches!(
            send_envelope(&mut Vec::new(), b"not protobuf"),
            Err(ControlTlsError::InvalidEnvelope(_))
        ));
        let mut empty_frame = Cursor::new(0_u32.to_be_bytes());
        assert!(matches!(
            receive_envelope(&mut empty_frame),
            Err(ControlTlsError::RecordTooLarge)
        ));
        let too_large = u32::try_from(MAX_CONTROL_ENVELOPE_BYTES + 1)
            .expect("control bound fits u32")
            .to_be_bytes();
        assert!(matches!(
            receive_envelope(&mut Cursor::new(too_large)),
            Err(ControlTlsError::RecordTooLarge)
        ));
    }

    #[test]
    fn mutually_authenticated_channel_binds_nodes_and_exchanges_envelopes() {
        let (server_identity, client_identity) = test_identities();
        let binding = binding();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("listener");
        let address = listener.local_addr().expect("address");
        let request = envelope(binding.session_id, "client-request");
        let response = envelope(binding.session_id, "server-response");
        let expected_request = request.clone();
        let sent_response = response.clone();
        let server = AuthenticatedControlServer::from_listener(
            listener,
            server_identity,
            binding,
            Duration::from_secs(5),
        )
        .expect("server");
        let server_thread = thread::spawn(move || {
            let mut channel = server.accept().expect("authenticated client");
            assert_eq!(channel.binding(), binding);
            assert_eq!(
                channel.receive_envelope().expect("request"),
                expected_request
            );
            channel
                .send_envelope(&sent_response)
                .expect("send response");
        });

        let mut client = AuthenticatedControlClientChannel::connect(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(5),
            },
            "localhost",
            client_identity,
            binding,
        )
        .expect("authenticated server");
        assert_eq!(client.binding(), binding);
        client.send_envelope(&request).expect("send request");
        assert_eq!(client.receive_envelope().expect("response"), response);
        server_thread.join().expect("server thread");
    }

    #[test]
    fn truncated_and_malformed_frames_permanently_fail_the_channel() {
        for bytes in [
            vec![0, 0],
            vec![0, 0, 0, 5, 1, 2],
            vec![0, 0, 0, 0],
            vec![0, 0, 0, 1, 0xff],
        ] {
            let mut channel = FramedControl::new(Cursor::new(bytes));
            assert!(channel.receive().is_err());
            let position = channel.io.position();
            assert!(matches!(
                channel.receive(),
                Err(ControlTlsError::ChannelFailed)
            ));
            assert!(matches!(
                channel.send(&envelope(SessionId::new(), "retry")),
                Err(ControlTlsError::ChannelFailed)
            ));
            assert_eq!(
                channel.io.position(),
                position,
                "failed channel must not touch I/O"
            );
        }
    }

    #[test]
    fn rejected_local_envelope_does_not_poison_an_untouched_channel() {
        let mut channel = FramedControl::new(Cursor::new(Vec::new()));
        assert!(matches!(
            channel.send(b"bad protobuf"),
            Err(ControlTlsError::InvalidEnvelope(_))
        ));
        assert!(!channel.failed);
        assert_eq!(channel.io.position(), 0);
        let bytes = envelope(SessionId::new(), "valid");
        channel.send(&bytes).expect("corrected local input");
        channel.io.set_position(0);
        assert_eq!(channel.receive().unwrap(), bytes);
    }

    #[test]
    fn partial_write_timeout_is_terminal_and_never_resends() {
        struct PartialWriter {
            calls: usize,
            written: usize,
        }
        impl Read for PartialWriter {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                panic!("must not read failed channel")
            }
        }
        impl Write for PartialWriter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.calls += 1;
                if self.calls == 1 {
                    self.written += bytes.len();
                    Ok(bytes.len())
                } else {
                    Err(std::io::ErrorKind::TimedOut.into())
                }
            }
            fn flush(&mut self) -> std::io::Result<()> {
                panic!("write failed before flush")
            }
        }
        let mut channel = FramedControl::new(PartialWriter {
            calls: 0,
            written: 0,
        });
        let bytes = envelope(SessionId::new(), "request");
        assert!(matches!(channel.send(&bytes), Err(ControlTlsError::Io(_))));
        assert_eq!(channel.io.written, 4, "only length prefix was written");
        let calls = channel.io.calls;
        assert!(matches!(
            channel.send(&bytes),
            Err(ControlTlsError::ChannelFailed)
        ));
        assert!(matches!(
            channel.receive(),
            Err(ControlTlsError::ChannelFailed)
        ));
        assert_eq!(channel.io.calls, calls);
    }

    #[test]
    fn tls_framing_failure_requires_a_new_authenticated_connection() {
        let (server_identity, client_identity) = test_identities();
        let binding = binding();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = AuthenticatedControlServer::from_listener(
            listener,
            server_identity,
            binding,
            Duration::from_secs(5),
        )
        .unwrap();
        let server_thread = thread::spawn(move || {
            let mut first = server.accept().unwrap();
            // An authenticated but malformed peer must not desynchronize the reader.
            first.stream.io.write_all(&0_u32.to_be_bytes()).unwrap();
            first.stream.io.flush().unwrap();
            drop(first);
            let mut second = server.accept().unwrap();
            let request = second.receive_envelope().unwrap();
            second.send_envelope(&request).unwrap();
        });
        let endpoint = ControlTlsEndpoint {
            address,
            io_timeout: Duration::from_secs(5),
        };
        let mut first = AuthenticatedControlClientChannel::connect(
            endpoint,
            "localhost",
            client_identity.clone(),
            binding,
        )
        .unwrap();
        assert!(matches!(
            first.receive_envelope(),
            Err(ControlTlsError::RecordTooLarge)
        ));
        assert!(matches!(
            first.receive_envelope(),
            Err(ControlTlsError::ChannelFailed)
        ));
        let request = envelope(binding.session_id, "fresh-connection");
        assert!(matches!(
            first.send_envelope(&request),
            Err(ControlTlsError::ChannelFailed)
        ));
        let mut second = AuthenticatedControlClientChannel::connect(
            endpoint,
            "localhost",
            client_identity,
            binding,
        )
        .unwrap();
        second.send_envelope(&request).unwrap();
        assert_eq!(second.receive_envelope().unwrap(), request);
        server_thread.join().unwrap();
    }

    #[test]
    fn authenticated_channel_carries_a_session_bound_owner_lease_decision() {
        let (server_identity, client_identity) = test_identities();
        let mut lab = DemoLab::new().expect("demo lab");
        let route_id = lab.routes.phone_microphone_to_windows;
        let route = lab.runtime.route(route_id).expect("Route").clone();
        let local_port = route.sink;
        let peer_node_id = route.source.node_id;
        lab.runtime
            .set_local_port_lease_policy(local_port, PortLeasePolicy::Exclusive)
            .expect("policy");
        lab.runtime
            .authorize_route(route_id, None)
            .expect("authorization");
        let binding = ControlTlsBinding::new(lab.session_id, local_port.node_id, peer_node_id)
            .expect("binding");
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("listener");
        let address = listener.local_addr().expect("address");
        let server = AuthenticatedControlServer::from_listener(
            listener,
            server_identity,
            binding,
            Duration::from_secs(5),
        )
        .expect("server");
        let server_thread = thread::spawn(move || {
            let mut channel = server.accept().expect("authenticated peer");
            let request = channel.receive_envelope().expect("lease request");
            let mut host =
                PeerCatalogControlHost::new(AdmittedPeerBinding::new(lab.session_id, peer_node_id));
            let outcome = host
                .handle_control_message(&mut lab.runtime, &request, 10)
                .expect("lease decision");
            let ControlMessageOutcome::Response { bytes, .. } = outcome else {
                panic!("expected lease response");
            };
            channel.send_envelope(&bytes).expect("lease response");
            lab.runtime.snapshot().port_leases
        });

        let request = encode_port_lease_request(
            binding.session_id,
            route_id,
            local_port,
            PortLeaseAction::Acquire,
            MIN_PORT_LEASE_TTL_MS,
        )
        .expect("lease request");
        let mut client = AuthenticatedControlClientChannel::connect(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(5),
            },
            "localhost",
            client_identity,
            binding,
        )
        .expect("control client");
        client.send_envelope(&request).expect("send lease request");
        let response =
            decode_port_lease_response(&client.receive_envelope().expect("receive lease response"))
                .expect("decode response");
        assert_eq!(response.outcome, PortLeaseOutcome::Acquired);
        let claims = server_thread.join().expect("server thread");
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].route_id, route_id);
        assert_eq!(claims[0].port, local_port);
    }

    #[test]
    fn endpoint_binding_and_credentials_have_closed_validation() {
        assert!(ControlTlsBinding::new(SessionId::new(), NodeId::new(), NodeId::new()).is_ok());
        let node = NodeId::new();
        assert!(matches!(
            ControlTlsBinding::new(SessionId::new(), node, node),
            Err(ControlTlsError::InvalidBinding)
        ));
        assert!(matches!(
            ControlTlsEndpoint {
                address: SocketAddr::from((Ipv4Addr::UNSPECIFIED, 42420)),
                io_timeout: Duration::from_secs(5),
            }
            .validate(),
            Err(ControlTlsError::InvalidEndpoint)
        ));
        assert!(matches!(
            build_server_config(ControlTlsServerIdentity {
                expected_peer_certificate: CertificateFingerprint::new([1; 32]).unwrap(),
                certificate_chain_der: Vec::new(),
                private_key_der: Vec::new(),
                client_ca_der: Vec::new(),
            }),
            Err(ControlTlsError::InvalidCredential)
        ));
    }

    #[test]
    fn pin_requires_the_exact_leaf_not_an_issuer_or_missing_certificate() {
        let leaf = CertificateDer::from(b"leaf fixture".to_vec());
        let issuer = CertificateDer::from(b"issuer fixture".to_vec());
        let expected = CertificateFingerprint::from_certificate_der(leaf.as_ref()).unwrap();
        assert!(matches!(
            verify_peer_certificate(None, expected),
            Err(ControlTlsError::PeerCertificateMismatch)
        ));
        assert!(matches!(
            verify_peer_certificate(Some(&[]), expected),
            Err(ControlTlsError::PeerCertificateMismatch)
        ));
        assert!(verify_peer_certificate(Some(&[leaf.clone(), issuer.clone()]), expected).is_ok());
        assert!(matches!(
            verify_peer_certificate(Some(&[issuer, leaf]), expected),
            Err(ControlTlsError::PeerCertificateMismatch)
        ));
    }

    #[test]
    fn valid_mtls_chain_cannot_replace_either_paired_leaf_identity() {
        for wrong_server_pin in [true, false] {
            let (mut server_identity, mut client_identity) = test_identities();
            // Both certificates chain to the same CA. Pairing still names an
            // exact leaf, not every credential issued by that authority.
            if wrong_server_pin {
                client_identity.expected_peer_certificate =
                    CertificateFingerprint::from_certificate_der(
                        &client_identity.certificate_chain_der[0],
                    )
                    .unwrap();
            } else {
                server_identity.expected_peer_certificate =
                    CertificateFingerprint::from_certificate_der(
                        &server_identity.certificate_chain_der[0],
                    )
                    .unwrap();
            }
            let binding = binding();
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            let address = listener.local_addr().unwrap();
            let server = AuthenticatedControlServer::from_listener(
                listener,
                server_identity,
                binding,
                Duration::from_secs(2),
            )
            .unwrap();
            let worker = thread::spawn(move || server.accept());
            let client_result = AuthenticatedControlClientChannel::connect(
                ControlTlsEndpoint {
                    address,
                    io_timeout: Duration::from_secs(2),
                },
                "localhost",
                client_identity,
                binding,
            );
            let server_result = worker.join().unwrap();
            assert!(client_result.is_err());
            assert!(server_result.is_err());
            if wrong_server_pin {
                assert!(matches!(
                    client_result,
                    Err(ControlTlsError::PeerCertificateMismatch)
                ));
            } else {
                assert!(matches!(
                    server_result,
                    Err(ControlTlsError::PeerCertificateMismatch)
                ));
            }
        }
    }

    fn binding() -> ControlTlsBinding {
        ControlTlsBinding::new(SessionId::new(), NodeId::new(), NodeId::new()).expect("binding")
    }

    fn envelope(session_id: SessionId, code: &str) -> Vec<u8> {
        encode_envelope(&new_envelope(
            Some(session_id),
            v1::envelope::Payload::Error(v1::ErrorMessage {
                code: code.to_owned(),
                category: "test".to_owned(),
                retryable: false,
                detail: String::new(),
                related_id: String::new(),
            }),
        ))
    }

    pub(super) fn test_identities() -> (ControlTlsServerIdentity, ControlTlsClientIdentity) {
        let mut ca_params = CertificateParams::new(Vec::<String>::new()).expect("CA params");
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
        let ca_key = KeyPair::generate().expect("CA key");
        let ca_cert = ca_params.self_signed(&ca_key).expect("CA cert");
        let ca_der = ca_cert.der().to_vec();
        let issuer = Issuer::new(ca_params, ca_key);
        let (server_cert, server_key) =
            leaf(&issuer, "localhost", ExtendedKeyUsagePurpose::ServerAuth);
        let (client_cert, client_key) = leaf(
            &issuer,
            "capyio-client-test",
            ExtendedKeyUsagePurpose::ClientAuth,
        );
        let server_fingerprint =
            CertificateFingerprint::from_certificate_der(&server_cert).unwrap();
        let client_fingerprint =
            CertificateFingerprint::from_certificate_der(&client_cert).unwrap();
        (
            ControlTlsServerIdentity {
                expected_peer_certificate: client_fingerprint,
                certificate_chain_der: vec![server_cert, ca_der.clone()],
                private_key_der: server_key,
                client_ca_der: ca_der.clone(),
            },
            ControlTlsClientIdentity {
                expected_peer_certificate: server_fingerprint,
                certificate_chain_der: vec![client_cert, ca_der.clone()],
                private_key_der: client_key,
                server_ca_der: ca_der,
            },
        )
    }

    fn leaf(
        issuer: &Issuer<'static, KeyPair>,
        name: &str,
        usage: ExtendedKeyUsagePurpose,
    ) -> (Vec<u8>, Vec<u8>) {
        let mut params = CertificateParams::new(vec![name.to_owned()]).expect("leaf params");
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![usage];
        let key = KeyPair::generate().expect("leaf key");
        let cert = params.signed_by(&key, issuer).expect("leaf cert");
        (cert.der().to_vec(), key.serialize_der())
    }
}

#[cfg(test)]
mod deadline_tests {
    use super::*;

    #[test]
    fn ipc_trickled_frame_cannot_renew_whole_operation_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut local, _) = listener.accept().unwrap();
        local
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let sender = std::thread::spawn(move || {
            for _ in 0..200 {
                if peer.write_all(&[1]).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let start = Instant::now();
        let result = with_socket_deadline(
            local.try_clone().unwrap(),
            start + Duration::from_millis(80),
            || local.read_exact(&mut [0; 200]).map_err(ControlTlsError::Io),
        );
        let elapsed = start.elapsed();
        let _ = local.shutdown(Shutdown::Both);
        sender.join().unwrap();
        assert!(result.is_err());
        assert!(
            elapsed < Duration::from_millis(750),
            "whole operation exceeded budget tolerance: {elapsed:?}"
        );
    }

    #[test]
    fn ipc_success_cancels_deadline_watcher_without_closing_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut local, _) = listener.accept().unwrap();
        local
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        peer.write_all(&[1]).unwrap();
        with_socket_deadline(
            local.try_clone().unwrap(),
            Instant::now() + Duration::from_millis(50),
            || local.read_exact(&mut [0; 1]).map_err(ControlTlsError::Io),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(80));
        peer.write_all(&[2]).unwrap();
        let mut byte = [0];
        local.read_exact(&mut byte).unwrap();
        assert_eq!(byte, [2]);
    }
}
