#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{
        fs,
        net::Ipv4Addr,
        path::PathBuf,
        thread,
        time::{Duration, Instant},
    };

    use capyio_display_engine_host::{DisplayEngineLibrary, exchange_authenticated_engine_launch};
    use capyio_display_stream_adapter::{
        DisplayMode, DisplayOnlyPolicy, DisplayStreamSessionRequest, DynamicRange,
        ValidatedDisplayStreamSession, VideoCodec,
        launch_record::{GameStreamLaunchBinding, GameStreamLaunchSecrets},
    };
    use capyio_display_tls::{
        AuthenticatedGameStreamLaunchServer, DisplayTlsServerEndpoint, DisplayTlsServerIdentity,
    };
    use capyio_windows_display::{
        InteractiveDisplaySessionId, verify_current_process_display_session,
    };

    let mut arguments = std::env::args_os().skip(1);
    let engine_path = PathBuf::from(arguments.next().ok_or("missing engine DLL")?);
    let expected_hash = parse_sha256(
        arguments
            .next()
            .ok_or("missing engine SHA-256")?
            .to_string_lossy()
            .as_ref(),
    )?;
    let server_certificate = PathBuf::from(arguments.next().ok_or("missing server certificate")?);
    let server_key = PathBuf::from(arguments.next().ok_or("missing server key")?);
    let client_ca = PathBuf::from(arguments.next().ok_or("missing client CA")?);
    let authorized_session = arguments
        .next()
        .ok_or("missing authorized Windows session")?
        .to_string_lossy()
        .parse::<u32>()?;
    if arguments.next().is_some() {
        return Err("unexpected current-session fixture argument".into());
    }

    let execution_session = verify_current_process_display_session(
        InteractiveDisplaySessionId::new(authorized_session)?,
    )?;
    println!("verified_windows_session={authorized_session}");

    let server = AuthenticatedGameStreamLaunchServer::bind(
        DisplayTlsServerEndpoint {
            address: Ipv4Addr::new(100, 66, 231, 100),
            port: 7543,
            io_timeout: Duration::from_secs(5),
        },
        DisplayTlsServerIdentity {
            certificate_chain_der: vec![fs::read(server_certificate)?],
            private_key_der: fs::read(server_key)?,
            client_ca_der: fs::read(client_ca)?,
        },
    )?;
    println!("launch_listener={}", server.local_addr()?);

    let session = ValidatedDisplayStreamSession::new(DisplayStreamSessionRequest {
        adapter_instance_id: identity(0x01, 0x10),
        display_capability_id: identity(0x01, 0x04),
        mode: DisplayMode {
            width: 1920,
            height: 1080,
            refresh_millihertz: 60_000,
        },
        codec: VideoCodec::Hevc,
        dynamic_range: DynamicRange::Sdr8,
        bitrate_kbps: 45_000,
        port_base: 50_000,
        policy: DisplayOnlyPolicy::CAPYIO,
    })?;
    let binding = GameStreamLaunchBinding {
        session_id: identity(0x01, 0x01),
        route_id: identity(0x01, 0x02),
        stream_id: identity(0x01, 0x03),
        stream_epoch: 3,
        host_ipv4: Ipv4Addr::new(100, 66, 231, 100),
    };
    let launch = exchange_authenticated_engine_launch(
        &server,
        &session,
        binding,
        GameStreamLaunchSecrets {
            launch_session_id: 7,
            key: [0x42; 16],
            key_id: 0x1234_5678,
            ping_payload: [0x10; 8],
            control_connect_data: 0x90ab_cdef,
        },
    )?;
    println!("authenticated_launch_ack=pass");

    let mut library = DisplayEngineLibrary::load_verified(&engine_path, expected_hash)?;
    println!(
        "engine_probe=features:{};max_sessions:{}",
        library.probe().enabled_features(),
        library.probe().max_concurrent_sessions()
    );
    let mut engine = library.start_static_session(&session, execution_session, launch)?;
    println!("engine_state={:?}", engine.lifecycle_state());

    let deadline = Instant::now() + Duration::from_secs(25);
    while Instant::now() < deadline {
        let batch = engine.drain_events();
        for event in batch.events {
            println!(
                "engine_event=kind:{};status:{};sequence:{};value:{}",
                event.kind, event.status, event.sequence, event.value
            );
        }
        if batch.dropped != 0 || batch.invalid != 0 {
            println!(
                "engine_event_counters=dropped:{};invalid:{}",
                batch.dropped, batch.invalid
            );
        }
        thread::sleep(Duration::from_millis(100));
    }

    engine.shutdown(5_000)?;
    drop(engine);
    library.try_unload()?;
    println!("engine_shutdown=pass");
    Ok(())
}

#[cfg(windows)]
fn identity(penultimate: u8, last: u8) -> [u8; 16] {
    let mut value = [0_u8; 16];
    value[6] = 0x40;
    value[8] = 0x80;
    value[14] = penultimate;
    value[15] = last;
    value
}

#[cfg(windows)]
fn parse_sha256(value: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("engine SHA-256 must contain exactly 64 hexadecimal characters".into());
    }
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(output)
}

#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-gamestream-current-session-fixture is Windows-only");
    std::process::exit(2);
}
