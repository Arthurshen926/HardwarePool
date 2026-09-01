# CAPY-CAMERA-001C12 — Bounded late-receiver connection retry

Date: 2026-08-30

Status: implementation, build validation and bounded V2419A late-receiver
verification complete.

## Objective

Allow the foreground Android camera session to start shortly before the fixed
ADB-reverse Windows lab receiver without permanently losing export after one
refused connection or bridge write failure. This is a bounded usability
improvement, not production network reconnect or discovery.

## Implementation

- `LoopbackConnectRetryPolicy` fixes 20 attempts, a 500 ms per-attempt connect
  timeout and a 500 ms inter-attempt delay.
- The existing worker thread owns all socket creation, connect, sleep and CAVC
  writes. A connect or write failure closes that session and consumes one retry
  attempt. Camera2 and MediaCodec callbacks continue to make only non-waiting
  offers to the two-entry drop-oldest queue.
- While waiting, the foreground status reports the current attempt and fixed
  bound. Once connected, the existing latest-frame/key-frame recovery sends the
  dedicated CAVC config followed by a discontinuity-marked access unit.
- Every replacement socket receives a fresh transport-free
  `AvcWireSessionEncoder` for the same stream key. This permits config replay and
  later-key-frame recovery without changing the CAVC record format.
- Closing the Activity closes any in-progress socket and interrupts the worker;
  retry cannot outlive the visible camera session.
- Exhaustion remains an explicit stopped export. There is no infinite retry,
  arbitrary destination, background service or mid-stream reconnect claim.

Depending on whether the platform refuses immediately or consumes the full
connect timeout, the fixed retry window is approximately 10–20 seconds. An
initial physical attempt with a 250 ms delay exhausted before the Windows tool
became ready, so that shorter bound was rejected rather than reported as pass.
A second attempt showed that ADB reverse can accept the Android-side connect
before a Windows listener exists and fail only on write; connect-only retry was
therefore also rejected and expanded to the bounded whole-session retry above.

## Evidence

- The no-dependency contract test covers first/last attempt, exhaustion,
  timeout, revised delay and invalid attempt rejection.
- Offline Gradle 9.5.0 `contractTest :app:assembleDebug :app:lintDebug` passed.
- Repository structural validation passed with the retry policy and sender
  composition pinned as required boundaries.
- The rebuilt APK SHA-256 is
  `5B894B439E49D9ED4CDE561F72422F63CD6CE568D0C25E488202FF9B707CFE01`.
- Permissions remain exactly CAMERA and INTERNET, with no service.

## Physical evidence

After exact-target authorization, the final hash-locked APK was installed on
`V2419A / PD2419` at `100.66.157.119:40263`; CAMERA and INTERNET remained the
only declared permissions.

Two intermediate attempts were retained as design evidence rather than pass:

- a 250 ms delay exhausted too quickly under immediate refusal;
- connect-only retry did not cover ADB reverse accepting Android first and
  surfacing the missing Windows listener as a later write failure.

The accepted implementation used the revised 500 ms delay and whole-session
retry. Windows port 38173 was confirmed without a listener before Android
capture and again after three seconds of active camera ID 0. The receiver then
started and accepted stream `56ba689d9dc61998a2e77c93c7180107`, epoch
`17961779626517`:

- 120 access units and 120 decoded 1280x720 NV12 frames;
- four key frames and one explicit discontinuity;
- source sequence advanced to 211, consistent with newest-frame recovery after
  the receiver-absent interval;
- distinct first/last checksums;
- Windows decoder low-latency mode enabled and zero pending-sample backlog.

Final cleanup force-stopped the app, removed the reverse mapping and left no
active Camera Service client or receiver process. No camera image was retained.
