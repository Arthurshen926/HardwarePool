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

Explicit non-scope: vendoring upstream source, a CapyIO Android app, virtual
render endpoint, microphone, codec rewrite, production pairing/encryption or
automatic retry policy.

## Later small tasks

- `CAPY-GAMEPAD-001`: DSU projection from recorded IMU fixture.
- `CAPY-GAMEPAD-002`: VIIPER sidecar probe and license/build spike.
- `CAPY-AUDIO-002`: MicYou AdapterManaged Android→Windows path.
- `CAPY-CAMERA-001`: VCamdroid sidecar catalog/probe spike.
- `CAPY-UX-001`: versioned Quick Action template schema.
- `CAPY-SEC-001`: authenticated control-channel design spike.
- `CAPY-DRIVER-001`: provision isolated Windows driver test VM; no driver code.

## Task safety checklist

- production dependencies have a reviewed note/ADR;
- public protocol changes reserve old fields and add compatibility tests;
- physical-device, APK, permission and driver actions have explicit approval;
- third-party source has provenance/license records before import;
- evidence never claims unrun hardware or platform tests.
