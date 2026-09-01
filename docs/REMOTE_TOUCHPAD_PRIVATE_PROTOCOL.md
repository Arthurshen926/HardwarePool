# Remote Touchpad Private Packet v1

> Status: AdapterManaged trusted-lab framing; not a public CapyDataPlane wire
> standard and not safe for unauthenticated network input.

## Binding

One codec instance is bound during Route setup to:

- one `InputStreamDescriptor` (StreamId, positive epoch and clock domain); and
- one validated `TouchpadDescriptor`.

StreamId and clock domain are not repeated per packet. The packet epoch must
equal the codec epoch. A reconnect or source restart requires an explicit
strictly increasing epoch before new packets are accepted.

## Byte layout

All multi-byte integers use little-endian encoding. Header size is 32 bytes.

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | ASCII magic `CPTP` |
| 4 | 1 | packet version, exactly `1` |
| 5 | 1 | frame kind: `0` update, `1` cancel-all |
| 6 | 1 | button: `0` released, `1` pressed |
| 7 | 1 | contact count, `0..=5` and within descriptor |
| 8 | 8 | stream epoch |
| 16 | 8 | frame sequence |
| 24 | 8 | source monotonic timestamp in nanoseconds |

Exactly `contact_count` 24-byte records follow:

| Contact offset | Size | Field |
|---:|---:|---|
| 0 | 4 | stable contact ID |
| 4 | 4 | X in himetric units |
| 8 | 4 | Y in himetric units |
| 12 | 1 | flags: bit 0 confidence, bit 1 size present, bit 2 pressure present |
| 13 | 1 | reserved, must be zero |
| 14 | 2 | normalized pressure; must be zero when absent |
| 16 | 4 | contact width in himetric; must be zero when absent |
| 20 | 4 | contact height in himetric; must be zero when absent |

Packet length is exactly:

```text
32 + 24 * contact_count
```

The maximum is therefore 152 bytes. Extra/truncated bytes, unknown flags,
non-zero reserved bytes and non-canonical absent optional fields are rejected.
After structural decoding, the ordinary `TouchpadFrame` contract validates
surface bounds, uniqueness, descriptor features and cancel-all semantics.

## Receiver lifecycle

`PrivateTouchpadReceiver` binds one codec, expected first sequence and
authorized Sink. Its local policy is:

- configured fixed-window admission of 1..=1000 packets/s, default 240;
- strictly monotonic trusted-local arrival timestamps;
- duplicate/late rejection with observable forward gaps;
- active-contact idle timeout of 10 ms..=30 s, default 250 ms;
- strictly increasing explicit epoch changes that reset sequence, receive-time
  and rate-window state before the Sink advances; and
- Sink close on structural, sequence, rate, arrival-clock or submission fault,
  explicit disconnect, active timeout and abandoned receiver drop.

A forward sequence gap is submitted so the semantic Windows projector can
cancel retained contacts and enter its suppress-until-cancel state. A duplicate
or late frame never reaches the Sink. The Runtime must poll the idle timeout;
construction alone does not provide a scheduler.

This codec and receiver provide no peer authenticity, confidentiality, Route
authorization or cryptographic replay window. A future transport must establish
those properties and bind the peer, Route, Stream and descriptor before receiver
construction. The per-session sequence/rate guards are defense in depth, not a
replacement for authenticated transport admission and scheduling.

## Authorized Route ingress

`PrivateTouchpadRouteSession` is the fixed queue/scheduler boundary above the
receiver. Construction requires a Runtime-owned Core Route snapshot with:

- exact expected Sink Port and retained Route, Session and Source identities;
- `capyio.input.touchpad-frames/1` over `AdapterManaged`;
- Starting or Active lifecycle state;
- an Authorized, unexpired grant; and
- Route epoch equal to the input stream epoch.

A Starting binding queues nothing until explicit activation against a current
Active snapshot. Every enqueue and pump rechecks identity, Active state,
authorization expiry and epoch. The queue is preallocated once for 1..=64
records; each record contains at most 152 packet bytes plus its trusted-local
arrival time. Overflow, oversize, local-clock regression, expired queued data
or Route mismatch clears the queue, closes the receiver and poisons the Route
session. A pump processes at most the configured capacity and then polls the
receiver even when the queue is empty.

A later Starting Route epoch can explicitly advance the receiver/Sink and
discard queued old-epoch records before reactivation. This reuses the same
Stream identity/clock domain with a later epoch; a changed Stream identity
requires a new session.

The Route object is a Runtime-to-Adapter input, not a bearer credential. This
layer checks current Core state but does not issue grants, authenticate a peer,
schedule a worker or make fabricated Route values trustworthy.

## Runtime worker boundary

`PrivateTouchpadRuntimeWorker` is the deterministic command boundary above one
Route session. Its production traits remain Core-only: a Node composition layer
provides a read-only Route snapshot provider and a coherent local clock sample
containing authorization milliseconds and ingress nanoseconds. Construction,
activation, packet enqueue, periodic tick and epoch advance fetch current state
internally instead of accepting caller-supplied Route/time arguments.

The worker rejects rollback in either clock value, even if the other value
advances. Provider failure, clock failure and rollback clear the queue, close
the Sink and mark the session failed. Stop deliberately remains independent of
the provider and clock so cleanup is still possible when Runtime state cannot
be read. Counters are fixed `u64` fields with saturating updates; no unbounded
telemetry queue or ordinary callback logging is introduced.

The worker is not an operating-system thread or transport loop. A future Node
composition task still owns scheduling, authenticated packet admission and
delivery of periodic ticks. Deterministic tests adapt a real `NodeRuntime` only
through the provider trait; the Adapter has no production Runtime dependency.

### Authorized Sink construction

The factory constructor preflights Route identity/state/authorization/epoch,
expected Sink, Stream/descriptor, first sequence, queue capacity, rate limit and
idle deadline before invoking any platform factory. A rejected preflight cannot
create a synthetic device. `WindowsSyntheticTouchpadSinkFactory` implements the
same interface for `SyntheticTouchpadSession`; the factory value itself performs
no platform operation, and `open` remains the explicit device-creation point.

This ordering reduces unauthorized or malformed construction side effects. It
does not make device creation automatic: a future authorized Node composition
command must deliberately select the Windows factory, and driver/device tests
retain their separate approval rules.

Controlled Windows evidence also sends one closed four-packet one-finger
lifecycle through this exact composition. The Route is explicitly activated;
each packet is independently enqueued and pumped, then release and Sink/Runtime
stop complete. The test is ignored by default and does not broaden transport or
peer-trust claims.

The controlled two-finger acceptance follows the same four-packet lifecycle but
retains contact IDs 1 and 2 while translating both vertically. It proves that
multi-contact packets survive the current composed path into native submission;
Windows gesture recognition and visible scrolling remain observation evidence,
not packet protocol behavior.

The controlled three-finger acceptance uses the existing fixed injection
fixture: CancelAll, eight horizontal updates retaining contact IDs 1..3, an
empty release and final CancelAll. Eleven packets are submitted 15 ms apart.
The private protocol still carries contact snapshots only; a visible Windows
desktop/application switch is external observation, not a wire-level result.

The controlled four-finger acceptance sends the corresponding fixed 11-packet
fixture with stable IDs 1..4 and the same release/cancellation tail. Together
these tests prove bounded composed native submission for one through four
contacts; Windows gesture-policy effects remain external observation.

On loss, gap, stale/future epoch, Adapter failure or Route stop, the receiver
uses the semantic cancellation/session cleanup path. Packet bytes never enter a
Windows driver or the JSON-RPC/Protobuf control path.

### Sender boundary

`PrivateTouchpadPacketSource` is the only composed sender entry added by
CAPY-PTP-002O. It requires an initial `CancelAll`, exact contiguous sequence
numbers and a released/cancelled tail before close. Codec or sequence rejection
does not advance its state or encoded-packet count. It produces one fixed-bound
`PrivateTouchpadPacketV1` but performs no I/O; a future authenticated transport
must own delivery, retry ambiguity, peer admission and reconnect policy.

`PrivateTouchpadDeliverySession` supplies the next ownership boundary. Its
channel interface must return a currently admitted binding containing the exact
Route, Session, Source, Sink, epoch and authorization expiry before construction
and every send. `RejectedBeforeWrite` commits no Source state and permits an
exact same-frame retry. `DeliveryUnknown` is terminal because retry could
duplicate a gesture; the channel is closed and the session faults. The trait is
implemented only by tests today and is not evidence of authentication or
authenticated encryption.

`PrivateTouchpadRuntimeDeliveryWorker` supplies the Runtime-driven scheduling
boundary above that session. It accepts a Route ID and expected Source once,
then obtains one coherent trusted-clock sample and one fresh Runtime-owned Route
snapshot at construction, before every frame and before normal close. Only an
Active, authorized, unexpired `AdapterManaged` touchpad Route with the exact
Source and stream epoch can produce the binding passed to the delivery session.
The complete tuple must remain equal to the initial binding. Clock rollback,
provider failure, authorization expiry and Route/Session/endpoints/epoch/state
drift are terminal and close the admitted channel. No network, peer identity,
cryptography, reconnect or scheduling thread is added by this worker.

`private_touchpad_host_channel` is the concrete in-process implementation used
by CAPY-PTP-002R. Its capacity is fixed at construction to 1..=64 complete v1
packets. A full queue, closed sender or closed receiver produces a definite
pre-write rejection; an accepted packet is copied into preallocated bounded
storage and may be drained after orderly sender close. Admission denial,
binding replacement and receiver close clear pending packets and retain fixed
saturating discard metrics. Because the handles share local process state, this
channel has no delivery-unknown path. It defines no stream framing, socket,
authentication, encryption or cross-device compatibility contract.

The separately authorized CAPY-PTP-002S acceptance sends the existing fixed
CancelAll/down/horizontal-move/release packet lifecycle through this channel to
the real Windows synthetic-touchpad Sink. Four packets are accepted, received
and processed before released shutdown. This is host composition evidence, not
a new packet version, remote framing or interoperability claim.

### Pre-authenticated stream records

CAPY-PTP-002T adds private record version 1 under ADR 0045. All integers are
little-endian and every record begins with `CPTR`, version, kind, flags,
reserved byte, epoch and sequence in an exact 24-byte header.

| Kind | Exact size | Meaning |
| --- | ---: | --- |
| Hello | 160 bytes | Route, Session, Source/Sink Ports, epoch and optional authorization expiry |
| Data | 56..=176 bytes | header plus one complete 32..=152-byte `CPTP` packet |
| Ack | 24 bytes | exact receiver-processed epoch and sequence |
| Close | 24 bytes | orderly close for the bound epoch |

Data outer epoch/sequence must equal the embedded packet. Hello absent-expiry
bytes are canonical zero; unknown kind/version/flags, non-zero reserved data,
wrong length or any binding difference is terminal. An Ack is emitted only
after receiver acceptance and Sink processing. Missing or mismatched Ack is
delivery-unknown, never permission to retry in the same epoch.

ADR 0049 composes these records with `PrivateTouchpadReceiver` and a platform
Sink factory. Construction checks the Route/Stream epoch and receiver bounds
without opening the factory. Exact Hello is the only transition that may open
the Sink; malformed Data then closes it and faults the connection. This object
is still a pure state machine, not a socket, named pipe or authentication layer.

These are record bytes for an already authenticated encrypted reliable stream,
not authentication or a socket implementation. Plaintext use, peer discovery,
key management, reconnect scheduling and replay persistence remain out of scope.
