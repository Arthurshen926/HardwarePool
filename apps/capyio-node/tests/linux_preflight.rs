#![cfg(target_os = "linux")]

use std::process::Command;

#[test]
fn linux_preflight_missing_session_is_json_not_readiness() {
    let output = Command::new(env!("CARGO_BIN_EXE_capyio-node"))
        .arg("linux-playback-preflight")
        .env_remove("XDG_RUNTIME_DIR")
        .env_remove("PIPEWIRE_REMOTE")
        .env_remove("PIPEWIRE_RUNTIME_DIR")
        .env_remove("RUST_LOG")
        .output()
        .expect("preflight process");
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON report");
    assert_eq!(value["readiness"], "not_verified");
    assert_eq!(value["default_pipewire_socket"], "runtime_directory_unset");
    assert_eq!(value["session"], "not_connected_or_verified");
    assert_eq!(value["target"], "not_supplied");
    assert_eq!(value["version"], 1);
    assert!(!output.stdout.windows(6).any(|value| value == b"Active"));
}
