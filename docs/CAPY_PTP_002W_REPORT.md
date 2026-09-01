# CAPY-PTP-002W Report

Date: 2026-08-30

Status: active; the physical five-contact boundary is proven, while cursor
rendering and Windows global three-/four-finger actions remain unresolved.

## Reported failure and cause

The previous accepted 002V run used an ADB-generated swipe after the listener
and reverse tunnel were prepared. After cleanup, opening the Activity without a
listener made its one-shot socket connection fail and permanently stopped the
capture state. `onTouchEvent` then returned `false`, explaining the reported
absence of response from a real finger.

## Change

- touch is always consumed and rendered by the foreground View;
- before the first connection, input is not sent into the Rust session, so the
  private sequence and contact lifecycle remain unchanged;
- the sender retries the loopback-only endpoint at a bounded 500 ms interval;
- after connection, transport failures still fail closed rather than silently
  reconnecting an ambiguous epoch;
- immersive system bars, a full-surface system-gesture exclusion region and
  parent-interception denial are reapplied while the Activity has focus;
- `performClick` preserves the custom View accessibility contract.
- high-rate MOVE events are sampled before JNI at a 16 ms minimum interval and
  skipped while four motion records are already pending; lifecycle records are
  retained, so queue pressure cannot consume their reserved capacity;
- `--manual-session` explicitly raises the receiver's bounded idle timeout from
  30 seconds to 600 seconds without changing the automated default.

These Android controls do not guarantee suppression of vivo or other OEM global
multi-finger gestures intercepted before the app receives `MotionEvent`.

## Device evidence

The first version of the gesture-exclusion change exposed a vivo-specific
startup crash because `WindowInsetsController` was requested before DecorView
creation. The exact stack identified `TouchpadLabActivity.applyTouchpadWindowMode`.
Moving the call after `setContentView` fixed launch.

The first physical sustained touch then exposed bounded queue pressure from the
phone's high-rate MOVE stream. The exact failure was `bounded record queue is
full`; this is the evidence that motivated pre-JNI sampling rather than a larger
latency-building queue.

Installed version 5 has SHA-256
`c8087d2a7e060739be8c9255028d6c6c594bd47e0de9bd6772afd38e7df9497b`,
declares only `android.permission.INTERNET`, and reached:

```text
hello_binding=accepted
device_creation=created
idle_timeout_seconds=600
```

The version 5 receiver and reverse mapping were retained for the user's
physical-touch check. Returning from the Activity exposed a close-drain bug and
the receiver failed closed on EOF; its dropped Sink cleaned up the virtual
device. Version 6 fixes connected shutdown by draining Close records. No system
gesture setting, driver or boot policy changed.

The user then confirmed physical one-finger motion and two-finger vertical
scrolling on version 5. No cursor was visually present, so a tap could not yet
be judged. Three- and four-finger swipes produced no visible Windows action;
vivo intercepted three-finger downward motion as a phone screenshot.

An interactive-desktop `GetCursorInfo` query nevertheless reported the Windows
cursor as showing at `(960, 540)`. `SPI_GETMOUSEVANISH` reported enabled. This
does not prove that a capture/display path composites the hardware cursor, so
version 6 adds a centered click-target harness and contact-count telemetry
instead of relying on cursor visibility. Android now logs effective contact
transitions and displays current/maximum contacts; the receiver prints its own
decoded transitions and maximum. Activity shutdown now drains the bounded close
records when connected.

The Settings database exposed `three_intercepts_rom_support=1`, consistent with
OEM interception, but it is capability metadata rather than a verified safe
toggle. No vivo setting was changed. The wireless-debug endpoint subsequently
went offline, preventing version 6 installation and exact Settings-UI discovery
in this slice.

The inspected version 6 APK still declares only `android.permission.INTERNET`,
contains only the `arm64-v8a` native ABI, and has SHA-256
`ccd9ec1efce6b49ed207f77ec939f0d50520dd86df217ad825cf8042588dc708`.

Version 6 was installed through the new wireless-debug endpoint
`100.66.157.119:42567`. Pulling the installed base APK produced the same exact
SHA-256, and package inspection reported version code 6 with `INTERNET`
granted.

The first version 6 physical run, before correcting native pointer phases,
processed 415 frames, submitted 413 batches / 536 contact records, observed at
most two contacts and closed cleanly. It also showed that active native records
had incorrectly omitted the required pointer lifecycle flags. The Windows
projector now emits `DOWN` for a new contact, `UPDATE` for a retained contact,
`UP` for release and `CANCELED | UP` for cancellation. This follows Microsoft's
documented [pointer flags](https://learn.microsoft.com/en-us/windows/win32/inputmsg/pointer-flags-contants)
and [touch injection state transitions](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-injecttouchinput).

After that correction, the accepted physical session processed 785 frames,
submitted 783 batches / 1,376 contact records, observed four simultaneous
contacts and closed the native device cleanly. The receiver repeatedly decoded
`2 -> 3` and `3 -> 4` transitions. Therefore the current three-/four-finger
failure is after Android capture and transport rather than a two-contact limit.
Some vivo three-finger sequences still end abruptly and the user observes the
OEM screenshot gesture, so pre-`MotionEvent` interception remains a separate
phone-side conflict.

Windows accepted deterministic native fixtures for one-finger tap and motion,
three-finger swipe and four-finger swipe. The one-finger motion changed the
interactive system cursor from `(1273,68)` to `(482,380)`, while the user still
could not see the cursor move. The user separately confirmed that physical
single-click, one-finger motion and two-finger scrolling work. The click-target
window is therefore not retained as acceptance evidence; its focus/event
routing made its zero count contradict both user observation and system cursor
movement.

While the synthetic device was live, a read-only
`SPI_GETTOUCHPADPARAMETERS` query returned `touchpadPresent=true`,
`touchpadEnabled=true`, `touchpadActive=true`, `maxSupportedContacts=5`, and
tap/pan/zoom enabled. This is direct Windows evidence that
`CreateSyntheticPointerDevice2(PT_TOUCHPAD)` created an active five-contact
Precision Touchpad. It is an ephemeral user-mode synthetic device, not an
installed persistent kernel driver or Device Manager package. Microsoft's
[synthetic device parameters](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-synthetic_device_creation_params)
confirm the `PT_TOUCHPAD`, five-contact, physical-size contract used here.

The Windows Advanced Gestures page was then inspected while the device was
online. Three-finger up/down/left/right were configured for Task View, Show
Desktop and application switching. Four-finger tap and swipes were configured
for Notification Center and virtual-desktop actions. Fixed three- and
four-finger horizontal fixtures still produced no visible action. This rules
out a disabled user gesture policy and supplies the ADR 0041 evidence required
to activate the separately reviewed VHF fallback in ADR 0048.

Targeted Rust test/Clippy, Android assemble/lint and `git diff --check` pass. A
full CI attempt passed check and Clippy, then stopped because Cargo could not
replace the then-running lab-listener EXE (Windows error 5). After the manual
session closed, `cargo xtask ci` passed in full; final `git diff --check` also
passed.

## Remaining acceptance

- diagnose why the system cursor coordinates move but its image is not visible;
- determine why Windows accepts three-/four-contact raw fixtures without a
  visible global action, including the exact configured gesture policy;
- locate the exact vivo screenshot toggle and decide separately whether to
  change that persistent device setting;
- run final full repository validation.
