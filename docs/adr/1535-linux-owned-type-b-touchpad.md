# ADR 1535: Owned Linux Type-B touchpad projection

- Status: implemented source; software validation, physical acceptance deferred
- Date: 2026-10-21

## Decision and compatibility

Add `capyio-linux-touchpad`, an optional Linux contact Sink behind the existing
`PrivateTouchpadSink`/factory and pinned `SecureTouchpadReceiver` boundaries.
There are no new network messages, relative mouse events, synthetic clicks or
application gesture recognizers. Existing Android complete touchpad snapshots
remain the contract: five contacts, 120 × 70 mm, NonClickable, normalized pressure,
no contact size. Desktop-stack recognition and gestures are not established by
software event traces. There is no default listener, pairing UI or deployment.

The owned session requires its first frame (and first frame after an epoch
advance) to be CancelAll before acknowledging readiness. The deterministic
projector uses the existing `TouchpadFrameTracker` for initial
CancelAll, exact stream/epoch, sequence gaps and timestamps. A gap releases all
slots and suppresses updates until CancelAll. Any low-confidence contact also
releases the whole snapshot and requires CancelAll; confidence returning alone
cannot resurrect a possibly accidental gesture. Missing contacts release their
slots. Reordering preserves slots. Reused wire IDs receive fresh nonnegative local
tracking IDs; arbitrary u32 wire IDs are never cast to Linux signed tracking IDs.
Local ID exhaustion fails closed without wraparound. Malformed input is rejected
transactionally and the owning session closes rather than continuing.

The fixed descriptor exposes INPUT_PROP_POINTER, not DIRECT or BUTTONPAD;
ABS_MT_SLOT 0..4, ABS_MT_TRACKING_ID 0..INT_MAX, X 0..12000 and Y 0..7000 at
100 units/mm, and pressure 0..65535. Compatibility X/Y track the lowest occupied
slot. Pressure preserves the normalized number exactly; absent optional pressure
means zero/unmeasured, not invented physical force. BTN_TOUCH is first in every
report and the five mutually exclusive tool-count keys reflect active contacts.
No physical click buttons are advertised. Each complete report ends in exactly
one SYN_REPORT. Event batches use a fixed 64-entry array with a proven smaller
maximum for this five-slot descriptor.

## Native boundary and authority

The optional `native-uinput` helper uses externally installed official libevdev
headers/libraries. Default builds require neither headers nor `/dev/uinput`.
The helper opens only fixed `/dev/uinput` with O_CLOEXEC and O_NONBLOCK after the
Rust owner validates exact Runtime Route, Ready Session, finite independent grant
and local Sink lease, and the TLS receiver matches Hello. It never clones a
physical device, changes permissions, adds udev rules, loads modules, changes
privilege, or falls back to another injection mechanism. Missing device or access
is an error. A caller must separately arrange authorized OS access; this feature
does not provision it.

One single-use factory owns the admitted descriptor/stream and finite deadline.
A receive worker handles synchronous TLS/helper I/O, outside owner polling.
Stop interrupts the TLS/helper sockets before joining the worker. Runtime revoke,
lease/grant expiry, EOF, malformed records, uncertain write, backpressure or helper
exit terminates the epoch rather than retrying a possibly committed snapshot.

Private helper IPC uses a 24-byte HPT1 startup record: little-endian lifetime_ms,
idle_ms, reserved zero, and absolute Unix deadline_ms. Lifetime is at most 300 s;
idle is at most 2 s. HPF1 batches have an 8-byte header and at most 64 fixed
8-byte type/code/value records. Reserved bytes, sizes, event vocabulary and
ranges are checked before writing. A zero-count record requests terminal close.
ACK byte 1 follows complete native submission including SYN_REPORT, never merely
queueing or a partial write. The helper independently applies a fixed-window
240-report/s limit, absolute/monotonic deadlines and partial-record deadline.
Only complete reports renew idle time; no independent heartbeat can keep stale
contact state alive. Thus inactivity can deliberately terminate the session.

Normal Stop/EOF/timeout releases all five slots and keys, synchronizes, destroys
the device and closes the owned descriptor. Cleanup is best-effort after a device
write fault but destruction is unconditional. SIGTERM follows that cleanup path.
SIGKILL/fatal process death closes the owned file descriptor but cannot guarantee
explicit release events; that limitation must not be described as graceful release.

## Dependency decision

libevdev is the maintained freedesktop input API with an MIT license. It avoids
vendoring Linux ioctl structure layouts and hand-implementing device creation.
Alternatives were raw uinput ioctls (more platform ABI responsibility) and Rust
evdev bindings (additional production dependencies with no benefit to this narrow
process boundary). Only official distribution development packages are used for
isolated compilation. No upstream source or binary is imported or redistributed.
Hosted evidence records the resolved distribution version; release distribution
and real-machine access remain separate decisions.

## Software validation and limits

The same projector is tested for complete snapshots, slot stability, arbitrary
wire IDs, pressure, confidence, sequence/epoch/time boundaries, cancellation and
thousands of deterministic randomized inputs. Native fake-backend tests run the
production parsing, deadline, rate and cleanup loop without any kernel device.
The exact immutable descriptor tables/path/flags drive both native creation and
the fake capability recorder. Native compilation against official libevdev
headers is a separate hosted gate, not evidence of device creation.

The authenticated owner gate uses loopback mTLS, exact Runtime authority and fake
backend traces. It must report actual Android policy participation separately from
synthetic frames. No tests here open `/dev/uinput`; kernel device creation, libinput
recognition, cursor movement, scrolling, gestures, Android physical contact capture
and user-session OS integration remain unverified until separately authorized
consolidated physical testing.

References: [kernel uinput](https://docs.kernel.org/input/uinput.html),
[Type-B protocol](https://docs.kernel.org/input/multi-touch-protocol.html),
[event ordering](https://docs.kernel.org/input/event-codes.html),
[libevdev and license](https://www.freedesktop.org/software/libevdev/doc/latest/).
