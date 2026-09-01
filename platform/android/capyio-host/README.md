# CapyIO Android Host boundary

This crate owns the platform-side pure data boundary for a future unified
Android Node host. `CAPY-PTP-002A` adds a narrow `MotionEvent` DTO mapper; it:

- requires an initial `cancel_all` for each stream epoch;
- maps Android post-event pointer lifecycles to complete `TouchpadFrame`
  snapshots;
- preserves stable pointer IDs and scales finite local pixel coordinates to
  the declared himetric touchpad surface;
- clamps calibrated Android pressure above 1 to the Profile full-scale value;
- accepts finger tools only and rejects malformed action indexes, duplicate
  IDs, excessive contacts and timestamp regression transactionally.

`CAPY-PTP-002N` adds the runtime-facing capture session above that mapper. It:

- tracks at most five active pointer IDs without a growing collection;
- rejects `MOVE`, pointer-down, pointer-up and final-up events that do not
  preserve the preceding Android pointer lifecycle;
- emits explicit cancellation when capture starts, stops or closes;
- supports stop/restart while preserving the stream sequence; and
- commits lifecycle state only after mapping succeeds.

The DTO is not a Rust memory layout for JNI. Future Kotlin code must copy the
explicit fields from `MotionEvent`, including its action index and complete
pointer array.

The session is the pure Rust boundary a future Kotlin callback will call; it
does not create a thread, queue, socket or Android component. There is still no
JNI/Gradle project, Android manifest, permission, foreground service,
notification, APK or physical-device action in this crate.

The remote-touchpad Adapter's `PrivateTouchpadPacketSource` now accepts the
session's frames through a tested composition boundary. Android pointer
lifecycle ownership remains here while private packet framing stays owned by
the Adapter.
