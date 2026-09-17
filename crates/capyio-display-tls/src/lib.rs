#![forbid(unsafe_code)]

//! Mutually authenticated TLS 1.3 transport for complete display records.

use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream},
    sync::Arc,
    time::Duration,
};

use capyio_display_transport::{
    AvcLevel, AvcProfile, DisplayTransportBinding, decode_private_display_record,
};
use rustls::{
    ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
    pki_types::{CertificateDer, PrivateKeyDer, ServerName},
    server::WebPkiClientVerifier,
    version::TLS13,
};
use thiserror::Error;

pub const DISPLAY_TLS_ALPN: &[u8] = b"capyio-display/2";
pub const GAMESTREAM_LAUNCH_TLS_ALPN: &[u8] = b"capyio-gamestream-launch/1";
pub const DISPLAY_BIND_BYTES: usize = 96;
pub const GAMESTREAM_LAUNCH_RECORD_BYTES: usize = 176;
pub const GAMESTREAM_LAUNCH_ACK_BYTES: usize = 64;
pub const MAX_TLS_DISPLAY_RECORD_BYTES: usize = 16 * 1024 * 1024 + 68;
const BIND_MAGIC: [u8; 8] = *b"CPYDTLS2";
const BIND_VERSION: u8 = 2;
const BIND_REQUEST: u8 = 1;
const BIND_ACK: u8 = 2;
const BIND_BODY_BYTES: u32 = 80;

#[derive(Debug, Error)]
pub enum DisplayTlsError {
    #[error("TLS display endpoint must use an explicit non-unspecified IPv4 address and port")]
    InvalidEndpoint,
    #[error("TLS display timeout must be inside 100ms..=30s")]
    InvalidTimeout,
    #[error("TLS display credential is invalid: {0}")]
    InvalidCredential(String),
    #[error("TLS display configuration failed: {0}")]
    TlsConfiguration(String),
    #[error("TLS display I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("TLS display handshake failed: {0}")]
    Tls(#[from] rustls::Error),
    #[error("TLS peer did not negotiate the required CapyIO application protocol")]
    WrongApplicationProtocol,
    #[error("TLS display Route binding is malformed or unauthorized")]
    UnauthorizedBinding,
    #[error("TLS display record is outside its bound")]
    RecordTooLarge,
    #[error("TLS display record violates the selected stream contract: {0}")]
    InvalidRecord(String),
    #[error("TLS display heartbeat acknowledgement is invalid")]
    InvalidHeartbeatAcknowledgement,
    #[error("TLS GameStream launch acknowledgement is invalid")]
    InvalidGameStreamLaunchAcknowledgement,
}

pub struct DisplayTlsServerIdentity {
    pub certificate_chain_der: Vec<Vec<u8>>,
    pub private_key_der: Vec<u8>,
    pub client_ca_der: Vec<u8>,
}

#[derive(Clone)]
pub struct DisplayTlsClientIdentity {
    pub certificate_chain_der: Vec<Vec<u8>>,
    pub private_key_der: Vec<u8>,
    pub server_ca_der: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayTlsServerEndpoint {
    pub address: Ipv4Addr,
    pub port: u16,
    pub io_timeout: Duration,
}

impl DisplayTlsServerEndpoint {
    pub fn validate(self) -> Result<(), DisplayTlsError> {
        if self.address.is_unspecified() || self.port == 0 {
            return Err(DisplayTlsError::InvalidEndpoint);
        }
        if !(Duration::from_millis(100)..=Duration::from_secs(30)).contains(&self.io_timeout) {
            return Err(DisplayTlsError::InvalidTimeout);
        }
        Ok(())
    }
}

pub struct AuthenticatedDisplaySender {
    stream: StreamOwned<ServerConnection, TcpStream>,
    binding: DisplayTransportBinding,
}

pub struct AuthenticatedDisplayReceiver {
    stream: StreamOwned<ClientConnection, TcpStream>,
    binding: DisplayTransportBinding,
}

#[derive(Debug, Eq, PartialEq)]
pub enum AuthenticatedDisplayItem {
    Heartbeat,
    Record(Vec<u8>),
}

/// Long-lived listener for sequential, independently authenticated display
/// receiver connections. A failed handshake or Route binding consumes only
/// that connection; the listener and expected authorization remain intact.
pub struct AuthenticatedDisplayServer {
    listener: TcpListener,
    config: Arc<ServerConfig>,
    expected: DisplayTransportBinding,
    io_timeout: Duration,
}

/// Separate mTLS control listener for one-shot GameStream launch material.
/// It never accepts CPD1 media records and never starts the native engine.
pub struct AuthenticatedGameStreamLaunchServer {
    listener: TcpListener,
    config: Arc<ServerConfig>,
    io_timeout: Duration,
}

impl AuthenticatedGameStreamLaunchServer {
    pub fn bind(
        endpoint: DisplayTlsServerEndpoint,
        identity: DisplayTlsServerIdentity,
    ) -> Result<Self, DisplayTlsError> {
        endpoint.validate()?;
        let listener = TcpListener::bind(SocketAddrV4::new(endpoint.address, endpoint.port))?;
        Self::from_listener(listener, identity, endpoint.io_timeout)
    }

    pub fn from_listener(
        listener: TcpListener,
        identity: DisplayTlsServerIdentity,
        io_timeout: Duration,
    ) -> Result<Self, DisplayTlsError> {
        if !(Duration::from_millis(100)..=Duration::from_secs(30)).contains(&io_timeout) {
            return Err(DisplayTlsError::InvalidTimeout);
        }
        Ok(Self {
            listener,
            config: build_server_config(identity, GAMESTREAM_LAUNCH_TLS_ALPN)?,
            io_timeout,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, DisplayTlsError> {
        Ok(self.listener.local_addr()?)
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> Result<(), DisplayTlsError> {
        Ok(self.listener.set_nonblocking(nonblocking)?)
    }

    /// Completes TLS 1.3 and client-certificate authentication before sending
    /// the secret-bearing record, then requires the caller-supplied exact ack.
    pub fn exchange(
        &self,
        record: &[u8; GAMESTREAM_LAUNCH_RECORD_BYTES],
        expected_acknowledgement: &[u8; GAMESTREAM_LAUNCH_ACK_BYTES],
    ) -> Result<(), DisplayTlsError> {
        let (socket, _) = self.listener.accept()?;
        self.exchange_socket(socket, record, expected_acknowledgement)
    }

    pub fn try_exchange(
        &self,
        record: &[u8; GAMESTREAM_LAUNCH_RECORD_BYTES],
        expected_acknowledgement: &[u8; GAMESTREAM_LAUNCH_ACK_BYTES],
    ) -> Result<bool, DisplayTlsError> {
        let (socket, _) = match self.listener.accept() {
            Ok(accepted) => accepted,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) => return Err(DisplayTlsError::Io(error)),
        };
        self.exchange_socket(socket, record, expected_acknowledgement)?;
        Ok(true)
    }

    fn exchange_socket(
        &self,
        socket: TcpStream,
        record: &[u8; GAMESTREAM_LAUNCH_RECORD_BYTES],
        expected_acknowledgement: &[u8; GAMESTREAM_LAUNCH_ACK_BYTES],
    ) -> Result<(), DisplayTlsError> {
        socket.set_nonblocking(false)?;
        socket.set_read_timeout(Some(self.io_timeout))?;
        socket.set_write_timeout(Some(self.io_timeout))?;
        socket.set_nodelay(true)?;
        let connection = ServerConnection::new(self.config.clone())?;
        let mut stream = StreamOwned::new(connection, socket);
        while stream.conn.is_handshaking() {
            stream.conn.complete_io(&mut stream.sock)?;
        }
        if stream.conn.protocol_version() != Some(rustls::ProtocolVersion::TLSv1_3)
            || stream.conn.alpn_protocol() != Some(GAMESTREAM_LAUNCH_TLS_ALPN)
        {
            return Err(DisplayTlsError::WrongApplicationProtocol);
        }
        stream.write_all(record)?;
        stream.flush()?;
        let mut acknowledgement = [0_u8; GAMESTREAM_LAUNCH_ACK_BYTES];
        stream.read_exact(&mut acknowledgement)?;
        if &acknowledgement != expected_acknowledgement {
            return Err(DisplayTlsError::InvalidGameStreamLaunchAcknowledgement);
        }
        Ok(())
    }
}

impl AuthenticatedDisplayServer {
    pub fn bind(
        endpoint: DisplayTlsServerEndpoint,
        identity: DisplayTlsServerIdentity,
        expected: DisplayTransportBinding,
    ) -> Result<Self, DisplayTlsError> {
        endpoint.validate()?;
        let listener = TcpListener::bind(SocketAddrV4::new(endpoint.address, endpoint.port))?;
        Self::from_listener(listener, identity, expected, endpoint.io_timeout)
    }

    pub fn from_listener(
        listener: TcpListener,
        identity: DisplayTlsServerIdentity,
        expected: DisplayTransportBinding,
        io_timeout: Duration,
    ) -> Result<Self, DisplayTlsError> {
        if !(Duration::from_millis(100)..=Duration::from_secs(30)).contains(&io_timeout) {
            return Err(DisplayTlsError::InvalidTimeout);
        }
        Ok(Self {
            listener,
            config: build_server_config(identity, DISPLAY_TLS_ALPN)?,
            expected,
            io_timeout,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, DisplayTlsError> {
        Ok(self.listener.local_addr()?)
    }

    /// Controls only listener acceptance. Every accepted socket is restored to
    /// blocking mode and remains bounded by the configured handshake and I/O
    /// timeout.
    pub fn set_nonblocking(&self, nonblocking: bool) -> Result<(), DisplayTlsError> {
        Ok(self.listener.set_nonblocking(nonblocking)?)
    }

    /// Accepts and fully authenticates one receiver. Call again after the
    /// returned sender disconnects, or after this call rejects a peer.
    pub fn accept(&self) -> Result<AuthenticatedDisplaySender, DisplayTlsError> {
        let (socket, _) = self.listener.accept()?;
        socket.set_nonblocking(false)?;
        AuthenticatedDisplaySender::authenticate(
            socket,
            self.config.clone(),
            self.expected.clone(),
            self.io_timeout,
        )
    }

    /// Polls a nonblocking listener without converting `WouldBlock` into an
    /// authentication failure. Call `set_nonblocking(true)` before using it.
    pub fn try_accept(&self) -> Result<Option<AuthenticatedDisplaySender>, DisplayTlsError> {
        let (socket, _) = match self.listener.accept() {
            Ok(accepted) => accepted,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
            Err(error) => return Err(DisplayTlsError::Io(error)),
        };
        socket.set_nonblocking(false)?;
        AuthenticatedDisplaySender::authenticate(
            socket,
            self.config.clone(),
            self.expected.clone(),
            self.io_timeout,
        )
        .map(Some)
    }
}

impl AuthenticatedDisplaySender {
    /// Binds and accepts one mutually authenticated receiver. Route
    /// authorization is represented by the exact expected binding supplied by
    /// the Runtime; certificate validation alone cannot authorize a Route.
    pub fn accept_once(
        endpoint: DisplayTlsServerEndpoint,
        identity: DisplayTlsServerIdentity,
        expected: DisplayTransportBinding,
    ) -> Result<Self, DisplayTlsError> {
        AuthenticatedDisplayServer::bind(endpoint, identity, expected)?.accept()
    }

    pub fn accept_from_listener(
        listener: TcpListener,
        identity: DisplayTlsServerIdentity,
        expected: DisplayTransportBinding,
        io_timeout: Duration,
    ) -> Result<Self, DisplayTlsError> {
        AuthenticatedDisplayServer::from_listener(listener, identity, expected, io_timeout)?
            .accept()
    }

    fn authenticate(
        socket: TcpStream,
        config: Arc<ServerConfig>,
        expected: DisplayTransportBinding,
        io_timeout: Duration,
    ) -> Result<Self, DisplayTlsError> {
        socket.set_read_timeout(Some(io_timeout))?;
        socket.set_write_timeout(Some(io_timeout))?;
        socket.set_nodelay(true)?;
        let connection = ServerConnection::new(config)?;
        let mut stream = StreamOwned::new(connection, socket);

        let mut request = [0_u8; DISPLAY_BIND_BYTES];
        stream.read_exact(&mut request)?;
        if stream.conn.alpn_protocol() != Some(DISPLAY_TLS_ALPN) {
            return Err(DisplayTlsError::WrongApplicationProtocol);
        }
        verify_bind_message(&request, &expected, BIND_REQUEST)?;
        stream.write_all(&encode_bind_message(&expected, BIND_ACK))?;
        stream.flush()?;
        Ok(Self {
            stream,
            binding: expected,
        })
    }

    #[must_use]
    pub const fn binding(&self) -> &DisplayTransportBinding {
        &self.binding
    }

    pub fn send_record(&mut self, record: &[u8]) -> Result<(), DisplayTlsError> {
        if record.is_empty() || record.len() > MAX_TLS_DISPLAY_RECORD_BYTES {
            return Err(DisplayTlsError::RecordTooLarge);
        }
        decode_private_display_record(record, self.binding.selected())
            .map_err(|error| DisplayTlsError::InvalidRecord(error.to_string()))?;
        let length = u32::try_from(record.len()).map_err(|_| DisplayTlsError::RecordTooLarge)?;
        self.stream.write_all(&length.to_be_bytes())?;
        self.stream.write_all(record)?;
        self.stream.flush()?;
        Ok(())
    }

    /// Sends the reserved zero-length outer frame and requires the peer to
    /// echo the same prefix. The socket read/write timeouts bound the entire
    /// round trip; no heartbeat byte reaches the private CPD1 decoder.
    pub fn round_trip_heartbeat(&mut self) -> Result<(), DisplayTlsError> {
        let heartbeat = 0_u32.to_be_bytes();
        self.stream.write_all(&heartbeat)?;
        self.stream.flush()?;
        let mut acknowledgement = [0_u8; 4];
        self.stream.read_exact(&mut acknowledgement)?;
        if acknowledgement != heartbeat {
            return Err(DisplayTlsError::InvalidHeartbeatAcknowledgement);
        }
        Ok(())
    }
}

impl AuthenticatedDisplayReceiver {
    pub fn connect(
        endpoint: DisplayTlsServerEndpoint,
        server_name: &str,
        identity: DisplayTlsClientIdentity,
        expected: DisplayTransportBinding,
    ) -> Result<Self, DisplayTlsError> {
        endpoint.validate()?;
        if server_name.is_empty() || server_name.len() > 253 {
            return Err(DisplayTlsError::InvalidEndpoint);
        }
        let socket = TcpStream::connect(SocketAddrV4::new(endpoint.address, endpoint.port))?;
        socket.set_read_timeout(Some(endpoint.io_timeout))?;
        socket.set_write_timeout(Some(endpoint.io_timeout))?;
        socket.set_nodelay(true)?;
        let server_name = ServerName::try_from(server_name.to_owned())
            .map_err(|_| DisplayTlsError::InvalidEndpoint)?;
        let connection = ClientConnection::new(
            build_client_config(identity, DISPLAY_TLS_ALPN)?,
            server_name,
        )?;
        let mut stream = StreamOwned::new(connection, socket);
        stream.write_all(&encode_bind_request(&expected))?;
        stream.flush()?;
        let mut acknowledgement = [0_u8; DISPLAY_BIND_BYTES];
        stream.read_exact(&mut acknowledgement)?;
        if stream.conn.alpn_protocol() != Some(DISPLAY_TLS_ALPN) {
            return Err(DisplayTlsError::WrongApplicationProtocol);
        }
        verify_bind_ack(&acknowledgement, &expected)?;
        Ok(Self {
            stream,
            binding: expected,
        })
    }

    #[must_use]
    pub const fn binding(&self) -> &DisplayTransportBinding {
        &self.binding
    }

    pub fn receive(&mut self) -> Result<AuthenticatedDisplayItem, DisplayTlsError> {
        let mut length = [0_u8; 4];
        self.stream.read_exact(&mut length)?;
        let length = u32::from_be_bytes(length);
        if length == 0 {
            self.stream.write_all(&0_u32.to_be_bytes())?;
            self.stream.flush()?;
            return Ok(AuthenticatedDisplayItem::Heartbeat);
        }
        let length = usize::try_from(length).map_err(|_| DisplayTlsError::RecordTooLarge)?;
        if length > MAX_TLS_DISPLAY_RECORD_BYTES {
            return Err(DisplayTlsError::RecordTooLarge);
        }
        let mut record = vec![0_u8; length];
        self.stream.read_exact(&mut record)?;
        decode_private_display_record(&record, self.binding.selected())
            .map_err(|error| DisplayTlsError::InvalidRecord(error.to_string()))?;
        Ok(AuthenticatedDisplayItem::Record(record))
    }
}

fn build_server_config(
    identity: DisplayTlsServerIdentity,
    alpn: &[u8],
) -> Result<Arc<ServerConfig>, DisplayTlsError> {
    if identity.certificate_chain_der.is_empty()
        || identity.private_key_der.is_empty()
        || identity.client_ca_der.is_empty()
    {
        return Err(DisplayTlsError::InvalidCredential(
            "certificate chain, private key and client trust root are required".to_owned(),
        ));
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(identity.client_ca_der))
        .map_err(|error| DisplayTlsError::InvalidCredential(error.to_string()))?;
    let verifier = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider.clone())
        .build()
        .map_err(|error| DisplayTlsError::TlsConfiguration(error.to_string()))?;
    let certificates = identity
        .certificate_chain_der
        .into_iter()
        .map(CertificateDer::from)
        .collect();
    let key = PrivateKeyDer::try_from(identity.private_key_der)
        .map_err(|error| DisplayTlsError::InvalidCredential(error.to_string()))?;
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&TLS13])
        .map_err(|error| DisplayTlsError::TlsConfiguration(error.to_string()))?
        .with_client_cert_verifier(verifier)
        .with_single_cert(certificates, key)
        .map_err(|error| DisplayTlsError::InvalidCredential(error.to_string()))?;
    config.alpn_protocols = vec![alpn.to_vec()];
    config.max_early_data_size = 0;
    config.send_tls13_tickets = 0;
    Ok(Arc::new(config))
}

fn build_client_config(
    identity: DisplayTlsClientIdentity,
    alpn: &[u8],
) -> Result<Arc<ClientConfig>, DisplayTlsError> {
    if identity.certificate_chain_der.is_empty()
        || identity.private_key_der.is_empty()
        || identity.server_ca_der.is_empty()
    {
        return Err(DisplayTlsError::InvalidCredential(
            "certificate chain, private key and server trust root are required".to_owned(),
        ));
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(identity.server_ca_der))
        .map_err(|error| DisplayTlsError::InvalidCredential(error.to_string()))?;
    let certificates = identity
        .certificate_chain_der
        .into_iter()
        .map(CertificateDer::from)
        .collect();
    let key = PrivateKeyDer::try_from(identity.private_key_der)
        .map_err(|error| DisplayTlsError::InvalidCredential(error.to_string()))?;
    let mut config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&TLS13])
        .map_err(|error| DisplayTlsError::TlsConfiguration(error.to_string()))?
        .with_root_certificates(roots)
        .with_client_auth_cert(certificates, key)
        .map_err(|error| DisplayTlsError::InvalidCredential(error.to_string()))?;
    config.alpn_protocols = vec![alpn.to_vec()];
    config.enable_early_data = false;
    Ok(Arc::new(config))
}

#[must_use]
pub fn encode_bind_request(binding: &DisplayTransportBinding) -> [u8; DISPLAY_BIND_BYTES] {
    encode_bind_message(binding, BIND_REQUEST)
}

pub fn verify_bind_ack(
    bytes: &[u8; DISPLAY_BIND_BYTES],
    expected: &DisplayTransportBinding,
) -> Result<(), DisplayTlsError> {
    verify_bind_message(bytes, expected, BIND_ACK)
}

fn verify_bind_message(
    bytes: &[u8; DISPLAY_BIND_BYTES],
    expected: &DisplayTransportBinding,
    kind: u8,
) -> Result<(), DisplayTlsError> {
    if bytes == &encode_bind_message(expected, kind) {
        Ok(())
    } else {
        Err(DisplayTlsError::UnauthorizedBinding)
    }
}

fn encode_bind_message(binding: &DisplayTransportBinding, kind: u8) -> [u8; DISPLAY_BIND_BYTES] {
    let selected = binding.selected();
    let mut bytes = [0_u8; DISPLAY_BIND_BYTES];
    bytes[..8].copy_from_slice(&BIND_MAGIC);
    bytes[8] = BIND_VERSION;
    bytes[9] = kind;
    bytes[12..16].copy_from_slice(&BIND_BODY_BYTES.to_be_bytes());
    bytes[16..32].copy_from_slice(binding.session_id().as_uuid().as_bytes());
    bytes[32..48].copy_from_slice(binding.route_id().as_uuid().as_bytes());
    bytes[48..64].copy_from_slice(binding.stream_id().as_uuid().as_bytes());
    bytes[64..72].copy_from_slice(&binding.stream_epoch().to_be_bytes());
    bytes[72..76].copy_from_slice(&selected.decoded.width.to_be_bytes());
    bytes[76..80].copy_from_slice(&selected.decoded.height.to_be_bytes());
    bytes[80..84].copy_from_slice(&selected.decoded.frame_rate.numerator().to_be_bytes());
    bytes[84..88].copy_from_slice(&selected.decoded.frame_rate.denominator().to_be_bytes());
    bytes[88..92].copy_from_slice(&selected.maximum_access_unit_bytes.to_be_bytes());
    bytes[92] = 1; // AVC Annex-B v1
    bytes[93] = encode_profile(selected.avc_profile);
    bytes[94] = encode_level(selected.avc_level);
    bytes
}

const fn encode_profile(profile: AvcProfile) -> u8 {
    match profile {
        AvcProfile::Baseline => 1,
        AvcProfile::Main => 2,
        AvcProfile::High => 3,
    }
}

const fn encode_level(level: AvcLevel) -> u8 {
    match level {
        AvcLevel::L3_1 => 1,
        AvcLevel::L4 => 2,
        AvcLevel::L4_1 => 3,
        AvcLevel::L4_2 => 4,
        AvcLevel::L5 => 5,
        AvcLevel::L5_1 => 6,
        AvcLevel::L5_2 => 7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use capyio_display_transport::{
        AvcDecoderConfig, EncodedDisplayStreamSpec, encode_decoder_config_record,
    };
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
        KeyUsagePurpose,
    };
    use std::thread;

    fn binding() -> DisplayTransportBinding {
        DisplayTransportBinding::new(
            id("00000000-0000-4000-8000-000000000001"),
            id("00000000-0000-4000-8000-000000000002"),
            id("00000000-0000-4000-8000-000000000003"),
            4,
            EncodedDisplayStreamSpec::fixture_720p30_avc(),
        )
        .unwrap()
    }

    fn id<T: std::str::FromStr>(value: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        value.parse().unwrap()
    }

    #[test]
    fn bind_request_is_fixed_and_exact() {
        let expected = binding();
        let request = encode_bind_request(&expected);
        assert_eq!(request.len(), DISPLAY_BIND_BYTES);
        assert_eq!(&request[..8], &BIND_MAGIC);
        assert_eq!(request[9], BIND_REQUEST);
        verify_bind_message(&request, &expected, BIND_REQUEST).unwrap();
    }

    #[test]
    fn bind_rejects_cross_route_epoch_and_kind() {
        let expected = binding();
        let other = DisplayTransportBinding::new(
            expected.session_id(),
            id("00000000-0000-4000-8000-000000000009"),
            expected.stream_id(),
            expected.stream_epoch(),
            expected.selected().clone(),
        )
        .unwrap();
        let request = encode_bind_request(&other);
        assert!(matches!(
            verify_bind_message(&request, &expected, BIND_REQUEST),
            Err(DisplayTlsError::UnauthorizedBinding)
        ));
        assert!(verify_bind_ack(&request, &expected).is_err());
    }

    #[test]
    fn endpoint_rejects_unspecified_zero_and_unbounded_timeout() {
        for endpoint in [
            DisplayTlsServerEndpoint {
                address: Ipv4Addr::UNSPECIFIED,
                port: 7443,
                io_timeout: Duration::from_secs(5),
            },
            DisplayTlsServerEndpoint {
                address: Ipv4Addr::LOCALHOST,
                port: 0,
                io_timeout: Duration::from_secs(5),
            },
            DisplayTlsServerEndpoint {
                address: Ipv4Addr::LOCALHOST,
                port: 7443,
                io_timeout: Duration::from_secs(31),
            },
        ] {
            assert!(endpoint.validate().is_err());
        }
    }

    #[test]
    fn nonblocking_listener_reports_no_pending_receiver_without_failure() {
        let (server_identity, _, _) = test_identities();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let server = AuthenticatedDisplayServer::from_listener(
            listener,
            server_identity,
            binding(),
            Duration::from_secs(5),
        )
        .unwrap();
        server.set_nonblocking(true).unwrap();
        assert!(server.try_accept().unwrap().is_none());
    }

    #[test]
    fn gamestream_launch_waits_for_mtls_and_exact_acknowledgement() {
        let (server_identity, client_identity, _) = test_identities();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = AuthenticatedGameStreamLaunchServer::from_listener(
            listener,
            server_identity,
            Duration::from_secs(5),
        )
        .unwrap();
        let record = [0x42; GAMESTREAM_LAUNCH_RECORD_BYTES];
        let acknowledgement = [0x24; GAMESTREAM_LAUNCH_ACK_BYTES];
        let server_thread = thread::spawn(move || {
            server.exchange(&record, &acknowledgement).unwrap();
        });

        let config = build_client_config(client_identity, GAMESTREAM_LAUNCH_TLS_ALPN).unwrap();
        let mut client = connect_client(address, config);
        let mut received = [0_u8; GAMESTREAM_LAUNCH_RECORD_BYTES];
        client.read_exact(&mut received).unwrap();
        assert_eq!(received, record);
        assert_eq!(
            client.conn.protocol_version(),
            Some(rustls::ProtocolVersion::TLSv1_3)
        );
        assert_eq!(
            client.conn.alpn_protocol(),
            Some(GAMESTREAM_LAUNCH_TLS_ALPN)
        );
        client.write_all(&acknowledgement).unwrap();
        client.flush().unwrap();
        server_thread.join().unwrap();
    }

    #[test]
    fn gamestream_launch_rejects_changed_acknowledgement() {
        let (server_identity, client_identity, _) = test_identities();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = AuthenticatedGameStreamLaunchServer::from_listener(
            listener,
            server_identity,
            Duration::from_secs(5),
        )
        .unwrap();
        let record = [0x42; GAMESTREAM_LAUNCH_RECORD_BYTES];
        let acknowledgement = [0x24; GAMESTREAM_LAUNCH_ACK_BYTES];
        let server_thread = thread::spawn(move || {
            assert!(matches!(
                server.exchange(&record, &acknowledgement),
                Err(DisplayTlsError::InvalidGameStreamLaunchAcknowledgement)
            ));
        });

        let config = build_client_config(client_identity, GAMESTREAM_LAUNCH_TLS_ALPN).unwrap();
        let mut client = connect_client(address, config);
        let mut received = [0_u8; GAMESTREAM_LAUNCH_RECORD_BYTES];
        client.read_exact(&mut received).unwrap();
        let mut changed = acknowledgement;
        changed[63] ^= 1;
        client.write_all(&changed).unwrap();
        client.flush().unwrap();
        server_thread.join().unwrap();
    }

    #[test]
    fn mutual_tls13_binds_route_and_delivers_one_bounded_record() {
        let (server_identity, _, client_config) = test_identities();
        let expected = binding();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server_binding = expected.clone();
        let record = encode_decoder_config_record(&AvcDecoderConfig {
            stream_id: expected.stream_id(),
            stream_epoch: expected.stream_epoch(),
            selected: expected.selected().clone(),
            sequence_parameter_set: vec![0, 0, 0, 1, 0x67, 1],
            picture_parameter_set: vec![0, 0, 0, 1, 0x68, 2],
        })
        .unwrap();
        let sent_record = record.clone();
        let server = thread::spawn(move || {
            let mut sender = AuthenticatedDisplaySender::accept_from_listener(
                listener,
                server_identity,
                server_binding,
                Duration::from_secs(5),
            )
            .unwrap();
            assert_eq!(
                sender.stream.conn.protocol_version(),
                Some(rustls::ProtocolVersion::TLSv1_3)
            );
            sender.send_record(&sent_record).unwrap();
        });

        let socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let connection = ClientConnection::new(
            client_config,
            ServerName::try_from("localhost").unwrap().to_owned(),
        )
        .unwrap();
        let mut client = StreamOwned::new(connection, socket);
        client.write_all(&encode_bind_request(&expected)).unwrap();
        client.flush().unwrap();
        let mut ack = [0_u8; DISPLAY_BIND_BYTES];
        client.read_exact(&mut ack).unwrap();
        verify_bind_ack(&ack, &expected).unwrap();
        assert_eq!(
            client.conn.protocol_version(),
            Some(rustls::ProtocolVersion::TLSv1_3)
        );
        assert_eq!(client.conn.alpn_protocol(), Some(DISPLAY_TLS_ALPN));
        let mut length = [0_u8; 4];
        client.read_exact(&mut length).unwrap();
        let mut received = vec![0; u32::from_be_bytes(length) as usize];
        client.read_exact(&mut received).unwrap();
        assert_eq!(received, record);
        server.join().unwrap();
    }

    #[test]
    fn heartbeat_is_zero_length_outer_frame_with_exact_acknowledgement() {
        let (server_identity, _, client_config) = test_identities();
        let expected = binding();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server_binding = expected.clone();
        let server = thread::spawn(move || {
            let mut sender = AuthenticatedDisplaySender::accept_from_listener(
                listener,
                server_identity,
                server_binding,
                Duration::from_secs(5),
            )
            .unwrap();
            sender.round_trip_heartbeat().unwrap();
        });

        let mut client = connect_client(address, client_config);
        client.write_all(&encode_bind_request(&expected)).unwrap();
        client.flush().unwrap();
        let mut acknowledgement = [0_u8; DISPLAY_BIND_BYTES];
        client.read_exact(&mut acknowledgement).unwrap();
        verify_bind_ack(&acknowledgement, &expected).unwrap();
        let mut heartbeat = [1_u8; 4];
        client.read_exact(&mut heartbeat).unwrap();
        assert_eq!(heartbeat, 0_u32.to_be_bytes());
        client.write_all(&heartbeat).unwrap();
        client.flush().unwrap();
        server.join().unwrap();
    }

    #[test]
    fn heartbeat_rejects_nonzero_acknowledgement() {
        let (server_identity, _, client_config) = test_identities();
        let expected = binding();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server_binding = expected.clone();
        let server = thread::spawn(move || {
            let mut sender = AuthenticatedDisplaySender::accept_from_listener(
                listener,
                server_identity,
                server_binding,
                Duration::from_secs(5),
            )
            .unwrap();
            assert!(matches!(
                sender.round_trip_heartbeat(),
                Err(DisplayTlsError::InvalidHeartbeatAcknowledgement)
            ));
        });

        let mut client = connect_client(address, client_config);
        client.write_all(&encode_bind_request(&expected)).unwrap();
        client.flush().unwrap();
        let mut acknowledgement = [0_u8; DISPLAY_BIND_BYTES];
        client.read_exact(&mut acknowledgement).unwrap();
        verify_bind_ack(&acknowledgement, &expected).unwrap();
        let mut heartbeat = [1_u8; 4];
        client.read_exact(&mut heartbeat).unwrap();
        client.write_all(&1_u32.to_be_bytes()).unwrap();
        client.flush().unwrap();
        server.join().unwrap();
    }

    #[test]
    fn authenticated_receiver_validates_records_and_echoes_heartbeats() {
        let (server_identity, client_identity, _) = test_identities();
        let expected = binding();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server_binding = expected.clone();
        let record = encode_decoder_config_record(&AvcDecoderConfig {
            stream_id: expected.stream_id(),
            stream_epoch: expected.stream_epoch(),
            selected: expected.selected().clone(),
            sequence_parameter_set: vec![0, 0, 0, 1, 0x67, 1],
            picture_parameter_set: vec![0, 0, 0, 1, 0x68, 2],
        })
        .unwrap();
        let sent_record = record.clone();
        let server = thread::spawn(move || {
            let mut sender = AuthenticatedDisplaySender::accept_from_listener(
                listener,
                server_identity,
                server_binding,
                Duration::from_secs(5),
            )
            .unwrap();
            sender.send_record(&sent_record).unwrap();
            sender.round_trip_heartbeat().unwrap();
        });

        let mut receiver = AuthenticatedDisplayReceiver::connect(
            DisplayTlsServerEndpoint {
                address: Ipv4Addr::LOCALHOST,
                port: address.port(),
                io_timeout: Duration::from_secs(5),
            },
            "localhost",
            client_identity,
            expected,
        )
        .unwrap();
        assert_eq!(
            receiver.receive().unwrap(),
            AuthenticatedDisplayItem::Record(record)
        );
        assert_eq!(
            receiver.receive().unwrap(),
            AuthenticatedDisplayItem::Heartbeat
        );
        server.join().unwrap();
    }

    #[test]
    fn long_lived_server_rejects_one_route_then_accepts_two_sequential_receivers() {
        let (server_identity, _, client_config) = test_identities();
        let expected = binding();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let server = AuthenticatedDisplayServer::from_listener(
            listener,
            server_identity,
            expected.clone(),
            Duration::from_secs(5),
        )
        .unwrap();
        let address = server.local_addr().unwrap();
        let record = encode_decoder_config_record(&AvcDecoderConfig {
            stream_id: expected.stream_id(),
            stream_epoch: expected.stream_epoch(),
            selected: expected.selected().clone(),
            sequence_parameter_set: vec![0, 0, 0, 1, 0x67, 1],
            picture_parameter_set: vec![0, 0, 0, 1, 0x68, 2],
        })
        .unwrap();
        let sent_record = record.clone();
        let server_thread = thread::spawn(move || {
            assert!(matches!(
                server.accept(),
                Err(DisplayTlsError::UnauthorizedBinding)
            ));
            for _ in 0..2 {
                let mut sender = server.accept().unwrap();
                sender.send_record(&sent_record).unwrap();
            }
        });

        let unauthorized = DisplayTransportBinding::new(
            expected.session_id(),
            id("00000000-0000-4000-8000-000000000099"),
            expected.stream_id(),
            expected.stream_epoch(),
            expected.selected().clone(),
        )
        .unwrap();
        let mut rejected = connect_client(address, client_config.clone());
        rejected
            .write_all(&encode_bind_request(&unauthorized))
            .unwrap();
        rejected.flush().unwrap();
        drop(rejected);

        for _ in 0..2 {
            let mut client = connect_client(address, client_config.clone());
            client.write_all(&encode_bind_request(&expected)).unwrap();
            client.flush().unwrap();
            let mut acknowledgement = [0_u8; DISPLAY_BIND_BYTES];
            client.read_exact(&mut acknowledgement).unwrap();
            verify_bind_ack(&acknowledgement, &expected).unwrap();
            let mut length = [0_u8; 4];
            client.read_exact(&mut length).unwrap();
            let mut received = vec![0; u32::from_be_bytes(length) as usize];
            client.read_exact(&mut received).unwrap();
            assert_eq!(received, record);
        }
        server_thread.join().unwrap();
    }

    fn connect_client(
        address: SocketAddr,
        config: Arc<ClientConfig>,
    ) -> StreamOwned<ClientConnection, TcpStream> {
        let socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let connection = ClientConnection::new(
            config,
            ServerName::try_from("localhost").unwrap().to_owned(),
        )
        .unwrap();
        StreamOwned::new(connection, socket)
    }

    fn test_identities() -> (
        DisplayTlsServerIdentity,
        DisplayTlsClientIdentity,
        Arc<ClientConfig>,
    ) {
        let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
        let ca_key = KeyPair::generate().unwrap();
        let ca_cert = ca_params.self_signed(&ca_key).unwrap();
        let ca_der = ca_cert.der().to_vec();
        let issuer = Issuer::new(ca_params, ca_key);

        let (server_cert, server_key) =
            leaf(&issuer, "localhost", ExtendedKeyUsagePurpose::ServerAuth);
        let (client_cert, client_key) = leaf(
            &issuer,
            "capyio-android-test",
            ExtendedKeyUsagePurpose::ClientAuth,
        );
        let server_identity = DisplayTlsServerIdentity {
            certificate_chain_der: vec![server_cert, ca_der.clone()],
            private_key_der: server_key,
            client_ca_der: ca_der.clone(),
        };

        let client_identity = DisplayTlsClientIdentity {
            certificate_chain_der: vec![client_cert.clone(), ca_der.clone()],
            private_key_der: client_key.clone(),
            server_ca_der: ca_der.clone(),
        };

        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut roots = RootCertStore::empty();
        roots.add(CertificateDer::from(ca_der.clone())).unwrap();
        let mut config = ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&TLS13])
            .unwrap()
            .with_root_certificates(roots)
            .with_client_auth_cert(
                vec![
                    CertificateDer::from(client_cert),
                    CertificateDer::from(ca_der),
                ],
                PrivateKeyDer::try_from(client_key).unwrap(),
            )
            .unwrap();
        config.alpn_protocols = vec![DISPLAY_TLS_ALPN.to_vec()];
        (server_identity, client_identity, Arc::new(config))
    }

    fn leaf(
        issuer: &Issuer<'static, KeyPair>,
        name: &str,
        usage: ExtendedKeyUsagePurpose,
    ) -> (Vec<u8>, Vec<u8>) {
        let mut params = CertificateParams::new(vec![name.to_owned()]).unwrap();
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![usage];
        let key = KeyPair::generate().unwrap();
        let cert = params.signed_by(&key, issuer).unwrap();
        (cert.der().to_vec(), key.serialize_der())
    }
}
