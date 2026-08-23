# CapyIO Build Status

> Updated: 2026-08-23 during `CAPY-FOUNDATION-001`.

## Verified baseline

At commit `d33d585`, before migration:

- repository validation passed;
- Rust workspace check passed;
- 34 Rust tests passed;
- Vue typecheck/build passed;
- hosted Windows/Linux/macOS Rust CI and UI/static jobs had passed on prior
  bootstrap commits.

Exact evidence is in `docs/BASELINE_REPORT.md`.

## Active migration

- Gate 0: complete.
- Gate 1: complete locally — names/docs/ADRs, dependency paths, Rust check and
  pinned pnpm UI build passed.
- Gate 2: implemented — symmetric Nodes, typed Port/Route Core, generic
  protocol/Runtime, four independent Routes and Quick Actions/Workspace UI.
- Gate 3: implemented — manifest/schema, bounded NDJSON codec, Sidecar Host,
  finite Mock Source/Sink and scoped crash isolation.

Full Rust checks (42 tests), Clippy, manifest validation, Adapter Smoke, pnpm
typecheck/build, repository validation and `cargo xtask ci` pass. Evidence and
remaining risks are recorded in `docs/GATE_0_3_REPORT.md`.

## Not built or tested

- Android application/APK or phone;
- Android permissions/foreground services;
- real microphone, speaker, camera, IMU or input data path;
- Windows virtual devices, driver, WDK or isolated-VM driver test;
- production transport, pairing, encryption, third-party Adapter or performance.

These absences are expected and must not be reported as working functionality.
