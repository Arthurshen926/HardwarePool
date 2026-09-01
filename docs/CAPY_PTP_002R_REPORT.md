# CAPY-PTP-002R Report

Date: 2026-08-30

Status: bounded host-channel composition complete; hardware-free validation
passes.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

`private_touchpad_host_channel` now provides concrete admission, sender and
receiver handles around one preallocated bounded queue. This replaces fake-only
channel composition for local Runtime integration while retaining the 002P/002Q
binding, retry and fail-closed semantics.

The end-to-end hardware-free test uses one real `NodeRuntime` Route. The Runtime
sender delivers initial cancellation, one active contact and release through the
four-packet host channel. The Runtime receiver validates and pumps each packet
into `WindowsTouchpadProjector`, then performs bounded close cancellation.

## Bounded lifecycle

- construction accepts only capacities 1..=64;
- a full queue or known closed endpoint rejects before write;
- accepted packets preserve FIFO order and drain after normal sender close;
- admission denial, complete-binding replacement and receiver close discard all
  queued packets;
- sender/receiver close is idempotent and metrics use saturating fixed fields;
- all channel state is single-threaded and caller-driven.

## Truthful boundary

This is a concrete in-process host handoff, not a cross-device transport. It
opens no socket, defines no TCP/UDP framing, authenticates no peer, encrypts no
packet and provides no cryptographic replay protection. ADR 0044 therefore
continues to classify the packet as private `AdapterManaged` framing; a public
or remote authenticated binding still requires a separate transport ADR.

## Validation

```text
cargo fmt --all -- --check
cargo test -p capyio-remote-touchpad-adapter --test touchpad_host_channel
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo xtask ci
cargo xtask validate-docs
git diff --check
```

Results: two channel tests passed; 13 ordinary Runtime-worker tests passed and
five separately authorized native Windows tests remained ignored. Clippy passed
with warnings denied, and full repository CI/document validation passed.

## Files

- `adapters/remote-touchpad/src/bounded_channel.rs`
- `adapters/remote-touchpad/src/lib.rs`
- `adapters/remote-touchpad/tests/touchpad_host_channel.rs`
- `adapters/remote-touchpad/tests/touchpad_runtime_worker.rs`
- `adapters/remote-touchpad/README.md`
- `docs/plans/completed/0042-touchpad-bounded-host-channel.md`
- architecture, private protocol, security, testing, product scope, build status
  and traceability documentation.

## Remaining work

- A separately reviewed authenticated cross-device transport and concrete Node
  task scheduler remain absent.
- Android OS/JNI and APK work still require a buildable Android toolchain.
- No network, Android device or Windows input operation was performed.
- The worktree remains uncommitted and based on `fc3da36`.
