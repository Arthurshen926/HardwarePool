#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-display-control-lab is only available on Windows");
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
    use capyio_windows_display::{
        IddDeviceController, IddMonitorConnectRequest, IddMonitorDisconnectRequest, IddMonitorMode,
    };

    let mut arguments = std::env::args();
    let _program = arguments.next();
    let operation = arguments.next().unwrap_or_else(|| "query".to_owned());

    match operation.as_str() {
        "query" => {
            require_no_more(arguments)?;
            let mut controller = IddDeviceController::open_unique()?;
            let status = controller.query()?;
            println!(
                "CAPYIO_ADAPTER maximum_monitors={} connected_monitors={}",
                status.maximum_monitors, status.connected_monitors
            );
        }
        "list" => {
            require_no_more(arguments)?;
            let mut controller = IddDeviceController::open_unique()?;
            for monitor in controller.enumerate_monitors()? {
                println!(
                    "CAPYIO_MONITOR identity={} connector_index={} target_id={} adapter_luid={:08x}:{:08x}",
                    monitor.identity.capability_id(),
                    monitor.connector_index,
                    monitor.target_id,
                    monitor.adapter_luid_high as u32,
                    monitor.adapter_luid_low
                );
            }
        }
        "connect" => {
            let identity = parse_identity(require_argument(&mut arguments, "capability-id")?)?;
            let width = parse_u32(require_argument(&mut arguments, "width")?, "width")?;
            let height = parse_u32(require_argument(&mut arguments, "height")?, "height")?;
            let refresh_millihz = parse_u32(
                require_argument(&mut arguments, "refresh-millihz")?,
                "refresh-millihz",
            )?;
            require_no_more(arguments)?;
            let mut controller = IddDeviceController::open_unique()?;
            let connected = controller.connect(IddMonitorConnectRequest {
                identity,
                preferred_mode: IddMonitorMode {
                    width,
                    height,
                    refresh_millihz,
                },
            })?;
            println!(
                "CAPYIO_MONITOR_CONNECTED identity={} connector_index={} target_id={} adapter_luid={:08x}:{:08x}",
                connected.identity.capability_id(),
                connected.connector_index,
                connected.target_id,
                connected.adapter_luid_high as u32,
                connected.adapter_luid_low
            );
        }
        "disconnect" => {
            let identity = parse_identity(require_argument(&mut arguments, "capability-id")?)?;
            require_no_more(arguments)?;
            let mut controller = IddDeviceController::open_unique()?;
            controller.disconnect(IddMonitorDisconnectRequest { identity })?;
            println!(
                "CAPYIO_MONITOR_DISCONNECTED identity={}",
                identity.capability_id()
            );
        }
        _ => {
            return Err(
                "operation must be query, list, connect <capability-id> <width> <height> <refresh-millihz>, or disconnect <capability-id>"
                    .into(),
            );
        }
    }
    Ok(())
}

#[cfg(windows)]
fn parse_identity(
    text: String,
) -> Result<capyio_windows_display::IddMonitorIdentity, Box<dyn std::error::Error>> {
    use capyio_core::ids::CapabilityId;
    use capyio_windows_display::IddMonitorIdentity;

    Ok(IddMonitorIdentity::new(text.parse::<CapabilityId>()?)?)
}

#[cfg(windows)]
fn require_argument(
    arguments: &mut impl Iterator<Item = String>,
    name: &'static str,
) -> Result<String, Box<dyn std::error::Error>> {
    arguments
        .next()
        .ok_or_else(|| format!("missing {name}").into())
}

#[cfg(windows)]
fn require_no_more(
    mut arguments: impl Iterator<Item = String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if arguments.next().is_some() {
        return Err("unexpected extra argument".into());
    }
    Ok(())
}

#[cfg(windows)]
fn parse_u32(text: String, name: &'static str) -> Result<u32, Box<dyn std::error::Error>> {
    text.parse::<u32>()
        .map_err(|_| format!("{name} must be an unsigned decimal integer").into())
}
