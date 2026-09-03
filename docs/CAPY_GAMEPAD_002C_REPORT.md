# CAPY-GAMEPAD-002C Report

Date: 2026-08-29

Status: implementation and Adapter-level automated validation complete

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-gamepad`

Branch: `codex/capyio-gamepad`

## Objective

Remove head-of-line capacity coupling between the high-rate IMU stream and the
complete touch-controls/gamepad-state stream introduced by the initial dual-
input worker. Preserve all CAPY-GAMEPAD-002B sequence, generation and neutral
safety behavior without adding a production dependency or allocating one heap
object per submitted IMU sample.

## Boundary and scheduling

- the compatible `queue_capacity` field now controls only the motion channel;
- `controls_queue_capacity` independently bounds the controls channel and uses
  the same explicit `1..=1024` validation boundary in dual-input mode; the
  IMU-only compatibility entry point ignores this unused setting;
- each producer still performs only a non-blocking `try_send`; a full motion
  queue cannot consume controls slots and a full controls queue cannot consume
  motion slots;
- controls overflow still advances the fail-safe generation, so controls queued
  before a dropped state cannot reactivate after neutral;
- the worker uses no selection/channel dependency. Every scheduling cycle
  processes at most 16 controls and 16 motion inputs, paired fairly, then
  returns to lifecycle checks and DSU request polling;
- stop is rechecked before every pair and between its controls and motion
  halves, so a saturated cycle cannot defer shutdown for all 32 inputs;
- an idle worker sleeps for the validated poll interval. The default remains
  2 ms and the maximum remains 100 ms;
- controls are checked before motion within each pair, including a generation
  synchronization before both projections. A pending controls reset therefore
  cannot be hidden behind an already queued IMU backlog;
- IMU-only startup creates no unused controls channel and retains the previous
  public sender behavior.

The fixed per-cycle budget bounds work under sustained producer load while
still allowing backlog draining without a mandatory sleep after productive
cycles. The worker returns to the outer loop between cycles, where stop,
lifecycle neutral and DSU subscriber traffic remain observable.

## Files

- `adapters/dsu/src/worker.rs`
- `adapters/dsu/src/lib.rs`
- `adapters/dsu/tests/worker.rs`
- `adapters/dsu/tests/dual_worker.rs`
- `adapters/dsu/README.md`
- `docs/CAPY_GAMEPAD_002B_REPORT.md` (historical risk marked superseded)
- `docs/CAPY_GAMEPAD_002C_REPORT.md`

## Automated evidence

`cargo test -p capyio-dsu-adapter` passes 46 tests. New deterministic unit
evidence covers:

- a full motion channel does not prevent controls submission;
- a full controls channel does not prevent motion submission;
- pressure counters remain attributed to the correct stream;
- one cycle accepts exactly 16 entries from each simultaneously backlogged
  stream, leaving one of each for the next cycle;
- controls generation exhaustion stops without wrapping or accepting another
  lifecycle request;
- the controls-overflow generation regression still discards an older queued
  controls snapshot rather than reactivating it, while a later current-
  generation complete snapshot can recover through the normal gap path;
- foreign-stream controls are classified from a copied sequence tracker before
  payload projection validation, so a foreign frame with an invalid payload
  cannot release a healthy current stream and an invalid current frame still
  does not consume its sequence.

All prior dual-input UDP tests continue to pass, including cached controls,
either-stream updates, gap-before-recovery neutral ordering, lifecycle neutral,
stop cleanup, independent stream guards, invalid controls and sequence
exhaustion.

Targeted validation also passes:

- `cargo clippy --locked -p capyio-dsu-adapter --all-targets -- -D warnings`

`cargo xtask ci` also passes, including workspace formatting, check, strict
clippy, tests, deterministic IMU demo, documentation/manifests/repository
validation, Adapter smoke/crash isolation and desktop typecheck/build.

## Remaining risks

- The standard library channels provide no blocking two-receiver select. Idle
  input detection is caller-configurable polling, so an input can wait up to
  `poll_interval` before processing.
- Per-cycle work is bounded, but each accepted update may fan out to every DSU
  subscriber. Existing subscriber and datagram budgets remain the outer bound.
- Stop completion is bounded by the configured idle poll interval plus current
  DSU poll/projection/send work; it is not a strict `poll_interval` deadline.
- Generation changes are non-blocking requests, not worker acknowledgements.
  A concurrent projection can emit one old-state packet before the next
  synchronization; neutral is then applied within a bounded cycle and older
  generations cannot reactivate afterward. A lifecycle owner must stop the
  upstream producer before requesting neutral when entering an offline state.
- Separate queue capacity prevents cross-stream rejection but does not provide
  clock synchronization between independent motion and controls sources; the
  DSU frame intentionally contains the latest accepted value from each.
- Runtime Route ownership, Android touch-control production and upstream
  lifecycle detection remain outside this Adapter slice.
- Real emulator/physical-device interoperability remains untested.

No commit or push was performed.
