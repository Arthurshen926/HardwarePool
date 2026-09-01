# ADR 0043: Project touchpad snapshots through bounded platform DTOs

Status: accepted

## Context

ADR 0042 defines complete, direction-neutral `TouchpadFrame` snapshots. Android
`MotionEvent` and Windows `POINTER_TYPE_INFO` have different lifecycle shapes:

- Android includes the pointer being removed in `ACTION_POINTER_UP` and
  `ACTION_UP`, so a complete post-event snapshot must omit the action index;
- Android pointer indexes are transient but pointer IDs remain stable from down
  until up/cancel;
- Windows synthetic touchpad injection consumes batches of pointer structures,
  where active contacts carry in-range/in-contact flags and removed contacts
  must be submitted once with those flags cleared;
- one complete snapshot can replace contacts, making active plus released
  records exceed the synthetic device's five-contact batch bound; and
- Android event time is in the device uptime clock domain and cannot safely be
  copied into Windows `dwTime` or QPC fields after network transit.

Mirroring either native structure in Core would violate the platform boundary.
Recognizing gestures on Android would also prevent Windows from applying the
user's native touchpad policy.

## Decision

Add two platform-owned, hardware-free mappings:

1. `capyio-android-host` accepts a narrow Rust DTO containing the values that a
   future Kotlin/JNI boundary reads from `MotionEvent`. It validates the local
   surface, finger tools, pointer IDs, action index, finite coordinates,
   pressure and timestamps before producing `TouchpadFrame`.
2. `capyio-windows-input` owns an allocation-free stateful projector. It
   compares accepted complete snapshots with at most five retained contacts,
   emits active and one-shot released/cancelled records, and encodes them as
   `POINTER_TYPE_INFO` with PT_TOUCHPAD and himetric locations.

When active plus released records exceed five, Windows projection emits a
release/cancel batch first and the complete active batch second. Each batch is
bounded to five contacts and one projected frame is bounded to two batches.
Initial, post-gap and post-epoch cancellation barriers from `TouchpadFrameTracker`
remain authoritative. A gap immediately projects cancellation for retained
native contacts, then suppresses new contacts until upstream `cancel_all`.

The Windows native encoder follows Microsoft's sample flags:

- active: `POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT`, plus
  `POINTER_FLAG_CONFIDENCE` when declared;
- ordinary release: confidence only;
- abrupt cancel: confidence plus `POINTER_FLAG_CANCELED`.

It fills `ptHimetricLocation` and `ptHimetricLocationRaw`, leaves pixel and
cross-clock timestamp fields zero, and does not submit the resulting batch in
this slice. Optional contact size/pressure remain in the portable/projected DTO
but are not copied into pixel-defined Win32 contact rectangles without a
documented conversion. Integrated-button injection also remains unclaimed;
the Android touch-area mapping declares a non-clickable surface and Windows can
still recognize contact taps and multi-finger motion.

## Consequences

- Android action-index semantics and Windows release semantics are separately
  testable without JNI, an APK, a synthetic device or desktop input injection.
- Windows continues to receive raw contacts and owns gesture recognition.
- Projection is deterministic, fixed-capacity and transactional on invalid
  frames or timestamps.
- This slice does not prove cursor motion, tap-to-click, pan/zoom, global
  three/four-finger actions, Settings integration or process-loss cleanup.
- A later controlled lab step may submit the encoded batches only with an
  explicit injection command and retained physical acceptance evidence.

## References

- Microsoft Precision Touchpad Programming Guide:
  <https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/precision-touchpad-guide>
- Microsoft synthetic creation options:
  <https://learn.microsoft.com/en-us/windows/win32/api/winuser/ne-winuser-synthetic_device_creation_options>
- Android `MotionEvent` reference:
  <https://developer.android.com/reference/android/view/MotionEvent>
