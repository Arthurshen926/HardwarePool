#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-display-session-probe is only available on Windows");
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
    use capyio_windows_display::{DisplaySessionContext, InteractiveDisplaySessionId};

    let context = DisplaySessionContext::query()?;
    println!(
        "DISPLAY_SESSION process={} active_console={}",
        context.process_session_id(),
        context
            .active_console_session_id()
            .map_or_else(|| "none".to_owned(), |value| value.to_string())
    );

    if let Some(raw) = std::env::args().nth(1) {
        if std::env::args().nth(2).is_some() {
            return Err("expected at most one authorized session ID".into());
        }
        let authorized = InteractiveDisplaySessionId::new(raw.parse()?)?;
        context.verify_authorized(authorized)?;
        println!("VERIFIED_DISPLAY_SESSION session={}", authorized.get());
    }
    Ok(())
}
