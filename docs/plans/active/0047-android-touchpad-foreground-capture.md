# CAPY-PTP-002W — Android foreground touch capture and gesture exclusion

Status: completed

Owner: Codex

Created: 2026-08-30

Requirements: `FR-SCEN-006`, `NFR-STAB-001..004`, `NFR-SEC-001..003`,
`NFR-PERF-001..003`, `NFR-MAINT-001..003`

## Objective

Make physical foreground touch observable before transport readiness, retain a
live Android-to-Windows session, and apply the strongest ordinary app-owned
controls against conflicting Android system gestures.

## In scope

- consume and draw 1–5 contacts while the initial receiver connection waits;
- retry only the initial loopback connection without advancing packet state;
- immersive system bars, system-gesture exclusion and parent-intercept denial;
- vivo launch compatibility and an installed debug APK;
- manual physical one-, two-, three- and four-finger observation.

## Out of scope

- changing vivo system settings or using privileged/device-owner APIs;
- claiming suppression of gestures intercepted before `MotionEvent` delivery;
- production reconnect, authentication or background service lifecycle.

## Acceptance criteria

1. Touch is consumed and drawn before the receiver connects.
2. Initial connection failure does not permanently stop capture or mutate the
   private stream sequence.
3. APK starts on the target vivo phone without a window-insets crash.
4. The live listener accepts Hello and creates the Windows synthetic touchpad.
5. A human observes physical one- and multi-finger behavior and records any OEM
   gesture still intercepted by the phone.

## Required tests and evidence

```text
platform/android/touchpad-bridge/gradlew.bat :lab-app:assembleDebug :lab-app:lintDebug --offline --no-daemon
cargo xtask ci
cargo xtask validate-docs
git diff --check
```

Artifacts to retain:

- exact APK permission inventory, version and SHA-256;
- Android crash or launch evidence;
- Windows Hello/device-creation output;
- user-observed physical-touch result.

## Dependency changes

None.

## Safety and approvals

- privileged/device operations required: yes, APK install and desktop input;
- target: explicitly approved V2419A and current Windows controlled lab;
- forbidden operations: system-setting, driver or boot-policy changes.

## Completion record

Implemented:

- local capture while disconnected, bounded initial retry and app-owned gesture
  exclusion;
- vivo-safe insets initialization and debug APK version 4.
- pre-JNI 60 Hz/high-water MOVE sampling and a separately selected bounded
  10-minute manual receiver session; installed debug APK version 5.
- version 6 contact-count telemetry, graceful close draining, receiver contact
  transitions and a bounded Windows click-target harness.
- Windows native DOWN/UPDATE/UP/CANCELED phase projection, a fixed stationary
  one-finger tap fixture, and submission-failure frame diagnostics.

Validated:

- Android assemble/lint: pass;
- live Hello validation and real synthetic device creation: pass.
- targeted receiver test/Clippy, Android assemble/lint and final diff check:
  pass.
- full repository CI after closing the version 5 receiver: pass.
- physical one-finger motion and two-finger scrolling on version 5: observed by
  the user without a new queue fault.
- version 6 installed and verified by installed-APK hash; a 785-frame physical
  run reached four Android/Windows contacts and closed the synthetic device
  cleanly.
- live Windows `SPI_GETTOUCHPADPARAMETERS` reported an active, enabled,
  five-contact Precision Touchpad with tap, pan and zoom enabled.
- fixed one-finger motion changed the system cursor from `(1273,68)` to
  `(482,380)`; the user confirmed tap, one-finger motion and two-finger pan.

Not validated:

- why the moving Windows cursor is not visibly rendered to the user;
- why accepted three-/four-contact physical and fixed fixtures produce no
  visible configured Windows global action;
- exact vivo three-finger screenshot control and separately approved change;

Subsequent VHF slices resolved the functional uncertainties above: the host
cursor moved under physical Android input, the user disabled the conflicting
vivo gesture, and physical three-/four-contact input produced Windows Shell
actions. Production reconnect, authentication and background lifecycle remain
explicitly outside this completed foreground-capture slice.
