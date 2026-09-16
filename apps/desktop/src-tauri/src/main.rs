fn main() {
    let mut broker_arguments = std::env::args().skip(1);
    let broker_command = broker_arguments.next();
    if broker_command.as_deref() == Some("--camera-broker-host") {
        if broker_arguments.next().is_some() {
            eprintln!(
                "CAPYIO_CAMERA_BROKER_FAILED: camera Broker host accepts no additional arguments"
            );
            std::process::exit(2);
        }
        if let Err(error) = capyio_desktop_lib::run_camera_broker_host() {
            eprintln!("CAPYIO_CAMERA_BROKER_FAILED: {error}");
            std::process::exit(2);
        }
        return;
    }
    if broker_command.as_deref() == Some("--camera-broker-control") {
        let operation = broker_arguments.next();
        if broker_arguments.next().is_some() {
            eprintln!(
                "CAPYIO_CAMERA_BROKER_CONTROL_FAILED: usage: capyio-desktop --camera-broker-control status|start|stop"
            );
            std::process::exit(2);
        }
        let client = capyio_windows_service::CameraBrokerClient::default();
        let result = match operation.as_deref() {
            Some("status") => client.status(),
            Some("start") => client.start(),
            Some("stop") => client.stop(),
            _ => Err("usage: capyio-desktop --camera-broker-control status|start|stop".to_owned()),
        };
        match result.and_then(|snapshot| {
            serde_json::to_string(&snapshot).map_err(|error| error.to_string())
        }) {
            Ok(snapshot) => println!("{snapshot}"),
            Err(error) => {
                eprintln!("CAPYIO_CAMERA_BROKER_CONTROL_FAILED: {error}");
                std::process::exit(2);
            }
        }
        return;
    }
    #[cfg(debug_assertions)]
    {
        let mut arguments = std::env::args().skip(1);
        let command = arguments.next();
        if command.as_deref() == Some("--gamepad-physical-gate") {
            let android_port = arguments
                .next()
                .unwrap_or_else(|| "31581".to_owned())
                .parse::<u16>();
            let dsu_port = arguments
                .next()
                .unwrap_or_else(|| "26761".to_owned())
                .parse::<u16>();
            let result = match (android_port, dsu_port) {
                (Ok(android_port), Ok(dsu_port)) => {
                    capyio_desktop_lib::run_gamepad_physical_gate(android_port, dsu_port)
                }
                _ => Err(
                    "usage: capyio-desktop --gamepad-physical-gate [android-port] [dsu-port]"
                        .to_owned(),
                ),
            };
            if let Err(error) = result {
                eprintln!("CAPYIO_PHYSICAL_GAMEPAD_FAILED: {error}");
                std::process::exit(2);
            }
            println!("CAPYIO_PHYSICAL_GAMEPAD_PASSED");
            return;
        }
        if command.as_deref() == Some("--gamepad-viiper-physical-gate") {
            let android_port = arguments
                .next()
                .unwrap_or_else(|| "31581".to_owned())
                .parse::<u16>();
            let dsu_port = arguments
                .next()
                .unwrap_or_else(|| "26761".to_owned())
                .parse::<u16>();
            let viiper_port = arguments
                .next()
                .unwrap_or_else(|| "3242".to_owned())
                .parse::<u16>();
            let hold_seconds = arguments
                .next()
                .unwrap_or_else(|| "90".to_owned())
                .parse::<u64>();
            let result = match (android_port, dsu_port, viiper_port, hold_seconds) {
                (Ok(android_port), Ok(dsu_port), Ok(viiper_port), Ok(hold_seconds)) => {
                    capyio_desktop_lib::run_gamepad_viiper_physical_gate(
                        android_port,
                        dsu_port,
                        viiper_port,
                        hold_seconds,
                    )
                }
                _ => Err(
                    "usage: capyio-desktop --gamepad-viiper-physical-gate [android-port] [dsu-port] [viiper-api-port] [hold-seconds]"
                        .to_owned(),
                ),
            };
            if let Err(error) = result {
                eprintln!("CAPYIO_PHYSICAL_VIIPER_GAMEPAD_FAILED: {error}");
                std::process::exit(2);
            }
            println!("CAPYIO_PHYSICAL_VIIPER_GAMEPAD_PASSED");
            return;
        }
        if command.as_deref() == Some("--gamepad-ds4-physical-gate") {
            let android_port = arguments
                .next()
                .unwrap_or_else(|| "31581".to_owned())
                .parse::<u16>();
            let viiper_port = arguments
                .next()
                .unwrap_or_else(|| "3242".to_owned())
                .parse::<u16>();
            let hold_seconds = arguments
                .next()
                .unwrap_or_else(|| "90".to_owned())
                .parse::<u64>();
            let result = match (android_port, viiper_port, hold_seconds) {
                (Ok(android_port), Ok(viiper_port), Ok(hold_seconds)) => {
                    capyio_desktop_lib::run_gamepad_ds4_physical_gate(
                        android_port,
                        viiper_port,
                        hold_seconds,
                    )
                }
                _ => Err(
                    "usage: capyio-desktop --gamepad-ds4-physical-gate [android-port] [viiper-api-port] [hold-seconds]"
                        .to_owned(),
                ),
            };
            if let Err(error) = result {
                eprintln!("CAPYIO_PHYSICAL_DS4_GAMEPAD_FAILED: {error}");
                std::process::exit(2);
            }
            println!("CAPYIO_PHYSICAL_DS4_GAMEPAD_PASSED");
            return;
        }
        if command.as_deref() == Some("--gamepad-windows-read-only-preflight") {
            if arguments.next().is_some() {
                eprintln!(
                    "CAPYIO_WINDOWS_GAMEPAD_PREFLIGHT_FAILED: usage: capyio-desktop --gamepad-windows-read-only-preflight"
                );
                std::process::exit(2);
            }
            if let Err(error) = capyio_desktop_lib::run_windows_gamepad_read_only_preflight() {
                eprintln!("CAPYIO_WINDOWS_GAMEPAD_PREFLIGHT_FAILED: {error}");
                std::process::exit(2);
            }
            println!("CAPYIO_WINDOWS_GAMEPAD_PREFLIGHT_PASSED");
            return;
        }
        if command.as_deref() == Some("--gamepad-windows-ds4-runtime-gate") {
            let android_port = arguments
                .next()
                .unwrap_or_else(|| "31581".to_owned())
                .parse::<u16>();
            let hold_seconds = arguments
                .next()
                .unwrap_or_else(|| "90".to_owned())
                .parse::<u64>();
            let result = match (android_port, hold_seconds) {
                (Ok(android_port), Ok(hold_seconds)) if arguments.next().is_none() => {
                    capyio_desktop_lib::run_windows_ds4_runtime_gate(
                        android_port,
                        hold_seconds,
                    )
                }
                _ => Err("usage: capyio-desktop --gamepad-windows-ds4-runtime-gate [android-port] [hold-seconds]".to_owned()),
            };
            if let Err(error) = result {
                eprintln!("CAPYIO_WINDOWS_DS4_RUNTIME_GATE_FAILED: {error}");
                std::process::exit(2);
            }
            println!("CAPYIO_WINDOWS_DS4_RUNTIME_GATE_PASSED");
            return;
        }
        if command.as_deref() == Some("--gamepad-windows-ds4-only-runtime-gate") {
            let android_port = arguments
                .next()
                .unwrap_or_else(|| "31581".to_owned())
                .parse::<u16>();
            let hold_seconds = arguments
                .next()
                .unwrap_or_else(|| "90".to_owned())
                .parse::<u64>();
            let result = match (android_port, hold_seconds) {
                (Ok(android_port), Ok(hold_seconds)) if arguments.next().is_none() => {
                    capyio_desktop_lib::run_windows_ds4_only_runtime_gate(
                        android_port,
                        hold_seconds,
                    )
                }
                _ => Err("usage: capyio-desktop --gamepad-windows-ds4-only-runtime-gate [android-port] [hold-seconds]".to_owned()),
            };
            if let Err(error) = result {
                eprintln!("CAPYIO_WINDOWS_DS4_ONLY_RUNTIME_GATE_FAILED: {error}");
                std::process::exit(2);
            }
            println!("CAPYIO_WINDOWS_DS4_ONLY_RUNTIME_GATE_PASSED");
            return;
        }
    }
    capyio_desktop_lib::run();
}
