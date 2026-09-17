#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{fs, net::Ipv4Addr, path::PathBuf, time::Duration};

    use capyio_display_engine_host::exchange_authenticated_engine_launch;
    use capyio_display_stream_adapter::{
        DisplayMode, DisplayOnlyPolicy, DisplayStreamSessionRequest, DynamicRange,
        ValidatedDisplayStreamSession, VideoCodec,
        launch_record::{GameStreamLaunchBinding, GameStreamLaunchSecrets},
    };
    use capyio_display_tls::{
        AuthenticatedGameStreamLaunchServer, DisplayTlsServerEndpoint, DisplayTlsServerIdentity,
    };

    let mut arguments = std::env::args_os().skip(1).map(PathBuf::from);
    let server_certificate = arguments.next().ok_or("missing server certificate")?;
    let server_key = arguments.next().ok_or("missing server key")?;
    let client_ca = arguments.next().ok_or("missing client CA")?;
    if arguments.next().is_some() {
        return Err("unexpected launch-fixture argument".into());
    }

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
    drop(launch);
    println!("authenticated_launch_ack=pass");
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

#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-gamestream-launch-fixture is Windows-only");
    std::process::exit(2);
}
