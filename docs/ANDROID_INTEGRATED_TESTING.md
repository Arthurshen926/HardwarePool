# Unified Android non-display test status

Version: `0.5.4-dev`

Artifact: `platform/android/app/build/outputs/apk/debug/app-debug.apk`

## Integrated in this APK

- one persisted Android Node identity and one hardware catalog;
- the same packaged Vue UI used by Windows;
- microphone Source and speaker Sink Service lifecycles plus bounded native-LAN
  workers;
- Camera2/MediaCodec source selection, capture and private AVC exporter;
- live Android IMU capture plus the existing bounded Controller Lab UDP sender;
- touch gamepad controls on that same UDP sender;
- multitouch controls over the existing JNI/private-record/ACK path;
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
at build time. Controlled-lab defaults use phone/Windows ports 40000/40001 for
speaker and 40010/40011 for microphone, matching ADR 0051. No endpoint is
inferred from the wireless-debugging address.

Touchpad acceptance uses the existing controlled loopback receiver:

```powershell
adb -s <LAB_IPV4_03>:44187 reverse tcp:61000 tcp:61000
cargo run -p capyio-remote-touchpad-adapter --bin capyio-ptp-adb-lab -- `
  --inject --acknowledge-desktop-input --manual-session
```

Start the receiver before invoking the Android Touchpad quick action. This
default user-mode projection does not install a driver. The optional `--vhf`
path is outside this run because driver operations require separate approval.

For gamepad/IMU, open the Windows Controller view, start **监听手机**, then copy
the displayed Windows IPv4, UDP port and token into the Android Controller view
and press **保存发送目标**. Starting Touch gamepad, IMU, or both then uses the
same receiver previously exercised by the standalone Controller Lab.

## Remaining before complete cross-device acceptance

- package and activate authenticated peer/catalog transport;
- configure and repeat microphone/speaker transport against the intended
  Windows peer;
- perform one explicitly authorized install and physical acceptance run.

The current package now contains every previously separate non-display Android
sender. Physical cross-device acceptance is still pending because this task did
not install the APK or create receiver processes/mappings.
