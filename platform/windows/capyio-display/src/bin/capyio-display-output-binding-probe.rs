#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-display-output-binding-probe is only available on Windows");
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
    use capyio_core::ids::CapabilityId;
    use capyio_windows_display::{
        IddMonitorIdentity, IddMonitorMode, verify_static_capyio_display_output,
    };

    let mut arguments = std::env::args().skip(1);
    let capability = arguments.next().ok_or("missing Capability UUID")?;
    let width = arguments.next().ok_or("missing width")?.parse::<u32>()?;
    let height = arguments.next().ok_or("missing height")?.parse::<u32>()?;
    let refresh_millihertz = arguments
        .next()
        .ok_or("missing refresh millihertz")?
        .parse::<u32>()?;
    if arguments.next().is_some() {
        return Err("unexpected output-binding probe argument".into());
    }
    let identity = IddMonitorIdentity::new(capability.parse::<CapabilityId>()?)?;
    let output = verify_static_capyio_display_output(
        identity,
        IddMonitorMode {
            width,
            height,
            refresh_millihz: refresh_millihertz,
        },
    )?;
    println!(
        "VERIFIED_STATIC_OUTPUT name={} capability={} width={} height={} refresh_millihertz={}",
        output.gdi_device_name(),
        output.identity().capability_id(),
        output.mode().width,
        output.mode().height,
        output.mode().refresh_millihz
    );
    Ok(())
}
