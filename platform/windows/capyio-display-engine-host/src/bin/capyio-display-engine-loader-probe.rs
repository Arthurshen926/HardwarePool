#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::Path;
    use std::{thread, time::Duration};

    use capyio_display_engine_host::{AuthenticatedEngineLaunch, DisplayEngineLibrary};
    use capyio_display_stream_adapter::{
        DisplayMode, DisplayOnlyPolicy, DisplayStreamSessionRequest, DynamicRange,
        ValidatedDisplayStreamSession, VideoCodec,
        engine_abi::{EngineEventKind, WindowsCaptureOutput},
        lifecycle::EngineLifecycleState,
    };

    let mut arguments = std::env::args_os().skip(1);
    let path = arguments.next().ok_or("missing absolute DLL path")?;
    let hash = arguments.next().ok_or("missing lowercase SHA-256")?;
    if arguments.next().is_some() {
        return Err("unexpected loader-probe argument".into());
    }
    let hash = hash.to_str().ok_or("SHA-256 must be UTF-8")?;
    let expected = decode_sha256(hash)?;
    let mut library = DisplayEngineLibrary::load_verified(Path::new(&path), expected)?;
    let session_request = DisplayStreamSessionRequest {
        adapter_instance_id: [3; 16],
        display_capability_id: [4; 16],
        mode: DisplayMode {
            width: 2_640,
            height: 1_216,
            refresh_millihertz: 120_000,
        },
        codec: VideoCodec::Hevc,
        dynamic_range: DynamicRange::Hdr10,
        bitrate_kbps: 45_000,
        // Vibepollo's GameStream base maps HTTPS at -5 and media/control/RTSP
        // at +9/+10/+21. The validated session derives all four.
        port_base: 50_000,
        policy: DisplayOnlyPolicy::CAPYIO,
    };
    let validated = ValidatedDisplayStreamSession::new(session_request)?;
    let capture_output = WindowsCaptureOutput::parse(r"\\.\DISPLAY2")?;
    let launch = AuthenticatedEngineLaunch::new(
        [3; 16],
        [4; 16],
        7,
        [0x42; 16],
        0x1234_5678,
        [0x10; 8],
        0x90ab_cdef,
    )?;
    let batch = {
        // SAFETY: this feature-gated executable deliberately tests only the
        // native ABI lifecycle with a fixture selector; it is not a production
        // capture authorization path.
        let mut session =
            unsafe { library.start_fixture_session(&validated, capture_output, launch)? };
        thread::sleep(Duration::from_millis(20));
        session.request_key_frame()?;
        if session.stop(100).is_ok() || session.lifecycle_state() != EngineLifecycleState::Running {
            return Err("injected stop timeout did not preserve running ownership".into());
        }
        session.request_key_frame()?;
        session.stop(1_000)?;
        thread::sleep(Duration::from_millis(20));
        let batch = session.drain_events();
        if batch.invalid != 0
            || batch.events.len() < 5
            || batch.events.last().map(|event| event.kind) != Some(EngineEventKind::Stopped as u32)
        {
            return Err("native callback batch failed identity/order/stop validation".into());
        }
        session.destroy(1_000)?;
        batch
    };
    println!(
        "verified_path={} max_sessions={} events={} dropped={} invalid={} timeout_preserved_owner=true",
        library.canonical_path().display(),
        library.probe().max_concurrent_sessions(),
        batch.events.len(),
        batch.dropped,
        batch.invalid
    );
    library.try_unload()?;
    Ok(())
}

#[cfg(windows)]
fn decode_sha256(value: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("SHA-256 must contain exactly 64 hexadecimal characters".into());
    }
    let mut decoded = [0_u8; 32];
    for (index, byte) in decoded.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(decoded)
}

#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-display-engine-loader-probe is Windows-only");
    std::process::exit(2);
}
