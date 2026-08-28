# CAPY-AUDIO-001A1 Report

Date: 2026-08-24

Status: local child-supervisor slice complete

## Delivered

- direct `std::process::Command` startup with explicit validated server
  arguments and no shell;
- pinned version and configured endpoint probe before every start;
- five-second default startup deadline and explicit TCP-listener readiness;
- typed already-running, early-exit and startup-timeout outcomes;
- fixed-capacity stdout/stderr retention that continues draining after overflow;
- running/terminal/stopped status without exposing raw upstream log text;
- idempotent stop, kill-race handling, synchronous wait/reap and Drop cleanup;
- a repository-built fixture executable covering process behavior without Audio
  Share, hardware, network peers or system audio.

## Evidence

- default crate tests: 10 passed, 2 ignored (7 parser/config plus 3 supervisor);
- targeted Clippy with warnings denied: passed;
- `cargo xtask ci`: passed with 107 Rust tests, 2 ignored hardware/real-CLI
  tests, repository validation, Adapter smoke and frontend typecheck/build;
- Windows Tauri `cargo check` and `cargo build`: passed (MSVC emitted only its
  normal import-library informational warning);
- ignored real-server test: passed against the hash-verified v0.3.4 CLI on an
  explicit loopback port;
- post-test inventory: zero TCP listeners on the test port and zero `as-cmd`
  processes.

The real test briefly opened Windows system-loopback capture but connected no
Android peer, retained no audio and exposed no payload to CapyIO.

## Observability limit

The v0.3.4 CLI exposes version, endpoint enumeration and a long-running server,
but no machine-readable connected-peer/heartbeat status. It logs accept/close
messages for people, yet CapyIO's architecture forbids parsing human prose as
behavior. Therefore this slice can prove server process and TCP listener health,
not Android receiver presence. Claiming receiver loss as `Offline` now would be
a false positive/negative risk.

`CAPY-AUDIO-001A2` must either establish a stable machine contract at an
explicit Adapter boundary, narrow the Route's Active semantics to server-only
availability with honest receiver-unknown state, or reconsider the upstream
integration. It must not silently promote listener readiness to playback health.
