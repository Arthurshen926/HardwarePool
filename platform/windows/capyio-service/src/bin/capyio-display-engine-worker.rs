#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-display-engine-worker is only available on Windows");
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
        thread,
        time::{Duration, Instant},
    };

    use capyio_display_engine_host::{
        DisplayEngineLibrary, DisplayWorkerControlCommand, DisplayWorkerControlEvent,
        DisplayWorkerPipeName, connect_display_worker_start_client,
    };
    use capyio_windows_display::{
        DisplayOutputBindingError, IddMonitorConnectRequest, IddMonitorIdentity, IddMonitorMode,
        SudoVdaMonitorLease, VerifiedWindowsDisplayOutput, activate_owned_windows_display_output,
        configure_process_display_dpi_awareness, verify_current_process_display_session,
        verify_owned_windows_display_output,
    };

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
    let execution_session = verify_current_process_display_session(lease.windows_session_id)?;
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
    let mut display = SudoVdaMonitorLease::create(IddMonitorConnectRequest {
        identity,
        preferred_mode: mode,
    })?;
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
                | DisplayOutputBindingError::DesktopMonitorUnavailable,
            ) if !activation_attempted => {
                activate_owned_windows_display_output(monitor)?;
                activation_attempted = true;
            }
            Err(_error) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(Box::new(error)),
        }
    };
    if !display.watchdog_healthy() {
        return Err("SudoVDA watchdog failed before display engine start".into());
    }
    let mut session = library.start_verified_output_session(
        configuration.session(),
        &binding,
        execution_session,
        launch,
    )?;

    let connection_result = loop {
        match connection.wait_for_control(&lease) {
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
    connection_result?;
    Ok(())
}
