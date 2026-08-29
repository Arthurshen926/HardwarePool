use std::{env, net::IpAddr, path::PathBuf};

use capyio_micyou_adapter::{DEFAULT_MICYOU_PORT, MicYouProbe, ProbeLimits};
use capyio_micyou_host_config::{
    TrustedMicYouHostConfig, load_trusted_host_config, write_new_default_config,
};

fn main() {
    if let Err(problem) = run(env::args().collect()) {
        eprintln!("CapyIO MicYou host configuration failed: {problem}");
        std::process::exit(2);
    }
}

fn run(arguments: Vec<String>) -> Result<(), String> {
    let mut arguments = arguments.into_iter();
    let _program = arguments.next();
    match arguments.next().as_deref() {
        Some("provision") => provision(arguments.collect()),
        Some("validate") => validate(arguments.collect()),
        Some("--help" | "-h") | None => {
            print_help();
            Ok(())
        }
        Some(_) => Err("expected `provision` or `validate`".to_owned()),
    }
}

fn provision(arguments: Vec<String>) -> Result<(), String> {
    let mut executable = None;
    let mut bind_ip = None;
    let mut port = DEFAULT_MICYOU_PORT;
    let mut endpoint_id = None;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--executable" => executable = Some(PathBuf::from(next(&mut arguments, &argument)?)),
            "--bind-ip" => {
                bind_ip = Some(
                    next(&mut arguments, &argument)?
                        .parse::<IpAddr>()
                        .map_err(|_| "--bind-ip must be an IP literal".to_owned())?,
                )
            }
            "--port" => {
                port = next(&mut arguments, &argument)?
                    .parse::<u16>()
                    .map_err(|_| "--port must be a non-zero u16".to_owned())?
            }
            "--endpoint-id" => endpoint_id = Some(next(&mut arguments, &argument)?),
            _ => return Err(format!("unknown argument: {argument}")),
        }
    }
    let config = TrustedMicYouHostConfig::probe_and_provision(
        executable.ok_or_else(|| "--executable is required".to_owned())?,
        bind_ip.ok_or_else(|| "--bind-ip is required".to_owned())?,
        port,
        &endpoint_id.ok_or_else(|| "--endpoint-id is required".to_owned())?,
    )
    .map_err(|error| error.to_string())?;
    write_new_default_config(&config).map_err(|error| error.to_string())?;
    println!("CapyIO MicYou host configuration created; restart CapyIO Desktop to load it.");
    Ok(())
}

fn validate(arguments: Vec<String>) -> Result<(), String> {
    if !arguments.is_empty() {
        return Err("validate accepts no arguments".to_owned());
    }
    let loaded = load_trusted_host_config().map_err(|error| error.to_string())?;
    let config = loaded
        .config
        .adapter_config()
        .map_err(|error| error.to_string())?;
    let inventory = MicYouProbe::new(ProbeLimits::default())
        .map_err(|error| error.to_string())?
        .probe_config(&config)
        .map_err(|error| error.to_string())?;
    println!(
        "CapyIO MicYou host configuration is valid for MicYou {} ({} audio endpoints observed).",
        inventory.version,
        inventory.output_devices.len()
    );
    Ok(())
}

fn next(arguments: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    arguments
        .next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn print_help() {
    println!(
        "CapyIO MicYou trusted-host configuration\n\n\
         provision --executable <path> --bind-ip <IPv4> [--port <u16>]\n\
                   --endpoint-id <stable-id>\n\
         validate\n\n\
         The tool probes a separately supplied pinned MicYou executable and writes only\n\
         the fixed user-local CapyIO configuration. Existing configuration is not overwritten."
    );
}
