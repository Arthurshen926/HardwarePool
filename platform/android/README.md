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

The Node Service also owns the local Source publication selection. Before any
future transport call, a pure contract projector revalidates the complete
selection against the current Android catalog, omits unselected Source-bearing
capabilities and retains Sink/Control-only inventory. Offline, detached,
missing and non-Source selections fail closed; a previously selected Source can
still be deselected after it goes Offline. This produces only a local catalog
snapshot and does not imply Session trust, Route authorization or delivery.
`NodeBinder.publicationCatalogJson()` now encodes that filtered snapshot into a
closed schema-v1, 64-KiB-bounded Rust handoff. It contains no recipient,
Session, endpoint or arbitrary Route mode. The native Runtime/JNI owner that
will consume the value and authenticated peer delivery are still pending.
ADR 1406 adds a separately build-verified native owner that constructs a real
Rust `NodeRuntime` from this JSON and permits same-Node replacement while
disconnected. Its Java wrapper is compile-checked but deliberately not yet
loaded by the Service or packaged into the unified APK.
ADR 1407 closes that packaging/lifecycle gap for version `0.5.3-dev`: Gradle
builds and packages the ARM64 Rust library, and the unified Service creates,
refreshes and closes its native catalog Runtime. Sharing intent commits only
after native projection accepts the candidate. This does not yet connect the
unified IMU/gamepad/touchpad controls to Windows Routes.

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

Version `0.5.2-dev` moves the bounded camera inventory into the common Vue UI.
After CAMERA consent and only while stopped, users can refresh directly openable
Camera2 IDs, choose automatic or reviewed vendor Zoom targets and select one of
three AVC quality presets. These are session-local Adapter settings, not sharing
consent or a Route.

Every future APK install/update still requires an explicitly selected ADB target
and the applicable human authorization.

The Android-free camera contract also encodes the private CAVC v1 config/access-
unit records defined in `docs/AVC_ADAPTER_WIRE.md`. The matched Rust decoder and
validation receiver live in `adapters/vcamdroid`. The trusted-LAN mode has an
exact Windows bind/phone allowlist and no DNS, discovery or wildcard listener,
but remains plaintext and is not production pairing or encryption.

## Native responsibilities

- runtime permission requests;
- foreground-service ownership of active audio and connected-device sessions;
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

The unified `0.5.1-dev` host also declares the explicitly approved normal
`VIBRATE` permission. Its service-owned haptics Sink has a generation-bound
0..1000 dual-rumble command, maps the stronger channel to the phone's single
actuator and caps each effect at 10 seconds. Starting the local shared-UI action
emits a short confirmation pulse; it does not claim remote feedback routing.

The unified `0.5.7-dev` host additionally declares
`FOREGROUND_SERVICE_CONNECTED_DEVICE` and the normal `CHANGE_NETWORK_STATE`
prerequisite under ADR 1421. IMU, gamepad, touchpad and haptics now enter the
non-exported Node service through closed started-service actions and keep a
visible foreground notification independently of Activity binding. The build
and hardware-free lifecycle policy pass; background/reopen/Stop behavior still
requires the separately approved physical gate on the current APK.

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
  -AdbSerial $env:CAPYIO_ADB_SERIAL
```

Set `CAPYIO_ADB_SERIAL` to the explicitly selected device serial or wireless-debug
endpoint from the current ADB device list before running this command.

The helper verifies the exact ADB endpoint, installed app version, loopback
listener, reverse mapping, established connection and top-resumed Activity. It
reuses an active session. If the receiver is absent, it fails closed unless the
caller explicitly adds `-StartReceiver`, which requests UAC for the separately
gated VHF wrapper. It never installs an APK or driver and never requests a
Windows restart.

## Hosted JNI and Java compatibility gate

The experimental Linux feature-branch workflow also cross-compiles the actual
ARM64 Android JNI library with Rust 1.97.1 and preinstalled NDK 29/API 29. It
compiles/runs the existing Java 17 dependency-free contracts and compares generated
AndroidNativeCatalogRuntime native names with exported library symbols. Missing
pinned tools fail without SDK installation or license acceptance. This is not an
APK build, Android-framework execution, deployment or physical audio test. See
[ADR 1525](../../docs/adr/1525-android-target-java-compile-gate.md) for prerequisites
and exact evidence limits.

### Audio selection boundary (ADR 1528)

Android discovers compatible advertised audio Ports and revalidates their exact
binding at Start. Compatible Linux playback Sinks appear with an unsupported
initiation explanation, not a Start action: Linux currently initiates playback
itself, and incoming Android-led Draft admission remains pending. This does not
add Linux reverse audio or a virtual microphone. Existing Windows admission,
independent consent and Stop behavior are retained. See
[ADR 1528](../../docs/adr/1528-android-audio-catalog-selection.md).

ADR 1529 adds an explicit incoming-mode Linux receiver contract. A peer that
advertises that implemented version can now be selected through its actual mono
Sink IDs; default Linux-led peers still show the unsupported-initiation reason.
JNI and hosted tests use the same `begin_audio_use` coordinator, which records
local intent and creates only a Pending Draft. Independent Linux approval and
finite reciprocal grants still precede leases and media. This is software
negotiation support, not a claim of incoming-mode physical audio acceptance.

## Secure camera bridge (hosted software acceptance, hardware pending)

The integrated camera path now has a separate, one-shot secure selection. The
Node Service uses four typed JNI calls (Start, State, Record, Stop); only a
Runtime handle, Route ID, generation handle and one bounded complete CAVC record
cross that boundary. Peer addresses, certificate pins and existing paired
credentials stay native. An authenticated secure offer never selects the legacy
`LoopbackAvcSender` or its Java Socket/reconnect behavior.

Start reserves one generation and returns Pending without network I/O. Native
preparation is cached by the authenticated control owner; current Source consent,
lease, Session, Route, Stream, epoch and CAVC format are checked again before
connect and after the exact pinned-mTLS binding acknowledgement. Camera2 may start
only after Ready and the existing camera permission/foreground checks. Ready is
not receiver delivery. Secure UI state never derives Active from capture,
encoding, local enqueue or TLS writes; an authenticated receiver Active status is
required. The present Linux secure receiver can remain Starting, so completed
receiver-delivery-to-Active reporting is still an explicit integration gap.

The initial Source lease is an immutable wall-clock and monotonic deadline, not
silently renewed. It is checked by native State and record reservation even if
Runtime is contended. Stop, control EOF, grant/lease loss, capability removal,
queue saturation, malformed records and generation replacement terminate owned
work. Completed worker reaping does not re-arm the Runtime's consumed Route
claim. A new explicit Route/consent/generation is required after termination;
this increment does not claim long-running renewable camera sessions.

The secure Java path reserves before copying codec output and bypasses both
legacy queues. Its two permits include callback copies, queued and in-flight
access units. The two native record permits include reservations, JNI copies,
queued and current worker records, each at most 4 MiB + 56 bytes. Parser copies
are separately bounded. The conservative application-owned payload live-set
budget is below 64 MiB for the single secure generation; this is not a total
process-memory bound and excludes uncollected JVM garbage and platform codec,
driver and OS buffers. Saturation fails closed rather than dropping config or
retrying an epoch. Socket work, joins and codec encoding do not run under the
Node Service/Runtime monitor; finished native workers are reaped outside those
locks, while snapshots and failure reconciliation use bounded/nonblocking paths.

Hardware-free evidence consists of the shared `SecureCameraCaptureSession`
fake-capture gate, real `AvcWireSessionEncoder` records sent through the exact
`SecureCameraTransport` JNI interface seam, native reservation/cancellation/
one-shot/expiry tests, actual pinned-TLS CameraJob handshake/Ready cancellation
races, valid CAVC exchange followed by real Source lease revocation, and the
existing authenticated synthetic graph. Revocation during an actually blocked
write syscall is not yet deterministically exercised. Run
`:app:secureCameraContractTest` and `cargo test -p capyio-android-jni`. These do not
claim physical Camera2 capture or actual JVM-to-Android-library invocation.
Hosted acceptance on 2026-10-21 passed for bridge commit
`54c026456375ed343767d6b465880db6106b0086`: all six jobs across three workflows
were green (acceptance run `39571074260`). The Android job compiled the actual
ARM64 Rust JNI shared library, checked generated Java native declarations against
its exports, and successfully ran `:app:compileDebugJavaWithJavac` against the
existing API 36 SDK. The Java compile excluded web-asset synchronization and the
redundant JNI build task; ARM64 compilation was checked separately. Existing
secure Linux synthetic graph regressions also passed.

This establishes production-source compilation and export compatibility, not
physical Camera2/MediaCodec encoder operation, an actual Android JVM calling the
shared library, APK installation, or an end-to-end physical camera session. The
one-shot initial lease and receiver Starting/Active reporting caveats above
remain, as does the unproven revoke-inside-blocked-write boundary. No permission,
foreground-service declaration, enrollment, credential creation or actual-device
recording is added. The legacy Windows plaintext lab path remains separate.

## Experimental secure mono microphone binding (ADR 1560)

The Android microphone settings now offer explicit
`secure_mono_tls13_v1` opt-in; `legacy` remains the default. This changes the
advertised format preference, not permission or consent. A new use still requires
existing microphone permission, visible foreground ownership, a compatible
selected peer Sink, authenticated control negotiation and a fresh finite Source
lease. The secure direction is `microphone-secure`; a secure/unknown DTO is never
passed to the legacy DatagramSocket adapter. Switching the profile stops and
invalidates existing microphone work before publishing the new catalog.

The secure capture path waits for Rust's authenticated media binding before
opening AudioRecord, requires actual mono 48 kHz PCM16, pins the initially routed
input, and copies only complete 480-sample records into a two-slot native queue.
Socket work stays on the native owner thread. Source streaming status requires a
successful authenticated media write, not merely capture/enqueue; it still does
not prove receiver playback. Stop, permission/routing loss, stale generation,
queue exhaustion and original grant/lease expiry terminate the use without UDP
fallback or automatic retry. Failed recorder cleanup blocks another capture in
that process rather than reporting a clean stop. Native TCP connect is bounded
but cannot be interrupted before the connected socket's Stop handle exists.

This is the Android binding slice only. The Linux production Node/console secure
playback and virtual-microphone launcher is still a separate integration step;
the existing nine secure tone graphs exercise reusable APIs, not that missing
user workflow. No physical phone capture or Android JVM-to-ARM64 invocation is
claimed by the fake AudioRecord, SDK compile or generated JNI export checks.
Run `:node-contract:secureMonoCaptureContractTest`,
`:app:secureAudioContractTest` and the mandatory hosted native job/TLS tests.
See [ADR 1560](../../docs/adr/1560-android-secure-microphone-binding.md) for limits.
