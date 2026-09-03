# CAPY-PTP-003T — Android multitouch motion policy

Status: completed

## Goal

Make physical Android three-/four-finger Precision Touchpad gestures easier to
form and less abrupt without changing the already accepted one-/two-finger
motion scale.

## Acceptance evidence

- One- and two-contact motion remains identity mapped.
- Reaching three contacts rebases every active contact without a position jump;
  later deltas use a bounded 700-per-mille scale until complete release.
- Initial and added-pointer stabilization suppresses MOVE only; every pointer
  lifecycle frame is retained.
- Six capture tests cover continuity, attenuation, release reset and bounds;
  four mapping tests and four JNI tests pass.
- Android arm64 JNI, APK compilation and lint pass. The exact v0.7 APK has only
  the previously approved `INTERNET` permission and its installed hash matches.
- The live VHF run processed and submitted 2,165 frames, reached four contacts,
  and closed cleanly. User observation confirms easier Windows gesture effects.

Residual rapid-placement difficulty was traced to phone-side OEM interception
and raw light-contact loss, not the bounded motion scale. It is continued as
CAPY-PTP-003U rather than expanding this completed slice into OEM setting
control.

## Scope boundary

This is bounded spatial conditioning, not Android-side gesture recognition.
Windows remains the sole owner of Precision Touchpad gesture semantics. OEM
gestures intercepted before `MotionEvent` remain outside the app's guaranteed
control.
