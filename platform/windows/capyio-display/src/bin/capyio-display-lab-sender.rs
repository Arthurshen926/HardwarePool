#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-display-lab-sender is only available on Windows");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    if let Err(error) = run() {
        eprintln!("ERROR {error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use std::{
        env, fs,
        net::Ipv4Addr,
        path::PathBuf,
        thread,
        time::{Duration, Instant},
    };

    use capyio_core::{RouteId, SessionId, StreamId};
    use capyio_display::{DisplayCursorMode, DisplayStreamSpec};
    use capyio_display_tls::{
        AuthenticatedDisplayServer, DisplayTlsServerEndpoint, DisplayTlsServerIdentity,
    };
    use capyio_display_transport::{
        AvcLevel, AvcProfile, DisplayTransportBinding, EncodedDisplayCodec,
        EncodedDisplayStreamSpec, MAX_ACCESS_UNIT_BYTES,
    };
    use capyio_video::{FrameRate, VideoColorimetry, VideoPixelFormat};
    use capyio_windows_display::{
        IddFrameBrokerMapping, MediaFoundationDisplayPipeline, MediaFoundationH264EncoderConfig,
    };

    fn variable(name: &str) -> Result<String, Box<dyn std::error::Error>> {
        let value = env::var(name).map_err(|_| format!("missing environment variable {name}"))?;
        if value.is_empty() || value.len() > 1024 {
            return Err(format!("environment variable {name} is outside its bound").into());
        }
        Ok(value)
    }
    fn number<T: std::str::FromStr>(name: &str) -> Result<T, Box<dyn std::error::Error>>
    where
        T::Err: std::error::Error + 'static,
    {
        Ok(variable(name)?.parse()?)
    }
    fn credential(name: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let path = PathBuf::from(variable(name)?);
        let metadata = fs::metadata(&path)?;
        if metadata.len() == 0 || metadata.len() > 64 * 1024 {
            return Err(format!("credential {name} is outside its bound").into());
        }
        Ok(fs::read(path)?)
    }

    let address: Ipv4Addr = number("CAPYIO_DISPLAY_ADDRESS")?;
    let port = number("CAPYIO_DISPLAY_PORT")?;
    let width = number("CAPYIO_DISPLAY_WIDTH")?;
    let height = number("CAPYIO_DISPLAY_HEIGHT")?;
    let duration_seconds: u64 = number("CAPYIO_DISPLAY_DURATION_SECONDS")?;
    if !(1..=3600).contains(&duration_seconds) {
        return Err("CAPYIO_DISPLAY_DURATION_SECONDS must be inside 1..=3600".into());
    }
    let maximum_connections = env::var("CAPYIO_DISPLAY_MAX_CONNECTIONS")
        .ok()
        .map(|value| value.parse::<u32>())
        .transpose()?
        .unwrap_or(1);
    if !(1..=16).contains(&maximum_connections) {
        return Err("CAPYIO_DISPLAY_MAX_CONNECTIONS must be inside 1..=16".into());
    }
    let decoded = DisplayStreamSpec {
        width,
        height,
        frame_rate: FrameRate::fps_30(),
        pixel_format: VideoPixelFormat::Bgra8,
        colorimetry: VideoColorimetry::SrgbFull,
        cursor_mode: DisplayCursorMode::Composited,
        target_latency_millis: 50,
        maximum_latency_millis: 150,
        allow_frame_drop: true,
    };
    let avc_level = AvcLevel::minimum_for(&decoded)
        .ok_or("no supported AVC level covers the selected display mode")?;
    let nv12_bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(3))
        .map(|bytes_twice| bytes_twice / 2)
        .ok_or("selected display mode overflowed its encoder buffer bound")?;
    let maximum_access_unit_bytes = nv12_bytes
        .max(2 * 1024 * 1024)
        .checked_next_power_of_two()
        .filter(|bound| *bound <= u64::from(MAX_ACCESS_UNIT_BYTES))
        .and_then(|bound| u32::try_from(bound).ok())
        .ok_or("selected display mode exceeds the maximum access-unit bound")?;
    let selected = EncodedDisplayStreamSpec {
        decoded,
        codec: EncodedDisplayCodec::AvcAnnexB,
        avc_profile: AvcProfile::Main,
        avc_level,
        maximum_access_unit_bytes,
    };
    let binding = DisplayTransportBinding::new(
        variable("CAPYIO_DISPLAY_SESSION_ID")?.parse::<SessionId>()?,
        variable("CAPYIO_DISPLAY_ROUTE_ID")?.parse::<RouteId>()?,
        variable("CAPYIO_DISPLAY_STREAM_ID")?.parse::<StreamId>()?,
        number("CAPYIO_DISPLAY_STREAM_EPOCH")?,
        selected,
    )?;
    let identity = DisplayTlsServerIdentity {
        certificate_chain_der: vec![
            credential("CAPYIO_DISPLAY_SERVER_CERT")?,
            credential("CAPYIO_DISPLAY_CA_CERT")?,
        ],
        private_key_der: credential("CAPYIO_DISPLAY_SERVER_KEY")?,
        client_ca_der: credential("CAPYIO_DISPLAY_CA_CERT")?,
    };

    let mut mapping = IddFrameBrokerMapping::create()?;
    println!("MAPPING_READY");
    let server = AuthenticatedDisplayServer::bind(
        DisplayTlsServerEndpoint {
            address,
            port,
            io_timeout: Duration::from_secs(5),
        },
        identity,
        binding.clone(),
    )?;
    println!("LISTENING address={}", server.local_addr()?);
    let mut total_frames = 0_u64;
    let mut total_records = 0_u64;
    for connection in 1..=maximum_connections {
        println!("WAITING connection={connection}");
        let mut sender = server.accept()?;
        println!("TLS_AUTHENTICATED connection={connection}");
        let mut pipeline = MediaFoundationDisplayPipeline::new(
            binding.clone(),
            MediaFoundationH264EncoderConfig::default(),
        )?;
        let deadline = Instant::now() + Duration::from_secs(duration_seconds);
        let mut connection_frames = 0_u64;
        let mut connection_records = 0_u64;
        while Instant::now() < deadline {
            if let Some(frame) = mapping.try_read()? {
                match pipeline.encode_frame_to_sender(&frame, &mut sender) {
                    Ok(sent) => {
                        connection_records = connection_records
                            .checked_add(sent as u64)
                            .ok_or("connection record count exhausted")?;
                        connection_frames = connection_frames
                            .checked_add(1)
                            .ok_or("connection frame count exhausted")?;
                        if connection_frames.is_multiple_of(30) {
                            println!(
                                "PROGRESS connection={connection} frames={connection_frames} records={connection_records}"
                            );
                        }
                    }
                    Err(capyio_windows_display::MfDisplayPipelineError::Transport(_)) => {
                        println!(
                            "DISCONNECTED connection={connection} frames={connection_frames} records={connection_records}"
                        );
                        break;
                    }
                    Err(error) => return Err(error.into()),
                }
            } else {
                thread::sleep(Duration::from_millis(2));
            }
        }
        total_frames = total_frames
            .checked_add(connection_frames)
            .ok_or("total frame count exhausted")?;
        total_records = total_records
            .checked_add(connection_records)
            .ok_or("total record count exhausted")?;
        println!(
            "CONNECTION_COMPLETE connection={connection} frames={connection_frames} records={connection_records}"
        );
    }
    println!(
        "COMPLETE connections={maximum_connections} frames={total_frames} records={total_records}"
    );
    Ok(())
}
