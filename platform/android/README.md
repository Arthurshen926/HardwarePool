# CapyIO Android Node

`CAPY-AUDIO-NATIVE-001C` introduced the first CapyIO-owned Android application
shell. One platform-managed service owns an independent microphone Source and
speaker Sink. `001D1/001D2` add the approved network permission, bounded UDP
codec, packetizer, queues, reassembler and one-shot sender/receiver workers.
ADRs 0046 and 0047 connect the Speaker Sink and microphone Source to that
media-worker composition. Exact-device native packet/render acceptance has
passed in both directions; subjective quality, lifecycle qualification and
safe compatibility rollback remain before the external applications can be
removed.

## Native responsibilities

- runtime permission requests;
- foreground-service ownership of active microphone/render sessions;
- persistent privacy notification;
- audio focus and route change handling;
- microphone capture and speaker render Adapter;
- lock-screen/background lifecycle;
- Kotlin ↔ Rust command/event boundary;
- real-device metrics and diagnostics.

The shell implements the permission/service and Java platform API parts. One
authorized Android 16 run qualifies positive permission, real audio endpoint,
independent lifecycle, Activity-finish and notification-stop behavior. The
native Java layer can encode/decode the same golden LAN datagram as Rust and
move exact PCM through bounded send/receive queues on explicit-peer UDP workers.
Rust/JNI, pairing, codecs and broader device/lifecycle qualification remain
later slices. Both direction-specific queue compositions are compile-,
contract- and controlled-device validated.

## Explicit non-responsibilities

- Android does not register a system-wide virtual microphone or speaker for arbitrary Android apps in the MVP.
- Activity lifetime does not own an active audio session.
- Platform callbacks do not mutate Core state directly; they report completions/events to the Runtime.

## Current Audio Lab behavior

- `Microphone Source` requests `RECORD_AUDIO`, starts only from a visible user
  action, opens `AudioRecord`, reports actual parameters and hands exact PCM to
  the bounded packetizer/send queue. A separate sender worker owns UDP; captured
  payload is not stored or logged. The default build has no peer and fails
  closed.
- `Speaker Sink` requires a build-time trusted-lab configuration, opens
  `AudioTrack`, starts the bounded native receiver and writes validated PCM on
  a separate non-blocking worker. The default build has no peer and fails
  closed rather than opening an empty Track.
- `Native LAN worker` has a 1,200-byte datagram bound, a fixed 104-byte header,
  one explicit peer and bounded deadlines. Packet queues cap count/aggregate
  bytes; one-shot workers stop within a bounded join. Each direction owns its
  worker composition, but no Android audio worker or Activity performs socket
  work.
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

This runs 36 dependency-free lifecycle assertions, 126 native-LAN packet,
queue, packetizer, reassembly, golden-wire and worker assertions, Android Lint
with warnings as errors, verifies the Activity and Service entrypoint classes,
and runs `assembleDebug`. The generated, ignored artifact is:

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

The current APK declares `INTERNET` under the owner's explicit 001D approval.
It still disables cleartext application traffic, adds no third-party runtime
library and has no automatic network start. Default builds have no peer; the
authorized controlled-lab build contains fixed speaker and microphone peers.
When a trusted-lab peer is compiled in, the speaker card shows aggregate UDP
receipt, wrong-peer, malformed-datagram, completed-packet,
reassembly-eviction and queue-drop counters without logging payloads. The
controlled 0.3.4-dev run accepted both the native tone baseline and ordinary
Windows application playback through the working CapyIO virtual endpoint.
The controlled 0.4.0-dev microphone run delivered 477 packets / 228,960 frames
to the Windows capture ring with zero observed wrong-peer, malformed or ring
drops and no MicYou process.
The accepted 0.4.1-dev build additionally showed microphone packets generated,
packets/datagrams sent, queue drops and partial buffered bytes. A simultaneous
microphone/speaker run observed 2,088 generated/sent packets and zero drops;
three ordinary Windows WAV captures were non-silent, including one after the
Broker Runtime released and reacquired both native UDP ports.
Version 0.4.2-dev coalesces worker-driven UI notifications to one refresh per
250 ms. It produced a fourth non-silent ordinary Windows recording and its
active Stop controls cleanly removed microphone then speaker foreground types.
Version 0.4.3-dev also maintains that bounded refresh while either capability
owns foreground lifecycle. A foreground-only speaker run displayed 1,000
received datagrams, 500 complete packets and 240,000 rendered frames without an
Activity background/foreground rebind, with every error/drop counter at zero.
