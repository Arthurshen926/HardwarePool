# CapyIO Build Status

> Updated: 2026-08-24 during `CAPY-IMU-001B0`.

## Verified baseline

At commit `d33d585`, before migration:

- repository validation passed;
- Rust workspace check passed;
- 34 Rust tests passed;
- Vue typecheck/build passed;
- hosted Windows/Linux/macOS Rust CI and UI/static jobs had passed on prior
  bootstrap commits.

Exact evidence is in `docs/BASELINE_REPORT.md`.

## Foundation status

- Gate 0: complete.
- Gate 1: complete locally — names/docs/ADRs, dependency paths, Rust check and
  pinned pnpm UI build passed.
- Gate 2: implemented — symmetric Nodes, typed Port/Route Core, generic
  protocol/Runtime, four independent Routes and Quick Actions/Workspace UI.
- Gate 3: implemented — manifest/schema, bounded NDJSON codec, Sidecar Host,
  finite Mock Source/Sink and scoped crash isolation.

The original Gate 0–3 run passed 42 Rust tests, Clippy, manifest validation,
Adapter Smoke, pnpm typecheck/build, repository validation and `cargo xtask ci`.
That historical evidence is recorded in `docs/GATE_0_3_REPORT.md`.

## CAPY-FOUNDATION-002 hardening

The current working tree hardens Gate 3 without adding a real data plane or
hardware Adapter:

- stdout/stderr are bounded while reading, including newline-free overflow;
- terminal sequential-control failures poison the Host, close stdin and reap
  the Sidecar before later requests are rejected;
- generic Route prepare/start/stop/status contracts replace Mock-specific Host
  return types while the finite sample remains private to Mock code;
- manifest deployment validation covers InProcess, Sidecar, ExternalService and
  DriverBacked metadata without embedding install commands;
- catalog replacement invalidates only dependent Routes, emits a Problem and
  advances epoch on compatible recovery;
- Route backend support is checked against Adapter and interoperability modes;
- 84 PRD IDs have checked status/Gate/evidence traceability, with automated
  duplicate and malformed-ID rejection;
- pull-request workflows check out the exact PR head; Rust/Adapter gates target
  Windows, Linux and macOS, UI uses a frozen lockfile, and Windows has an
  additional Tauri check/build gate.

The full local matrix passed: repository self-tests/validation, Rust
format/check/Clippy, 70 workspace tests, docs, two manifests, Adapter Smoke,
`cargo xtask ci`, frozen pnpm install/typecheck/build and Windows Tauri
check/build. Exact command and test-count evidence is retained in
`docs/FOUNDATION_HARDENING_REPORT.md`.

PR #10 passed repository, UI, Windows/Linux/macOS Rust/Adapter and Windows
Tauri exact-head checks before merge commit `5f5b81f`. Linux/macOS native Tauri
packaging remains an explicit skip; their Rust/Adapter jobs did pass.

## CAPY-IMU-001A fixture-first path

The active branch adds a real in-process semantic data path, not a live phone
transport:

- bounded Profile/stream/epoch/sequence/timestamp envelopes;
- explicit gap, duplicate, late, wrong-stream, stale/future-epoch and full
  outcomes;
- independent per-consumer queues and lifecycle for Panel and Recorder;
- deterministic `capyio.motion.imu-samples/1` JSONL fixture with SI units,
  coordinate frame, accuracy, calibration and sensor metadata;
- headless replay plus shared Browser/Tauri numeric Panel and Recorder summary;
- safe Android doctor/baseline/collect commands requiring an explicit serial.

Full workspace format/check/Clippy, 79 Rust tests, repository validation,
headless replay, `cargo xtask ci`, frozen frontend install/typecheck/build and
Windows Tauri check/build passed. A real read-only vivo inventory also passed;
the ignored artifact is sanitized and explicitly says no APK, permission change
or live CapyIO stream. Exact evidence is in `docs/CAPY_IMU_001A_REPORT.md`.

## CAPY-IMU-001B0 SensorServer contract

The active slice has started the first real-Adapter boundary without opening a
network connection or claiming phone data:

- upstream SensorServer commit and GPL-3.0-only external-service provenance are
  recorded with no source or binary import;
- its documented JSON message is bounded and strictly parsed;
- asynchronous accelerometer/gyroscope readings pair deterministically with
  explicit skew, replacement, regression and sequence-exhaustion behavior;
- original component timestamps survive in the IMU Profile.

Full workspace format/check/Clippy, 88 Rust tests, repository validation,
manifests, Adapter Smoke, fixture replay, frontend typecheck/build and
`cargo xtask ci` passed. Exact evidence is in
`docs/CAPY_IMU_001B0_REPORT.md`.

## CAPY-IMU-001B1 bounded WebSocket client

The active slice adds the first concrete local-lab transport inside the
SensorServer Adapter:

- user-approved Tungstenite 0.30.0, handshake-only, with no Tokio or TLS stack;
- IP-literal endpoint and fixed Android sensor paths;
- TCP connect/read/write deadlines and 4 KiB frame/message bounds;
- typed text/control/close/timeout/error outcomes;
- loopback mock-server tests for success and abnormal frames.

Complete locally. Seven WebSocket loopback tests cover the fixed endpoint,
valid text, ping/pong, close, timeout, malformed/binary/oversized messages and
oversized handshake behavior. Together with the nine parser/pairing tests, the
SensorServer Adapter has 16 contract tests. Full workspace format/check/Clippy,
95 Rust tests, repository validation, manifests, Adapter Smoke, fixture replay,
frontend typecheck/build and `cargo xtask ci` passed. Exact evidence is in
`docs/CAPY_IMU_001B1_REPORT.md`.

## Not built or tested

- Android application/APK or live phone payload path;
- Android permissions/foreground services;
- real microphone, speaker, camera, IMU or input data path;
- Windows virtual devices, driver, WDK or isolated-VM driver test;
- production transport, pairing, encryption, live third-party Adapter transport
  or performance.

These absences are expected and must not be reported as working functionality.
