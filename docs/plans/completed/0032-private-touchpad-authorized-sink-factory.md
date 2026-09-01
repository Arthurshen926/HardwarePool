# CAPY-PTP-002H — Authorized Sink factory boundary

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-ROUTE-006..007`,
`NFR-STAB-001..004`, `NFR-SEC-001..003`, `NFR-MAINT-001..003`

## Objective

Connect the private touchpad worker type path to the production Windows
synthetic touchpad Sink while ensuring invalid Route or contract input cannot
trigger platform device creation.

## In scope

- side-effect-free Route/session/receiver construction preflight;
- a generic `PrivateTouchpadSinkFactory` opened only after preflight;
- a Windows production factory for `SyntheticTouchpadSession`;
- distinct preflight, factory and post-open session build errors;
- tests counting memory-Sink opens and a no-open Windows type assertion;
- documentation and full repository validation.

## Out of scope

- calling the Windows factory in tests or creating a synthetic device;
- native frame submission or physical gesture acceptance;
- transport, pairing, encryption, reconnect or Android runtime capture;
- background task scheduling, UI commands or automatic device startup;
- driver/VHF implementation or installation.

## Acceptance criteria

1. Route identity/state/authorization/epoch, expected Sink, Stream descriptor,
   first sequence and all queue/receiver limits validate before factory open.
2. Invalid Route or semantic contract opens zero Sinks.
3. A valid preflight opens exactly one Sink and constructs the worker.
4. Factory failure is distinct from validation/session construction failure.
5. The Windows factory statically produces `SyntheticTouchpadSession` but no
   test invokes its `open` method.
6. Full CI passes without device, desktop-input or network operations.

## Dependency changes

None. The existing Windows target dependency already supplies
`SyntheticTouchpadSession`; no external or production dependency is added.

## Required validation

```text
cargo fmt --all -- --check
cargo check -p capyio-remote-touchpad-adapter --all-targets
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo test -p capyio-remote-touchpad-adapter
cargo xtask validate-docs
cargo xtask ci
git diff --check
```

## Completion evidence

- Receiver and Route-session preflight reuse the same validation rules as final
  construction without owning a platform Sink.
- `new_with_sink_factory` invokes a factory only after successful preflight.
- The Windows factory is zero-sized and its explicit `open` is the only device
  creation point.
- Three new tests cover zero-open rejection, one-open success, typed factory
  failure and compile-time Windows integration without a device operation.
- Focused checks and full repository CI pass.

Detailed evidence: `docs/CAPY_PTP_002H_REPORT.md`.
