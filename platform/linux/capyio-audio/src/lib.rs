//! Explicit trusted-lab PCM playback, virtual microphone or selected stereo capture.
//! No mode performs pairing or opens resources on import.
use std::{io, net::SocketAddrV4, time::Duration};

use capyio_audio::{AudioMediaPacket, AudioMediaStreamBinding};
use capyio_native_audio_lan::microphone_lab_binding;

#[cfg(target_os = "linux")]
mod pipewire;
#[cfg(target_os = "linux")]
mod pipewire_command;
#[cfg(target_os = "linux")]
pub use pipewire::PipeWirePlayback;

#[cfg(target_os = "linux")]
mod route;
#[cfg(target_os = "linux")]
pub use route::RuntimePlayback;

#[cfg(target_os = "linux")]
mod owner;
#[cfg(target_os = "linux")]
pub use owner::{
    AuthenticatedPlaybackOwner, PlaybackAdmission, PlaybackOwnerEvent, PlaybackTarget,
};

#[cfg(target_os = "linux")]
mod capture;
#[cfg(target_os = "linux")]
pub use capture::{CAPTURE_PCM_PACKET_BYTES, CaptureMetrics, CapturePump, PcmCaptureSource};
#[cfg(target_os = "linux")]
mod capture_pipewire;
#[cfg(target_os = "linux")]
pub use capture_pipewire::PipeWireCapture;
#[cfg(target_os = "linux")]
mod capture_route;

pub const PCM_PACKET_BYTES: usize = 960;
pub const MAX_STALL: Duration = Duration::from_millis(250);

/// Blocking lifecycle-reader seam, never a media callback. One byte, EOF or a
/// read error requests Stop. The owner performs bounded media/child cleanup.
pub fn observe_owner_stop(input: &mut impl io::Read, stop: &std::sync::atomic::AtomicBool) {
    let _ = input.read(&mut [0_u8; 1]);
    stop.store(true, std::sync::atomic::Ordering::Release);
}

/// A worker-only endpoint. Implementations must return promptly, never wait for
/// buffer space, and must not be called from an OS real-time callback.
pub trait PcmPlaybackSink {
    fn try_write_pcm(&mut self, bytes: &[u8]) -> io::Result<usize>;
    fn check_alive(&mut self) -> io::Result<()>;
}

/// Explicit local projection, independent of network Source/Sink direction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EndpointMode {
    #[default]
    Playback,
    VirtualMicrophone,
    /// Explicit selected input only, never the default system source.
    StereoCapture,
    /// Owned application-selectable Audio/Sink, producing network stereo PCM.
    VirtualSpeaker,
}
impl EndpointMode {
    pub fn capture(self) -> bool {
        matches!(self, Self::StereoCapture | Self::VirtualSpeaker)
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn spec(self) -> capyio_audio::AudioStreamSpec {
        if self.capture() {
            capyio_audio::AudioStreamSpec::media_balanced()
        } else {
            capyio_audio::AudioStreamSpec::voice_interactive()
        }
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn endpoint(self) -> capyio_protocol::NativeAudioEndpoint {
        if self.capture() {
            capyio_protocol::NativeAudioEndpoint::Source
        } else {
            capyio_protocol::NativeAudioEndpoint::Sink
        }
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn preset(self) -> capyio_protocol::NativeAudioPreset {
        if self.capture() {
            capyio_protocol::NativeAudioPreset::PcmS16le48000Stereo
        } else {
            capyio_protocol::NativeAudioPreset::PcmS16le48000Mono
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Playback => "playback",
            Self::VirtualMicrophone => "virtual_microphone",
            Self::StereoCapture => "stereo_capture",
            Self::VirtualSpeaker => "virtual_speaker",
        }
    }
}

/// Runtime-owned identity, never a peer-provided device name or default change.
pub fn virtual_microphone_name(port: capyio_core::PortId) -> String {
    format!("capyio.virtual-microphone.{port}")
}

/// Capture intent cannot reinterpret an external target as a virtual endpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureEndpoint {
    SelectedSource(String),
    VirtualSpeaker(capyio_core::PortId),
}
impl CaptureEndpoint {
    pub fn node_name(&self) -> String {
        match self {
            Self::SelectedSource(target) => target.clone(),
            Self::VirtualSpeaker(source) => virtual_speaker_name(*source),
        }
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::SelectedSource(target) => {
                validate_endpoint_target(EndpointMode::StereoCapture, target)
            }
            Self::VirtualSpeaker(source) if !source.as_uuid().is_nil() => Ok(()),
            Self::VirtualSpeaker(_) => Err("virtual speaker requires owned Source identity"),
        }
    }
}
pub fn virtual_speaker_name(source: capyio_core::PortId) -> String {
    format!("capyio.virtual-speaker.{source}")
}

#[cfg(target_os = "linux")]
pub(crate) fn validate_endpoint_target(
    mode: EndpointMode,
    target: &str,
) -> Result<(), &'static str> {
    validate_target(target)?;
    if mode == EndpointMode::StereoCapture
        && (target.eq_ignore_ascii_case("auto") || target.eq_ignore_ascii_case("default"))
    {
        return Err("capture requires an explicit source");
    }
    if mode == EndpointMode::VirtualSpeaker {
        let suffix = target
            .strip_prefix("capyio.virtual-speaker.")
            .ok_or("virtual speaker name must be owned")?;
        let id: capyio_core::PortId = suffix
            .parse()
            .map_err(|_| "invalid virtual speaker identity")?;
        if id.as_uuid().is_nil() || virtual_speaker_name(id) != target {
            return Err("invalid virtual speaker identity");
        }
    }
    if mode == EndpointMode::VirtualMicrophone {
        let suffix = target
            .strip_prefix("capyio.virtual-microphone.")
            .ok_or("virtual microphone name must be owned")?;
        let id: capyio_core::PortId = suffix
            .parse()
            .map_err(|_| "invalid virtual microphone identity")?;
        if id.as_uuid().is_nil() || virtual_microphone_name(id) != target {
            return Err("invalid virtual microphone identity");
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackConfig {
    pub local: SocketAddrV4,
    pub peer: SocketAddrV4,
    pub duration: Duration,
    pub target: String,
    pub endpoint_mode: EndpointMode,
}

impl PlaybackConfig {
    /// Deliberately no implicit opt-in, address, target or duration. The fixed
    /// binding is a compatibility lab identity, never authenticated authority.
    pub fn parse(args: &[String]) -> Result<Self, &'static str> {
        if args.len() != 5 || args.iter().any(|arg| arg.len() > 128) || args[0] != "--trusted-lab" {
            return Err(
                "expected --trusted-lab <local-ip:port> <peer-ip:port> <seconds:1..300> <PipeWire-target>",
            );
        }
        let local = parse_lab_endpoint(&args[1])?;
        let peer = parse_lab_endpoint(&args[2])?;
        if local == peer {
            return Err("local and peer endpoints must differ");
        }
        let seconds = args[3].parse::<u64>().map_err(|_| "invalid duration")?;
        if !(1..=300).contains(&seconds) {
            return Err("duration must be 1..300 seconds");
        }
        validate_target(&args[4])?;
        Ok(Self {
            local,
            peer,
            duration: Duration::from_secs(seconds),
            target: args[4].clone(),
            endpoint_mode: EndpointMode::Playback,
        })
    }
}

fn parse_lab_endpoint(value: &str) -> Result<SocketAddrV4, &'static str> {
    let endpoint: SocketAddrV4 = value.parse().map_err(|_| "expected literal IPv4:port")?;
    let ip = endpoint.ip();
    let octets = ip.octets();
    let overlay = octets[0] == 100 && (64..=127).contains(&octets[1]);
    if endpoint.port() == 0 || !(ip.is_private() || ip.is_loopback() || overlay) || octets[3] == 255
    {
        return Err(
            "only concrete private, loopback or shared-overlay IPv4 endpoints are permitted",
        );
    }
    Ok(endpoint)
}

fn validate_target(target: &str) -> Result<(), &'static str> {
    if target.is_empty()
        || target.len() > 128
        || matches!(target, "auto" | "0" | "-1")
        || !target
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
        || target.starts_with('-')
    {
        return Err("expected an explicit bounded PipeWire node name or serial, not auto");
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlaybackMetrics {
    pub packets_queued: u64,
    /// Entire packets copied to the local process stream, not audible frames.
    pub packets_written: u64,
    /// Stereo capture packets handed to UDP, never proof of audible delivery.
    pub packets_sent: u64,
    pub stale_packets: u64,
    pub malformed_packets: u64,
    pub packets_dropped_busy: u64,
    pub bytes_written: u64,
}

/// A one-packet staging buffer. A partial write retains its offset; another
/// packet can never interleave. Congestion drops newest complete packets and a
/// fixed total stall budget fails closed even if tiny writes keep progressing.
pub struct PlaybackPump {
    binding: AudioMediaStreamBinding,
    pending: [u8; PCM_PACKET_BYTES],
    offset: usize,
    pending_since: Option<Duration>,
    last: Option<(u64, u64, u64)>,
    last_tick: Duration,
    lifetime: Duration,
    metrics: PlaybackMetrics,
}

impl PlaybackPump {
    pub fn new(lifetime: Duration) -> io::Result<Self> {
        if !(Duration::from_secs(1)..=Duration::from_secs(300)).contains(&lifetime) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "playback lifetime must be 1..300 seconds",
            ));
        }
        Self::with_binding(lifetime, microphone_lab_binding())
    }

    /// Exact dynamic binding from the owning Runtime; this does not grant access.
    pub fn with_binding(lifetime: Duration, binding: AudioMediaStreamBinding) -> io::Result<Self> {
        if !(Duration::from_secs(1)..=Duration::from_secs(300)).contains(&lifetime)
            || binding.selected_spec != capyio_audio::AudioStreamSpec::voice_interactive()
            || binding.validate().is_err()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid PCM binding or lifetime",
            ));
        }
        Ok(Self {
            binding,
            pending: [0; PCM_PACKET_BYTES],
            offset: 0,
            pending_since: None,
            last: None,
            last_tick: Duration::ZERO,
            lifetime,
            metrics: PlaybackMetrics::default(),
        })
    }
}

impl PlaybackPump {
    fn check_time(&mut self, now: Duration) -> io::Result<()> {
        if now < self.last_tick {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "playback clock regressed",
            ));
        }
        if now >= self.lifetime {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "playback lifetime expired",
            ));
        }
        self.last_tick = now;
        Ok(())
    }

    pub fn accept(&mut self, packet: &AudioMediaPacket, now: Duration) -> io::Result<()> {
        self.check_time(now)?;
        if packet.validate_against(&self.binding).is_err()
            || packet.payload.len() != PCM_PACKET_BYTES
        {
            self.metrics.malformed_packets = self.metrics.malformed_packets.saturating_add(1);
            return Ok(());
        }
        if self.last.is_some_and(|(sequence, end_sample, timestamp)| {
            packet.sequence <= sequence
                || packet.first_sample_index < end_sample
                || packet.source_timestamp_micros <= timestamp
        }) {
            self.metrics.stale_packets = self.metrics.stale_packets.saturating_add(1);
            return Ok(());
        }
        self.last = Some((
            packet.sequence,
            packet.first_sample_index + u64::from(packet.sample_count),
            packet.source_timestamp_micros,
        ));
        if self.pending_since.is_some() {
            self.metrics.packets_dropped_busy = self.metrics.packets_dropped_busy.saturating_add(1);
            return Ok(());
        }
        self.pending.copy_from_slice(&packet.payload);
        self.offset = 0;
        self.pending_since = Some(now);
        self.metrics.packets_queued = self.metrics.packets_queued.saturating_add(1);
        Ok(())
    }

    /// At most one nonblocking write per call, with no retry loop.
    pub fn flush(&mut self, sink: &mut impl PcmPlaybackSink, now: Duration) -> io::Result<()> {
        self.check_time(now)?;
        sink.check_alive()?;
        let Some(since) = self.pending_since else {
            return Ok(());
        };
        if now.saturating_sub(since) >= MAX_STALL {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "PCM endpoint exceeded stall budget",
            ));
        }
        match sink.try_write_pcm(&self.pending[self.offset..]) {
            Ok(0) => Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "PCM endpoint closed",
            )),
            Ok(bytes) if bytes <= PCM_PACKET_BYTES - self.offset => {
                self.offset += bytes;
                self.metrics.bytes_written =
                    self.metrics.bytes_written.saturating_add(bytes as u64);
                if self.offset == PCM_PACKET_BYTES {
                    self.pending_since = None;
                    self.metrics.packets_written = self.metrics.packets_written.saturating_add(1);
                }
                Ok(())
            }
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "PCM endpoint returned invalid write length",
            )),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    pub const fn metrics(&self) -> PlaybackMetrics {
        self.metrics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Sink {
        data: Vec<u8>,
        limit: usize,
        alive: bool,
    }
    impl PcmPlaybackSink for Sink {
        fn try_write_pcm(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.limit == 0 {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let n = self.limit.min(bytes.len());
            self.data.extend_from_slice(&bytes[..n]);
            Ok(n)
        }
        fn check_alive(&mut self) -> io::Result<()> {
            if self.alive {
                Ok(())
            } else {
                Err(io::ErrorKind::BrokenPipe.into())
            }
        }
    }
    fn sink(limit: usize) -> Sink {
        Sink {
            data: vec![],
            limit,
            alive: true,
        }
    }
    pub(crate) fn packet(sequence: u64) -> AudioMediaPacket {
        AudioMediaPacket {
            stream_id: microphone_lab_binding().stream_id,
            stream_epoch: 1,
            sequence,
            source_timestamp_micros: sequence.saturating_mul(10_000).saturating_add(1),
            first_sample_index: sequence.saturating_mul(480),
            sample_count: 480,
            discontinuity: false,
            payload: (0..PCM_PACKET_BYTES).map(|i| i as u8).collect(),
        }
    }
    fn pump() -> PlaybackPump {
        PlaybackPump::new(Duration::from_secs(1)).unwrap()
    }
    fn args() -> Vec<String> {
        [
            "--trusted-lab",
            "127.0.0.1:46000",
            "127.0.0.1:46001",
            "1",
            "alsa_output.test",
        ]
        .map(str::to_owned)
        .to_vec()
    }
    #[test]
    fn owner_stdin_eof_byte_or_error_requests_stop() {
        use std::sync::atomic::{AtomicBool, Ordering};
        for bytes in [vec![], vec![b's', b't', b'o', b'p']] {
            let stop = AtomicBool::new(false);
            let mut input = io::Cursor::new(bytes.clone());
            observe_owner_stop(&mut input, &stop);
            assert!(stop.load(Ordering::Acquire));
            assert_eq!(input.position(), u64::from(!bytes.is_empty()));
        }
        struct Broken;
        impl io::Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::ErrorKind::Interrupted.into())
            }
        }
        let stop = AtomicBool::new(false);
        observe_owner_stop(&mut Broken, &stop);
        assert!(stop.load(Ordering::Acquire));
    }

    #[test]
    fn launch_requires_explicit_lab_addresses_target_and_finite_duration() {
        assert!(PlaybackConfig::parse(&args()).is_ok());
        for (index, value) in [
            (0, "--play"),
            (1, "0.0.0.0:46000"),
            (1, "8.8.8.8:46000"),
            (1, "[::1]:46000"),
            (1, "127.0.0.1:0"),
            (2, "127.0.0.1:46000"),
            (2, "192.168.1.255:46000"),
            (3, "0"),
            (3, "301"),
            (4, "auto"),
            (4, "node;cmd"),
        ] {
            let mut values = args();
            values[index] = value.into();
            assert!(
                PlaybackConfig::parse(&values).is_err(),
                "accepted {index}: {value}"
            );
        }
        let mut values = args();
        values.push("extra".into());
        assert!(PlaybackConfig::parse(&values).is_err());
        assert!(PlaybackConfig::parse(&[]).is_err());
        assert!(PlaybackPump::new(Duration::ZERO).is_err());
    }
    #[test]
    fn partial_writes_preserve_exact_pcm_without_interleaving() {
        let mut p = pump();
        let mut s = sink(137);
        let first = packet(0);
        p.accept(&first, Duration::ZERO).unwrap();
        p.flush(&mut s, Duration::ZERO).unwrap();
        p.accept(&packet(1), Duration::from_millis(1)).unwrap();
        for tick in 2..10 {
            p.flush(&mut s, Duration::from_millis(tick)).unwrap();
        }
        assert_eq!(s.data, first.payload);
        assert_eq!(p.metrics().packets_written, 1);
        assert_eq!(p.metrics().packets_dropped_busy, 1);
        p.accept(&packet(1), Duration::from_millis(10)).unwrap();
        assert_eq!(p.metrics().stale_packets, 1);
    }
    #[test]
    fn replay_reset_reorder_and_wrong_binding_are_rejected_but_gaps_allowed() {
        let mut p = pump();
        let mut s = sink(960);
        for sequence in [4, 4, 3, 0, 6] {
            p.accept(&packet(sequence), Duration::ZERO).unwrap();
            p.flush(&mut s, Duration::ZERO).unwrap();
        }
        assert_eq!(p.metrics().packets_written, 2);
        assert_eq!(p.metrics().stale_packets, 3);
        let mut bad = packet(7);
        bad.stream_epoch = 2;
        p.accept(&bad, Duration::ZERO).unwrap();
        bad = packet(7);
        bad.payload.pop();
        p.accept(&bad, Duration::ZERO).unwrap();
        bad = packet(7);
        bad.first_sample_index = u64::MAX;
        p.accept(&bad, Duration::ZERO).unwrap();
        bad = packet(7);
        bad.stream_id = capyio_native_audio_lan::speaker_lab_binding().stream_id;
        p.accept(&bad, Duration::ZERO).unwrap();
        assert_eq!(p.metrics().malformed_packets, 4);
        bad = packet(7);
        bad.source_timestamp_micros = 0;
        p.accept(&bad, Duration::ZERO).unwrap();
        bad = packet(7);
        bad.first_sample_index = 0;
        p.accept(&bad, Duration::ZERO).unwrap();
        assert_eq!(p.metrics().stale_packets, 5);
    }
    #[test]
    fn congestion_and_trickled_writes_cannot_renew_stall_budget() {
        for limit in [0, 1] {
            let mut p = pump();
            let mut s = sink(limit);
            p.accept(&packet(0), Duration::ZERO).unwrap();
            for tick in 0..250 {
                p.flush(&mut s, Duration::from_millis(tick)).unwrap();
            }
            assert_eq!(
                p.flush(&mut s, MAX_STALL).unwrap_err().kind(),
                io::ErrorKind::TimedOut
            );
            assert_eq!(p.metrics().packets_written, 0);
        }
    }
    #[test]
    fn expiry_and_clock_regression_do_not_write_pending_pcm() {
        let mut p = pump();
        let mut s = sink(960);
        p.accept(&packet(0), Duration::from_millis(999)).unwrap();
        assert_eq!(
            p.flush(&mut s, Duration::from_secs(1)).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert!(s.data.is_empty());
        let mut p = pump();
        p.accept(&packet(0), Duration::from_millis(10)).unwrap();
        assert_eq!(
            p.flush(&mut s, Duration::ZERO).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(s.data.is_empty());
    }
    #[test]
    fn child_failure_is_checked_even_without_a_pending_packet() {
        let mut p = pump();
        let mut s = sink(960);
        s.alive = false;
        assert_eq!(
            p.flush(&mut s, Duration::ZERO).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }
    #[test]
    fn sequence_overflow_cannot_wrap_to_a_fresh_stream() {
        let mut p = pump();
        let mut s = sink(960);
        let mut end = packet(0);
        end.sequence = u64::MAX;
        p.accept(&end, Duration::ZERO).unwrap();
        p.flush(&mut s, Duration::ZERO).unwrap();
        p.accept(&packet(1), Duration::ZERO).unwrap();
        assert_eq!(p.metrics().packets_written, 1);
        assert_eq!(p.metrics().stale_packets, 1);
    }
}
