# CAPY-GAMEPAD-003A Report

Date: 2026-08-29

Status: implementation and automated validation complete

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-gamepad`

Branch: `codex/capyio-gamepad`

## Objective

Add the first platform-neutral source-side boundary for touch controls: compose
already-normalized semantic changes into complete `GamepadState` snapshots,
then prove that those snapshots join the independent IMU stream and cross the
existing bounded DSU loopback path. This slice deliberately excludes Android
touch geometry, raw pointer events, multi-touch arbitration and Runtime Route
ownership.

## Composer contract

- `GamepadControlUpdate` covers semantic button press/release, D-pad, left or
  right stick, left or right trigger and full reset operations. It is a local
  source helper, not a serialized Profile or wire contract.
- A composer starts neutral and is fixed to one stream ID and one positive
  stream epoch. A new Route epoch requires a new composer, preventing retained
  state from crossing an epoch boundary.
- Every successful update emits a complete `GamepadState` and consumes exactly
  one sequence, including a repeated value or an already-neutral reset. This
  preserves a downstream recovery opportunity if an earlier complete snapshot
  was dropped by a bounded queue.
- Updates use copy-validate-commit semantics. An invalid candidate leaves the
  retained controls and next sequence unchanged.
- Sequence `u64::MAX` may be emitted as the final state. The composer then
  becomes exhausted; later anchors and updates fail without wrapping or
  mutating controls.
- `anchor(timestamp)` validates and describes the next complete snapshot
  without emitting it or consuming its sequence. It is intended only to bind a
  fixed-epoch consumer such as the dual-input worker before the first update.
- Timestamps remain caller-owned. The composer records them but does not impose
  a clock source or enforce monotonicity.
- The valid hot path uses only fixed-size `Copy` values and performs no heap
  allocation. Existing validation errors contain diagnostic `String` values,
  so error paths are not claimed to be allocation-free.
- `Reset` is a source snapshot operation. It does not replace the worker's
  generation-based overflow and lifecycle neutral fail-safe behavior.

## End-to-end evidence

The DSU dual-worker test constructs a composer anchor, submits button, stick and
trigger updates before the first IMU sample, and verifies that controls are
cached without fabricating motion. After motion arrives, the exact 100-byte UDP
packet is compared against an independently constructed complete controls
state. Further packet comparisons prove that button release retains the stick
and trigger, reset becomes neutral, and stopping an already-neutral worker does
not emit a redundant state.

Contract tests separately cover complete-state accumulation, release and reset,
repeated reset sequencing, invalid-update transactionality, anchor behavior,
zero-epoch rejection, the terminal maximum sequence and exhaustion.

## Files

- `crates/capyio-input/src/gamepad.rs`
- `crates/capyio-input/src/gamepad/composer.rs`
- `crates/capyio-input/src/lib.rs`
- `crates/capyio-input/tests/gamepad_composer.rs`
- `adapters/dsu/tests/dual_worker.rs`
- `adapters/dsu/README.md`
- `docs/CAPY_GAMEPAD_003A_REPORT.md`

## Automated evidence

- `cargo test --locked -p capyio-input` passes 11 tests, including 4 composer
  contract tests.
- `cargo test --locked -p capyio-dsu-adapter` passes 47 tests, including the new
  composer-to-dual-worker-to-UDP loopback test.
- `cargo clippy --locked -p capyio-input -p capyio-dsu-adapter --all-targets --
  -D warnings` passes.
- `cargo xtask ci` passes, including workspace formatting, check, strict
  Clippy, tests, deterministic demo, documentation/manifests/repository
  validation, Adapter smoke/crash isolation and desktop typecheck/build.

## Remaining risks

- No Android producer, UI layout, hit testing, coordinate transform or
  multi-touch/pointer-ownership policy is implemented in this slice.
- The caller remains responsible for timestamps, source lifecycle and creating
  a new composer when the stream epoch changes.
- The composer owns no queue, thread, backpressure policy or Runtime Route
  state; those remain explicit adjacent boundaries.
- Paddle controls remain intentionally unrepresentable in the pinned DSU
  projection and continue to fail closed there.
- Real device and emulator interoperability remain untested.

No commit or push was performed.
