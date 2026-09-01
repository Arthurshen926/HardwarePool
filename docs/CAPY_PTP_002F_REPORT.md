# CAPY-PTP-002F Report

Date: 2026-08-30

Status: authorized Route ingress scheduler complete; focused and full
repository validation pass.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

The private touchpad receiver is now reachable only through a bounded,
caller-driven Route session that validates current Core lifecycle state before
accepting or pumping packet bytes. This closes the gap between the abstract
authorized Route and the packet/receiver/Sink chain without adding a socket,
worker thread, UI command or platform device action.

## Route binding

`PrivateTouchpadRouteSession` retains one exact binding:

- Route ID and Session ID;
- Source and expected local Sink `PortRef` values;
- `capyio.input.touchpad-frames/1` Profile;
- `AdapterManaged` backend;
- authorization expiry; and
- positive Route/input stream epoch.

Construction accepts only Starting or Active Routes with an Authorized,
unexpired grant. A Starting binding rejects data until explicit activation
against the same current Active Route. Enqueue and pump revalidate identity,
state, authorization/expiry and epoch. A Route value remains Runtime-owned
input, not a bearer credential or peer authentication mechanism.

## Bounded scheduler

The ingress queue is logically configured as 1..=64 records and preallocated
once. Each record owns one fixed 152-byte packet buffer, exact length and
trusted-local arrival timestamp. No push can grow the queue after construction.

One pump drains at most that configured capacity in order, then calls the
receiver timeout poll even if no record arrived. A record that remains queued
until the active-idle deadline is rejected before reaching the Sink; this avoids
briefly submitting stale contact motion followed by cancellation.

Queue overflow, oversize, local-clock regression, stale queue residence, Route
stop/offline, authorization expiry, unexpected epoch or receiver failure clears
pending records, attempts receiver/Sink close and poisons the session. Cleanup
failure remains attached to the primary ingress fault.

## Epoch lifecycle

A strictly later Starting Route epoch can advance the existing receiver and
Sink before reactivation. Pending old-epoch records are counted and discarded
before the transition commits. The binding then accepts only packets from the
new epoch. Reusing this path retains the same Stream identity and clock domain;
a changed identity requires a new Route session.

## Hardware-free evidence

Ten tests cover:

- exact binding and queue construction failures;
- Starting-to-Active gating and ordered pump;
- overflow cleanup without partial submission;
- Route Offline, grant expiry and epoch drift;
- oversize and local-clock regression;
- exact empty-pump active timeout and stale queued-record rejection;
- later epoch advancement/discard/reactivation;
- retained cleanup failure;
- selected touchpad contract fixture; and
- Core Route → fixed queue → receiver → Windows projector cancel/active/release
  behavior.

The Windows test uses `WindowsTouchpadProjector`, not
`SyntheticTouchpadSession`; no native device or user32 API is called.

## Files

- `adapters/remote-touchpad/src/ingress.rs`
- `adapters/remote-touchpad/src/lib.rs`
- `adapters/remote-touchpad/Cargo.toml`
- `adapters/remote-touchpad/tests/touchpad_route_session.rs`
- `adapters/remote-touchpad/README.md`
- `docs/REMOTE_TOUCHPAD_PRIVATE_PROTOCOL.md`
- `docs/plans/completed/0030-private-touchpad-route-ingress.md`
- architecture, data-plane, protocol, security, testing, Port Profile, product
  scope, build status, ADR 0044 and traceability documentation.

## Dependency note

No external dependency was added. The remote-touchpad Adapter now directly
depends on the existing local `capyio-core` crate for typed Route/Session/Port
identity, lifecycle, backend and authorization state. Core remains independent
of the Adapter, platform SDK and packet implementation.

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

The Adapter test run contains eight generic touch-to-pointer fallback tests,
six private-packet tests, one Android-to-Windows packet boundary test, eight
receiver tests and ten Route-ingress tests: 33 total. Full CI passed formatting,
workspace check, Clippy, tests, demo, documentation, manifests, Adapter smoke,
repository validation and frontend typecheck/build.

## Remaining work and risks

- No live transport exists. Peer authentication/authorization issuance,
  encryption, cryptographic replay defense and network admission remain
  required before remote exposure.
- No Runtime worker currently obtains Runtime-owned Route snapshots, captures
  local monotonic time and calls enqueue/pump.
- Android runtime/JNI touch capture remains DTO-only.
- Production construction of `SyntheticTouchpadSession` remains absent.
- Visible two/three/four-finger behavior still requires separately approved
  physical acceptance.
- The private packet/queue remains AdapterManaged, not a public interoperability
  promise.
- The worktree remains uncommitted and based on `fc3da36`.
