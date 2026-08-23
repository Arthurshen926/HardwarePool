# Audio Data-Plane Bootstrap

> Status: semantic foundation; no network binding selected

## 1. Purpose

`capyio-audio` provides transport-independent primitives used after a transport has authenticated, bounded and decoded an audio frame. It does not open sockets, choose a codec, access a hardware device or run inside the Windows driver.

## 2. Audio frame semantics

Every decoded frame contains:

- `stream_id` — prevents one stream from entering another stream's buffer;
- `stream_epoch` — changes whenever a stream restarts or its format changes;
- `sequence` — packet/frame ordering and loss detection;
- `source_timestamp_micros` — source monotonic-clock timestamp, never wall-clock time;
- `first_sample_index` — exact position in the source audio sample timeline;
- `sample_count` — samples per channel in this frame;
- `discontinuity` — explicit source discontinuity marker;
- bounded PCM payload.

Payload length is validated against the negotiated `AudioFormat`:

```text
sample_count × channel_count × bytes_per_sample
```

The bootstrap code rejects zero-sample frames, arithmetic overflow, payload-size mismatch and payloads over the conservative one-megabyte semantic limit. A concrete transport must impose a smaller MTU/fragmentation policy where appropriate.

## 3. Reordering and loss

`ReorderBuffer` is bound to exactly one `stream_id` and one `stream_epoch`. It has a fixed frame capacity and accepts only sequences inside the current reorder window.

Outcomes are explicit:

- accepted;
- duplicate;
- late;
- wrong stream;
- wrong epoch;
- too far ahead;
- full.

The queue never silently evicts an earlier frame. A caller may wait for its jitter deadline and then invoke `skip_gap_and_pop`; only that explicit operation counts the missing sequence range as lost.

This container is intended for a worker/network thread. It is not the final lock-free platform audio-callback ring.

## 4. Clock drift

`ClockDriftEstimator` correlates source sample index with receiver monotonic time and reports:

- observed source rate;
- rate ratio relative to nominal sample rate;
- signed parts-per-million drift.

It establishes a measurable baseline for later dynamic resampling. Production work still needs robust filtering, window rotation, outlier handling and integration with buffer-fill control.

## 5. Deferred wire encoding

The semantic frame is not itself the public binary wire layout. A transport ADR must define:

- byte order and fixed header layout;
- authentication/encryption relationship;
- MTU and fragmentation;
- PCM versus codec payload identifiers;
- replay window;
- stream/epoch binding;
- maximum packet and aggregate rates;
- fuzz and golden-fixture strategy.

The Windows kernel driver never receives this network frame. The user-mode Broker converts validated decoded PCM into the smaller driver IPC contract.
