# ADR 0047: Connect the native microphone path through one selected capture producer

- Status: accepted for `CAPY-AUDIO-NATIVE-001F1`
- Date: 2026-08-31

## Context

The MicYou compatibility path already proves that ordinary Windows clients can
consume phone microphone PCM through the CapyIO capture endpoint. It does not
use the CapyIO common packet or the native LAN backend. ADR 0045 supplies a
bounded Android sender worker, but `MicrophoneSourceAdapter` still discards its
`AudioRecord` reads and Windows has no native receiver for the capture ring.

The capture mapping is a single-producer/single-consumer ring. MicYou ingress
and a native receiver therefore cannot be treated as concurrent producers.

## Decision

1. Move ownership of `Global\CapyIO.CaptureRing.v1` into the internal
   `capyio-windows-capture-ring` crate. The SCM Broker creates and resets the
   mapping; a named owner mutex distinguishes a live Broker from a stale mapping.
2. Add one user-mode `capyio-native-virtual-microphone` receiver. It accepts
   only the fixed 48 kHz mono S16LE microphone lab binding, receives complete
   common packets from one explicit peer, converts PCM to the ring's float32
   representation and commits or drops one complete block.
3. Connect Android `AudioRecord` reads to the existing bounded packetizer,
   queue and sender worker. The audio worker never performs socket I/O; the
   sender owns the UDP endpoint and reports one stable asynchronous failure.
4. Keep build-time lab authority closed and outside the Activity. The default
   build disables the native microphone path. The accepted lab configuration
   uses dedicated ports and deterministic Session, Route, Stream and epoch.
5. Extend the native audio service configuration with one all-or-none
   microphone child specification. The service starts speaker first, starts
   microphone second and rolls speaker back if microphone startup fails. Both
   children must remain running for the native audio unit to remain active.
6. The native service mode and the MicYou compatibility host are mutually
   exclusive capture producers. The service never launches both. Manually
   launching a compatibility producer while native microphone is active is
   unsupported until every producer participates in a common producer claim.
7. Physical acceptance is staged. Non-zero phone-to-native-receiver-to-ring
   counters prove the media data plane. Ordinary Windows-client WAV capture
   additionally requires deploying the matching service build that owns the
   new ring mutex and supervises the receiver.

## Consequences

- The CapyIO Android application can source microphone PCM without the MicYou
  Android application or MicYou private TCP/UDP wire.
- Speaker and microphone now use the same common packet, fragmentation,
  explicit-peer UDP endpoint and bounded worker concepts, while retaining
  direction-specific formats and consumers.
- This trusted-lab backend remains unauthenticated, unencrypted and without
  replay protection. It is not a production StandardPort transport.
- There is still no jitter deadline, resampling, clock correction, codec, AEC,
  concealment, reconnect controller or production pairing authority.
- The service snapshot currently represents the native audio unit as one
  lifecycle. Independent Route control and metrics are follow-up work.
