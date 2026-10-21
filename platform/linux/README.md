# Linux platform boundary

For the unified build-only command, strict staging inventory, passive preflight
and the boundary between synthetic bootstrap and separately authorized real
operation, see [Linux build and operations](../../docs/LINUX_BUILD_AND_OPERATIONS.md).
This is not an installer or a completed Linux pairing/provisioning flow.

## Current software-validated scope — 2026-10-20

Implementation `510aea94` passed all three hosted gates in
[run 39453132776](https://github.com/Arthurshen926/HardwarePool/actions/runs/39453132776).
The explicit, finite one-peer launcher supports mono 48 kHz S16LE
Android-compatible PCM into native PipeWire playback or an owned Audio/Source.
The private graph proves direct independent-recorder readback, no-consumer Stop,
owned-child exit and server disconnect, with consent and cleanup checks.

These graph cases use Linux-led proposals. Android advertised-capability
selection and incoming-Draft admission have separate IPC/JNI software evidence,
not combined incoming-Draft real-graph acceptance. Physical Android capture and
user-session testing remain deferred. There is no PulseAudio/ALSA compatibility,
reverse audio, complete Linux pairing UI, autodiscovery or provisioning claim.
Media is still unauthenticated, unencrypted trusted-lab UDP; existing paired TLS,
independent consent, exact binding, finite grants and leases remain mandatory.

## Default headless host and earlier increments

`capyio-host` owns a hardware-free Linux NodeRuntime through bounded inherited-pipe
management. Start with an explicitly supplied stable identity:

```text
cargo run -p capyio-node -- linux-host --node-id 00000000-0000-4000-8000-000000000301
```

Send newline-delimited requests on stdin; read one JSON response per record:

```json
{"version":1,"id":1,"command":"host_status"}
{"version":1,"id":2,"command":"get_snapshot"}
{"version":1,"id":3,"command":"host_shutdown"}
```

stdout is protocol only; logs use stderr. EOF exits. An oversized request terminates
without waiting for newline. See ADR 1514 for limits, error codes and trust boundary.

The default catalog is empty by design. Without explicit mock opt-in this host
does not compose Routes. It does not pair with another device, persist an identity, open hardware, install a service or expose
a network listener. Existing demo commands remain explicitly Windows/Android fixtures.
Future Linux Adapters may use PipeWire, v4l2loopback, uinput/uhid and USB/IP; none of
those implementations is enabled by this default headless-host command. The
separate explicit audio launcher described above is the current PipeWire exception.

## Opt-in mock subprocess lifecycle

ADR 1515 adds a separate hardware-free test mode:

```text
cargo build -p capyio-node -p capyio-mock-source -p capyio-mock-sink
cargo run -p capyio-node -- linux-host --node-id 00000000-0000-4000-8000-000000000301 --mock-routes
```

Only repository-built `capyio-mock-source` and `capyio-mock-sink` beside the node
executable are launched, without shell/arguments. That directory must be trusted;
Adapter ID checks do not authenticate executable content. The Source catalog is
local; the Sink belongs to a clearly synthetic peer with synthetic Session/consent.

```json
{"version":1,"id":1,"command":"mock_route_start"}
{"version":1,"id":2,"command":"get_snapshot"}
{"version":1,"id":3,"command":"mock_route_stop"}
{"version":1,"id":4,"command":"host_shutdown"}
```

`mock_source_crash` intentionally exits only the owned mock Source for failure
exercises. The next snapshot/status or repeated Start observes failure, stops and
reaps the mock pair; the host remains queryable. Restart the host to recover a
failed mock pair. Observation is command-boundary, not a background watchdog.

These are real subprocesses controlling a simulated Route lifecycle, not data
transfer. `sample_transfer_enabled`, `hardware_enabled` and `network_enabled` stay
false. All resources retain `test_only` labeling. No real peer trust is created.

### Experimental finite payload fixture (ADR 1516)

After building the node and both mock binaries, add `--mock-routes --mock-payload`
to `linux-host`. Start sends one synthetic UTF-8 token over separate Unix-local IPC;
Active requires a Sink receipt. Repeat Start does not resend; Stop clears receipt;
restart uses a fresh Stream/epoch. Without the additional flag, lifecycle-only
behavior is unchanged. This needs Linux Unix sockets, not IP listeners or hardware.
The full finite-fixture tests passed on hosted Ubuntu for commit `4210120`:
[CI run 39277747051](https://github.com/Arthurshen926/HardwarePool/actions/runs/39277747051).
This is bounded synthetic IPC evidence, not continuous media or hardware support.
The original cloud environment still denies Unix socket execution; see ADR 1516
for the exact tested scope and historical local blocker.

### Experimental fixed batch fixture (ADR 1517)

Use `--mock-routes --mock-batch` for exactly four ordered synthetic records. It
conflicts with `--mock-payload`; the old one-record option is unchanged. Complete
consumption and an aggregate receipt are required before Active. No continuous
stream, worker thread or hardware support is implied. Stop waits for bounded
synchronous Start to finish. Fixed-batch IPC acceptance passed for implementation
commit `07b068e43ccf628326288d2918a2b90021984918`:
[CI run 39280283094](https://github.com/Arthurshen926/HardwarePool/actions/runs/39280283094).
That run passed 39 focused tests and all 17 then-required IPC guards. It does not
cover subsequent test changes or documentation commits. The original cloud
socket restriction remains unchanged.

The later kernel-queue fixture hardening passed for implementation
`4e8c24907532daf781a8760ab12283edd67d542f` in
[CI run 39295525829](https://github.com/Arthurshen926/HardwarePool/actions/runs/39295525829):
40 focused tests and all 18 successful IPC guards. This includes bounded Source
failure under independently established queue pressure and test-fixture cleanup,
not new subprocess lifecycle or continuous-streaming proof. See
[LINUX-A05](../../docs/plans/active/LINUX-A05-kernel-queue-acceptance.md) for the
failed first fixture attempt, correction and evidence limits. Later docs-only
commits are not the tested implementation SHA.

## Separate experimental PCM playback endpoint

ADR 1518 adds `capyio-linux-audio-playback`, explicitly enabled for a finite
trusted-lab Android manual-microphone UDP → PipeWire playback path. The default
headless host is unchanged. This original standalone CLI does not create a
virtual microphone or negotiate dynamic paired Routes; later launcher increments
are described below. No physical-device playback is claimed. See
[the endpoint README](capyio-audio/README.md) and LINUX-A06 for limits and next gates.

The synthetic endpoint gate passed on implementation `4b5db1a342994365dab9ccff93b670a72fcba49d`:
[run 39353185940](https://github.com/Arthurshen926/HardwarePool/actions/runs/39353185940/job/118401894752). All 79 focused tests and 34
success guards passed, including exact UDP-to-child PCM bytes and owned-child
cleanup. This is not actual PipeWire or Android-device playback acceptance.

`capyio-linux-host::LinuxPlaybackHost::from_admitted_trusted_lab` explicitly
supervises an already authenticated/admitted playback owner. It adds no CLI
listener or credential configuration. ADR 1521 lists the remaining executable
paired identity, catalog, consent and lease composition prerequisites. This is
still insecure-UDP trusted-lab playback, not completed Linux product integration.

The separately invoked `capyio-node linux-playback --trusted-lab` now composes
one-shot existing credential/paired-record FIFO handoff, actual TLS/catalog,
exact local approval and supervised playback. Default commands remain closed.
Read ADR1522 before using it: inherited stdin is not inherently secure, the
launcher must be trusted/exclusive, and actual credential access/deployment/device
setup requires separate authorization. The ADR defines the complete framed input
schema. No key files, environment-secret loading or enrollment are provided.

## Passive prerequisite report

`capyio-node linux-playback-preflight [--target <name-or-serial>]` reports JSON
without starting playback, executing tools, connecting to PipeWire, reading
credentials or listening. Missing prerequisites are diagnostics; successful exit
and present metadata do not mean ready or authorized. Versions, server liveness
and actual target availability remain unverified. See [ADR 1523](../../docs/adr/1523-linux-passive-playback-preflight.md)
and the [real-device acceptance guide](../../docs/reports/LINUX_PLAYBACK_DEVICE_ACCEPTANCE.md)
before planning any separately approved deployment.

The Linux-led paired path now follows the actual Android Draft proposal, local
Android consent, matching finite grant, remote lease and Start sequence (ADR 1524).
Hosted tests call the real Android coordinator rather than a synthesized ready
peer. The subsequent advertised-capability selector and incoming-Draft admission
remove the old Windows-only selection restriction; physical acceptance remains
unverified. Local approval time includes negotiation; it never
extends itself while waiting for the peer.

### Optional incoming Android Draft mode

The same explicit one-shot trusted-lab launcher configuration may add
`"proposal_mode":"incoming_draft"`. Omit it (or use `"linux_led"`) to retain
existing Linux-led behavior. Incoming mode advertises its versioned mono audio
receiver contract and waits for the configured paired Android Source to propose
its actual Route before asking for fresh route-specific local approval. It does
not approve Android capture, auto-start playback or broaden the configured peer,
Source, Sink, endpoint mode or finite duration. Proposal mode itself adds no
reverse audio or OS projection; virtual microphone projection is a separate
endpoint mode below. See [ADR 1529](../../docs/adr/1529-linux-incoming-audio-draft.md) for
binding, grant ordering, Stop, timeouts and test limitations. Incoming-mode real
Android/PipeWire audio acceptance is still unverified.

### Experimental native PipeWire virtual microphone

ADR 1530 adds explicit `"endpoint_mode":"virtual_microphone"` to the bounded
playback launcher configuration. Omit `target` for this mode; the launcher owns a
fresh name derived from the local Sink Port ID. The default endpoint mode remains
`playback` and still requires its explicit target. `proposal_mode` is independent.
The local approval event displays the exact mode and generated `node_name`.

The advertised capability is Microphone, but its network port is Sink. Only after
existing independent consent, finite grants and leases can the owned pw-cat
process expose an Audio/Source for an application to select. No autolinking,
default-device change, service installation, PulseAudio/ALSA compatibility or
reverse capture is implied. The four private real-PipeWire scenarios passed at
`510aea94` in the run linked above. This establishes bounded native software graph
behavior, not physical-device or incoming-Draft real-graph acceptance.
See [ADR 1530](../../docs/adr/1530-linux-owned-virtual-microphone.md).


### Experimental explicit stereo capture to Android

`capyio-node linux-capture --trusted-lab` adds a separate Linux-led reverse path.
The bounded version-1 configuration names the exact paired Android `sink` Port
and explicit native PipeWire `target`, along with `node_id`, `listen`, `media`
and finite `duration_secs`. It rejects default/automatic capture and does not
change the playback launcher schema. Independent consent, finite grants and
Source/Sink leases must complete before the owned capture process starts.
The network Source advertises 48 kHz S16LE stereo, matching Android's existing
Speaker receiver. See [ADR 1531](../../docs/adr/1531-linux-explicit-stereo-capture.md).

All five private reverse graph scenarios passed at `e2316b06` in
[run 39560030792](https://github.com/Arthurshen926/HardwarePool/actions/runs/39560030792),
alongside all three CI jobs and prior virtual-source regression cases. They use a
synthetic distinct-channel source, the production Linux sender, actual Android
Rust policy/shared UDP receiver, and simulated consumption. They do not establish
Java AudioTrack, physical audibility, user-session automatic routing or encrypted
media. Existing mono/virtual-source results above remain separate evidence.


### Experimental decoded NV12 native camera projection

`capyio-linux-camera` now owns a finite, already-decoded canonical NV12 ingress and
an optional `native-pipewire` fixed sibling helper. The actual Runtime grant,
Session/Route/Stream/epoch and local Sink lease are checked before node creation
and every submit. It exposes only an owned native `Video/Source`; no automatic
links, portal/V4L2/kernel integration or default-device changes are made.

All seven actual private-graph scenarios passed at `23bcc2f0` in
[run 39563035324](https://github.com/Arthurshen926/HardwarePool/actions/runs/39563035324),
with all four CI jobs and existing audio gates green. A separate consumer checked
20 full 1280x720, 30-fps NV12 BT.709-limited frames byte-for-byte, stride and order.
The other cases cover no consumer/full-pipe backpressure, expiry, child exit,
server disconnect and malformed/partial IPC, with owned cleanup and sentinel
isolation. See [ADR 1532](../../docs/adr/1532-linux-owned-native-camera.md).

These ADR 1532 results establish the decoded-frame API and software projection,
not by themselves Android AVC/network or executable integration. Later secure
camera and executable slices are documented separately below. Queuing
alone never marks a Route Active. Physical camera and third-party application
compatibility remain deferred.


## Next bounded backlog

1. Consolidate deferred physical/AudioTrack acceptance separately; the explicit
   Linux → Android stereo software slice above has passed its hosted gates.
2. The finite camera and touchpad executable software entries are accepted
   under ADRs 1536 and 1537; consolidate their build and operation boundaries.
   Physical camera use and actual uinput/desktop gesture acceptance stay deferred;
   decoded NV12 results alone do not prove these later integration slices.
3. Gamepad, haptics, display and IMU remain experimental tracks, not mandatory
   full-platform parity for closing this audio milestone.

The Alpha's primary Speaker/Microphone/Camera/Touchpad versus experimental split
is recorded in [current status](../../docs/CURRENT_STATUS.md). This backlog is
planning only, not implementation or deployment evidence. Physical tests remain
postponed until the separate software work is ready for consolidated acceptance.

## Experimental owned Type-B touchpad

`capyio-linux-touchpad` adds the fixed five-contact 120 × 70 mm touchpad Sink,
Runtime owner and optional native libevdev helper behind existing pinned TLS.
It preserves full contacts rather than faking relative mouse gestures. The default
host still exposes no new device/listener and does not provision uinput access.
See [ADR 1535](../../docs/adr/1535-linux-owned-type-b-touchpad.md) and the
[crate guide](capyio-touchpad/README.md). Software acceptance uses fake native
operations and actual authenticated owner/policy paths. Kernel creation, desktop
recognition and physical gestures remain unverified and deferred.

## Explicit finite camera command

`capyio-node linux-camera --trusted-lab` now connects the existing pinned camera
owner to a real one-shot executable. It consumes only private inherited framed
bootstrap and exact approval, publishes a real LinuxCameraSink, and selects one
explicit Android Source from its authenticated catalog. Independent finite
consent and both leases precede secure dynamic-port data admission and native
helpers. This is not an Android-initiated picker or Camera2 permission flow.
See [ADR 1536](../../docs/adr/1536-linux-explicit-camera-launcher.md) for the schema,
limits, failure semantics and software-only acceptance boundaries.

Build the executable and fixed sibling helpers explicitly:

```text
cargo build --locked -p capyio-node --features native-camera
cargo build --locked -p capyio-linux-avc --features native-pipewire --bin capyio-linux-avc-helper
cargo build --locked -p capyio-linux-camera --features native-pipewire --bin capyio-linux-camera-helper
```

The three binaries must remain siblings with the same owner; helpers must be
regular non-symlink files and not group/world-writable. Native development
libraries are required to compile, but this does not install or authorize camera
capture. The default binary remains hardware-free. Canonical 720p30 rotation-zero
Annex B is capped at 60 seconds, 32 MiB and 120 frames without auto-renewal.
Queued frames leave the Route Starting, never falsely Active.


### Accepted camera executable evidence

The finite `linux-camera --trusted-lab` entry passed all seven hosted jobs at
`bbf6531731f27e7243f10fa4a2e80ae780f14b74`. All eight actual-node private graph
scenarios passed in [run 39576272282](https://github.com/Arthurshen926/HardwarePool/actions/runs/39576272282),
including independent NV12 pixels, exact helper ownership, Stop/EOF/revocation,
finite expiry, malformed CAVC and handshake timeout (the last correctly reports
no real graph). Legacy Linux IPC and new camera subprocess cases also passed.
See the [camera executable report](../../docs/LINUX_CAMERA_LAUNCHER_20261021_REPORT.md).

This proves the actual executable composition, not only the earlier library
owner. Production queued-frame status still remains Starting: application
consumption is not inferred from queue admission. Physical Camera2, Android
permission/UI, ordinary-user application compatibility and device testing remain
separate deferred acceptance. The separately accepted touchpad executable below
uses fake syscalls for validation; this camera result grants no input-device access.

## Finite touchpad executable

ADR 1537 adds `capyio-node linux-touchpad --trusted-lab` with the same private
length-prefixed bootstrap/nonce approval envelope as camera, but the existing
Android Source-listener touchpad data convention. Build production with
`cargo build -p capyio-node --features native-touchpad` and separately
`cargo build -p capyio-linux-touchpad --features native-uinput`; the owned native
helper must be beside the node. These commands compile only. Running the native
entry would create an input device after exact consent and must not be used for
software-only validation. It requires existing `/dev/uinput` permissions and
never installs modules, escalates or changes them.

Software acceptance instead builds the separate fixed fixture helper with
`cargo build -p capyio-linux-touchpad --features test-fixture` and runs
`cargo test -p capyio-node --test linux_touchpad --features touchpad-test-fixture`.
That explicit fixture command uses fake syscalls only. It does not establish
kernel/device/OS gesture behavior. Native and fixture features do not redirect
one another's command or helper path.

### Accepted touchpad executable evidence

Implementation `6257da2e9bd77c05dc12b0f3de458bc4d50b330f` passed all seven jobs
across four hosted workflows. [Touchpad run 39578248821](https://github.com/Arthurshen926/HardwarePool/actions/runs/39578248821)
passed ten default admission tests, all 18 fixture-feature actual-node tests,
fixed helper packaging/cleanup and native/fake separation checks. Official
libevdev native Node/helper compilation passed without executing the real helper.
See the [touchpad executable report](../../docs/LINUX_TOUCHPAD_LAUNCHER_20261021_REPORT.md).

The caller still needs an already-issued paired credential bundle/directory,
protected private bootstrap/control pipes, the exact selected Android Source,
local nonce-bound approval and independent Android consent. This is one finite
3–60-second session, not pairing/setup automation or continuous input. Actual
uinput creation, desktop recognition, OS gestures and physical Android capture
remain untested and deferred. Fake syscall traces and successful compilation
must not be described as real kernel input acceptance.

## Explicit virtual speaker capture mode

The existing `linux-capture --trusted-lab` command accepts
`endpoint_mode: "virtual_speaker"` without an external target. It creates a fresh
owned native PipeWire Audio/Sink only after finite bilateral Route authority,
then forwards explicitly routed application stereo audio to the Android Speaker
Sink. Omitted mode still captures only the explicitly selected source. See
[ADR 1538](../../docs/adr/1538-linux-owned-virtual-speaker.md) for the schema,
six-variable helper hardening and accepted hosted real-graph evidence.
Implementation `1ad1e97e6cf57067177a5e86100dccccba17360a` passed all seven required
CI jobs, including waveform, no-input Stop, child/server/control loss and expiry
in [PipeWire run 39579126851](https://github.com/Arthurshen926/HardwarePool/actions/runs/39579126851).
This is native PipeWire software routing through the Android Rust receiver with
simulated consumption, not physical AudioTrack or general desktop compatibility.
UDP remains unauthenticated/unencrypted trusted-lab-only; default devices are unchanged.

## Pairing persistence boundary (core only)

`capyio-linux-pairing-store` supplies an owner-bound non-secret journal, explicit
local recovery and typed single-peer bootstrap material for a caller-supplied
protected credential backend. No native Linux secure-store backend or enrollment
CLI is shipped, so this is not production secure persistence. It does not modify
finite launcher, resource-consent or lease semantics. See
[ADR 1539](../../docs/adr/1539-linux-pairing-persistence-boundary.md).

### Explicit finite camera stream profile

The opt-in Linux profile `cavc-avc-finite-720p30-v1` derives bounded work from
the remaining finite lease instead of the legacy120-frame limit. It retains
32MiB cumulative input and native resource ceilings; see
[ADR1555](../../docs/adr/1555-linux-explicit-finite-camera-stream.md).
Legacy defaults and Android production advertisement are unchanged.
