fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("linux_lab_playback_error={error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn run() -> Result<(), String> {
    Err("Linux playback is supported only on Linux".into())
}

#[cfg(target_os = "linux")]
fn run() -> Result<(), String> {
    use capyio_linux_audio::{PipeWirePlayback, PlaybackConfig, PlaybackPump, observe_owner_stop};
    use capyio_native_audio_lan::{
        NativeLanEndpointConfig, NativeLanError, NativeLanReceiveOutcome, NativeLanUdpEndpoint,
        microphone_lab_binding,
    };
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant},
    };
    // Inherited stdin is lifecycle only: EOF or any byte means Stop. One
    // bounded reader owns no media/process handles; process exit ends a still
    // blocked reader. It never delays child cleanup or the finite deadline.
    let stopped_by_owner = Arc::new(AtomicBool::new(false));

    // Bound retained argument count; parser rejects extra arguments.
    let args: Vec<String> = std::env::args().skip(1).take(6).collect();
    let config = PlaybackConfig::parse(&args).map_err(str::to_owned)?;
    let reader_stop = Arc::clone(&stopped_by_owner);
    std::thread::Builder::new()
        .name("linux-audio-stop".into())
        .spawn(move || {
            observe_owner_stop(&mut std::io::stdin(), &reader_stop);
        })
        .map_err(|error| error.to_string())?;
    let start = Instant::now();
    let deadline = start + config.duration;
    let mut endpoint = NativeLanUdpEndpoint::bind(
        config.local.into(),
        NativeLanEndpointConfig {
            peer: config.peer.into(),
            read_timeout: Duration::from_millis(10),
            inflight_packet_capacity: 2,
        },
        microphone_lab_binding(),
    )
    .map_err(|error| error.to_string())?;
    let mut sink = PipeWirePlayback::open(&config.target).map_err(|error| error.to_string())?;
    let mut pump = PlaybackPump::new(config.duration).map_err(|error| error.to_string())?;
    eprintln!(
        "linux_lab_playback_started=true authenticated=false virtual_microphone=false audible_playback_verified=false"
    );
    let outcome = (|| -> Result<(), String> {
        while Instant::now() < deadline && !stopped_by_owner.load(Ordering::Acquire) {
            pump.flush(&mut sink, start.elapsed())
                .map_err(|error| error.to_string())?;
            let packet = match endpoint.receive() {
                Ok(NativeLanReceiveOutcome::Packet(packet)) => Some(packet),
                Ok(_) | Err(NativeLanError::ReceiveTimeout) => None,
                Err(error) => return Err(error.to_string()),
            };
            // Blocking receive can cross the deadline. Never forward then.
            if Instant::now() >= deadline || stopped_by_owner.load(Ordering::Acquire) {
                break;
            }
            if let Some(packet) = packet {
                pump.accept(&packet, start.elapsed())
                    .map_err(|error| error.to_string())?;
            }
            pump.flush(&mut sink, start.elapsed())
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    })();
    let stopped = sink.stop().map_err(|error| error.to_string());
    let metrics = pump.metrics();
    let network = endpoint.metrics();
    eprintln!(
        "linux_lab_playback_ended=true packets_received={} packets_forwarded_to_pipe={} pcm_bytes_forwarded={} stale={} malformed={} dropped_busy={} wrong_peer={} audible_playback_verified=false",
        network.packets_received,
        metrics.packets_written,
        metrics.bytes_written,
        metrics.stale_packets,
        network
            .malformed_datagrams
            .saturating_add(metrics.malformed_packets),
        metrics.packets_dropped_busy,
        network.wrong_peer_datagrams
    );
    // Cleanup failure takes precedence and cannot be hidden by a media failure.
    stopped?;
    outcome?;
    if metrics.packets_written == 0 {
        return Err("no complete PCM packet was forwarded".into());
    }
    Ok(())
}
