# CapyIO Build Status

> Updated: 2026-08-24 during `CAPY-FOUNDATION-002`.

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

Hosted results for this exact head are pending until the branch is pushed and
GitHub Actions completes; workflow configuration is not reported as a hosted
pass. Linux/macOS native Tauri packaging is an explicit current skip, while
their Rust/Adapter and web UI jobs remain required.

## Not built or tested

- Android application/APK or phone;
- Android permissions/foreground services;
- real microphone, speaker, camera, IMU or input data path;
- Windows virtual devices, driver, WDK or isolated-VM driver test;
- production transport, pairing, encryption, third-party Adapter or performance.

These absences are expected and must not be reported as working functionality.
