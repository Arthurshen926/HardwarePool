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
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use capyio_native_audio_lan::{
        MAX_NATIVE_LAN_INFLIGHT_PACKETS, NativeLanEndpointConfig, NativeLanReceiveOutcome,
        NativeLanUdpEndpoint, NativeMicrophoneLaunch,
    };
    use capyio_windows_capture_ring::{CaptureRingProducer, CaptureWriteOutcome};

    let wall_ms = || -> Result<u64, String> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "invalid local clock".to_owned())?
            .as_millis()
            .try_into()
            .map_err(|_| "local clock overflow".to_owned())
    };
    let args = env::args().skip(1).take(9).collect::<Vec<_>>();
    let launch = NativeMicrophoneLaunch::parse(&args, wall_ms()?)?;
    let deadline = launch.deadline(wall_ms()?, Instant::now())?;
    let peer = launch.peer;
    let mut endpoint = NativeLanUdpEndpoint::bind(
        launch.local.into(),
        NativeLanEndpointConfig {
            peer: peer.into(),
            read_timeout: Duration::from_millis(50),
            inflight_packet_capacity: MAX_NATIVE_LAN_INFLIGHT_PACKETS,
        },
        launch.binding.clone(),
    )
    .map_err(|error| error.to_string())?;
    let mut ring = CaptureRingProducer::attach().map_err(|error| error.to_string())?;
    if !launch.is_live(deadline, wall_ms()?, Instant::now()) {
        return Err("audio grant ended during worker startup".into());
    }
    let mut packets_committed = 0_u64;
    let mut packets_dropped_full = 0_u64;
    let mut frames_committed = 0_u64;
    println!(
        "{} local={} peer={} format=s16le/48000/mono ring=Global\\CapyIO.CaptureRing.v1",
        if launch.expires_at_ms.is_some() {
            "native_virtual_microphone_route_v1=true"
        } else {
            "native_virtual_microphone=true"
        },
        endpoint.local_addr().map_err(|error| error.to_string())?,
        peer
    );
    std::io::stdout()
        .flush()
        .map_err(|error| format!("flush native microphone readiness: {error}"))?;

    loop {
        if !launch.is_live(deadline, wall_ms()?, Instant::now()) {
            break;
        }
        match endpoint.receive() {
            Ok(NativeLanReceiveOutcome::Packet(packet)) => {
                // A packet can arrive as a blocking receive crosses grant expiry.
                if !launch.is_live(deadline, wall_ms()?, Instant::now()) {
                    break;
                }
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
