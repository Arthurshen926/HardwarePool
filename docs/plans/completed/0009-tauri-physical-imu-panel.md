# CAPY-IMU-001B3A — Tauri physical IMU panel

Status: complete

Owner: Codex

Created: 2026-08-24

Completed: 2026-08-24

Requirements: `FR-PORT-004`, `FR-DIAG-001`, `FR-UX-005`, `NFR-SEC-004..005`, `NFR-STAB-002..005`

## Objective

Project the bounded SensorServer Adapter into a user-visible Windows Tauri
numeric panel with honest connection state, explicit stop and retained physical
evidence.

## In scope

- narrow start/read/stop Tauri DTO commands;
- worker-owned accelerometer and gyroscope clients;
- live acceleration, angular velocity, clock, epoch, sequence and count state;
- visible connecting, active, failed and stopped states;
- deterministic Browser Mock compatibility;
- one authorized physical Android/Windows UI run.

## Out of scope

- production Node Runtime Route orchestration or background service lifecycle;
- automatic retry, authentication, encryption or WAN transport;
- a 3D renderer, Recorder UI, CapyIO Android APK, microphone or speaker;
- Windows system projection or drivers.

## Completion record

- invalid/broad endpoint input is excluded by the typed IP/port command;
- desktop Rust check, Clippy and unit tests passed;
- an ignored physical backend test received real paired samples and stopped;
- frontend typecheck/build passed;
- visual interaction proved an explicit timeout Problem, recovery after the
  phone service restarted, an active count growing past 100 samples, and a
  stopped snapshot retaining 226 received samples;
- no physical address, pairing code or raw device identifier is committed.

Follow-up: `CAPY-IMU-001B3B` integrates the Adapter with real Runtime Route and
Problem lifecycle instead of the temporary desktop-lab controller.
