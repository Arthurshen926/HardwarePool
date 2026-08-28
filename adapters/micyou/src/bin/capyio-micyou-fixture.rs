use std::{net::TcpListener, process};

const DEVICE: &str = "CapyIO Fixture Microphone Ingress";

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--version"] {
        println!("micyou-cli 2.0.1");
        return;
    }
    if args == ["capyio-capabilities"] {
        println!("device-index-v1");
        return;
    }
    if args == ["devices"] {
        println!("audio output devices:\n  1. {DEVICE}");
        return;
    }
    if args.first().map(String::as_str) != Some("serve") {
        process::exit(2);
    }
    let bind = value(&args, "--bind").expect("explicit bind");
    let port = value(&args, "--port").expect("explicit port");
    let device = value(&args, "--device").expect("explicit device");
    let device_index = value(&args, "--device-index").expect("explicit device index");
    if device != DEVICE || device_index != "1" {
        process::exit(3);
    }
    let listener = TcpListener::bind(format!("{bind}:{port}")).expect("fixture listener");
    println!("MicYou fixture server started");
    let mut connections = Vec::new();
    for connection in listener.incoming() {
        match connection {
            Ok(connection) => connections.push(connection),
            Err(_) => break,
        }
    }
}

fn value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find_map(|pair| (pair[0] == flag).then_some(pair[1].as_str()))
}
