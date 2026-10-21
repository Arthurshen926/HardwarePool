# Linux platform boundary

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

The new private graph acceptance is pending. Its five required scenarios use a
synthetic distinct-channel source, the production Linux sender, actual Android
Rust policy/shared UDP receiver, and simulated consumption. They do not establish
Java AudioTrack, physical audibility, user-session automatic routing or encrypted
media. Existing mono/virtual-source results above remain separate evidence.


## Next bounded backlog

1. Complete hosted acceptance of the explicit Linux → Android stereo candidate
   above, then consolidate deferred physical/AudioTrack acceptance separately.
2. Camera, then touchpad, following the Alpha primary capability scope. Each needs
   its own platform projection, permission boundary and software acceptance gate.
3. Gamepad, haptics, display and IMU remain experimental tracks, not mandatory
   full-platform parity for closing this audio milestone.

The Alpha's primary Speaker/Microphone/Camera/Touchpad versus experimental split
is recorded in [current status](../../docs/CURRENT_STATUS.md). This backlog is
planning only, not implementation or deployment evidence. Physical tests remain
postponed until the separate software work is ready for consolidated acceptance.
