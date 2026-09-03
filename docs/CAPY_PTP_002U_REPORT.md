# CAPY-PTP-002U Report

Date: 2026-08-30

Status: Android MotionEvent JNI bridge and ARM64 library complete.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

`capyio-android-jni` now composes `AndroidTouchpadCaptureSession` with
`PrivateTouchpadPacketSource`. Its version-1 DTO accepts only primitive values,
validates complete equal-length pointer arrays and returns the existing bounded
private packet bytes. Start, stop and close retain explicit cancellation.

The Kotlin Android library owns real `MotionEvent` extraction, rejects
cross-thread session use, clamps view-local coordinates to the declared touch
area and passes complete pointer arrays including the lifted pointer. It contains
no Activity, service, receiver, transport or permission.

## Real build and device evidence

- Microsoft OpenJDK 17.0.20.1;
- Android SDK/API 36 and NDK 29.0.14206865;
- Rust `aarch64-linux-android` plus cargo-ndk 4.1.2;
- jni-rs 0.22.4, Android Gradle Plugin 9.3.1 and Gradle 9.5.0;
- `libcapyio_android_jni.so` built for `arm64-v8a`;
- release AAR assembled with `classes.jar`, manifest and
  `jni/arm64-v8a/libcapyio_android_jni.so`;
- wireless ADB connected at `100.66.157.119:40263`; read-only properties report
  model `V2419A`, ABI `arm64-v8a`, API 36.

No APK was created or installed, and the device was not modified.

## Validation

```text
cargo test -p capyio-android-jni --offline
cargo ndk -t arm64-v8a -o target/android-jni build -p capyio-android-jni --release --offline
platform/android/touchpad-bridge/gradlew.bat :bridge:assembleRelease --offline --no-daemon
cargo xtask ci
cargo xtask validate-docs
git diff --check
```

The targeted Rust tests cover one-finger packet sequencing, two-finger contact
preservation and transactional array rejection. The final ARM64 library exports
all five expected JNI entry points. The Android native and AAR builds passed;
full repository CI, Clippy with warnings denied, documentation/manifest
validation and desktop typecheck/build also passed.

## Remaining work

- add an Android touch-surface UI/host and feed its returned packets to a
  finite-deadline authenticated transport;
- complete peer pairing, identity and encryption before any production stream;
- integrate the Windows Runtime receiver process and reconnect/epoch control;
- build an APK and install it only after a separate explicit human approval;
- run end-to-end phone contact-to-Windows gesture evidence.
