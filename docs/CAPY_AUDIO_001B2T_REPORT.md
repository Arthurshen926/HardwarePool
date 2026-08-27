# CAPY-AUDIO-001B2T Report

Date: 2026-08-27

Status: Broker-owned PCM ingest and physical Android submission proven;
human-confirmed audibility pending

## Outcome

The external Audio Share `as-cmd` process is no longer required for the final
CapyIO Speaker media path. A CapyIO-authored user-mode transport now accepts
bounded PCM blocks and implements only the pinned Audio Share v0.3.4 Android
private contract:

```text
simulated Broker PCM
  -> bounded non-blocking queue
  -> TCP format/start/heartbeat control
  -> frame-aligned UDP PCM segments
  -> Audio Share v0.3.4 Android AudioTrack
```

The transport remains `AdapterManaged` and trusted-lab-only. It is not a
`capyio.audio.frames/1` wire binding and does not add network or protocol work
to the driver or render APO callback. ADR 0030 records the decision, upstream
fixed-revision review, dependency and security limits.

## Deterministic evidence

The Adapter tests prove:

- the 48 kHz stereo S16 format encodes to the pinned Protobuf wire bytes;
- TCP get-format and start-play commands round-trip;
- UDP session association requires a live id and the TCP peer's source IP;
- a 4,000-byte PCM block is split below the upstream IPv4 UDP payload bound,
  remains sample-frame aligned and reconstructs byte-for-byte;
- empty, oversized and unaligned blocks fail explicitly;
- queue and peer counts, block sizes, waits and shutdown are bounded.

`cargo xtask ci` passes with the new transport and tone producer.

## Physical evidence

Approved Android target:

- ADB target used explicitly: `100.66.157.119:33071`;
- model: Vivo `V2419A` (`PD2419`);
- installed receiver: `io.github.mkckr0.audio_share_app` v0.3.4,
  version code 3004;
- receiver configuration observed through UI automation:
  `100.66.231.100:65530`;
- path: host and phone on the existing Tailscale network.

The second 10-second 440 Hz run reported:

```text
receiver_connected=true
blocks_enqueued=1000
queue_full=0
blocks_without_receiver=0
datagrams_sent=2000
datagram_send_errors=0
pcm_bytes_sent=1920000
```

The byte count is exact for 48,000 samples/channel × two channels × two bytes
for ten seconds. Android `dumpsys audio` independently showed an active
`AudioTrack` owned by the pinned package, `state:started`, stereo channel mask,
48 kHz sample rate, `USAGE_MEDIA`/`CONTENT_TYPE_MUSIC`, and an unmuted music
stream routed to the speaker. These facts prove network delivery through
Android audio submission; audible output still requires the user's observation
and is not inferred from counters.

## Remaining Gate 7B work

1. Record user-confirmed audibility for this simulated-producer run.
2. Complete the elevated ADR 0029 recovery/rollback audit and exact-package
   approval before any local driver deployment.
3. Import only the reviewed minimal Microsoft endpoint/APO paths with MS-PL
   notices and CapyIO identifiers.
4. Implement and stress the preallocated APO-to-Broker staging ring, then feed
   its real PCM into the transport proven here.
5. Prove independent `CapyIO Speaker` selection, silence on ordinary output,
   Broker/receiver loss, audio-service/reboot behavior and clean uninstall.
