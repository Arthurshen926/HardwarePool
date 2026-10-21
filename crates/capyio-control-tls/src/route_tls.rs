// Shared pinned TLS 1.3 Route transport. Profile-specific modules supply distinct
// ALPN, binding magic and Profile; existing touchpad wire behavior is unchanged.
macro_rules! route_tls {
    ($binding:ident, $listener:ident, $channel:ident, $stop:ident,
     $alpn_name:ident, $max_name:ident, $alpn:expr, $magic:expr, $profile:expr) => {
        route_tls!(
            $binding, $listener, $channel, $stop, $alpn_name, $max_name, $alpn, $magic, $profile,
            24, 176, u16, false
        );
    };
    ($binding:ident, $listener:ident, $channel:ident, $stop:ident,
     $alpn_name:ident, $max_name:ident, $alpn:expr, $magic:expr, $profile:expr,
     $min:expr, $max:expr, $length:ty, $one_shot:expr) => {
        // Separate private Route data stream, never a control Envelope carrier.
        // A binding is lifecycle metadata, not authority: hosts must derive it from
        // current Runtime state and enforce local leases before opening any Sink.

        use std::{
            sync::atomic::{AtomicBool, Ordering},
            time::{Instant, SystemTime, UNIX_EPOCH},
        };

        use capyio_core::{
            AuthorizationState, PortRef, Route, RouteBackend, RouteId, RouteState, StreamId,
        };

        use super::*;

        pub const $alpn_name: &[u8] = $alpn;
        pub const $max_name: usize = $max;
        const BIND_BYTES: usize = 208;

        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $binding {
            pub session: ControlTlsBinding,
            pub route_id: RouteId,
            pub source: PortRef,
            pub sink: PortRef,
            pub stream_id: StreamId,
            pub epoch: u64,
            pub expires_at_ms: u64,
        }

        impl $binding {
            pub fn from_route(
                session: ControlTlsBinding,
                route: &Route,
                stream_id: StreamId,
                now_ms: u64,
            ) -> Result<Self, ControlTlsError> {
                let AuthorizationState::Authorized {
                    expires_at_ms: Some(expires_at_ms),
                } = route.authorization
                else {
                    return Err(ControlTlsError::UnauthorizedBinding);
                };
                if route.session_id != session.session_id
                    || route.backend != RouteBackend::AdapterManaged
                    || route.profile.name != $profile
                    || route.profile.major != 1
                    || !matches!(route.state, RouteState::Starting | RouteState::Active)
                {
                    return Err(ControlTlsError::UnauthorizedBinding);
                }
                let value = Self {
                    session,
                    route_id: route.id,
                    source: route.source,
                    sink: route.sink,
                    stream_id,
                    epoch: route.epoch,
                    expires_at_ms,
                };
                value.validate(now_ms)?;
                Ok(value)
            }

            fn validate(self, now_ms: u64) -> Result<(), ControlTlsError> {
                ControlTlsBinding::new(
                    self.session.session_id,
                    self.session.server_node_id,
                    self.session.client_node_id,
                )?;
                if self.route_id.as_uuid().is_nil()
                    || self.stream_id.as_uuid().is_nil()
                    || self.epoch == 0
                    || self.expires_at_ms <= now_ms
                    || self.source.node_id == self.sink.node_id
                    || [self.source, self.sink].iter().any(|port| {
                        port.capability_id.as_uuid().is_nil()
                            || port.port_id.as_uuid().is_nil()
                            || (port.node_id != self.session.server_node_id
                                && port.node_id != self.session.client_node_id)
                    })
                {
                    return Err(ControlTlsError::UnauthorizedBinding);
                }
                Ok(())
            }

            fn encode(self, acknowledgement: bool) -> [u8; BIND_BYTES] {
                let mut bytes = [0; BIND_BYTES];
                bytes[..8].copy_from_slice($magic);
                bytes[8] = u8::from(acknowledgement);
                let ids = [
                    self.session.session_id.as_uuid(),
                    self.session.server_node_id.as_uuid(),
                    self.session.client_node_id.as_uuid(),
                    self.route_id.as_uuid(),
                    self.stream_id.as_uuid(),
                    self.source.node_id.as_uuid(),
                    self.source.capability_id.as_uuid(),
                    self.source.port_id.as_uuid(),
                    self.sink.node_id.as_uuid(),
                    self.sink.capability_id.as_uuid(),
                    self.sink.port_id.as_uuid(),
                ];
                for (index, id) in ids.iter().enumerate() {
                    bytes[16 + index * 16..32 + index * 16].copy_from_slice(id.as_bytes());
                }
                bytes[192..200].copy_from_slice(&self.epoch.to_be_bytes());
                bytes[200..208].copy_from_slice(&self.expires_at_ms.to_be_bytes());
                bytes
            }
        }

        fn now_ms() -> Result<u64, ControlTlsError> {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|time| time.as_millis() as u64)
                .map_err(|_| ControlTlsError::UnauthorizedBinding)
        }

        fn grant_deadline(expires_at_ms: u64) -> Result<Instant, ControlTlsError> {
            let remaining = expires_at_ms
                .checked_sub(now_ms()?)
                .filter(|remaining| *remaining > 0)
                .ok_or(ControlTlsError::UnauthorizedBinding)?;
            Instant::now()
                .checked_add(Duration::from_millis(remaining))
                .ok_or(ControlTlsError::UnauthorizedBinding)
        }

        fn require_protocol(
            version: Option<rustls::ProtocolVersion>,
            alpn: Option<&[u8]>,
        ) -> Result<(), ControlTlsError> {
            if version == Some(rustls::ProtocolVersion::TLSv1_3) && alpn == Some($alpn_name) {
                Ok(())
            } else {
                Err(ControlTlsError::WrongApplicationProtocol)
            }
        }

        /// A single Route listener. The platform polls accept and drops it on Stop,
        /// revocation, control disconnect or lease loss; no accept thread is hidden here.
        pub struct $listener {
            listener: Mutex<Option<TcpListener>>,
            address: SocketAddr,
            stopped: Arc<AtomicBool>,
            config: Arc<ServerConfig>,
            pin: CertificateFingerprint,
            binding: $binding,
            timeout: Duration,
            local_deadline: Instant,
        }

        impl $listener {
            pub fn from_listener(
                listener: TcpListener,
                identity: ControlTlsServerIdentity,
                binding: $binding,
                timeout: Duration,
            ) -> Result<Self, ControlTlsError> {
                Self::from_listener_until(
                    listener,
                    identity,
                    binding,
                    timeout,
                    grant_deadline(binding.expires_at_ms)?,
                )
            }

            /// Cap all local transport work without changing the peer-visible grant.
            pub fn from_listener_until(
                listener: TcpListener,
                identity: ControlTlsServerIdentity,
                binding: $binding,
                timeout: Duration,
                local_deadline: Instant,
            ) -> Result<Self, ControlTlsError> {
                validate_timeout(timeout)?;
                binding.validate(now_ms()?)?;
                if local_deadline <= Instant::now() {
                    return Err(ControlTlsError::UnauthorizedBinding);
                }
                let pin = identity.expected_peer_certificate;
                let mut config = build_server_config(identity)?;
                Arc::make_mut(&mut config).alpn_protocols = vec![$alpn_name.to_vec()];
                listener.set_nonblocking(true)?;
                let address = listener.local_addr()?;
                Ok(Self {
                    listener: Mutex::new(Some(listener)),
                    address,
                    stopped: Arc::new(AtomicBool::new(false)),
                    config,
                    pin,
                    binding,
                    timeout,
                    local_deadline,
                })
            }

            pub fn local_addr(&self) -> Result<SocketAddr, ControlTlsError> {
                Ok(self.address)
            }

            /// Obtain cancellation authority before an accepted TLS handshake starts.
            pub fn stop_handle(&self) -> Result<$stop, ControlTlsError> {
                Ok($stop {
                    socket: None,
                    stopped: self.stopped.clone(),
                })
            }

            pub fn try_accept(&self) -> Result<Option<$channel>, ControlTlsError> {
                self.binding.validate(now_ms()?)?;
                if self.stopped.load(Ordering::Acquire) || Instant::now() >= self.local_deadline {
                    return Err(ControlTlsError::ChannelFailed);
                }
                let mut listener = self
                    .listener
                    .lock()
                    .map_err(|_| ControlTlsError::ChannelFailed)?;
                let Some(active) = listener.as_ref() else {
                    return Err(ControlTlsError::ChannelFailed);
                };
                let (socket, _) = match active.accept() {
                    Ok(accepted) => accepted,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        return Ok(None);
                    }
                    Err(error) => return Err(error.into()),
                };
                let socket = BoundedSocket::new(
                    socket,
                    self.timeout,
                    grant_deadline(self.binding.expires_at_ms)?.min(self.local_deadline),
                    self.stopped.clone(),
                )?;
                let connection = ServerConnection::new(self.config.clone())?;
                let mut stream = StreamOwned::new(connection, socket);
                while stream.conn.is_handshaking() {
                    stream.conn.complete_io(&mut stream.sock)?;
                }
                require_protocol(stream.conn.protocol_version(), stream.conn.alpn_protocol())?;
                verify_peer_certificate(stream.conn.peer_certificates(), self.pin)?;
                let mut bytes = [0; BIND_BYTES];
                stream.read_exact(&mut bytes)?;
                if bytes != self.binding.encode(false) {
                    return Err(ControlTlsError::UnauthorizedBinding);
                }
                self.binding.validate(now_ms()?)?;
                stream.write_all(&self.binding.encode(true))?;
                stream.flush()?;
                stream.sock.check()?;
                self.binding.validate(now_ms()?)?;
                // Camera admits exactly one connection; legacy listeners remain reusable.
                if $one_shot {
                    drop(listener.take());
                }
                Ok(Some($channel {
                    io: DataIo::Accepted(stream),
                    binding: self.binding,
                    failed: false,
                }))
            }
        }

        enum DataIo {
            Accepted(StreamOwned<ServerConnection, BoundedSocket>),
            Dialed(StreamOwned<ClientConnection, BoundedSocket>),
        }

        impl DataIo {
            fn socket(&self) -> &BoundedSocket {
                match self {
                    Self::Accepted(stream) => &stream.sock,
                    Self::Dialed(stream) => &stream.sock,
                }
            }
            fn begin_record(&mut self) {
                let socket = match self {
                    Self::Accepted(stream) => &mut stream.sock,
                    Self::Dialed(stream) => &mut stream.sock,
                };
                socket.deadline = (Instant::now() + socket.timeout).min(socket.grant_deadline);
            }
        }
        impl Read for DataIo {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                match self {
                    Self::Accepted(stream) => stream.read(bytes),
                    Self::Dialed(stream) => stream.read(bytes),
                }
            }
        }
        impl Write for DataIo {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                match self {
                    Self::Accepted(stream) => stream.write(bytes),
                    Self::Dialed(stream) => stream.write(bytes),
                }
            }
            fn flush(&mut self) -> std::io::Result<()> {
                match self {
                    Self::Accepted(stream) => stream.flush(),
                    Self::Dialed(stream) => stream.flush(),
                }
            }
        }

        /// Nonblocking syscalls make cancellation portable, including Windows where
        /// shutdown on a cloned socket need not interrupt an already-blocked recv.
        /// One absolute deadline covers all TLS fragments of a record/handshake.
        struct BoundedSocket {
            socket: TcpStream,
            stopped: Arc<AtomicBool>,
            listener_stopped: Arc<AtomicBool>,
            timeout: Duration,
            deadline: Instant,
            grant_deadline: Instant,
        }
        impl BoundedSocket {
            fn new(
                socket: TcpStream,
                timeout: Duration,
                grant_deadline: Instant,
                stopped: Arc<AtomicBool>,
            ) -> std::io::Result<Self> {
                socket.set_nodelay(true)?;
                socket.set_nonblocking(true)?;
                Ok(Self {
                    socket,
                    stopped: Arc::new(AtomicBool::new(false)),
                    listener_stopped: stopped,
                    timeout,
                    deadline: (Instant::now() + timeout).min(grant_deadline),
                    grant_deadline,
                })
            }
            fn check(&self) -> std::io::Result<()> {
                if self.stopped.load(Ordering::Acquire)
                    || self.listener_stopped.load(Ordering::Acquire)
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionAborted,
                        "Route data stopped",
                    ));
                }
                if Instant::now() >= self.deadline {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "Route data record deadline",
                    ));
                }
                Ok(())
            }
            fn shutdown(&self) {
                self.stopped.store(true, Ordering::Release);
                let _ = self.socket.shutdown(Shutdown::Both);
            }
        }
        impl Read for BoundedSocket {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                loop {
                    self.check()?;
                    match self.socket.read(bytes) {
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(2))
                        }
                        result => return result,
                    }
                }
            }
        }
        impl Write for BoundedSocket {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                loop {
                    self.check()?;
                    match self.socket.write(bytes) {
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(2))
                        }
                        result => return result,
                    }
                }
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.check()
            }
        }

        /// Only shutdown authority crosses to the lifecycle owner, never plaintext I/O.
        pub struct $stop {
            socket: Option<TcpStream>,
            stopped: Arc<AtomicBool>,
        }
        impl $stop {
            pub fn stop(&self) {
                self.stopped.store(true, Ordering::Release);
                if let Some(socket) = &self.socket {
                    let _ = socket.shutdown(Shutdown::Both);
                }
            }
        }

        pub struct $channel {
            io: DataIo,
            binding: $binding,
            failed: bool,
        }

        impl $channel {
            pub fn connect(
                endpoint: ControlTlsEndpoint,
                server_name: &str,
                identity: ControlTlsClientIdentity,
                binding: $binding,
            ) -> Result<Self, ControlTlsError> {
                Self::connect_until(
                    endpoint,
                    server_name,
                    identity,
                    binding,
                    grant_deadline(binding.expires_at_ms)?,
                )
            }

            /// A local Source lease/lifetime can be shorter than the immutable grant.
            pub fn connect_until(
                endpoint: ControlTlsEndpoint,
                server_name: &str,
                identity: ControlTlsClientIdentity,
                binding: $binding,
                local_deadline: Instant,
            ) -> Result<Self, ControlTlsError> {
                endpoint.validate()?;
                binding.validate(now_ms()?)?;
                if server_name.is_empty() || server_name.len() > 253 {
                    return Err(ControlTlsError::InvalidEndpoint);
                }
                let name = ServerName::try_from(server_name.to_owned())
                    .map_err(|_| ControlTlsError::InvalidEndpoint)?;
                let pin = identity.expected_peer_certificate;
                let mut config = build_client_config(identity)?;
                Arc::make_mut(&mut config).alpn_protocols = vec![$alpn_name.to_vec()];
                let grant_deadline = grant_deadline(binding.expires_at_ms)?.min(local_deadline);
                let remaining = grant_deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(ControlTlsError::UnauthorizedBinding);
                }
                let socket = TcpStream::connect_timeout(
                    &endpoint.address,
                    endpoint.io_timeout.min(remaining),
                )?;
                let socket = BoundedSocket::new(
                    socket,
                    endpoint.io_timeout,
                    grant_deadline,
                    Arc::new(AtomicBool::new(false)),
                )?;
                let mut stream = StreamOwned::new(ClientConnection::new(config, name)?, socket);
                while stream.conn.is_handshaking() {
                    stream.conn.complete_io(&mut stream.sock)?;
                }
                require_protocol(stream.conn.protocol_version(), stream.conn.alpn_protocol())?;
                verify_peer_certificate(stream.conn.peer_certificates(), pin)?;
                stream.write_all(&binding.encode(false))?;
                stream.flush()?;
                let mut bytes = [0; BIND_BYTES];
                stream.read_exact(&mut bytes)?;
                if bytes != binding.encode(true) {
                    return Err(ControlTlsError::UnauthorizedBinding);
                }
                binding.validate(now_ms()?)?;
                stream.sock.check()?;
                Ok(Self {
                    io: DataIo::Dialed(stream),
                    binding,
                    failed: false,
                })
            }

            /// Monotonically shorten local authority; never renew a connection.
            pub fn restrict_deadline(&mut self, deadline: Instant) {
                let socket = match &mut self.io {
                    DataIo::Accepted(stream) => &mut stream.sock,
                    DataIo::Dialed(stream) => &mut stream.sock,
                };
                socket.grant_deadline = socket.grant_deadline.min(deadline);
                socket.deadline = socket.deadline.min(socket.grant_deadline);
                if Instant::now() >= socket.grant_deadline {
                    self.close();
                }
            }

            pub const fn binding(&self) -> $binding {
                self.binding
            }

            pub fn stop_handle(&self) -> Result<$stop, ControlTlsError> {
                Ok($stop {
                    socket: Some(self.io.socket().socket.try_clone()?),
                    stopped: self.io.socket().stopped.clone(),
                })
            }

            fn preflight(&mut self) -> Result<(), ControlTlsError> {
                if self.failed
                    || self.io.socket().stopped.load(Ordering::Acquire)
                    || self.io.socket().listener_stopped.load(Ordering::Acquire)
                {
                    return Err(ControlTlsError::ChannelFailed);
                }
                if Instant::now() >= self.io.socket().grant_deadline {
                    self.close();
                    return Err(ControlTlsError::UnauthorizedBinding);
                }
                if let Err(error) = now_ms().and_then(|now| self.binding.validate(now)) {
                    self.close();
                    return Err(error);
                }
                Ok(())
            }

            pub fn send_record(&mut self, bytes: &[u8]) -> Result<(), ControlTlsError> {
                self.preflight()?;
                if !($min..=$max_name).contains(&bytes.len()) {
                    return Err(ControlTlsError::RecordTooLarge);
                }
                self.io.begin_record();
                let result = (|| {
                    self.io.write_all(&(bytes.len() as $length).to_be_bytes())?;
                    self.io.write_all(bytes)?;
                    self.io.flush()
                })();
                if result.is_err() {
                    self.close();
                }
                result.map_err(ControlTlsError::Io)?;
                self.preflight()
            }

            pub fn receive_record(&mut self) -> Result<Vec<u8>, ControlTlsError> {
                self.preflight()?;
                self.io.begin_record();
                let result = read_record(&mut self.io);
                if result.is_err() {
                    self.close();
                }
                let bytes = result?;
                self.preflight()?;
                Ok(bytes)
            }

            pub fn close(&mut self) {
                self.failed = true;
                self.io.socket().shutdown();
            }
        }

        impl Drop for $channel {
            fn drop(&mut self) {
                self.close();
            }
        }

        fn read_record(io: &mut impl Read) -> Result<Vec<u8>, ControlTlsError> {
            let mut length = [0; std::mem::size_of::<$length>()];
            io.read_exact(&mut length)?;
            let length = <$length>::from_be_bytes(length) as usize;
            if !($min..=$max_name).contains(&length) {
                return Err(ControlTlsError::RecordTooLarge);
            }
            let mut bytes = vec![0; length];
            io.read_exact(&mut bytes)?;
            Ok(bytes)
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use capyio_core::{CapabilityId, PortId};
            use std::{io::Cursor, thread, time::Instant};

            fn binding() -> $binding {
                let session =
                    ControlTlsBinding::new(SessionId::new(), NodeId::new(), NodeId::new()).unwrap();
                $binding {
                    session,
                    route_id: RouteId::new(),
                    source: PortRef {
                        node_id: session.server_node_id,
                        capability_id: CapabilityId::new(),
                        port_id: PortId::new(),
                    },
                    sink: PortRef {
                        node_id: session.client_node_id,
                        capability_id: CapabilityId::new(),
                        port_id: PortId::new(),
                    },
                    stream_id: StreamId::new(),
                    epoch: 1,
                    expires_at_ms: now_ms().unwrap() + 60_000,
                }
            }

            fn server(identity: ControlTlsServerIdentity, binding: $binding) -> $listener {
                $listener::from_listener(
                    TcpListener::bind("127.0.0.1:0").unwrap(),
                    identity,
                    binding,
                    Duration::from_secs(2),
                )
                .unwrap()
            }

            fn accept(server: $listener) -> Result<$channel, ControlTlsError> {
                let deadline = Instant::now() + Duration::from_secs(3);
                while Instant::now() < deadline {
                    if let Some(channel) = server.try_accept()? {
                        return Ok(channel);
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                panic!("test accept deadline exceeded")
            }

            fn connect(
                address: SocketAddr,
                identity: ControlTlsClientIdentity,
                binding: $binding,
            ) -> Result<$channel, ControlTlsError> {
                $channel::connect(
                    ControlTlsEndpoint {
                        address,
                        io_timeout: Duration::from_secs(2),
                    },
                    "localhost",
                    identity,
                    binding,
                )
            }

            #[test]
            fn dedicated_tls_roundtrip_and_stop_interrupts_blocked_read() {
                let binding = binding();
                let (identity, client) = crate::tests::test_identities();
                let server = server(identity, binding);
                let address = server.local_addr().unwrap();
                let worker = thread::spawn(move || {
                    let mut channel = accept(server).unwrap();
                    assert_eq!(channel.receive_record().unwrap(), vec![7; $max_name]);
                    channel.send_record(&[8; $min]).unwrap();
                    let stop = channel.stop_handle().unwrap();
                    let stopper = thread::spawn(move || {
                        thread::sleep(Duration::from_millis(50));
                        stop.stop();
                    });
                    let before = Instant::now();
                    assert!(channel.receive_record().is_err());
                    assert!(before.elapsed() < Duration::from_secs(1));
                    assert!(matches!(
                        channel.receive_record(),
                        Err(ControlTlsError::ChannelFailed)
                    ));
                    stopper.join().unwrap();
                });
                let mut channel = connect(address, client, binding).unwrap();
                assert!(channel.send_record(&vec![0; $max_name + 1]).is_err()); // No wire mutation.
                channel.send_record(&vec![7; $max_name]).unwrap();
                assert_eq!(channel.receive_record().unwrap(), vec![8; $min]);
                worker.join().unwrap();
            }

            #[test]
            fn every_binding_field_is_exact_and_ack_is_directional() {
                let original = binding();
                let mut alternatives = Vec::new();
                macro_rules! changed {
                    ($field:ident, $value:expr) => {{
                        let mut value = original;
                        value.$field = $value;
                        alternatives.push(value);
                    }};
                }
                changed!(route_id, RouteId::new());
                changed!(stream_id, StreamId::new());
                changed!(epoch, 2);
                changed!(expires_at_ms, original.expires_at_ms + 1);
                let mut value = original;
                value.source.port_id = PortId::new();
                alternatives.push(value);
                let mut value = original;
                value.sink.capability_id = CapabilityId::new();
                alternatives.push(value);
                let mut value = original;
                value.session.session_id = SessionId::new();
                alternatives.push(value);
                for source in [true, false] {
                    for field in 0..3 {
                        let mut value = original;
                        let port = if source {
                            &mut value.source
                        } else {
                            &mut value.sink
                        };
                        match field {
                            0 => port.node_id = NodeId::new(),
                            1 => port.capability_id = CapabilityId::new(),
                            _ => port.port_id = PortId::new(),
                        }
                        alternatives.push(value);
                    }
                }
                let mut value = original;
                value.session.server_node_id = NodeId::new();
                alternatives.push(value);
                let mut value = original;
                value.session.client_node_id = NodeId::new();
                alternatives.push(value);
                for value in alternatives {
                    assert_ne!(original.encode(false), value.encode(false));
                }
                assert_ne!(original.encode(false), original.encode(true));
                assert!(
                    $binding {
                        expires_at_ms: 1,
                        ..original
                    }
                    .validate(1)
                    .is_err()
                );
                assert!(
                    $binding {
                        epoch: 0,
                        ..original
                    }
                    .validate(0)
                    .is_err()
                );
            }

            #[test]
            fn valid_certificate_cannot_substitute_a_different_route_or_epoch() {
                for change_epoch in [false, true] {
                    let expected = binding();
                    let mut wrong = expected;
                    if change_epoch {
                        wrong.epoch += 1;
                    } else {
                        wrong.route_id = RouteId::new();
                    }
                    let (identity, client) = crate::tests::test_identities();
                    let server = server(identity, expected);
                    let address = server.local_addr().unwrap();
                    let worker = thread::spawn(move || accept(server));
                    assert!(connect(address, client, wrong).is_err());
                    assert!(matches!(
                        worker.join().unwrap(),
                        Err(ControlTlsError::UnauthorizedBinding)
                    ));
                }
            }

            #[test]
            fn certificate_pins_are_checked_on_both_data_endpoints() {
                for wrong_server_pin in [true, false] {
                    let expected = binding();
                    let (mut identity, mut client) = crate::tests::test_identities();
                    let (other, other_client) = crate::tests::test_identities();
                    if wrong_server_pin {
                        client.expected_peer_certificate = other_client.expected_peer_certificate;
                    } else {
                        identity.expected_peer_certificate = other.expected_peer_certificate;
                    }
                    let server = server(identity, expected);
                    let address = server.local_addr().unwrap();
                    let worker = thread::spawn(move || accept(server));
                    assert!(connect(address, client, expected).is_err());
                    assert!(worker.join().unwrap().is_err());
                }
            }

            #[test]
            fn control_and_route_alpn_cannot_be_confused() {
                let expected = binding();
                let (identity, client) = crate::tests::test_identities();
                let server = server(identity, expected);
                let address = server.local_addr().unwrap();
                let worker = thread::spawn(move || accept(server));
                assert!(
                    AuthenticatedControlClientChannel::connect(
                        ControlTlsEndpoint {
                            address,
                            io_timeout: Duration::from_secs(2)
                        },
                        "localhost",
                        client,
                        expected.session
                    )
                    .is_err()
                );
                assert!(worker.join().unwrap().is_err());
            }

            #[test]
            fn malformed_lengths_and_partial_records_are_rejected() {
                for size in [
                    0 as $length,
                    ($min - 1) as $length,
                    ($max_name + 1) as $length,
                    <$length>::MAX,
                ] {
                    assert!(matches!(
                        read_record(&mut Cursor::new(size.to_be_bytes())),
                        Err(ControlTlsError::RecordTooLarge)
                    ));
                }
                let mut partial = ($min as $length).to_be_bytes().to_vec();
                partial.extend_from_slice(&[0; $min - 1]);
                assert!(read_record(&mut Cursor::new(partial)).is_err());
            }

            #[test]
            fn protocol_requires_exact_alpn_and_tls13() {
                assert!(
                    require_protocol(Some(rustls::ProtocolVersion::TLSv1_3), Some($alpn_name))
                        .is_ok()
                );
                assert!(
                    require_protocol(Some(rustls::ProtocolVersion::TLSv1_2), Some($alpn_name))
                        .is_err()
                );
                assert!(
                    require_protocol(Some(rustls::ProtocolVersion::TLSv1_3), Some(b"plaintext"))
                        .is_err()
                );
                assert!(require_protocol(None, None).is_err());
            }

            #[test]
            fn oversized_prefix_is_rejected_without_reading_payload() {
                struct PrefixOnly {
                    prefix: Cursor<Vec<u8>>,
                }
                impl Read for PrefixOnly {
                    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                        assert!(
                            (self.prefix.position() as usize) < self.prefix.get_ref().len(),
                            "invalid length must not request a payload"
                        );
                        self.prefix.read(bytes)
                    }
                }
                let mut reader = PrefixOnly {
                    prefix: Cursor::new((($max_name + 1) as $length).to_be_bytes().to_vec()),
                };
                assert!(matches!(
                    read_record(&mut reader),
                    Err(ControlTlsError::RecordTooLarge)
                ));
            }

            #[test]
            fn grant_deadline_is_finite_and_excludes_expired_grants() {
                assert!(grant_deadline(0).is_err());
                let before = Instant::now();
                let deadline = grant_deadline(now_ms().unwrap() + 100).unwrap();
                assert!(deadline > before);
                assert!(deadline <= Instant::now() + Duration::from_millis(100));
            }

            #[test]
            fn listener_cancellation_precedes_and_interrupts_handshake() {
                let expected = binding();
                let (identity, _) = crate::tests::test_identities();
                let server = server(identity, expected);
                let stop = server.stop_handle().unwrap();
                stop.stop();
                assert!(matches!(
                    server.try_accept(),
                    Err(ControlTlsError::ChannelFailed)
                ));

                let (identity, _) = crate::tests::test_identities();
                let server = self::server(identity, expected);
                let address = server.local_addr().unwrap();
                let stop = server.stop_handle().unwrap();
                let mut raw = TcpStream::connect(address).unwrap();
                // Incomplete TLS header must not monopolize the lifecycle owner.
                raw.write_all(&[0x16]).unwrap();
                let worker = thread::spawn(move || accept(server));
                thread::sleep(Duration::from_millis(30));
                let before = Instant::now();
                stop.stop();
                assert!(worker.join().unwrap().is_err());
                assert!(before.elapsed() < Duration::from_secs(1));
            }

            #[test]
            fn slow_partial_handshake_cannot_outlive_grant() {
                let mut expected = binding();
                expected.expires_at_ms = now_ms().unwrap() + 200;
                let (identity, _) = crate::tests::test_identities();
                let server = server(identity, expected);
                let mut raw = TcpStream::connect(server.local_addr().unwrap()).unwrap();
                raw.write_all(&[0x16]).unwrap();
                let before = Instant::now();
                assert!(accept(server).is_err());
                assert!(before.elapsed() < Duration::from_secs(1));
            }

            #[test]
            fn local_deadline_caps_handshake_without_changing_wire_grant() {
                let expected = binding();
                let (identity, _) = crate::tests::test_identities();
                let server = $listener::from_listener_until(
                    TcpListener::bind("127.0.0.1:0").unwrap(),
                    identity,
                    expected,
                    Duration::from_secs(2),
                    Instant::now() + Duration::from_millis(150),
                )
                .unwrap();
                let mut raw = TcpStream::connect(server.local_addr().unwrap()).unwrap();
                raw.write_all(&[0x16]).unwrap();
                let before = Instant::now();
                assert!(accept(server).is_err());
                assert!(before.elapsed() < Duration::from_secs(1));
            }

            #[test]
            fn local_record_deadline_can_only_shorten() {
                let expected = binding();
                let (identity, client) = crate::tests::test_identities();
                let server = server(identity, expected);
                let address = server.local_addr().unwrap();
                let worker = thread::spawn(move || {
                    let mut channel = accept(server).unwrap();
                    let short = Instant::now() + Duration::from_millis(150);
                    channel.restrict_deadline(short);
                    channel.restrict_deadline(short + Duration::from_secs(10));
                    assert_eq!(channel.io.socket().grant_deadline, short);
                    assert_eq!(channel.binding(), expected);
                    let before = Instant::now();
                    assert!(channel.receive_record().is_err());
                    assert!(before.elapsed() < Duration::from_secs(1));
                });
                let _channel = $channel::connect_until(
                    ControlTlsEndpoint {
                        address,
                        io_timeout: Duration::from_secs(2),
                    },
                    "localhost",
                    client,
                    expected,
                    Instant::now() + Duration::from_secs(1),
                )
                .unwrap();
                worker.join().unwrap();
            }

            #[test]
            fn plaintext_peer_is_rejected() {
                let expected = binding();
                let (identity, _) = crate::tests::test_identities();
                let server = server(identity, expected);
                let address = server.local_addr().unwrap();
                let worker = thread::spawn(move || accept(server));
                let mut raw = TcpStream::connect(address).unwrap();
                raw.write_all(b"CAVC plaintext camera records are forbidden")
                    .unwrap();
                raw.shutdown(Shutdown::Both).unwrap();
                assert!(worker.join().unwrap().is_err());
            }

            #[test]
            fn accepted_binding_closes_listener_and_partial_record_expires_at_grant() {
                let mut expected = binding();
                expected.expires_at_ms = now_ms().unwrap() + 800;
                let (identity, client) = crate::tests::test_identities();
                let server = server(identity, expected);
                let address = server.local_addr().unwrap();
                let worker = thread::spawn(move || {
                    let mut channel = loop {
                        if let Some(channel) = server.try_accept().unwrap() {
                            break channel;
                        }
                        thread::sleep(Duration::from_millis(2));
                    };
                    if $one_shot {
                        assert!(matches!(
                            server.try_accept(),
                            Err(ControlTlsError::ChannelFailed)
                        ));
                        assert!(
                            TcpStream::connect_timeout(&address, Duration::from_millis(100))
                                .is_err()
                        );
                    } else {
                        assert!(server.try_accept().unwrap().is_none());
                    }
                    let before = Instant::now();
                    assert!(channel.receive_record().is_err());
                    assert!(before.elapsed() < Duration::from_millis(1200));
                    assert!(matches!(
                        channel.receive_record(),
                        Err(ControlTlsError::ChannelFailed)
                    ));
                });
                let mut channel = connect(address, client, expected).unwrap();
                channel
                    .io
                    .write_all(&($min as $length).to_be_bytes())
                    .unwrap();
                channel.io.write_all(&[0]).unwrap();
                channel.io.flush().unwrap();
                worker.join().unwrap();
            }

            #[test]
            fn legacy_listener_can_accept_after_first_channel_closes() {
                if $one_shot {
                    return;
                }
                let expected = binding();
                let (identity, client) = crate::tests::test_identities();
                let server = server(identity, expected);
                let address = server.local_addr().unwrap();
                let worker = thread::spawn(move || {
                    for _ in 0..2 {
                        let mut channel = loop {
                            if let Some(channel) = server.try_accept().unwrap() {
                                break channel;
                            }
                            thread::sleep(Duration::from_millis(2));
                        };
                        channel.send_record(&[1; $min]).unwrap();
                        channel.close();
                    }
                });
                for _ in 0..2 {
                    let mut channel = connect(address, client.clone(), expected).unwrap();
                    assert_eq!(channel.receive_record().unwrap(), vec![1; $min]);
                }
                worker.join().unwrap();
            }

            #[test]
            fn expired_channel_fails_closed_without_more_payload() {
                let expected = binding();
                let (identity, client) = crate::tests::test_identities();
                let server = server(identity, expected);
                let address = server.local_addr().unwrap();
                let worker = thread::spawn(move || {
                    let mut channel = accept(server).unwrap();
                    channel.binding.expires_at_ms = 1;
                    assert!(matches!(
                        channel.send_record(&[0; $min]),
                        Err(ControlTlsError::UnauthorizedBinding)
                    ));
                    assert!(matches!(
                        channel.receive_record(),
                        Err(ControlTlsError::ChannelFailed)
                    ));
                });
                let _channel = connect(address, client, expected).unwrap();
                worker.join().unwrap();
            }
        }
    };
}
pub(crate) use route_tls;
