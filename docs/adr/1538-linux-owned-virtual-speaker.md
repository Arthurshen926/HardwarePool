# ADR 1538: Owned Linux virtual speaker feeding the Android stereo Route

Status: bounded native software graph accepted at `1ad1e97e`; physical acceptance deferred.
Date: 2026-10-21
Base: 6257da2e9bd77c05dc12b0f3de458bc4d50b330f

## Scope and direction

The existing `capyio-node linux-capture --trusted-lab` configuration gains
`endpoint_mode: "virtual_speaker"`. It rejects a non-null external `target`.
Omission of endpoint mode preserves the existing explicit selected-source path;
`"selected_source"` names that same path explicitly. Playback configuration and
its defaults do not accept this capture-only mode.

The fresh local catalog owns a Speaker capability and network stereo Source
Port. The projection is an application-selectable PipeWire `Audio/Sink` with
actual input ports, not a monitor, default device, module or renamed null sink.
Its name is `capyio.virtual-speaker.<local Source Port UUID>`. The approval record
shows the mode and generated name with the exact Route, endpoints, nonce and
finite lifetime. No physical source is selected or captured by virtual mode.
The network destination remains the exact admitted Android Speaker Sink.

`CaptureEndpoint::SelectedSource(String)` and `VirtualSpeaker(PortId)` keep
external target selection separate from owned projection identity. Both the
control owner and capture worker bind the virtual name to the admitted Route's
local Source. The latter produces the typed endpoint rather than treating the
name as an external capture target. No new dependency or wire format is added.

## Authority and fixed helper

Existing pinned control, independent consent, finite grants, both exact leases,
and reciprocal data readiness remain prerequisites. Only then does the worker
open `/usr/bin/pw-cat --record --target=0` with `Audio/Sink`, `node.virtual=true`,
`node.autoconnect=false`, the generated name, no-fallback/no-reconnect/no-move,
48 kHz S16LE, two channels and explicit stereo positions. PCM is stdout only.
The exact bounded probe retains 1.0.5 implicit raw `-` and 1.6.9 explicit
`--raw -`; unsupported/mixed versions fail closed. Authority is rechecked after
the probe and before capture opens. Existing CapturePump, finite assembly bounds,
fragment checks and owned child cleanup are reused unchanged.

A shared fixed-command builder removes six upstream stream environment overrides
for playback, virtual microphone, selected capture and virtual speaker:
`PIPEWIRE_NODE`, `PIPEWIRE_AUTOCONNECT`, `PIPEWIRE_PROPS`, `PIPEWIRE_QUANTUM`,
`PIPEWIRE_LATENCY`, and `PIPEWIRE_RATE`. Upstream applies these after stream rules
and can otherwise replace explicit properties. See upstream stream.c
[1.0.5 lines 1917–1940](https://github.com/PipeWire/pipewire/blob/1.0.5/src/pipewire/stream.c#L1917-L1940)
and [1.6.9 lines 2003–2025](https://github.com/PipeWire/pipewire/blob/1.6.9/src/pipewire/stream.c#L2003-L2025).
The [1.0.5 target parser](https://github.com/PipeWire/pipewire/blob/1.0.5/src/tools/pw-cat.c#L1634-L1639)
and [1.6.9 target parser](https://github.com/PipeWire/pipewire/blob/1.6.9/src/tools/pw-cat.c#L2166-L2171)
clear autoconnect for target zero. No upstream source is imported.

Private remote/runtime/config routing remains intentionally inherited for isolated
CI. This scrub is not a sandbox against arbitrary trusted PipeWire configuration,
server policy, dynamic libraries or an adversarial invoker. No global metadata,
defaults, user configuration or service settings are modified.

## Required evidence and limits

Unit tests cover both endpoint types, exact commands, raw-version contracts,
all six removals on existing playback/virtual microphone paths, strict capture
schema, derived identity, network direction, missing lease and expiry rejection.
The private hosted graph must independently link a synthetic application Source
to actual owned Sink input ports, then inspect the production capture-to-packetizer
and dynamic UDP path through Android's actual Rust policy/shared receiver.
Distinct left/right waveform, silence and ordering assertions remain mandatory.

The added virtual-speaker scenarios cover waveform, no-input Stop, child exit,
server loss, control loss and expiry. Poisoned inherited overrides must not alter
class, owned name, target/autolink policy or rate. Exact client/PID/PPid, direct
ports, no monitor/autolinks and owned cleanup must be checked, preserving unrelated
nodes when the server survives. Previous playback, virtual microphone, selected
capture, camera, AVC, secure camera and touchpad gates remain intact.

Local graph execution is not attempted under the known socket restriction.
Compilation and pure tests are not real graph evidence; exact-head hosted
acceptance is recorded below. This is software-only native PipeWire,
not physical phone/AudioTrack output or general desktop application compatibility.
UDP is still the existing unauthenticated, unencrypted trusted-lab transport.
EOF, Stop, expiry or failure require fresh admission to restart. Physical testing
remains deferred.


## Software acceptance — 2026-10-21

Implementation `1ad1e97e6cf57067177a5e86100dccccba17360a` passed all seven
required CI jobs. The private PipeWire gate in
[run 39579126851](https://github.com/Arthurshen926/HardwarePool/actions/runs/39579126851)
passed all six virtual-speaker scenarios: `waveform`, `no_input`, `child_exit`,
`server_disconnect`, `control_disconnect`, and `expiry`.

This proves the owned native Audio/Sink input ports and independent synthetic
stereo application path through the production capture/packetizer/dynamic UDP
sender into Android's actual Rust policy/shared receiver with simulated consumer.
The gate checks separate left/right waveform, silence/order, poisoned stream
environment overrides, exact owned identity/parentage, no autolinks and bounded
cleanup, retaining unrelated graph objects when the server survives. The previous
selected-source and other required regression gates remain green.

This result does not establish physical Android AudioTrack playback, phone or
general desktop-application compatibility. UDP remains unauthenticated and
unencrypted, restricted to the explicitly opted-in trusted lab. No user default,
physical capture, module installation or system-setting change is implied.
