#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let path = arguments
        .next()
        .ok_or("usage: capyio-display-provisioning-properties <schema-3-config.json>")?;
    if arguments.next().is_some() {
        return Err("usage: capyio-display-provisioning-properties <schema-3-config.json>".into());
    }
    let properties =
        capyio_windows_service::android_gamestream_properties_from_single_display_config(
            std::path::Path::new(&path),
        )?;
    print!("{properties}");
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-display-provisioning-properties is Windows-only");
    std::process::exit(1);
}
