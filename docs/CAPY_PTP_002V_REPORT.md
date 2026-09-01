# CAPY-PTP-002V Report

Date: 2026-08-30

Status: live Android-to-Windows virtual touchpad path complete for one closed
single-finger lab gesture.

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-touchpad`

Branch: `codex/capyio-touchpad`

## Outcome

The installed `CapyIO Touchpad Lab` Activity now captures complete Android
`MotionEvent` pointer arrays, converts them through the Rust JNI record session,
and sends bounded Hello/Data records through an ADB reverse tunnel. The Windows
loopback receiver validates the full compiled binding, decodes the private
packet, submits it to a real `SyntheticTouchpadSession`, and returns an exact Ack
only after successful native processing.

An Android shell-generated 350 ms single-finger horizontal swipe completed:

```text
hello_binding=accepted
device_creation=created
lab_status=complete
frames_processed=44
batches_submitted=43
contact_records_submitted=43
device_cleanup=closed
```

This proves API acceptance and cleanup. It does not claim that an independent
observer saw a particular pointer displacement.

## Fail-closed evidence

The first live touch exposed mixed Android time sources: session start used
elapsed-realtime while `MotionEvent` used uptime. The mapper rejected the first
event as a timestamp regression before it reached Windows. The implementation
now names and uses `android.uptime-nanos` consistently.

A second run reached native submission from a sandboxed receiver, where Windows
returned error 5. The Sink attempted cancellation and exited. Repeating the same
explicitly gated receiver outside the sandbox completed all 44 frames.

## APK and device evidence

- Device: `V2419A`, API 36, ABI `arm64-v8a` at wireless ADB endpoint
  `100.66.157.119:40263`.
- Package: `dev.capyio.touchpad.lab`, version code 2.
- Permission: only `android.permission.INTERNET`; it is granted.
- Local and installed `base.apk` SHA-256:
  `91cb04e9e64f89f1e8b11951479e0fbb41007cd124053221a9fe2f147b4e4311`.
- APK contains `lib/arm64-v8a/libcapyio_android_jni.so`.
- Installation used the separately approved ADB package-manager path.
- The reverse mapping, Activity, uploaded `/data/local/tmp` APK and virtual
  device were removed/stopped; the installed application remains.

## Validation

```text
cargo test -p capyio-android-jni --offline
cargo test -p capyio-remote-touchpad-adapter --bin capyio-ptp-adb-lab --offline
cargo clippy -p capyio-android-jni --all-targets --offline -- -D warnings
cargo ndk -t arm64-v8a -o target/android-jni build -p capyio-android-jni --release --offline
platform/android/touchpad-bridge/gradlew.bat :lab-app:assembleDebug --offline --no-daemon
platform/android/touchpad-bridge/gradlew.bat :lab-app:lintDebug --offline --no-daemon
cargo xtask ci
cargo xtask validate-docs
cargo xtask validate-manifests
git diff --check
```

Targeted Rust/JNI, Windows listener, Android native, APK build and Android lint
passed. The full repository CI, documentation validation, manifest validation
and final diff check also passed.

## Remaining work

- replace ADB with production mutual authentication, encryption and peer binding;
- add reconnect/new-epoch ownership and background lifecycle;
- exercise manual two-, three- and four-finger gestures from the physical screen;
- integrate the lab Activity into the real Android Node host UX;
- qualify latency, coalescing, queue pressure and long-duration behavior;
- decide whether any unsupported Windows build requires the separate VHF driver
  fallback; no driver was installed in this slice.
