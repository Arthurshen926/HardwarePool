use std::{io::Write, net::TcpListener, process, thread, time::Duration};

const DEFAULT_ENDPOINT: &str = "fixture-default";
const EXIT_ENDPOINT: &str = "fixture-exit";
const NO_LISTEN_ENDPOINT: &str = "fixture-no-listen";
const SPAM_ENDPOINT: &str = "fixture-spam";

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let [bind] = args.as_slice()
        && !bind.starts_with("--")
    {
        let listener = TcpListener::bind(bind).expect("virtual speaker fixture listener");
        let mut connections = Vec::new();
        for connection in listener.incoming() {
            match connection {
                Ok(connection) => connections.push(connection),
                Err(_) => break,
            }
        }
        return;
    }
    if args == ["--version"] {
        println!("as-cmd\nversion: 0.3.4\nurl: https://github.com/mkckr0/audio-share");
        return;
    }
    if args == ["--list-endpoint"] {
        println!(
            "endpoint list:\n* id: {DEFAULT_ENDPOINT} name: Fixture default\n  id: {EXIT_ENDPOINT} name: Fixture early exit\n  id: {NO_LISTEN_ENDPOINT} name: Fixture no listener\n  id: {SPAM_ENDPOINT} name: Fixture bounded output\ntotal: 4"
        );
        return;
    }

    let endpoint = value(&args, "--endpoint=").unwrap_or_default();
    if endpoint == EXIT_ENDPOINT {
        process::exit(23);
    }
    if endpoint == NO_LISTEN_ENDPOINT {
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
    if endpoint == SPAM_ENDPOINT {
        let bytes = vec![b'x'; 4096];
        let _ = std::io::stdout().write_all(&bytes);
        let _ = std::io::stdout().flush();
    }

    let bind = value(&args, "--bind=").expect("fixture requires explicit bind");
    let listener = TcpListener::bind(bind).expect("fixture listener");
    let mut connections = Vec::new();
    for connection in listener.incoming() {
        match connection {
            Ok(connection) => connections.push(connection),
            Err(_) => break,
        }
    }
}

fn value<'a>(args: &'a [String], prefix: &str) -> Option<&'a str> {
    args.iter().find_map(|arg| arg.strip_prefix(prefix))
}
