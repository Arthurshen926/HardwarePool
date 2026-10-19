# Android local catalog update acceptance — ADR 1447

Date: 2026-09-18

## Build and target

- Authorized target: Lenovo TB323FU, Android 16.
- Unified package: `dev.capyio.android`, version `0.5.11-dev`, code 23.
- Debug APK SHA-256:
  `8DBC35897771EF487CC4ED124B4F9E1151C49A5527F6B805E762977B0041B196`.
- Update installed with existing application data retained. No permissions,
  foreground-service declarations or drivers were changed.

## Automated evidence

- Runtime: 9 tests passed.
- Testkit: 22 tests passed, including three new local replacement cases.
- Android JNI: 8 tests passed, including connected peer/Session preservation.
- Authenticated catalog link: 8 loopback mTLS tests passed.
- Desktop: 76 tests passed, 10 physical cases intentionally ignored.
- Targeted Clippy with warnings denied, documentation validation, Android
  capyioCheck (ARM64 JNI build, Java compilation, contract tests and Lint) and
  Windows debug build passed.
- Debug packaging warned that the JNI library was packaged without stripping;
  it did not fail the build. This is not release-package qualification.

## Physical application checks

The corrected wireless-debug endpoint connected to the expected tablet. A
temporary dynamically allocated ADB forward targeted only the CapyIO WebView;
it was removed after testing. Device endpoint, process ID and Node UUID are not
retained in this report.

1. Shared UI loaded with schema 5 and `android_node`, not Browser Mock.
2. The local catalog exposed seven classes: microphone, speaker, camera, IMU,
   gamepad, haptics and touchpad. Display has a separate panel; this check does
   not claim that it is part of the native catalog.
3. All five selectable Sources (microphone, camera, IMU, gamepad and touchpad)
   accepted a selection change through the real Java bridge/Service/JNI path.
   Immediate snapshot readback matched. Every initial selection was restored.
4. All seven local hardware actions remained `idle`; no microphone/camera
   capture, input injection, speaker playback or vibration was started.
5. Local sharing, Remote resources and Settings each navigated to the expected
   heading. At the tested viewport, document width did not exceed viewport width.
   This is DOM-level navigation/overflow evidence, not a visual design review.

## Limits

The physical app had no authenticated peer or Route. Connected-state retention
is proved by automated Runtime/JNI tests, not by this tablet run. Discovery,
pairing, secure credential persistence, actual resource directory exchange,
automatic reconnect and media/input transmission were not physically accepted.
This report is not a full-feature or ordinary-user release sign-off.
