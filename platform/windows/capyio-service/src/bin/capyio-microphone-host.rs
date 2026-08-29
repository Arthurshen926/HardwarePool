#[cfg(windows)]
mod windows_host {
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

    use capyio_micyou_host_config::load_trusted_host_config;
    use capyio_windows_service::{
        DEFAULT_PHONE_WAIT_POLLS, DEFAULT_STABLE_PHONE_POLLS, MicrophoneHostClient,
        MicrophoneHostRuntime, microphone_control_server_loop, wake_microphone_control_server,
    };

    const POLL_INTERVAL: Duration = Duration::from_millis(250);
    const MAX_BOUNDED_RUN: Duration = Duration::from_secs(600);

    pub fn main() -> Result<(), String> {
        let arguments = std::env::args().skip(1).collect::<Vec<_>>();
        match arguments.as_slice() {
            [] => run(None),
            [control, operation] if control == "--control" => run_control(operation),
            [run_for, milliseconds] if run_for == "--run-for-ms" => {
                let duration = Duration::from_millis(
                    milliseconds
                        .parse::<u64>()
                        .map_err(|_| usage().to_owned())?,
                );
                if duration.is_zero() || duration > MAX_BOUNDED_RUN {
                    return Err(usage().to_owned());
                }
                run(Some(duration))
            }
            _ => Err(usage().to_owned()),
        }
    }

    fn run_control(operation: &str) -> Result<(), String> {
        let client = MicrophoneHostClient::default();
        let snapshot = match operation {
            "status" => client.status()?,
            "start" => client.start()?,
            "stop" => client.stop()?,
            _ => return Err(usage().to_owned()),
        };
        println!(
            "{}",
            serde_json::to_string(&snapshot).map_err(|error| error.to_string())?
        );
        Ok(())
    }

    fn run(run_for: Option<Duration>) -> Result<(), String> {
        let loaded = load_trusted_host_config().map_err(|error| error.to_string())?;
        let adapter_config = loaded
            .config
            .adapter_config()
            .map_err(|error| error.to_string())?;
        let bind_address = adapter_config.bind_address();
        let supervisor = loaded
            .config
            .supervisor()
            .map_err(|error| error.to_string())?;
        let runtime = Arc::new(Mutex::new(MicrophoneHostRuntime::new(
            supervisor,
            bind_address,
            DEFAULT_STABLE_PHONE_POLLS,
            DEFAULT_PHONE_WAIT_POLLS,
        )?));
        let stop = Arc::new(AtomicBool::new(false));
        let control_runtime = Arc::clone(&runtime);
        let control_stop = Arc::clone(&stop);
        let control = thread::Builder::new()
            .name("capyio-microphone-control".to_owned())
            .spawn(move || microphone_control_server_loop(control_runtime, control_stop))
            .map_err(|_| "start microphone control thread".to_owned())?;

        let client = MicrophoneHostClient::default();
        let ready_deadline = Instant::now() + Duration::from_secs(5);
        while client.status().is_err() {
            if Instant::now() >= ready_deadline {
                stop.store(true, Ordering::Release);
                wake_microphone_control_server();
                let _ = control.join();
                return Err("microphone control pipe did not become ready".to_owned());
            }
            thread::sleep(Duration::from_millis(25));
        }

        let deadline = run_for.map(|duration| Instant::now() + duration);
        while !deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            runtime
                .lock()
                .map_err(|_| "microphone host state lock poisoned".to_owned())?
                .poll();
            thread::sleep(POLL_INTERVAL);
        }

        stop.store(true, Ordering::Release);
        wake_microphone_control_server();
        let control_result = control
            .join()
            .map_err(|_| "microphone control thread panicked".to_owned())?;
        let stop_result = runtime
            .lock()
            .map_err(|_| "microphone host state lock poisoned".to_owned())?
            .ensure_stopped()
            .map(|_| ());
        control_result.and(stop_result)
    }

    fn usage() -> &'static str {
        "usage: capyio-microphone-host [--run-for-ms 1..600000 | --control status|start|stop]"
    }
}

#[cfg(windows)]
fn main() -> Result<(), String> {
    windows_host::main()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-microphone-host is supported only on Windows");
}
