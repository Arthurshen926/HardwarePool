# Finite Linux touchpad executable candidate: 2026-10-21

## Scope and baseline

Based on accepted camera launcher commit
`bbf6531731f27e7243f10fa4a2e80ae780f14b74`. ADR 1537 adds the finite
`capyio-node linux-touchpad --trusted-lab` composition around the accepted
RuntimeTouchpadWorker. Camera acceptance documentation is reconciled in this
candidate with the publisher's exact-head all-seven-job result; camera source
is unchanged.

Production preserves Android's touchpad Source-listener convention, paired
credential/pin identity and exact Route/Stream/epoch binding. Private framed
bootstrap and local approval follow camera's entry convention. Actual Android
Source consent remains independent. A validated Source lease plus local Sink
lease precede data admission; the Sink deadline is shortened to the Source
lease so the existing independent watchdog enforces both. Neither catalog nor
Starting state claims a created device or visible input.

A separate feature/command/helper executes the production projector and helper
IPC/C loop with fake syscalls only. It does not modify the production native
path or accept arbitrary helper/device/trace filenames. The production helper
still requires existing `/dev/uinput` permission and a same-owner non-writable
regular sibling. No permission changes, module loads or real input injection
were performed.

## Validation performed in the cloud workspace

- 18 actual node subprocess tests: ten default admission/control tests and eight
  explicit software-fixture tests. The latter also include the ten default
  cases in their feature-enabled run.
- Five contact slots, fresh tracking lifetimes, pressure and real Android contact
  transformation; dropped-sequence suppression and CancelAll recovery.
- Test consent calls the existing public wrapper over Android's production
  TouchpadConsentGate; contact frames use AndroidTouchpadCaptureSession. This
  does not execute Android Java/JNI service or physical MotionEvent capture.
- Backend-commit-before-ACK behavior, delayed commit, partial native write and
  backpressure rejection without acknowledging uncertain input.
- Exact approval and independent Android denial; strict config/authenticated
  Source catalogs; denied or missing Source leases; control/data pin rejection,
  changed epoch offer, mismatched Hello and partial data handshake cancellation.
- Active local/control Stop, control/data/local EOF, revoke and earlier Source
  lease expiry; terminal release/destruction and no later helper work.
- 47 host unit tests passed, including new config/catalog tests.
- Touchpad crate default 25 and fixture-feature 28 Rust tests passed, retaining
  all ten actual mTLS owner tests and twelve projector cases. The extra cases
  cover fixed helper ownership/path checks and the packaged fake binary.
- Strict all-target fixture Clippy and formatting passed.
- Extended production-C-loop fake syscall suite passed, including trace output
  backpressure. Fixture C archive has no open/ioctl/libevdev references.
- Existing camera 13, capture 14, playback 26 and preflight one subprocess tests
  passed. Their four explicitly ignored private-graph cases were not run here;
  they remain separate hosted gates, not inferred passes.
- Documentation traceability validation passed. Workflow YAML/shell syntax and
  exact-name gate tests passed.

Independent review identified Runtime locks held across status/heartbeat I/O
and an early cleanup return. These were fixed: snapshots leave guards before
writes; live heartbeats send outside the guard; cleanup always attempts worker
Stop and connector join even if Runtime lock acquisition fails. Teardown tests
check the final cleanup trace, rather than counting startup CancelAll releases,
and retain each live helper's exact PID, direct parent and fixed executable so
post-terminal absence also detects orphaning/reparenting.

## Hosted gate and remaining limits

The existing Linux touchpad workflow retains all prior library/native-fake gates
and adds exact presence/success checks for every actual-node test, fixed helper
packaging checks, and native Node/helper compilation with official libevdev.
Both native and fixture features are compiled together to verify separate C
entrypoints; the native executable is never run. Other workflows/jobs remain
unchanged.

Local native-uinput compilation was unavailable because official libevdev
headers are absent. Exact-head hosted native build and required checks remain
pending publication; this report is not a hosted pass claim.

All device behavior tested here is simulated at the syscall boundary. Kernel
uinput creation, desktop device classification, OS gestures, physical Android
capture and user-session input remain deferred. Stopping during an in-flight
native ACK can terminate with an explicit sanitized error even after the helper
has destroyed its device; that is not misreported as a successful graceful
release. A hard-kill fallback cannot guarantee explicit release events.
