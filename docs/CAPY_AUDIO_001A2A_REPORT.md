# CAPY-AUDIO-001A2A Report

Date: 2026-08-24

Status: local Windows receiver-transport observation complete

## Delivered

- Windows-only `GetExtendedTcpTable` query through `windows-sys` 0.61.2;
- documented two-call sizing with a 16 MiB allocation bound;
- aligned storage and checked row-count/byte arithmetic around the unsafe FFI
  boundary;
- filtering by supervised process ID, explicit local port and TCP
  `ESTABLISHED` state;
- `ReceiverTcpPresence` states for not-running, unsupported platform,
  disconnected and established connection count;
- no local/remote address, endpoint ID or upstream log text in the result.

## Evidence

The Windows fixture test starts the supervised server and first observes
`Disconnected`, proving the short readiness probe does not remain a receiver.
A separate held TCP peer becomes `Established`; closing it returns to
`Disconnected`; stopping the process returns `SupervisorNotRunning`.

Targeted crate tests and Clippy with warnings denied pass. `cargo xtask ci`
passes with 108 Rust tests and 2 ignored real-CLI tests, repository validation,
Adapter smoke and frontend typecheck/build. Windows Tauri `cargo check` and
`cargo build` also pass with only the normal MSVC import-library informational
warning.

## Interpretation and limits

This is a machine-readable transport-presence signal. It does not prove the TCP
peer is an authorized Android app, that Audio Share negotiation completed, that
UDP PCM arrived, that Android submitted frames, or that sound was audible. The
current trusted lab remains unauthenticated, so an unrelated peer can create a
false presence signal. Runtime/UI labels must preserve these limits.
