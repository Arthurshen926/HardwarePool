# CAPY-IMU-001B3B — Runtime-owned physical IMU Route

Status: complete

Owner: Codex

Created: 2026-08-24

Completed: 2026-08-24

Requirements: `FR-ROUTE-003..005`, `FR-DIAG-001..002`, `FR-SESSION-003`, `NFR-STAB-002..004`

## Objective

Move the physical SensorServer lab lifecycle from an isolated Tauri status
controller into a real `NodeRuntime` Route with structured Problems, sequenced
events and deterministic epoch changes.

## Delivered

- staged Runtime authorization, preparation, start, activation, recovery,
  offline Problem, stop and completion commands;
- atomic Adapter plus initial Capability catalog registration;
- one SensorServer Source to built-in Panel Sink Route in the desktop Node's
  single Runtime;
- Tauri projection of Route ID/state, epoch and stable Problem code;
- loopback activation, disconnect, retry/epoch and stop tests;
- authorized physical activation and explicit-stop verification.

## Excluded

- production pairing, authentication, encryption or automatic retry policy;
- a long-lived out-of-process Node service;
- 3D rendering, Recorder product UI, audio, system projection or drivers;
- changing the four deterministic Quick Action fixtures into physical claims.

## Evidence

See `docs/CAPY_IMU_001B3B_REPORT.md`.
