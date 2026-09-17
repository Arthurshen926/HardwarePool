#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use capyio_windows_service::{DisplayControlOperation, DisplayServiceClient};

    let mut arguments = std::env::args().skip(1);
    let operation = match arguments.next().as_deref() {
        Some("status") => DisplayControlOperation::Status,
        Some("start") => DisplayControlOperation::Start,
        Some("stop") => DisplayControlOperation::Stop,
        _ => return Err("usage: capyio-display-control <status|start|stop>".into()),
    };
    if arguments.next().is_some() {
        return Err("usage: capyio-display-control <status|start|stop>".into());
    }
    let snapshot = DisplayServiceClient::default().invoke(operation)?;
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-display-control is Windows-only");
    std::process::exit(1);
}
