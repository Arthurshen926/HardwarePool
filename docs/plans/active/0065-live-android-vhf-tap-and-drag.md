# CAPY-PTP-0041 — live Android/VHF tap-and-drag diagnosis

Status: completed

## Goal

Attribute and correct the one remaining user-observed interaction gap: a short
first tap followed by a held, moving second contact does not start a Windows
tap-and-drag operation over the otherwise accepted Android-to-VHF path.

## In scope

- read the Windows Precision Touchpad tap-and-drag setting without changing it;
- add a deterministic, closed double-tap-and-drag VHF fixture and hardware-free
  projection tests;
- observe the fixed fixture against an isolated desktop drag target;
- add bounded timing evidence for a physical Android tap-and-drag attempt;
- change Android sampling only if the evidence identifies a sender-side timing
  or frame-shape loss.

## Out of scope

- changing Windows touchpad settings on the user's behalf;
- recognizing Windows gestures or synthesizing mouse events on Android;
- modifying or reinstalling the VHF driver package;
- production reconnect, authentication or non-ADB transport.

## Acceptance criteria

1. Read-only Windows evidence records whether tap-and-drag is enabled.
2. The deterministic fixture contains two separately released/pressed contacts,
   a bounded inter-tap gap and motion only during the held second contact.
3. Hardware-free fixture and projection tests pass.
4. A hash-pinned, explicitly gated VHF run records whether Windows produces a
   held primary-button drag on an isolated target.
5. A physical Android comparison either produces the same drag or retains an
   exact, bounded timing/frame trace that identifies the next correction.
6. No driver, boot policy, Android permission or Windows restart change occurs.

## Safety

The fixed VHF run is desktop input and uses an isolated target. It requires the
existing explicit human authorization and the already installed exact driver;
it does not deploy or alter that driver.

## Current evidence

- The fixed VHF sequence produces a Windows held drag, so the installed device
  and Windows `TapAndDrag` setting can support the operation.
- Physical Android comparison preserves source/arrival timing and submits every
  traced frame, while the user reports that a deliberately slower second
  contact can drag and a rapid second contact is commonly interpreted as a
  click.
- The VHF driver already converts a disappearing active contact into the
  protocol-required same-ID, last-position, tip-clear release report; a missing
  lift report is not the remaining cause.
- Android v1.6 removes the app's 24 ms initial one-finger MOVE suppression and
  resets MOVE sampling at each `ACTION_DOWN`; the 72 ms added-contact window is
  unchanged. It adds a one-shot, system-setting-respecting drag-start haptic as
  a local diagnostic. Unit tests, debug lint and assembly pass. Physical
  comparison remains pending.
- The user reports that v1.6 makes drag response approximately acceptable.
  Android v1.7 replaces setting-gated View haptics with an explicitly
  authorized `VIBRATE`-backed, persistent in-app switch; the user confirms the
  drag-start effect. Android v1.8 adds one light tick for each qualified tap so
  ordinary double-click feedback consists of two discrete ticks; the user
  confirms both tap and drag feedback work.
- The user reports that dragging a file is still less reliable than drawing an
  empty-desktop selection rectangle. Android v1.9 adds a persistent weak,
  medium or strong haptic setting. The VHF lab Sink now projects the exact
  original Android frame stream through a bounded tap-drag compatibility latch:
  a nearby second one-finger contact within 500 ms holds ClickPad Button 1 from
  its initial down frame through release. Far, late and multi-contact attempts
  do not latch. Synthetic/user32 behavior and the private wire contract remain
  unchanged. Nine receiver tests, Clippy, the Android unit tests and lint pass;
  the user reports that ordinary interaction and file dragging are now
  approximately normal.
