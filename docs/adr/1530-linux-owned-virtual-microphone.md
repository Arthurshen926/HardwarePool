# ADR 1530: Explicit owned PipeWire virtual microphone

Status: experimental candidate; exact-head hosted graph acceptance pending.
Date: 2026-10-20
Base: ff0f0a261a53e26fc2260ca7d09033f5be648299

## Scope and direction

The existing bounded Linux audio launcher accepts `endpoint_mode` independently
of `proposal_mode`. Omission or `"playback"` preserves explicit-target playback.
`"virtual_microphone"` rejects an externally supplied non-null `target` and derives
`capyio.virtual-microphone.<local Sink Port UUID>` from its fresh local catalog.
Unknown modes fail closed. The standalone trusted-lab playback CLI is unchanged.

This mode advertises a Microphone capability with a network **Sink** Port: it
consumes Android microphone PCM. Its local native PipeWire projection is an
**Audio/Source**, which an application may explicitly select. Network direction
and OS projection direction are not interchangeable. This is not Linux capture,
reverse audio, a PulseAudio/ALSA compatibility promise, or a default-device change.

## Ownership and authority

The approval record includes endpoint mode and exact generated node name, in
addition to the existing Route, endpoint, nonce and finite-lifetime bindings.
The paired TLS Session, independent Android and Linux approvals, finite grants,
exact Source/Sink leases and reciprocal media offer remain mandatory. Mode is not
permission. The owner and worker both revalidate the name against the admitted
Route's local Sink. No process or source is opened merely by catalog discovery,
a Draft, or the local approval event.

Only the existing fixed `/usr/bin/pw-cat` process boundary is used. Virtual mode
sends raw mono S16LE/48 kHz PCM to stdin with `--playback`, `--target=0` and explicit
`media.class=Audio/Source`, `node.virtual=true`, and the generated `node.name`.
The child removes `PIPEWIRE_NODE`, which could otherwise override targeting;
private PipeWire remote/configuration environment remains usable for isolation.
No global service, module, null-sink monitor, hardware device or default is
created or changed. No new production dependency is introduced.

The existing nonblocking one-packet pump, finite stall budget, child supervision,
lease/grant expiry and Session invalidation remain applicable. Stop closes UDP
and stdin, terminates/reaps only the owned child and cannot authorize a restart.
An independent application may read the source only during the approved lifetime.

## Required acceptance and limitations

Pure tests cover default/mode validation, rejected target overrides, generated
identity, catalog class/direction, no authority from catalog construction, exact
arguments/environment, wrong identity, expiry and missing lease. The existing
private PipeWire gate must additionally prove a real `Audio/Source` node owned by
the launcher's pw-cat process, output ports linked directly to an independent
recorder, ordered injected PCM readback, no source before consent, and bounded
Stop with owned node/ports/links gone while unrelated graph objects remain.
No-consumer Stop, early owned-child exit and private-server disconnect must be
bounded; synthetic full-stdin tests remain the separate backpressure proof.

A null-sink monitor with a microphone label is not acceptance. Neither correct
arguments nor an ignored integration test proves a source works. Local execution
is blocked by unavailable pw-cat, sandbox IPC restrictions and an uncached Rust
dependency; hosted exact-head results are required before software readiness is
reported. Physical Android capture, user-session compatibility and real-device
acceptance remain postponed and unclaimed. Native UDP remains the existing
unencrypted, unauthenticated trusted-lab transport.

## Upstream basis

The [pw-cat manual](https://docs.pipewire.org/page_man_pw-cat_1.html) documents
stream properties and target selection. The pinned upstream
[pw-cat 0.3.65 source](https://raw.githubusercontent.com/PipeWire/pipewire/0.3.65/src/tools/pw-cat.c)
clears autoconnect for target `0`; the corresponding
[stream implementation](https://raw.githubusercontent.com/PipeWire/pipewire/0.3.65/src/pipewire/stream.c)
retains an explicitly supplied media class. These support the candidate design,
not a substitute for the hosted topology and PCM acceptance gate. No upstream
code is imported or modified.
