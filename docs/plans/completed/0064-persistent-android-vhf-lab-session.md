# CAPY-PTP-003Z — persistent Android-to-VHF lab session

Status: completed

## Goal

Separate bounded continuous touchpad use from one-shot cursor diagnostics so a
successful gesture does not itself close the Android/Windows lab session.

## Acceptance criteria

- A hash-pinned elevated wrapper selects VHF and the existing bounded
  `--manual-session` mode without any release-exit or cursor-anchor option.
- The session remains active across multiple one- through four-contact gestures
  and closes only on Android Close, transport failure or the existing 600-second
  idle deadline.
- The Android status explains that reopening the page also requires a running
  Windows receiver and that leaving CapyIO transfers touch ownership away from
  the Activity.
- Android assembly/lint and PowerShell parsing pass; the APK still declares only
  the already approved `INTERNET` permission.
- No driver change, Android permission change or Windows restart occurs.

## Completion record

The Android 1.2 APK and hash-pinned receiver established one continuous VHF
session. It processed and submitted all 4,356 frames, reached four contacts and
closed cleanly after the Android Activity closed. The user confirmed every
requested interaction except double-tap followed by drag; that distinct gap is
not part of persistent-session lifecycle acceptance. Exact evidence is recorded
in `docs/CAPY_PTP_003Z_REPORT.md`.

## Scope boundary

This remains an ADB-reverse local lab. It does not add authenticated production
transport, an Android overlay, background touch capture or a Windows service.
It does not make any claim about how a separate-computer remote viewer renders
the Windows cursor.
