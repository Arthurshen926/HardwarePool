# ADR 0042: Separate touchpad frames from generic touch snapshots

Status: accepted

## Context

`capyio.input.touch-events/1` is a generic normalized touch-surface snapshot
with up to 32 contacts. It intentionally has no physical dimensions, contact
confidence, contact geometry, touchpad button type or explicit cancel-all
frame. Those omissions are appropriate for touchscreens and gamepad touch
surfaces but cannot faithfully describe a Precision Touchpad source or sink.

Converting generic touch snapshots to pointer events before routing also loses
the multi-contact lifecycle required for receiver-side native gesture policy.
Conversely, putting Windows HID reports, Android `MotionEvent` flags or
`POINTER_TOUCH_INFO` layouts into the portable Profile would couple the contract
to one platform mechanism.

Microsoft's Precision Touchpad model requires three to five concurrent
contacts, stable contact IDs, X/Y, Tip and Confidence, with optional width,
height and pressure. It also declares click-pad, pressure-pad and discrete-pad
button implementations. The CapyIO contract needs those semantics without
becoming a HID descriptor or Windows ABI.

## Decision

Add a distinct StandardPort Profile:

```text
capyio.input.touchpad-frames/1
format: touchpad-frame-v1
```

The Profile owns:

- `TouchpadDescriptor`: physical himetric size, a maximum of 3..=5 contacts,
  button implementation and optional contact-size/pressure capabilities;
- `TouchpadFrame`: one complete active-contact snapshot or `cancel_all`;
- `TouchpadContact`: stable contact ID, himetric X/Y, mandatory confidence and
  optional bounding size/pressure;
- `TouchpadButtonState`: the integrated surface-button state;
- `TouchpadFrameTracker`: stream/epoch/sequence/timestamp validation and
  fail-safe suppression after gaps or epoch changes until `cancel_all`;
- `TouchpadMetrics` and a bounded deterministic `TouchpadFixture`.

An ordinary update omits contacts that have left the surface. A Projection
retains the last accepted position long enough to produce a platform-specific
release report. `cancel_all` carries no contacts and a released button; it is
the explicit fail-safe for Android cancellation, Route stop/offline, peer loss,
Adapter failure and reconnect. After a sequence gap or explicit epoch advance,
the tracker suppresses updates until `cancel_all` arrives.

Add `Touchpad` as a distinct Core Capability class and append wire enum value
16 to protocol v1. This is append-only pre-alpha evolution; existing enum
numbers and fields are unchanged. High-rate touchpad frames do not enter the
Protobuf control envelope.

`capyio.input.touch-events/1` remains unchanged for generic touch surfaces. A
named Converter may map it to pointer events or touchpad frames only when its
declared policy is explicit; Profile identity is never silently reinterpreted.

## Consequences

- Android touch areas and Windows physical touchpads can share one
  direction-neutral receiver contract.
- Windows synthetic/VHF projections receive raw multi-contact semantics rather
  than sender-recognized mouse commands.
- A five-contact upper bound makes frame allocation and validation bounded and
  matches the Windows target; sources with more contacts must suppress new
  contacts according to Adapter policy rather than truncate an accepted frame.
- Physical size and contact geometry use himetric units (1/100 mm), avoiding
  display pixels, DPI scaling and Windows structure layouts in the Profile.
- The v1 Profile does not include HID certification blobs, scan-time rollover,
  azimuth, haptic waveforms, network framing or transport security.
- Android mapping, Windows injection and VHF remain later platform slices.

## References

- Microsoft Windows Precision Touchpad collection:
  <https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-windows-precision-touchpad-collection>
- Microsoft sample report descriptors:
  <https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-sample-report-descriptors>
- ADR 0041: user-mode synthetic Precision Touchpad before VHF.
