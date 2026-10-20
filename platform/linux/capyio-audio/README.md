# Experimental Linux lab PCM playback

This is the first Linux real transport/endpoint implementation, not a mock batch.
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
