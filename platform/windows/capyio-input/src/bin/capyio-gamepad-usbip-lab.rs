#![forbid(unsafe_code)]

use std::{
    io::{self, Write},
    net::{Ipv4Addr, SocketAddrV4},
    path::PathBuf,
    time::Duration,
};

use capyio_windows_input::{
    PINNED_USBIP_WIN2_VERSION, UsbipBusId, UsbipWin2Client, UsbipWin2Config,
    UsbipWin2DeploymentVerified,
};

const DEFAULT_USBIP_PORT: u16 = 3241;
const DEFAULT_USBIP_EXE: &str = r"C:\Program Files\USBip\usbip.exe";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const OUTPUT_LIMIT: usize = 16 * 1024;
const MIN_HOLD_SECONDS: u64 = 5;
const MAX_HOLD_SECONDS: u64 = 300;

#[derive(Clone, Debug, Eq, PartialEq)]
enum LabCommand {
    Help,
    Preflight(LabConfig),
    Attach {
        config: LabConfig,
        hold_seconds: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControllerKind {
    Xbox360,
    DualShock4,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LabConfig {
    kind: ControllerKind,
    bus_id: UsbipBusId,
    usbip_port: u16,
    usbip_executable: PathBuf,
}

fn main() {
    match parse_command(std::env::args().skip(1)).and_then(run_command) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("CAPYIO_USBIP_LAB_FAILED: {error}");
            std::process::exit(2);
        }
    }
}

fn run_command(command: LabCommand) -> Result<(), String> {
    match command {
        LabCommand::Help => {
            print_usage();
            Ok(())
        }
        LabCommand::Preflight(config) => {
            let client = client(&config)?;
            let device = match config.kind {
                ControllerKind::Xbox360 => client.list_xbox360(&config.bus_id),
                ControllerKind::DualShock4 => client.list_dualshock4(&config.bus_id),
            }
            .map_err(|error| error.to_string())?;
            println!("CAPYIO_USBIP_VERSION={PINNED_USBIP_WIN2_VERSION}");
            println!("CAPYIO_USBIP_SERVER=127.0.0.1:{}", config.usbip_port);
            println!("CAPYIO_USBIP_BUS_ID={}", device.bus_id());
            println!(
                "CAPYIO_USBIP_DEVICE={:04x}:{:04x}:{}",
                device.vendor_id(),
                device.product_id(),
                device.label()
            );
            println!("CAPYIO_USBIP_PREFLIGHT_PASSED");
            Ok(())
        }
        LabCommand::Attach {
            config,
            hold_seconds,
        } => {
            let client = client(&config)?;
            let mut attachment = match config.kind {
                ControllerKind::Xbox360 => client.attach_xbox360_once(
                    UsbipWin2DeploymentVerified::confirmed_by_caller(),
                    config.bus_id,
                ),
                ControllerKind::DualShock4 => client.attach_dualshock4_once(
                    UsbipWin2DeploymentVerified::confirmed_by_caller(),
                    config.bus_id,
                ),
            }
            .map_err(|error| error.to_string())?;
            println!("CAPYIO_USBIP_VERSION={PINNED_USBIP_WIN2_VERSION}");
            println!("CAPYIO_USBIP_BUS_ID={}", attachment.bus_id());
            println!("CAPYIO_USBIP_OWNED_PORT={}", attachment.port());
            println!("CAPYIO_USBIP_ATTACHMENT_READY");
            io::stdout()
                .flush()
                .map_err(|error| format!("could not flush USB/IP lab status: {error}"))?;
            for elapsed in 1..=hold_seconds {
                std::thread::sleep(Duration::from_secs(1));
                if let Err(error) = attachment.verify_present() {
                    let port = attachment.port();
                    let cleanup = attachment.stop().err();
                    return Err(format!(
                        "owned USB/IP port {port} disappeared after {elapsed} seconds: {error}{}",
                        cleanup.map_or_else(String::new, |cleanup| {
                            format!("; exact-port cleanup also failed: {cleanup}")
                        })
                    ));
                }
            }
            println!("CAPYIO_USBIP_ATTACHMENT_LIVENESS_PASSED");
            let port = attachment.port();
            if let Err(error) = attachment.stop() {
                return Err(format!(
                    "owned USB/IP port {port} did not detach: {error}; resolve the exact attachment with `usbip.exe port {port}` before `usbip.exe detach --port {port}`"
                ));
            }
            println!("CAPYIO_USBIP_DETACHED_PORT={port}");
            println!("CAPYIO_USBIP_ATTACHMENT_PASSED");
            Ok(())
        }
    }
}

fn client(config: &LabConfig) -> Result<UsbipWin2Client, String> {
    UsbipWin2Config::new(
        config.usbip_executable.clone(),
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, config.usbip_port),
        COMMAND_TIMEOUT,
        OUTPUT_LIMIT,
    )
    .map(UsbipWin2Client::new)
    .map_err(|error| error.to_string())
}

fn parse_command(arguments: impl IntoIterator<Item = String>) -> Result<LabCommand, String> {
    let mut arguments = arguments.into_iter();
    let Some(command) = arguments.next() else {
        return Ok(LabCommand::Help);
    };
    match command.as_str() {
        "help" | "--help" | "-h" => {
            require_end(arguments)?;
            Ok(LabCommand::Help)
        }
        "preflight" | "preflight-ds4" => {
            let kind = if command == "preflight-ds4" {
                ControllerKind::DualShock4
            } else {
                ControllerKind::Xbox360
            };
            let config = parse_config(kind, &mut arguments)?;
            require_end(arguments)?;
            Ok(LabCommand::Preflight(config))
        }
        "attach" | "attach-ds4" => {
            let kind = if command == "attach-ds4" {
                ControllerKind::DualShock4
            } else {
                ControllerKind::Xbox360
            };
            let viiper_bus_id = parse_required::<u32>(&mut arguments, "VIIPER bus ID")?;
            let viiper_device_id = arguments
                .next()
                .ok_or_else(|| "missing VIIPER device ID".to_owned())?;
            let hold_seconds = parse_required::<u64>(&mut arguments, "hold seconds")?;
            if !(MIN_HOLD_SECONDS..=MAX_HOLD_SECONDS).contains(&hold_seconds) {
                return Err(format!(
                    "hold seconds must be within {MIN_HOLD_SECONDS}..={MAX_HOLD_SECONDS}"
                ));
            }
            let config = parse_config_tail(kind, viiper_bus_id, &viiper_device_id, &mut arguments)?;
            require_end(arguments)?;
            Ok(LabCommand::Attach {
                config,
                hold_seconds,
            })
        }
        value => Err(format!(
            "unknown command {value:?}; expected preflight, attach or help"
        )),
    }
}

fn parse_config(
    kind: ControllerKind,
    arguments: &mut impl Iterator<Item = String>,
) -> Result<LabConfig, String> {
    let viiper_bus_id = parse_required::<u32>(arguments, "VIIPER bus ID")?;
    let viiper_device_id = arguments
        .next()
        .ok_or_else(|| "missing VIIPER device ID".to_owned())?;
    parse_config_tail(kind, viiper_bus_id, &viiper_device_id, arguments)
}

fn parse_config_tail(
    kind: ControllerKind,
    viiper_bus_id: u32,
    viiper_device_id: &str,
    arguments: &mut impl Iterator<Item = String>,
) -> Result<LabConfig, String> {
    let usbip_port = arguments.next().map_or(Ok(DEFAULT_USBIP_PORT), |value| {
        value
            .parse::<u16>()
            .map_err(|error| format!("invalid USB/IP port: {error}"))
    })?;
    if usbip_port == 0 {
        return Err("USB/IP port must be non-zero".to_owned());
    }
    let usbip_executable = arguments
        .next()
        .map_or_else(|| PathBuf::from(DEFAULT_USBIP_EXE), PathBuf::from);
    if !usbip_executable.is_absolute()
        || !usbip_executable
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("usbip.exe"))
    {
        return Err("USB/IP executable must be an absolute path named usbip.exe".to_owned());
    }
    Ok(LabConfig {
        kind,
        bus_id: UsbipBusId::from_viiper(viiper_bus_id, viiper_device_id)
            .map_err(|error| error.to_string())?,
        usbip_port,
        usbip_executable,
    })
}

fn parse_required<T: std::str::FromStr>(
    arguments: &mut impl Iterator<Item = String>,
    label: &str,
) -> Result<T, String>
where
    T::Err: Display,
{
    arguments
        .next()
        .ok_or_else(|| format!("missing {label}"))?
        .parse()
        .map_err(|error| format!("invalid {label}: {error}"))
}

fn require_end(mut arguments: impl Iterator<Item = String>) -> Result<(), String> {
    if let Some(extra) = arguments.next() {
        Err(format!("unexpected extra argument {extra:?}"))
    } else {
        Ok(())
    }
}

fn print_usage() {
    println!(
        "usage:\n  capyio-gamepad-usbip-lab preflight[-ds4] <viiper-bus-id> <viiper-device-id> [usbip-port] [absolute-usbip.exe]\n  capyio-gamepad-usbip-lab attach[-ds4] <viiper-bus-id> <viiper-device-id> <hold-seconds> [usbip-port] [absolute-usbip.exe]\n\nThe default commands target Xbox 360; the -ds4 commands require exact 054c:09cc. Attach uses --once, holds for 5..=300 seconds, then detaches only its reported hub port. It is an explicitly authorized local-lab operation."
    );
}

use std::fmt::Display;

#[cfg(test)]
mod tests {
    use super::*;

    fn args<'a>(values: &'a [&'a str]) -> impl Iterator<Item = String> + 'a {
        values.iter().map(|value| (*value).to_owned())
    }

    #[test]
    fn parser_is_closed_and_attach_is_bounded() {
        assert_eq!(parse_command(args(&[])).unwrap(), LabCommand::Help);
        assert_eq!(
            parse_command(args(&["preflight", "1", "1"])).unwrap(),
            LabCommand::Preflight(LabConfig {
                kind: ControllerKind::Xbox360,
                bus_id: UsbipBusId::from_viiper(1, "1").unwrap(),
                usbip_port: DEFAULT_USBIP_PORT,
                usbip_executable: PathBuf::from(DEFAULT_USBIP_EXE),
            })
        );
        assert_eq!(
            parse_command(args(&[
                "attach",
                "7",
                "3",
                "90",
                "4241",
                r"D:\Lab\usbip.exe",
            ]))
            .unwrap(),
            LabCommand::Attach {
                config: LabConfig {
                    kind: ControllerKind::Xbox360,
                    bus_id: UsbipBusId::from_viiper(7, "3").unwrap(),
                    usbip_port: 4241,
                    usbip_executable: PathBuf::from(r"D:\Lab\usbip.exe"),
                },
                hold_seconds: 90,
            }
        );
        assert_eq!(
            parse_command(args(&["preflight-ds4", "9", "4"])).unwrap(),
            LabCommand::Preflight(LabConfig {
                kind: ControllerKind::DualShock4,
                bus_id: UsbipBusId::from_viiper(9, "4").unwrap(),
                usbip_port: DEFAULT_USBIP_PORT,
                usbip_executable: PathBuf::from(DEFAULT_USBIP_EXE),
            })
        );
        assert_eq!(
            parse_command(args(&["attach-ds4", "9", "4", "30"])).unwrap(),
            LabCommand::Attach {
                config: LabConfig {
                    kind: ControllerKind::DualShock4,
                    bus_id: UsbipBusId::from_viiper(9, "4").unwrap(),
                    usbip_port: DEFAULT_USBIP_PORT,
                    usbip_executable: PathBuf::from(DEFAULT_USBIP_EXE),
                },
                hold_seconds: 30,
            }
        );
        for invalid in [
            vec!["unknown"],
            vec!["preflight"],
            vec!["preflight", "0", "1"],
            vec!["preflight", "1", "1", "0"],
            vec!["attach", "1", "1", "4"],
            vec!["attach", "1", "1", "301"],
            vec!["attach", "1", "1", "5", "3241", "usbip.exe"],
        ] {
            assert!(
                parse_command(args(&invalid)).is_err(),
                "accepted {invalid:?}"
            );
        }
    }
}
