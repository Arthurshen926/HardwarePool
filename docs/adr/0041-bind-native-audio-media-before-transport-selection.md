# ADR 0041: Bind native audio media before selecting a network transport

Status: accepted

## Context

The Windows-to-Android Speaker and Android-to-Windows microphone paths are
functionally proven, but their live media still uses two unrelated private
compatibility protocols and two Android applications. ADR 0035 already gives
both directions one semantic audio model. It does not define the seam between
a platform capture/render engine and a concrete media transport.

Copying one compatibility wire into the other direction would preserve the
wrong product boundary. Selecting AOO, RTP, QUIC or a custom wire before a
common seam exists would instead leak that candidate into platform and product
code. A public binary format also cannot be declared production-ready before
authentication, replay, MTU and downgrade policy are designed and tested.

## Decision

Add a direction-neutral media-channel contract to `capyio-audio`:

- `AudioMediaStreamBinding` binds one typed Session, directed Route, Stream,
  positive epoch and exact validated `AudioStreamSpec`;
- `AudioMediaPacket` carries the timing/sample metadata and a bounded payload
  whose interpretation is fixed by the binding's selected encoding;
- PCM frames convert losslessly to and from packets, while encoded packets are
  representable without putting a codec implementation in the shared crate;
- `BoundedAudioPacketQueue` is a deterministic worker-thread reference boundary
  with explicit packet-count and aggregate-byte limits.

The binding contains no Source/Sink or microphone/Speaker role. Direction
continues to belong to the Route's Ports. Opposite Routes may share one Session
but always use distinct Route/Stream/epoch bindings and independent queues.

This media packet is an in-process semantic contract, not a public network byte
layout. A concrete transport Adapter must bind it to authenticated Session and
Route state, define framing/MTU/replay behavior and retain its own dependency
and provenance decision. ADR 0006 therefore remains in force.

Audio Share and MicYou remain `AdapterManaged` compatibility transports and
physical golden baselines during migration. They may map into this seam only to
the extent their private protocols preserve the declared semantics; missing
metadata remains explicitly unobservable. They do not become mutually or
generically interoperable by sharing the seam.

The target Android product is one CapyIO Node/service containing native
microphone Source and speaker Sink platform Adapters. That application and its
permissions are later separately approved slices, not part of this decision's
first implementation.

## Consequences

- PCM/Opus, voice/media and future backend choices can share lifecycle,
  buffering, timing and metrics contracts without sharing one fixed format or
  processing chain.
- Platform adapters and Windows projections need not depend on AOO, MicYou,
  Audio Share or a future native wire.
- The first implementation can be exhaustively tested without hardware or
  networking, but it is not evidence of latency, codec quality or transport
  security.
- Compatibility paths remain available until native per-direction parity is
  physically proven; removing them merely for architectural uniformity is not
  allowed.
- Any imported third-party implementation remains outside the portable Core
  and retains upstream, revision, license, imported-path and modification
records even when later packaged with CapyIO.
