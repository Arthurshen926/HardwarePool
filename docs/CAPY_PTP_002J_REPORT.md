# CAPY-PTP-002J Report

Date: 2026-08-30

Status: separately authorized composed one-finger native submission complete;
default and controlled validation pass.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

One fixed one-finger lifecycle has passed through the complete current
in-process path:

```text
real NodeRuntime Route
  -> authorized worker/factory
  -> private packet codec
  -> bounded Route queue and receiver
  -> SyntheticTouchpadSession
  -> Windows native submission
```

The exact controlled test passed and completed release, Sink close, device
destruction and Runtime stop. It verifies API submission and lifecycle success;
no independent human observation of pointer displacement was recorded by the
test, so visible behavior remains a separate evidence category.

## Fixed input lifecycle

The compiled input contains exactly four semantic frames:

1. `CancelAll` establishes empty state;
2. contact 1 down at `(2000, 3000)` himetric;
3. contact 1 moves to `(6500, 3000)` himetric; and
4. an empty update releases all contacts.

Each frame is encoded into one private packet, enqueued against a current Active
Route and pumped individually. A fixed 12 ms host interval separates native
submissions. Worker metrics must reach four enqueued and four processed before
close.

## Default isolation

The Windows-only test is exact-name and ignored with a desktop-pointer impact
message. Ordinary worker tests report eight passed and two ignored; default
`cargo xtask ci` does not create a touchpad or submit input.

## Exact evidence

Authorized command:

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker authorized_worker_submits_one_finger_motion_then_releases_and_closes -- --ignored --exact --nocapture
```

Result: one passed, zero failed, nine filtered out, completed in approximately
0.06 seconds.

The full default `cargo xtask ci` passed immediately before the controlled
desktop-input command.

## Files

- `adapters/remote-touchpad/tests/touchpad_runtime_worker.rs`
- `adapters/remote-touchpad/README.md`
- `docs/plans/completed/0034-private-touchpad-composed-one-finger-native-smoke.md`
- architecture, private protocol, security, testing, product scope, build
  status and traceability documentation.

## Device and rollback record

- Device: ephemeral five-contact, 100 x 60 mm Windows synthetic touchpad.
- Route: real authorized AdapterManaged Runtime Route, epoch 1.
- Submission: four fixed packets; no user/network-supplied payload.
- Final semantic state: empty release snapshot.
- Primary rollback: worker stop after release, then Runtime stop.
- Fallback rollback: receiver/session/device Drop and process exit.
- Persistent driver/package/security change: none.

## Remaining work and risks

- No authenticated live transport or production Node task loop exists.
- Android runtime/JNI touch capture remains DTO-only.
- Visible pointer movement from this exact composed test was not independently
  confirmed in retained evidence.
- Two/three/four-finger composed submission and gesture observation remain
  separately approved acceptance work.
- The worktree remains uncommitted and based on `fc3da36`.
