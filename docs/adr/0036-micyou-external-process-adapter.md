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
`b22c41fff3d3d1169c04c8acd1db7266cf9d4c62` and keep it behind a
CapyIO-authored external-process Adapter. The initial unmodified CLI contract
was insufficient on Windows because CPAL exposes duplicate localized endpoint
names and upstream selected the first match or silently fell back to the
default output device. The approved local build therefore applies a narrow
GPL patch that preserves enumeration order, accepts a one-based device index,
verifies its expected name and fails closed on inventory drift. It also exposes
the machine-readable `device-index-v1` capability. No MicYou source, patch,
Rust crate, executable, installer or APK is copied, linked or distributed by
CapyIO in this Gate.

The Adapter verifies the exact CLI version and patched capability, enumerates a
bounded output-device inventory without deduplicating display names, requires
an exact index/name pair and launches Wi-Fi mode directly with explicit IPv4
bind, TCP port and output device arguments. It re-probes immediately before
spawn, so endpoint re-enumeration fails before audio can be sent to the wrong
device. It does not use a shell or invoke `mics --install`. Probe/start
deadlines, retained child output, device count and device-name size are bounded.
The Adapter owns child stop and reap behavior.

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
- the local CLI must be rebuilt from the pinned GPL source with the reviewed
  fail-closed selector patch; public patch/binary distribution remains pending
  legal and release review;
- no ordinary Windows application can record this path until the capture
  endpoint/PCM ingress Gate is complete;
- redistribution, signing, APK installation and driver deployment require
  separate evidence and approval.
