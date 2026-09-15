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

The Android host will provide the phone's microphone, speaker and explicitly
shared camera while reusing the shared Rust Core, Protocol and Runtime.

## Camera2 service and retained lab

The unified Node now compiles the proven `capyio-camera-app` Camera2/MediaCodec
service sources behind a narrow facade. The service is unexported, owns capture
independently from the Activity and is controlled from the shared UI after a
visible system permission flow. The original standalone lab remains available
for source/quality and end-to-end transport parity work. It declares
only `android.permission.CAMERA` and `android.permission.INTERNET`, requests CAMERA after a visible user
action, targets a visible preview plus an AVC encoder Surface and exposes only
bounded frame/access-unit counters. It stops whenever its Activity is not
visible and stores no image. The INTERNET permission is used only by the
foreground exporter. Blank endpoint input preserves the C4 device-loopback ADB
reverse tunnel; C23 also permits one explicit canonical private, link-local or
100.64.0.0/10 Windows IPv4 on fixed port 38173 for a trusted-LAN lab.

One explicitly authorized Android 16 device run accepted the two-Surface session
and produced 1280x720 AVC output. The unified default still uses the fixed
ADB-reverse development endpoint; it is not production discovery, pairing,
encryption or Route authorization.
Every future APK install/update still requires an explicitly selected ADB target
and the applicable human authorization.

The Android-free camera contract also encodes the private CAVC v1 config/access-
unit records defined in `docs/AVC_ADAPTER_WIRE.md`. The matched Rust decoder and
validation receiver live in `adapters/vcamdroid`. The trusted-LAN mode has an
exact Windows bind/phone allowlist and no DNS, discovery or wildcard listener,
but remains plaintext and is not production pairing or encryption.

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
9.3.1, compile/target SDK 36, min SDK 29 and Java 17 bytecode. There are no
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
