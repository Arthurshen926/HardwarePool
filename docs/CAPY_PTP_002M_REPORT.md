# CAPY-PTP-002M Report

Date: 2026-08-30

Status: separately authorized composed four-finger native swipe submission
complete; default and controlled validation pass.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

The fixed four-contact horizontal swipe crossed the complete current in-process
path from a real Active `NodeRuntime` Route through the private codec and bounded
worker into real Windows synthetic-touchpad submission. All frames were
accepted; release, cancellation, Sink close, device destruction and Runtime
stop completed successfully.

This completes composed native-submission evidence for fixed one- through
four-contact fixtures. It does not independently observe which Windows system
action, if any, the current four-finger policy displayed.

## Fixed contact lifecycle

The 11 frames are:

1. initial `CancelAll`;
2. eight gradual updates retaining IDs 1..4, translating x from 2500 to 7500
   at fixed y positions 1500, 2500, 3500 and 4500;
3. an empty update releasing all contacts; and
4. final `CancelAll`.

Each frame produces one private packet and one worker pump. The fixture's fixed
15 ms interval separates native submissions. Metrics reach 11 enqueued and 11
processed packets before close.

## Controlled harness

The Windows-only acceptance fixture constructs and activates an authorized
worker, accepts only compiled frame slices and centralizes explicit
Sink/Runtime cleanup. All desktop-impact tests remain exact-name ignored.

Default worker results: eight passed, five ignored. Targeted Clippy passed. The
first full CI attempt reached two unrelated Audio Share tests but Windows denied
their sandboxed local TCP binds with error 10013. The unchanged full CI passed
outside that socket restriction before the controlled input run.

Authorized command:

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker authorized_worker_submits_four_finger_swipe_then_releases_and_closes -- --ignored --exact --nocapture
```

Result: one passed, zero failed, twelve filtered out, completed in approximately
0.17 seconds.

## Files

- `adapters/remote-touchpad/tests/touchpad_runtime_worker.rs`
- `adapters/remote-touchpad/README.md`
- `docs/plans/completed/0037-private-touchpad-composed-four-finger-swipe-smoke.md`
- architecture, private protocol, security, testing, product scope, build
  status and traceability documentation.

## Device and rollback record

- Device: ephemeral five-contact, 100 x 60 mm Windows synthetic touchpad.
- Route: real authorized Active AdapterManaged Route, epoch 1.
- Submission: 11 compiled packets with at most four contacts.
- Final semantic state: empty release followed by CancelAll.
- Primary rollback: worker stop, then Runtime stop.
- Fallback rollback: receiver/session/device Drop and process exit.
- Persistent driver/package/security change: none.

## Remaining work and risks

- Visible Windows four-finger behavior was not independently observed.
- Fixed-fixture acceptance is not live phone control: Android runtime capture,
  authenticated transport and the production Node task loop remain absent.
- One-finger visual behavior and three-finger visual behavior are also still
  unrecorded.
- The worktree remains uncommitted and based on `fc3da36`.
