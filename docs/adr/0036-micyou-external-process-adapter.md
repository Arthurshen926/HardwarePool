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
default output device. The approved local build therefore applies a narrow GPL
patch that preserves enumeration order and exposes the machine-readable
`device-stable-id-v1` capability. On Windows, it pairs the current CPAL output
inventory with bounded Core Audio endpoint IDs. Persisted configuration
contains the stable endpoint ID plus its expected name; a one-based CPAL index
is only a freshly resolved launch coordinate. The CLI validates the complete
ID/index/name tuple before and after audio startup and fails closed on missing,
duplicate, renamed or reordered identity. No MicYou source, patch, Rust crate,
executable, installer or APK is copied, linked or distributed by CapyIO in this
Gate.

The controlled-lab build also tightens MicYou's Windows mode lock. Upstream
records a PID and timestamp but treats any live process that later reuses the
PID as the old MicYou instance. The local patch compares the current process
creation time with the recorded timestamp before accepting the lock as live.
This permits a supervised hard stop followed by an explicit retry without
deleting an unverified lock or trusting PID identity alone.

The Adapter verifies the exact CLI version and patched capability, enumerates a
bounded output-device inventory without deduplicating display names, resolves
the configured endpoint ID to the current index, and launches Wi-Fi mode
directly with explicit IPv4 bind, TCP port and output-device arguments. It
re-probes immediately before spawn, while the CLI repeats validation around
audio startup, so inventory drift fails before decoded audio can reach the
wrong device. It does not use a shell or invoke `mics --install`. Probe/start
deadlines, retained child output, device count, endpoint-ID size and device-name
size are bounded. The Adapter owns child stop and reap behavior.

The desktop Route controller stops and reaps the receiver when an already
active phone TCP session disappears. This is a fail-closed data-plane cutoff:
the bounded capture ring may drain frames already committed to an active
Windows capture graph, then yields exact silence. An explicit retry advances
the Route epoch and starts a new receiver. Initial `Starting` state still keeps
the listener alive for its bounded phone-connection window.

The route is `AdapterManaged` and maps semantically to
`AudioStreamSpec::voice_interactive()`. MicYou retains its private negotiation,
codec, FEC, jitter, DSP and TCP/UDP bytes, so the Adapter does not claim a
`capyio.audio.frames/1` wire or expose MicYou media on the control path.

ADR 0042 later formalizes this as an opaque compatibility backend. CapyIO can
associate the process with one validated conservative voice Route/epoch for
lifecycle, but exposes no `AudioMediaPacket` API and declares common identity,
payload and exact private codec negotiation unobservable or absent.

The paired CapyIO Windows capture endpoint and bounded PCM ingress are specified
separately by ADR 0037. They do not change MicYou's private wire contract.

## Consequences

- GPL and upstream real-time behavior remain isolated in a separate process;
- CapyIO can reuse the shared audio policy without duplicating Speaker media
  contracts;
- the local CLI must be rebuilt from the pinned GPL source with the reviewed
  fail-closed selector patch and a direct Windows API dependency already
  present transitively at `windows` 0.61.3 (MIT OR Apache-2.0); public
  patch/binary distribution remains pending legal and release review;
- redistribution, signing, APK installation and driver deployment require
  separate evidence and approval.
