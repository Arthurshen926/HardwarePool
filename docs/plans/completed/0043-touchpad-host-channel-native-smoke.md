# CAPY-PTP-002S — Host-channel native one-finger smoke

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-ROUTE-006..007`, `NFR-STAB-001..004`,
`NFR-SEC-001..003`, `NFR-PERF-001..003`, `NFR-MAINT-001..003`

## Objective

Prove that the concrete 002R host channel can carry a fixed one-finger lifecycle
through both Runtime workers into a real Windows synthetic Precision Touchpad.

## In scope

- one exact-name Windows-only test ignored by default;
- real `NodeRuntime` Route and validate-before-open Windows factory;
- CancelAll, down, horizontal move and release through the four-packet channel;
- per-packet Runtime Route/clock checks on sender and receiver;
- exact packet/channel/worker counters and bounded cleanup.

## Out of scope

- arbitrary gesture input, long-running device ownership or UI control;
- sockets, Android device connection or authenticated remote transport;
- driver installation, signing, boot configuration or security-policy changes;
- automated proof of visually observed pointer motion.

## Acceptance criteria

1. Default tests compile but do not execute the native acceptance.
2. The exact approved invocation creates the synthetic touchpad successfully.
3. All four packets cross sender, channel and receiver into native submission.
4. Contacts release before Sink close and Runtime stop.
5. Targeted tests, Clippy, full CI and repository validation pass.

## Required validation

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker authorized_bounded_host_channel_submits_one_finger_motion_to_real_windows_touchpad -- --ignored --exact --nocapture
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo xtask ci
cargo xtask validate-docs
git diff --check
```

## Completion evidence

- The exact native test passed: one test passed, 18 filtered out.
- Four fixed packets were enqueued, received and processed before cleanup.
- Default Runtime-worker tests retain 13 pass and six ignored native tests.
- Full repository CI and documentation validation pass.
- No driver, Android device, socket or system-policy operation was performed.

Detailed evidence: `docs/CAPY_PTP_002S_REPORT.md`.
