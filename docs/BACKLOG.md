# CapyIO Backlog

Tasks are small implementation slices. A task enters `docs/plans/active/` before
code changes. Roadmap presence is not authorization to begin hardware work.

## Completed foundation

### CAPY-FOUNDATION-001 — Gates 0–3 migration

Requirements: PRD v0.3 foundation acceptance.
Plan: `docs/plans/completed/0003-capyio-foundation-migration.md`.

Deliverables:

- CapyIO names, docs and ADRs;
- typed Node/Adapter/Capability/Port/Route/Session/Problem model;
- `capyio.v1` catalogs, Route control and Problems;
- deterministic four-Route UI;
- Adapter manifest/SDK/Host and mock Sidecars;
- validator, smoke commands and evidence.

## Completed foundation hardening

### CAPY-FOUNDATION-002 — Harden Adapter foundation and exact-head CI

Goal: make the Gate 3 process/control boundary safe enough for the next
low-rate StandardPort task without starting a real SensorServer or data plane.

In scope:

- bounded stdout/stderr reads and terminal poisoned-Host behavior;
- generic bounded Route control contracts;
- deployment-mode-specific manifest validation;
- scoped Route reconciliation after catalog change and backend support checks;
- checked PRD traceability, stale status cleanup and exact-head hosted gates.

Completion evidence: `docs/FOUNDATION_HARDENING_REPORT.md` and completed plan
`docs/plans/completed/0004-capyio-foundation-hardening.md`. All local gates
passed. PR #10 then passed repository, UI, Windows/Linux/macOS Rust/Adapter and
Windows Tauri hosted checks before merge commit `5f5b81f`.

Explicit non-scope: StandardPort payload transport, SensorServer, Android/phone,
drivers, production security, third-party source and physical-device tests.

## Completed product groundwork

### CAPY-IMU-001A — Fixture-first bounded StandardPort path

Goal: establish generic bounded StandardPort semantics, deterministic IMU
replay, independent numeric Panel/JSONL Recorder sinks and safe Android lab
inventory before importing SensorServer or installing an APK.

Completion evidence: `docs/CAPY_IMU_001A_REPORT.md` and completed plan
`docs/plans/completed/0005-fixture-first-imu-standard-path.md`.

Explicit non-scope: live phone payloads, SensorServer, APK install/permissions,
network transport, gamepad/VIIPER, driver work and production security.

## Completed product slice

### CAPY-IMU-001B — SensorServer IMU Source to Panel and Recorder

Goal: prove one real StandardPort path with a low-risk phone sensor before
system-driver work.

In scope:

- SensorServer Connection/Source Adapter;
- `capyio.motion.imu-samples/1` mapping with source timestamp, clock domain,
  sequence, units, coordinate frame, accuracy and sensor metadata;
- built-in numeric/3D IMU Panel;
- bounded Mock/initial Recorder output;
- explicit vivo physical-device test plan.

Out of scope:

- VIIPER, virtual gamepad, haptics, APK auto-install, production MCAP, ROS or
  other real Capabilities.

Acceptance:

1. recorded/mocked fixture tests run without a phone;
2. real phone test is separately authorized and retains version/device evidence;
3. Panel and Recorder can consume the same Profile independently;
4. stopping either Route does not stop the other;
5. disconnect creates explicit gaps/Problems rather than silent timestamp repair.

Implementation slices:

- `CAPY-IMU-001B0` (complete): pin upstream provenance and implement bounded
  SensorServer JSON parsing plus deterministic accelerometer/gyroscope pairing;
- `CAPY-IMU-001B1` (complete): add a reviewed WebSocket client dependency and local
  mock-server tests, without a phone;
- `CAPY-IMU-001B2` (complete): use the reconnected wireless ADB target, install/start
  the externally maintained app under device authorization, and retain live
  Panel/Recorder plus disconnect evidence;
- `CAPY-IMU-001B3A` (complete): project the bounded physical stream into a narrow
  Tauri numeric panel with visible failure/recovery/stop state;
- `CAPY-IMU-001B3B` (complete): bind the Adapter to the desktop's single real
  Node Runtime Route/Problem lifecycle, advance epochs across retry, and prove
  physical activation plus explicit stop. Evidence is in
  `docs/CAPY_IMU_001B3B_REPORT.md`.

## Active product slice

### CAPY-AUDIO-000/001A — Audio Share remote-speaker spike and external Adapter

Goal: first verify a pinned, unmodified Audio Share release on the authorized
Windows/Android lab, then wrap its data plane as one AdapterManaged Route from
Windows System Mix Source to Android Speaker Sink.

Initial acceptance: probe and enumerate `as-cmd` endpoints; start/stop the
server; retain real playback evidence; translate receiver loss to a structured
Problem and `Offline`; retry with a fresh epoch; stop without changing the IMU
Route; keep PCM and ordinary logs outside Sidecar stdout; expose the workflow
through a generic Quick Action projection rather than `UiLiveSpeaker`.

Progress:

- `CAPY-AUDIO-000` complete: v0.3.4 provenance/hashes/license and unmodified
  Windows/Android lab behavior are recorded. TCP negotiation, UDP PCM delivery,
  Android `AudioTrack`, heartbeat, close and a second clean start were observed;
  subjective audible quality remains unclaimed without a listener beside the
  phone.
- `CAPY-AUDIO-001A0` complete locally: bounded direct-process version/endpoint
  probing and explicit bind/endpoint/PCM configuration are implemented with no
  new production dependency. Runtime process supervision is the next slice.
- `CAPY-AUDIO-001A1` complete locally: the child is probed, started directly,
  checked for TCP-listener readiness, boundedly drained, polled, stopped and
  reaped with typed early-exit/timeout behavior. The upstream CLI exposes no
  machine-readable Android peer state; disconnect must not be inferred from
  ordinary logs and remains open for `001A2`.
- `CAPY-AUDIO-001A2A` complete locally: a Windows-only bounded IP Helper query
  observes established TCP rows owned by the supervised PID and explicit port,
  without retaining addresses or parsing logs. It is labeled receiver transport
  presence, not playback health.
- `CAPY-AUDIO-001A2B` complete locally: the desktop composition layer owns one
  `AdapterManaged` Runtime Route, requires three consecutive receiver samples
  for activation, reports receiver/process failures as typed `Offline`
  Problems, advances retry epochs, stops explicitly and leaves active IMU state
  unchanged.
- `CAPY-AUDIO-001A3` complete locally: a schema-v1 generic
  Quick Action projects the physical Route, exposes finite start/retry/stop
  operations, keeps executable/network/endpoint configuration in the host, and
  is polled by a host-owned worker rather than the WebView. The authorized lab
  passed Active epoch 1, disconnect/Offline epoch 2, retry Active epoch 3 and
  explicit stop, while Android reported a 48 kHz stereo Track with non-zero
  written frames.
- Post-`001A3` hardening bounds receiver startup, reaps and reports a retryable
  Problem on exhaustion, and uses a private-negotiated Route format because the
  upstream CLI can fall back to an endpoint default. A short physical
  screen-off run retained TCP and advanced the Android Track; secure lock,
  longer background, audio focus, latency and soak remain open.
- Endpoint drift now has a typed, sanitized start-time Problem. A later UX slice
  no longer strands the user on a stale host configuration.
- `CAPY-AUDIO-001A4` complete locally: the desktop card rescans bounded endpoint
  display names, selects through a fresh host-owned opaque-token allow-list and
  replaces configuration only while the Route is inactive. Raw endpoint IDs
  and executable paths never enter the WebView; selection is session-local.

Explicit `001A` non-scope: vendoring upstream source, a CapyIO Android app,
virtual render endpoint, microphone, codec rewrite, production
pairing/encryption or automatic retry policy. The newly authorized virtual
endpoint is isolated in `001B` rather than retrofitted into the proven transport
slice.

### CAPY-AUDIO-001B — Dedicated Windows `CapyIO Speaker`

Goal: expose a real Windows render endpoint that applications can select
independently of the physical/RDP output, then bridge only that endpoint's real
PCM through a bounded render APO/Broker path and feed the proven Android speaker transport.

The Speaker functional Gate through B6B is complete. The installed
`CapyIO Speaker` post-mix APO feeds a bounded global render ring, the
service-owned Broker sends it to the pinned Android receiver, and human tests
confirmed playback plus Windows endpoint volume/mute. ADRs 0033/0034 separate
the privileged Broker from Tauri and expose only bounded local
start/stop/status control to the ordinary desktop user. The completed evidence
plan is `docs/plans/completed/0012-windows-virtual-speaker.md`. Signed installer,
qualification, production security and Android distribution continue separately
in `docs/plans/active/0013-speaker-release-qualification.md`.

## Later small tasks

- `CAPY-GAMEPAD-001`: DSU projection from recorded IMU fixture.
- `CAPY-GAMEPAD-002`: VIIPER sidecar probe and license/build spike.
- `CAPY-AUDIO-002`: MicYou AdapterManaged Android→Windows path.
- `CAPY-CAMERA-001`: VCamdroid sidecar catalog/probe spike.
- `CAPY-UX-001`: versioned Quick Action template schema.
- `CAPY-SEC-001`: authenticated control-channel design spike.
- `CAPY-DRIVER-001`: provision the isolated Windows driver VM required by
  `CAPY-AUDIO-001B0`.

## Task safety checklist

- production dependencies have a reviewed note/ADR;
- public protocol changes reserve old fields and add compatibility tests;
- physical-device, APK, permission and driver actions have explicit approval;
- third-party source has provenance/license records before import;
- evidence never claims unrun hardware or platform tests.
