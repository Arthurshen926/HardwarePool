#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-sudovda-smoke is only available on Windows");
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

    use capyio_core::ids::CapabilityId;
    use capyio_windows_display::{
        DisplayOutputBindingError, IddMonitorConnectRequest, IddMonitorIdentity, IddMonitorMode,
        SudoVdaMonitorLease, activate_owned_windows_display_output,
        configure_process_display_dpi_awareness, verify_owned_windows_display_output,
    };

    configure_process_display_dpi_awareness()?;

    let protocol_only = match std::env::args().nth(1).as_deref() {
        None => false,
        Some("--protocol-only") => true,
        Some(_) => return Err("usage: capyio-sudovda-smoke [--protocol-only]".into()),
    };
    if std::env::args().nth(2).is_some() {
        return Err("usage: capyio-sudovda-smoke [--protocol-only]".into());
    }
    let capability_id = std::env::var("CAPYIO_SUDOVDA_SMOKE_CAPABILITY_ID")
        .unwrap_or_else(|_| "6e8916f4-c570-4e55-a4a5-c06678808f8d".to_owned());
    let identity = IddMonitorIdentity::new(capability_id.parse::<CapabilityId>()?)?;
    let mode = IddMonitorMode {
        width: 2640,
        height: 1216,
        refresh_millihz: 120_000,
    };
    let mut lease = SudoVdaMonitorLease::create(IddMonitorConnectRequest {
        identity,
        preferred_mode: mode,
    })?;
    let monitor = lease.monitor()?.as_idd_monitor();
    if protocol_only {
        thread::sleep(Duration::from_secs(2));
        println!(
            "SudoVDA protocol smoke monitor created: target={} luid={:08x}:{:08x}",
            monitor.target_id, monitor.adapter_luid_high as u32, monitor.adapter_luid_low,
        );
    } else {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut activation_attempted = false;
        let binding = loop {
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
        println!(
            "SudoVDA smoke display active: target={} luid={:08x}:{:08x} output={}",
            monitor.target_id,
            monitor.adapter_luid_high as u32,
            monitor.adapter_luid_low,
            binding.gdi_device_name()
        );
        if let Ok(value) = std::env::var("CAPYIO_SUDOVDA_SMOKE_HOLD_MILLIS") {
            let hold_millis = value.parse::<u64>()?;
            let mut reactivations_left = std::env::var("CAPYIO_SUDOVDA_SMOKE_REACTIVATE_LIMIT")
                .ok()
                .map(|value| value.parse::<u32>())
                .transpose()?
                .unwrap_or(0);
            let started = Instant::now();
            while started.elapsed() < Duration::from_millis(hold_millis) {
                thread::sleep(Duration::from_millis(250));
                if let Err(error) = verify_owned_windows_display_output(monitor, mode) {
                    if reactivations_left > 0 {
                        println!(
                            "SudoVDA output lost after {}ms; attempting bounded topology reactivation",
                            started.elapsed().as_millis()
                        );
                        activate_owned_windows_display_output(monitor)?;
                        reactivations_left -= 1;
                        continue;
                    }
                    return Err(format!(
                        "SudoVDA output lost after {}ms while the driver lease remained alive: {error}",
                        started.elapsed().as_millis()
                    )
                    .into());
                }
            }
        }
    }
    if !lease.watchdog_healthy() {
        return Err("SudoVDA watchdog failed during smoke test".into());
    }
    lease.release()?;
    println!("SudoVDA smoke display removed by exact Capability GUID");
    Ok(())
}
