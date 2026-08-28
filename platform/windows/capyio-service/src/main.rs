#[cfg(windows)]
mod windows_host {
    use std::{
        ffi::OsString,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

    use capyio_windows_service::{
        BrokerServiceRuntime, CaptureRingOwner, DEFAULT_POLL_INTERVAL,
        DEFAULT_STABLE_RECEIVER_POLLS, SERVICE_NAME, ServiceConfig, control_server_loop,
        wake_control_server,
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
        if arguments
            .get(1)
            .is_some_and(|argument| argument == "--control")
        {
            return run_control_command(arguments.get(2).map(String::as_str));
        }
        if arguments.iter().any(|argument| argument == "--console") {
            let config = ServiceConfig::parse(arguments).map_err(|error| error.to_string())?;
            return run_broker(config, Arc::new(AtomicBool::new(false)), true);
        }
        service_dispatcher::start(SERVICE_NAME, ffi_service_main).map_err(|error| error.to_string())
    }

    fn run_control_command(operation: Option<&str>) -> Result<(), String> {
        let client = capyio_windows_service::BrokerServiceClient::default();
        let snapshot = match operation {
            Some("status") => client.status()?,
            Some("start") => client.start()?,
            Some("stop") => client.stop()?,
            _ => return Err("usage: capyio-windows-service --control status|start|stop".to_owned()),
        };
        println!(
            "{}",
            serde_json::to_string(&snapshot).map_err(|error| error.to_string())?
        );
        Ok(())
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
        let _capture_ring =
            CaptureRingOwner::create_baseline().map_err(|error| error.to_string())?;
        status_handle
            .set_service_status(service_status(
                ServiceState::StartPending,
                false,
                1,
                Duration::from_secs(10),
            ))
            .map_err(|error| error.to_string())?;
        let supervisor = config.supervisor()?;
        let runtime = Arc::new(Mutex::new(BrokerServiceRuntime::new(
            supervisor,
            DEFAULT_STABLE_RECEIVER_POLLS,
        )?));
        let control_runtime = Arc::clone(&runtime);
        let control_stop = Arc::clone(&stop);
        let control = thread::Builder::new()
            .name("capyio-service-control".to_owned())
            .spawn(move || control_server_loop(control_runtime, control_stop))
            .map_err(|_| "start CapyIO service control thread".to_owned())?;
        // Prove the control listener is reachable before reporting Running.
        let client = capyio_windows_service::BrokerServiceClient::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        while client.status().is_err() {
            if Instant::now() >= deadline {
                stop.store(true, Ordering::Release);
                wake_control_server();
                let _ = control.join();
                return Err("CapyIO service control pipe did not become ready".to_owned());
            }
            thread::sleep(Duration::from_millis(25));
        }
        status_handle
            .set_service_status(service_status(
                ServiceState::Running,
                true,
                0,
                Duration::ZERO,
            ))
            .map_err(|error| error.to_string())?;
        while !stop.load(Ordering::Acquire) {
            if let Ok(mut runtime) = runtime.lock() {
                runtime.poll();
            }
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
        wake_control_server();
        let control_result = control
            .join()
            .map_err(|_| "CapyIO service control thread panicked".to_owned())?;
        let result = runtime
            .lock()
            .map_err(|_| "CapyIO service state lock poisoned".to_owned())?
            .ensure_stopped()
            .map(|_| ());
        status_handle
            .set_service_status(service_status(
                ServiceState::Stopped,
                false,
                0,
                Duration::ZERO,
            ))
            .map_err(|error| error.to_string())?;
        control_result.and(result)
    }

    fn run_broker(
        config: ServiceConfig,
        stop: Arc<AtomicBool>,
        emit_console_snapshot: bool,
    ) -> Result<(), String> {
        let deadline = config
            .console_run_for
            .map(|duration| Instant::now() + duration);
        let _capture_ring =
            CaptureRingOwner::create_baseline().map_err(|error| error.to_string())?;
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
