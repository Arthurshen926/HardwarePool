# CAPY-AUDIO-NATIVE-001B — Compatibility-backend report

Date: 2026-08-31

Status: complete locally

## Outcome

Audio Share and MicYou now expose validated compatibility-backend contracts
above their unchanged private protocols. The declaration covers stable backend
identity, `AdapterManaged`/`StandardPort` semantics, full-packet/PCM-only/opaque
access, PCM/Opus support, per-field metadata fidelity and observable transport
security.

The validator prevents a backend from claiming StandardPort audio unless it
sees the full common packet and preserves every common field exactly. It also
prevents PCM-payload-only or opaque backends from overstating payload/codec
visibility and rejects inconsistent security claims.

## Audio Share mapping

`AudioShareMediaSender` binds the existing private sender to one validated
`AudioMediaStreamBinding`. Every packet must match its Stream, epoch and exact
selected PCM specification. Only after that validation does the wrapper copy
the PCM payload into the existing bounded queue.

The compatibility declaration records:

- `AdapterManaged`, PCM-payload-only access;
- exact PCM payload bytes and partial selected-format mapping;
- absent Session/Route, Stream, epoch, sequence, source time, sample timeline
  and discontinuity on the private wire;
- no peer authentication, confidentiality, integrity, replay protection or
  downgrade binding.

The existing loopback TCP/UDP test now enters through this common packet
wrapper. It negotiates the pinned v0.3.4 format, associates UDP, preserves the
1,920-byte stereo PCM packet and segments it below the private datagram bound.
Wrong Stream and mismatched mono/stereo binding are rejected before enqueue.

## MicYou mapping

MicYou remains an opaque external-process backend. CapyIO validates and retains
one conservative voice Session/Route/Stream/epoch association for lifecycle,
but deliberately exposes no `AudioMediaPacket` send/receive API. Its contract
records private PCM/Opus capability, partial semantic stream mapping, opaque
media/timing internals, absent CapyIO identity on the wire and no production
security.

This distinction is material: the microphone compatibility path cannot be made
native merely by changing a type name. Android capture, codec/packet access and
network transport must move into CapyIO-owned components.

## Changed implementation

- `crates/capyio-audio/src/backend.rs` — typed contract, fidelity/access/
  encoding/security model and fail-closed validation;
- `adapters/audio-share/src/transport.rs` — Audio Share declaration and bound
  common PCM packet sender;
- `adapters/micyou/src/lib.rs` — opaque declaration and conservative lifecycle
  media binding;
- ADR 0042 plus architecture, Adapter, Profile, data-plane, protocol, security,
  testing, roadmap, backlog, build-status and traceability updates.

No dependency, codec, third-party source, private wire byte, driver, APK or
platform permission was added or changed.

## Automated evidence

- targeted audio/Audio Share/MicYou suites: 65 passed, 4 explicitly ignored
  external/physical tests;
- targeted Clippy with warnings denied: PASS;
- Audio Share common-packet/private TCP+UDP loopback: PASS;
- `cargo xtask validate-docs`: PASS — 88 unique Requirement IDs;
- `cargo xtask ci`: PASS — format, workspace check, Clippy, 173 passed Rust
  tests with 4 explicit ignored tests, deterministic IMU demo, repository/docs
  validation, 2 manifests, Adapter smoke, desktop typecheck and Vite build.

No physical audio, phone, Android package, Windows service or driver action was
needed. This report does not repeat the already retained human-audibility
evidence for either compatibility path.

## Remaining work

`CAPY-AUDIO-NATIVE-001C` creates the single CapyIO Android Node/service shell
and native speaker Sink/microphone Source platform boundaries. Adding Android
microphone and foreground-service declarations remains a separately explicit
high-risk approval. `001D` then implements and measures a replaceable native
network backend before either compatibility application stops being the default
physical path.

Production authentication/encryption/replay/downgrade protection, real-device
background/focus/permission behavior, native codec quality, latency/soak,
simultaneous duplex and 96 kHz/multichannel evidence remain unproven.
