use std::{
    net::{IpAddr, Ipv4Addr, TcpListener},
    path::PathBuf,
    time::Duration,
};

use capyio_audio_share_adapter::{
    AudioEncoding, AudioShareConfig, AudioShareError, AudioShareSupervisor, ProbeLimits,
    SupervisorLimits, SupervisorStatus,
};

const DEFAULT_ENDPOINT: &str = "fixture-default";
const EXIT_ENDPOINT: &str = "fixture-exit";
const NO_LISTEN_ENDPOINT: &str = "fixture-no-listen";
const SPAM_ENDPOINT: &str = "fixture-spam";

fn fixture_executable() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_capyio-audio-share-fixture"))
}

fn unused_loopback_port() -> u16 {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("ephemeral loopback");
    listener.local_addr().expect("local address").port()
}

fn config(endpoint: &str) -> AudioShareConfig {
    AudioShareConfig::new(
        fixture_executable(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        unused_loopback_port(),
        endpoint,
        AudioEncoding::F32,
        2,
        48_000,
    )
    .expect("fixture config")
}

fn supervisor(endpoint: &str, process_output_bytes: usize) -> AudioShareSupervisor {
    AudioShareSupervisor::new(
        config(endpoint),
        ProbeLimits {
            deadline: Duration::from_secs(2),
            ..ProbeLimits::default()
        },
        SupervisorLimits {
            startup_deadline: Duration::from_secs(2),
            process_output_bytes,
        },
    )
    .expect("supervisor")
}

#[test]
fn starts_polls_stops_and_reaps_idempotently() {
    let mut supervisor = supervisor(DEFAULT_ENDPOINT, 4096);
    let started = supervisor.start().expect("start fixture server");
    assert!(started.tcp_listener_ready);
    assert_eq!(
        supervisor.status().expect("poll"),
        SupervisorStatus::Running {
            process_id: started.process_id
        }
    );

    let stopped = supervisor.stop().expect("stop and reap");
    assert!(stopped.was_running);
    assert_eq!(stopped.process_id, Some(started.process_id));
    assert_eq!(
        supervisor.status().expect("stopped status"),
        SupervisorStatus::Stopped
    );
    assert!(!supervisor.stop().expect("idempotent stop").was_running);
}

#[test]
fn early_exit_and_startup_timeout_are_typed_and_reaped() {
    let mut early_exit = supervisor(EXIT_ENDPOINT, 4096);
    assert!(matches!(
        early_exit.start(),
        Err(AudioShareError::SupervisorExitedBeforeReady {
            exit_code: Some(23)
        })
    ));
    assert!(matches!(
        early_exit.status().expect("terminal status"),
        SupervisorStatus::Exited(report) if report.exit_code == Some(23)
    ));

    let mut no_listener = AudioShareSupervisor::new(
        config(NO_LISTEN_ENDPOINT),
        ProbeLimits::default(),
        SupervisorLimits {
            startup_deadline: Duration::from_millis(100),
            process_output_bytes: 4096,
        },
    )
    .expect("supervisor");
    assert!(matches!(
        no_listener.start(),
        Err(AudioShareError::SupervisorStartupTimedOut)
    ));
    assert_eq!(
        no_listener.status().expect("timed out process is reaped"),
        SupervisorStatus::Stopped
    );
}

#[test]
fn long_running_output_is_drained_with_bounded_retention() {
    let mut supervisor = supervisor(SPAM_ENDPOINT, 128);
    supervisor.start().expect("start spam fixture");
    let stopped = supervisor.stop().expect("stop spam fixture");
    let output = stopped.output.expect("output summary");
    assert_eq!(output.stdout_retained_bytes, 128);
    assert!(output.stdout_overflowed);
}

#[test]
#[ignore = "starts a user-supplied, hash-verified Audio Share server and system loopback capture"]
fn supervises_real_user_supplied_audio_share_server() {
    let executable = std::env::var_os("CAPYIO_AUDIO_SHARE_EXE")
        .map(PathBuf::from)
        .expect("set CAPYIO_AUDIO_SHARE_EXE explicitly");
    let endpoint = std::env::var("CAPYIO_AUDIO_SHARE_ENDPOINT")
        .expect("set CAPYIO_AUDIO_SHARE_ENDPOINT explicitly");
    let port = std::env::var("CAPYIO_AUDIO_SHARE_PORT")
        .expect("set CAPYIO_AUDIO_SHARE_PORT explicitly")
        .parse::<u16>()
        .expect("port is a u16");
    let config = AudioShareConfig::new(
        executable,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        port,
        endpoint,
        AudioEncoding::F32,
        2,
        44_100,
    )
    .expect("real server config");
    let mut supervisor =
        AudioShareSupervisor::new(config, ProbeLimits::default(), SupervisorLimits::default())
            .expect("real supervisor");
    let started = supervisor.start().expect("real server starts and listens");
    assert!(matches!(
        supervisor.status().expect("real server status"),
        SupervisorStatus::Running { process_id } if process_id == started.process_id
    ));
    assert!(
        supervisor
            .stop()
            .expect("real server stop and reap")
            .was_running
    );
    assert_eq!(
        supervisor.status().expect("real stopped status"),
        SupervisorStatus::Stopped
    );
}
