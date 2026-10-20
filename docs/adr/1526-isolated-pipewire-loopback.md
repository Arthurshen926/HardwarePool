# ADR 1526: Isolated hosted PipeWire PCM loopback

- Status: Accepted
- Date: 2026-10-20

## Context

Rust-only playback fixtures and ARM64 Android compilation do not prove that the
Linux adapter's fixed `/usr/bin/pw-cat` invocation works with an actual PipeWire
graph. A headless runner can verify software audio without a microphone, speaker,
GUI, Android device, credentials or any user server access.

## Decision

Add a separate, explicit hosted integration job, with a 20-minute job limit. It
installs only Ubuntu 24.04's official `pipewire-bin` and required dependencies,
using `--no-install-recommends`. It does not install a session manager, enable or
start a system service, change system audio configuration, accept an SDK license,
configure credentials or install anything on a user machine. Package versions are
logged. Missing tools fail rather than skip. No new production dependency or
production default is introduced.

The Python harness creates a private temporary runtime/config directory, writes
its own daemon and client configurations, and starts only its own PipeWire daemon.
The configuration includes a dummy clock driver and mono null sink, but no ALSA
hardware node, discovery, Pulse server, DBus/realtime module, session manager or
external network transport. Environment inheritance is deliberately restricted.
Private client `stream.properties` configure DSP ports without a session manager.
The harness identifies exact unique nodes/ports and manually creates two links:

`production pw-cat output -> null sink input -> null sink monitor -> recording pw-cat`

The explicit Rust example uses the actual production `PlaybackPump` and
`PipeWirePlayback`, not a substitute sender. It creates generated mono 48 kHz
S16LE packets: two seconds at 1 kHz, one second of silence, two seconds at 2 kHz.
It requires explicit invocation, matching private runtime variables and marker
files. It queues no PCM until the harness has confirmed links. The fixture
asserts packet/byte counts and explicit owned pw-cat reaping.

The monitor is read as raw PCM. The independent Python oracle requires sustained,
ordered 1 kHz, silence and 2 kHz segments, with minimum signal energy and low
cross-frequency energy. Byte count or nonzero samples alone cannot pass. Pure
negative tests reject wrong frequency/order, silence-only, noise, clipping,
short signals and oversized input. Leading silence and partial windows are
allowed, avoiding assumptions about scheduler latency.

The harness has a 30-second total operation deadline, two-second graph-command
waits, kernel-enforced two-MiB regular-file output limits per child stream,
and finite readiness waits. Cleanup terminates/reaps only owned process groups,
including error paths, then removes the temporary directory. The hosted command
also has an outer 60-second timeout. A success marker is emitted only after
waveform validation and cleanup. No broad process killing or global service
operations are used.

## Evidence boundaries

Until the exact-head hosted job succeeds, this is a prepared integration test,
not a PipeWire acceptance result. A successful run establishes real software
PCM traversal and process cleanup on the tested distro/package version. It does
not exercise UDP transport, the authenticated command, Android capture, physical
speaker output, Bluetooth, latency quality, reconnect, or real-device pairing.
Those remain distinct gates; the lab UDP media protocol remains unauthenticated.
The example is not a new application command or deployment path.

Local validation is compilation and pure tests only. Local sandbox IPC denial is
not bypassed; actual graph execution is confined to the disposable hosted job.

## Upstream references

- [Ubuntu Noble pipewire-bin dependencies](https://packages.ubuntu.com/noble/pipewire-bin)
- [PipeWire 1.0.5 daemon example](https://github.com/PipeWire/pipewire/blob/1.0.5/src/daemon/minimal.conf.in)
- [PipeWire 1.0.5 stream properties](https://github.com/PipeWire/pipewire/blob/1.0.5/src/pipewire/stream.c)
- [PipeWire 1.0.5 adapter port configuration](https://github.com/PipeWire/pipewire/blob/1.0.5/src/modules/module-adapter/adapter.c)
- [PipeWire 1.0.5 pw-cat raw stdin/stdout](https://github.com/PipeWire/pipewire/blob/1.0.5/src/tools/pw-cat.c)

Configurations are project-authored minimal test settings based on documented
interfaces, not vendored upstream code. Upstream `minimal.conf` is deliberately
not executed unchanged because it contains hardware-node examples.
