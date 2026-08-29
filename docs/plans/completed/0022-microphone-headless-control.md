# CAPY-MIC-003A — Per-user headless microphone control

Status: completed source slice; deployment qualification remains in `CAPY-MIC-003`

Owner: Codex and project owner

Created: 2026-08-29

Completed: 2026-08-29

Depends on: completed `CAPY-MIC-001/002`; release backlog `CAPY-MIC-003`

## Objective

Remove the desktop window as the only owner of the MicYou receiver while
preserving the existing user-local trust boundary, independent microphone
Route and privileged capture-ring ownership.

## In scope

1. Add an ordinary-user headless MicYou host with bounded start, peer-wait,
   disconnect, failure, retry and stop state.
2. Reuse one Windows local named-pipe transport implementation for Speaker and
   microphone control while keeping their schemas and lifecycle independent.
3. Add a per-user, remote-rejecting microphone pipe whose closed schema accepts
   only `status`, `start` and `stop` and never carries launch/endpoint authority.
4. Make CapyIO Desktop prefer the headless host, adopt an already-running host
   and preserve it on desktop shutdown, with the direct supervisor retained as
   a development fallback.
5. Add hardware-free state-machine, schema, ACL construction and real local
   pipe round-trip tests on Windows.

## Out of scope

- installing the host, registering login autostart or changing services;
- running MicYou, installing/updating an APK or changing Android permissions;
- updating the Windows driver/APO, restarting Windows Audio or rebooting;
- peer authentication, encryption, GPL binary distribution or release signing;
- claiming PCM/audibility from process or TCP state.

## Acceptance criteria

1. Stable peer presence activates the host; initial wait and active peer loss
   stop/reap it with typed bounded state.
2. Unknown request fields/versions fail and responses disclose no executable
   path or endpoint identity.
3. The real named pipe accepts same-owner local control and rejects remote use
   at the Win32 pipe mode boundary.
4. Desktop startup uses a short bounded headless probe, keeps direct fallback
   and does not stop a headless Route when the UI closes.
5. Existing Speaker control v1 remains compatible and all available local
   repository gates pass.

## Safety

This slice changes source, documentation and hardware-free tests only. It does
not authorize deployment, service/autostart mutation, a driver/APK operation,
audio-service restart, reboot, signing, commit, push or pull request.

## Completion evidence

- `capyio-microphone-host` now reads the existing fixed user-local trusted
  configuration and owns a bounded MicYou lifecycle independently of Tauri.
- Speaker v1 and microphone v1 keep independent schemas/pipes while sharing
  one 4 KiB/deadline/remote-rejecting named-pipe transport implementation.
- Microphone pipe DACL construction and real same-owner status/start/stop round
  trips passed against a fake Runtime; the test uses a non-production pipe name.
- Desktop tests prove an already-active headless host is adopted after stable
  observations and receives no stop request during UI shutdown. Direct trusted
  configuration remains covered as the fallback.
- `cargo test -p capyio-windows-service --all-targets` passed 16 tests;
  `cargo test -p capyio-desktop --lib` passed 32 tests with 5 physical tests
  intentionally ignored; both package Clippy runs passed with warnings denied.
- `cargo xtask ci` passed the Rust workspace, documentation/manifests, Adapter
  smoke, repository structure, frontend typecheck and frontend build. A final
  targeted rerun after the test-only pipe-name isolation also passed.
- `capyio-microphone-host --run-for-ms 500` completed successfully using the
  current trusted configuration without starting MicYou.
- No service/autostart/driver/APK/permission/audio-service/boot state changed,
  and no physical microphone or persistence claim was made.
