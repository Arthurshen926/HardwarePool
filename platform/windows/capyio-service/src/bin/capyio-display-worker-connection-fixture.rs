#[cfg(not(windows))]
fn main() {
    eprintln!("capyio display worker connection fixture is only available on Windows");
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
    use capyio_display_engine_host::{
        DisplayWorkerControlCommand, DisplayWorkerControlEvent, DisplayWorkerPipeName,
        connect_display_worker_start_client,
    };

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
    let (mut connection, lease, _configuration, _launch) = prepared.into_parts();
    loop {
        match connection.wait_for_control(&lease)? {
            DisplayWorkerControlEvent::Disconnected => break,
            DisplayWorkerControlEvent::Command { command, record } => {
                connection.acknowledge_control(&lease, &record)?;
                if command == DisplayWorkerControlCommand::GracefulStop {
                    break;
                }
            }
        }
    }
    Ok(())
}
