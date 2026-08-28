# ADR 0036: Keep MicYou v2.0.1 behind an external process Adapter

Status: accepted

## Context

Gate 8 needs Android microphone audio to reach a Windows system capture
endpoint. MicYou v2.0.1 already implements Android capture, a private TCP/UDP
session, PCM and Opus modes, bounded network/FEC/jitter structures and optional
voice DSP. Its code is GPL-3.0-only while CapyIO is Apache-2.0. The upstream
desktop decoded-PCM handoff also currently uses an unbounded standard channel,
and its Windows release contains an installer rather than a standalone CLI
artifact.

CapyIO's shared audio contract must not imply that the private MicYou wire is a
StandardPort. Installing MicYou's VB-CABLE dependency from its CLI is a driver
operation outside ordinary Adapter lifecycle ownership.

## Decision

Pin MicYou v2.0.1 at commit
`b22c41fff3d3d1169c04c8acd1db7266cf9d4c62` and integrate only a user-supplied,
unmodified CLI through a CapyIO-authored external-process Adapter. No MicYou
source, Protobuf schema, Rust crate, executable, installer or APK is copied,
linked or distributed in this Gate.

The Adapter verifies the exact CLI version, enumerates a bounded output-device
inventory, requires an exact device-name match and launches Wi-Fi mode directly
with explicit IPv4 bind, TCP port and output device arguments. It does not use a
shell or invoke `mics --install`. Probe/start deadlines, retained child output,
device count and device-name size are bounded. The Adapter owns child stop and
reap behavior.

The route is `AdapterManaged` and maps semantically to
`AudioStreamSpec::voice_interactive()`. MicYou retains its private negotiation,
codec, FEC, jitter, DSP and TCP/UDP bytes, so the Adapter does not claim a
`capyio.audio.frames/1` wire or expose MicYou media on the control path.

A separate Gate will provide the CapyIO Windows capture endpoint and a bounded
user-mode PCM ingress. Until that exists, the process Adapter alone is not a
usable `CapyIO Microphone` system device.

## Consequences

- GPL and upstream real-time behavior remain isolated in a separate process;
- CapyIO can reuse the shared audio policy without duplicating Speaker media
  contracts;
- exact upstream reproduction is possible, but a CLI must currently be built
  from the pinned source rather than taken from an official standalone asset;
- no ordinary Windows application can record this path until the capture
  endpoint/PCM ingress Gate is complete;
- redistribution, signing, APK installation and driver deployment require
  separate evidence and approval.
