use crate::{LocalIdentity, PeerHello};
use ring::hmac;
use rustls::{
    ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
    pki_types::{CertificateDer, PrivateKeyDer, ServerName},
    server::WebPkiClientVerifier,
    version::TLS13,
};
use sha2::{Digest, Sha256};
use spake2::{Ed25519Group, Identity, Password, Spake2};
use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::Arc,
};

const ALPN: &[u8] = b"capyio-enrollment/3";
const MAX_FRAME: usize = 32_768;

pub enum EnrollmentChannel {
    Initiator(StreamOwned<ClientConnection, TcpStream>),
    Welcomer(StreamOwned<ServerConnection, TcpStream>),
}

impl EnrollmentChannel {
    pub(crate) fn set_read_timeout(&self, timeout: std::time::Duration) -> Result<(), String> {
        let socket = match self {
            Self::Initiator(s) => &s.sock,
            Self::Welcomer(s) => &s.sock,
        };
        socket
            .set_read_timeout(Some(timeout))
            .map_err(|e| e.to_string())
    }
}

impl Read for EnrollmentChannel {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Initiator(s) => s.read(bytes),
            Self::Welcomer(s) => s.read(bytes),
        }
    }
}
impl Write for EnrollmentChannel {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Initiator(s) => s.write(bytes),
            Self::Welcomer(s) => s.write(bytes),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Initiator(s) => s.flush(),
            Self::Welcomer(s) => s.flush(),
        }
    }
}

pub(crate) fn send(stream: &mut impl Write, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_FRAME {
        return Err("enrollment frame too large".into());
    }
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .and_then(|_| stream.write_all(bytes))
        .and_then(|_| stream.flush())
        .map_err(|e| e.to_string())
}
pub(crate) fn receive(stream: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut header = [0; 4];
    stream.read_exact(&mut header).map_err(|e| e.to_string())?;
    let len = u32::from_be_bytes(header) as usize;
    if len == 0 || len > MAX_FRAME {
        return Err("invalid enrollment frame length".into());
    }
    let mut bytes = vec![0; len];
    stream.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// Authenticate a single attempt, then prove possession of the exact PAKE-bound
/// certificate by ordinary pinned mTLS. Caller sets deadline and rejects existing peers.
pub fn authenticate(
    mut socket: TcpStream,
    local: &LocalIdentity,
    pin: &str,
    initiator: bool,
) -> Result<(EnrollmentChannel, PeerHello), String> {
    if pin.len() != 8 || !pin.bytes().all(|b| b.is_ascii_digit()) {
        return Err("PIN must contain eight digits".into());
    }
    let ours = serde_json::to_vec(&local.hello).map_err(|e| e.to_string())?;
    send(&mut socket, &ours)?;
    let theirs = receive(&mut socket)?;
    let peer: PeerHello =
        serde_json::from_slice(&theirs).map_err(|_| "invalid peer enrollment hello")?;
    peer.validate()?;
    if peer.node_id == local.hello.node_id {
        return Err("cannot pair a node with itself".into());
    }
    let (a, b) = if initiator {
        (&ours, &theirs)
    } else {
        (&theirs, &ours)
    };
    let mut transcript = Sha256::new();
    transcript.update(ALPN);
    for hello in [a, b] {
        transcript.update((hello.len() as u32).to_be_bytes());
        transcript.update(hello);
    }
    let context = transcript.finalize();
    let mut a_id = b"capyio-enroll/1/initiator/".to_vec();
    a_id.extend_from_slice(&context);
    let mut b_id = b"capyio-enroll/1/welcomer/".to_vec();
    b_id.extend_from_slice(&context);
    let password = Password::new(pin.as_bytes());
    let (state, outbound) = if initiator {
        Spake2::<Ed25519Group>::start_a(&password, &Identity::new(&a_id), &Identity::new(&b_id))
    } else {
        Spake2::<Ed25519Group>::start_b(&password, &Identity::new(&a_id), &Identity::new(&b_id))
    };
    send(&mut socket, &outbound)?;
    let inbound = receive(&mut socket)?;
    let mut secret = state.finish(&inbound).map_err(|_| "PIN exchange failed")?;
    let key = hmac::Key::new(hmac::HMAC_SHA256, &secret);
    secret.fill(0);
    let confirm = |role: u8| {
        let mut bytes = b"capyio-enroll/1/confirm/".to_vec();
        bytes.push(role);
        bytes.extend_from_slice(&context);
        bytes
    };
    send(
        &mut socket,
        hmac::sign(&key, &confirm(if initiator { 1 } else { 2 })).as_ref(),
    )?;
    let proof = receive(&mut socket)?;
    hmac::verify(&key, &confirm(if initiator { 2 } else { 1 }), &proof)
        .map_err(|_| "PIN does not match; reopen pairing for a new attempt")?;

    let bundle = local.credential_for(&peer)?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(peer.ca.clone()))
        .map_err(|e| e.to_string())?;
    let (chain, key_der, _) = bundle.into_parts();
    let certs = chain.into_iter().map(CertificateDer::from).collect();
    let private_key = PrivateKeyDer::try_from(key_der).map_err(|e| e.to_string())?;
    let channel = if initiator {
        let mut config = ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&TLS13])
            .map_err(|e| e.to_string())?
            .with_root_certificates(roots)
            .with_client_auth_cert(certs, private_key)
            .map_err(|e| e.to_string())?;
        config.alpn_protocols = vec![ALPN.to_vec()];
        let mut conn = ClientConnection::new(
            Arc::new(config),
            ServerName::try_from(peer.server_name()).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        while conn.is_handshaking() {
            conn.complete_io(&mut socket).map_err(|e| e.to_string())?;
        }
        if conn.alpn_protocol() != Some(ALPN)
            || conn
                .peer_certificates()
                .and_then(|c| c.first())
                .map(|c| c.as_ref())
                != Some(peer.leaf.as_slice())
        {
            return Err("enrollment TLS peer mismatch".into());
        }
        EnrollmentChannel::Initiator(StreamOwned::new(conn, socket))
    } else {
        let verifier =
            WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider.clone())
                .build()
                .map_err(|e| e.to_string())?;
        let mut config = ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&TLS13])
            .map_err(|e| e.to_string())?
            .with_client_cert_verifier(verifier)
            .with_single_cert(certs, private_key)
            .map_err(|e| e.to_string())?;
        config.alpn_protocols = vec![ALPN.to_vec()];
        let mut conn = ServerConnection::new(Arc::new(config)).map_err(|e| e.to_string())?;
        while conn.is_handshaking() {
            conn.complete_io(&mut socket).map_err(|e| e.to_string())?;
        }
        if conn.alpn_protocol() != Some(ALPN)
            || conn
                .peer_certificates()
                .and_then(|c| c.first())
                .map(|c| c.as_ref())
                != Some(peer.leaf.as_slice())
        {
            return Err("enrollment TLS peer mismatch".into());
        }
        EnrollmentChannel::Welcomer(StreamOwned::new(conn, socket))
    };
    Ok((channel, peer))
}
