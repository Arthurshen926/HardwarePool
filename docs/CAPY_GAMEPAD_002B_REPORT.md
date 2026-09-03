# CAPY-GAMEPAD-002B Report

Date: 2026-08-29

Status: implementation and Adapter-level automated validation complete

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-gamepad`

Branch: `codex/capyio-gamepad`

## Objective

Connect the independent `capyio.input.gamepad-state/1` stream to the bounded
DSU IMU worker so one DSU v1001 pad-data frame contains the most recently
accepted controls and motion samples. Preserve the existing IMU-only entry
point and keep Runtime, Android UI and physical-device lifecycle out of the
Adapter.

## Boundary and state machine

- `DsuImuWorker::start` remains the compatible IMU-only mode;
- `DsuImuWorker::start_with_controls` validates separate fixed-epoch IMU and
  controls anchors before binding or spawning and exposes independent
  non-blocking senders;
- controls use `InputSequenceTracker`; IMU retains its independent bounded
  data-plane queue, stream and epoch guard;
- controls received before the first valid IMU sample are cached without
  inventing motion or sending a packet;
- every later valid update on either input emits the latest accepted pair;
- a controls gap returns the projected controller to neutral before accepting
  and emitting the later complete snapshot. An IMU gap is counted but does not
  invalidate an otherwise healthy controls stream;
- invalid controls, DSU-unrepresentable paddle buttons, a future controls
  epoch and controls sequence exhaustion fail safe to neutral. Foreign-stream,
  stale-epoch and duplicate/late frames are rejected without allowing those
  frames to release a valid current stream;
- clean stop sends the last known motion plus neutral controls once before the
  loopback socket closes. Failure cleanup makes the same best-effort attempt
  while retaining the original transport error;
- `request_neutral` lets the future Runtime owner deliver Route
  `Offline`/`Failed`/`Stopped` and upstream peer-loss lifecycle signals without
  blocking or coupling the Adapter to Runtime state.

Controls overflow uses a monotonically increasing generation rather than a
single reset flag. Each queued controls snapshot carries its submission
generation. Overflow or an explicit neutral request advances the generation;
the worker first neutralizes the current state and discards every older queued
controls snapshot. This prevents an overflow-before-release race from
reactivating an old held button after neutral. Generation exhaustion stops the
worker rather than wrapping silently. IMU messages are not discarded by this
controls-specific recovery.

`DsuImuWorkerStats` now separates controls acceptance, rejection, gap,
generation and neutral-transition evidence. The old `motion_packets_*` fields
remain for compatibility and count all emitted pad-data packets in dual-input
mode; the new `dsu_pad_packets_*` fields are the unambiguous counters.

## Files

- `adapters/dsu/src/worker.rs`
- `adapters/dsu/src/lib.rs`
- `adapters/dsu/tests/dual_worker.rs`
- `adapters/dsu/README.md`
- `docs/CAPY_GAMEPAD_002B_REPORT.md`

## Automated evidence

`cargo test -p capyio-dsu-adapter` passes 43 tests. New coverage includes:

- controls cached until the first motion sample, then full 100-byte packet
  equality against the deterministic encoder;
- controls-triggered packets reusing the last motion and motion-triggered
  packets reusing the last controls;
- strict network ordering of neutral before a recovered controls snapshot
  after a sequence gap;
- independent stream, epoch, sequence and gap classification for motion and
  controls;
- invalid and unsupported controls neutralizing without terminating the worker,
  followed by recovery at the still-expected sequence;
- controls sequence exhaustion returning to neutral;
- explicit lifecycle neutral and exactly one stop-time neutral packet;
- invalid and unsupported controls anchors failing before worker start;
- deterministic generation evidence that an overflow-old queued controls
  snapshot cannot reactivate after neutral.

The following targeted checks also pass:

- `cargo clippy --locked -p capyio-dsu-adapter --all-targets -- -D warnings`
- `git diff --check`

`cargo xtask ci` also passes, including workspace formatting, check, strict
clippy, tests, deterministic IMU demo, documentation/manifests/repository
validation, Adapter smoke/crash isolation and desktop typecheck/build.

## Remaining risks

- No Runtime Route currently starts this worker or calls `request_neutral`; no
  Android touch-control UI or live phone input is included.
- At the 002B boundary, motion and controls shared one bounded worker channel,
  so high IMU load could create controls head-of-line pressure. This historical
  risk is removed by CAPY-GAMEPAD-002C's independent bounded ingress queues.
- A broken UDP transport may make the best-effort failure neutral packet
  impossible to deliver. Remote release cannot be guaranteed after socket or
  process failure.
- The Adapter has no upstream inactivity timer. Runtime lifecycle/peer-loss
  detection must explicitly request neutral; DSU subscriber TTL is not used as
  a substitute for upstream Route health.
- Cemu/Dolphin face-layout behavior remains untested on a real emulator, and
  loopback DSU remains unauthenticated local interoperability.

No commit or push was performed.
