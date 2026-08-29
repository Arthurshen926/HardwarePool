use std::{
    net::{IpAddr, Ipv4Addr, TcpListener, TcpStream},
    path::PathBuf,
    time::Duration,
};

use capyio_micyou_adapter::{
    MicYouConfig, MicYouProbe, MicYouSupervisor, PeerTcpPresence, ProbeLimits, SupervisorLimits,
    SupervisorStatus,
};

const DEVICE: &str = "CapyIO Fixture Microphone Ingress";
const DEVICE_ID: &str = "{0.0.0.00000000}.{capyio-fixture-ingress}";

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_capyio-micyou-fixture"))
}

fn unused_port() -> u16 {
    loop {
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .expect("ephemeral port")
            .local_addr()
            .expect("local address")
            .port();
        if port != u16::MAX {
            return port;
        }
    }
}

#[test]
fn probes_pinned_fixture_and_requires_exact_device() {
    let config = MicYouConfig::new(
        fixture(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        unused_port(),
        DEVICE_ID,
        DEVICE,
    )
    .expect("config");
    let inventory = MicYouProbe::new(ProbeLimits::default())
        .expect("probe")
        .probe_config(&config)
        .expect("inventory");
    assert_eq!(inventory.version, "2.0.1");
    assert_eq!(inventory.output_devices[0].index.get(), 1);
    assert_eq!(inventory.output_devices[0].id, DEVICE_ID);
    assert_eq!(inventory.output_devices[0].name, DEVICE);
}

#[test]
fn starts_polls_stops_and_reaps_fixture() {
    let config = MicYouConfig::new(
        fixture(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        unused_port(),
        DEVICE_ID,
        DEVICE,
    )
    .expect("config");
    let mut supervisor = MicYouSupervisor::new(
        config,
        ProbeLimits::default(),
        SupervisorLimits {
            startup_deadline: Duration::from_secs(2),
            retained_output_bytes: 4096,
        },
    )
    .expect("supervisor");
    let process_id = supervisor.start().expect("start");
    assert_eq!(
        supervisor.status().expect("status"),
        SupervisorStatus::Running { process_id }
    );
    let output = supervisor.stop().expect("stop").expect("was running");
    assert!(output.stdout_bytes > 0);
    assert_eq!(
        supervisor.status().expect("stopped"),
        SupervisorStatus::Stopped
    );
    assert!(supervisor.stop().expect("idempotent").is_none());
}

#[cfg(windows)]
#[test]
fn windows_peer_presence_tracks_only_the_supervised_server_connection() {
    let port = unused_port();
    let config = MicYouConfig::new(
        fixture(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        port,
        DEVICE_ID,
        DEVICE,
    )
    .expect("config");
    let mut supervisor = MicYouSupervisor::new(
        config,
        ProbeLimits::default(),
        SupervisorLimits {
            startup_deadline: Duration::from_secs(2),
            retained_output_bytes: 4096,
        },
    )
    .expect("supervisor");
    supervisor.start().expect("start");
    assert_eq!(
        supervisor.peer_tcp_presence().expect("initial presence"),
        PeerTcpPresence::Disconnected
    );

    let peer = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("fixture peer");
    let started = std::time::Instant::now();
    loop {
        if matches!(
            supervisor.peer_tcp_presence().expect("connected presence"),
            PeerTcpPresence::Established {
                connection_count: 1..
            }
        ) {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "process-owned TCP peer was not observed"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    drop(peer);
    let started = std::time::Instant::now();
    loop {
        if supervisor.peer_tcp_presence().expect("closed presence") == PeerTcpPresence::Disconnected
        {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "closed TCP peer remained established"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    supervisor.stop().expect("stop");
    assert_eq!(
        supervisor.peer_tcp_presence().expect("stopped presence"),
        PeerTcpPresence::SupervisorNotRunning
    );
}

#[test]
#[ignore = "requires a user-supplied MicYou v2.0.1 CLI built from the pinned revision"]
fn probes_real_user_supplied_micyou_cli() {
    let executable = std::env::var_os("CAPYIO_MICYOU_CLI")
        .map(PathBuf::from)
        .expect("set CAPYIO_MICYOU_CLI");
    let inventory = MicYouProbe::new(ProbeLimits::default())
        .expect("probe")
        .inventory(&executable)
        .expect("real inventory");
    assert_eq!(inventory.version, "2.0.1");
}
