# CAPY-PTP-002Q Report

Date: 2026-08-30

Status: Runtime-driven sender Route/clock admission complete; hardware-free
validation passes.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

`PrivateTouchpadRuntimeDeliveryWorker` now composes the 002P delivery session
with the existing read-only Runtime Route provider and coherent local clock.
The Android/packet-facing caller supplies only semantic frames. Construction,
each frame and normal close sample the clock, fetch a fresh Route snapshot and
derive the complete Active Source binding inside the Adapter.

That derived Route, Session, Source, Sink, epoch and authorization-expiry tuple
must remain identical to the initially admitted binding. The inner channel then
independently reasserts its current admission before any write, preserving the
transactional retry and delivery-ambiguity rules established in 002P.

## Fail-closed behavior

- construction requires an Active, authorized, unexpired `AdapterManaged`
  touchpad Route with the exact expected Source and stream epoch;
- provider/clock failure and rollback in either milliseconds or nanoseconds
  fault and close the channel;
- authorization expiry, Route stop and any complete-binding drift fault before
  another packet is delivered;
- errors remain typed and metrics are fixed saturating `u64` counters;
- the worker creates no thread, timer, socket, Android component or platform
  device.

## Truthful security boundary

The integration tests adapt a real local `NodeRuntime`, but the admitted
channel remains an in-memory fake. Runtime-owned snapshots narrow stale-state
and confused-deputy risk inside the trusted host. They do not authenticate a
peer, encrypt packets, manage keys, establish a live network path or provide
cryptographic replay protection.

## Validation

```text
cargo fmt --all -- --check
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker
cargo test -p capyio-remote-touchpad-adapter
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo xtask ci
cargo xtask validate-docs
git diff --check
```

Results: 12 ordinary Runtime-worker tests passed and five explicitly authorized
Windows native-input tests remained ignored; all Adapter tests and Clippy passed
with warnings denied. Full repository CI and documentation validation passed.

## Files

- `adapters/remote-touchpad/src/delivery_worker.rs`
- `adapters/remote-touchpad/src/delivery.rs`
- `adapters/remote-touchpad/src/ingress.rs`
- `adapters/remote-touchpad/src/lib.rs`
- `adapters/remote-touchpad/tests/touchpad_runtime_worker.rs`
- `adapters/remote-touchpad/README.md`
- `docs/plans/completed/0041-touchpad-runtime-delivery-worker.md`
- architecture, protocol, security, testing, product scope, build status and
  traceability documentation.

## Remaining work

- Pairing identity, authenticated encryption, key/version binding, live
  transport and cryptographic replay protection remain absent.
- A concrete bounded channel implementation and Node composition scheduling
  task remain absent.
- Android OS/JNI and APK work still require a buildable Android toolchain.
- No network, Android device or Windows input operation was performed.
- The worktree remains uncommitted and based on `fc3da36`.
