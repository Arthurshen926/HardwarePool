# CAPY-PTP-002G Report

Date: 2026-08-30

Status: deterministic Runtime worker boundary complete; focused and full
repository validation pass.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

The private touchpad path now has a deterministic composition boundary above
the authorized Route ingress. Packet/tick callers no longer provide a Route
snapshot or timestamps. The worker reads one immutable current Route snapshot
and one coherent local clock sample, then delegates to the existing bounded
session. No thread, timer, socket, OS device or native input submission was
added.

## Runtime boundary

`PrivateTouchpadRouteProvider` returns an owned Core `Route` snapshot for one
Route ID. `PrivateTouchpadMonotonicClock` returns authorization milliseconds and
ingress nanoseconds together. The production Adapter still depends only on Core
and input contracts; the real `NodeRuntime` is connected through a test-only
provider, avoiding a reversed production dependency.

`PrivateTouchpadRuntimeWorker` owns both boundaries and one Route session. It
drives construction, explicit activation, packet enqueue, periodic tick,
strictly later epoch transition and stop. Stop does not consult provider/clock,
so cleanup remains available when state retrieval is broken.

## Fail-closed behavior

The worker retains the last coherent sample and rejects rollback in either
millisecond or nanosecond values. Provider failure, clock failure and rollback
clear queued records, attempt Sink close and mark the session failed. Current
Route state is fetched for every live command, so a Runtime transition to
Stopping is observed at the next tick and follows the existing Route-binding
cleanup path.

Fixed saturating counters expose clock samples, Route reads, activations,
enqueues, ticks, processed/discarded packets, epoch advances and stops without
allocating an event or logging queue.

## Hardware-free evidence

Five tests use real `NodeRuntime` Route construction and lifecycle with a pure
memory Sink:

- Starting to Active, packet enqueue/pump and orderly worker/Runtime stop;
- Stopped to Prepared to Starting epoch 2 recovery with old-queue discard;
- authorization millisecond rollback while nanoseconds advance;
- Route-provider and clock source failure; and
- Runtime Active to Stopping drift detected by the next worker tick.

No test creates a thread, timer, socket, `SyntheticTouchpadSession`, user32
device, driver or desktop input operation.

## Files

- `adapters/remote-touchpad/src/worker.rs`
- `adapters/remote-touchpad/src/ingress.rs`
- `adapters/remote-touchpad/src/lib.rs`
- `adapters/remote-touchpad/Cargo.toml`
- `adapters/remote-touchpad/tests/touchpad_runtime_worker.rs`
- `adapters/remote-touchpad/README.md`
- `docs/plans/completed/0031-private-touchpad-runtime-worker.md`
- architecture, data-plane, protocol, security, testing, product scope, build
  status and traceability documentation.

## Dependency note

No external or production dependency was added. Existing local
`capyio-runtime` and `capyio-testkit` are development-only dependencies for
real Runtime lifecycle tests.

## Validation

The following commands pass:

```text
cargo fmt --all -- --check
cargo check -p capyio-remote-touchpad-adapter --all-targets
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo test -p capyio-remote-touchpad-adapter
cargo xtask validate-docs
cargo xtask ci
git diff --check
```

The Adapter test run contains the prior 33 tests plus five Runtime-worker
tests: 38 total.

## Remaining work and risks

- No authenticated live transport or Node task loop exists yet.
- Android runtime/JNI touch capture remains DTO-only.
- Production construction of the Windows `SyntheticTouchpadSession` remains
  absent.
- Visible two/three/four-finger behavior still requires separately approved
  physical acceptance.
- The private packet/worker remains AdapterManaged, not a public
  interoperability promise.
- The worktree remains uncommitted and based on `fc3da36`.
