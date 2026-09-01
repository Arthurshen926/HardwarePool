# CAPY-PTP-002E Report

Date: 2026-08-30

Status: private receiver lifecycle complete; focused and full repository
validation pass.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

The private touchpad packet boundary now feeds one explicit, fail-closed
receiver lifecycle rather than exposing a bare decoder directly to a future
transport. `PrivateTouchpadReceiver` binds one negotiated Stream/descriptor,
expected sequence, configured rate/idle limits and authorized Sink. The slice
opens no socket, creates no Windows synthetic device and injects no desktop
input.

## Admission and sequence policy

Construction accepts only:

- `1..=1000` packets/s, with a default of 240; and
- an active-contact idle timeout of `10 ms..=30 s`, with a default of 250 ms.

Receive-time checks use a trusted local monotonic nanosecond value supplied by
the caller, not the packet's source timestamp. The receiver rejects clock
regression and the first packet above a fixed one-second admission window.
Each decoded frame then passes the bound per-epoch sequence tracker before the
Sink is called. Duplicate and late frames never reach the Sink. Forward gaps
remain observable and do reach the Sink so its existing semantic tracker can
cancel retained contacts and suppress later updates until `cancel_all`.

## Fail-closed lifecycle

Packet, rate, arrival-clock or sequence failure attempts Sink close and poisons
the receiver. Sink submission failure also attempts close and retains both the
primary and cleanup errors when both fail. An active contact stream closes at
the configured idle deadline; an already released stream stays idle. Explicit
disconnect and abandoned active receiver drop use the same bounded close
boundary.

Epoch changes are explicit and strictly increasing. The codec, receiver
sequence, receive clock and rate window advance/reset only after the Sink
accepts the same transition. A failed transition poisons the receiver rather
than leaving two lifecycle owners on different epochs.

On Windows, `SyntheticTouchpadSession` implements the receiver's Sink trait.
This is a compile-time type adapter only: the implementation does not open a
session. Runtime authorization and session construction remain outside the
receiver.

## Hardware-free evidence

Eight receiver tests prove:

- invalid construction bounds;
- duplicate rejection without duplicate Sink submission;
- observable gaps and cancellation of a retained Windows projector contact;
- rate-limit failure and valid epoch-window reset;
- malformed packet and local receive-clock regression cleanup;
- exact active-idle timeout versus released-stream idle behavior;
- retained primary plus cleanup Sink errors; and
- one close for explicit disconnect and abandoned active drop.

The test Sink is either pure in-memory state or `WindowsTouchpadProjector`.
`SyntheticTouchpadSession::open`, user32, networking and OS injection are never
called.

## Files

- `adapters/remote-touchpad/src/receiver.rs`
- `adapters/remote-touchpad/src/lib.rs`
- `adapters/remote-touchpad/Cargo.toml`
- `adapters/remote-touchpad/tests/touchpad_receiver.rs`
- `adapters/remote-touchpad/README.md`
- `docs/REMOTE_TOUCHPAD_PRIVATE_PROTOCOL.md`
- `docs/plans/completed/0029-private-touchpad-receiver-session.md`
- architecture, data-plane, protocol, security, testing, Port Profile, product
  scope and traceability documentation.

## Dependency note

No external dependency was added. The existing workspace
`capyio-windows-input` crate is now also a Windows-target production dependency
of the remote-touchpad Adapter solely for the
`SyntheticTouchpadSession: PrivateTouchpadSink` implementation. Non-Windows
production builds do not compile that edge; hardware-free cross-platform tests
continue to use the crate as a dev-dependency.

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

The Adapter test run contains eight existing generic touch-to-pointer fallback
tests, six private-packet tests, one Android-to-Windows boundary-loop test and
eight receiver tests: 23 total. Full CI passed formatting, workspace check,
Clippy, tests, demo, documentation, manifests, Adapter smoke, repository
validation and frontend typecheck/build.

## Remaining work and risks

- No live transport exists. Peer authentication/authorization, encryption,
  cryptographic replay defense, transport admission and reconnect ownership
  remain required before network exposure.
- The Runtime must schedule `poll_timeout` from a trusted local monotonic clock;
  the receiver has no worker/thread of its own.
- Android runtime/JNI touch capture remains DTO-only.
- Production Runtime wiring that constructs an authorized Windows
  `SyntheticTouchpadSession` remains absent.
- Visible two/three/four-finger behavior still requires separately approved
  physical acceptance.
- The private packet/receiver remains AdapterManaged and is not a public
  interoperability promise.
- The worktree remains uncommitted and based on `fc3da36`.
