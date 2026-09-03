# CapyIO Testing Strategy

## Principles

- Tests prove explicit requirements and failure isolation.
- Core/Protocol/Adapter DTO tests run without hardware.
- Platform and driver claims require identified environments and retained
  evidence.
- Mock UI/Sidecar behavior is visibly simulated.
- Tests are not removed or weakened to hide migration defects.

## Unified commands

```text
cargo xtask doctor
cargo xtask fmt
cargo xtask check
cargo xtask test
cargo xtask validate-docs
cargo xtask validate-manifests
cargo xtask adapter-smoke
cargo xtask ci
cargo xtask demo
cargo xtask imu-demo
cargo xtask android-doctor --serial <explicit-serial>
cargo xtask android-baseline --serial <explicit-serial>
cargo xtask android-collect --serial <explicit-serial>
```

Frontend uses `corepack pnpm typecheck` and `corepack pnpm build`.

## Foundation unit tests

- Node has no global role and may own Source/Sink Ports;
- Capability/Adapter ownership and duplicate IDs;
- Source→Sink Profile compatibility;
- Source→Source, Sink→Sink and mismatched Profile rejection;
- valid/invalid Route transitions;
- opposite-direction Routes coexist;
- stopping one Route leaves another active;
- Adapter failure affects owned Routes only;
- catalog replacement after Adapter restart;
- Protocol catalog/Route/Problem round trips and enum/version failures;
- Adapter manifest validation;
- NDJSON framing, malformed/oversized messages and correlation;
- stdout/stderr limits are enforced while reading, including oversized input
  without a newline;
- timeout, late/unexpected response, malformed response and stdout closure
  poison the sequential Host, reap the child and reject later requests;
- generic Route prepare/start/stop/status contracts round trip without carrying
  continuous data payloads;
- child stdout machine-only behavior and bounded/truncated stderr;
- Requirement parser rejects duplicate, malformed and non-canonical IDs, and
  traceability covers every normative PRD ID;
- deterministic UI snapshot with four Routes.

## Fixture-first IMU tests

- committed JSONL envelopes validate Profile, timestamps, clock domain, epoch,
  sequence, SI units, coordinate frame, accuracy, calibration and sensor data;
- Panel and Recorder consume independent bounded queues from one fan-out;
- a full/stopped Recorder does not block Panel progress;
- gaps, duplicates, late samples, wrong streams, stale/future epochs, sequence
  exhaustion and recorder bounds have explicit regression tests;
- `cargo xtask imu-demo` replays the same compiled fixture through the headless
  node and emits numeric Panel plus JSONL Recorder evidence;
- the desktop Browser Mock and Tauri backend expose the same schema-v3 fixture
  summary and label it as simulated rather than live phone data.

These tests require no phone and make no SensorServer, APK, network or physical
timing claim.

## Video and input contract tests

- canonical video and input/haptics Profile helpers match the normative
  registry and replace stale mock-only names;
- packed NV12/BGRA candidates validate dimensions, reduced rational frame
  rate, closed colorimetry and derived payload bounds;
- negotiation requires exact complete candidates and performs no implicit
  resize, rotation, decode, color conversion or QoS rewrite;
- frame descriptors reject zero/stale epochs, wrong payload sizes and invalid
  end-of-stream payloads;
- input stream descriptors keep clock-domain allocation out of per-frame
  headers;
- the shared sequence guard rejects wrong stream, stale/future epoch,
  duplicate/late sequence, non-advancing epoch and sequence exhaustion, while
  reporting gaps;
- pointer/keyboard reset, empty touch snapshots, fixed gamepad neutral state and
  explicit haptics stop make fail-safe cleanup testable.

These tests prove only deterministic semantic contracts. They do not capture a
camera, inject OS input, register a virtual camera, start DSU/VIIPER, install a
driver/APK or prove ordinary-application compatibility.

## VIIPER Xbox 360 codec tests

- a fixed-revision golden vector distinguishes the external device-stream
  `InputState` frame from VIIPER's different same-length host USB report;
- every representable semantic button and all nine D-pad combinations map to
  exact little-endian masks;
- six independent source-axis sign selectors, asymmetric signed endpoints and
  trigger rounding are explicit;
- current and future source buttons without Xbox 360 fields fail closed, while
  all six reserved bytes remain zero;
- rumble feedback accepts exactly two bytes, preserves left/right raw motor
  intensity and rejects both truncated and trailing data.

These tests do not open a socket, start VIIPER, attach USB/IP, install a driver,
enumerate a system controller, prove game compatibility or construct a haptics
command without a duration/Route lifecycle.

## VIIPER DualShock 4 codec tests

- the pinned 31-byte stream layout keeps controls, two inactive touch contacts,
  three fixed-point gyro axes and three fixed-point acceleration axes at exact
  offsets;
- semantic face buttons use DS4 physical positions, non-zero analog triggers
  set their matching digital bits, and signed sticks cover both asymmetric
  `i8` endpoints;
- separately typed SI-unit IMU envelopes map through explicit signed axis
  permutations to 16 counts per degree/second and 512 counts per m/s²;
- the fixed Controller Lab landscape mount maps Android source `+Y,+Z,+X`
  into DS4 body X,Y,Z while leaving the StandardPort sample in its normative
  Android device coordinate frame;
- duplicate axes, invalid envelopes, fixed-point overflow and unsupported
  paddles fail closed before a packet is emitted;
- feedback accepts exactly seven bytes and preserves both rumble motors, RGB
  LED and flash timing bytes without inventing a reverse haptics Route.

These tests do not provision `dualshock4`, connect to a live VIIPER stream,
attach USB/IP or prove that a Windows game consumes native DS4 motion.

## VIIPER owned DualShock 4 session fixture tests

- a loopback fixture observes exact probe, create, fixed `dualshock4` add,
  device stream, initial neutral/stationary state and owned-bus removal;
- the response must match DS4 `054c:09cc`, a numeric device ID and object-shaped
  fixed metadata; an identity mismatch rolls back the known bus;
- gamepad and IMU streams have independent sequence trackers. A gamepad-only
  gap releases controls while retaining the last accepted motion sample before
  the recovered report;
- exact seven-byte feedback is read without creating a reverse Route, and
  explicit stop writes the safe state before stream shutdown and cleanup.

The USB/IP unit boundary separately filters DS4 exports and verifies an owned
port against the exact server, VIIPER bus and `054c:09cc` identity. Desktop
tests prove motion-only DSU never submits local controls. No test performs a
real attachment.

## Runtime-owned DualShock 4 paired Route fixture tests

- controls and IMU remain two independent StandardPort Routes with their own
  profiles, QoS modes, Route IDs and epochs;
- the shared DS4 Worker activates only after both anchors match their Runtime
  epochs and exact VIIPER provisioning succeeds;
- an IMU-source disconnect writes the safe state, removes the owned bus,
  offlines both dependent DS4 Routes and leaves an unrelated active Route
  unchanged;
- retry recovers both Routes with advancing epochs and a fresh owned bus;
- mismatched anchors fail closed before any VIIPER connection is opened;
- the optional USB/IP branch is constrained to the separately tested one-shot
  DS4 selector and exact owned-port lifecycle.

These fixtures use a process-local fake VIIPER server. They do not attach
USB/IP, enumerate a Windows DS4, install a driver or connect to a phone.

## Desktop DS4 selection and complete-state ingress tests

- the Windows host DTO keeps Xbox 360 and DS4 identities explicit and defaults
  the motion-capable path to DS4 `054c:09cc`;
- read-only preflight selects the controller-specific USB/IP inventory method,
  so DS4 mode cannot accidentally accept an Xbox export or vice versa;
- one accepted Android datagram remains one paired controls+IMU event on a
  capacity-eight non-blocking channel;
- peer timeout and listener stop emit explicit upstream-offline events rather
  than performing blocking projection cleanup on the UDP receiver thread;
- the desktop inspector drains the bounded ingress and publishes accepted
  packet, offline-event and last-remote-sequence counters;
- starting DSU replaces the observer ingress with the DSU senders, and stopping
  DSU restores the bounded Windows ingress.

These tests do not start the DS4 Worker, attach USB/IP or mutate Windows. The
ingress-to-DS4 activation Worker remains the final hardware-free 004C slice.

## VIIPER bounded probe fixture tests

- configuration rejects non-loopback IPs, port zero, zero/overlong deadlines
  and response limits outside `1..=4096`;
- a process-local `127.0.0.1:0` fixture observes exact `ping\0`, and the client
  requires exact `VIIPER` / `0.7.0` identity;
- the response limit accepts exactly N bytes and rejects N+1 before JSON parse;
- whitespace-only, malformed/trailing JSON, fixed Problem responses and wrong
  identity/version have distinct failures;
- a complete JSON prefix held open without EOF times out rather than being
  accepted early.

The fixture uses bounded channel joins and socket deadlines. It never starts or
connects to a real VIIPER server, creates a bus/device, enters a device stream,
attaches USB/IP or touches a driver/certificate store.

## VIIPER owned Xbox 360 session fixture tests

- a process-local server observes exact `ping`, create, fixed Xbox add, stream
  handshake, initial neutral and remove ordering;
- unsupported controls and rejected stream headers do not consume sequence or
  write a frame;
- an input gap writes neutral before the recovered state, epoch advance writes
  neutral before fresh input, and sequence exhaustion writes neutral and
  latches until explicit epoch advance;
- stop writes neutral, shuts down the device stream, removes only the owned bus
  and is idempotent;
- consecutive exact rumble frames, a no-byte timeout, one-byte truncation and
  clean peer close are distinguished; failed streams still attempt bus cleanup;
- add failure and injected device identity trigger removal of the known bus,
  and a cleanup failure is retained alongside the primary failure.

Expected stream packets are hard-coded independently of the production codec.
All tests use `TcpListener(127.0.0.1:0)` and a fake protocol peer. They neither
start VIIPER nor create a real virtual device or USB/IP attachment.

## VIIPER Runtime Route fixture tests

- a typed remote gamepad Source and local VIIPER Sink form one
  `ExternalProtocol` Route, which reaches Active only after the owned Worker
  sends initial neutral;
- unsupported controls reject one frame without consuming sequence or taking
  the Route Offline;
- upstream disconnect sends neutral and removes the owned bus before Offline,
  while retry provisions a fresh bus under a strictly newer Runtime epoch;
- add failure rolls back the known bus before the typed open-failure Problem;
- sequence exhaustion sends fail-safe neutral, cleans up and requires an
  explicit Route retry;
- a closed device stream is detected by bounded raw-feedback polling, cleaned
  up and reported as a stream Problem without constructing a haptics Route;
- a simultaneously Active IMU Route remains Active with no related Problem in
  every injected gamepad failure case.

The four tests use only process-local `127.0.0.1:0` listeners and independently
hard-coded request/frame expectations. They do not start or configure VIIPER,
touch USB/IP or a driver, enumerate a controller or make a game-compatibility
claim.

## DSU Runtime Route and live-lab fixture tests

- the DSU composition registers a typed IMU StandardPort sink and does not
  activate until its Worker binds IPv4 loopback with the exact Runtime epoch;
- upstream disconnect joins the Worker and releases its port before Offline;
  retry binds a new Worker under a strictly newer epoch;
- occupied-port and mismatched-epoch starts produce owner-correct typed
  Problems while a simultaneously Active unrelated IMU Route remains unchanged;
- the lab CLI parser closes commands, ports, sample count and signed axis
  permutations, while preflight releases the exact checked UDP port;
- a process-local two-WebSocket SensorServer fixture emits accelerometer and
  gyroscope readings through the real assembler, Runtime Route, DSU Worker and
  a real loopback DSU subscriber; acceptance requires 16/16 samples, observed
  subscription/motion delivery and zero queue, projection or transport errors.

All automated endpoints are ephemeral loopback fixtures. The command does not
prove a phone, emulator UI, game response or physical orientation. Those are
retained only by the manual procedure in `docs/CAPY_GAMEPAD_DSU_LAB.md`.

## Desktop Controller source and inspector tests

- a closed semantic Tauri DTO feeds the portable `GamepadStateComposer`; a
  button followed by a stick update retains both controls in complete snapshots
  and monotonically advances sequence;
- invalid D-pad and reserved signed-axis inputs fail without consuming sequence;
- reset returns buttons, D-pad, sticks and triggers to neutral;
- the desktop simulator starts the existing bounded DSU dual-input Worker on an
  ephemeral IPv4-loopback fixture, seeds a labeled stationary motion sample,
  submits one complete control state and observes it accepted with no queue
  overflow before bounded neutral stop;
- browser typecheck/build and visual checks cover the desktop layout, top-level
  Controller navigation, button press/release sequencing, 844x390 responsive
  layout, absence of horizontal overflow and absence of browser errors.
- the Windows status preflight has no input DTO, filters the bounded USB/IP
  inventory to exact Xbox 360 exports, distinguishes zero/one/multiple matches,
  preserves partial VIIPER/USB-IP readiness and maps host failures to stable
  sanitized codes without producing an owned attachment; a single-flight test
  rejects concurrent invocation and proves permit release;
- a real pre-restart run covers service-offline, ready-without-export,
  unique-export-ready and post-cleanup disappearance states while the owned
  USB/IP port remains empty. See `CAPY_GAMEPAD_006C_REPORT.md`.

Those `005A` checks do not package or install an Android application, capture a
phone sensor, establish a peer data plane, observe an emulator subscription or
prove game compatibility. The `005A` DSU source and motion seed are explicitly
simulated.

## Android Controller Lab tests

- closed UDP decoding accepts a valid complete control+motion frame and rejects
  unknown fields, a wrong token and the reserved `i16::MIN` axis value;
- a real loopback UDP listener accepts one sequence, rejects its replay and
  replaces a pressed control with neutral after the 350 ms peer deadline;
- the composition acceptance starts Android input and the existing DSU Worker,
  sends a complete UDP frame, observes the desktop source change to
  `android_touch`, verifies button/stick/motion values and observes the DSU
  controls queue accept the same state;
- Android `lintDebug` and `assembleDebug` compile the native Java surface for
  minSdk 26 / targetSdk 36 without a third-party runtime dependency; `aapt2`
  verifies package `io.capyio.controllerlab` and exactly one declared permission,
  `android.permission.INTERNET`;
- physical checks install only the recorded debug APK on the explicitly
  authorized device, launch the exported launcher Activity, inspect the visible
  UI, send touch+IMU frames to the desktop and verify timeout neutralization;
- the debug-only `--gamepad-physical-gate` registers a DSUC subscriber, waits
  for real Android data, closes it, then requires a replacement subscriber to
  observe a non-neutral control and finite IMU values. This covers Windows UDP
  `ConnectionReset` recovery after a stale subscriber as well as the complete
  phone-to-DSUS packet path;
- the debug-only `--gamepad-viiper-physical-gate` composes that same accepted
  Android stream into DSU and a real pinned VIIPER v0.7.0 Xbox 360 session. It
  prints exact bus/device IDs, stays alive for a bounded operator-selected
  enumeration window, neutralizes peer timeout, and explicitly removes only
  its owned bus. Windows USB/IP installation and attachment are separately
  authorized host-lab operations recorded in `CAPY_GAMEPAD_006A_REPORT.md`;
- `capyio-gamepad-usbip-lab preflight` executes exact-version probe and
  read-only list against the live export. Fixtures reject non-loopback/unbounded
  config, malformed or wrong-VID exports, persistent/all-device arguments,
  ambiguous ports and oversized output. A fake executor proves ordered
  probe/list/one-shot-attach/exact-port-detach without touching a driver. The
  real pre-restart run stops after list; see `CAPY_GAMEPAD_006B_REPORT.md`.
- post-restart fixtures require the terse returned port to resolve through
  `usbip port <port>` to the exact loopback server, VIIPER bus and Xbox
  `045e:028e` identity. Missing/mismatched inventory triggers exact-port
  cleanup; the physical lab polls that same invariant once per second and the
  recorded `006D` run proves PnP, XInput button/axis delivery, liveness and
  exact detach.
- the `--gamepad-windows-ds4-only-runtime-gate` keeps the optional XInput
  companion disabled and requires physical Android packets plus non-neutral
  controls before exact cleanup. The recorded 150-second run accepted 7,096
  packets and 723 non-neutral states; the independent WGI probe observed an
  advancing `054c:09cc` report with button, D-pad switch and axis changes.
- a paired Android projection replaces each transient 350 ms WLAN peer gap
  with a complete neutral controls+motion state while keeping the virtual
  controller owned and active. Explicit listener stop remains terminal and
  removes the projection.
- the fixture DSU subscriber uses bounded nonblocking 1 ms polling so its
  initial request cannot be lost behind a 100 ms receive wait before the
  Runtime-owned loopback endpoint binds.

Loopback automation proves the wire/parser/composition boundary but not Android
touch dispatch, physical SensorManager axes, WLAN/firewall behavior, emulator
response or game compatibility. The recorded `005B` run proves the first four
physical boundaries through a conforming DSU subscriber; emulator/game behavior
still requires a separately installed Cemu/Dolphin instance.

## SensorServer mapping contract tests

- the pinned upstream three-field JSON shape maps exact finite axes, timestamp
  and Android accuracy values;
- empty, oversized, malformed, unknown-field, wrong-axis-count, zero-timestamp
  and unknown-accuracy messages fail explicitly;
- accelerometer and gyroscope readings pair in either arrival order only inside
  a configured skew bound;
- each required reading is consumed once; replacing an unpaired sample is
  observable and a later in-skew sample recovers;
- timestamp regression and sequence exhaustion fail closed;
- optional fresh magnetic-field data and every component timestamp remain in
  the IMU Profile output.

These tests use recorded synthetic JSON and no WebSocket implementation, phone,
APK or network connection.

## SensorServer WebSocket contract tests

- endpoint construction accepts only typed IP addresses, non-zero ports and
  fixed per-sensor paths;
- a loopback RFC 6455 server proves exact text-message mapping;
- ping/pong, close code and socket timeout have distinct outcomes;
- malformed JSON, binary data and messages above 4 KiB do not reach the IMU
  consumer;
- an HTTP upgrade response exceeding Tungstenite's 64 KiB handshake attack
  limit fails the connection;
- dependency validation pins Tungstenite 0.30.0 to `handshake` only and rejects
  async/TLS additions in this slice.

These loopback tests open only an ephemeral local port. They do not connect to
the phone, install an APK or claim production authentication.

## Deterministic integration tests

Fixtures use HP OmniBook Ultra Flip 14 and vivo X200 Pro mini with no environment
or hardware reads. Tests register both catalogs, open a Session, prepare/start
opposite-direction Routes, stop one, simulate Adapter/peer loss and assert
ordered bounded events/snapshots.

## Audio Share external-process probe tests

- configuration requires an explicit IP address, non-zero port, bounded
  enumerated endpoint ID, encoding, channel count and sample rate;
- server arguments are direct process arguments and never a shell string;
- pinned version and endpoint-list parsing enforce output, line, count, ID and
  name bounds, reject duplicates/mismatched totals and tolerate lossy device
  display names without weakening ASCII structure parsing;
- a fake runner covers unsupported versions and missing configured endpoints;
- an ignored test probes a separately supplied, hash-verified v0.3.4 CLI and is
  never required by hosted CI.

The probe tests above do not start the audio server or send PCM. Process
supervision, receiver-loss/Route behavior and physical playback are separate
acceptance steps.

A separately ignored real-CLI stale-endpoint test re-probes an explicitly
supplied endpoint that is expected to be absent, requires the typed
`ConfiguredEndpointMissing` result before child spawn and confirms supervisor
state remains stopped. It complements, but never replaces, the ignored current
endpoint start/listen/stop probe.

The next supervisor tests use a repository-built fixture executable to prove
TCP-listener readiness, running/early-exit state, startup timeout, bounded
continuous output, explicit kill/reap, idempotent stop and Drop cleanup. A
separately ignored test briefly starts the hash-verified real Windows CLI on an
explicit loopback port, verifies it remains running after readiness, then stops
and confirms no process/listener remains. Listener readiness is not receiver
presence; v0.3.4 has no machine-readable peer-status API and tests never parse
ordinary log prose into lifecycle state.

The same bounded supervisor has a dedicated virtual-speaker launch contract:
one positional explicit IPv4 bind address, no upstream version/endpoint probe,
listener readiness, bounded output and idempotent stop/reap. Tests reject an
unspecified address and zero port. Desktop projection tests keep this fixed
mode separate from the legacy endpoint picker.

An ignored Windows/Android lab test composes that supervisor with the Runtime
Route. It requires explicit `CAPYIO_VIRTUAL_SPEAKER_EXE`, bind-IP and port host
configuration plus an elevated token, waits for stable receiver presence,
retains a bounded directed-playback window and proves explicit stop. The test
does not install a driver, change permissions or infer audibility from TCP.

Windows-only owner-table tests then prove that the short-lived readiness
connection is not retained as a receiver, a process-owned established peer is
observed, peer close becomes disconnected and stopped supervision becomes
unknown/not-running. The test filters by PID and port and never asserts or
retains peer addresses. This is transport-presence evidence, not Audio Share
negotiation or playback evidence.

Hardware-free desktop composition tests bind a fake process boundary to a real
`NodeRuntime` `AdapterManaged` Route. They require three consecutive established
receiver samples before `Active`, reset the counter on an intervening absence,
map receiver loss, child exit and process-start failure to typed Route Problems,
bound the initial receiver wait, reap the child on wait exhaustion, verify retry
advances the epoch, and prove that audio failure leaves an already active IMU
Route unchanged. They also assert that the AdapterManaged Route exposes a
private-negotiated Audio Share format rather than claiming an unobserved PCM
request. The fake proves orchestration only; the adapter fixture tests remain
the evidence for real child/TCP observation behavior.

The desktop composition tests also map the Adapter's concrete
`ConfiguredEndpointMissing` start error to the stable
`CAPY.AUDIO_SHARE.ENDPOINT_UNAVAILABLE` Problem without retaining the endpoint
ID. Other start failures remain `PROCESS_START_FAILED`; no ordinary CLI log
text is parsed to distinguish them.

Quick Action tests assert schema version 1, a truthful blocked state when host
configuration is absent, finite operations derived from Route state, rejection
of unknown request fields (including an attempted executable path), and matching
Browser Mock/Tauri TypeScript contracts. The Tauri host owns a 250 ms poll loop;
the WebView refresh only observes the projection and is not lifecycle authority.
A separate endpoint-selection contract rejects unknown request fields,
unbounded or non-token input and active-Route process replacement. Display
names are bounded and control characters replaced; inactive replacement is
covered with a fake process boundary. The real ignored CLI probe confirms the
current Windows endpoint inventory remains parseable, while raw IDs are not
asserted or retained in repository evidence.
A separately ignored physical test composes the real supervisor and Runtime
Route and waits for active, disconnect, later-epoch retry, active and stopped.

Windows-service unit tests validate closed launch configuration, explicit
non-unspecified IPv4/port bounds, stable receiver gating, receiver loss,
Broker exit and child-stop ownership without installing a service. The binary
also provides an explicitly time-bounded console mode for a later exact
physical fixture run. SCM install/start/reboot evidence is not implied by these
tests and requires a separately approved service-deployment slice.

The approved `CAPY-AUDIO-001B6A` local-lab run registered the exact manual
LocalSystem service, observed its Broker child plus TCP/UDP ownership, proved a
controlled stop released processes and port 65530, restarted with new process
IDs and regained the Android receiver. A directed five-second CapyIO endpoint
submission then left the service and transport healthy. The run does not claim
human audibility without a separate operator observation and does not cover
reboot/autostart, desktop IPC or installer behavior.

`CAPY-AUDIO-001B6B` adds closed-schema/control-bound unit tests and an ignored
physical desktop/service composition test. The approved local run exercised
dozens of non-administrator status calls across stop/start generations, proved
port cleanup without stopping the SCM host, proved Quick Action service
selection and UI-shutdown independence, restored the Android receiver and
retained `active` state after a five-second endpoint submission. Automatic SCM
configuration was observed; reboot recovery and signed-installer behavior are
still not implied.

## Sidecar smoke test

Adapter Host launches repository-built mock binaries, performs initialize,
probe, health, catalog, Route prepare/start/status/stop/status and shutdown, then
verifies exit and stderr/stdout separation. Separate cases simulate abnormal
child exit, newline-free stdout/stderr overflow and a response that arrives after
the deadline. The terminal-failure cases assert `Poisoned`, child reaping and
future-request rejection. Finite Mock-private samples are not a generic Adapter
contract, data plane or performance test.

## Later platform tests

- Android: actual sensor/audio parameters, permissions, visible service,
  lock/background, focus, route changes and power saving;
- Windows user mode: endpoint enumeration, Broker restart, bounded IPC and
  sleep/resume;
- drivers: install/update/remove, service restart and reboot in an isolated
  VM/dedicated target or the ADR 0029 controlled local lab; project-only
  Verifier remains isolated by default and requires separate approval;
- end to end: IMU Panel/Recorder, audio both directions, camera, gamepad,
  independent Routes, disconnect/reconnect and clock epochs.

Gate 7B first proves an unchanged pinned SysVAD build and an approved-target
install as a toolchain/enumeration baseline; synthetic SysVAD
WASAPI loopback is not real-PCM evidence. It then requires `CapyIO Speaker`
enumeration, explicit application selection, endpoint-associated render APO
PCM evidence, bounded ring-full/Broker-loss behavior, silence on the ordinary
physical/RDP endpoint, audio-service/reboot survival and clean uninstall. The
APO callback must have evidence of no blocking, allocation, file/network I/O
or ordinary logging. Repository validation prevents driver source from
appearing while the SysVAD record still declares `source_imported: false`.
ADR 0029 permits `DESKTOP-AT8EVE9` for the approved-target install only after
its recovery, exact-package and rollback preflight passes.

## Android read-only lab commands

Android commands require `--serial`; target order is never inferred. They use
an allow-list of `adb devices`, `getprop`, `wm size` and `dumpsys
sensorservice`, impose a four-megabyte process-output bound, and retain only
model/build-version plus bounded sensor-list fields. `android-baseline` prints
sanitized JSON; `android-collect` writes it only below ignored
`test-results/android/<run-id>/`. Neither command installs an APK, grants a
permission, starts a service or changes settings.

The separately authorized physical `CAPY-IMU-001B2` run used the fixed upstream
SensorServer v7.2.1 binary after its published SHA-256 matched. Live evidence
requires paired source timestamps, sequential envelopes, equal Panel/Recorder
counts, zero silent sequence repair, a second clean connection after graceful
close, and an explicit client failure when the phone service stops. Physical
addresses, pairing codes and raw device identifiers are not committed.

The authorized `CAPY-IMU-001B3A` desktop run additionally exercises the Tauri
start/read/stop DTO boundary. Acceptance requires a visible typed failure, a
later successful connection with changing numeric vectors and monotonically
growing sample count, and a stopped state that retains the last snapshot. The
desktop Rust physical test remains ignored by default and requires explicit
`CAPYIO_LIVE_IMU_IP` and `CAPYIO_LIVE_IMU_PORT` environment variables. Normal CI
does not require a phone or a reachable private endpoint.

`CAPY-IMU-001B3B` binds that worker to the same `NodeRuntime` that owns the
desktop Node. Loopback tests assert the staged Route lifecycle, retained
disconnect Problem, fresh retry epoch and explicit stop without a phone. The
ignored physical test asserts real paired samples drive the Route to `Active`
and shutdown reaches `Stopped`. The authorized lab run also confirmed that a
stale phone listener produces `Offline` rather than a false success, then
succeeds after the service is restarted. Private addresses are not retained in
repository evidence.

## Data and timing quality

Signal tests measure latency, clipping, gaps, discontinuities, loss/repeat and
RMS. Drift tests record source/sink samples, queue water level and resampling
ratio rather than inferring drift from acoustic latency. Sensor tests preserve
clock domain, sequence, units, coordinate frame, accuracy and calibration.

## Evidence format

```text
test-results/<run-id>/
  manifest.json
  summary.md
  config.json
  metrics.jsonl
  runtime.log
  adapter-stderr.log
  platform/device inventories as applicable
  input/output recordings only when explicitly authorized
```

`manifest.json` records Git commit, versions, OS/device, Route/Profile/backend,
network mode, case and timestamps.

## CI policy

Required before merge: Rust format, check, Clippy warnings denied, tests,
Protobuf build, docs/repository validation, manifest validation, Adapter smoke,
frontend typecheck/build and dependency/license review when dependencies change.
Hardware jobs may be manual but must attach evidence. Claims match actual runs.

The native DS4 application-consumer Gate compiles and self-tests the SDK-only
`RawGameController` probe without an attached device. Its no-device case must
fail closed. A physical pass is recorded only inside a separately authorized,
exact VID:PID USB/IP attachment window and requires a user-visible control
change; normal CI never attaches a controller.

Pull-request workflows targeting `main` explicitly check out
`github.event.pull_request.head.sha`; a synthetic merge commit is not substituted
for the submitted head. Rust format/check/Clippy/tests, documentation, manifests
and Adapter smoke run on Windows, Linux and macOS. The frontend uses the frozen
pnpm lockfile for typecheck/build. Windows additionally runs native Tauri Cargo
check/build.

Linux/macOS native Tauri packaging is an explicit merge-gate skip in the current
foundation: those runners still execute Rust Core/Adapter and web UI gates, but
they do not count as Tauri application build evidence. Adding non-Windows Tauri
packaging requires an explicit workflow and platform prerequisites; absence is
never reported as a pass.
