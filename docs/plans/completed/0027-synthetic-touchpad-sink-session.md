# CAPY-PTP-002C — Synthetic touchpad Sink session

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-PLAT-004`, `NFR-STAB-001..004`,
`NFR-SEC-003`, `NFR-MAINT-001..003`

## Objective

Turn the controlled Windows injection proof into a reusable, bounded Sink
session that owns projection, submission and fail-safe contact cleanup without
coupling a future Runtime or transport to the lab CLI.

## In scope

- record the separately approved one-finger host acceptance result;
- add a reusable `SyntheticTouchpadSession` around one projector and one
  synthetic device;
- expose frame submission, epoch advancement and explicit close outcomes;
- attempt bounded contact cancellation before device destruction on close,
  submission failure and abandoned-session drop;
- make a failed/closed session reject further frames;
- refactor the fixed CLI to use the same session lifecycle;
- unit-test lifecycle and failure cleanup with a non-native fake device.

## Out of scope

- additional desktop injection, including two-, three- or four-finger tests;
- Android runtime, transport, pairing or Node Runtime integration;
- tap/click, integrated buttons or `InjectTouchpadAction`;
- driver/VHF development or deployment;
- claiming visible cursor movement from API success alone.

## Acceptance criteria

1. The approved host run retains its exact command/result and distinguishes the
   sandbox-denied attempt from the interactive-host success.
2. A production caller can open one session, submit validated touchpad frames,
   advance epochs and close it without using the CLI.
3. Close, submission failure and drop each have a bounded cancellation path;
   destruction remains the final native cleanup.
4. Projection errors do not mutate the projector and native submission errors
   poison the session so it cannot continue with uncertain state.
5. Ordinary tests never create a native device or inject desktop input.
6. Focused checks and full repository CI pass.

## Required validation

```text
cargo fmt --all -- --check
cargo check -p capyio-windows-input --all-targets
cargo clippy -p capyio-windows-input --all-targets -- -D warnings
cargo test -p capyio-windows-input
cargo run -p capyio-windows-input --bin capyio-ptp-inject -- --gesture one-finger-motion --dry-run
cargo xtask validate-docs
cargo xtask ci
git diff --check
```

## Dependency changes

None. This slice uses the existing platform crate and `capyio-input` contract.

## Safety

- The separately approved one-finger command is the only native submission in
  this slice.
- Unit/integration tests use a fake batch device and cannot call user32.
- No arbitrary coordinates, timing or repeat count are accepted by the CLI.
- The session is an input lifecycle primitive, not an authorization boundary.

## Implementation plan

1. Retain the controlled host evidence and identify the sandbox boundary.
2. Add a fake-testable Sink session core and native public wrapper.
3. Refactor the fixed harness onto the session.
4. Update architecture, security, testing and traceability evidence.
5. Run focused validation and full CI, then archive this plan.

## Completion record

Implemented:

- retained the separately approved one-finger host API acceptance evidence;
- added a reusable fake-testable `SyntheticTouchpadSession` lifecycle;
- added explicit frame, epoch and close submission outcomes;
- added failed/closed session rejection and bounded failure/drop cleanup;
- refactored the fixed CLI to use the reusable session;
- updated architecture, security, testing and traceability documentation.

Validation:

- format/check/Clippy and all Windows input tests: pass;
- fixed one-finger dry-run: pass with no device creation or input injection;
- documentation validation: pass with 84 traced requirements;
- full `cargo xtask ci`: pass after one unrelated Audio Share TCP-bind test
  transiently returned Win32 10013, passed alone, then passed in the full rerun;
- `git diff --check`: pending final delivery audit.

Detailed evidence: `docs/CAPY_PTP_002C_REPORT.md`.
