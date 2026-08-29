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

The shared audio-contract tests validate microphone, balanced-speaker and
lossless-music presets; reject voice processing on media playback, invalid QoS,
empty/duplicate/unbounded candidate inventories and unsupported use cases; and
prove deterministic exact candidate intersection without implicit conversion.

The MicYou Adapter tests verify exact v2.0.1 CLI identity plus the required
`device-stable-id-v1` capability, bounded structural output-device parsing that
preserves duplicate names but rejects duplicate endpoint IDs, stable-ID/name
configuration, fresh ID-to-index resolution after reorder, exact
ID/index/name child arguments, explicit Wi-Fi bind/port/device arguments,
VoiceInteractive semantic mapping, fixture listener readiness, child status,
bounded diagnostics and idempotent stop/reap behavior. The ignored real-CLI
test requires `CAPYIO_MICYOU_CLI` to name a user-supplied executable built from
the pinned revision. The reviewed Windows CLI validates the same
ID/index/name tuple before and after audio startup. The ignored probe does not
install VB-CABLE or an APK and by itself does not prove a Windows capture
endpoint.

Windows MicYou supervisor tests use a repository fixture to distinguish the
short-lived listener-readiness connection from a retained process-owned TCP
peer, then prove peer close and stopped supervision. Audio Share runs the same
regression through the shared `capyio-process-presence` safe boundary. The
bounded Windows owner table returns counts only; tests and DTOs do not retain
peer addresses.

Microphone desktop-composition tests bind a fake MicYou process boundary to a
real `NodeRuntime` `AdapterManaged` Route. They require three consecutive phone
presence samples before `Active`, keep listener-only readiness at `Starting`,
map peer loss and endpoint/start failures to typed sanitized Problems, stop and
reap the receiver after active peer loss or a bounded initial wait, advance
epoch on retry and prove an active IMU Route remains unchanged. Quick Action
tests require schema v2, a truthful blocked Browser Mock/Tauri state and a
connection hint containing only the validated bind IP/port—not the executable
path or raw endpoint ID.

`physical_quick_action_tracks_disconnect_retry_and_stop` is ignored by default
and requires explicit physical-lab authorization plus `CAPYIO_ADB` and
`CAPYIO_ANDROID_ADB_SERIAL`. Optional bounded `CAPYIO_MIC_*_HOLD_MS` variables
create observation windows. The test force-stops and quick-starts only the
already installed MicYou Android package, drives `Start`, stable `Active`,
phone loss, `Offline`, explicit `Retry`, renewed `Active` and terminal `Stop`,
and leaves the phone package stopped. PCM and exact-silence claims additionally
require an ordinary Windows capture client; TCP presence alone is insufficient.
Any WAV or other recording created for physical acceptance contains raw
microphone payload and must stay in a Git-ignored local evidence directory. It
must never be committed, placed in ordinary logs or uploaded by hosted CI.

MicYou trusted-host configuration tests require a fixed user-local path,
schema-v1 and deny-unknown-fields parsing, complete-or-rejected environment
overrides, exact stable-ID selection even when endpoint names are duplicated,
redacted debug output and create-new persistence that refuses silent overwrite.
Desktop tests prove a file-equivalent configuration installs the same typed
Route while the serialized Quick Action still excludes executable and endpoint
identity. These tests use synthetic paths/inventories and do not run MicYou or
write the real user configuration.

Audio Share regression tests require the `MediaBalanced` common PCM
specification to produce the unchanged pinned protobuf bytes and map only
observable transport counters into common metrics.

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
