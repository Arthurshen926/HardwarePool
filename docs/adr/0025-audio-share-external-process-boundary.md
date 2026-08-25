# ADR 0025: Keep Audio Share behind an external process boundary

- Status: Accepted
- Date: 2026-08-24

## Context

Gate 7 needs a Windows-system-playback to Android-speaker proof. Audio Share
already implements Windows WASAPI loopback capture, TCP negotiation, UDP PCM
delivery and Android `AudioTrack` playback. Reimplementing or copying pieces of
that stack would duplicate the highest-risk real-time and mobile lifecycle
work. The official v0.3.4 Windows binary is not Authenticode signed, and the
project is not yet ready to distribute third-party binaries or APKs.

The Product Requirements previously listed every real Audio Share integration
as a current non-goal, while the authorized Roadmap and Backlog made Gate 7 the
next active slice. That conflict must be resolved before implementation.

## Decision

Pin Audio Share v0.3.4 at commit
`342751fe675367483170b002ec6054e243966dc0` under Apache-2.0 and integrate only
an unmodified, user-supplied release through a CapyIO-authored Adapter/process
controller.

The controller launches an allow-listed executable directly with explicit
arguments; it never invokes a command shell. It requires an explicit IP-literal
bind address, non-zero port and enumerated playback endpoint instead of relying
on Audio Share's default interface selection. Probe/log reads, retained
diagnostics, startup and shutdown all have fixed bounds and deadlines. The
upstream TCP control and UDP PCM data plane remain Adapter-private and never
enter CapyIO Sidecar stdout or node-control envelopes.

The first Route is `AdapterManaged`. It may describe only the pinned Audio Share
contract and does not advertise a general `capyio.audio.frames/1` StandardPort.
Upstream child/receiver failure maps to a structured Runtime Problem and affects
only the owned Route. A retry requires a fresh Route epoch.

CapyIO does not vendor, link, repackage, sign or distribute the upstream
Windows binary or Android APK in this slice. Physical tests use separately
authorized, hash-verified artifacts. The unsigned-binary and Android
background/audio-focus risks remain explicit release blockers.

## Alternatives

- implement a new PCM network stack and Android receiver;
- import or vendor selected upstream source;
- treat Audio Share as a generic StandardPort audio transport;
- launch the CLI through a shell or accept its default network interface;
- bundle the official binaries immediately.

## Consequences

The first speaker path can reuse proven upstream behavior while Core, Runtime
and UI retain typed lifecycle and diagnostics. The integration remains local
lab/pre-alpha, cannot claim interoperability with unrelated audio Adapters, and
requires later packaging, signing, security and long-duration mobile tests
before public distribution.

## 2026-08-25 clarification

Physical testing under Windows Remote Desktop showed that v0.3.4 can accept an
explicit 48 kHz signed-16 request yet fall back to the selected endpoint's 44.1
kHz float default. The Adapter has no machine-readable API for the negotiated
result, and ordinary log prose cannot become lifecycle or catalog authority.
The Route format is therefore `audio-share-v0.3.4-private-negotiated`; requested
CLI arguments remain bounded configuration but are not presented as observed
PCM truth. The desktop host also bounds receiver-wait polling and reports a
retryable offline Problem while reaping the process when that budget is
exhausted.
