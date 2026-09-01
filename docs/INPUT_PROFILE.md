# CapyIO Input and Haptics Profiles

> Status: normative pre-alpha semantic contracts for the initial input Profile family.

## Profile registry

| Profile | Initial format | Semantic payload |
|---|---|---|
| `capyio.input.key-events/1` | `key-events-v1` | bounded physical-key transitions/reset |
| `capyio.input.pointer-events/1` | `pointer-events-v1` | relative/absolute pointer, buttons, scroll/reset |
| `capyio.input.touch-events/1` | `touch-snapshot-v1` | complete active-contact snapshot |
| `capyio.input.touchpad-frames/1` | `touchpad-frame-v1` | physical five-contact touchpad snapshot/cancel |
| `capyio.input.gamepad-state/1` | `gamepad-state-v1` | complete fixed-size controller state |
| `capyio.haptics.feedback/1` | `dual-rumble-v1` | stop or bounded dual-motor rumble |

These are semantic Profiles, not Windows `SendInput`, HID reports, USB/IP,
DSU, VIIPER, Android UI events or driver ABIs. Platform/protocol Adapters map
the normalized semantics explicitly.

## Timing, epoch and delivery

`InputStreamDescriptor` binds one `StreamId`, positive epoch and bounded clock
domain at stream setup. The allocation-free per-frame `InputFrameHeader`
contains only `StreamId`, epoch, sequence and source-monotonic timestamp.

`InputSequenceTracker` rejects wrong streams, stale/future epochs,
duplicate/late sequences, non-advancing epoch changes and sequence exhaustion.
It reports gaps explicitly. A data-plane implementation may provide equivalent
guards, but it cannot accept older-epoch input as current.

On a gap, epoch change, Route `Offline`/`Failed`/`Stopped`, Adapter failure or
peer loss, a Projection must fail safe before accepting later state:

- release every pointer button and keyboard key;
- publish/act as an empty touch snapshot;
- cancel every touchpad contact and release its integrated button;
- return every gamepad control to neutral;
- stop haptics.

Pointer and keyboard include explicit reset events for normal in-stream cleanup.
Resets cannot be mixed with other events in one frame.

## Coordinates and values

`NormalizedPosition` uses unsigned 16-bit coordinates over the closed unit
square. Origin is top-left, X grows right and Y grows down. Screen selection,
DPI scaling, physical units and multi-monitor mapping remain Projection policy.

Touchpad positions and contact sizes instead use himetric units (1/100 mm)
inside the physical dimensions declared once by `TouchpadDescriptor`. Origin is
top-left, X grows right and Y grows down. The distinction is intentional:
receiver-side touchpad gesture policy depends on physical motion rather than
display pixels or a normalized touchscreen area.

`SignedAxis` accepts `-32767..=32767`; zero is neutral and `-32768` is reserved.
`NormalizedMagnitude` and gamepad `TriggerValue` use `0..=65535`.

## Pointer

Pointer frames contain 1..=64 bounded events: relative device counts, absolute
normalized position, semantic buttons, detent/pixel scroll or reset. Windows
wheel constants, virtual-desktop coordinates and `SendInput` restrictions do
not enter the Profile.

## Touch

Touch frames are complete active-contact snapshots with at most 32 unique
contact IDs. An empty snapshot means all contacts are released. Phase changes
are derived by comparing accepted consecutive snapshots; a gap requires the
fail-safe release above before a later snapshot is applied.

Precision Touchpad contact confidence/geometry and cancel behavior are not part
of this generic Profile; they use the separate touchpad Profile below.

## Touchpad

`capyio.input.touchpad-frames/1` is a direction-neutral StandardPort Profile
for Android touch areas, Windows physical touchpads and remote system
Projections. It is not a HID report or a Windows/Android native structure.

`TouchpadDescriptor` declares:

- physical width and height in positive bounded himetric units;
- 3..=5 concurrent contacts;
- click-pad, pressure-pad or non-clickable integrated-button behavior;
- whether contact width/height and pressure can appear.

`TouchpadFrame` is either an `update` containing the complete set of active
contacts or `cancel_all`. Every active contact has a stable `u32` ID for its
lifetime, in-range himetric X/Y and mandatory confidence. Optional non-zero
bounding size and normalized pressure are legal only when advertised. IDs may
be reused only after their earlier contact disappears from an accepted
snapshot.

An omitted contact means it left the surface. A stateful Projection retains its
last accepted position long enough to emit the platform-specific release form.
`cancel_all` contains no contacts and a released button; it is required for
initial fail-safe state, Android cancellation, Route stop/offline, peer loss,
Adapter failure and reconnect.

`TouchpadFrameTracker` rejects wrong streams, stale/future epochs, duplicates,
late frames and timestamp regression. After a sequence gap or explicit epoch
advance it suppresses all updates until `cancel_all` arrives. A gap-causing
`cancel_all` both reports the gap and restores the released state.

The five-contact bound follows the current Windows Precision Touchpad target.
Sources that detect more contacts do not truncate an accepted snapshot; their
Adapter suppresses newly arriving contacts according to a documented bounded
policy while preserving already accepted contact IDs.

HID certification blobs, scan-time rollover, azimuth, Windows Settings policy,
Android action codes, network framing and VHF reports are Projection/Adapter
concerns, not v1 payload semantics.

## Keyboard

Keyboard uses a closed CapyIO physical-key enum plus pressed/released/repeat
semantics. HID usages, Windows scan codes, text/IME composition and layout
translation are Adapter responsibilities. A released key cannot be a repeat.

## Gamepad

Gamepad state is a complete, fixed-size snapshot: a validated button bitmask,
D-pad, two signed sticks and two unsigned triggers. `neutral()` is explicit.
IMU, touch contacts, battery and haptics do not hide inside this state:

- IMU remains `capyio.motion.imu-samples/1`;
- touch remains `capyio.input.touch-events/1`;
- touchpad contacts remain `capyio.input.touchpad-frames/1`;
- haptics is an independent Source-to-Sink Route.

## Haptics

Haptics is data feedback, not an untyped Control Port. v1 supports explicit
stop or two normalized motor amplitudes with a positive duration no longer than
10 seconds. Zero-amplitude rumble is invalid and must be represented as stop.

## Serialization boundary

Serde derives support bounded fixtures and diagnostics only. They are not a
CapyIO wire contract or a promise that Rust memory layout maps to a HID/DSU
report. Concrete data planes need independent framing, bounds, authorization,
replay defense and epoch binding.
