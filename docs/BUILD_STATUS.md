# CapyIO Build Status

> Updated: 2026-08-31 after the `CAPY-CAMERA-001C27` registered producer-stall
> recovery slice.

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

## CAPY-CAMERA-001 local-lab MVP

Complete on the authorized V2419A/Windows lab pair as a bounded local-lab MVP.
The CapyIO Android app captures a visible Camera2 preview, encodes 720p30 AVC,
switches front/back, quality and directly openable Camera2 ID/vendor Zoom
targets, retries the fixed exporter and fails a configured stream after five
seconds without encoded progress. Zoom targets let a vendor logical camera
choose a lens; they do not guarantee one physical sensor. The Windows path
validates the private CAVC stream,
uses the inbox H.264 decoder, publishes bounded NV12 shared memory and can expose
one temporary Session/CurrentUser Media Foundation virtual camera. Windows
inbox Camera displayed live phone pixels in controlled runs, and the C20
portable 0.670x/2x endpoints each decoded changing frames without backlog.
The final continuity regression exposed and fixed Windows accepted sockets
inheriting nonblocking mode after the bounded front/back reconnect poll.
The C23 build also supports an explicit trusted-LAN destination with one exact
Windows bind and phone IPv4 allowlist, fixed port and no DNS/wildcard/discovery.
An authorized no-ADB run accepted, decoded and Global-published 358 changing
frames and enumerated one temporary camera; ordinary Windows Camera pixels and
complete same-run cleanup were not captured. C24 extends Android connection
startup to a fixed roughly two-minute budget, Windows mapping readiness to 120
seconds and post-connection recovery to 60 seconds. It also prevents cleanup
errors from being hidden and keeps the display awake only while the visible
foreground capture state is active. Its authorized exact-hash run reached a
trusted-LAN config, Global mapping, one temporary camera and complete automatic
cleanup. Ordinary Windows Camera evidence was interrupted when rotation
recreated the Android Activity and stopped the foreground session. C25 now
handles rotation and bounded display-size changes in the same Activity without
locking orientation; true pause/surface loss still stops capture. Its authorized
exact-hash regression retained the same Activity record and CAVC stream/epoch
across portrait/landscape/portrait, and ordinary Windows Camera displayed two
visibly different live frames. A real background transition released Camera2.
The unrelated `tcp:61000` reverse remained, no camera reverse was created, and
the temporary Windows processes, port, ProgramData deployment and CLSID all
returned to a preflight-clean state. C26 closes the next hardware-free startup
gap: if registered activation occurs before the fixed Global mapping exists, it
emits a deterministic offline fixture and checks the fixed mapping after a
fixed 15-placeholder-frame countdown. C27 closes that provider's one-way
lifecycle gap. All registered activations now use the same asynchronous
provider; 400 consecutive empty 5 ms live polls cause it to release its mapping
handle, resume the fixture at the next output sequence/timestamp and mark the
fallback discontinuous. It continues fixed-name probing, rejects replay of an
old producer's last publication and reattaches either a fresh publication or a
replacement producer onto the same virtual timeline. Focused tests cover
pause, producer exit, same-name replacement and reattachment. The exact C27
release package, read-only clean-host preflight and full repository CI pass; a
hash-recorded ordinary Windows Camera three-transition regression remains
pending.

This does not provide production pairing/encryption, a production ADB-free
pairing/discovery workflow, installer/signing, unified Runtime lifecycle,
unattended background capture, long-duration reliability or broad
Android/Windows compatibility. The closure boundary, reconnect correction and
trusted-LAN and reconnect/cleanup evidence are in
`docs/CAPY_CAMERA_001C21_REPORT.md`, `docs/CAPY_CAMERA_001C22_REPORT.md`,
`docs/CAPY_CAMERA_001C23_REPORT.md` and
`docs/CAPY_CAMERA_001C24_REPORT.md`, with rotation follow-up in
`docs/CAPY_CAMERA_001C25_REPORT.md`, registered late-start follow-up in
`docs/CAPY_CAMERA_001C26_REPORT.md` and producer-stall recovery in
`docs/CAPY_CAMERA_001C27_REPORT.md`.

## Not built or tested

- automated Android permission or foreground-service management;
- CapyIO-owned microphone or input data path;
- production-installed Windows camera/audio/input virtual devices, driver, WDK
  or isolated-VM driver test;
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
