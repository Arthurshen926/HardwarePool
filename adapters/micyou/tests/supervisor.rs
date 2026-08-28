use std::{
    net::{IpAddr, Ipv4Addr, TcpListener},
    path::PathBuf,
    time::Duration,
};

use capyio_micyou_adapter::{
    MicYouConfig, MicYouProbe, MicYouSupervisor, ProbeLimits, SupervisorLimits, SupervisorStatus,
};

const DEVICE: &str = "CapyIO Fixture Microphone Ingress";

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
        DEVICE,
    )
    .expect("config");
    let inventory = MicYouProbe::new(ProbeLimits::default())
        .expect("probe")
        .probe_config(&config)
        .expect("inventory");
    assert_eq!(inventory.version, "2.0.1");
    assert_eq!(inventory.output_devices, [DEVICE]);
}

#[test]
fn starts_polls_stops_and_reaps_fixture() {
    let config = MicYouConfig::new(
        fixture(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        unused_port(),
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
