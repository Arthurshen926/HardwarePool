#![forbid(unsafe_code)]

//! Mutually authenticated TLS 1.3 framing for CapyIO control Envelopes.
//! Credential enrollment and persistence are deliberately caller-owned.

pub mod audio;
pub mod audio_stereo;
pub mod camera;
pub mod display;
pub mod haptics;
mod route_tls;
pub mod touchpad;

use std::{
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{Arc, Mutex, TryLockError},
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
    accept_lock: Mutex<()>,
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
            accept_lock: Mutex::new(()),
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
            accept_lock: Mutex::new(()),
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
        let socket = {
            let _guard = self
                .accept_lock
                .lock()
                .map_err(|_| std::io::Error::other("control accept lock poisoned"))?;
            self.listener.accept()?.0
        };
        self.authenticate_socket(socket)
    }

    /// Poll once without waiting for a connection. Once accepted, the entire
    /// TLS handshake and authenticated binding exchange share `deadline`.
    /// Callers must clamp it to their own lifetime/cancellation polling budget.
    /// An expired deadline is an error even when the listener is idle. Any
    /// accepted connection failure is terminal; it is shut down, not retried.
    pub fn try_accept_until(
        &self,
        deadline: Instant,
    ) -> Result<Option<AuthenticatedControlServerChannel>, ControlTlsError> {
        check_poll_deadline(deadline)?;
        let socket = {
            let _guard = match self.accept_lock.try_lock() {
                Ok(guard) => guard,
                Err(TryLockError::WouldBlock) => {
                    check_poll_deadline(deadline)?;
                    return Ok(None);
                }
                Err(TryLockError::Poisoned(_)) => {
                    return Err(std::io::Error::other("control accept lock poisoned").into());
                }
            };
            self.listener.set_nonblocking(true)?;
            let accepted = self.listener.accept();
            // Restore before inspecting the result, including WouldBlock/error.
            if let Err(error) = self.listener.set_nonblocking(false) {
                if let Ok((socket, _)) = accepted {
                    let _ = socket.shutdown(Shutdown::Both);
                }
                return Err(error.into());
            }
            match accepted {
                Ok((socket, _)) => socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    check_poll_deadline(deadline)?;
                    return Ok(None);
                }
                Err(error) => return Err(error.into()),
            }
        };
        // Keep the original socket for terminal cleanup even if cloning,
        // watcher startup, authentication or its final deadline check fails.
        let result = (|| {
            let watchdog_socket = socket.try_clone()?;
            with_socket_deadline(watchdog_socket, deadline, || {
                self.authenticate_socket(socket.try_clone()?)
            })
        })();
        if result.is_err() {
            let _ = socket.shutdown(Shutdown::Both);
        }
        result.map(Some)
    }

    fn authenticate_socket(
        &self,
        socket: TcpStream,
    ) -> Result<AuthenticatedControlServerChannel, ControlTlsError> {
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
    /// Socket address of the already authenticated paired peer.
    pub fn peer_addr(&self) -> Result<SocketAddr, ControlTlsError> {
        self.stream.io.sock.peer_addr().map_err(ControlTlsError::Io)
    }

    /// Local endpoint selected by the already-authenticated TCP connection.
    /// This is socket metadata, not a caller-supplied media address.
    pub fn local_addr(&self) -> Result<SocketAddr, ControlTlsError> {
        self.stream
            .io
            .sock
            .local_addr()
            .map_err(ControlTlsError::Io)
    }

    pub fn connect_fresh_session(
        endpoint: ControlTlsEndpoint,
        server_name: &str,
        identity: ControlTlsClientIdentity,
        expected_nodes: ControlTlsNodeBinding,
    ) -> Result<Self, ControlTlsError> {
        let binding = expected_nodes.with_session(SessionId::new())?;
        Self::connect(endpoint, server_name, identity, binding)
    }

    /// Propose a fresh Session within one absolute TCP/TLS/binding deadline.
    /// Expiry or any connection failure is terminal; no retry is performed.
    pub fn connect_fresh_session_until(
        endpoint: ControlTlsEndpoint,
        server_name: &str,
        identity: ControlTlsClientIdentity,
        expected_nodes: ControlTlsNodeBinding,
        deadline: Instant,
    ) -> Result<Self, ControlTlsError> {
        check_poll_deadline(deadline)?;
        let binding = expected_nodes.with_session(SessionId::new())?;
        Self::connect_until(endpoint, server_name, identity, binding, deadline)
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
        Self::authenticate_socket(socket, endpoint.io_timeout, server_name, identity, expected)
    }

    /// Bound TCP connection, TLS handshake and the complete authenticated
    /// binding exchange by the same absolute deadline, including trickled
    /// input. The TCP timeout is the lesser of the remaining budget and the
    /// endpoint timeout. Expired budgets fail before opening a socket; any
    /// failure after connecting shuts down that connection without retry.
    /// Success retains the endpoint's ordinary per-I/O timeout.
    pub fn connect_until(
        endpoint: ControlTlsEndpoint,
        server_name: &str,
        identity: ControlTlsClientIdentity,
        expected: ControlTlsBinding,
        deadline: Instant,
    ) -> Result<Self, ControlTlsError> {
        check_poll_deadline(deadline)?;
        endpoint.validate()?;
        if server_name.is_empty() || server_name.len() > 253 {
            return Err(ControlTlsError::InvalidEndpoint);
        }
        let timeout = connect_timeout_until(endpoint.io_timeout, deadline)?;
        let socket = match TcpStream::connect_timeout(&endpoint.address, timeout) {
            Ok(socket) => socket,
            Err(error) => {
                check_poll_deadline(deadline)?;
                return Err(error.into());
            }
        };
        // Retain an owner for terminal cleanup, including cloning/watcher
        // startup errors and expiry racing with successful authentication.
        let result = (|| {
            let watchdog_socket = socket.try_clone()?;
            with_socket_deadline(watchdog_socket, deadline, || {
                Self::authenticate_socket(
                    socket.try_clone()?,
                    endpoint.io_timeout,
                    server_name,
                    identity,
                    expected,
                )
            })
        })();
        if result.is_err() {
            let _ = socket.shutdown(Shutdown::Both);
        }
        result
    }

    fn authenticate_socket(
        socket: TcpStream,
        io_timeout: Duration,
        server_name: &str,
        identity: ControlTlsClientIdentity,
        expected: ControlTlsBinding,
    ) -> Result<Self, ControlTlsError> {
        socket.set_read_timeout(Some(io_timeout))?;
        socket.set_write_timeout(Some(io_timeout))?;
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

    /// Exact leaf authenticated during this live pinned mutual-TLS handshake.
    pub fn peer_certificate_fingerprint(&self) -> Result<CertificateFingerprint, ControlTlsError> {
        self.stream
            .io
            .conn
            .peer_certificates()
            .and_then(|chain| chain.first())
            .and_then(|leaf| CertificateFingerprint::from_certificate_der(leaf.as_ref()))
            .ok_or(ControlTlsError::PeerCertificateMismatch)
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

    /// Poll one authenticated Envelope without blocking. A partial frame keeps
    /// its original absolute deadline from the first plaintext byte, even if
    /// later calls supply a later one. Encrypted-only traffic is work-bounded
    /// per poll, not assigned an application-frame deadline: the caller must
    /// also enforce its Session/grant/lease/lifetime deadline.
    /// `None` means idle or bounded progress; errors permanently close the channel.
    pub fn poll_envelope_until(
        &mut self,
        deadline: Instant,
    ) -> Result<Option<Vec<u8>>, ControlTlsError> {
        self.stream.poll_until(deadline)
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

    /// Local endpoint selected by the already-authenticated TCP connection.
    /// This is socket metadata, not a caller-supplied media address.
    pub fn local_addr(&self) -> Result<SocketAddr, ControlTlsError> {
        self.stream
            .io
            .sock
            .local_addr()
            .map_err(ControlTlsError::Io)
    }

    #[must_use]
    pub const fn binding(&self) -> ControlTlsBinding {
        self.binding
    }

    /// Exact leaf authenticated during this live pinned mutual-TLS handshake.
    pub fn peer_certificate_fingerprint(&self) -> Result<CertificateFingerprint, ControlTlsError> {
        self.stream
            .io
            .conn
            .peer_certificates()
            .and_then(|chain| chain.first())
            .and_then(|leaf| CertificateFingerprint::from_certificate_der(leaf.as_ref()))
            .ok_or(ControlTlsError::PeerCertificateMismatch)
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

    /// Poll one authenticated Envelope without blocking. A partial frame keeps
    /// its original absolute deadline from the first plaintext byte, even if
    /// later calls supply a later one. Encrypted-only traffic is work-bounded
    /// per poll, not assigned an application-frame deadline: the caller must
    /// also enforce its Session/grant/lease/lifetime deadline.
    /// `None` means idle or bounded progress; errors permanently close the channel.
    pub fn poll_envelope_until(
        &mut self,
        deadline: Instant,
    ) -> Result<Option<Vec<u8>>, ControlTlsError> {
        self.stream.poll_until(deadline)
    }

    pub fn receive_envelope(&mut self) -> Result<Vec<u8>, ControlTlsError> {
        let result = self.stream.receive();
        if self.stream.failed {
            let _ = self.stream.io.sock.shutdown(Shutdown::Both);
        }
        result
    }
}

fn connect_timeout_until(
    io_timeout: Duration,
    deadline: Instant,
) -> Result<Duration, ControlTlsError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "control connection deadline expired",
        )
        .into());
    }
    Ok(io_timeout.min(remaining))
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
    incoming: IncrementalEnvelope,
}

impl<S: Read + Write> FramedControl<S> {
    fn new(io: S) -> Self {
        Self {
            io,
            failed: false,
            incoming: IncrementalEnvelope::default(),
        }
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
        if self.incoming.deadline.is_some() {
            self.failed = true;
            return Err(ControlTlsError::PartialReceiveInProgress);
        }
        let result = receive_envelope(&mut self.io);
        self.failed = result.is_err();
        result
    }
}

// Each poll has a fixed syscall and plaintext budget, including TLS control
// records that produce no application bytes. No thread or timer is created.
const POLL_IO_CALLS: usize = 256;
const POLL_WIRE_BYTES: usize = 32 * 1024;
const POLL_PLAINTEXT_READS: usize = 32;

#[derive(Default)]
struct IncrementalEnvelope {
    prefix: [u8; 4],
    prefix_read: usize,
    body: Vec<u8>,
    body_read: usize,
    deadline: Option<Instant>,
}

impl IncrementalEnvelope {
    fn effective_deadline(&self, requested: Instant) -> Instant {
        self.deadline
            .map_or(requested, |saved| saved.min(requested))
    }

    fn poll(
        &mut self,
        reader: &mut impl Read,
        deadline: Instant,
    ) -> Result<Option<Vec<u8>>, ControlTlsError> {
        for _ in 0..POLL_PLAINTEXT_READS {
            check_poll_deadline(deadline)?;
            let destination = if self.prefix_read < self.prefix.len() {
                &mut self.prefix[self.prefix_read..]
            } else {
                &mut self.body[self.body_read..]
            };
            let read = match reader.read(destination) {
                Ok(0) => return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into()),
                Ok(read) => read,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            };
            self.deadline.get_or_insert(deadline);
            check_poll_deadline(deadline)?;
            if self.prefix_read < self.prefix.len() {
                self.prefix_read += read;
                if self.prefix_read == self.prefix.len() {
                    let length = usize::try_from(u32::from_be_bytes(self.prefix))
                        .map_err(|_| ControlTlsError::RecordTooLarge)?;
                    if length == 0 || length > MAX_CONTROL_ENVELOPE_BYTES {
                        return Err(ControlTlsError::RecordTooLarge);
                    }
                    self.body.resize(length, 0);
                }
            } else {
                self.body_read += read;
                if self.body_read == self.body.len() {
                    validate_envelope_bytes(&self.body)?;
                    check_poll_deadline(deadline)?;
                    let result = std::mem::take(&mut self.body);
                    *self = Self::default();
                    return Ok(Some(result));
                }
            }
        }
        Ok(None)
    }
}

// Provenance is attached only by the deadline check, never inferred from an
// operating-system timeout or its human-readable error message.
#[derive(Debug, Error)]
#[error("control poll deadline expired")]
struct ControlPollDeadlineExpired {
    deadline: Instant,
}

/// Return the exact local deadline checked by a control poll timeout.
/// A retained partial-frame deadline may be earlier than the caller's requested
/// deadline. Ordinary I/O timeouts, including identical messages, return None.
/// This is attribution only: it does not authorize retrying a failed channel.
pub fn control_poll_timeout_deadline(error: &std::io::Error) -> Option<Instant> {
    error
        .get_ref()?
        .downcast_ref::<ControlPollDeadlineExpired>()
        .map(|timeout| timeout.deadline)
}

fn check_poll_deadline(deadline: Instant) -> std::io::Result<()> {
    if Instant::now() >= deadline {
        Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            ControlPollDeadlineExpired { deadline },
        ))
    } else {
        Ok(())
    }
}

struct BoundedPollIo<'a, S> {
    io: &'a mut S,
    calls_left: usize,
    bytes_left: usize,
    deadline: Instant,
}

impl<'a, S> BoundedPollIo<'a, S> {
    fn new(io: &'a mut S, deadline: Instant) -> Self {
        Self {
            io,
            calls_left: POLL_IO_CALLS,
            bytes_left: POLL_WIRE_BYTES,
            deadline,
        }
    }

    fn allowance(&mut self, requested: usize) -> std::io::Result<usize> {
        check_poll_deadline(self.deadline)?;
        if self.calls_left == 0 || self.bytes_left == 0 {
            return Err(std::io::ErrorKind::WouldBlock.into());
        }
        self.calls_left -= 1;
        Ok(requested.min(self.bytes_left))
    }
}

impl<S: Read> Read for BoundedPollIo<'_, S> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let allowance = self.allowance(bytes.len())?;
        let read = self.io.read(&mut bytes[..allowance])?;
        self.bytes_left -= read;
        check_poll_deadline(self.deadline)?;
        Ok(read)
    }
}

impl<S: Write> Write for BoundedPollIo<'_, S> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let allowance = self.allowance(bytes.len())?;
        let written = self.io.write(&bytes[..allowance])?;
        self.bytes_left -= written;
        check_poll_deadline(self.deadline)?;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.allowance(0)?;
        self.io.flush()?;
        check_poll_deadline(self.deadline)
    }
}

impl<C, D> FramedControl<StreamOwned<C, TcpStream>>
where
    C: std::ops::DerefMut<Target = rustls::ConnectionCommon<D>>,
    D: rustls::SideData,
{
    fn poll_until(&mut self, requested: Instant) -> Result<Option<Vec<u8>>, ControlTlsError> {
        if self.failed {
            return Err(ControlTlsError::ChannelFailed);
        }
        let deadline = self.incoming.effective_deadline(requested);
        if self.incoming.deadline.is_some() {
            self.incoming.deadline = Some(deadline);
        }
        let result = (|| {
            check_poll_deadline(deadline)?;
            self.io.sock.set_nonblocking(true)?;
            let mut bounded = BoundedPollIo::new(&mut self.io.sock, deadline);
            let mut tls = rustls::Stream::new(&mut self.io.conn, &mut bounded);
            let result = self.incoming.poll(&mut tls, deadline);
            // Restoration failure is terminal, even if a frame was obtained.
            self.io.sock.set_nonblocking(false)?;
            check_poll_deadline(deadline)?;
            let complete = matches!(result, Ok(Some(_)));
            // Coalesced next-frame plaintext was already received under this
            // deadline. Keep it rather than granting a fresh budget next poll.
            if complete
                && self
                    .io
                    .conn
                    .reader()
                    .into_first_chunk()
                    .is_ok_and(|chunk| !chunk.is_empty())
            {
                self.incoming.deadline.get_or_insert(deadline);
            }
            result
        })();
        if result.is_err() {
            self.failed = true;
            let _ = self.io.sock.shutdown(Shutdown::Both);
        }
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

// Best-effort cleanup of caller-owned transient DER before handing ownership
// to rustls. This is not a cryptographic-erasure or rustls-memory guarantee.
struct SecretDer(Vec<u8>);

impl SecretDer {
    fn into_private_key(mut self) -> Result<PrivateKeyDer<'static>, ControlTlsError> {
        // Validate borrowed bytes first: a failed owned TryFrom<Vec<u8>> would
        // drop its input without giving this guard an opportunity to clear it.
        let parsed = PrivateKeyDer::try_from(self.0.as_slice())
            .map_err(|_| ControlTlsError::InvalidCredential)?;
        let key = match parsed {
            PrivateKeyDer::Pkcs1(_) => PrivateKeyDer::Pkcs1(std::mem::take(&mut self.0).into()),
            PrivateKeyDer::Sec1(_) => PrivateKeyDer::Sec1(std::mem::take(&mut self.0).into()),
            PrivateKeyDer::Pkcs8(_) => PrivateKeyDer::Pkcs8(std::mem::take(&mut self.0).into()),
            _ => return Err(ControlTlsError::InvalidCredential),
        };
        Ok(key)
    }

    fn clear(&mut self) {
        self.0.fill(0);
    }
}

impl Drop for SecretDer {
    fn drop(&mut self) {
        self.clear();
    }
}

fn build_server_config(
    identity: ControlTlsServerIdentity,
) -> Result<Arc<ServerConfig>, ControlTlsError> {
    let secret = SecretDer(identity.private_key_der);
    if identity.certificate_chain_der.is_empty()
        || secret.0.is_empty()
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
    let key = secret.into_private_key()?;
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
    #[error("blocking receive cannot resume an incremental control frame")]
    PartialReceiveInProgress,
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

    struct FragmentReader {
        bytes: std::collections::VecDeque<u8>,
        reads_left: usize,
        chunk: usize,
    }

    impl Read for FragmentReader {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if self.reads_left == 0 || self.bytes.is_empty() {
                return Err(std::io::ErrorKind::WouldBlock.into());
            }
            self.reads_left -= 1;
            let len = out.len().min(self.bytes.len()).min(self.chunk);
            for value in &mut out[..len] {
                *value = self.bytes.pop_front().unwrap();
            }
            Ok(len)
        }
    }

    #[test]
    fn incremental_poll_preserves_fragments_and_coalesced_frames() {
        let payload = envelope(SessionId::new(), "incremental");
        let mut wire = Vec::new();
        send_envelope(&mut wire, &payload).unwrap();
        send_envelope(&mut wire, &payload).unwrap();
        let mut reader = FragmentReader {
            bytes: wire.into(),
            reads_left: 1,
            chunk: 1,
        };
        let mut frame = IncrementalEnvelope::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        for _ in 0..2 {
            loop {
                reader.reads_left = 1;
                match frame.poll(&mut reader, deadline).unwrap() {
                    Some(bytes) => {
                        assert_eq!(bytes, payload);
                        break;
                    }
                    None => assert!(frame.deadline.is_some()),
                }
            }
        }
        assert!(reader.bytes.is_empty());
        assert!(frame.deadline.is_none());
        assert!(frame.poll(&mut reader, deadline).unwrap().is_none());
        assert!(
            frame.deadline.is_none(),
            "true idle must not start a frame timer"
        );
    }

    #[test]
    fn incremental_poll_bounds_lengths_and_plaintext_work() {
        let deadline = Instant::now() + Duration::from_secs(5);
        for length in [0, (MAX_CONTROL_ENVELOPE_BYTES + 1) as u32] {
            let mut frame = IncrementalEnvelope::default();
            assert!(matches!(
                frame.poll(&mut Cursor::new(length.to_be_bytes()), deadline),
                Err(ControlTlsError::RecordTooLarge)
            ));
            assert!(frame.body.is_empty());
        }
        let payload = envelope(SessionId::new(), &"bounded".repeat(100));
        let mut wire = Vec::new();
        send_envelope(&mut wire, &payload).unwrap();
        let mut reader = FragmentReader {
            bytes: wire.into(),
            reads_left: usize::MAX,
            chunk: 1,
        };
        let mut frame = IncrementalEnvelope::default();
        assert!(frame.poll(&mut reader, deadline).unwrap().is_none());
        assert_eq!(reader.reads_left, usize::MAX - POLL_PLAINTEXT_READS);
        assert_eq!(frame.prefix_read + frame.body_read, POLL_PLAINTEXT_READS);
    }

    #[test]
    fn incremental_poll_deadline_is_not_renewed_and_blocking_mix_fails() {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut frame = IncrementalEnvelope::default();
        let mut reader = FragmentReader {
            bytes: vec![0].into(),
            reads_left: 1,
            chunk: 1,
        };
        assert!(frame.poll(&mut reader, deadline).unwrap().is_none());
        assert_eq!(
            frame.effective_deadline(deadline + Duration::from_secs(5)),
            deadline
        );
        let earlier = Instant::now() - Duration::from_secs(1);
        assert_eq!(frame.effective_deadline(earlier), earlier);
        assert!(
            matches!(frame.poll(&mut reader, earlier), Err(ControlTlsError::Io(error))
            if error.kind() == std::io::ErrorKind::TimedOut)
        );
        let mut channel = FramedControl::new(Cursor::new(Vec::new()));
        channel.incoming = frame;
        assert!(matches!(
            channel.receive(),
            Err(ControlTlsError::PartialReceiveInProgress)
        ));
        assert!(channel.failed);
        assert_eq!(channel.io.position(), 0);
        assert!(matches!(
            channel.receive(),
            Err(ControlTlsError::ChannelFailed)
        ));
    }

    #[test]
    fn control_poll_timeout_preserves_earlier_retained_deadline() {
        let now = Instant::now();
        let saved = now - Duration::from_secs(1);
        let requested = now + Duration::from_secs(60);
        let frame = IncrementalEnvelope {
            prefix_read: 1,
            deadline: Some(saved),
            ..Default::default()
        };
        // This is the same effective deadline FramedControl::poll_until passes
        // to its preflight, bounded TLS I/O and incremental envelope checks.
        let effective = frame.effective_deadline(requested);
        assert_eq!(effective, saved);
        let error = check_poll_deadline(effective).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert_eq!(error.to_string(), "control poll deadline expired");
        assert_eq!(control_poll_timeout_deadline(&error), Some(saved));
        assert_ne!(control_poll_timeout_deadline(&error), Some(requested));
        let wrapped = ControlTlsError::Io(error);
        assert_eq!(
            wrapped.to_string(),
            "control TLS I/O failed: control poll deadline expired"
        );
    }

    #[test]
    fn control_poll_timeout_preserves_exact_requested_deadline() {
        let now = Instant::now();
        let requested = now - Duration::from_secs(1);
        for saved in [None, Some(now + Duration::from_secs(60)), Some(requested)] {
            let frame = IncrementalEnvelope {
                deadline: saved,
                ..Default::default()
            };
            let effective = frame.effective_deadline(requested);
            assert_eq!(effective, requested);
            let error = check_poll_deadline(effective).unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
            assert_eq!(control_poll_timeout_deadline(&error), Some(requested));
        }
    }

    #[test]
    fn control_poll_timeout_rejects_untyped_timeouts_even_with_same_display() {
        let errors = [
            std::io::Error::from(std::io::ErrorKind::TimedOut),
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "control poll deadline expired",
            ),
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "control operation deadline expired",
            ),
            std::io::Error::from(std::io::ErrorKind::WouldBlock),
        ];
        for error in errors {
            assert_eq!(control_poll_timeout_deadline(&error), None);
        }
        #[cfg(target_os = "linux")]
        {
            // Linux ETIMEDOUT, constructed without a socket or elapsed wait.
            let os_timeout = std::io::Error::from_raw_os_error(110);
            assert_eq!(os_timeout.kind(), std::io::ErrorKind::TimedOut);
            assert_eq!(control_poll_timeout_deadline(&os_timeout), None);
        }
        let error = check_poll_deadline(Instant::now() - Duration::from_secs(1)).unwrap_err();
        assert!(control_poll_timeout_deadline(&error).is_some());
        assert!(check_poll_deadline(Instant::now() + Duration::from_secs(60)).is_ok());
    }

    #[test]
    fn incremental_poll_rejects_malformed_and_truncated_plaintext() {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut malformed = IncrementalEnvelope::default();
        assert!(matches!(
            malformed.poll(&mut Cursor::new([0, 0, 0, 1, 0xff]), deadline),
            Err(ControlTlsError::InvalidEnvelope(_))
        ));
        for bytes in [vec![], vec![0, 0], vec![0, 0, 0, 5, 1, 2]] {
            let mut frame = IncrementalEnvelope::default();
            assert!(matches!(frame.poll(&mut Cursor::new(bytes), deadline),
                Err(ControlTlsError::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof));
        }
    }

    #[test]
    fn incremental_poll_transport_enforces_shared_call_byte_and_time_budgets() {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut io = Cursor::new(vec![7; POLL_WIRE_BYTES + 1]);
        let mut bounded = BoundedPollIo::new(&mut io, deadline);
        let mut bytes = vec![0; POLL_WIRE_BYTES + 1];
        assert_eq!(bounded.read(&mut bytes).unwrap(), POLL_WIRE_BYTES);
        assert_eq!(
            bounded.write(&[1]).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert_eq!(bounded.io.position(), POLL_WIRE_BYTES as u64);
        let mut bounded = BoundedPollIo::new(&mut io, deadline);
        for _ in 0..POLL_IO_CALLS {
            bounded.flush().unwrap();
        }
        assert_eq!(
            bounded.read(&mut [0]).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        let mut bounded = BoundedPollIo::new(&mut io, Instant::now() - Duration::from_secs(1));
        assert_eq!(
            bounded.flush().unwrap_err().kind(),
            std::io::ErrorKind::TimedOut
        );
    }

    #[test]
    fn secret_der_cleanup_clears_entire_transient_buffer() {
        let mut secret = SecretDer(vec![7, 11, 19, 31]);
        secret.clear();
        assert_eq!(secret.0, [0, 0, 0, 0]);
    }

    #[test]
    fn secret_der_rejects_invalid_borrowed_encoding() {
        assert!(matches!(
            SecretDer(vec![0xff; 32]).into_private_key(),
            Err(ControlTlsError::InvalidCredential)
        ));
    }

    #[test]
    fn secret_der_server_rejects_invalid_ca_before_key_handoff() {
        let (mut identity, _) = test_identities();
        identity.client_ca_der = vec![0xff; 32];
        assert!(matches!(
            build_server_config(identity),
            Err(ControlTlsError::InvalidCredential)
        ));
    }

    #[test]
    fn accept_deadline_check_rejects_expired_budget_without_io() {
        assert_eq!(
            check_poll_deadline(Instant::now() - Duration::from_millis(1))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::TimedOut
        );
        assert!(check_poll_deadline(Instant::now() + Duration::from_secs(1)).is_ok());
    }

    #[test]
    fn connect_deadline_check_clamps_tcp_timeout_to_remaining_budget() {
        let deadline = Instant::now() + Duration::from_millis(50);
        let timeout = connect_timeout_until(Duration::from_secs(2), deadline).unwrap();
        assert!(!timeout.is_zero());
        assert!(timeout <= Duration::from_millis(50));
        assert_eq!(
            connect_timeout_until(
                Duration::from_millis(100),
                Instant::now() + Duration::from_secs(2),
            )
            .unwrap(),
            Duration::from_millis(100)
        );
        assert!(matches!(
            connect_timeout_until(
                Duration::from_secs(2),
                Instant::now() - Duration::from_millis(1),
            ),
            Err(ControlTlsError::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut
        ));
    }

    #[test]
    fn connect_deadline_check_expired_dial_apis_fail_without_io() {
        // Invalid endpoint/credential fixtures ensure this regression cannot
        // open a socket even if the initial deadline check is accidentally lost.
        let endpoint = ControlTlsEndpoint {
            address: (Ipv4Addr::UNSPECIFIED, 0).into(),
            io_timeout: Duration::from_secs(2),
        };
        let identity = ControlTlsClientIdentity {
            expected_peer_certificate: CertificateFingerprint::new([7; 32]).unwrap(),
            certificate_chain_der: Vec::new(),
            private_key_der: Vec::new(),
            server_ca_der: Vec::new(),
        };
        let expected = binding();
        let deadline = Instant::now() - Duration::from_millis(1);
        assert!(matches!(
            AuthenticatedControlClientChannel::connect_until(
                endpoint,
                "localhost",
                identity.clone(),
                expected,
                deadline,
            ),
            Err(ControlTlsError::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut
        ));
        assert!(matches!(
            AuthenticatedControlClientChannel::connect_fresh_session_until(
                endpoint,
                "localhost",
                identity,
                ControlTlsNodeBinding::new(expected.server_node_id, expected.client_node_id)
                    .unwrap(),
                deadline,
            ),
            Err(ControlTlsError::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut
        ));
    }

    #[test]
    fn ipc_connect_until_pinned_tls_exact_and_fresh_binding_succeed() {
        for fresh in [false, true] {
            let (server_identity, client_identity) = test_identities();
            let expected = binding();
            let nodes =
                ControlTlsNodeBinding::new(expected.server_node_id, expected.client_node_id)
                    .unwrap();
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            let endpoint = ControlTlsEndpoint {
                address: listener.local_addr().unwrap(),
                io_timeout: Duration::from_secs(2),
            };
            let server = if fresh {
                AuthenticatedControlServer::from_listener_fresh_session(
                    listener,
                    server_identity,
                    nodes,
                    endpoint.io_timeout,
                )
            } else {
                AuthenticatedControlServer::from_listener(
                    listener,
                    server_identity,
                    expected,
                    endpoint.io_timeout,
                )
            }
            .unwrap();
            let worker = thread::spawn(move || server.accept().unwrap());
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut client = if fresh {
                AuthenticatedControlClientChannel::connect_fresh_session_until(
                    endpoint,
                    "localhost",
                    client_identity,
                    nodes,
                    deadline,
                )
            } else {
                AuthenticatedControlClientChannel::connect_until(
                    endpoint,
                    "localhost",
                    client_identity,
                    expected,
                    deadline,
                )
            }
            .unwrap();
            let mut accepted = worker.join().unwrap();
            assert_eq!(client.binding(), accepted.binding());
            assert_eq!(client.binding().server_node_id, expected.server_node_id);
            assert_eq!(client.binding().client_node_id, expected.client_node_id);
            if fresh {
                assert_ne!(client.binding().session_id, expected.session_id);
                assert!(!client.binding().session_id.as_uuid().is_nil());
            } else {
                assert_eq!(client.binding(), expected);
            }
            assert_eq!(
                client.stream.io.sock.read_timeout().unwrap(),
                Some(endpoint.io_timeout)
            );
            assert_eq!(
                client.stream.io.sock.write_timeout().unwrap(),
                Some(endpoint.io_timeout)
            );
            // A successful dial leaves no watcher that later closes the channel.
            thread::sleep(deadline.saturating_duration_since(Instant::now()));
            let payload = envelope(client.binding().session_id, "deadline dial");
            client.send_envelope(&payload).unwrap();
            assert_eq!(accepted.receive_envelope().unwrap(), payload);
            accepted.send_envelope(&payload).unwrap();
            assert_eq!(client.receive_envelope().unwrap(), payload);
        }
    }

    #[test]
    fn ipc_connect_until_stalled_handshake_times_out_and_shuts_down_peer() {
        let (_, identity) = test_identities();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let endpoint = ControlTlsEndpoint {
            address: listener.local_addr().unwrap(),
            io_timeout: Duration::from_secs(2),
        };
        let worker = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut hello = Vec::new();
            // EOF must arrive from terminal client shutdown, not our timeout.
            socket.read_to_end(&mut hello).unwrap();
            assert!(!hello.is_empty());
        });
        let start = Instant::now();
        let result = AuthenticatedControlClientChannel::connect_until(
            endpoint,
            "localhost",
            identity,
            binding(),
            start + Duration::from_millis(250),
        );
        let elapsed = start.elapsed();
        worker.join().unwrap();
        assert!(matches!(result, Err(ControlTlsError::Io(error))
            if error.kind() == std::io::ErrorKind::TimedOut));
        assert!(elapsed >= Duration::from_millis(250));
        assert!(elapsed < Duration::from_secs(1));
    }

    #[test]
    fn ipc_connect_until_trickled_handshake_cannot_renew_whole_deadline() {
        let (server_identity, client_identity) = test_identities();
        let config = build_server_config(server_identity).unwrap();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let endpoint = ControlTlsEndpoint {
            address: listener.local_addr().unwrap(),
            io_timeout: Duration::from_secs(2),
        };
        let writer = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            socket.set_nodelay(true).unwrap();
            let mut connection = ServerConnection::new(config).unwrap();
            while !connection.wants_write() {
                assert!(connection.read_tls(&mut socket).unwrap() > 0);
                connection.process_new_packets().unwrap();
            }
            let mut flight = Vec::new();
            while connection.wants_write() {
                connection.write_tls(&mut flight).unwrap();
            }
            assert!(flight.len() > 100);
            let mut sent = 0;
            // Keep the fixture finite even if the deadline regresses.
            for byte in flight.iter().take(100) {
                if socket.write_all(&[*byte]).is_err() {
                    break;
                }
                sent += 1;
                thread::sleep(Duration::from_millis(20));
            }
            sent
        });
        let start = Instant::now();
        let result = AuthenticatedControlClientChannel::connect_until(
            endpoint,
            "localhost",
            client_identity,
            binding(),
            start + Duration::from_millis(250),
        );
        let elapsed = start.elapsed();
        let sent = writer.join().unwrap();
        assert!(matches!(result, Err(ControlTlsError::Io(error))
            if error.kind() == std::io::ErrorKind::TimedOut));
        assert!(elapsed >= Duration::from_millis(250));
        assert!(elapsed < Duration::from_secs(1));
        assert!(sent > 0, "the peer started its TLS handshake response");
        assert!(sent < 100, "the peer observed terminal shutdown");
    }

    #[test]
    fn ipc_connect_until_trickled_binding_shares_handshake_deadline() {
        let (server_identity, client_identity) = test_identities();
        let config = build_server_config(server_identity).unwrap();
        let expected = binding();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let endpoint = ControlTlsEndpoint {
            address: listener.local_addr().unwrap(),
            io_timeout: Duration::from_secs(2),
        };
        let writer = thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            socket.set_nodelay(true).unwrap();
            let connection = ServerConnection::new(config).unwrap();
            let mut stream = StreamOwned::new(connection, socket);
            while stream.conn.is_handshaking() {
                stream.conn.complete_io(&mut stream.sock).unwrap();
            }
            let mut request = [0; CONTROL_BIND_BYTES];
            stream.read_exact(&mut request).unwrap();
            verify_bind(&request, expected, BIND_REQUEST).unwrap();
            thread::sleep(Duration::from_millis(100));
            let mut sent = 0;
            for byte in encode_bind(expected, BIND_ACK) {
                if stream
                    .write_all(&[byte])
                    .and_then(|()| stream.flush())
                    .is_err()
                {
                    break;
                }
                sent += 1;
                thread::sleep(Duration::from_millis(20));
            }
            sent
        });
        let start = Instant::now();
        let result = AuthenticatedControlClientChannel::connect_until(
            endpoint,
            "localhost",
            client_identity,
            expected,
            start + Duration::from_millis(400),
        );
        let elapsed = start.elapsed();
        let sent = writer.join().unwrap();
        assert!(matches!(result, Err(ControlTlsError::Io(error))
            if error.kind() == std::io::ErrorKind::TimedOut));
        assert!(elapsed >= Duration::from_millis(400));
        assert!(elapsed < Duration::from_secs(1));
        assert!(sent > 0, "the peer reached the binding acknowledgement");
        assert!(
            sent < CONTROL_BIND_BYTES,
            "the peer observed terminal shutdown"
        );
    }

    #[test]
    fn ipc_try_accept_idle_is_immediate_and_expired_idle_is_error() {
        let (identity, _) = test_identities();
        let server = AuthenticatedControlServer::from_listener(
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap(),
            identity,
            binding(),
            Duration::from_secs(2),
        )
        .unwrap();
        let start = Instant::now();
        assert!(
            server
                .try_accept_until(start + Duration::from_secs(2))
                .unwrap()
                .is_none()
        );
        assert!(start.elapsed() < Duration::from_millis(500));
        assert!(matches!(
            server.try_accept_until(Instant::now() - Duration::from_millis(1)),
            Err(ControlTlsError::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut
        ));
        // A delayed client must still be accepted by the legacy blocking API.
        let address = server.local_addr().unwrap();
        let connector = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            TcpStream::connect(address).unwrap()
        });
        // The listener itself proves restoration without waiting for TLS.
        assert!(server.listener.accept().is_ok());
        connector.join().unwrap();
    }

    #[test]
    fn ipc_try_accept_pinned_tls_exact_and_fresh_binding_succeed() {
        for fresh in [false, true] {
            let (server_identity, client_identity) = test_identities();
            let binding = binding();
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            let address = listener.local_addr().unwrap();
            let server = if fresh {
                AuthenticatedControlServer::from_listener_fresh_session(
                    listener,
                    server_identity,
                    ControlTlsNodeBinding::new(binding.server_node_id, binding.client_node_id)
                        .unwrap(),
                    Duration::from_secs(2),
                )
            } else {
                AuthenticatedControlServer::from_listener(
                    listener,
                    server_identity,
                    binding,
                    Duration::from_secs(2),
                )
            }
            .unwrap();
            let client = thread::spawn(move || {
                AuthenticatedControlClientChannel::connect(
                    ControlTlsEndpoint {
                        address,
                        io_timeout: Duration::from_secs(2),
                    },
                    "localhost",
                    client_identity,
                    binding,
                )
                .unwrap()
            });
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut accepted = loop {
                if let Some(channel) = server.try_accept_until(deadline).unwrap() {
                    break channel;
                }
                thread::yield_now();
            };
            let mut client = client.join().unwrap();
            assert_eq!(accepted.binding(), binding);
            assert_eq!(client.binding(), binding);
            // Success leaves the connection usable after watcher cancellation.
            assert!(accepted.poll_envelope_until(deadline).unwrap().is_none());
            assert!(client.poll_envelope_until(deadline).unwrap().is_none());
        }
    }

    #[test]
    fn ipc_try_accept_trickled_handshake_cannot_renew_whole_deadline() {
        let (server_identity, client_identity) = test_identities();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = AuthenticatedControlServer::from_listener(
            listener,
            server_identity,
            binding(),
            Duration::from_secs(2),
        )
        .unwrap();
        let mut client = ClientConnection::new(
            build_client_config(client_identity).unwrap(),
            ServerName::try_from("localhost").unwrap(),
        )
        .unwrap();
        let mut hello = Vec::new();
        client.write_tls(&mut hello).unwrap();
        assert!(hello.len() > 30);
        let mut socket = TcpStream::connect(address).unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let writer = thread::spawn(move || {
            for byte in hello {
                if socket.write_all(&[byte]).is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
        });
        let start = Instant::now();
        let result = server.try_accept_until(start + Duration::from_millis(250));
        let elapsed = start.elapsed();
        assert!(matches!(result, Err(ControlTlsError::Io(error))
            if error.kind() == std::io::ErrorKind::TimedOut));
        assert!(elapsed >= Duration::from_millis(250));
        assert!(elapsed < Duration::from_secs(1));
        writer.join().unwrap();
    }

    #[test]
    fn ipc_try_accept_trickled_binding_shares_handshake_deadline() {
        let (server_identity, client_identity) = test_identities();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let binding = binding();
        let server = AuthenticatedControlServer::from_listener(
            listener,
            server_identity,
            binding,
            Duration::from_secs(2),
        )
        .unwrap();
        let socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let writer = thread::spawn(move || {
            let connection = ClientConnection::new(
                build_client_config(client_identity).unwrap(),
                ServerName::try_from("localhost").unwrap(),
            )
            .unwrap();
            let mut stream = StreamOwned::new(connection, socket);
            while stream.conn.is_handshaking() {
                stream.conn.complete_io(&mut stream.sock).unwrap();
            }
            let mut sent = 0;
            for byte in encode_bind(binding, BIND_REQUEST) {
                if stream
                    .write_all(&[byte])
                    .and_then(|()| stream.flush())
                    .is_err()
                {
                    break;
                }
                sent += 1;
                thread::sleep(Duration::from_millis(20));
            }
            sent
        });
        let start = Instant::now();
        let result = server.try_accept_until(start + Duration::from_millis(400));
        let elapsed = start.elapsed();
        assert!(matches!(result, Err(ControlTlsError::Io(error))
            if error.kind() == std::io::ErrorKind::TimedOut));
        assert!(elapsed >= Duration::from_millis(400));
        assert!(elapsed < Duration::from_secs(1));
        let sent = writer.join().unwrap();
        assert!(
            sent > 0,
            "the peer reached the authenticated binding exchange"
        );
        assert!(
            sent < CONTROL_BIND_BYTES,
            "the peer observed terminal shutdown"
        );
    }

    fn incremental_tls_pair() -> (
        AuthenticatedControlServerChannel,
        AuthenticatedControlClientChannel,
    ) {
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
        let accepted = thread::spawn(move || server.accept().unwrap());
        let client = AuthenticatedControlClientChannel::connect(
            ControlTlsEndpoint {
                address,
                io_timeout: Duration::from_secs(5),
            },
            "localhost",
            client_identity,
            binding,
        )
        .unwrap();
        (accepted.join().unwrap(), client)
    }

    #[test]
    fn ipc_incremental_poll_real_tls_idle_fragments_and_coalesced_frames() {
        let (mut server, mut client) = incremental_tls_pair();
        let deadline = Instant::now() + Duration::from_secs(5);
        assert!(server.poll_envelope_until(deadline).unwrap().is_none());
        assert!(client.poll_envelope_until(deadline).unwrap().is_none());
        let payload = envelope(client.binding().session_id, "fragmented TLS");
        let mut wire = Vec::new();
        send_envelope(&mut wire, &payload).unwrap();
        client.stream.io.write_all(&wire[..2]).unwrap();
        client.stream.io.flush().unwrap();
        while server.stream.incoming.prefix_read < 2 {
            assert!(server.poll_envelope_until(deadline).unwrap().is_none());
            thread::yield_now();
        }
        client.stream.io.write_all(&wire[2..]).unwrap();
        client.stream.io.write_all(&wire).unwrap();
        client.stream.io.flush().unwrap();
        for _ in 0..2 {
            loop {
                if let Some(actual) = server.poll_envelope_until(deadline).unwrap() {
                    assert_eq!(actual, payload);
                    break;
                }
                thread::yield_now();
            }
        }
        assert!(server.poll_envelope_until(deadline).unwrap().is_none());
        // Both directions and legacy blocking receive remain usable after polling.
        server.send_envelope(&payload).unwrap();
        assert_eq!(client.receive_envelope().unwrap(), payload);
    }

    #[test]
    fn ipc_incremental_poll_partial_frame_deadline_and_expired_idle_are_terminal() {
        let (mut server, mut client) = incremental_tls_pair();
        let deadline = Instant::now() + Duration::from_millis(150);
        client.stream.io.write_all(&[0]).unwrap();
        client.stream.io.flush().unwrap();
        while server.stream.incoming.prefix_read == 0 {
            assert!(server.poll_envelope_until(deadline).unwrap().is_none());
            thread::yield_now();
        }
        thread::sleep(
            deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(5),
        );
        assert!(
            matches!(server.poll_envelope_until(Instant::now() + Duration::from_secs(5)),
            Err(ControlTlsError::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut)
        );
        assert!(matches!(
            server.poll_envelope_until(Instant::now() + Duration::from_secs(5)),
            Err(ControlTlsError::ChannelFailed)
        ));
        assert!(
            matches!(client.poll_envelope_until(Instant::now() - Duration::from_secs(1)),
            Err(ControlTlsError::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut)
        );
        assert!(matches!(
            client.receive_envelope(),
            Err(ControlTlsError::ChannelFailed)
        ));
    }

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
