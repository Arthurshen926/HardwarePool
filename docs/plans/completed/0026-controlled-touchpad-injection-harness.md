# CAPY-PTP-002B — Controlled Windows touchpad injection harness

Status: complete

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `FR-PLAT-004`, `NFR-STAB-001..004`,
`NFR-SEC-003`, `NFR-MAINT-001..003`

## Objective

Add a bounded, default-dry-run Windows harness that can create one generic
synthetic Precision Touchpad and submit prevalidated fixed gesture fixtures only
after an explicit injection mode and a separate desktop-impact acknowledgement.

## In scope

- RAII ownership of the dynamically loaded user32 module and synthetic device;
- dynamic `CreateSyntheticPointerDevice2`, `InjectSyntheticPointerInput` and
  `DestroySyntheticPointerDevice` resolution;
- typed create/inject errors and empty-batch behavior;
- fixed one-finger motion, two-finger pan, three-finger swipe and four-finger
  swipe fixtures with bounded steps and timing;
- a dry-run summary that executes all contract/projection/native encoding but
  performs no Windows call;
- a separate CLI binary requiring `--inject` and
  `--acknowledge-desktop-input` together;
- hardware-free unit/CLI tests and documentation.

## Out of scope

- running desktop injection without separate explicit approval;
- tap/click, integrated physical-button or direct `InjectTouchpadAction` use;
- arbitrary coordinates, scripts, repeat counts or unbounded fixture input;
- Android runtime, network transport or end-to-end phone input;
- automated proof that Windows recognized a cursor/scroll/system gesture;
- VHF/HID driver work.

## Acceptance criteria

1. Dry-run is the default and cannot create a device or inject input.
2. Injection mode fails before device creation unless the acknowledgement flag
   is also present.
3. Every fixture starts with cancel-all, uses 1..=4 stable contacts, has fixed
   finite timing, ends released/cancelled and remains within five-contact/two-
   batch projection limits.
4. The device is destroyed exactly once on success or any submission failure.
5. Invalid parameters and missing APIs return typed errors rather than panic.
6. Tests and full repository CI pass without submitting input.

## Required validation

```text
cargo fmt --all -- --check
cargo check -p capyio-windows-input --all-targets
cargo clippy -p capyio-windows-input --all-targets -- -D warnings
cargo test -p capyio-windows-input
cargo run -p capyio-windows-input --bin capyio-ptp-inject -- --gesture one-finger-motion --dry-run
cargo run -p capyio-windows-input --bin capyio-ptp-inject -- --gesture two-finger-pan --dry-run
cargo run -p capyio-windows-input --bin capyio-ptp-inject -- --gesture three-finger-swipe --dry-run
cargo run -p capyio-windows-input --bin capyio-ptp-inject -- --gesture four-finger-swipe --dry-run
cargo xtask validate-docs
cargo xtask ci
```

## Dependency changes

None. The harness uses existing `capyio-input` and pinned `windows-sys` modules.

## Safety

- The ordinary test/CI path is dry-run only.
- No gesture accepts user-provided positions, frame counts or repeats.
- Contact injection remains unavailable unless both explicit CLI gates are
  present.
- The binary prints whether input was actually submitted.
- Real execution is a separate controlled-lab action because even motion-only
  touchpad input can move the pointer, scroll content or switch UI state.

## Implementation plan

1. Add fixed fixtures and deterministic dry-run metrics.
2. Add the RAII dynamic user32 device and batch submission method.
3. Add the double-gated CLI and parser tests.
4. Run all no-injection checks and retain a completion report.

## Completion record

Implemented:

- fixed one/two/three/four-finger fixtures with exact dry-run metrics;
- System32 module plus synthetic-device RAII ownership;
- typed creation/submission errors and empty-batch handling;
- double-gated CLI with no arbitrary gesture inputs;
- bounded emergency cancellation after submission failure;
- security, architecture, testing and platform documentation.

Validation:

- format/check/Clippy/tests: pass;
- four fixed dry-runs: pass, with no device creation/input injection;
- missing acknowledgement rejection: pass before platform work;
- `cargo xtask validate-docs`: pass;
- full `cargo xtask ci`: pass.

Not validated:

- any actual Windows batch submission or recognized gesture;
- Android/network end-to-end behavior;
- integrated buttons, tap/click and VHF behavior.

Detailed evidence: `docs/CAPY_PTP_002B_REPORT.md`.
