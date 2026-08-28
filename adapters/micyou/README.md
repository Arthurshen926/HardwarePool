# MicYou Adapter

This crate is CapyIO's bounded process boundary for the pinned MicYou v2.0.1
CLI. MicYou remains a separate GPL-3.0-only program and owns its private
TCP/UDP audio transport, decoding, jitter buffering and DSP. No MicYou source,
schema, library or binary is copied, linked or distributed here.

The current slice:

- verifies the exact `micyou-cli 2.0.1` version and the CapyIO
  `device-index-v1` capability;
- preserves a bounded output-device inventory, including duplicate display
  names, and requires both a one-based index and its expected name;
- starts Wi-Fi mode with an explicit IPv4 bind address and port;
- drains child output with bounded retention and owns stop/reap behavior;
- maps the Adapter-managed route to CapyIO's shared voice audio policy.

The adapter now targets the CapyIO-owned paired Windows microphone endpoints,
but ordinary-application and phone audio evidence remains pending. The pinned
upstream CLI must be locally rebuilt with the reviewed fail-closed device-index
patch; an unmodified v2.0.1 binary is rejected. Do not run
`micyou-cli mics --install` from this Adapter: that performs a separate driver
installation and needs its own approval and provenance review.
