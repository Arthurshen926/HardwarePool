# CAPY-PTP-002I Report

Date: 2026-08-30

Status: explicitly authorized real Windows factory create/close smoke complete;
default and controlled validation pass.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

The production `WindowsSyntheticTouchpadSinkFactory` has now been invoked
through the authorized Runtime worker boundary on the Windows host. A real
synthetic touchpad was created and immediately closed/destroyed. The worker
never activated and no packet or touchpad frame was enqueued, processed or
submitted.

This confirms actual factory composition and device lifecycle only. It does not
claim pointer movement, gesture recognition or Android-to-Windows live input.

## Controlled test

The new Windows-only test is marked ignored with an explicit device-impact
message. It:

1. constructs a real `NodeRuntime` catalog, Session and AdapterManaged touchpad
   Route;
2. authorizes, prepares and begins Route epoch 1 in Starting state;
3. runs side-effect-free Route/contract preflight;
4. calls the real Windows factory to open `SyntheticTouchpadSession`;
5. verifies worker state is Starting and packet counters remain zero;
6. explicitly stops the worker, drops it and completes Runtime stop.

Because no frame is submitted, the close path has no retained contact to
cancel. RAII and process exit remain fallback destruction if explicit cleanup
cannot complete.

## Exact evidence

Default isolation:

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker
```

Result: eight passed, one ignored.

Explicitly authorized command:

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker authorized_windows_factory_opens_and_closes_real_synthetic_touchpad_without_frames -- --ignored --exact --nocapture
```

Result: one passed, zero failed, eight filtered out.

The full default `cargo xtask ci` also passes and does not execute the ignored
device test.

## Files

- `adapters/remote-touchpad/tests/touchpad_runtime_worker.rs`
- `adapters/remote-touchpad/README.md`
- `docs/plans/completed/0033-private-touchpad-real-windows-factory-smoke.md`
- architecture, security, testing, product scope, build status and traceability
  documentation.

## Device and rollback record

- Device: ephemeral five-contact, 100 x 60 mm Windows synthetic touchpad.
- Creation: `WindowsSyntheticTouchpadSinkFactory::open` after authorized
  Runtime preflight.
- Submission: none; worker counters remained zero.
- Primary rollback: `PrivateTouchpadRuntimeWorker::stop`.
- Fallback rollback: `SyntheticTouchpadSession`/device Drop and process exit.
- Persistent driver/package/security change: none.

## Remaining work and risks

- No authenticated live transport or Node production task loop exists.
- Android runtime/JNI touch capture remains DTO-only.
- The real factory is proven only for zero-frame creation/close through the
  worker; packet-to-native submission through this exact composition remains a
  separate explicitly authorized acceptance.
- Visible two/three/four-finger behavior still requires separately approved
  physical acceptance.
- The worktree remains uncommitted and based on `fc3da36`.
