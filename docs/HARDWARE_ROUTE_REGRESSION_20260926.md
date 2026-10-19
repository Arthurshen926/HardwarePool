# Hardware Route regression — 2026-09-26

## Acceptance boundaries

Touchpad usability was confirmed by the user. That result is not evidence for
other hardware. Native audio physical checks are recorded separately in
`NATIVE_AUDIO_PREFLIGHT_AND_READINESS_20260926.md`.

## Camera physical regression

Device: Y700 TB323FU, Android 16, existing CapyIO debug APK 0.5.12-dev (24).
ADB selected the explicitly authorized Y700; its transient address is not configuration.

Started the existing camera quick action with a temporary ADB reverse mapping
for TCP 38173 and ran:

```text
target/debug/capyio-avc-lab-receiver.exe --port 38173 --max-access-units 30 --decode-nv12
```

Receiver exited successfully: 1280x720, 30 fps configuration, 4 Mbps target,
rotation 90, Annex B, 30 access units, one key frame, one discontinuity marker,
734954 encoded payload bytes. All 30 frames decoded to NV12 (41472000 bytes).
Decoder latency averaged 3117 microseconds, maximum 12238 microseconds. This
is decoder timing, not end-to-end latency. No media content was saved.

Stopped the camera through the app, removed only the test's TCP 38173 reverse
mapping, and checked the camera quick action was Idle, routeId null, no problem.

This passes bounded capture/transport/decode over the **ADB compatibility
path**. It does not validate automatic Route negotiation, Windows virtual camera
publication, OBS consumption, long-duration stability or disconnection recovery.

## Dynamic audio groundwork

ADR 1491 adds Rust/Java dynamic worker bindings and deadline/epoch validation.
Production audio starts are not yet wired to these contracts. The remaining
audio milestone is authenticated offer negotiation, dynamic configuration on
both workers, authorization/lease ownership, Stop/disconnect cleanup and actual
data-based readiness. Camera, IMU, gamepad and display also require independent
Route lifecycle acceptance; this report does not mark them complete.
