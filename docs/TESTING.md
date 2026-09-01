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
- the distinct touchpad Profile enforces positive himetric size, 3..=5 contacts,
  stable unique IDs per snapshot, declared geometry/pressure and closed button
  semantics;
- touchpad frame tracking suppresses initial, post-gap and post-epoch updates
  until explicit cancel-all, and timestamp validation is transactional;
- the committed synthetic fixture replays cancellation, one/two/four/five
  contacts, release and final cancellation with exact bounded metrics;
- the appended `Touchpad` Capability class round-trips as protocol enum value
  16 while touchpad frames stay outside the control envelope.

These tests prove only deterministic semantic contracts. They do not capture a
camera, inject OS input, register a virtual camera, start DSU/VIIPER, install a
driver/APK or prove ordinary-application compatibility.

## Windows synthetic touchpad API probe

- parameter tests enforce a positive physical himetric size and `1..=5`
  contacts before any platform call;
- the default command dynamically checks the four required `user32.dll`
  exports and performs no device creation or input injection;
- the explicit create-device smoke creates and immediately destroys one
  five-contact synthetic touchpad without submitting a contact frame;
- absent exports and creation failure remain typed probe outcomes rather than
  being mislabeled as a working Projection;
- 1/2/3/4-finger behavior, Windows Settings integration, RDP/multi-user
  behavior and process-loss cleanup require a later explicit lab fixture.

The probe installs no driver and makes no VHF, HID enumeration or Android
end-to-end claim.

## Touchpad platform mapping tests

- Android mapping requires an initial epoch cancellation and maps down, move,
  pointer-down, pointer-up, up and cancel to post-event complete snapshots;
- Android tests cover stable/reordered IDs, himetric coordinate scaling,
  calibrated pressure clamping, invalid action indexes, non-finger tools,
  duplicate IDs, surface/contact bounds and transactional timestamp failures;
- Android capture-session tests cover start barriers, reordered multi-touch,
  cross-event pointer-ID drift, transactional mapping failure, stop/restart,
  timestamp failure during cleanup, idempotent stop/close and closed-state
  rejection;
- Windows projection tests cover active/released/cancelled records, immediate
  cleanup after gaps and epoch changes, suppression until cancellation and a
  five-for-five replacement split into two batches bounded at five each;
- the Windows native encoder test checks PT_TOUCHPAD, documented pointer flags,
  himetric predicted/raw coordinates, zero cross-clock timestamps and omitted
  pixel-defined optional contact fields.

These tests inspect DTOs/native structures and the pure Rust capture boundary
only. They do not call `InjectSyntheticPointerInput`, create an Android OS/JNI
runtime, install an APK or
prove a physical cursor, tap, pan/zoom or global three/four-finger gesture.

## Controlled touchpad injection harness tests

- fixture tests validate four closed one-shot gestures with eight active
  updates between initial cancellation and final release/cancellation;
- exact dry-run metrics cover 1/2/3/4 contacts, 11 projected frames, nine
  native batches and maximum one batch per fixture frame;
- CLI parser tests prove dry-run is the default, unknown gestures/arguments are
  rejected, and injection requires a separate desktop-impact acknowledgement;
- invalid device parameters fail before user32 loading or device creation;
- all ordinary commands print `device_creation=not_requested` and
  `input_injected=false`.

CI never supplies the two injection gates. API submission success and observed
Windows behavior require a separately approved controlled-lab run.

## Synthetic touchpad Sink session and host acceptance

- fake-device unit tests cover explicit close, epoch cancellation, abandoned-
  session drop, transactional projection rejection, primary submission failure
  and retained cleanup failure;
- a failed or closed session rejects further frames, while a pure projection
  error leaves the session active and the rejected sequence reusable;
- ordinary session tests never load user32 or submit desktop input;
- on Windows build 26200.9168, the approved fixed one-finger command first
  failed with Win32 error 5 under the isolated `CodexSandboxOffline` account;
- the same already-built fixed binary then succeeded in the authorized
  interactive host context, reporting nine submitted batches and nine contact
  records.

The successful API return establishes accepted one-finger native submission,
not independently observed cursor displacement. Later composed exact-name
tests established accepted two-, three- and four-finger native submission; the
user separately confirmed visible scrolling for the two-finger run. Visible
three-/four-finger behavior, settings policy and remote
end-to-end behavior remain separate controlled acceptance work.

## Private remote-touchpad packet tests

- exact header-only cancel and five-contact 152-byte maximum packets round-trip
  deterministically;
- encoder rejects wrong Stream, stale/future epoch and semantic contract
  violations before producing bytes;
- decoder rejects short/long/non-exact packets, wrong magic/version/enums,
  excess contacts, reserved data, unknown flags and non-canonical optional
  fields;
- explicit epoch advance rejects old packets and non-advancing transitions;
- duplicate decoded contact IDs still pass through and fail the ordinary
  touchpad semantic validator;
- a hardware-free boundary test maps Android cancel/down/two-finger/move/up
  snapshots through packet encode/decode into Windows active/release batches.
- packet-Source tests pass a start/down/two-finger/reordered-move/up/stop
  lifecycle from the Android capture session through seven encoded and decoded
  packets; they also cover initial-cancel enforcement, transactional sequence
  gaps, active-contact close refusal, idempotent close and terminal rejection.
- delivery-session tests cover successful commit/release, exact same-frame retry
  after definite pre-write rejection, fail-closed unknown delivery, admission
  revocation, binding drift, construction mismatch and exactly-once channel
  cleanup including abandoned-session Drop.
- sender Runtime-worker tests use an actual `NodeRuntime` Route and prove that
  construction, each frame and close independently sample the trusted clock and
  fetch current Route state; authorization expiry, Route stop, inactive initial
  state, provider/clock failure and clock rollback all close the channel without
  delivering an unadmitted packet.
- concrete host-channel tests cover invalid capacity, full-queue definite
  rejection, ordered drain after sender close, admission denial, binding
  replacement, stale-packet discard, receiver close and idempotent cleanup.

These tests open no socket, create no synthetic device and inject no desktop
input. Authentication, encryption and transport reconnect remain future
acceptance work.

## Private remote-touchpad receiver tests

- invalid packet-rate and active-idle bounds fail during construction;
- duplicate/late packets, malformed packets and trusted-local arrival-clock
  regression close the Sink, poison the receiver and do not submit a rejected
  frame;
- a sequence gap remains observable and makes the hardware-free Windows
  projector emit cancellation for its retained contact;
- the fixed one-second rate window rejects the first excess packet and resets
  only after a valid explicit epoch advance;
- active contacts close at the exact idle deadline while an already released
  stream remains idle;
- Sink submission failure retains both the primary and failed-cleanup error;
- explicit disconnect and abandoned active receiver drop each close once.

All receiver tests use a pure in-memory Sink or `WindowsTouchpadProjector`.
They do not construct `SyntheticTouchpadSession`, call user32, open a socket or
inject desktop input.

## Private remote-touchpad Route ingress tests

- construction rejects zero/oversized queues, wrong backend/Profile/Sink,
  Pending or expired authorization, non-live Route state and stream/Route epoch
  mismatch;
- a Starting binding rejects packets until explicit activation against the
  same current Active Route;
- ordered enqueue/pump preserves sequences through the receiver and reports a
  bounded pump result;
- queue overflow clears pending records and closes without submitting them;
- Route Offline, authorization expiry and unexpected epoch change each close
  the affected session;
- oversized packets, local-clock regression and records left queued through
  the active-idle deadline never reach the Sink;
- an empty pump closes active contacts at the exact idle deadline;
- a strictly later Starting Route epoch discards old queued records, advances
  the Sink and accepts only the fresh epoch after reactivation;
- cleanup failure remains attached to the primary Route-session fault;
- the hardware-free Core Route → fixed queue → receiver → Windows projector
  path applies cancel, active and release snapshots without a native device.

Ten Route-ingress tests use only deterministic Core values, local timestamps,
memory and `WindowsTouchpadProjector`. No Runtime thread, socket,
`SyntheticTouchpadSession`, user32 call or desktop input exists in the test
path.

## Private remote-touchpad Runtime worker tests

- a dev-only Route provider reads owned snapshots from a real `NodeRuntime` as
  its Route moves Starting to Active to Stopping/Stopped;
- one worker drives activation, packet enqueue, periodic pump and explicit stop
  without accepting Route snapshots or timestamps from the packet caller;
- a later Runtime Route epoch discards the old queue, advances the memory Sink
  and accepts only the reactivated epoch;
- millisecond clock rollback fails closed even while nanoseconds advance; and
- Route-provider failure, clock failure and observed Runtime stopping state each
  close and poison the session.

Five receiver worker tests use the existing local `capyio-runtime` and `capyio-testkit`
crates only as development dependencies. They create no thread, timer, socket,
`SyntheticTouchpadSession`, user32 call or desktop input.

Four sender delivery-worker tests reuse that real Runtime provider boundary.
They verify three admitted frames plus close against five separate Route/clock
observations, and cover authorization expiry, Runtime stop, inactive
construction, provider/clock failure and rollback. Fake admitted channels retain
packet and exactly-once-close evidence; no transport or platform device exists.

One additional composition test connects the Runtime sender worker to the
Runtime receiver worker through the concrete four-packet host channel. Initial
cancel, one active contact and release reach `WindowsTouchpadProjector` in
order, followed by bounded receiver cancellation on close. The test uses a real
local `NodeRuntime` Route but no socket, native device or desktop input.

A separately ignored Windows acceptance uses the identical sender/channel/
receiver composition with `WindowsSyntheticTouchpadSinkFactory`. After explicit
approval it submits exactly four fixed one-finger packets, verifies all four
were enqueued and processed, releases contacts and closes the synthetic device.
Native API success is recorded separately from visible pointer observation.

Four private stream-record tests cover exact full-binding Hello with and without
authorization expiry, maximum five-contact Data at 176 bytes, exact Ack/Close,
outer/embedded epoch and sequence agreement, and rejection of truncation,
unknown flags, binding mutation and invalid embedded packet length. They perform
no I/O and make no authentication claim.

Three additional construction-boundary tests prove that a stopped Route or
invalid touchpad descriptor opens zero Sinks, a valid preflight opens exactly
one memory Sink, factory failure is typed, and the Windows production factory
satisfies the interface without calling `open`. The eight worker tests remain
device-free; the compile-time Windows factory assertion does not create a
synthetic device.

One ninth Windows-only worker test is ignored by default. With explicit human
approval it constructs a real `NodeRuntime` Route, passes authorized preflight,
opens and immediately closes `SyntheticTouchpadSession`, and asserts zero
enqueued/processed packets. The exact test submits no contact frame and is not
part of `cargo xtask ci`.

A tenth Windows-only worker test is also ignored by default. After separate
approval it activates the real Route and submits the compiled
CancelAll/down/move/release lifecycle through private packet encode, enqueue,
pump and `SyntheticTouchpadSession`. It verifies four packets enqueued and four
processed before release/close. A passing API result is retained separately
from any claim of visually observed pointer behavior.

An eleventh Windows-only worker test is ignored by default and requires a
separate approval. It submits CancelAll, two contacts down, synchronous vertical
translation and empty release through the real composed path. Four packets are
enqueued/processed before close. Passing native submission is not treated as
proof that the foreground application visibly scrolled.

## Android touchpad JNI and AAR tests

`cargo test -p capyio-android-jni` exercises the composition boundary without a
JVM or phone. It proves contiguous cancel/down/move/up packets, preserves both
contacts through a two-finger lifecycle and rejects unequal primitive arrays
before capture state mutates.

The Android target check uses `cargo ndk` with NDK 29 to link the release
`arm64-v8a` shared library. The separate Gradle release build compiles real
`MotionEvent` Kotlin code and packages the shared library into an AAR. Neither
targeted build installs an APK, opens a socket, creates a Windows synthetic
device or changes Android permissions. Wireless ADB ABI/API queries are
read-only evidence and are not required by hosted CI.

## Live Android-to-Windows touchpad lab

The CAPY-PTP-002V procedure requires explicit approval for the Internet
permission, APK installation and desktop input. The APK permission inventory
and installed hash are verified before execution. The Windows binary listens
only on `127.0.0.1:61000` and requires both `--inject` and
`--acknowledge-desktop-input`; it rejects mismatched Hello before device
creation. ADB reverse supplies the paired lab tunnel. Every Data record receives
an Ack only after native submission.

The accepted run processed initial cancellation plus an Android-generated
down/move/release lifecycle as 44 frames and 43 native batches/contact records.
The receiver closed the virtual device. A mixed Android clock-domain attempt
and a sandboxed Windows submission attempt failed closed and did not count as
acceptance. Hosted CI never installs the APK, creates the ADB mapping or submits
desktop input.

CAPY-PTP-003O repeats the same exact binding over the installed VHF fallback.
The receiver must add explicit `--vhf`, run elevated because of the protected
Broker ACL, and still validate Hello before opening the interface. Its first
physical phone gesture produced 80 acknowledged VHF frames with contact
transitions `0 -> 1 -> 0`, then closed cleanly. The separate `003P` acceptance
processed 830 physical frames, reached `0 -> 1 -> 2 -> 3 -> 4`, submitted all
accepted frames to VHF before acknowledgement and closed normally. Its bounded
wrapper passed with `max_contacts_observed=4`. This is physical transport/device
evidence; the remaining live Shell observation is distinct from the already
passing fixed Windows-generated three-/four-finger fixtures.

CAPY-PTP-003Q moves the lab's Hello/Data/Ack/Close ordering into the reusable
`PrivateTouchpadTransportReceiver`. Five hardware-free tests prove that invalid
construction or mismatched Hello opens no Sink, valid active/release Data is
acknowledged only after submission, malformed Data closes without submission,
and factory failure/active timeout are terminal. The existing lab argument test
also proves VHF remains explicit opt-in. Default tests open no device or socket.

CAPY-PTP-002W separates local touch capture from transport readiness. Before an
initial connection, touch must remain visibly consumed by the Activity while no
packet sequence is advanced; the sender retries the loopback endpoint. The live
vivo launch must reach `hello_binding=accepted` and `device_creation=created`.
Manual physical one- through four-finger behavior remains separate evidence.
Immersive bars, the Android system-gesture exclusion region and parent
interception denial are tested as app-owned best effort and must not be reported
as suppressing OEM gestures that are intercepted before `MotionEvent` delivery.
The manual receiver mode is separately selected with `--manual-session` and
raises only the bounded idle deadline from 30 seconds to 600 seconds. Sustained
phone input must not exceed four pending MOVE records; lifecycle records are
never sampled away.

CAPY-PTP-003T adds deterministic Android multi-finger motion-policy tests. They
prove one-/two-finger identity mapping, no position jump when the third contact
arms attenuation, 700-per-mille motion while contacts remain, identity reset
after complete release and rejection of scales outside `1..=1000`. Pointer
lifecycle remained separate from the Activity's then-current 24 ms initial/72 ms
added-contact MOVE settling. The arm64 JNI build, v0.7 debug APK and Android lint pass; the
installed APK hash matches the inspected local package and its manifest still
declares only `INTERNET`. The tuned live run processed and submitted all 2,165
frames, reached four contacts and closed cleanly.

User comparison found easier gesture effects but unreliable rapid simultaneous
three-/four-finger placement. Timestamped Activity logs repeatedly show
pointer-down through three or four contacts followed within 1--3 ms by
`ACTION_CANCEL`; read-only input diagnostics identify a full-screen system-UID
`global_gesture_monitor`. Raw input sampling separately observes tracking-ID
loss under light contact. CAPY-PTP-003U therefore counts probable system
interceptions and offers a user-initiated vivo SmartShot settings route. The
v0.8 debug APK builds and lints, declares only `INTERNET`, resolves the intended
settings Activity on the identified device and has a matching installed hash.
The post-setting physical comparison processed and submitted all 2,050 frames,
repeatedly reached four contacts and closed cleanly after the user disabled the
conflicting OEM gestures. The setting decision remains manual because ordinary
app tests cannot synthesize or suppress an OEM global gesture monitor.

CAPY-PTP-0041 Android v1.6 removes the initial 24 ms MOVE settling only for a
new one-finger `ACTION_DOWN`, resets the 16 ms sampling epoch at that boundary
and retains the existing 72 ms added-contact settling for multi-finger
stability. A pure tap/drag feedback tracker test covers an immediate second
contact, single-fire movement threshold, timeout, distance, moved-first-contact
and multi-touch cancellation cases. The app emits one setting-respecting local
`DRAG_START` haptic when the second contact crosses Android touch slop; it adds
no permission and does not claim that Windows accepted the semantic drag.
Android v1.7 adds the explicitly authorized `VIBRATE` permission and a
persistent, default-enabled in-app switch that uses predefined vibrator effects
independently of the global touch-feedback setting. Android v1.8 extends the
local feedback contract so every qualified one-finger tap emits a light `TICK`
(two taps therefore emit two ticks), while drag lock continues to emit the
stronger `CLICK`. The pure tracker test covers both tap events in a double tap.
Android v1.9 persists a weak, medium or strong intensity and applies it to both
tap and drag effects, preferring supported vibrator primitives and retaining a
bounded amplitude fallback. Its pure strength tests cover cycling, invalid
preference fallback and bounded per-effect scales/amplitudes. The VHF lab Sink
also gains a bounded ClickPad Button 1 compatibility latch: after a qualified
first tap, a nearby second one-finger down within 500 ms is pressed from its
first frame and remains pressed through movement until release. Receiver tests
cover press/hold/release plus far-contact and multi-touch rejection. This latch
does not change the private Android packet stream, user32 synthetic Sink or
installed driver package; physical file-drag comparison remains required.
Android v1.10 adds UI-only physical diagnostics before pressure-driven behavior
is considered. A pure observer accumulates finite non-negative per-contact
pressure and touch-major/minor values, counts five-contact frames, and counts at
most one five-contact reach per complete gesture. Duplicate IDs and invalid
values are rejected transactionally; the Activity ignores a rejected diagnostic
sample while continuing the unchanged JNI/network input path. Three pure tests,
debug assembly and lint pass. Pressure, geometry and Mechanical Force are not
yet submitted to VHF, and no five-finger Windows action is claimed.
The physical v1.10 comparison then recorded 9,074 samples with pressure fixed at
`1.00..=1.00`, maximum contact axes `2.40 x 1.60`, 828 five-contact frames and
11 complete gestures reaching five contacts; the unchanged receiver
acknowledged 2,530 frames and Android reached `maxContactCount=5`. This proves
five-contact capture, not a Windows five-finger action. It also rejects
pressure-driven behavior for the identified vivo because its ordinary-finger
pressure signal has no observed dynamic range.

CAPY-PTP-003V adds a VHF-only cursor-observation gate. It requires an exact
one-contact release with at least 100 himetric of source displacement, ignores
two-contact gestures and taps, anchors the interactive Windows cursor once and
reports both source and host deltas. Four CLI/motion-gate tests are hardware
free. The accepted physical run submitted all 155 frames and moved the cursor
from `(960,600)` to `(794,239)`. An unchanged sample taken while CapyIO was not
the resumed phone Activity is invalid for the user's actual separate-computer
UU topology and is not used as causal evidence. Version 1.0 still correctly
closes on foreground loss. CAPY-PTP-003Z uses the existing bounded 600-second
manual mode for continuous lab use instead of a one-shot release gate. Its
accepted Android 1.2 run submitted all 4,356 frames, reached four contacts and
closed cleanly only after Android Close. The user confirmed all requested
interactions except double-tap followed by drag; a read-only Windows query
reported that tap-and-drag is enabled, so that input shape remains a separate
diagnostic rather than a persistent-session failure.

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
