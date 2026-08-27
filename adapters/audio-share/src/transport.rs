//! Bounded sender for the pinned Audio Share v0.3.4 Android private contract.
//!
//! This is intentionally not a CapyIO StandardPort wire protocol. It exists so
//! the user-mode Broker can feed PCM obtained from the CapyIO render boundary
//! to the existing Android receiver without routing media through JSON-RPC.

use std::{
    collections::HashMap,
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use capyio_audio::{AudioFormat, AudioSampleFormat};
use prost::Message;
use thiserror::Error;

const CMD_GET_FORMAT: u32 = 1;
const CMD_START_PLAY: u32 = 2;
const CMD_HEARTBEAT: u32 = 3;
const AUDIO_SHARE_MTU: usize = 1492;
const IPV4_UDP_OVERHEAD: usize = 20 + 8;
const MAX_UDP_PCM_BYTES: usize = AUDIO_SHARE_MTU - IPV4_UDP_OVERHEAD;
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const IO_TIMEOUT: Duration = Duration::from_millis(100);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(3);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioSharePrivateFormat {
    encoding: i32,
    channels: u16,
    sample_rate_hz: u32,
    block_align: usize,
}

impl AudioSharePrivateFormat {
    pub fn from_audio_format(format: &AudioFormat) -> Result<Self, AudioShareTransportError> {
        format
            .validate()
            .map_err(|error| AudioShareTransportError::InvalidFormat(error.to_string()))?;
        if format.channels > 8 || format.sample_rate_hz > 192_000 {
            return Err(AudioShareTransportError::InvalidFormat(
                "Audio Share v0.3.4 supports at most 8 channels and 192 kHz".to_owned(),
            ));
        }
        let encoding = match format.sample_format {
            AudioSampleFormat::FloatF32Le => 1,
            AudioSampleFormat::SignedI16Le => 3,
            AudioSampleFormat::SignedI24Le => 4,
            AudioSampleFormat::SignedI32Le => 5,
        };
        let block_align =
            usize::from(format.channels) * usize::from(format.sample_format.bytes_per_sample());
        if block_align > MAX_UDP_PCM_BYTES {
            return Err(AudioShareTransportError::InvalidFormat(
                "PCM block alignment exceeds the private UDP payload bound".to_owned(),
            ));
        }
        Ok(Self {
            encoding,
            channels: format.channels,
            sample_rate_hz: format.sample_rate_hz,
            block_align,
        })
    }

    #[must_use]
    pub const fn block_align(self) -> usize {
        self.block_align
    }

    fn protobuf(self) -> Vec<u8> {
        WireAudioFormat {
            encoding: self.encoding,
            channels: i32::from(self.channels),
            sample_rate: i32::try_from(self.sample_rate_hz)
                .expect("validated Audio Share sample rate fits i32"),
        }
        .encode_to_vec()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioShareTransportConfig {
    pub bind_address: SocketAddr,
    pub queue_blocks: usize,
    pub max_block_bytes: usize,
    pub max_peers: usize,
}

impl AudioShareTransportConfig {
    pub fn local_lab(bind_address: SocketAddr) -> Self {
        Self {
            bind_address,
            queue_blocks: 8,
            max_block_bytes: 64 * 1024,
            max_peers: 4,
        }
    }

    fn validate(self) -> Result<Self, AudioShareTransportError> {
        if !self.bind_address.is_ipv4() || self.bind_address.ip().is_unspecified() {
            return Err(AudioShareTransportError::InvalidBindAddress);
        }
        if self.queue_blocks == 0 || self.queue_blocks > 256 {
            return Err(AudioShareTransportError::InvalidQueueCapacity);
        }
        if self.max_block_bytes == 0 || self.max_block_bytes > 1024 * 1024 {
            return Err(AudioShareTransportError::InvalidBlockLimit);
        }
        if self.max_peers == 0 || self.max_peers > 64 {
            return Err(AudioShareTransportError::InvalidPeerLimit);
        }
        Ok(self)
    }
}

#[derive(Clone)]
pub struct AudioShareTransportSender {
    tx: SyncSender<Vec<u8>>,
    format: AudioSharePrivateFormat,
    max_block_bytes: usize,
    stopped: Arc<AtomicBool>,
    counters: Arc<TransportCounters>,
}

impl AudioShareTransportSender {
    /// Copies one bounded, frame-aligned PCM block into the Broker-owned queue.
    /// It never waits for queue capacity.
    pub fn try_send_pcm(&self, pcm: &[u8]) -> Result<(), AudioShareTransportError> {
        if pcm.is_empty()
            || pcm.len() > self.max_block_bytes
            || !pcm.len().is_multiple_of(self.format.block_align())
        {
            return Err(AudioShareTransportError::InvalidPcmBlock {
                actual: pcm.len(),
                limit: self.max_block_bytes,
                block_align: self.format.block_align(),
            });
        }
        if self.stopped.load(Ordering::Acquire) {
            return Err(AudioShareTransportError::Stopped);
        }
        match self.tx.try_send(pcm.to_vec()) {
            Ok(()) => {
                saturating_increment(&self.counters.blocks_enqueued, 1);
                Ok(())
            }
            Err(TrySendError::Full(_)) => {
                saturating_increment(&self.counters.queue_full, 1);
                Err(AudioShareTransportError::QueueFull)
            }
            Err(TrySendError::Disconnected(_)) => Err(AudioShareTransportError::Stopped),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AudioShareTransportStats {
    pub blocks_enqueued: u64,
    pub queue_full: u64,
    pub blocks_without_receiver: u64,
    pub datagrams_sent: u64,
    pub datagram_send_errors: u64,
    pub pcm_bytes_sent: u64,
}

#[derive(Default)]
struct TransportCounters {
    blocks_enqueued: AtomicU64,
    queue_full: AtomicU64,
    blocks_without_receiver: AtomicU64,
    datagrams_sent: AtomicU64,
    datagram_send_errors: AtomicU64,
    pcm_bytes_sent: AtomicU64,
}

impl TransportCounters {
    fn snapshot(&self) -> AudioShareTransportStats {
        AudioShareTransportStats {
            blocks_enqueued: self.blocks_enqueued.load(Ordering::Relaxed),
            queue_full: self.queue_full.load(Ordering::Relaxed),
            blocks_without_receiver: self.blocks_without_receiver.load(Ordering::Relaxed),
            datagrams_sent: self.datagrams_sent.load(Ordering::Relaxed),
            datagram_send_errors: self.datagram_send_errors.load(Ordering::Relaxed),
            pcm_bytes_sent: self.pcm_bytes_sent.load(Ordering::Relaxed),
        }
    }
}

pub struct AudioShareTransport {
    local_address: SocketAddr,
    sender: AudioShareTransportSender,
    peers: Arc<Mutex<HashMap<i32, PeerRegistration>>>,
    counters: Arc<TransportCounters>,
    stopped: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl AudioShareTransport {
    pub fn bind(
        config: AudioShareTransportConfig,
        format: AudioSharePrivateFormat,
    ) -> Result<Self, AudioShareTransportError> {
        let config = config.validate()?;
        let tcp = TcpListener::bind(config.bind_address).map_err(|source| {
            AudioShareTransportError::Bind {
                protocol: "TCP",
                source,
            }
        })?;
        tcp.set_nonblocking(true)
            .map_err(|source| AudioShareTransportError::Configure {
                protocol: "TCP",
                source,
            })?;
        let local_address =
            tcp.local_addr()
                .map_err(|source| AudioShareTransportError::Configure {
                    protocol: "TCP",
                    source,
                })?;
        let udp =
            UdpSocket::bind(local_address).map_err(|source| AudioShareTransportError::Bind {
                protocol: "UDP",
                source,
            })?;
        udp.set_nonblocking(true)
            .map_err(|source| AudioShareTransportError::Configure {
                protocol: "UDP",
                source,
            })?;
        let broadcast_udp =
            udp.try_clone()
                .map_err(|source| AudioShareTransportError::Configure {
                    protocol: "UDP",
                    source,
                })?;

        let stopped = Arc::new(AtomicBool::new(false));
        let peers = Arc::new(Mutex::new(HashMap::new()));
        let next_id = Arc::new(AtomicI32::new(1));
        let counters = Arc::new(TransportCounters::default());
        let (tx, rx) = mpsc::sync_channel(config.queue_blocks);
        let format_wire = Arc::new(format.protobuf());

        let accept_thread = {
            let stopped = Arc::clone(&stopped);
            let peers = Arc::clone(&peers);
            let next_id = Arc::clone(&next_id);
            thread::Builder::new()
                .name("capyio-audio-share-control".to_owned())
                .spawn(move || {
                    accept_loop(tcp, stopped, peers, next_id, format_wire, config.max_peers);
                })
                .map_err(AudioShareTransportError::Spawn)?
        };
        let udp_thread = {
            let stopped = Arc::clone(&stopped);
            let peers = Arc::clone(&peers);
            thread::Builder::new()
                .name("capyio-audio-share-registration".to_owned())
                .spawn(move || udp_registration_loop(udp, stopped, peers))
                .map_err(AudioShareTransportError::Spawn)?
        };
        let broadcast_thread = {
            let stopped = Arc::clone(&stopped);
            let peers = Arc::clone(&peers);
            let counters = Arc::clone(&counters);
            thread::Builder::new()
                .name("capyio-audio-share-pcm".to_owned())
                .spawn(move || {
                    broadcast_loop(
                        broadcast_udp,
                        rx,
                        stopped,
                        peers,
                        counters,
                        format.block_align,
                    )
                })
                .map_err(AudioShareTransportError::Spawn)?
        };

        let sender = AudioShareTransportSender {
            tx,
            format,
            max_block_bytes: config.max_block_bytes,
            stopped: Arc::clone(&stopped),
            counters: Arc::clone(&counters),
        };
        Ok(Self {
            local_address,
            sender,
            peers,
            counters,
            stopped,
            threads: vec![accept_thread, udp_thread, broadcast_thread],
        })
    }

    #[must_use]
    pub const fn local_address(&self) -> SocketAddr {
        self.local_address
    }

    #[must_use]
    pub fn sender(&self) -> AudioShareTransportSender {
        self.sender.clone()
    }

    /// Number of live TCP sessions that completed UDP association.
    #[must_use]
    pub fn connected_receivers(&self) -> usize {
        self.peers
            .lock()
            .map(|guard| {
                guard
                    .values()
                    .filter(|peer| peer.udp_address.is_some())
                    .count()
            })
            .unwrap_or(0)
    }

    #[must_use]
    pub fn stats(&self) -> AudioShareTransportStats {
        self.counters.snapshot()
    }

    pub fn shutdown(mut self) {
        self.stop_and_join();
    }

    fn stop_and_join(&mut self) {
        self.stopped.store(true, Ordering::Release);
        while let Some(handle) = self.threads.pop() {
            let _ = handle.join();
        }
    }
}

impl Drop for AudioShareTransport {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

#[derive(Debug)]
struct PeerRegistration {
    tcp_ip: std::net::IpAddr,
    udp_address: Option<SocketAddr>,
    last_heartbeat: Instant,
}

fn accept_loop(
    listener: TcpListener,
    stopped: Arc<AtomicBool>,
    peers: Arc<Mutex<HashMap<i32, PeerRegistration>>>,
    next_id: Arc<AtomicI32>,
    format_wire: Arc<Vec<u8>>,
    max_peers: usize,
) {
    let mut connections = Vec::new();
    while !stopped.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                let stopped = Arc::clone(&stopped);
                let peers = Arc::clone(&peers);
                let next_id = Arc::clone(&next_id);
                let format_wire = Arc::clone(&format_wire);
                if let Ok(handle) = thread::Builder::new()
                    .name("capyio-audio-share-peer".to_owned())
                    .spawn(move || {
                        control_peer_loop(stream, stopped, peers, next_id, format_wire, max_peers);
                    })
                {
                    connections.push(handle);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(POLL_INTERVAL);
            }
            Err(_) => break,
        }
    }
    for connection in connections {
        let _ = connection.join();
    }
}

fn control_peer_loop(
    mut stream: TcpStream,
    stopped: Arc<AtomicBool>,
    peers: Arc<Mutex<HashMap<i32, PeerRegistration>>>,
    next_id: Arc<AtomicI32>,
    format_wire: Arc<Vec<u8>>,
    max_peers: usize,
) {
    let Ok(tcp_ip) = stream.peer_addr().map(|address| address.ip()) else {
        return;
    };
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let mut command = [0_u8; 4];
    let mut filled = 0;
    let mut peer_id = None;
    let mut next_heartbeat = Instant::now() + HEARTBEAT_INTERVAL;

    while !stopped.load(Ordering::Acquire) {
        if let Some(id) = peer_id {
            let now = Instant::now();
            let timed_out = peers
                .lock()
                .map(|guard| {
                    guard.get(&id).is_none_or(|peer| {
                        now.duration_since(peer.last_heartbeat) > HEARTBEAT_TIMEOUT
                    })
                })
                .unwrap_or(true);
            if timed_out {
                break;
            }
            if now >= next_heartbeat {
                if stream.write_all(&CMD_HEARTBEAT.to_le_bytes()).is_err() {
                    break;
                }
                next_heartbeat = now + HEARTBEAT_INTERVAL;
            }
        }

        match stream.read(&mut command[filled..]) {
            Ok(0) => break,
            Ok(count) => {
                filled += count;
                if filled != command.len() {
                    continue;
                }
                let value = u32::from_le_bytes(command);
                filled = 0;
                if value == CMD_GET_FORMAT {
                    let Ok(size) = u32::try_from(format_wire.len()) else {
                        break;
                    };
                    if stream.write_all(&CMD_GET_FORMAT.to_le_bytes()).is_err()
                        || stream.write_all(&size.to_le_bytes()).is_err()
                        || stream.write_all(&format_wire).is_err()
                    {
                        break;
                    }
                } else if value == CMD_START_PLAY && peer_id.is_none() {
                    let id = next_id.fetch_add(1, Ordering::Relaxed);
                    let inserted = peers
                        .lock()
                        .map(|mut guard| {
                            if guard.len() >= max_peers {
                                return false;
                            }
                            guard.insert(
                                id,
                                PeerRegistration {
                                    tcp_ip,
                                    udp_address: None,
                                    last_heartbeat: Instant::now(),
                                },
                            );
                            true
                        })
                        .unwrap_or(false);
                    if !inserted
                        || stream.write_all(&CMD_START_PLAY.to_le_bytes()).is_err()
                        || stream.write_all(&id.to_le_bytes()).is_err()
                    {
                        break;
                    }
                    peer_id = Some(id);
                    next_heartbeat = Instant::now() + HEARTBEAT_INTERVAL;
                } else if value == CMD_HEARTBEAT {
                    if let Some(id) = peer_id
                        && let Ok(mut guard) = peers.lock()
                        && let Some(peer) = guard.get_mut(&id)
                    {
                        peer.last_heartbeat = Instant::now();
                    }
                } else {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
    if let Some(id) = peer_id
        && let Ok(mut guard) = peers.lock()
    {
        guard.remove(&id);
    }
}

fn udp_registration_loop(
    socket: UdpSocket,
    stopped: Arc<AtomicBool>,
    peers: Arc<Mutex<HashMap<i32, PeerRegistration>>>,
) {
    let mut bytes = [0_u8; 64];
    while !stopped.load(Ordering::Acquire) {
        match socket.recv_from(&mut bytes) {
            Ok((4, address)) => {
                let id = i32::from_le_bytes(bytes[..4].try_into().expect("four-byte id"));
                if let Ok(mut guard) = peers.lock()
                    && let Some(peer) = guard.get_mut(&id)
                    && peer.tcp_ip == address.ip()
                {
                    peer.udp_address = Some(address);
                }
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(POLL_INTERVAL);
            }
            Err(_) => break,
        }
    }
}

fn broadcast_loop(
    socket: UdpSocket,
    rx: Receiver<Vec<u8>>,
    stopped: Arc<AtomicBool>,
    peers: Arc<Mutex<HashMap<i32, PeerRegistration>>>,
    counters: Arc<TransportCounters>,
    block_align: usize,
) {
    let segment_bytes = MAX_UDP_PCM_BYTES - (MAX_UDP_PCM_BYTES % block_align);
    while !stopped.load(Ordering::Acquire) {
        let pcm = match rx.recv_timeout(IO_TIMEOUT) {
            Ok(pcm) => pcm,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        let addresses = peers
            .lock()
            .map(|guard| {
                guard
                    .values()
                    .filter_map(|peer| peer.udp_address)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if addresses.is_empty() {
            saturating_increment(&counters.blocks_without_receiver, 1);
            continue;
        }
        for segment in pcm.chunks(segment_bytes) {
            for address in &addresses {
                match socket.send_to(segment, address) {
                    Ok(bytes) => {
                        saturating_increment(&counters.datagrams_sent, 1);
                        saturating_increment(
                            &counters.pcm_bytes_sent,
                            u64::try_from(bytes).unwrap_or(u64::MAX),
                        );
                    }
                    Err(_) => saturating_increment(&counters.datagram_send_errors, 1),
                }
            }
        }
    }
}

fn saturating_increment(counter: &AtomicU64, increment: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_add(increment))
    });
}

#[derive(Clone, PartialEq, Message)]
struct WireAudioFormat {
    #[prost(int32, tag = "1")]
    encoding: i32,
    #[prost(int32, tag = "2")]
    channels: i32,
    #[prost(int32, tag = "3")]
    sample_rate: i32,
}

#[derive(Debug, Error)]
pub enum AudioShareTransportError {
    #[error("invalid Audio Share private format: {0}")]
    InvalidFormat(String),
    #[error("Audio Share transport queue capacity is outside 1..=256")]
    InvalidQueueCapacity,
    #[error("Audio Share transport PCM block limit is outside 1..=1048576")]
    InvalidBlockLimit,
    #[error("Audio Share transport peer limit is outside 1..=64")]
    InvalidPeerLimit,
    #[error("Audio Share private transport requires an explicit IPv4 bind address")]
    InvalidBindAddress,
    #[error("could not bind Audio Share {protocol}: {source}")]
    Bind {
        protocol: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("could not configure Audio Share {protocol}: {source}")]
    Configure {
        protocol: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("could not spawn Audio Share transport worker: {0}")]
    Spawn(#[source] io::Error),
    #[error("PCM block has {actual} bytes; limit={limit}, block_align={block_align}")]
    InvalidPcmBlock {
        actual: usize,
        limit: usize,
        block_align: usize,
    },
    #[error("Audio Share PCM queue is full")]
    QueueFull,
    #[error("Audio Share transport is stopped")]
    Stopped,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaker_baseline_matches_pinned_protobuf_wire_format() {
        let format = AudioSharePrivateFormat::from_audio_format(&AudioFormat::speaker_baseline())
            .expect("baseline format");
        assert_eq!(format.block_align(), 4);
        assert_eq!(
            format.protobuf(),
            [0x08, 0x03, 0x10, 0x02, 0x18, 0x80, 0xf7, 0x02]
        );
    }

    #[test]
    fn private_transport_negotiates_and_delivers_segmented_pcm() {
        let format = AudioSharePrivateFormat::from_audio_format(&AudioFormat::speaker_baseline())
            .expect("baseline format");
        let transport = AudioShareTransport::bind(
            AudioShareTransportConfig::local_lab("127.0.0.1:0".parse().unwrap()),
            format,
        )
        .expect("bind transport");
        let address = transport.local_address();
        let mut tcp = TcpStream::connect(address).expect("connect TCP");
        tcp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        tcp.write_all(&CMD_GET_FORMAT.to_le_bytes()).unwrap();
        assert_eq!(read_u32(&mut tcp), CMD_GET_FORMAT);
        let format_size = read_u32(&mut tcp) as usize;
        let mut format_bytes = vec![0; format_size];
        tcp.read_exact(&mut format_bytes).unwrap();
        assert_eq!(format_bytes, format.protobuf());

        tcp.write_all(&CMD_START_PLAY.to_le_bytes()).unwrap();
        assert_eq!(read_u32(&mut tcp), CMD_START_PLAY);
        let id = read_i32(&mut tcp);
        assert!(id > 0);

        let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
        udp.set_read_timeout(Some(Duration::from_millis(250)))
            .unwrap();
        udp.send_to(&id.to_le_bytes(), address).unwrap();
        let pcm = (0..2_000_u16)
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let mut received = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(2);
        while received.len() < pcm.len() && Instant::now() < deadline {
            transport.sender().try_send_pcm(&pcm).unwrap();
            let mut datagram = [0_u8; 2_048];
            while let Ok((count, _)) = udp.recv_from(&mut datagram) {
                assert!(count <= MAX_UDP_PCM_BYTES);
                assert!(count.is_multiple_of(format.block_align()));
                received.extend_from_slice(&datagram[..count]);
                if received.len() >= pcm.len() {
                    break;
                }
            }
            if received.is_empty() {
                thread::sleep(POLL_INTERVAL);
            }
        }
        assert_eq!(&received[..pcm.len()], pcm);
        let stats = transport.stats();
        assert!(stats.blocks_enqueued >= 1);
        assert!(stats.datagrams_sent >= 3);
        assert!(stats.pcm_bytes_sent >= pcm.len() as u64);
        assert_eq!(stats.datagram_send_errors, 0);
        transport.shutdown();
    }

    #[test]
    fn sender_rejects_empty_oversized_and_unaligned_blocks() {
        let format = AudioSharePrivateFormat::from_audio_format(&AudioFormat::speaker_baseline())
            .expect("baseline format");
        let config = AudioShareTransportConfig {
            max_block_bytes: 16,
            ..AudioShareTransportConfig::local_lab("127.0.0.1:0".parse().unwrap())
        };
        let transport = AudioShareTransport::bind(config, format).unwrap();
        let sender = transport.sender();
        for invalid in [&[][..], &[0_u8; 3][..], &[0_u8; 20][..]] {
            assert!(matches!(
                sender.try_send_pcm(invalid),
                Err(AudioShareTransportError::InvalidPcmBlock { .. })
            ));
        }
    }

    fn read_u32(stream: &mut TcpStream) -> u32 {
        let mut bytes = [0_u8; 4];
        stream.read_exact(&mut bytes).unwrap();
        u32::from_le_bytes(bytes)
    }

    fn read_i32(stream: &mut TcpStream) -> i32 {
        let mut bytes = [0_u8; 4];
        stream.read_exact(&mut bytes).unwrap();
        i32::from_le_bytes(bytes)
    }
}
