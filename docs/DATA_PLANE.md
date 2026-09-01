# CapyIO Data-Plane Bootstrap

> Status: bounded semantic foundation; no production network binding selected;
> one AdapterManaged Audio Share compatibility binding exists for trusted labs

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

ADR 0030's Audio Share-compatible TCP/UDP sender is a deliberately private
`AdapterManaged` bridge to the pinned Android receiver. It strips CapyIO frame
metadata because the upstream protocol cannot carry it and therefore is not a
StandardPort implementation or a candidate production binding.

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
excessive skew are explicit outcomes. This parser/pairing behavior opens no
socket and makes no reconnect, authentication or physical-timing claim.

The next Adapter layer uses a synchronous Tungstenite worker connection. It
accepts only IP-literal local-lab endpoints and fixed SensorServer paths, applies
connect/read/write deadlines, limits frames/messages to 4 KiB and keeps JSON
mapping outside the WebSocket implementation. Ping/pong/close never become IMU
payloads. A connection timeout is retryable by caller policy; close/capacity and
other terminal errors require a fresh client and later a new stream epoch.

The physical-lab command composes two such clients with the deterministic
assembler and bounded fan-out. It waits for the accelerometer handshake before
starting the gyroscope worker, publishes each emitted envelope independently to
a numeric Panel and JSONL Recorder, and sends WebSocket Close before successful
exit. It is a bounded lab consumer, not Runtime reconnect policy or a public
wire protocol.

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

ADR 0044's 152-byte remote-touchpad packet is deliberately narrower than this
future public binding. It is a private `AdapterManaged` record codec with no
socket, authentication, encryption, cryptographic replay window or
cross-Adapter interoperability claim. Its Adapter receiver enforces bounded
single-session sequence/rate/idle lifecycle policy after Route binding. A
fixed-capacity ingress rechecks the current Runtime-owned Route before enqueue
and pump, but still chooses no transport and grants no network trust. It
exists to test and later connect the two touchpad platform boundaries without
changing the transport-independent semantic envelope contract.

The private touchpad worker makes scheduling inputs explicit without choosing a
transport. Each caller-driven packet or tick reads one current Route snapshot
and one coherent local clock sample before entering the fixed queue. This closes
manual Route/time injection at the Adapter API boundary, detects lifecycle and
clock rollback, and still leaves authentication, socket I/O, reconnect and task
scheduling to a future composition slice.

ADR 0045 additionally fixes a private bounded record envelope for a future
pre-authenticated reliable stream: a 160-byte full-binding Hello, up to 176-byte
Data, and exact 24-byte Ack/Close. This does not select authentication,
encryption, socket or public CapyDataPlane interoperability. Its purpose is to
make stream framing, binding confirmation and delivery ambiguity testable before
the live Android transport composition is introduced.

The Windows kernel driver and render APO never receive this network frame. For
a virtual render endpoint, the preferred first path is a bounded copy from the
endpoint-associated render APO into a pre-opened shared-memory/SPSC staging
ring consumed by the Broker. SysVAD WASAPI loopback is synthetic and cannot
prove real render PCM. A smaller versioned driver IPC remains available only
if isolated-target APO lifecycle or certification evidence fails.
