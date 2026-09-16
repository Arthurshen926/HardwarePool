# Unified Android non-display test status

Version: `0.5.3-dev`

Artifact: `platform/android/app/build/outputs/apk/debug/app-debug.apk`

## Integrated in this APK

- one persisted Android Node identity and one hardware catalog;
- the same packaged Vue UI used by Windows;
- microphone Source and speaker Sink Service lifecycles plus bounded native-LAN
  workers;
- Camera2/MediaCodec source selection, capture and private AVC exporter;
- live Android IMU capture lifecycle;
- touch gamepad and multitouch surface controls;
- vibrator/haptics Sink;
- source publication checkboxes;
- ARM64 Rust `NodeRuntime` ownership of the selected publication catalog.

## What can be tested from this artifact

The APK can be manually installed on an ARM64 Android 10+ device after separate
human authorization. Its unified UI and local hardware lifecycles can then be
tested together. Camera and microphone still require Android-owned runtime
permission prompts.

The default artifact deliberately has no compiled audio peer, so microphone and
speaker transport fail closed until an explicit Windows peer IPv4 is supplied
at build time. No endpoint is inferred from the wireless-debugging address.

## Remaining before complete cross-device acceptance

- package and activate authenticated peer/catalog transport;
- connect the unified touchpad controls to the proven private Windows touchpad
  receiver instead of the separate lab APK;
- connect unified gamepad and standalone IMU samples to their Windows Route
  workers;
- configure and repeat microphone/speaker transport against the intended
  Windows peer;
- perform one explicitly authorized install and physical acceptance run.

The current package is therefore ready for unified UI/local-lifecycle testing,
but not yet evidence that every capability reaches Windows from this one APK.
