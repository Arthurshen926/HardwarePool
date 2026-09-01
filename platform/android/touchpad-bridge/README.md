# Android touchpad bridge

This Android library owns the narrow `MotionEvent` to JNI projection for the
private remote-touchpad packet source. It deliberately contains no Activity,
service, socket, permission, pairing logic, or APK packaging.

The ARM64 JNI library is built separately into `target/android-jni`:

```text
cargo ndk -t arm64-v8a -o target/android-jni build -p capyio-android-jni --release
```

The Android host must start the session before forwarding events, transmit each
returned private packet in order over an already authenticated touchpad route,
and transmit the optional final cancellation returned by `stop` or `close`.

The current capture policy identity-maps one and two contacts. On reaching three
contacts it continuously rebases the active set and attenuates later deltas to
700 per mille until complete release. The lab Activity additionally suppresses
MOVE for 72 ms after an added pointer while retaining every lifecycle event.
Neither layer recognizes gestures; Windows Precision Touchpad policy remains
authoritative.

The debug lab app separately treats `ACTION_CANCEL` after three or more
contacts as a probable OEM-system interception, displays a counter and offers a
user-driven vivo settings route. That diagnostic is Activity policy, not part
of this reusable bridge and not authority to mutate device settings.
