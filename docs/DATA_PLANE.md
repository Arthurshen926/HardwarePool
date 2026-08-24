# CapyIO Data-Plane Bootstrap

> Status: bounded semantic foundation; no production network binding selected

## 1. Purpose

`capyio-data-plane` provides bounded, transport-independent StandardPort
envelopes, per-consumer queues and fan-out used after a transport has
authenticated and decoded data. `capyio-audio` retains audio-specific frame,
reordering and drift primitives. Neither crate opens sockets, accesses hardware
or runs inside a Windows driver.

The semantic envelope is not a public wire layout. It carries Profile identity,
typed stream identity, stream epoch, sequence, source/receive timestamps, clock
domain and a validated bounded payload. A concrete transport still owns
framing, authentication, replay defense, rate limits and MTU policy.

## 2. StandardPort queue and fan-out semantics

`BoundedEnvelopeQueue` binds one Profile, StreamId and epoch to a fixed
capacity. Push outcomes explicitly distinguish accepted data, preceding gaps,
duplicates, late samples, wrong streams, stale/future epochs and full queues.
An epoch advances only through an explicit operation, which clears retained
data and resets sequence tracking. Timestamps are never rewritten to conceal a
gap or restart.

`BoundedFanout` gives each consumer its own queue and lifecycle state. A full or
stopped Recorder cannot block or mutate a Panel queue. Overflow rejects the
incoming envelope for that consumer and increments a saturating diagnostic
counter; it never silently evicts an older accepted envelope.

## 3. Fixture-first IMU specialization

`capyio.motion.imu-samples/1` preserves SI acceleration and angular velocity,
optional microtesla magnetic field, Android device coordinates, accuracy,
calibration, bounded sensor metadata and optional per-component source
timestamps. Per-component timestamps are necessary when an Adapter pairs
asynchronous accelerometer and gyroscope readings; the envelope timestamp is
the maximum included source timestamp, not a replacement for the originals.
The committed JSONL fixture is parsed
with line/record limits and replayed to a numeric Panel plus a bounded JSONL
Recorder. This path is deterministic test/demo evidence, not live phone data or
a network decoder.

The SensorServer protocol Adapter enforces a 4 KiB message bound before JSON
decode, exactly three finite axes, known Android accuracy values, positive and
strictly increasing per-sensor timestamps, a fixed pairing-skew limit and
one-time consumption of each required component. Replaced unpaired readings and
excessive skew are explicit outcomes. This is mapping behavior only; it opens no
socket and makes no reconnect, authentication or physical-timing claim.

## 4. Audio frame semantics

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

## 5. Reordering and loss

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

## 6. Clock drift

`ClockDriftEstimator` correlates source sample index with receiver monotonic time and reports:

- observed source rate;
- rate ratio relative to nominal sample rate;
- signed parts-per-million drift.

It establishes a measurable baseline for later dynamic resampling. Production work still needs robust filtering, window rotation, outlier handling and integration with buffer-fill control.

## 7. Deferred wire encoding

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
