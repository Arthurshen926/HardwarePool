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
    eprintln!("capyio-native-virtual-microphone is supported only on Windows");
    std::process::ExitCode::FAILURE
}

#[cfg(windows)]
fn windows_main() -> Result<(), String> {
    use std::{
        env,
        io::Write,
        time::{Duration, Instant},
    };

    use capyio_native_audio_lan::{
        MAX_NATIVE_LAN_INFLIGHT_PACKETS, NativeLanEndpointConfig, NativeLanReceiveOutcome,
        NativeLanUdpEndpoint, microphone_lab_binding,
    };
    use capyio_windows_capture_ring::{CaptureRingProducer, CaptureWriteOutcome};

    let mut args = env::args();
    let executable = args
        .next()
        .unwrap_or_else(|| "capyio-native-virtual-microphone".to_owned());
    let usage = || format!("usage: {executable} <windows-ipv4:port> <phone-ipv4:port> [seconds]");
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

    let binding = microphone_lab_binding();
    let mut endpoint = NativeLanUdpEndpoint::bind(
        local,
        NativeLanEndpointConfig {
            peer,
            read_timeout: Duration::from_millis(50),
            inflight_packet_capacity: MAX_NATIVE_LAN_INFLIGHT_PACKETS,
        },
        binding,
    )
    .map_err(|error| error.to_string())?;
    let mut ring = CaptureRingProducer::attach().map_err(|error| error.to_string())?;
    let deadline = duration.map(|value| Instant::now() + value);
    let mut packets_committed = 0_u64;
    let mut packets_dropped_full = 0_u64;
    let mut frames_committed = 0_u64;
    println!(
        "native_virtual_microphone=true local={} peer={} format=s16le/48000/mono ring=Global\\CapyIO.CaptureRing.v1",
        endpoint.local_addr().map_err(|error| error.to_string())?,
        peer
    );
    std::io::stdout()
        .flush()
        .map_err(|error| format!("flush native microphone readiness: {error}"))?;

    loop {
        if deadline.is_some_and(|value| Instant::now() >= value) {
            break;
        }
        match endpoint.receive() {
            Ok(NativeLanReceiveOutcome::Packet(packet)) => {
                match ring
                    .try_write_s16le_mono(&packet.payload)
                    .map_err(|error| error.to_string())?
                {
                    CaptureWriteOutcome::Committed { frames } => {
                        packets_committed = packets_committed.saturating_add(1);
                        frames_committed = frames_committed.saturating_add(frames as u64);
                    }
                    CaptureWriteOutcome::DroppedFull { .. } => {
                        packets_dropped_full = packets_dropped_full.saturating_add(1);
                    }
                }
            }
            Ok(
                NativeLanReceiveOutcome::Pending
                | NativeLanReceiveOutcome::DuplicateFragment
                | NativeLanReceiveOutcome::DroppedWrongPeer
                | NativeLanReceiveOutcome::DroppedMalformed,
            ) => {}
            Err(capyio_native_audio_lan::NativeLanError::ReceiveTimeout) => {}
            Err(error) => return Err(error.to_string()),
        }
    }

    let network = endpoint.metrics();
    let capture = ring.metrics();
    println!(
        "bridge_complete=true packets_received={} datagrams_received={} wrong_peer={} malformed={} packets_committed={} packets_dropped_full={} frames_committed={} ring_produced={} ring_dropped={} producer_attaches={} consumer_attaches={} last_stage={} last_error={}",
        network.packets_received,
        network.datagrams_received,
        network.wrong_peer_datagrams,
        network.malformed_datagrams,
        packets_committed,
        packets_dropped_full,
        frames_committed,
        capture.produced_frames,
        capture.dropped_frames,
        capture.producer_attaches,
        capture.consumer_attaches,
        capture.last_stage,
        capture.last_error,
    );
    Ok(())
}

#[cfg(windows)]
fn parse_explicit_ipv4(value: &str, name: &str) -> Result<std::net::SocketAddr, String> {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    let address = value
        .parse::<SocketAddr>()
        .map_err(|_| format!("{name} must be an explicit IPv4 socket address"))?;
    let IpAddr::V4(ip) = address.ip() else {
        return Err(format!("{name} must use IPv4"));
    };
    if address.port() == 0 || ip.is_unspecified() || ip.is_multicast() || ip == Ipv4Addr::BROADCAST
    {
        return Err(format!(
            "{name} must be a concrete unicast IPv4 and non-zero port"
        ));
    }
    Ok(address)
}

#[cfg(test)]
mod tests {
    use capyio_audio::AudioStreamSpec;
    use capyio_native_audio_lan::microphone_lab_binding;

    #[test]
    fn microphone_binding_is_exact_voice_mono_pcm() {
        let binding = microphone_lab_binding();
        assert_eq!(binding.selected_spec, AudioStreamSpec::voice_interactive());
        assert_eq!(binding.selected_spec.format.sample_rate_hz, 48_000);
        assert_eq!(binding.selected_spec.format.channels, 1);
        assert_eq!(binding.selected_spec.format.frame_duration_micros, 10_000);
        binding.validate().unwrap();
    }
}
