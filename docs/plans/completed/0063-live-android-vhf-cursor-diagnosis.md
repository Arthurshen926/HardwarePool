# CAPY-PTP-003V — live Android VHF cursor diagnosis

Status: completed

## Goal

Identify and correct why live one-contact Android motion did not appear to move
the Windows cursor even though fixed VHF motion and live Shell gestures passed.

## Acceptance evidence

- The separately gated VHF-only option anchors the interactive cursor once and
  accepts only an exact-one-contact gesture with at least 100 himetric of source
  motion. Invalid combinations fail before binding or device creation.
- It reports first/last source timestamps and coordinates plus the real Windows
  cursor delta. Four CLI/motion tests, formatting and Clippy pass.
- The physical run submitted all 155 frames and moved the Windows cursor from
  `(960,600)` to `(794,239)` after ignoring three two-contact gestures.
- Android sender initialization ordering no longer overwrites a successful
  connection status. Losing the foreground closes and finishes the Activity.
- An earlier unchanged-cursor sample taken while CapyIO was not the resumed
  phone Activity is classified as invalid for the user's actual topology. The
  separate-computer UU rendering discrepancy remains outside this completed
  host-cursor acceptance.
- The exact v1.0 APK installation was authorized and verified with only
  `INTERNET`. No driver change or Windows restart occurred.

## Scope boundary

The diagnostic uses `SetCursorPos` only for an initial measurable anchor. It
does not synthesize fallback mouse input or mask Precision Touchpad reports.
Persistent lab use continues separately as CAPY-PTP-003Z.
