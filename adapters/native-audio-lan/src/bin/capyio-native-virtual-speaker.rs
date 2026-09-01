use std::collections::VecDeque;

const BYTES_PER_FRAME: usize = 4;
const FRAMES_PER_PACKET: u32 = 480;
const PACKET_BYTES: usize = FRAMES_PER_PACKET as usize * BYTES_PER_FRAME;
const MAX_PENDING_BYTES: usize = 64 * 1024;

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    match windows_main() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("capyio-native-virtual-speaker is supported only on Windows");
    std::process::ExitCode::FAILURE
}

#[cfg(windows)]
fn windows_main() -> Result<(), String> {
    use std::{
        env,
        io::Write,
        net::IpAddr,
        thread,
        time::{Duration, Instant},
    };

    use capyio_audio::AudioMediaPacket;
    use capyio_native_audio_lan::{
        MAX_NATIVE_LAN_INFLIGHT_PACKETS, NativeLanEndpointConfig, NativeLanUdpEndpoint,
        speaker_lab_binding,
    };
    use capyio_windows_render_ring::RenderRingConsumer;

    let mut args = env::args();
    let executable = args
        .next()
        .unwrap_or_else(|| "capyio-native-virtual-speaker".to_owned());
    let usage = || format!("usage: {executable} <local-ipv4:port> <phone-ipv4:port> [seconds]");
    let local = args
        .next()
        .ok_or_else(&usage)
        .and_then(|value| parse_explicit_ipv4(&value, "local"))?;
    let peer = args
        .next()
        .ok_or_else(&usage)
        .and_then(|value| parse_explicit_ipv4(&value, "peer"))?;
    let duration = args
        .next()
        .map(|value| {
            value
                .parse::<u64>()
                .ok()
                .filter(|seconds| (1..=300).contains(seconds))
                .map(Duration::from_secs)
                .ok_or_else(|| "seconds must be between 1 and 300".to_owned())
        })
        .transpose()?;
    if args.next().is_some() {
        return Err(usage());
    }

    if !matches!(local.ip(), IpAddr::V4(ip) if is_concrete_unicast(ip))
        || !matches!(peer.ip(), IpAddr::V4(ip) if is_concrete_unicast(ip))
        || local.port() == 0
        || peer.port() == 0
    {
        return Err("local and peer must be concrete unicast IPv4 socket addresses".to_owned());
    }

    // The Broker creates the protected cross-session mapping before an
    // application opens CapyIO Speaker and AudioDG attaches the render APO.
    let mut ring = RenderRingConsumer::create_baseline().map_err(|error| error.to_string())?;
    let binding = speaker_lab_binding();
    let mut endpoint = NativeLanUdpEndpoint::bind(
        local,
        NativeLanEndpointConfig {
            peer,
            read_timeout: Duration::from_millis(50),
            inflight_packet_capacity: MAX_NATIVE_LAN_INFLIGHT_PACKETS,
        },
        binding.clone(),
    )
    .map_err(|error| error.to_string())?;
    let mut accumulator = PcmPacketAccumulator::new();
    let mut pcm_block = Vec::with_capacity(8 * 1024);
    let mut sequence = 0_u64;
    let mut first_sample_index = 0_u64;
    let mut last_ring_dropped = 0_u64;
    let deadline = duration.map(|value| Instant::now() + value);
    println!(
        "native_virtual_speaker=true local={} peer={} format=s16le/48000/stereo ring=Global\\CapyIO.RenderRing.v1",
        endpoint.local_addr().map_err(|error| error.to_string())?,
        peer
    );
    std::io::stdout()
        .flush()
        .map_err(|error| format!("flush native speaker readiness: {error}"))?;

    loop {
        if deadline.is_some_and(|value| Instant::now() >= value) {
            break;
        }
        match ring.try_read_s16le(&mut pcm_block) {
            Ok(true) => {
                let (_, ring_dropped) = ring.counters();
                if ring_dropped != last_ring_dropped {
                    accumulator.mark_discontinuity();
                    last_ring_dropped = ring_dropped;
                }
                let dropped_frames = accumulator.push(&pcm_block)?;
                first_sample_index = first_sample_index.wrapping_add(dropped_frames);
                while let Some((payload, discontinuity)) = accumulator.pop_packet() {
                    endpoint
                        .send_packet(&AudioMediaPacket {
                            stream_id: binding.stream_id,
                            stream_epoch: binding.stream_epoch,
                            sequence,
                            source_timestamp_micros: first_sample_index * 1_000_000 / 48_000,
                            first_sample_index,
                            sample_count: FRAMES_PER_PACKET,
                            discontinuity: sequence == 0 || discontinuity,
                            payload,
                        })
                        .map_err(|error| error.to_string())?;
                    sequence = sequence.wrapping_add(1);
                    first_sample_index =
                        first_sample_index.wrapping_add(u64::from(FRAMES_PER_PACKET));
                }
            }
            Ok(false) => thread::sleep(Duration::from_millis(2)),
            Err(error) => return Err(error.to_string()),
        }
    }

    let (ring_produced, ring_dropped) = ring.counters();
    let (
        attach_attempts,
        attach_successes,
        attach_sample_rate,
        attach_channels,
        attach_stage,
        attach_error,
    ) = ring.attach_diagnostics();
    let stats = endpoint.metrics();
    println!(
        "bridge_complete=true ring_produced={} ring_dropped={} attach_attempts={} attach_successes={} attach_sample_rate={} attach_channels={} attach_stage={} attach_error={} packets_sent={} datagrams_sent={} bytes_sent={} accumulator_dropped_bytes={}",
        ring_produced,
        ring_dropped,
        attach_attempts,
        attach_successes,
        attach_sample_rate,
        attach_channels,
        attach_stage,
        attach_error,
        stats.packets_sent,
        stats.datagrams_sent,
        stats.bytes_sent,
        accumulator.dropped_bytes,
    );
    Ok(())
}

#[cfg(windows)]
fn parse_explicit_ipv4(value: &str, name: &str) -> Result<std::net::SocketAddr, String> {
    value
        .parse::<std::net::SocketAddr>()
        .map_err(|_| format!("{name} must be an explicit IPv4 socket address"))
}

#[cfg(windows)]
fn is_concrete_unicast(ip: std::net::Ipv4Addr) -> bool {
    !ip.is_unspecified() && !ip.is_multicast() && ip != std::net::Ipv4Addr::BROADCAST
}

struct PcmPacketAccumulator {
    pending: VecDeque<u8>,
    discontinuity: bool,
    dropped_bytes: u64,
}

impl PcmPacketAccumulator {
    fn new() -> Self {
        Self {
            pending: VecDeque::with_capacity(MAX_PENDING_BYTES),
            discontinuity: true,
            dropped_bytes: 0,
        }
    }

    fn push(&mut self, block: &[u8]) -> Result<u64, String> {
        if block.is_empty() || !block.len().is_multiple_of(BYTES_PER_FRAME) {
            return Err("render ring produced an empty or frame-misaligned PCM block".to_owned());
        }
        if block.len() > MAX_PENDING_BYTES {
            return Err("render ring PCM block exceeds the native accumulator bound".to_owned());
        }
        let mut dropped_frames = 0_u64;
        if self.pending.len() + block.len() > MAX_PENDING_BYTES {
            let dropped = self.pending.len();
            self.dropped_bytes = self.dropped_bytes.saturating_add(dropped as u64);
            dropped_frames = (dropped / BYTES_PER_FRAME) as u64;
            self.pending.clear();
            self.discontinuity = true;
        }
        self.pending.extend(block);
        Ok(dropped_frames)
    }

    fn pop_packet(&mut self) -> Option<(Vec<u8>, bool)> {
        if self.pending.len() < PACKET_BYTES {
            return None;
        }
        let payload = self.pending.drain(..PACKET_BYTES).collect();
        let discontinuity = std::mem::take(&mut self.discontinuity);
        Some((payload, discontinuity))
    }

    fn mark_discontinuity(&mut self) {
        self.discontinuity = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulator_emits_exact_packets_across_ring_blocks() {
        let mut accumulator = PcmPacketAccumulator::new();
        accumulator.push(&vec![1; 1_000]).unwrap();
        assert!(accumulator.pop_packet().is_none());
        accumulator.push(&vec![2; 2_840]).unwrap();
        let (first, discontinuity) = accumulator.pop_packet().unwrap();
        let (second, second_discontinuity) = accumulator.pop_packet().unwrap();
        assert_eq!(first.len(), PACKET_BYTES);
        assert_eq!(second.len(), PACKET_BYTES);
        assert!(discontinuity);
        assert!(!second_discontinuity);
        assert!(accumulator.pop_packet().is_none());
    }

    #[test]
    fn accumulator_pressure_is_bounded_and_discontinuous() {
        let mut accumulator = PcmPacketAccumulator::new();
        accumulator.push(&vec![1; MAX_PENDING_BYTES]).unwrap();
        accumulator.push(&[2; BYTES_PER_FRAME]).unwrap();
        assert_eq!(accumulator.pending.len(), BYTES_PER_FRAME);
        assert_eq!(accumulator.dropped_bytes, MAX_PENDING_BYTES as u64);
        assert!(accumulator.discontinuity);
        assert!(accumulator.push(&[0; 3]).is_err());
    }
}
