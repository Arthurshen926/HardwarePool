# Remote Touchpad Adapter

This crate contains two separate hardware-free paths:

- the `touch-events/1` to `pointer-events/1` compatibility converter maps one
  active contact to relative motion, taps and primary-button drag; and
- the full `touchpad-frames/1` path preserves up to five physical contacts in a
  private AdapterManaged packet for a remote Windows Precision Touchpad Sink.

Touch input is a complete active-contact snapshot. Empty snapshots, sequence
gaps, explicit epoch changes, unsupported multi-contact input and lifecycle
cleanup produce a pointer `Reset`. After a gap or ambiguous contact set,
non-empty snapshots are ignored until an empty snapshot restores a known state.

The converter uses distinct input/output Stream IDs and epochs. It emits at
most two bounded Pointer frames for one Touch snapshot and performs no network
I/O, Windows input injection, HID/VIIPER mapping, driver work, hardware access,
multi-finger scrolling or gesture expansion.

`PrivateTouchpadPacketCodecV1` is bound to one negotiated touchpad stream,
epoch and descriptor. It emits a fixed-capacity packet of exactly
`32 + 24 * contact_count` bytes, with a 152-byte maximum. Decode checks magic,
version, exact length, epoch, enum values, reserved data, optional-field
canonical form and the ordinary semantic contract before returning a frame.
StreamId and clock domain remain Route/session bindings rather than repeated
packet fields.

`PrivateTouchpadReceiver` adds the user-mode lifecycle immediately above that
codec. It is configured for 1..=1000 packets/s (240 by default), rejects
duplicate/late sequences and local receive-clock regression, reports gaps to
the Sink, and closes the Sink on faults. While contacts are active, a
10 ms..=30 s idle deadline (250 ms by default) also closes the Sink. Explicit
disconnect, strictly increasing epoch transition and abandoned receiver drop
all use the same bounded close path. The Runtime must call the timeout poll
from a trusted local monotonic clock.

`PrivateTouchpadRouteSession` supplies that caller-driven scheduling boundary.
It binds the receiver to one Runtime-owned Core `Route` snapshot: exact Route,
Session, Source/Sink Ports, `AdapterManaged` backend, touchpad Profile,
authorization expiry, Active state and matching epoch are rechecked at enqueue
and pump. A Starting Route must be activated explicitly. The packet queue is
preallocated for 1..=64 fixed 152-byte records; overflow, stale queued data,
Route invalidation or local-clock regression closes rather than drops unknown
contact state. One pump is bounded to the configured queue and always polls the
active-contact deadline afterward.

`PrivateTouchpadRuntimeWorker` removes Route snapshots and timestamps from the
transport-facing call surface. A composition layer supplies a read-only Route
provider and one coherent local clock sample; start, packet, tick and epoch
commands each obtain exactly one immutable Route snapshot. Both millisecond and
nanosecond clock rollback, provider/clock failure and Route lifecycle drift fail
closed. The worker is deterministic and caller-driven: it creates no thread,
timer, socket or platform device.

`PrivateTouchpadPacketSource` is the corresponding bounded sender-side entry.
It requires an initial CancelAll, exact contiguous stream sequence and released
contacts before close. Encoding and sequence checks commit transactionally;
the Source still opens no socket and leaves authenticated delivery to a later
transport boundary.

`PrivateTouchpadDeliverySession` defines that transport handoff without
implementing cryptography or a socket. A trusted host-supplied channel must
reconfirm the exact Route/Session/endpoints/epoch/authorization-expiry binding
before every send. Definite pre-write rejection leaves the same frame retryable;
an unknown delivery result, admission loss or binding drift faults the session
and closes the channel exactly once.

`PrivateTouchpadRuntimeDeliveryWorker` removes Route snapshots and timestamps
from that sender-facing call surface. Construction, each frame and normal close
sample the trusted local clock and fetch a fresh Runtime-owned Route snapshot;
the worker derives the complete active Source binding itself and requires it to
remain identical to the initially admitted binding. Route stop, authorization
expiry, endpoint/Session/epoch drift, provider/clock failure or clock rollback
fail closed before further delivery. It is caller-driven and creates no thread,
timer, socket, Android component or platform device.

`private_touchpad_host_channel` is the first concrete implementation of the
admitted-channel contract. It is a single-threaded, caller-driven host handoff
with a preallocated 1..=64 packet queue. Full queues and disconnected receivers
reject before write; admission denial, binding replacement and receiver close
discard queued packets so stale contact state cannot cross a lifecycle change.
The receiver may drain packets already accepted before an orderly sender close.
This composes both Runtime workers and the Windows projector in tests, but it is
not a cross-device socket, authenticated peer connection or public wire format.

`PrivateTouchpadTransportCodecV1` defines the next bounded boundary for a future
pre-authenticated reliable byte stream. Its 160-byte Hello binds the complete
Route tuple, Data records wrap one private packet with a 176-byte maximum, and
24-byte Ack/Close records retain exact epoch/sequence semantics. Outer and
embedded packet identity must match. The codec opens no socket and supplies no
authentication or encryption; a lost or mismatched Ack remains terminal
delivery ambiguity and requires cancellation plus a later Route epoch.

`PrivateTouchpadTransportReceiver<F>` composes that codec, the packet receiver
and a selected Sink factory without adding I/O. Construction validates the
semantic bounds but opens no Sink. Only an exact Hello invokes the factory; Data
is acknowledged only after packet/sequence/rate validation and Sink acceptance.
Malformed transport closes an active Sink and faults the connection. Runtime
Route authorization and peer authentication remain caller responsibilities.

`new_with_sink_factory` separates validation from platform side effects. It
preflights the authorized Route binding, Stream/descriptor, first sequence and
all queue/receiver limits before calling a `PrivateTouchpadSinkFactory`.
`WindowsSyntheticTouchpadSinkFactory` is the production type bridge to
`SyntheticTouchpadSession`; constructing the factory is side-effect free and
only its explicit `open` creates a Windows device. Preflight failure therefore
opens no Sink, while factory failure remains a distinct typed build error.

`WindowsVhfTouchpadSinkFactory` reuses that exact boundary for the protected
VHF fallback. It derives the Broker generation from the admitted Stream epoch;
epoch advance closes/releases the prior driver generation and completes a new
Hello before accepting frames. Invalid Route preflight is tested with the real
factory type and returns before interface enumeration/open.

The VHF real-interface acceptance is a separate exact-name ignored test. It
opens an already installed protected interface through an authorized Runtime
Route, completes Hello and immediately closes without submitting a contact
frame. Default tests compile but never invoke it or install a driver.

Separately approved fixed VHF desktop acceptances submit bounded one-finger
motion/tap, two-finger pan and three-/four-finger horizontal-swipe fixtures. They
are exact-name ignored tests, pin the compiled executable in their elevated
wrappers and have objective cursor, click, wheel and Shell-target evidence in
`docs/CAPY_PTP_003J_REPORT.md`.

The real Windows factory acceptance is a separately ignored, exact-name test.
It uses a real authorized `NodeRuntime` Route, opens one
`SyntheticTouchpadSession`, verifies that zero packets were enqueued/processed,
then explicitly closes it. Ordinary `cargo test` and CI never create the device;
running the ignored test still requires explicit human approval.

A second exact-name ignored acceptance activates the real Route and submits one
fixed closed one-finger lifecycle through packet encode, worker queue/pump and
the Windows session: CancelAll, down, horizontal move and release. It asserts
four packets enqueued/processed before immediate close. This proves native API
submission through the composed path, not user-observed pointer behavior.

A third ignored acceptance preserves two stable contact IDs while moving both
contacts vertically, then releases them. Four packets cross the same real
Route/worker/Windows Sink path and are intended for Precision Touchpad two-finger
pan/scroll recognition. Native submission success is recorded separately from
user-observed window scrolling.

A fourth ignored acceptance reuses the fixed Windows injection fixture for a
three-contact horizontal swipe. It sends CancelAll, eight gradual updates with
stable IDs 1..3, an empty release and a final CancelAll at 15 ms intervals. The
11 packets cross the same composed path. Native submission success does not by
itself establish that Windows visibly switched a desktop or application.

A fifth ignored acceptance applies the same bounded lifecycle to four stable
contacts. Its 11 fixed packets can trigger the configured Windows four-finger
system action, but expose no arbitrary gesture input. API success and visible
system behavior remain distinct evidence.

A sixth exact-name ignored acceptance replaces direct packet enqueue with the
002R host channel. One real Runtime sender emits CancelAll, down, horizontal
move and release; the receiver Runtime worker drains all four packets into a
real `SyntheticTouchpadSession`, then closes the released device. The controlled
run proves native creation and submission across the concrete host handoff. It
still does not establish a cross-device transport or independently observed
pointer motion.

The packet is a private trusted-lab framing described by ADR 0044, not a public
CapyDataPlane wire contract. This crate still opens no socket and provides no
peer authentication, encryption, Route authorization, network scheduling,
Android runtime or desktop injection. Its sequence/rate guards limit one
already-bound session; they are not cryptographic anti-replay or transport
admission control. A future transport must establish peer trust before creating
the Route session. Supplying a structurally valid `Route` value is not itself
authorization; the hosting Runtime must own that snapshot.

`capyio-ptp-adb-lab` is the separately gated CAPY-PTP-002V physical harness. It
binds only Windows loopback port 61000, accepts the compiled full-binding Hello,
submits Data to a selected Sink, then emits Ack. Synthetic projection remains
the default; explicit `--vhf` opens the already installed protected VHF interface
only after Hello validation and therefore also requires elevation. Both
`--inject` and `--acknowledge-desktop-input` are mandatory in either mode. Its
intended peer is the explicitly authorized Android debug APK through
`adb reverse`; it is not a LAN server, production authentication mechanism or
ordinary Runtime entry point.

The cursor diagnostic additionally requires `--vhf`,
`--exit-after-release-exactly=1` and `--anchor-and-observe-cursor`; it ignores
contact-count mismatches and source motion below 100 himetric before comparing
the interactive Windows cursor baseline. The separate bounded
`--manual-session` mode keeps the physical lab open until Android Close,
transport failure or 600 seconds of idle time. Neither mode is a persistent
production host or reconnect owner.
