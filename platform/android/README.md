# Android Host Adapter

The Android host will provide the phone's microphone and speaker while reusing the shared Rust Core, Protocol, Runtime and Vue UI.

## Native responsibilities

- runtime permission requests;
- foreground-service ownership of active microphone/render sessions;
- persistent privacy notification;
- audio focus and route change handling;
- microphone capture and speaker render Adapter;
- lock-screen/background lifecycle;
- Kotlin ↔ Rust command/event boundary;
- real-device metrics and diagnostics.

## Explicit non-responsibilities

- Android does not register a system-wide virtual microphone or speaker for arbitrary Android apps in the MVP.
- Activity lifetime does not own an active audio session.
- Platform callbacks do not mutate Core state directly; they report completions/events to the Runtime.

## First spike

Create an isolated Audio Lab before networking:

1. enumerate actual input/output route and negotiated format;
2. render a sine/chirp and WAV;
3. capture microphone to WAV;
4. report sample rate, channel count, frames-per-burst and xrun counters;
5. verify foreground-service, lock-screen and permission behavior on the vivo test phone.

## Touchpad bridge slice

`capyio-jni` and `touchpad-bridge` provide a buildable, primitive-only
`MotionEvent` to private touchpad packet boundary. The Kotlin library owns
framework extraction and the Rust library owns lifecycle mapping and packet
encoding. The module declares no Android permission or component and performs
no transport. It is an embeddable library, not an APK.

The separately scoped `touchpad-bridge/lab-app` is an explicitly authorized
debug-only physical-device harness. It declares only `INTERNET`, sends the same
bounded private records through an ADB reverse tunnel, and is not a production
Android Node host or network transport.

The lab Activity consumes touch locally even while the Windows listener is not
yet available, but it does not advance the Rust stream until the initial ADB
connection succeeds. It retries only that initial connection. Its foreground
window uses immersive system bars, a full-surface system-gesture exclusion
region and disallows parent interception. These are best-effort Android app
controls: an OEM may still intercept a global multi-finger gesture before the
Activity receives `MotionEvent`, so suppressing such a vivo gesture requires a
separate, explicit device-setting decision.

High-rate `ACTION_MOVE` input is sampled before JNI at no more than roughly
60 Hz and is skipped while four motion records are pending. Lifecycle events
remain unsampled. This preserves bounded latency and reserves queue capacity for
down, pointer changes, release and cancellation instead of growing a stale
motion backlog.

The v0.7 lab tuning keeps one-/two-finger coordinates unchanged. Added contacts
hold MOVE for a bounded 72 ms assembly window; lifecycle snapshots still cross
immediately. Once one gesture reaches three contacts, the pure Rust Android
mapper rebases each active contact without a jump and applies 700-per-mille
spatial gain until complete release. Windows still owns gesture recognition;
this policy only conditions bounded contact motion.

The v0.8 lab reports a probable OEM conflict when Android cancels a stream that
had reached at least three contacts. The first conflict explains the boundary
and offers a user-initiated intent to vivo's Super Screenshot settings, with a
generic Android Settings fallback. It does not write a device setting, request
another permission or claim that gesture exclusion rectangles suppress an OEM
full-screen monitor.

Version 1.0 starts the sender only after Hello and initial cancellation are
queued, and it closes the lab session when the Activity leaves the foreground.
This prevents any foreground-app switch from retaining a misleading connected
state while CapyIO no longer owns touch. Version 1.2 makes that generic
foreground requirement visible and explains that reopening also requires a
running Windows receiver. It adds no permission. The bounded continuous lab
wrapper uses `--manual-session`; one-shot cursor diagnostics use a separate
exact-contact/motion gate and intentionally exit after acceptance.

For the installed debug harness, a new wireless-debugging port can be connected
without repeating manual ADB commands:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File `
  scripts/connect_android_touchpad_lab.ps1 `
  -AdbSerial 100.66.157.119:44801
```

The helper verifies the exact ADB endpoint, installed app version, loopback
listener, reverse mapping, established connection and top-resumed Activity. It
reuses an active session. If the receiver is absent, it fails closed unless the
caller explicitly adds `-StartReceiver`, which requests UAC for the separately
gated VHF wrapper. It never installs an APK or driver and never requests a
Windows restart.
