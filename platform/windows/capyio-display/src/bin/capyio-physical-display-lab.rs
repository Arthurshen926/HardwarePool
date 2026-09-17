#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-physical-display-lab is only available on Windows");
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
    use std::{thread, time::Duration};

    use capyio_windows_display::{
        DisplayPathCategory, inspect_windows_display_topology,
        preview_physical_display_suppression, suppress_active_physical_displays,
    };

    let mut arguments = std::env::args();
    let _program = arguments.next();
    let operation = arguments.next().unwrap_or_else(|| "preview".to_owned());
    match operation.as_str() {
        "preview" => {
            if arguments.next().is_some() {
                return Err("preview accepts no additional arguments".into());
            }
            let summary = preview_physical_display_suppression()?;
            print_summary("PREVIEW", &summary);
        }
        "suppress-for-seconds" => {
            let seconds = arguments
                .next()
                .ok_or("suppress-for-seconds requires a duration")?
                .parse::<u64>()?;
            if arguments.next().is_some() || !(5..=30).contains(&seconds) {
                return Err("suppression duration must be an integer inside 5..=30 seconds".into());
            }
            let mut lease = suppress_active_physical_displays()?;
            print_summary("SUPPRESSED", lease.summary());

            // Do not return from the inspection branch before restoration. A
            // failed during-state assertion is retained and reported only
            // after the exact suppression lease has attempted restoration.
            let during_result = inspect_windows_display_topology().and_then(|paths| {
                let active_physical = paths
                    .iter()
                    .filter(|path| path.active && path.category == DisplayPathCategory::Physical)
                    .count();
                let active_virtual = paths
                    .iter()
                    .filter(|path| path.active && path.category == DisplayPathCategory::Virtual)
                    .count();
                println!(
                    "DURING active_physical={active_physical} active_virtual={active_virtual}"
                );
                if active_physical != 0 || active_virtual == 0 {
                    return Err(
                        capyio_windows_display::DisplayTopologyError::SuppressionPostcondition {
                            operation: "verify the bounded suppression interval",
                        },
                    );
                }
                Ok(())
            });
            thread::sleep(Duration::from_secs(seconds));
            let restore_result = lease.restore();
            println!("RESTORE attempted=true success={}", restore_result.is_ok());
            restore_result?;
            during_result?;

            let after = inspect_windows_display_topology()?;
            let active_physical = after
                .iter()
                .filter(|path| path.active && path.category == DisplayPathCategory::Physical)
                .count();
            let active_virtual = after
                .iter()
                .filter(|path| path.active && path.category == DisplayPathCategory::Virtual)
                .count();
            println!("AFTER active_physical={active_physical} active_virtual={active_virtual}");
            if active_physical == 0 || active_virtual == 0 {
                return Err("restored topology is missing a physical or virtual display".into());
            }
        }
        _ => return Err("operation must be preview or suppress-for-seconds".into()),
    }
    Ok(())
}

#[cfg(windows)]
fn print_summary(label: &str, summary: &capyio_windows_display::PhysicalDisplaySuppressionSummary) {
    println!(
        "{label} suppressed={} retained={}",
        summary.suppressed.len(),
        summary.retained.len()
    );
    for path in &summary.suppressed {
        println!(
            "{label}_PHYSICAL display={:?} adapter={:?} monitor={:?}",
            path.gdi_device_name, path.adapter_description, path.monitor_friendly_name
        );
    }
    for path in &summary.retained {
        println!(
            "{label}_RETAINED display={:?} adapter={:?} monitor={:?} category={:?}",
            path.gdi_device_name,
            path.adapter_description,
            path.monitor_friendly_name,
            path.category
        );
    }
}
