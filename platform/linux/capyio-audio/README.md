# Standalone experimental Linux lab PCM playback

This section describes the original ADR 1518 standalone CLI, not the full current
audio library/launcher. The later explicit launcher adds paired admission and an
owned virtual Audio/Source; see the [current Linux boundary](../README.md).

The standalone CLI is a real transport/endpoint implementation, not a mock batch.
It reuses Android's **manual native microphone lab** wire and launches an already
installed `/usr/bin/pw-cat` for raw mono 48 kHz S16LE playback. It does not create
a system microphone, pair peers, negotiate Routes or integrate the Android
application's dynamic audio workflow. Actual playback has not been device-tested.

Build without opening sockets or devices:

```text
cargo build --locked -p capyio-linux-audio
```

For a separately authorized trusted-lab test only, the CLI shape is:

```text
capyio-linux-audio-playback --trusted-lab <explicit-local-ip:port> <expected-Android-ip:port> <seconds:1..300> <PipeWire-node-name-or-serial>
```

Keep inherited stdin open while running. EOF or any byte requests Stop. The
Android manual lab sender must use reciprocal ports and the existing fixed
microphone binding. This is not permission to start Android capture; its visible
consent and lifecycle still apply. Default `capyio-node linux-host` is unaffected.
No installation, module loading, permission or system-setting changes are done.

WARNING: UDP media is unauthenticated and unencrypted. The lab opt-in, fixed IDs
and expected/private IP do not authenticate packets or prevent replay between
launches. Do not expose this listener on untrusted networks. A paired control
connection would not make the existing UDP secure. Counts mean received packets
and bytes forwarded to a local pipe, not audible sound or rendered frames.

See [ADR 1518](../../../docs/adr/1518-linux-audio-playback.md) for limits and
[LINUX-A06](../../../docs/plans/active/LINUX-A06-audio-playback.md) for subsequent
virtual microphone, control/security and physical acceptance gates.

## Dynamic Runtime worker (library only)

`RuntimePlayback::open_trusted_lab` connects an already admitted Starting Route
to this real UDP/PipeWire endpoint using its dynamic binding and local Sink
lease. The owner must poll it, apply authenticated control/local consent changes
to its Runtime, and call Stop on shutdown. It does not authenticate peers, grant
consent, create leases or expose a new host command. Paused polling forwards no
PCM but does not independently close resources. See
[ADR 1519](../../../docs/adr/1519-linux-runtime-audio-worker.md) for exact caller,
security, cleanup and evidence boundaries. The CLI above is still static-lab-only.

## Accepted authenticated control owner (library only)

`AuthenticatedPlaybackOwner::from_accepted_trusted_lab` consumes a real accepted
TLS channel plus the host's exact expected paired binding and an already
locally authorized single-Route Runtime. It reads reciprocal offers only through
that channel, derives peer IP from the socket and starts the dynamic worker.
It creates no credentials, listener, grants or leases. Stop/EOF/timeout closes
its dedicated Session. Calls remain caller-driven: a control receive pauses
media polling for at most its absolute deadline (one-second cap, owner lifetime,
grant and lease), with trickled framing interrupted by connection shutdown.
Timeout is terminal. This is not complete Linux pairing or
continuous scheduled playback. UDP remains unencrypted/unauthenticated. See
[ADR 1520](../../../docs/adr/1520-linux-authenticated-audio-owner.md).

For a continuously supervised embedding, use the explicit `LinuxPlaybackHost`
entry from `capyio-linux-host` with an already authenticated/admitted owner.
The owner's `poll` fairly advances bounded incremental control and media without
blocking on idle control. Existing blocking calls remain available. See ADR 1521
for cancellation/expiry semantics and unfinished executable admission steps.

## Application-selectable virtual speaker

The finite `capyio-node linux-capture --trusted-lab` launcher accepts
`endpoint_mode: "virtual_speaker"` with no external `target`. After independent
consent and exact finite Route authority it exposes an owned native `Audio/Sink`
and sends selected application stereo PCM to the admitted Android Speaker Sink.
Default selected-source capture is unchanged. See
[ADR 1538](../../../docs/adr/1538-linux-owned-virtual-speaker.md) for authority,
shared environment hardening, pending hosted evidence and compatibility limits.
