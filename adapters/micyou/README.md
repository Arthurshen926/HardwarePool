# MicYou Adapter

This crate is CapyIO's bounded process boundary for the pinned MicYou v2.0.1
CLI. MicYou remains a separate GPL-3.0-only program and owns its private
TCP/UDP audio transport, decoding, jitter buffering and DSP. No MicYou source,
schema, library or binary is copied, linked or distributed here.

The current slice:

- verifies the exact `micyou-cli 2.0.1` version;
- enumerates a bounded output-device inventory and requires an exact device;
- starts Wi-Fi mode with an explicit IPv4 bind address and port;
- drains child output with bounded retention and owns stop/reap behavior;
- maps the Adapter-managed route to CapyIO's shared voice audio policy.

It does not yet provide a CapyIO virtual microphone. A future Gate must add the
Windows capture endpoint and a supported user-mode PCM ingress. Do not run
`micyou-cli mics --install` from this Adapter: that performs a separate driver
installation and needs its own approval and provenance review.
