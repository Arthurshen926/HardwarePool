# CAPY-PTP-002S Report

Date: 2026-08-30

Status: bounded-host-channel native one-finger acceptance complete.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

The concrete 002R host channel has now reached a real Windows synthetic
Precision Touchpad. One exact-name ignored test constructed the device through
authorized Runtime preflight, then carried CancelAll, one-finger down,
horizontal move and release through the Runtime sender, four-packet host queue
and Runtime receiver.

All four packets were enqueued, received and processed. Contacts were released,
the Sink closed, and the Runtime Route completed stop. The native Windows API
returned success for creation and submission. No independent visual pointer
observation was recorded, so the evidence does not claim one.

## Controlled operation

The user explicitly authorized creation of the user-mode virtual touchpad and
the fixed desktop input injection. The exact command ran only one ignored test;
18 other tests were filtered out. It installed no driver, changed no boot or
security setting, connected no Android device and opened no network socket.

## Validation

```text
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker
cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker authorized_bounded_host_channel_submits_one_finger_motion_to_real_windows_touchpad -- --ignored --exact --nocapture
cargo clippy -p capyio-remote-touchpad-adapter --all-targets -- -D warnings
cargo xtask ci
cargo xtask validate-docs
git diff --check
```

Results: the exact native test passed in 0.06 seconds; default Runtime-worker
tests retained 13 pass and six ignored. Clippy passed with warnings denied, and
full repository CI/document validation passed.

## Files

- `adapters/remote-touchpad/tests/touchpad_runtime_worker.rs`
- `adapters/remote-touchpad/README.md`
- `docs/plans/completed/0043-touchpad-host-channel-native-smoke.md`
- architecture, private protocol, security, testing, product scope, build status
  and traceability documentation.

## Remaining work

- Android OS/JNI/APK integration and authenticated cross-device transport remain
  absent.
- Visible one-/three-/four-finger behavior remains independently unobserved;
  two-finger scrolling retains prior user-confirmed evidence.
- No driver installation or Android connection was performed.
- The worktree remains uncommitted and based on `fc3da36`.
