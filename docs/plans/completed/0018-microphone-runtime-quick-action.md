# CAPY-MIC-001F — Runtime-owned microphone Quick Action

Status: completed (controlled-lab functional Gate); release qualification moved to plan 0021

Owner: Codex and project owner

Created: 2026-08-29

Completed: 2026-08-29 in PR #14, merge commit `2145f9f`

Depends on: `CAPY-MIC-001D`, `CAPY-MIC-001E`

## Objective

Project the proven MicYou Android-to-Windows microphone path into the desktop
Node Runtime and the versioned Quick Action contract. Starting and stopping the
workflow must mutate one typed `AdapterManaged` Route, expose bounded status and
Problems, and never accept an executable path or raw Windows endpoint ID from
the WebView.

## In scope

1. Add bounded process-owned TCP peer observation to the MicYou supervisor on
   Windows, with an explicit unsupported result on other platforms.
2. Register Android microphone Source and Windows `CapyIO Microphone` Sink
   catalogs and one independent `AdapterManaged` Route.
3. Keep the Route `Starting` until stable phone TCP presence is observed, move
   it `Offline` on peer/process loss and advance its epoch on explicit retry.
4. Add a trusted-host-configured microphone Quick Action to Tauri and a matching
   blocked Browser Mock DTO. The UI may show bind IP/port, but not the stable
   Windows endpoint ID or local executable path.
5. Prove orchestration with a fake process boundary, including start failure,
   bounded connection wait, disconnect, retry, stop and unrelated-Route
   isolation.

## Out of scope

- changing or reinstalling the Windows audio driver or Android APK;
- persisting trusted host configuration from the WebView;
- claiming that TCP presence proves microphone frames, audible quality or
  permission state;
- moving the Runtime into a production Windows service in this slice;
- distributing the locally patched GPL MicYou executable.

## Acceptance criteria

1. Tauri and Browser Mock expose the same versioned two-action DTO contract.
2. Unknown fields, action IDs and WebView-supplied paths/endpoint IDs fail.
3. Process readiness alone does not activate the microphone Route; stable phone
   TCP presence does.
4. Phone loss affects only the microphone Route and retains a typed bounded
   Problem; retry creates a later epoch.
5. UI start/stop does not implicitly mutate Speaker or IMU Routes.
6. Adapter tests, Tauri tests, frontend typecheck/build, repository validation
   and full CI pass.

## Safety

This slice performs source, build and loopback process tests only. It does not
authorize a driver/APK update, permission change, service change, reboot,
signing, commit, push or pull request.

## Completion evidence

- The shared Windows helper reports only a bounded count of process-owned TCP
  peers and keeps Win32 parsing and `unsafe` code outside the MicYou Adapter.
- The Runtime owns a typed Android microphone Source, Windows microphone Sink
  and independent `AdapterManaged` Route. Tests cover stable connection,
  bounded wait, disconnect, retry, stop, sanitized endpoint failure and IMU
  Route isolation.
- Tauri and Browser Mock expose Quick Action schema v2 with separate Speaker
  and microphone actions. The microphone action exposes only a connection hint;
  the executable path and raw endpoint ID remain trusted-host data.
- `cargo xtask ci` passed on 2026-08-29, including Rust formatting, checks,
  Clippy, workspace tests, documentation/manifests, Adapter smoke tests,
  repository validation and frontend typecheck/build.
- The separately authorized CAPY-MIC-001H controlled-lab run exercised this
  Quick Action through start, stable phone presence, ordinary-client PCM,
  disconnect, exact silence, retry with restored PCM and terminal stop. Active
  phone loss now stops and reaps the receiver before reporting `Offline`.
- No driver/APK installation, Android permission change, audio-service change
  or reboot was performed by that acceptance run. Release qualification still
  requires the lifecycle and soak work listed in CAPY-MIC-001H.
