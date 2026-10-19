#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-display-engine-worker is only available on Windows");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    let stage = std::cell::Cell::new(
        capyio_windows_service::display_worker_failure::DisplayWorkerStage::Handshake,
    );
    if let Err(error) = run(&stage) {
        eprintln!("ERROR {error}");
        // Startup failures occur off the media callback. Keep one bounded local
        // diagnostic beside the deployed worker; never serialize launch material.
        if let Ok(exe) = std::env::current_exe() {
            let detail: String = error.to_string().chars().take(1024).collect();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let _ = std::fs::write(
                exe.with_file_name("display-worker-start-error.txt"),
                format!("timeMs={now} stage={} detail={detail}", stage.get() as i32),
            );
        }
        std::process::exit(stage.get() as i32);
    }
}

#[cfg(windows)]
fn enter_stage(
    stage: &std::cell::Cell<capyio_windows_service::display_worker_failure::DisplayWorkerStage>,
    next: capyio_windows_service::display_worker_failure::DisplayWorkerStage,
) {
    stage.set(next);
    if let Ok(exe) = std::env::current_exe() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let _ = std::fs::write(
            exe.with_file_name("display-worker-stage.txt"),
            format!(
                "timeMs={now} pid={} stage={}",
                std::process::id(),
                next as i32
            ),
        );
    }
}

#[cfg(windows)]
fn record_binding_failure(error: &capyio_windows_display::DisplayOutputBindingError) {
    if let Ok(exe) = std::env::current_exe() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let _ = std::fs::write(
            exe.with_file_name("display-worker-binding-error.txt"),
            format!("timeMs={now} pid={} error={error}", std::process::id()),
        );
    }
}

#[cfg(windows)]
fn run(
    stage: &std::cell::Cell<capyio_windows_service::display_worker_failure::DisplayWorkerStage>,
) -> Result<(), Box<dyn std::error::Error>> {
    use capyio_windows_service::display_worker_failure::DisplayWorkerStage;
    use std::{
        thread,
        time::{Duration, Instant},
    };

    use capyio_display_engine_host::{
        DisplayEngineLibrary, DisplayWorkerControlCommand, DisplayWorkerControlEvent,
        DisplayWorkerPipeName, connect_display_worker_start_client,
    };
    use capyio_windows_display::{
        DisplayOutputBindingError, IddMonitorConnectRequest, IddMonitorIdentity, IddMonitorMode,
        SudoVdaMonitorLease, VerifiedWindowsDisplayOutput, configure_owned_windows_display_output,
        configure_process_display_dpi_awareness, verify_current_process_display_session,
        verify_owned_windows_display_output,
    };

    enter_stage(stage, DisplayWorkerStage::Handshake);
    configure_process_display_dpi_awareness()?;

    let mut arguments = std::env::args().skip(1);
    if arguments.next().as_deref() != Some("--lease-pipe") {
        return Err("expected --lease-pipe <CapyIO pipe name>".into());
    }
    let pipe =
        DisplayWorkerPipeName::parse(&arguments.next().ok_or("missing CapyIO worker pipe name")?)?;
    if arguments.next().is_some() {
        return Err("unexpected display worker argument".into());
    }
    let prepared = connect_display_worker_start_client(&pipe)?;
    let (mut connection, lease, configuration, launch) = prepared.into_parts();
    enter_stage(stage, DisplayWorkerStage::Session);
    let execution_session = verify_current_process_display_session(lease.windows_session_id)?;
    enter_stage(stage, DisplayWorkerStage::EngineLoad);
    let mut library = DisplayEngineLibrary::load_verified(
        configuration.library_path(),
        configuration.expected_sha256(),
    )?;
    let request = configuration.session().request();
    let identity = IddMonitorIdentity::from_capability_bytes(request.display_capability_id)?;
    let mode = IddMonitorMode {
        width: request.mode.width,
        height: request.mode.height,
        refresh_millihz: request.mode.refresh_millihertz,
    };
    enter_stage(stage, DisplayWorkerStage::MonitorCreate);
    let mut display = SudoVdaMonitorLease::create(IddMonitorConnectRequest {
        identity,
        preferred_mode: mode,
    })?;
    enter_stage(stage, DisplayWorkerStage::OutputBind);
    let monitor = display.monitor()?.as_idd_monitor();
    // SudoVDA returns its stable adapter/target identity before Windows has
    // necessarily committed the new target to the active desktop topology.
    // The physical lab host has taken more than five seconds to publish the
    // active path after a prior monitor removal, so keep the worker bounded
    // but allow the same slow Plug-and-Play path that the standalone smoke
    // test exercises.
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut activation_attempted = false;
    let binding: VerifiedWindowsDisplayOutput = loop {
        match verify_owned_windows_display_output(monitor, mode) {
            Ok(binding) => break binding,
            Err(
                DisplayOutputBindingError::InactiveOwnedPath
                | DisplayOutputBindingError::DesktopMonitorUnavailable
                | DisplayOutputBindingError::ModeMismatch,
            ) if !activation_attempted => {
                if let Err(error) = configure_owned_windows_display_output(monitor, mode) {
                    record_binding_failure(&error);
                    return Err(Box::new(error));
                }
                activation_attempted = true;
            }
            Err(_error) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => {
                record_binding_failure(&error);
                return Err(Box::new(error));
            }
        }
    };
    if !display.watchdog_healthy() {
        return Err("SudoVDA watchdog failed before display engine start".into());
    }
    enter_stage(stage, DisplayWorkerStage::EngineStart);
    let mut session = library.start_verified_output_session(
        configuration.session(),
        &binding,
        execution_session,
        launch,
    )?;

    enter_stage(stage, DisplayWorkerStage::Streaming);
    let mut next_output_check = Instant::now() + Duration::from_secs(1);
    let mut output_missing_since = None;
    let mut display_mode_changed = false;
    let connection_result = loop {
        match connection.wait_for_control_checked(&lease, || {
            let now = Instant::now();
            if now < next_output_check {
                return true;
            }
            next_output_check = now + Duration::from_secs(1);
            let verified = verify_owned_windows_display_output(monitor, mode);
            if matches!(verified, Err(DisplayOutputBindingError::ModeMismatch)) {
                display_mode_changed = true;
                record_binding_failure(&DisplayOutputBindingError::ModeMismatch);
                enter_stage(stage, DisplayWorkerStage::DisplayModeChanged);
                return false;
            }
            if display.watchdog_healthy() && verified.is_ok_and(|current| current == binding) {
                output_missing_since = None;
                true
            } else {
                let since = *output_missing_since.get_or_insert(now);
                now.duration_since(since) < Duration::from_secs(3)
            }
        }) {
            Ok(DisplayWorkerControlEvent::Disconnected) => break Ok(()),
            Ok(DisplayWorkerControlEvent::Command { command, record }) => match command {
                DisplayWorkerControlCommand::RequestKeyFrame => {
                    session.request_key_frame()?;
                    connection.acknowledge_control(&lease, &record)?;
                }
                DisplayWorkerControlCommand::GracefulStop => {
                    session.shutdown(5_000)?;
                    connection.acknowledge_control(&lease, &record)?;
                    break Ok(());
                }
            },
            Err(error) => break Err(error),
        }
    };
    let failed_while_streaming = connection_result.is_err();
    enter_stage(stage, DisplayWorkerStage::Cleanup);
    let shutdown_result = session.shutdown(5_000);
    drop(session);
    shutdown_result?;
    // The mature engine can publish a delayed display-topology cleanup while
    // its library unloads. Complete that teardown before the SudoVDA lease
    // restores the topology captured before monitor creation, so CapyIO owns
    // the final topology write instead of having it overwritten afterward.
    library.try_unload()?;
    // The engine and Windows display stack can publish their final topology
    // cleanup asynchronously after library shutdown. Keep the interactive
    // worker alive for a bounded settle interval, then let the SudoVDA lease
    // perform the final monitor removal and captured-topology restoration.
    thread::sleep(Duration::from_secs(5));
    display.release()?;
    if failed_while_streaming {
        enter_stage(
            stage,
            if display_mode_changed {
                DisplayWorkerStage::DisplayModeChanged
            } else {
                DisplayWorkerStage::Streaming
            },
        );
    }
    connection_result?;
    Ok(())
}
