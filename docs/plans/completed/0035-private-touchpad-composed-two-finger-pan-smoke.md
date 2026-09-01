# CAPY-PTP-002K — Composed two-finger pan native smoke

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-ROUTE-006..007`,
`NFR-STAB-001..004`, `NFR-SEC-001..003`, `NFR-MAINT-001..003`

## Objective

With separate human authorization, submit a fixed closed two-contact vertical
pan through the complete in-process Route worker and real Windows synthetic
touchpad composition.

## In scope

- one Windows-only exact-name ignored desktop-scroll integration test;
- real Runtime Route activation and Windows Sink construction;
- CancelAll, two contacts down, synchronous vertical move and empty release;
- stable contact IDs 1 and 2 across down/move;
- private packet encode, bounded enqueue/pump and native Sink submission;
- exact four-enqueued/four-processed assertions and full cleanup;
- reusable controlled Windows acceptance fixture for later bounded gestures.

## Out of scope

- pinch/zoom, click, tap, three-finger or four-finger gestures;
- live Android input, sockets or user-provided frame data;
- authenticated production transport or Node task scheduling;
- claiming visible application scroll from test status alone;
- driver installation or persistent device changes.

## Acceptance criteria

1. The test is ignored by ordinary tests and full CI.
2. Both contact IDs remain stable while both y positions translate together.
3. Four packets cross the complete real composed path.
4. An empty snapshot releases both contacts before close.
5. Worker/device and Runtime stop cleanly with no residual state.
6. Default CI and the separately authorized exact test pass.

## Recovery

The final compiled frame is an empty release. Worker close and Runtime stop are
explicit; receiver/session/device Drop and process exit remain fallback cleanup.

## Required validation

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker
cargo xtask ci
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker authorized_worker_submits_two_finger_pan_then_releases_and_closes -- --ignored --exact --nocapture
cargo xtask validate-docs
git diff --check
```

## Completion evidence

- Default worker tests report eight passed and three ignored.
- Full default repository CI passes without device/input execution.
- The authorized two-finger exact test reports one passed and ten filtered.
- Four packets were submitted, both contacts released, then Sink/device and
  Runtime cleanup completed.

Detailed evidence: `docs/CAPY_PTP_002K_REPORT.md`.
