fn main() {
    #[cfg(windows)]
    if std::env::args().nth(1).as_deref() == Some("--node-host") {
        if let Err(error) = capyio_desktop_lib::run_node_host() {
            eprintln!("CAPYIO_NODE_HOST_FAILED: {error}");
            std::process::exit(2);
        }
        return;
    }
    let mut broker_arguments = std::env::args().skip(1);
    let broker_command = broker_arguments.next();
    if broker_command.as_deref() == Some("--camera-broker-host") {
        let requesting_user_sid = broker_arguments.next();
        if requesting_user_sid.is_none() || broker_arguments.next().is_some() {
            eprintln!(
                "CAPYIO_CAMERA_BROKER_FAILED: camera Broker host requires exactly one requesting-user SID"
            );
            std::process::exit(2);
        }
        if let Err(error) = capyio_desktop_lib::run_camera_broker_host(
            requesting_user_sid
                .as_deref()
                .expect("checked SID argument"),
        ) {
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
    if matches!(
        broker_command.as_deref(),
        Some("--create-pairing-invitation" | "--renew-pairing-invitation")
    ) {
        let renew = broker_command.as_deref() == Some("--renew-pairing-invitation");
        let target_node_id = broker_arguments.next();
        let target_display_name = broker_arguments.next();
        if target_node_id.is_none()
            || target_display_name.is_none()
            || (renew
                && broker_arguments.next().as_deref() != Some("--acknowledge-pairing-interruption"))
            || broker_arguments.next().is_some()
        {
            eprintln!(
                "CAPYIO_PAIRING_INVITATION_FAILED: usage: capyio-desktop --create-pairing-invitation <target-node-uuid> <display-name> OR --renew-pairing-invitation <target-node-uuid> <display-name> --acknowledge-pairing-interruption"
            );
            std::process::exit(2);
        }
        let issue = if renew {
            capyio_desktop_lib::renew_pairing_invitation_cli
        } else {
            capyio_desktop_lib::create_pairing_invitation_cli
        };
        match issue(
            target_node_id.as_deref().expect("checked target Node ID"),
            target_display_name
                .as_deref()
                .expect("checked display name"),
        ) {
            Ok(path) => println!("{}", path.display()),
            Err(error) => {
                eprintln!("CAPYIO_PAIRING_INVITATION_FAILED: {error}");
                std::process::exit(2);
            }
        }
        return;
    }
    if broker_command.as_deref() == Some("--verify-paired-control-endpoint") {
        let node_id = broker_arguments.next();
        let host = broker_arguments.next();
        let port = broker_arguments
            .next()
            .and_then(|value| value.parse::<u16>().ok());
        if node_id.is_none()
            || host.is_none()
            || port.is_none()
            || broker_arguments.next().is_some()
        {
            eprintln!(
                "CAPYIO_CONTROL_VERIFY_FAILED: usage: capyio-desktop --verify-paired-control-endpoint <node-uuid> <ip> <port>"
            );
            std::process::exit(2);
        }
        match capyio_desktop_lib::verify_paired_control_endpoint_cli(
            node_id.as_deref().expect("checked Node ID"),
            host.as_deref().expect("checked host"),
            port.expect("checked port"),
        ) {
            Ok(capabilities) => {
                println!("CAPYIO_CONTROL_VERIFY_PASSED capabilities={capabilities}")
            }
            Err(error) => {
                eprintln!("CAPYIO_CONTROL_VERIFY_FAILED: {error}");
                std::process::exit(2);
            }
        }
        return;
    }
    if broker_command.as_deref() == Some("--verify-paired-control-listener") {
        let node_id = broker_arguments.next();
        let host = broker_arguments.next();
        let port = broker_arguments
            .next()
            .and_then(|value| value.parse::<u16>().ok());
        if node_id.is_none()
            || host.is_none()
            || port.is_none()
            || broker_arguments.next().is_some()
        {
            eprintln!(
                "CAPYIO_CONTROL_LISTENER_FAILED: usage: capyio-desktop --verify-paired-control-listener <peer-node-uuid> <local-bind-ip> <port>"
            );
            std::process::exit(2);
        }
        match capyio_desktop_lib::verify_paired_control_listener_cli(
            node_id.as_deref().expect("checked Node ID"),
            host.as_deref().expect("checked host"),
            port.expect("checked port"),
            |address| println!("CAPYIO_CONTROL_LISTENER_READY address={address}"),
        ) {
            Ok(capabilities) => {
                println!("CAPYIO_CONTROL_LISTENER_PASSED capabilities={capabilities}")
            }
            Err(error) => {
                eprintln!("CAPYIO_CONTROL_LISTENER_FAILED: {error}");
                std::process::exit(2);
            }
        }
        return;
    }
    if matches!(
        broker_command.as_deref(),
        Some("--verify-paired-touchpad-data" | "--verify-paired-touchpad-soak")
    ) {
        let node = broker_arguments.next();
        let host = broker_arguments.next();
        let port = broker_arguments
            .next()
            .and_then(|value| value.parse::<u16>().ok());
        if node.is_none()
            || host.is_none()
            || port.is_none()
            || broker_arguments.next().as_deref() != Some("--acknowledge-desktop-input")
            || broker_arguments.next().is_some()
        {
            eprintln!(
                "usage: capyio-desktop --verify-paired-touchpad-data <node-uuid> <ip> <port> --acknowledge-desktop-input"
            );
            std::process::exit(2);
        }
        let gate = if broker_command.as_deref() == Some("--verify-paired-touchpad-soak") {
            capyio_desktop_lib::verify_paired_touchpad_soak_cli
        } else {
            capyio_desktop_lib::verify_paired_touchpad_data_cli
        };
        match gate(
            node.as_deref().unwrap(),
            host.as_deref().unwrap(),
            port.unwrap(),
        ) {
            Ok(route) => println!("CAPYIO_TOUCHPAD_DATA_PASSED route={route} transport=mtls"),
            Err(error) => {
                eprintln!("CAPYIO_TOUCHPAD_DATA_FAILED: {error}");
                std::process::exit(2);
            }
        }
        return;
    }
    if broker_command.as_deref() == Some("--verify-paired-touchpad-consent") {
        let node_id = broker_arguments.next();
        let host = broker_arguments.next();
        let port = broker_arguments
            .next()
            .and_then(|value| value.parse::<u16>().ok());
        if node_id.is_none()
            || host.is_none()
            || port.is_none()
            || broker_arguments.next().is_some()
        {
            eprintln!(
                "usage: capyio-desktop --verify-paired-touchpad-consent <node-uuid> <ip> <port>"
            );
            std::process::exit(2);
        }
        match capyio_desktop_lib::verify_paired_touchpad_consent_cli(
            node_id.as_deref().unwrap(),
            host.as_deref().unwrap(),
            port.unwrap(),
        ) {
            Ok(route) => {
                println!("CAPYIO_TOUCHPAD_CONSENT_PASSED route={route} input_injected=false")
            }
            Err(error) => {
                eprintln!("CAPYIO_TOUCHPAD_CONSENT_FAILED: {error}");
                std::process::exit(2);
            }
        }
        return;
    }
    if broker_command.as_deref() == Some("--verify-paired-route-endpoint") {
        let node_id = broker_arguments.next();
        let host = broker_arguments.next();
        let port = broker_arguments
            .next()
            .and_then(|value| value.parse::<u16>().ok());
        let capability_id = broker_arguments.next();
        if node_id.is_none()
            || host.is_none()
            || port.is_none()
            || capability_id.is_none()
            || broker_arguments.next().is_some()
        {
            eprintln!(
                "CAPYIO_ROUTE_VERIFY_FAILED: usage: capyio-desktop --verify-paired-route-endpoint <node-uuid> <ip> <port> <capability-id>"
            );
            std::process::exit(2);
        }
        match capyio_desktop_lib::verify_paired_route_endpoint_cli(
            node_id.as_deref().expect("checked Node ID"),
            host.as_deref().expect("checked host"),
            port.expect("checked port"),
            capability_id.as_deref().expect("checked Capability ID"),
        ) {
            Ok(route_id) => println!("CAPYIO_ROUTE_VERIFY_PASSED route={route_id}"),
            Err(error) => {
                eprintln!("CAPYIO_ROUTE_VERIFY_FAILED: {error}");
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
