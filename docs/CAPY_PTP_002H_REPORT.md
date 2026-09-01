# CAPY-PTP-002H Report

Date: 2026-08-30

Status: authorized Sink factory boundary complete; focused and full repository
validation pass.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

The Runtime worker now has a production type path to Windows
`SyntheticTouchpadSession`, but platform construction is gated behind a complete
side-effect-free preflight. Invalid Route, authorization, epoch, Stream,
descriptor or queue/receiver limits cannot call the Sink factory.

No Windows device was created and no native input was submitted in this slice.

## Validate-before-open construction

Receiver binding validation is reusable without owning a Sink. Route-session
preflight checks:

- exact Route, Session, Source and expected Sink identities;
- `AdapterManaged` touchpad Profile, Starting/Active state and authorization;
- Route/input epoch equality;
- Stream/descriptor and first-sequence validity;
- queue capacity, packet-rate limit and active-idle deadline.

`PrivateTouchpadRuntimeWorker::new_with_sink_factory` samples the local clock,
reads one immutable current Route, runs this preflight and only then calls the
factory. The final session constructor repeats the same checks defensively.
Preflight, factory and final session failures remain distinct typed errors.

## Windows production bridge

`WindowsSyntheticTouchpadSinkFactory` is a zero-sized implementation whose Sink
is the existing `SyntheticTouchpadSession`. Constructing the factory performs no
platform action. Its explicit `open` delegates to the existing RAII Windows
session, which owns projection, cancellation and device destruction.

This is a composition path, not automatic startup or authorization. An eventual
Node task must deliberately choose the factory after authenticated Route setup.

## Hardware-free evidence

Three new tests prove:

- a Stopped Runtime Route opens zero memory Sinks;
- an invalid two-contact descriptor opens zero memory Sinks;
- valid preflight opens exactly one Sink and worker stop closes it;
- a factory failure has its own error variant; and
- the Windows factory satisfies the production trait without invoking `open`.

The worker test module now has eight tests and the complete Adapter has 41
tests. All remain free of device creation, user32 calls, desktop input, sockets
and background threads.

## Files

- `adapters/remote-touchpad/src/receiver.rs`
- `adapters/remote-touchpad/src/ingress.rs`
- `adapters/remote-touchpad/src/worker.rs`
- `adapters/remote-touchpad/src/lib.rs`
- `adapters/remote-touchpad/tests/touchpad_runtime_worker.rs`
- `adapters/remote-touchpad/README.md`
- `docs/plans/completed/0032-private-touchpad-authorized-sink-factory.md`
- architecture, private protocol, security, testing, product scope, build
  status and traceability documentation.

## Dependency note

No dependency changed. The existing target-specific local
`capyio-windows-input` dependency supplies `SyntheticTouchpadSession`.

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

The first full-CI attempt encountered Windows error 10013 while two unrelated
audio-share loopback TCP tests tried to bind. An immediate isolated rerun of all
four audio transport tests passed, followed by a complete `cargo xtask ci`
pass without code changes; this is retained as transient host evidence rather
than attributed to the touchpad slice.

## Remaining work and risks

- No authenticated live transport or Node task loop exists yet.
- Android runtime/JNI touch capture remains DTO-only.
- The Windows factory is not yet invoked by production composition.
- Actual factory invocation creates a synthetic input device and requires an
  explicitly authorized controlled test or production Route command.
- Visible two/three/four-finger behavior still requires separately approved
  physical acceptance.
- The worktree remains uncommitted and based on `fc3da36`.
