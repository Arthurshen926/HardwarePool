#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-display-worker-session-fixture is only available on Windows");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    if let Err(error) = run(std::env::args_os().skip(1)) {
        eprintln!("ERROR {error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn run(
    arguments: impl IntoIterator<Item = std::ffi::OsString>,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use capyio_display_engine_host::{
        AuthenticatedEngineLaunch, DisplayWorkerEngineConfigRecord, DisplayWorkerLease,
        DisplayWorkerPipeName,
    };
    use capyio_display_stream_adapter::{
        DisplayMode, DisplayOnlyPolicy, DisplayStreamSessionRequest, DynamicRange,
        ValidatedDisplayStreamSession, VideoCodec, launch_record::GameStreamLaunchBinding,
    };
    use capyio_windows_display::InteractiveDisplaySessionId;
    use capyio_windows_service::{
        BoundDisplayWorkerChannel, VerifiedDisplayWorkerExecutable,
        verify_local_system_service_context,
    };

    let config = FixtureConfig::parse(arguments)?;
    let authorized_session = InteractiveDisplaySessionId::new(config.authorized_session)?;
    let executable =
        VerifiedDisplayWorkerExecutable::open(&config.worker_executable, config.worker_sha256)?;
    if config.mode == FixtureMode::Preflight {
        println!(
            "{}",
            serde_json::json!({
                "schemaVersion": 1,
                "mode": "preflight",
                "workerHashVerified": true,
                "authorizedSession": authorized_session.get(),
                "systemStateChanged": false
            })
        );
        return Ok(());
    }

    verify_local_system_service_context()?;
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let pipe_name = DisplayWorkerPipeName::parse(&format!(
        r"\\.\pipe\CapyIO.DisplayWorker.Fixture.{}.{suffix}",
        std::process::id()
    ))?;
    let mut channel = BoundDisplayWorkerChannel::launch(
        &executable,
        &pipe_name,
        authorized_session,
        Duration::from_secs(5),
    )?;
    let worker_process_id = channel.process_id();
    let session = ValidatedDisplayStreamSession::new(DisplayStreamSessionRequest {
        adapter_instance_id: [0xA1; 16],
        display_capability_id: [0xC1; 16],
        mode: DisplayMode {
            width: 1920,
            height: 1080,
            refresh_millihertz: 60_000,
        },
        codec: VideoCodec::Hevc,
        dynamic_range: DynamicRange::Sdr8,
        bitrate_kbps: 30_000,
        port_base: 50_000,
        policy: DisplayOnlyPolicy::CAPYIO,
    })?;
    let lease = DisplayWorkerLease::new(
        1,
        authorized_session,
        &session,
        GameStreamLaunchBinding {
            session_id: [0x51; 16],
            route_id: [0x52; 16],
            stream_id: [0x53; 16],
            stream_epoch: 1,
            host_ipv4: std::net::Ipv4Addr::LOCALHOST,
        },
    )?;
    channel.exchange_lease(&lease, Duration::from_secs(5))?;
    let engine_configuration = DisplayWorkerEngineConfigRecord::new(
        &lease,
        std::path::Path::new(r"C:\CapyIO\capyio-display-engine.dll"),
        [0x31; 32],
        &session,
    )?;
    channel.exchange_configuration(&engine_configuration, Duration::from_secs(5))?;
    let start = AuthenticatedEngineLaunch::new(
        session.request().adapter_instance_id,
        session.request().display_capability_id,
        1,
        [0xA5; 16],
        2,
        [0x5A; 8],
        3,
    )?
    .into_worker_start_record(&lease)?;
    channel.exchange_start(start, Duration::from_secs(5))?;
    channel.wait_for_exit(Duration::from_secs(2))?;
    if channel.is_running()? {
        return Err("display worker remained running after fixture lease".into());
    }
    println!(
        "{}",
        serde_json::json!({
            "schemaVersion": 1,
            "mode": "execute",
            "callerSid": "S-1-5-18",
            "callerSession": 0,
            "authorizedSession": authorized_session.get(),
            "workerProcessId": worker_process_id,
            "workerHashVerified": true,
            "leaseAcknowledged": true,
            "engineConfigurationAcknowledged": true,
            "secretStartAcknowledged": true,
            "workerExited": true,
            "driverChanged": false,
            "displayTopologyChanged": false
        })
    );
    Ok(())
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FixtureMode {
    Preflight,
    Execute,
}

#[cfg(windows)]
struct FixtureConfig {
    mode: FixtureMode,
    worker_executable: std::path::PathBuf,
    worker_sha256: [u8; 32],
    authorized_session: u32,
}

#[cfg(windows)]
impl FixtureConfig {
    fn parse(
        arguments: impl IntoIterator<Item = std::ffi::OsString>,
    ) -> Result<Self, &'static str> {
        let mut arguments = arguments.into_iter();
        let mode = match arguments.next().and_then(|value| value.into_string().ok()) {
            Some(value) if value == "--preflight" => FixtureMode::Preflight,
            Some(value) if value == "--execute-local-system-fixture" => FixtureMode::Execute,
            _ => return Err("expected --preflight or --execute-local-system-fixture"),
        };
        if arguments
            .next()
            .and_then(|value| value.into_string().ok())
            .as_deref()
            != Some("--worker-exe")
        {
            return Err("expected --worker-exe <absolute path>");
        }
        let worker_executable = arguments
            .next()
            .map(std::path::PathBuf::from)
            .ok_or("missing worker executable")?;
        if arguments
            .next()
            .and_then(|value| value.into_string().ok())
            .as_deref()
            != Some("--worker-sha256")
        {
            return Err("expected --worker-sha256 <64 lowercase hex characters>");
        }
        let worker_sha256 = arguments
            .next()
            .and_then(|value| value.into_string().ok())
            .ok_or("missing worker SHA-256")?;
        let worker_sha256 = parse_sha256(&worker_sha256)?;
        if arguments
            .next()
            .and_then(|value| value.into_string().ok())
            .as_deref()
            != Some("--authorized-session")
        {
            return Err("expected --authorized-session <nonzero decimal ID>");
        }
        let authorized_session = arguments
            .next()
            .and_then(|value| value.into_string().ok())
            .ok_or("missing authorized Session")?
            .parse()
            .map_err(|_| "invalid authorized Session")?;
        if arguments.next().is_some() {
            return Err("unexpected fixture argument");
        }
        Ok(Self {
            mode,
            worker_executable,
            worker_sha256,
            authorized_session,
        })
    }
}

#[cfg(windows)]
fn parse_sha256(value: &str) -> Result<[u8; 32], &'static str> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("worker SHA-256 must be 64 lowercase hex characters");
    }
    let mut result = [0_u8; 32];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| "invalid worker SHA-256")?;
    }
    Ok(result)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn fixture_arguments_are_closed_and_hash_is_lowercase() {
        let config = FixtureConfig::parse([
            OsString::from("--preflight"),
            OsString::from("--worker-exe"),
            OsString::from(r"C:\\CapyIO\\worker.exe"),
            OsString::from("--worker-sha256"),
            OsString::from("01".repeat(32)),
            OsString::from("--authorized-session"),
            OsString::from("7"),
        ])
        .unwrap();
        assert_eq!(config.mode, FixtureMode::Preflight);
        assert_eq!(config.worker_sha256, [1; 32]);
        assert_eq!(config.authorized_session, 7);

        assert!(parse_sha256(&"A1".repeat(32)).is_err());
        assert!(FixtureConfig::parse([OsString::from("--execute")]).is_err());
    }
}
