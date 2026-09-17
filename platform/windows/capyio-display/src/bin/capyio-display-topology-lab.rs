#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-display-topology-lab is only available on Windows");
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
        activate_capyio_extended_path, inspect_capyio_display_path, validate_capyio_extended_path,
    };

    let mut arguments = std::env::args();
    let _program = arguments.next();
    let operation = arguments.next().unwrap_or_else(|| "inspect".to_owned());
    if arguments.next().is_some() {
        return Err("expected at most one operation argument".into());
    }
    match operation.as_str() {
        "inspect" => {
            let path = inspect_capyio_display_path()?;
            println!(
                "CAPYIO_PATH name={} active={} available={} source_id={} target_id={}",
                path.gdi_device_name,
                path.active,
                path.target_available,
                path.source_id,
                path.target_id
            );
        }
        "validate-extend" => {
            let status = validate_capyio_extended_path()?;
            println!("VALIDATED status={status}");
        }
        "apply-extend" => {
            let result = activate_capyio_extended_path()?;
            println!(
                "APPLIED validation_status={} apply_status={} mode_test_status={} mode_stage_status={} mode_apply_status={} before_active={} after_active={} name={}",
                result.validation_status,
                result.apply_status,
                result.mode_test_status,
                result.mode_stage_status,
                result.mode_apply_status,
                result.before.active,
                result.after.active,
                result.after.gdi_device_name
            );
        }
        _ => return Err("operation must be inspect, validate-extend or apply-extend".into()),
    }
    Ok(())
}
