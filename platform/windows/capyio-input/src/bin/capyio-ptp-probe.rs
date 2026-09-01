use std::{env, process::ExitCode};

use capyio_windows_input::{
    DeviceCreationProbe, PROBE_SCHEMA_VERSION, SyntheticTouchpadParameters,
    VhfBrokerInterfaceProbe, probe_synthetic_touchpad_api,
    probe_synthetic_touchpad_device_creation, probe_vhf_broker_interface,
};

#[derive(Clone, Copy, Eq, PartialEq)]
enum ProbeMode {
    Symbols,
    CreateSyntheticDevice,
    VhfInterface,
}

fn main() -> ExitCode {
    let mut mode = ProbeMode::Symbols;
    for argument in env::args().skip(1) {
        match argument.as_str() {
            "--create-device" if mode == ProbeMode::Symbols => {
                mode = ProbeMode::CreateSyntheticDevice
            }
            "--vhf-interface" if mode == ProbeMode::Symbols => mode = ProbeMode::VhfInterface,
            "--symbols-only" if mode == ProbeMode::Symbols => {}
            "--help" | "-h" => {
                println!(
                    "Usage: capyio-ptp-probe [--symbols-only | --create-device | --vhf-interface]\n\
                     Default: resolve user32 symbols only. --create-device creates and\n\
                     immediately destroys a synthetic touchpad without injecting input.\n\
                     --vhf-interface performs read-only SetupAPI enumeration and never opens it."
                );
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("unsupported argument: {argument}");
                return ExitCode::from(64);
            }
        }
    }

    if mode == ProbeMode::VhfInterface {
        println!("schema_version={PROBE_SCHEMA_VERSION}");
        println!("platform={}", std::env::consts::OS);
        match probe_vhf_broker_interface() {
            VhfBrokerInterfaceProbe::UnsupportedPlatform => {
                println!("vhf_broker_interface=unsupported_platform")
            }
            VhfBrokerInterfaceProbe::Absent => println!("vhf_broker_interface=absent"),
            VhfBrokerInterfaceProbe::Single => println!("vhf_broker_interface=single"),
            VhfBrokerInterfaceProbe::Multiple => println!("vhf_broker_interface=multiple"),
            VhfBrokerInterfaceProbe::Failed(code) => {
                println!("vhf_broker_interface=failed");
                println!("vhf_broker_interface_error={code}");
            }
        }
        println!("device_opened=false");
        println!("ioctl_sent=false");
        return ExitCode::SUCCESS;
    }

    let probe = probe_synthetic_touchpad_api();
    println!("schema_version={PROBE_SCHEMA_VERSION}");
    println!("platform={}", probe.platform);
    println!("user32_loaded={}", probe.user32_loaded);
    match probe.load_error_code {
        Some(error_code) => println!("user32_load_error={error_code}"),
        None => println!("user32_load_error=none"),
    }
    for symbol in probe.symbols {
        println!("export.{}={}", symbol.name, symbol.exported);
    }
    println!("synthetic_touchpad_api_available={}", probe.is_available());

    if mode != ProbeMode::CreateSyntheticDevice {
        println!("device_creation=not_requested");
        return ExitCode::SUCCESS;
    }

    match probe_synthetic_touchpad_device_creation(SyntheticTouchpadParameters::default()) {
        DeviceCreationProbe::UnsupportedPlatform => {
            println!("device_creation=unsupported_platform")
        }
        DeviceCreationProbe::InvalidParameters(error) => {
            println!("device_creation=invalid_parameters");
            println!("device_creation_detail={error}");
        }
        DeviceCreationProbe::User32LoadFailed { error_code } => {
            println!("device_creation=user32_load_failed");
            println!("device_creation_error={error_code}");
        }
        DeviceCreationProbe::MissingSymbols { symbols } => {
            println!("device_creation=missing_symbols");
            println!("device_creation_detail={}", symbols.join(","));
        }
        DeviceCreationProbe::CreationFailed { error_code } => {
            println!("device_creation=failed");
            println!("device_creation_error={error_code}");
        }
        DeviceCreationProbe::CreatedAndDestroyed { parameters } => {
            println!("device_creation=created_and_destroyed");
            println!("device_max_contacts={}", parameters.max_contacts);
            println!("device_width_himetric={}", parameters.width_himetric);
            println!("device_height_himetric={}", parameters.height_himetric);
            println!("input_injected=false");
        }
    }

    ExitCode::SUCCESS
}
