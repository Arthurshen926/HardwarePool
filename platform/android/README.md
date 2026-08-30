# CapyIO Android Node

`CAPY-AUDIO-NATIVE-001C` introduces the first CapyIO-owned Android application
shell. One platform-managed service owns an independent microphone Source and
speaker Sink. This is not yet the native network transport or a replacement for
the physically proven Audio Share/MicYou compatibility applications.

## Native responsibilities

- runtime permission requests;
- foreground-service ownership of active microphone/render sessions;
- persistent privacy notification;
- audio focus and route change handling;
- microphone capture and speaker render Adapter;
- lock-screen/background lifecycle;
- Kotlin ↔ Rust command/event boundary;
- real-device metrics and diagnostics.

The current 001C shell implements the permission/service and Java platform API
parts. One authorized Android 16 run qualifies positive permission, real audio
endpoint, independent lifecycle, Activity-finish and notification-stop
behavior. Rust/JNI, networking, pairing, codecs and broader device/lifecycle
qualification remain later slices.

## Explicit non-responsibilities

- Android does not register a system-wide virtual microphone or speaker for arbitrary Android apps in the MVP.
- Activity lifetime does not own an active audio session.
- Platform callbacks do not mutate Core state directly; they report completions/events to the Runtime.

## Current Audio Lab behavior

- `Microphone Source` requests `RECORD_AUDIO`, starts only from a visible user
  action, opens `AudioRecord`, reports actual parameters and discards every
  captured byte without storage, logging or networking.
- `Speaker Sink` opens an empty `AudioTrack`, reports actual parameters and
  waits for the future native backend. It does not synthesize or receive media.
- A persistent notification identifies the active capability or capabilities
  and has a `Stop all` action.
- Activity closure does not define Route lifetime; the non-exported service is
  the lifecycle owner and is `START_NOT_STICKY`.
- The two state machines have separate generations, failures and Stop/Retry
  behavior.

## Build and check

The reproducible wrapper pins Gradle 9.5.0 by SHA-256, Android Gradle Plugin
9.3.1, compile/target SDK 36, min SDK 26 and Java 17 bytecode. There are no
third-party Android runtime dependencies.

From the repository root:

```text
cargo xtask android-check
```

This runs 36 dependency-free lifecycle assertions, Android Lint with warnings
as errors and `assembleDebug`. The generated, ignored artifact is:

```text
platform/android/app/build/outputs/apk/debug/app-debug.apk
```

The command never runs ADB, installs an APK, grants a permission or starts a
device service. Those remain separately authorized physical-lab operations.

## Physical acceptance

Completed on one authorized Android 16/API 36 device:

1. actual microphone and speaker formats plus microphone frame progress;
2. concurrent activation and both independent Stop orders;
3. foreground type-mask transitions and Activity-finish survival;
4. persistent notification plus `全部停止` resource cleanup.

Still required:

1. permission denial/revoke and explicit microphone-indicator inspection;
2. lock-screen, process death, long background and vendor power behavior;
3. input/output route and audio-focus changes, underruns and power use;
4. repetition after native transport connection without retaining microphone
   payload in repository evidence.
