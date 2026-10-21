# Linux owned touchpad

This library adds a real Type-B event projection/backend implementation, not a
relative-cursor approximation. It is opt-in and does not arrange OS access.
It reuses existing pinned touchpad TLS and complete five-contact snapshots.

## Accepted backend software scope

Commit `fda6080eb9e58844ebe52d6d8c957c22fd6de9da` passed all seven hosted jobs
across four workflows. The dedicated
[touchpad gate](https://github.com/Arthurshen926/HardwarePool/actions/runs/39573626828)
passed 25 Rust tests, including ten actual mTLS owner cases, native libevdev
compilation and fake-backend lifecycle checks. No kernel input device was
accessed. This is library/backend acceptance; executable touchpad launcher
composition remains a separate next step.

The [acceptance report](../../../docs/LINUX_TOUCHPAD_20261021_REPORT.md) preserves
the unrelated aggregate workspace-clippy failure and deferred OS testing limits.

Software-only checks (never open `/dev/uinput`):

```text
cargo test --locked -p capyio-linux-touchpad
cargo clippy --locked -p capyio-linux-touchpad --all-targets -- -D warnings
python3 platform/linux/capyio-touchpad/native/test_helper.py
```

Optional native compile with official libevdev development headers installed:

```text
cargo build --locked -p capyio-linux-touchpad --features native-uinput
```

Do not run the resulting helper as a diagnostic. Actual creation/injection is a
separate explicitly authorized OS acceptance step. No module loading, permission
changes, default-device settings or fallback injection are part of this feature.

The factory is single-use and the owner requires exact Runtime authority and a
matched authenticated Hello before device creation. Helpers are trusted regular
sibling executables, retain their own finite/idle deadlines and fail closed on
uncertain or partial writes. Stop must interrupt transport before worker join.
Normal cleanup releases slots/keys, synchronizes and destroys. Hard process kill
only guarantees owned descriptor closure, not explicit release events.

See [ADR 1535](../../../docs/adr/1535-linux-owned-type-b-touchpad.md) for the fixed
descriptor, pressure/confidence policy, bounded IPC and acceptance limits.
Physical Android contact capture and Linux desktop recognition/gestures remain
unverified; compilation and fake event traces are not physical acceptance.

## Executable software fixture boundary

The separate `test-fixture` feature builds only the fixed
`capyio-linux-touchpad-fixture-helper` sibling and exposes
`RuntimeTouchpadWorker::start_fixture`. It is an explicit software-only test
entrypoint, never a fallback for native access failure. `start` continues to
select only `capyio-linux-touchpad-helper`, including when both features are
built. Both siblings must be regular, same-owner files that are not writable
by group or others; symlinks are rejected. Neither constructor accepts a helper
path, device path, arbitrary command, or inherited environment override.

The fixture compiles the same C parsing, limits, ACK and cleanup loop with the
existing fake backend and a distinct archive/entrypoint. It requires no libevdev
headers and cannot open or issue ioctls on `/dev/uinput`. The caller supplies a
preopened trace descriptor and one closed enum mode: Normal, FailCreate,
PartialWrite, DelayedCommit, or Backpressure. The latter three act on batch 3,
after initial CancelAll and the first contact snapshot have committed. Partial
write fails after one event; delayed commit waits 100 ms; backpressure waits
1 s but aborts before writing the batch when its parent closes the private IPC.
The ordinary 250 ms Rust IPC budget therefore terminates backpressure early.

Trace records are `FRAME_BEGIN n`, `COMMIT_WAIT n` when applicable, each native
`EVENT type code value`, then `FRAME_COMMITTED n` before ACK. `CLEANUP_BEGIN`
marks terminal release events and `DESTROY` ends ownership. An interrupted
delayed frame records `FRAME_ABORTED n`. Trace output is unbuffered and
nonblocking; a full trace pipe fails closed instead of blocking the worker.
Callers must drain trace pipes concurrently and must not treat incomplete
traces as successful evidence. The fixture never records credentials.

```text
cargo test --locked -p capyio-linux-touchpad --features test-fixture
cargo clippy --locked -p capyio-linux-touchpad --all-targets --features test-fixture -- -D warnings
python3 platform/linux/capyio-touchpad/native/test_helper.py
```

The packaged fixture test executes only the distinct fixture binary; boundary
unit tests reject missing, linked and group/world-writable siblings without
executing any helper. None of these checks establishes desktop recognition,
physical input, gestures, permissions or kernel-device acceptance.
