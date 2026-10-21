# ADR 1531: Explicit Linux stereo Source for Android playback

Status: implemented candidate; hosted software acceptance pending.
Date: 2026-10-21
Base: 9473e9dbc2c8b46378a55a6841e81ea191f45d0e

## Bounded command and direction

`capyio-node linux-capture --trusted-lab` is a separate, one-shot launcher. Its
version-1 configuration uses `sink` for the exact paired Android Speaker Port,
`target` for one explicit native PipeWire capture node name or serial, and the
existing `node_id`, `listen`, `media`, and finite `duration_secs` fields. Unknown
fields, implicit/default targets, and playback/incoming proposal options fail
closed. It does not enumerate or capture the default desktop or microphone.
The existing `linux-playback` schema and default endpoint remain unchanged.

The local capability is Speaker with a network Source Port, matching the existing
Windows-to-Android stereo contract. Its advertised and negotiated format is
`audio_frames_v1`, `pcm-s16le-48000-stereo`, Interactive, AdapterManaged. The Source
is included in the publication selection; publication itself grants no access.
This first slice is Linux-led, not Android-initiated reverse negotiation.

## Authority and resource lifetime

The configured paired TLS peer supplies the authenticated catalog and peer media
address. The approval event binds the actual Route, Source, Sink, stereo capture
mode, explicit target, nonce and finite lifetime. Independent Android approval,
finite bilateral grants, the exact local Source and remote Sink leases, reciprocal
media offers, and Start remain required. No capture child is opened on discovery,
Draft, or local consent alone. Each restart needs fresh stream identities and
consent. A normal Busy update is accepted only for the exact started remote Port.

Only the fixed `/usr/bin/pw-cat --record` boundary is used, with 48 kHz S16LE,
two channels and explicit stereo channel mapping. `PIPEWIRE_NODE` is removed;
no-fallback, no-reconnect and no-move properties preserve the selected target.
A bounded 512-byte/500-ms version probe accepts only matching compiled and linked
1.0.5 (legacy raw stdout with `-`) or 1.6.9 (explicit `--raw` plus `-`). Unknown or
mixed versions fail closed; no AU/WAV container is accepted by this contract. The
fixed executable metadata must stay unchanged across probe and spawn, and current
finite authority is rechecked after probing, before opening capture. These exact
contracts follow upstream [1.0.5](https://github.com/PipeWire/pipewire/blob/1.0.5/src/tools/pw-cat.c)
and [1.6.9](https://github.com/PipeWire/pipewire/blob/1.6.9/src/tools/pw-cat.c) source;
they do not claim general version compatibility.
Private PipeWire remote/configuration variables remain available for isolated
acceptance. There is no device installation, service or global default change.

The owned nonblocking stdout has one fixed 1920-byte packet assembly buffer:
480 frames per 10 ms packet. Short reads preserve exact frame order; truncated
EOF, child exit and an overdue partial packet fail closed. Each owner poll reads
at most once and sends at most one packet. Control is drained before media work.
Fresh binding/grant/lease checks occur before and after reads and before each UDP
fragment. A configured 1 ms send timeout and finite expiry headroom bound the send
attempts; OS scheduling is not a real-time guarantee. Revocation, Stop, expiry or
failure discard partial PCM, close UDP and reap only the owned child. A stale
owner cannot stop or release a newer stream epoch.

## Required software evidence and limits

The hosted private PipeWire gate must explicitly run all five scenarios: waveform,
capture child exit, server disconnect, control disconnect and finite expiry.
The waveform case creates a synthetic native Audio/Source with distinct left and
right tones, explicitly links its ports to the production capture child inside
the private graph, and checks ordered stereo PCM from production UDP packets.
Android's actual Rust catalog/consent coordinator and shared native-LAN binding
and receiver guard delivery to a simulated consumer. This is not Java AudioTrack
execution or physical audibility. Fresh valid post-Stop packets must not reach
the simulated consumer; child, UDP and surviving graph cleanup are also checked.

Pure packetizer/oracle tests and compilation do not establish real graph capture.
The new hosted cases remain pending until their exact pass markers are observed.
The existing mono playback, virtual Source, incoming-Draft policy, ARM64/JNI and
Java contract gates remain required. UDP remains an unauthenticated trusted-lab
transport, not production secure media. Native PipeWire only is in scope; no
PulseAudio/ALSA compatibility, general pairing UI, auto-discovery or physical
Android/AudioTrack result is claimed. Physical testing remains deferred.
