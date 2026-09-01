# CAPY-PTP-002K Report

Date: 2026-08-30

Status: separately authorized composed two-finger native pan submission
complete; default and controlled validation pass.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

A fixed two-contact vertical pan has crossed the complete current in-process
path from a real Active `NodeRuntime` Route through private packets and the
bounded worker into real Windows synthetic-touchpad submission. Release, Sink
close, device destruction and Runtime stop completed successfully.

The test proves multi-contact native submission and lifecycle success. It does
not itself observe the foreground application. After the run, the user
explicitly reported that scrolling was visible; this human observation is
retained separately from the automated native-submission result.

## Fixed contact lifecycle

The four frames are:

1. `CancelAll`;
2. contact 1 at `(3500, 2000)` and contact 2 at `(6500, 2000)`;
3. both stable IDs move to y `4500` while retaining their x coordinates; and
4. an empty update releases both contacts.

Each frame produces one private packet and one worker pump. A fixed 12 ms host
interval separates native submissions. Metrics reach four enqueued and four
processed packets before close.

## Controlled harness

The reusable Windows-only acceptance fixture constructs and activates an
authorized worker, submits only compiled frame slices and centralizes explicit
Sink/Runtime cleanup. It is test-only and all desktop-impact tests remain
exact-name ignored.

Default worker results at completion: eight passed, three ignored.

Authorized command:

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker authorized_worker_submits_two_finger_pan_then_releases_and_closes -- --ignored --exact --nocapture
```

Result: one passed, zero failed, ten filtered out, completed in approximately
0.05 seconds. Full default `cargo xtask ci` passed immediately beforehand.

## Files

- `adapters/remote-touchpad/tests/touchpad_runtime_worker.rs`
- `adapters/remote-touchpad/README.md`
- `docs/plans/completed/0035-private-touchpad-composed-two-finger-pan-smoke.md`
- architecture, private protocol, security, testing, product scope, build
  status and traceability documentation.

## Device and rollback record

- Device: ephemeral five-contact, 100 x 60 mm Windows synthetic touchpad.
- Route: real authorized Active AdapterManaged Route, epoch 1.
- Submission: four compiled packets with at most two contacts.
- Final semantic state: empty release snapshot.
- Primary rollback: worker stop, then Runtime stop.
- Fallback rollback: receiver/session/device Drop and process exit.
- Persistent driver/package/security change: none.

## Remaining work and risks

- Visible foreground scrolling was confirmed by the user after the controlled
  run; direction/amount were not instrumented.
- Three- and four-finger composed submissions remain separately authorized
  acceptance work.
- No live Android runtime capture, authenticated transport or production Node
  task loop exists yet.
- The worktree remains uncommitted and based on `fc3da36`.
