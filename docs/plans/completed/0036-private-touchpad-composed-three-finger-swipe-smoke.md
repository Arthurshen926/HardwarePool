# CAPY-PTP-002L — Composed three-finger swipe native smoke

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-ROUTE-006..007`,
`NFR-STAB-001..004`, `NFR-SEC-001..003`, `NFR-MAINT-001..003`

## Objective

With separate human authorization, submit the fixed closed three-contact
horizontal swipe through the complete in-process Route worker and real Windows
synthetic touchpad composition.

## In scope

- one Windows-only exact-name ignored desktop-input integration test;
- real Runtime Route activation and Windows Sink construction;
- the existing deterministic three-finger fixture: CancelAll, eight gradual
  updates, empty release and final CancelAll;
- stable contact IDs 1..3, x movement from 2500 to 7500 and fixed y positions
  2000, 3000 and 4000;
- 15 ms submission intervals, exact packet metrics and full cleanup.

## Out of scope

- four-finger input or arbitrary/user-provided gestures;
- claiming visible desktop/application switching from API success alone;
- live Android input, authenticated transport or production task scheduling;
- driver installation or persistent system changes.

## Acceptance criteria

1. The test is ignored by ordinary tests and full CI.
2. Three contact IDs remain stable across all eight update frames.
3. Eleven packets cross the complete real composed path.
4. Empty release and final CancelAll precede close.
5. Worker/device and Runtime stop cleanly with no residual state.
6. Default CI and the separately authorized exact test pass.

## Recovery

The fixture ends with an empty release and CancelAll. Worker close and Runtime
stop are explicit; receiver/session/device Drop and process exit remain fallback
cleanup.

## Required validation

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo xtask ci
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker authorized_worker_submits_three_finger_swipe_then_releases_and_closes -- --ignored --exact --nocapture
cargo xtask validate-docs
git diff --check
```

## Completion evidence

- Default worker tests report eight passed and four ignored.
- Targeted Clippy and full default repository CI pass without device/input
  execution.
- The authorized exact test reports one passed and eleven filtered.
- Eleven packets were submitted, the three contacts were released and
  cancelled, then Sink/device and Runtime cleanup completed.

Detailed evidence: `docs/CAPY_PTP_002L_REPORT.md`.
