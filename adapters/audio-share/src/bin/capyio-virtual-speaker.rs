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
    use std::{env, net::SocketAddr, thread, time::Duration};

    use capyio_audio::AudioFormat;
    use capyio_audio_share_adapter::{
        AudioSharePrivateFormat, AudioShareTransport, AudioShareTransportConfig,
        AudioShareTransportError, RenderRingConsumer,
    };

    let mut args = env::args();
    let executable = args
        .next()
        .unwrap_or_else(|| "capyio-virtual-speaker".to_owned());
    let Some(bind) = args.next() else {
        return Err(format!("usage: {executable} <explicit-ipv4:port>"));
    };
    if args.next().is_some() {
        return Err(format!("usage: {executable} <explicit-ipv4:port>"));
    }
    let bind_address = bind
        .parse::<SocketAddr>()
        .map_err(|_| "bind address must be an explicit IPv4 socket address".to_owned())?;

    // Create the ring first: the APO opens it outside its real-time callback
    // when Windows starts a stream on CapyIO Speaker.
    let mut ring = RenderRingConsumer::create_baseline().map_err(|error| error.to_string())?;
    let format = AudioSharePrivateFormat::from_audio_format(&AudioFormat::speaker_baseline())
        .map_err(|error| error.to_string())?;
    let transport =
        AudioShareTransport::bind(AudioShareTransportConfig::local_lab(bind_address), format)
            .map_err(|error| error.to_string())?;
    let sender = transport.sender();
    let mut pcm = Vec::with_capacity(8 * 1024);
    println!(
        "listening={} endpoint=CapyIO Speaker format=s16le/48000/stereo ring=Local\\CapyIO.RenderRing.v1",
        transport.local_address()
    );

    loop {
        match ring.try_read_s16le(&mut pcm) {
            Ok(true) => match sender.try_send_pcm(&pcm) {
                Ok(()) | Err(AudioShareTransportError::QueueFull) => {}
                Err(error) => return Err(error.to_string()),
            },
            Ok(false) => thread::sleep(Duration::from_millis(2)),
            Err(error) => return Err(error.to_string()),
        }
    }
}
