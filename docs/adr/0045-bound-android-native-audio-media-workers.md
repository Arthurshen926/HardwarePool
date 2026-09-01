# ADR 0045: Bound Android native-audio packetization and network workers

- Status: accepted for `CAPY-AUDIO-NATIVE-001D2`
- Date: 2026-08-31

## Context

ADR 0044 fixes a small UDP reference wire and matching Rust/Java codecs, but a
codec alone is not a safe audio path. Android's capture/render loops must never
wait on a socket, network workers must not grow queues or partial packets
without a limit, and queue pressure must not silently corrupt the media
timeline. The product Routes also need to remain independently bound; a worker
assembled from mismatched Session/Route/Stream/epoch values must fail before a
thread starts.

This slice still precedes physical switching of `AudioRecord` and `AudioTrack`.
It needs a hardware-free composition that can later sit between each platform
audio worker and the selected native endpoint without making Activity lifetime
or a real-time callback own networking.

## Decision

Add a dependency-free Java media-worker layer beside the ADR 0044 codec:

1. `NativeLanPacketQueue` is pre-bound to one media binding, accepts at most
   1–128 immutable packets and at most 4 MiB in aggregate, never silently evicts
   an accepted packet and exposes packet-full, byte-full and wrong-binding
   drops separately;
2. producer `offer` is non-blocking. Only a background worker may use the
   bounded 0–2,000 ms wait in `poll`; neither method is approved for a
   real-time callback;
3. `NativeLanPcmPacketizer` validates 8–384 kHz, 1–32 channels, 2–4 bytes per
   sample, 2.5–60 ms integral frame duration and the ADR 0044 payload ceiling.
   It converts arbitrary frame-aligned worker reads into exact packets while
   retaining sequence, timestamp and first-sample timeline;
4. a queue-pressure drop advances the media timeline and sets discontinuity on
   the next accepted packet. Explicitly discarding a partial packet advances
   its sample/time position before marking discontinuity;
5. `NativeLanPacketReassembler` mirrors the Rust 1–8 partial-packet bound,
   canonical metadata checks, duplicate/conflict behavior and unsigned sequence
   ordering;
6. one-shot sender and receiver workers own their UDP endpoint and use stable
   problem codes. Stop closes the socket, interrupts an empty-queue wait and
   joins for at most two seconds; restart requires constructing a fresh worker
   and endpoint for the new Route epoch;
7. sender endpoint/queue and receiver endpoint/reassembler/queue bindings must
   match exactly before start;
8. a dependency-free loopback contract moves two 48 kHz stereo PCM packets
   through packetizer, bounded send queue, sender worker, two UDP fragments per
   packet, receiver worker, reassembler and bounded sink queue, then proves
   payload/timeline equality and bounded terminal stop.

The existing Android platform adapters are not connected in this slice. The
service still has no peer configuration or automatic network start, and the
Activity/binder does not accept IP, port, UUID or format authority.

## Consequences

- The native backend now has a complete hardware-free worker composition for
  either media direction, with explicit pressure and lifecycle semantics.
- Android app compilation includes this layer, but installed 001C behavior is
  unchanged: microphone data is discarded and the speaker remains empty.
- The queues use bounded synchronization and allocation on blocking audio/media
  workers. They are not the final lock-free callback ring and must not be used
  from an AAudio/Oboe or other real-time callback.
- Jitter deadlines, gap release, loss concealment, retransmission, resampling,
  clock control and authentication remain absent.
- 001E physical evidence found 32- and 64-packet speaker queues insufficient
  for Android/Tailscale burst scheduling despite zero network/reassembly loss.
  The refined 128-packet ceiling holds at most 1.28 seconds of the fixed 10 ms
  lab packets; it is a bounded acceptance buffer, not a latency target.
- `001E/001F` must separately connect speaker and microphone platform workers,
  add narrow trusted Route configuration and collect authorized physical
  evidence without merging the two Route lifecycles.
