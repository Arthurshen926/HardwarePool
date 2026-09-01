# CapyIO Build Status

> Updated: 2026-08-31 after the `CAPY-AUDIO-NATIVE-001C` Android service-shell slice.

## Verified baseline

At commit `d33d585`, before migration:

- repository validation passed;
- Rust workspace check passed;
- 34 Rust tests passed;
- Vue typecheck/build passed;
- hosted Windows/Linux/macOS Rust CI and UI/static jobs had passed on prior
  bootstrap commits.

Exact evidence is in `docs/BASELINE_REPORT.md`.

## Foundation status

- Gate 0: complete.
- Gate 1: complete locally — names/docs/ADRs, dependency paths, Rust check and
  pinned pnpm UI build passed.
- Gate 2: implemented — symmetric Nodes, typed Port/Route Core, generic
  protocol/Runtime, four independent Routes and Quick Actions/Workspace UI.
- Gate 3: implemented — manifest/schema, bounded NDJSON codec, Sidecar Host,
  finite Mock Source/Sink and scoped crash isolation.

The original Gate 0–3 run passed 42 Rust tests, Clippy, manifest validation,
Adapter Smoke, pnpm typecheck/build, repository validation and `cargo xtask ci`.
That historical evidence is recorded in `docs/GATE_0_3_REPORT.md`.

## CAPY-FOUNDATION-002 hardening

The current working tree hardens Gate 3 without adding a real data plane or
hardware Adapter:

- stdout/stderr are bounded while reading, including newline-free overflow;
- terminal sequential-control failures poison the Host, close stdin and reap
  the Sidecar before later requests are rejected;
- generic Route prepare/start/stop/status contracts replace Mock-specific Host
  return types while the finite sample remains private to Mock code;
- manifest deployment validation covers InProcess, Sidecar, ExternalService and
  DriverBacked metadata without embedding install commands;
- catalog replacement invalidates only dependent Routes, emits a Problem and
  advances epoch on compatible recovery;
- Route backend support is checked against Adapter and interoperability modes;
- 84 PRD IDs have checked status/Gate/evidence traceability, with automated
  duplicate and malformed-ID rejection;
- pull-request workflows check out the exact PR head; Rust/Adapter gates target
  Windows, Linux and macOS, UI uses a frozen lockfile, and Windows has an
  additional Tauri check/build gate.

The full local matrix passed: repository self-tests/validation, Rust
format/check/Clippy, 70 workspace tests, docs, two manifests, Adapter Smoke,
`cargo xtask ci`, frozen pnpm install/typecheck/build and Windows Tauri
check/build. Exact command and test-count evidence is retained in
`docs/FOUNDATION_HARDENING_REPORT.md`.

PR #10 passed repository, UI, Windows/Linux/macOS Rust/Adapter and Windows
Tauri exact-head checks before merge commit `5f5b81f`. Linux/macOS native Tauri
packaging remains an explicit skip; their Rust/Adapter jobs did pass.

## CAPY-IMU-001A fixture-first path

The active branch adds a real in-process semantic data path, not a live phone
transport:

- bounded Profile/stream/epoch/sequence/timestamp envelopes;
- explicit gap, duplicate, late, wrong-stream, stale/future-epoch and full
  outcomes;
- independent per-consumer queues and lifecycle for Panel and Recorder;
- deterministic `capyio.motion.imu-samples/1` JSONL fixture with SI units,
  coordinate frame, accuracy, calibration and sensor metadata;
- headless replay plus shared Browser/Tauri numeric Panel and Recorder summary;
- safe Android doctor/baseline/collect commands requiring an explicit serial.

Full workspace format/check/Clippy, 79 Rust tests, repository validation,
headless replay, `cargo xtask ci`, frozen frontend install/typecheck/build and
Windows Tauri check/build passed. A real read-only vivo inventory also passed;
the ignored artifact is sanitized and explicitly says no APK, permission change
or live CapyIO stream. Exact evidence is in `docs/CAPY_IMU_001A_REPORT.md`.

## CAPY-IMU-001B0 SensorServer contract

The active slice has started the first real-Adapter boundary without opening a
network connection or claiming phone data:

- upstream SensorServer commit and GPL-3.0-only external-service provenance are
  recorded with no source or binary import;
- its documented JSON message is bounded and strictly parsed;
- asynchronous accelerometer/gyroscope readings pair deterministically with
  explicit skew, replacement, regression and sequence-exhaustion behavior;
- original component timestamps survive in the IMU Profile.

Full workspace format/check/Clippy, 88 Rust tests, repository validation,
manifests, Adapter Smoke, fixture replay, frontend typecheck/build and
`cargo xtask ci` passed. Exact evidence is in
`docs/CAPY_IMU_001B0_REPORT.md`.

## CAPY-IMU-001B1 bounded WebSocket client

The active slice adds the first concrete local-lab transport inside the
SensorServer Adapter:

- user-approved Tungstenite 0.30.0, handshake-only, with no Tokio or TLS stack;
- IP-literal endpoint and fixed Android sensor paths;
- TCP connect/read/write deadlines and 4 KiB frame/message bounds;
- typed text/control/close/timeout/error outcomes;
- loopback mock-server tests for success and abnormal frames.

Complete locally. Seven WebSocket loopback tests cover the fixed endpoint,
valid text, ping/pong, close, timeout, malformed/binary/oversized messages and
oversized handshake behavior. Together with the nine parser/pairing tests, the
SensorServer Adapter has 16 contract tests. Full workspace format/check/Clippy,
95 Rust tests, repository validation, manifests, Adapter Smoke, fixture replay,
frontend typecheck/build and `cargo xtask ci` passed. Exact evidence is in
`docs/CAPY_IMU_001B1_REPORT.md`.

## CAPY-IMU-001B2 physical SensorServer lab

Complete on the authorized Android/Windows lab pair. The official SensorServer
v7.2.1 APK matched its published SHA-256 and fixed upstream revision before
installation. The live command produced paired accelerometer/gyroscope
StandardPort envelopes, delivered the same eight-sample run to an independent
numeric Panel and JSONL Recorder with zero missing sequences, then repeated the
run without restarting the service. A physical service stop closed the active
client explicitly with code 4004. Exact bounded evidence and remaining limits
are in `docs/CAPY_IMU_001B2_REPORT.md`. Full workspace format/check/Clippy, 96
Rust tests and the remaining `cargo xtask ci` gates passed.

## CAPY-IMU-001B3A Windows Tauri physical panel

Complete as a bounded desktop-lab slice. Narrow Tauri commands now start, read
and stop a host-owned SensorServer worker without exposing arbitrary networking
to the WebView. The panel renders live acceleration and angular velocity plus
epoch, sequence, clock and sample count, and visibly distinguishes idle,
connecting, active, failed and stopped states.

An authorized physical UI run first surfaced a real handshake timeout, then
recovered after the phone service restarted, grew beyond 100 samples and
stopped explicitly while retaining a 226-sample final snapshot. The physical
endpoint is not committed. Exact evidence and the production-Runtime boundary
are in `docs/CAPY_IMU_001B3A_REPORT.md`.

## CAPY-IMU-001B3B Runtime-owned physical IMU Route

Complete as a local physical-lab slice. The SensorServer ExternalService
Adapter, Android IMU Source, Windows Panel Sink and `ExternalProtocol` Route are
registered in the desktop Node's single `NodeRuntime`. Staged commands expose
Draft/Prepared/Starting/Active/Offline/Stopped, retain a retryable disconnect
Problem and advance the epoch before retry.

Loopback success and failure tests cover activation, disconnect, retry and
thread shutdown. The ignored physical test passed against the authorized phone
after a stale SensorServer listener was restarted. Exact evidence and limits
are in `docs/CAPY_IMU_001B3B_REPORT.md`.

## Not built or tested

- platform-audio integration or physical cross-device sound through the
  CapyIO-native LAN backend;
- Android permission denial/revoke, indicator, lock/process-death,
  long-background, focus and route-change behavior;
- CapyIO-owned camera or input data path;
- release-qualified Windows virtual devices, signed installer or stable
  isolated-VM driver target;
- production transport, pairing, encryption, Runtime-owned live audio Adapter
  transport or performance.

The physical SensorServer lab transport is owned by the desktop process's real
Node Runtime Route, but it is not production transport or a long-lived headless
Node service.

These absences are expected and must not be reported as working functionality.

## CAPY-AUDIO-000/001A0 Audio Share boundary

The pinned, unmodified Audio Share v0.3.4 release was verified against official
SHA-256 files and Apache-2.0 provenance. On the separately authorized
Windows/Android lab, the upstream system-loopback server negotiated float
stereo PCM, delivered UDP payloads, maintained heartbeats, closed cleanly and
started a second peer. Android `AudioFlinger` reported a matching app-owned
`AudioTrack` with frames written. No person was beside the phone, so subjective
audibility/quality is not claimed.

The repository contains only CapyIO-authored configuration, probe, supervision
and Runtime-composition code:
explicit IP/port/endpoint/PCM arguments, direct no-shell execution, bounded
stdout/stderr/deadlines and strict v0.3.4 endpoint parsing. No upstream binary
or APK is committed or distributed. Generic Quick Action exposure and renewed
authorized physical playback/disconnect evidence remain active work.

`CAPY-AUDIO-001A1` additionally implements direct child startup, explicit TCP
listener readiness, bounded output draining, typed early exit/startup timeout,
polling, idempotent stop and synchronous process reaping. Three fixture-backed
supervisor tests pass. An ignored real-CLI run also started the verified release
on loopback, observed it running, stopped it and confirmed zero remaining
listeners/processes. This is server-process evidence only: upstream v0.3.4 has
no machine-readable Android peer state, so this slice did not infer receiver
loss and human logs are not used as lifecycle authority. The later `001A2A/B`
slices add owner-scoped transport observation and Runtime mapping.

`CAPY-AUDIO-001A2A` adds a Windows-only, bounded `GetExtendedTcpTable` query.
The test proves readiness self-connect is excluded, an established process-owned
peer is detected, disconnect clears presence and stopped supervision reports
not running. The result contains a count only and is deliberately weaker than
protocol negotiation, UDP delivery or audible playback.

`CAPY-AUDIO-001A2B` binds the supervised process boundary to one real
Runtime-owned `AdapterManaged` Route. Three consecutive established receiver
samples are required for activation; an absence resets the starting counter,
and loss after activation, process exit or process-start failure creates a
stable Route-related Problem and `Offline`. Explicit retry advances the Route
epoch, explicit stop terminates the Route/process boundary, and tests prove an
active IMU Route is unchanged by audio failure. The controller is not yet wired
to a desktop Quick Action, and TCP presence remains weaker than playback
health.

`CAPY-AUDIO-001A3` adds a generic schema-v1 Quick Action for the physical
speaker Route. It exposes only lifecycle/evidence state and finite
start/retry/stop operations; executable path, bind address, port and endpoint ID
remain host environment configuration. A Tauri-owned 250 ms worker performs
receiver polling independently of WebView refresh. Browser Mock uses the same
DTO and remains visibly blocked/simulated. The authorized physical lab passed
Active epoch 1, disconnect/Offline epoch 2, retry Active epoch 3 and explicit
stop with no remaining Windows process/listener. A Windows system WAV produced
non-zero written frames on the app-owned 48 kHz stereo Android Track. A later
authorized repeat advanced that Track from zero to 2,421,542 server frames, and
the user beside the receiver clearly heard phone playback. This confirms one
audible case only; background/focus, latency and long-duration quality remain
unverified. The Quick Action labels this initial path as system-audio mirroring,
not a CapyIO virtual speaker, because the selected Windows endpoint may continue
to play locally.

Post-`001A3` hardening bounds receiver startup at 120 host polls. Exhaustion
reaps the external process, retains a retryable typed Problem and permits a
later-epoch retry instead of leaving the Route in `Starting`. A Windows Remote
Desktop physical repeat also demonstrated that v0.3.4 may fall back from the
requested signed-16/48 kHz format to an endpoint's 44.1 kHz float default. The
Route now truthfully declares `audio-share-v0.3.4-private-negotiated`. During a
15-second phone screen-off interval, TCP stayed established and the Android
Track advanced by 757,150 frames; secure-lock, longer background, focus,
latency and soak behavior remain unverified.

Audio Share start now re-probes the current endpoint inventory. Endpoint drift
caused by RDP, hot-plug or audio-service re-enumeration produces the sanitized,
retryable `CAPY.AUDIO_SHARE.ENDPOINT_UNAVAILABLE` Route Problem rather than a
generic process-start failure. The raw endpoint ID remains trusted host
configuration and is not returned to the WebView.

The hash-verified real CLI passed the current-endpoint start/listen/stop probe
and rejected an explicitly stale former endpoint before spawn, leaving the
supervisor stopped.

`CAPY-AUDIO-001A4` adds trusted-host playback-endpoint reselection to the
desktop Quick Action. The UI receives bounded names and per-scan opaque tokens;
the Rust host resolves only its current token allow-list to raw IDs and rejects
selection while the Route is active. Refresh and successful selection
invalidate tokens. Selection intentionally lasts only for the current desktop
run. Desktop unit tests cover token/request/name bounds and inactive/active
process replacement; the hash-verified CLI real probe passed against the
current Windows inventory.

## CAPY-AUDIO-001B0 dedicated virtual-speaker start

The product target now explicitly includes an independent Windows `CapyIO
Speaker` render endpoint. ADR 0027 splits the proven mirror transport (7A) from
the driver-backed projection (7B). Fixed-revision source review showed that
SysVAD WASAPI loopback is synthetic, so ADR 0028 replaces that data path with a
minimal WaveRT endpoint, endpoint-associated render APO, bounded staging ring
and user-mode Broker. Networking remains outside both the driver and APO.

Microsoft Windows-driver-samples is pinned at revision
`717778a20ba4dd2440fe609f69153a1f8a64f597`; its repository license is MS-PL.
No upstream source has been imported. The identified local host is
`DESKTOP-AT8EVE9`, AMD64 Windows build 26200.9168, with Visual Studio Build
Tools 17.14, Windows SDK 10.0.26100.0 and WDK 10.0.26100.6584. WDK MSBuild and
x64 InfVerif execute locally. The pinned SysVAD `EndpointsCommon` target and a
v142/WIL x64 Release `SwapAPO.dll` compile baseline succeeded. No driver
installation or signing action has been performed.
The refreshed `arthu` token now includes `Hyper-V Administrators`. An exact
Generation 2 target named `CapyIO-DriverLab` has been created on `F:` with
Secure Boot, vTPM, 8 vCPU, 4–16 GiB dynamic memory and a 96 GiB dynamic disk.
Its Windows 11 Enterprise 25H2 Evaluation ZH-CN installer hash matches
Microsoft's published value. Guest installation reached OOBE but did not
produce a stable recoverable target. ADR 0029 permits a controlled local-host
Gate 7B exception after elevated recovery, exact-package and rollback preflight.
ADR 0030 now resolves Broker-to-Android PCM ingest without the external
`as-cmd` capture process. A bounded simulated producer sent 1,920,000 bytes to
the pinned Android receiver over Tailscale with zero queue-full, receiver-gap
or UDP-send errors; Android reported a started stereo 48 kHz `AudioTrack`.
Human-confirmed audibility and the real APO staging producer remain pending.

## CAPY-AUDIO-001B6B ordinary-user service control

The controlled local lab now has an Automatic LocalSystem `CapyIOBroker`
service and an ACL-protected local named pipe defined by ADR 0034. Closed 4 KiB
schema-v1 requests expose only `status`, `start` and `stop`; trusted executable
and network configuration remain outside the WebView contract. Repeated
ordinary-user requests survived stop/start generations, stop released TCP/UDP
65530 without stopping the SCM host, and the ignored physical desktop test
proved Quick Action service selection plus UI-shutdown independence. The
Android receiver re-established an owned TCP connection and a five-second
`CapyIO Speaker` submission left service state active. Full evidence and exact
hashes are in `docs/CAPY_AUDIO_001B6B_REPORT.md`; signed installer, reboot/soak,
multi-user policy and production peer security remain open.

The Speaker functional Gate is therefore closed at the controlled-lab evidence
level. Remaining distribution, qualification and production-security work is
tracked without ambiguity in
`docs/plans/active/0013-speaker-release-qualification.md`.

## CAPY-MIC-001H trusted Quick Action physical acceptance

The pinned MicYou v2.0.1 external-process path now reaches the independent
installed `CapyIO Microphone` through the desktop Runtime's trusted-configured
Quick Action. Stable process-owned phone TCP presence activates only the
microphone Route; an ordinary Windows CPAL client received non-zero 48 kHz mono
float32 PCM both before and after explicit retry. Active phone loss stops and
reaps the receiver, reports the Route `Offline` and reaches exact digital
silence after the bounded committed-frame drain. Stop remains terminal.

An eight-second ordinary-client WAV contained non-zero samples in every
one-second interval, and the project owner listened to it on 2026-08-29 and
confirmed audible phone microphone audio. Raw microphone media remains ignored
local evidence and is not committed or emitted through CI logs.

The controlled-lab CLI carries reviewed stable-endpoint, exact-silence,
per-session UDP-metric and Windows PID-plus-creation-time mode-lock patches. It
is built from GPL-3.0-only source outside this repository and is not distributed
by CapyIO. Local firewall access is limited to that exact executable, the
Tailscale IPv4 range and TCP 8554/UDP 8555. Full evidence and current limits are
in `docs/CAPY_MIC_001H_REPORT.md`.

The Gate 8 functional slice is closed at the controlled-lab evidence level.
Android lock/background and permission-revocation tests, reboot survival,
performance/soak, production pairing/security, headless ownership and GPL
distribution decisions remain in the release-qualification plan rather than
being implied by the functional claim.

PR #14 provided independent exact-head evidence: Ubuntu, macOS and Windows Rust,
static repository validation, shared UI and Windows Tauri all passed on head
`f513859`. It merged to `main` as merge commit `2145f9f`. Functional plans
0016–0020 are archived under `docs/plans/completed/`; release work remains in
`docs/plans/active/0021-microphone-release-qualification.md`.

## CAPY-AUDIO-NATIVE-001A backend-neutral media seam

The first CapyIO-native audio-consolidation slice is complete locally. ADR 0041
and `capyio-audio` now bind one typed Session, directed Route, Stream, positive
epoch and exact selected `AudioStreamSpec` before a platform engine enters any
concrete transport. The same direction-neutral packet represents exact PCM or
bounded opaque encoded payload, and the reference worker queue has independent
packet-count and aggregate-byte limits.

Twenty-four `capyio-audio` tests cover lossless PCM frame/packet conversion,
encoded-payload bounds without a codec, nil/epoch/spec/timeline rejection,
wrong-Stream and wrong-epoch drops, invalid payloads, both queue capacity modes
and opposite microphone/Speaker Routes sharing one Session without sharing
state. Full `cargo xtask ci` passed format, workspace check, Clippy with warnings
denied, 165 Rust tests (4 separately ignored physical/external tests), IMU demo,
88-ID documentation/structure validation, two manifests, Adapter smoke and
desktop typecheck/build.

This is an in-process media seam, not a public network byte layout. It does not
add a socket, codec, AOO dependency, Android service, permission or APK, and it
does not change either proven Audio Share or MicYou wire. Compatibility-backend
mapping is the next slice; CapyIO Android capture/playback and native transport
remain later separately tested work under
`docs/plans/active/0022-native-audio-subsystem.md`.

## CAPY-AUDIO-NATIVE-001B compatibility backends

The compatibility paths now sit behind a machine-checkable common backend
contract rather than merely sharing audio terminology. The contract records
media visibility, PCM/Opus support, field-by-field metadata fidelity and
observable transport security, and rejects a partial/opaque backend that claims
StandardPort interoperability.

Audio Share now has an executable common-media wrapper: one bound PCM packet is
validated against Session/Route/Stream/epoch/spec, then only its payload enters
the unchanged Audio Share queue and TCP/UDP wire. Its contract truthfully marks
the remaining identity/timing metadata absent, the format mapping partial and
all production security false. The loopback test proves exact PCM payload and
private datagram segmentation through this wrapper.

MicYou is deliberately not given a fake packet API. Its Adapter can associate
the external process with one conservative voice Route/epoch for lifecycle, but
declares media opaque, private PCM/Opus negotiation partial and production
security absent. Replacing that boundary requires native Android capture and a
native transport rather than another wrapper.

Full `cargo xtask ci` passed format, workspace check, Clippy with warnings
denied, 173 Rust tests (4 separately ignored physical/external tests), IMU demo,
88-ID docs/structure validation, two manifests, Adapter smoke and desktop
typecheck/build. No APK, phone, driver, service or installed virtual endpoint was
changed or exercised in this slice. Evidence is in
`docs/CAPY_AUDIO_NATIVE_001B_REPORT.md`.

## CAPY-AUDIO-NATIVE-001C Android audio Node shell

The first CapyIO-owned Android application now builds locally as
`dev.capyio.android`. ADR 0043 gives one app-private Node two independent Ports:
an Android microphone Source and speaker Sink under
`capyio.audio.frames/1`. A non-exported `START_NOT_STICKY` foreground service
owns both resources while the Activity observes a schema-v1 payload-free
snapshot.

The microphone path requests recording/notification consent, opens a real
48 kHz mono PCM16 `AudioRecord`, records the actual granted parameters and
counts frames on one bounded/preallocated worker before discarding bytes. The
speaker path opens a real requested 48 kHz stereo PCM16 `AudioTrack` and waits
empty for the future backend. Each direction has independent generation,
state, actual format, metrics and sanitized Problem code; stale completions and
one-direction failures cannot mutate the other slot.

The 001C manifest declared the approved recording and dual foreground-service
permissions, required a persistent notification, exported no service, disabled
backup/cleartext and deliberately omitted Internet permission. ADR 0044's
subsequent 001D1 slice adds the separately approved Internet permission while
retaining those restrictions. The APK has no third-party runtime dependency.
Gradle 9.5.0 is wrapper/checksum pinned with AGP 9.3.1 and SDK 36.

`cargo xtask android-check` passed 36 lifecycle/validation assertions, Android
Java/resource/manifest compilation, Lint with warnings denied and debug APK
assembly. APK manifest analysis confirmed application ID, SDK bounds, five
permission names and foreground type mask. `cargo test -p xtask` and the
88-ID offline repository validator also passed. Full `cargo xtask ci` passed
173 Rust tests with 4 explicit ignores plus every existing Core, Adapter,
documentation and frontend Gate.

The checksum-identified debug APK was subsequently installed under explicit
authorization on one Android 16/API 36 vivo V2419A. System permission UI,
48 kHz mono PCM16 `AudioRecord`, 48 kHz stereo PCM16 `AudioTrack`, simultaneous
activation, both independent Stop orders, foreground type-mask transitions,
Activity-finish survival and notification `全部停止` cleanup passed. Android
AudioService recorded capture start/stop and player start/release, with no
remaining CapyIO service or running recording AppOp after cleanup. This is one
device's platform-endpoint/lifecycle evidence, not remote sound, network,
focus/routing, lock, revoke, latency, quality or vendor-power qualification.
Evidence and remaining risks are in
`docs/CAPY_AUDIO_NATIVE_001C_REPORT.md`.

## CAPY-AUDIO-NATIVE-001D1 bounded LAN reference backend

ADR 0044 now gives the common audio packet its first CapyIO-owned executable
network reference. `capyio-native-audio-lan` moves either Route direction over
an explicit-peer UDP socket with a fixed version-1 header, 1,200-byte datagram
limit, at most 64 fragments/70,144 packet bytes and at most eight incomplete
packets. Session, Route, Stream, epoch and every packet timeline field remain
exact. Malformed, conflicting, wrong-binding and wrong-peer input is rejected
or reported through bounded counters.

The Android tree contains a dependency-free Java codec and UDP worker endpoint
with the same bounds. A committed 120-byte golden datagram is asserted by both
Rust and Java, including complete unsigned 64-bit counter preservation. The
manifest contains the approved `INTERNET` permission, but platform audio classes
still contain no network API: microphone bytes remain discarded and speaker
render remains empty until `001E/001F` connect the now-bounded 001D2 workers.

The backend contract declares authentication, confidentiality, integrity,
replay defense and downgrade binding false. It is an `AdapterManaged` trusted-
lab reference, not production transport or a public StandardPort. No AOO,
SonoBus, codec or other runtime dependency was added. Local Rust tests cover
codec, fragmentation/reassembly, duplicate/eviction behavior, malformed input,
UDP loopback, spoofed peer and timeout. Android contract/Lint/APK and full
workspace Gate results are recorded in
`docs/CAPY_AUDIO_NATIVE_001D1_REPORT.md`.

The final local `cargo xtask ci` passed 181 Rust tests with 4 explicit
external/physical ignores, plus format, workspace check, warnings-as-errors
Clippy, IMU demo, documentation/manifests, Adapter smoke, repository validation
and desktop typecheck/build. `cargo xtask android-check` separately passed 36
audio lifecycle assertions, 35 native-LAN assertions, Lint and debug APK
assembly without ADB or installation.

## CAPY-AUDIO-NATIVE-001D2 Android bounded media workers

ADR 0045 completes the hardware-free Android composition around the 001D1
wire. `NativeLanPcmPacketizer` converts bounded frame-aligned PCM reads into
exact packets. `NativeLanPacketQueue` caps both packet count and aggregate
bytes, never blocks a producer and reports packet pressure, byte pressure and
wrong binding independently. Dropped packets advance sequence/sample/time and
mark the next accepted packet discontinuous.

The Android reassembler now mirrors the Rust 1–8 partial-packet bound. One-shot
sender/receiver threads own endpoint I/O, validate matching Route bindings
before start, expose stable sanitized I/O codes, and stop by closing/interruption
plus a two-second join ceiling. A local test moves two 48 kHz stereo packets
through packetizer, send queue, UDP fragmentation, receiver/reassembly and sink
queue with exact bytes and timeline, then proves terminal stop.

`cargo xtask android-check` passes 36 lifecycle assertions, 85 native-LAN
assertions, Java/application compilation, warnings-as-errors Lint and debug APK
assembly. No APK/ADB/device/driver/service operation occurred. Existing Android
platform Adapters remain deliberately disconnected, so this is not audible or
cross-device evidence. Details are in
`docs/CAPY_AUDIO_NATIVE_001D2_REPORT.md`.

The final local `cargo xtask ci` also passes format, workspace check, strict
Clippy, 181 Rust tests with 4 explicit external/physical ignores, IMU demo,
documentation/manifests, Adapter smoke, repository structural validation for
88 traced Requirement IDs, and desktop typecheck/build.

## CAPY-AUDIO-NATIVE-001E native speaker implementation

ADR 0046 connects the Android Speaker Sink to the 001D2 receiver queue through
`NativeLanPcmSinkWorker` and `AudioTrack.WRITE_NON_BLOCKING`. The worker checks
PCM geometry, handles partial writes, resets on sequence/sample discontinuity
and stops within two seconds. A closed build-time configuration supplies the
trusted-lab peer and Route binding without exposing authority through the
Activity; the default build disables this path.

The protocol-neutral Windows render-ring consumer now lives in
`capyio-windows-render-ring`. Both the Audio Share rollback Broker and the new
`capyio-native-virtual-speaker` reuse it. A separate native 440 Hz sender
isolates network/Android acceptance before ordinary Windows playback.

Hardware-free evidence passes 36 Android lifecycle assertions, 117 native-LAN
assertions, Android compile/Lint/debug APK, 12 native-LAN Rust tests, render-
ring and Audio Share regressions, and strict affected-crate Clippy. Physical
work progressed from the failed 0.3.0-dev source-set package through bounded
queue diagnostics to accepted 0.3.4-dev. The final native tone baseline
received 1,000 datagrams, completed 500 packets, rendered exactly 240,000
frames and recorded zero transport/reassembly/queue drops. An ordinary Windows
`SoundPlayer` playback through the working `CapyIO Speaker with Render Bridge`
instance then raised Android to 512,160 rendered frames / 1,067 packets with
all error counters still zero. A stale identically named Windows endpoint was
also found. A phone Stop/Start reset every counter and the next ordinary WAV
rendered 272,640 frames / 568 packets with zero errors or drops, proving a fresh
service generation. Endpoint cleanup and stable instance handling remain
release work.

The native Broker is also integrated into the existing `CapyIOBroker` service
and its ACL-protected local control pipe. Service-owned start reached `active`
while honestly retaining `receiverPresent=false`; an ordinary WAV advanced the
phone to 296,640 frames / 618 packets without drops. A full Windows service
stop released UDP 46001, restart reacquired it, and a second ordinary WAV
advanced the phone to 565,920 frames / 1,179 packets with all transport,
reassembly and queue error counters still zero. The final clean Release
deployment then delivered an exact additional 24,000 frames / 50 packets /
100 datagrams, reaching 589,920 frames / 1,229 packets with no errors or drops.
Abrupt-termination and signed
installer-upgrade handling remain release qualification. Details are in
`docs/CAPY_AUDIO_NATIVE_001E_REPORT.md`.

The final local `cargo xtask ci` passes format, workspace check, strict Clippy,
185 Rust tests with 4 explicit external/physical ignores, IMU demo,
documentation/manifests, Adapter smoke, repository validation for 88 traced
Requirement IDs, and desktop typecheck/build.

## CAPY-AUDIO-NATIVE-001F1 native microphone implementation

ADR 0047 connects Android `AudioRecord` through the existing exact PCM
packetizer, bounded queue and native LAN sender worker. The default build still
has no peer authority; the controlled 0.4.0-dev build uses a fixed microphone
Route and dedicated UDP 46010/46011 endpoints. The audio worker does not perform
socket I/O and captured payload is neither stored nor logged.

The fixed Windows capture ABI now lives in `capyio-windows-capture-ring` with a
live-owner mutex, strict layout/generation validation, S16LE-mono conversion and
whole-block pressure behavior. `capyio-native-virtual-microphone` receives the
exact common packet from one explicit peer and is supervised as the optional
second child of native `CapyIOBroker`. Its configuration is all-or-none;
microphone startup failure rolls the speaker child back and health requires
both children. Native and MicYou capture producers remain mutually exclusive.

Hardware-free evidence passes 4 capture-ring tests, 17 native-LAN tests, 17
Windows service tests, strict affected-crate Clippy, 36 Android lifecycle
assertions, 126 Android native-LAN assertions, Android compile/Lint and debug
APK assembly. A controlled no-MicYou run received 477 packets and committed
228,960 real microphone frames into the capture ring with zero observed
wrong-peer, malformed, full-ring or frame drops. The matching service Release
build is ready, but its administrator-token deployment and ordinary Windows
WAV capture remain 001F2 acceptance rather than a completed claim. Details are
in `docs/CAPY_AUDIO_NATIVE_001F1_REPORT.md`.

## CAPY-AUDIO-NATIVE-001F2 native microphone physical acceptance

The exact combined Release was deployed behind automatic LocalSystem
`CapyIOBroker`. Both native children reached `active`; UDP 46001 and 46011 were
owned by their separate Session 0 processes. An ordinary Windows WASAPI client
recorded three eight-second WAVs containing respectively 374,080, 372,649 and
378,387 non-zero samples out of 384,000, with non-zero RMS/peak and no client
errors. The second recording followed a full Runtime Stop/Start: both ports were
absent while stopped and reacquired by new processes in generation 2.

Android 0.4.1-dev first corrected the stale microphone-discard copy and exposed
bounded sender metrics. With both capabilities active (`0x82`), its panel observed
1,002,240 microphone frames, 2,088 packets generated/sent, 2,088 datagrams,
zero queue drops and zero buffered bytes. Final 0.4.2-dev coalesces UI refreshes
to 250 ms, has SHA-256
`79DE1A0BA7C6CDDE08D33A3C15D2C971027B7EC181EC6034DC7F6B83B5A68578`
and produced another non-silent WAV with 382,496/384,000 non-zero samples.
Exact active-button taps stopped microphone then speaker and removed foreground
ownership. Android 36 lifecycle assertions, 126 native-LAN assertions, compile,
strict Lint and APK assembly pass. `CAPY-AUDIO-NATIVE-001F` is therefore functionally
accepted for the controlled lab; details and retained release non-scope are in
`docs/CAPY_AUDIO_NATIVE_001F2_REPORT.md`.

## CAPY-AUDIO-NATIVE-001G1 partial-failure isolation

ADR 0048 corrects the first dual-Route orchestration defect: after successful
transactional startup, native pair liveness now means either child remains
running, while readiness still requires both. A single speaker or microphone
child exit therefore returns the Broker from `active` to `starting` without
stopping the opposite healthy Route. Loss of both remains a terminal bounded
failure. Two focused regressions join the existing startup-rollback test; the
Windows service crate now passes 19 tests. Physical child termination,
simultaneous media, bounded per-direction retry and direction-specific Desktop
diagnostics remain 001G work. See `docs/CAPY_AUDIO_NATIVE_001G1_REPORT.md`.

The exact 001G1 Broker was subsequently deployed with SHA-256
`C8A917F92134AF2F4DD3266FA319F746B61C50A909027B5FFD987A47870055BE`.
Symmetric controlled process-exit tests passed: killing the speaker released
only 46001 while microphone PID 78996/46011 survived, and killing the
microphone released only 46011 while speaker PID 83900/46001 survived. Both
cases moved the same generation from `active` to `starting` without a terminal
Problem; explicit Stop/Start restored a complete `active` generation 5.

Full concurrent-media acceptance remains open, but the two initial diagnostics
are now resolved at source. Android 0.4.3-dev proved the independent speaker
sender actually delivered 1,000 datagrams / 500 packets / 240,000 rendered
frames; the earlier zero was a foreground metrics-refresh defect. Windows
endpoint properties prove the second same-name render device is the retired
MicYou `WaveMicIngress`. Its presence also makes APO role selection order-
dependent and can route the microphone graph through the render lock path,
explaining the reproduced WASAPI `0x80070057`. ADR 0049 removes that endpoint
and producer role. The signed 21.83 package is installed and exposes one active
speaker plus one active microphone. Interactive-session acceptance opened the
CapyIO microphone for 99 callbacks / 47,520 samples and measured live Android
media at RMS `0.00795198`, peak `0.13494873`. A 21.84 float-pin experiment was
rejected after making the capture endpoint `NOTPRESENT`; rollback restored
21.83 without reboot. A single retained simultaneous-media evidence bundle
remains pending.

The subsequent UU Remote compatibility investigation ran the Android service
in speaker-only mode: microphone state remained `STOPPED` with zero frames and
packets, while eight seconds of Windows playback delivered 384,000 speaker
frames through 800 packets / 1,600 datagrams with zero wrong-source, malformed,
reassembly-eviction or queue-drop counters. The high-frequency remote sound is
therefore not full-duplex feedback or native-LAN corruption. Source review
confirms that the inherited SysVAD loopback capture stream generates a sine
tone. ADR 0051 introduces a build-only 21.85 candidate that rejects this
synthetic loopback while preserving offload render and the APO ring. Deployment
and controlled-host regression remain pending explicit package approval.

That approval was subsequently granted. Signed 21.85 installed without reboot
as `oem179/oem180/oem181`, retaining signed 21.83 as
`oem176/oem177/oem178`. One render and one capture endpoint enumerate `OK`,
and the three Windows audio/Broker services are Running. Direct WASAPI
loopback initialization now fails closed with `0x88890008`. Ordinary Windows
playback added about 240,480 Android frames in five seconds with all reported
transport error/drop counters at zero. A normal Windows capture client saw
non-zero Android microphone samples in all 100 ms windows, including while the
speaker test ran concurrently. Both directions stopped cleanly after the run.
The owner subsequently confirmed normal Android playback through UU Remote and
no recurrence of the high-frequency squeal.

The apparent post-stop UDP restart failure was then isolated from CapyIO. With
UU Remote running, every tested port in 45980--46030 returned Winsock 10048
without an enumerated endpoint owner, including unused 46012. The 40000/40001/
40010/40011 set remained available, so the controlled Android build and Windows
service configuration moved there; the rejected `SO_REUSEADDR` experiment and
temporary `socket2` dependency were removed. In the accepted restart sequence,
generation 3 owned speaker/microphone PIDs 60340/56084, Stop removed both
processes and ports, and generation 4 reacquired 40001/40011 with new PIDs
2960/44928 while UU stayed running. Android reported microphone `ACTIVE` with
6,016 generated/sent packets, 6,016 datagrams and zero queue drop, plus speaker
`ACTIVE` with 34,614 received datagrams, 17,307 complete packets and zero
source/format/reassembly/queue errors. The interactive Windows microphone probe
again measured non-zero RMS/peak in every shown 100 ms window without saving
audio. The owner's final UU listening confirmed normal phone playback without
the previous squeal, so the controlled-host 21.85 acceptance is complete.
