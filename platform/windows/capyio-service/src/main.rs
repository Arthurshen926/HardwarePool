#[cfg(windows)]
mod windows_host {
    use std::{
        ffi::OsString,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

    use capyio_windows_service::{
        BrokerServiceRuntime, DEFAULT_POLL_INTERVAL, DEFAULT_STABLE_RECEIVER_POLLS, SERVICE_NAME,
        ServiceConfig,
    };
    use windows_service::{
        define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
    };

    define_windows_service!(ffi_service_main, service_main);

    pub fn main() -> Result<(), String> {
        let arguments = std::env::args().collect::<Vec<_>>();
        if arguments.iter().any(|argument| argument == "--console") {
            let config = ServiceConfig::parse(arguments).map_err(|error| error.to_string())?;
            return run_broker(config, Arc::new(AtomicBool::new(false)), true);
        }
        service_dispatcher::start(SERVICE_NAME, ffi_service_main).map_err(|error| error.to_string())
    }

    fn service_main(_arguments: Vec<OsString>) {
        let _ = run_service();
    }

    fn run_service() -> Result<(), String> {
        let stop = Arc::new(AtomicBool::new(false));
        let handler_stop = Arc::clone(&stop);
        let status_handle =
            service_control_handler::register(SERVICE_NAME, move |control| match control {
                ServiceControl::Stop => {
                    handler_stop.store(true, Ordering::Release);
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            })
            .map_err(|error| error.to_string())?;

        let config = ServiceConfig::parse(std::env::args()).map_err(|error| error.to_string())?;
        status_handle
            .set_service_status(service_status(
                ServiceState::StartPending,
                false,
                1,
                Duration::from_secs(10),
            ))
            .map_err(|error| error.to_string())?;
        let supervisor = config.supervisor()?;
        let mut runtime = BrokerServiceRuntime::new(supervisor, DEFAULT_STABLE_RECEIVER_POLLS)?;
        runtime.start()?;
        status_handle
            .set_service_status(service_status(
                ServiceState::Running,
                true,
                0,
                Duration::ZERO,
            ))
            .map_err(|error| error.to_string())?;
        while !stop.load(Ordering::Acquire) {
            runtime.poll();
            thread::sleep(DEFAULT_POLL_INTERVAL);
        }
        status_handle
            .set_service_status(service_status(
                ServiceState::StopPending,
                false,
                1,
                Duration::from_secs(10),
            ))
            .map_err(|error| error.to_string())?;
        let result = runtime.stop().map(|_| ());
        status_handle
            .set_service_status(service_status(
                ServiceState::Stopped,
                false,
                0,
                Duration::ZERO,
            ))
            .map_err(|error| error.to_string())?;
        result
    }

    fn run_broker(
        config: ServiceConfig,
        stop: Arc<AtomicBool>,
        emit_console_snapshot: bool,
    ) -> Result<(), String> {
        let deadline = config
            .console_run_for
            .map(|duration| Instant::now() + duration);
        let supervisor = config.supervisor()?;
        let mut runtime = BrokerServiceRuntime::new(supervisor, DEFAULT_STABLE_RECEIVER_POLLS)?;
        runtime.start()?;
        loop {
            if stop.load(Ordering::Acquire)
                || deadline.is_some_and(|deadline| Instant::now() >= deadline)
            {
                break;
            }
            runtime.poll();
            thread::sleep(DEFAULT_POLL_INTERVAL);
        }
        let snapshot = runtime.stop()?;
        if emit_console_snapshot {
            println!(
                "{}",
                serde_json::to_string(&snapshot).map_err(|error| error.to_string())?
            );
        }
        Ok(())
    }

    fn service_status(
        current_state: ServiceState,
        accepts_stop: bool,
        checkpoint: u32,
        wait_hint: Duration,
    ) -> ServiceStatus {
        ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state,
            controls_accepted: if accepts_stop {
                ServiceControlAccept::STOP
            } else {
                ServiceControlAccept::empty()
            },
            exit_code: ServiceExitCode::Win32(0),
            checkpoint,
            wait_hint,
            process_id: None,
        }
    }
}

#[cfg(windows)]
fn main() -> Result<(), String> {
    windows_host::main()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("capyio-windows-service is supported only on Windows");
}
