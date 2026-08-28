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
    eprintln!("capyio-virtual-speaker is supported only on Windows");
    std::process::ExitCode::FAILURE
}

#[cfg(windows)]
fn windows_main() -> Result<(), String> {
    use std::{
        env,
        net::SocketAddr,
        thread,
        time::{Duration, Instant},
    };

    use capyio_audio::AudioStreamSpec;
    use capyio_audio_share_adapter::{
        AudioSharePrivateFormat, AudioShareTransport, AudioShareTransportConfig,
        AudioShareTransportError, RenderRingConsumer,
    };

    let mut args = env::args();
    let executable = args
        .next()
        .unwrap_or_else(|| "capyio-virtual-speaker".to_owned());
    let Some(bind) = args.next() else {
        return Err(format!(
            "usage: {executable} <explicit-ipv4:port> [duration-seconds]"
        ));
    };
    let duration = args
        .next()
        .map(|value| {
            value
                .parse::<u64>()
                .ok()
                .filter(|seconds| (1..=300).contains(seconds))
                .map(Duration::from_secs)
                .ok_or_else(|| "duration-seconds must be between 1 and 300".to_owned())
        })
        .transpose()?;
    if args.next().is_some() {
        return Err(format!(
            "usage: {executable} <explicit-ipv4:port> [duration-seconds]"
        ));
    }
    let bind_address = bind
        .parse::<SocketAddr>()
        .map_err(|_| "bind address must be an explicit IPv4 socket address".to_owned())?;

    // Create the ring first: the APO opens it outside its real-time callback
    // when Windows starts a stream on CapyIO Speaker.
    let mut ring = RenderRingConsumer::create_baseline().map_err(|error| error.to_string())?;
    let format = AudioSharePrivateFormat::from_stream_spec(&AudioStreamSpec::media_balanced())
        .map_err(|error| error.to_string())?;
    let transport =
        AudioShareTransport::bind(AudioShareTransportConfig::local_lab(bind_address), format)
            .map_err(|error| error.to_string())?;
    let sender = transport.sender();
    let mut pcm = Vec::with_capacity(8 * 1024);
    println!(
        "listening={} endpoint=CapyIO Speaker format=s16le/48000/stereo ring=Global\\CapyIO.RenderRing.v1",
        transport.local_address()
    );
    let deadline = duration.map(|value| Instant::now() + value);

    loop {
        if deadline.is_some_and(|value| Instant::now() >= value) {
            break;
        }
        match ring.try_read_s16le(&mut pcm) {
            Ok(true) => match sender.try_send_pcm(&pcm) {
                Ok(()) | Err(AudioShareTransportError::QueueFull) => {}
                Err(error) => return Err(error.to_string()),
            },
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
    let stats = transport.stats();
    println!(
        "bridge_complete=true ring_produced={} ring_dropped={} attach_attempts={} attach_successes={} attach_sample_rate={} attach_channels={} attach_stage={} attach_error={} blocks_enqueued={} queue_full={} blocks_without_receiver={} datagrams_sent={} datagram_send_errors={} pcm_bytes_sent={}",
        ring_produced,
        ring_dropped,
        attach_attempts,
        attach_successes,
        attach_sample_rate,
        attach_channels,
        attach_stage,
        attach_error,
        stats.blocks_enqueued,
        stats.queue_full,
        stats.blocks_without_receiver,
        stats.datagrams_sent,
        stats.datagram_send_errors,
        stats.pcm_bytes_sent,
    );
    transport.shutdown();
    Ok(())
}
