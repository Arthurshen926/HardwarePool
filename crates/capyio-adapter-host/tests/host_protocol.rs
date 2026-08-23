use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use capyio_adapter_host::{
    HostError, MAX_STDERR_LINE_BYTES, SidecarHost, SidecarHostOptions, SidecarHostState,
};
use capyio_adapter_sdk::{AdapterManifest, RpcError};

fn fixture_executable() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_capyio-host-fixture"))
}

fn manifest() -> AdapterManifest {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../adapters/mock-source/adapter.json");
    AdapterManifest::from_json(&fs::read(path).expect("read Mock manifest"))
        .expect("valid Mock manifest")
}

fn spawn(mode: &str, deadline: Duration) -> SidecarHost {
    SidecarHost::spawn_with_options(
        &fixture_executable(),
        [mode],
        manifest(),
        SidecarHostOptions {
            response_deadline: deadline,
        },
    )
    .expect("spawn Host fixture")
}

#[test]
fn timeout_poisons_and_late_response_cannot_be_reused() {
    let mut host = spawn("late-first-response", Duration::from_millis(25));
    assert!(matches!(
        host.probe(),
        Err(HostError::ResponseDeadline { .. })
    ));
    assert_eq!(host.state(), SidecarHostState::Poisoned);
    assert!(matches!(host.health(), Err(HostError::SidecarPoisoned)));
}

#[test]
fn oversized_stdout_without_newline_is_a_terminal_protocol_failure() {
    let mut host = spawn("oversized-stdout", Duration::from_secs(1));
    assert!(matches!(
        host.probe(),
        Err(HostError::Rpc(RpcError::LineTooLarge { .. }))
    ));
    assert_eq!(host.state(), SidecarHostState::Poisoned);
    assert!(matches!(host.probe(), Err(HostError::SidecarPoisoned)));
}

#[test]
fn unexpected_response_id_poisons_the_sequential_channel() {
    let mut host = spawn("unexpected-id", Duration::from_secs(1));
    assert!(matches!(
        host.probe(),
        Err(HostError::Rpc(RpcError::UnexpectedResponseId(_)))
    ));
    assert_eq!(host.state(), SidecarHostState::Poisoned);
}

#[test]
fn malformed_response_poisons_the_sequential_channel() {
    let mut host = spawn("malformed-response", Duration::from_secs(1));
    assert!(matches!(
        host.probe(),
        Err(HostError::Rpc(RpcError::Json(_)))
    ));
    assert_eq!(host.state(), SidecarHostState::Poisoned);
}

#[test]
fn closed_stdout_poisons_and_reaps_the_sidecar() {
    let mut host = spawn("closed-stdout", Duration::from_secs(1));
    assert!(matches!(host.probe(), Err(HostError::UnexpectedExit(_))));
    assert_eq!(host.state(), SidecarHostState::Poisoned);
    assert!(matches!(host.health(), Err(HostError::SidecarPoisoned)));
}

#[test]
fn oversized_stderr_without_newline_is_retained_as_one_bounded_prefix() {
    let mut host = spawn("oversized-stderr", Duration::from_secs(1));
    assert!(host.probe().expect("probe").ready);
    host.shutdown().expect("shutdown");
    let lines = host.stderr_lines();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].ends_with(" [truncated]"));
    assert!(lines[0].len() <= MAX_STDERR_LINE_BYTES);
}
