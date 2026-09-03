# CAPY-GAMEPAD-001C Report

Date: 2026-08-29

Status: implementation and automated validation complete

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-gamepad`

Branch: `codex/capyio-gamepad`

## Objective

Provide an Adapter-owned background-worker seam that a later integration owner
can bind to one Runtime IMU Route without importing SensorServer, desktop UI or
platform lifecycle code into the DSU Adapter.

## Boundary

- one worker derives exactly one typed `StreamId` and positive Route epoch from
  a validated `DataEnvelope<ImuSampleV1>` anchor; the anchor is not submitted
  implicitly;
- a new epoch requires a new worker, preventing stale samples from surviving a
  Route restart;
- producers use a bounded `sync_channel` and non-blocking `try_submit`;
- the worker validates the canonical IMU Profile plus stream, epoch and sequence
  before projection;
- wrong stream, stale/future epoch, late input, gaps, invalid payload,
  projection failure and channel pressure have independent counters; a repeat
  of an already delivered sequence is classified as late;
- accepted samples retain the CAPY-GAMEPAD-001A explicit axis mapping and feed
  the CAPY-GAMEPAD-001B loopback endpoint;
- `stop` is idempotent, wakes within the configured polling bound and joins the
  thread; a transport failure is returned as a typed error when joined, and
  `Drop` requests the same cleanup;
- the worker never owns CapyIO Route authorization/state, SensorServer network
  capture, Android lifecycle, UI state or emulator configuration.

The stream identity comes from the validated typed envelope, so this slice adds
no production dependency and does not change a root manifest or lockfile.
Worker statistics are monotonic but eventually consistent while the worker is
running because producers and the worker update independent atomics. A snapshot
after `stop` joins is stable.

## Files

- `adapters/dsu/src/worker.rs`
- `adapters/dsu/src/lib.rs`
- `adapters/dsu/tests/worker.rs`
- `adapters/dsu/README.md`
- `docs/CAPY_GAMEPAD_001C_REPORT.md`

## Automated evidence

`cargo test --locked -p capyio-dsu-adapter` passed 25 tests:

- all six deterministic IMU fixture records crossed the bounded worker channel
  and arrived as sequential DSU UDP pad packets;
- stop joined the worker, rejected later submissions and released the UDP port;
- wrong stream, stale/future epoch, a sequence gap, late data and an invalid
  Profile were rejected or counted without worker failure;
- an invalid/zero-epoch anchor, invalid queue capacity and invalid poll interval
  failed before the worker thread started;
- a full bounded submission channel returned `QueueFull` without blocking and
  updated its pressure counter;
- the earlier codec, projection and caller-polled transport suites remained
  green.

Additional passing commands:

- `cargo check --locked -p capyio-dsu-adapter --all-targets`
- `cargo clippy --locked -p capyio-dsu-adapter --all-targets -- -D warnings`
- `cargo xtask validate-docs`
- `cargo xtask validate-manifests`
- `cargo xtask ci`
- `git diff --check`

The full CI run passed on its first attempt, including workspace checks/tests,
Adapter smoke/structural validation and desktop typecheck/build.

## Remaining risks

- The desktop/Node Runtime does not yet create or own this worker.
- No live SensorServer sample or physical phone enters the worker.
- No emulator has discovered the endpoint.
- Host configuration must still choose a stable-per-run server ID, explicit
  port, axis mapping and Route-to-worker lifecycle policy.
- Loopback DSU remains unauthenticated local interoperability, not production
  CapyIO network transport.
- Buttons, sticks, triggers, touch, calibration controls and haptics remain
  later work.

No commit or push was performed.
