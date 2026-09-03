# CAPY-PTP-002I — Real Windows factory create/close smoke

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-ROUTE-006..007`,
`NFR-STAB-001..004`, `NFR-SEC-001..003`, `NFR-MAINT-001..003`

## Objective

With explicit human authorization, prove that the authorized Runtime worker
factory path can create and destroy a real Windows synthetic touchpad without
submitting any contact frame.

## In scope

- one Windows-only exact-name ignored integration test;
- real `NodeRuntime` touchpad Route authorization and Starting epoch;
- `new_with_sink_factory` using `WindowsSyntheticTouchpadSinkFactory`;
- assertion of zero packets enqueued and processed;
- explicit worker close, synthetic-device destruction and Runtime stop;
- default CI isolation and retained command evidence.

## Out of scope

- Route activation, packet enqueue, frame pump or native input submission;
- pointer movement, clicks or multi-finger gesture observation;
- Android device/APK/runtime capture and live transport;
- driver installation, VHF, boot/security changes or persistent device state.

## Acceptance criteria

1. The test remains ignored during ordinary `cargo test` and CI.
2. The exact authorized invocation creates through the real Windows factory.
3. The worker remains Starting with zero enqueued/processed packets.
4. Explicit stop closes the Sink and Runtime cleanup reaches Stopped.
5. The test passes without input frames, driver changes or residual device.
6. Default full CI passes independently.

## Recovery

`SyntheticTouchpadSession` owns the synthetic handle through RAII. Explicit
worker stop is the primary rollback; Drop and process exit destroy the handle if
the test unwinds before the explicit close. No driver/package is installed.

## Required validation

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker
cargo xtask ci
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker authorized_windows_factory_opens_and_closes_real_synthetic_touchpad_without_frames -- --ignored --exact --nocapture
cargo xtask validate-docs
git diff --check
```

## Completion evidence

- Default worker tests report eight passed and one ignored.
- Full default repository CI passes without creating a touchpad device.
- The separately authorized exact ignored test reports one passed, eight
  filtered out.
- The test opened the real factory after Runtime Route preflight, submitted no
  packets/frames, explicitly closed and completed logical Runtime stop.

Detailed evidence: `docs/CAPY_PTP_002I_REPORT.md`.
