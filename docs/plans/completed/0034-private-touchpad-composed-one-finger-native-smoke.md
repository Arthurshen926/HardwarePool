# CAPY-PTP-002J — Composed one-finger native submission smoke

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-ROUTE-006..007`,
`NFR-STAB-001..004`, `NFR-SEC-001..003`, `NFR-MAINT-001..003`

## Objective

With separate explicit human authorization, submit a fixed closed one-finger
motion through the entire in-process Route worker and real Windows synthetic
touchpad composition.

## In scope

- one Windows-only exact-name ignored desktop-input integration test;
- real Runtime Route activation after authorized Sink construction;
- four compiled frames: CancelAll, one-finger down, horizontal move and release;
- private packet encode, bounded enqueue, one-packet pump and native Sink submit;
- exact four-enqueued/four-processed assertions;
- explicit Sink close/device destruction and Runtime stop;
- default CI isolation and evidence recording.

## Out of scope

- click, drag, tap, scroll or multi-finger gestures;
- user-supplied frames, files, sockets or live Android input;
- authenticated network transport and production Node task scheduling;
- driver installation, VHF or security/boot changes;
- claiming visually observed pointer movement from process exit status alone.

## Acceptance criteria

1. The test remains ignored during ordinary test and CI runs.
2. The exact authorized invocation activates a real Runtime Route.
3. All four fixed frames cross codec, queue, receiver and Windows Sink.
4. The release frame commits before worker close.
5. Metrics report four enqueued and four processed packets.
6. Worker close and Runtime stop complete without residual device state.
7. Default full CI passes independently.

## Recovery

The compiled sequence ends with an empty release snapshot. Worker stop closes
the session after release. Receiver/session/device Drop and process exit remain
fallback cancellation/destruction paths if the test unwinds.

## Required validation

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker
cargo xtask ci
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker authorized_worker_submits_one_finger_motion_then_releases_and_closes -- --ignored --exact --nocapture
cargo xtask validate-docs
git diff --check
```

## Completion evidence

- Default worker tests report eight passed and two ignored.
- Full default repository CI passes without device/input execution.
- The separately authorized exact test reports one passed and nine filtered.
- Four packet/pump operations completed, followed by explicit release, close and
  Runtime stop.

Detailed evidence: `docs/CAPY_PTP_002J_REPORT.md`.
