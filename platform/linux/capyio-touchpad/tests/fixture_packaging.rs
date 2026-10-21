//! Runs only the separately named software fixture, never the native helper.
#![cfg(all(target_os = "linux", feature = "test-fixture"))]
use std::{
    io::Write,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn packaged_fixture_runs_real_protocol_and_fake_backend() {
    let path = env!("CARGO_BIN_EXE_capyio-linux-touchpad-fixture-helper");
    assert_eq!(
        std::path::Path::new(path).file_name().unwrap(),
        "capyio-linux-touchpad-fixture-helper"
    );
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let mut header = Vec::from(&b"HPT1"[..]);
    header.extend_from_slice(&1500u32.to_le_bytes());
    header.extend_from_slice(&1000u32.to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes());
    header.extend_from_slice(&(now + 1500).to_le_bytes());
    // One complete SYN_REPORT-only batch, then terminal close.
    header.extend_from_slice(b"HPF1\x01\0\0\0\0\0\0\0\0\0\0\0HPF1\0\0\0\0");
    let mut child = Command::new(path)
        .env_clear()
        .env("CAPYIO_FAKE_MODE", "normal")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&header).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, [1, 1, 1]);
    let trace = String::from_utf8(output.stderr).unwrap();
    assert!(trace.starts_with("CREATE\nOPEN /dev/uinput "));
    assert!(trace.contains("FRAME_BEGIN 1\nEVENT 0 0 0\nFRAME_COMMITTED 1\n"));
    assert!(trace.contains("CLEANUP_BEGIN\n"));
    assert_eq!(trace.matches("EVENT 3 57 -1\n").count(), 5);
    assert!(trace.ends_with("EVENT 0 0 0\nDESTROY\n"));
}
