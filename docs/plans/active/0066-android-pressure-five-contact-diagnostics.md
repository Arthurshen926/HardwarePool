# CAPY-PTP-0042 — Android pressure and five-contact diagnostics

Status: completed

## Goal

Collect physical-device evidence before deciding whether approximate Android
pressure can safely influence touchpad haptics or button state, and verify that
one complete phone gesture can reach five simultaneous contacts without
changing the Windows input contract.

## In scope

- display current per-contact Android pressure and touch-major/minor axes;
- accumulate bounded pressure range, maximum contact axes, sample count,
  five-contact frame count and five-contact gesture count;
- count at most one five-contact reach per complete touch lifecycle;
- ignore invalid diagnostic samples without changing touch delivery;
- retain Android v1.9 packet, haptic and VHF behavior unchanged.

## Out of scope

- submitting pressure, contact geometry or Mechanical Force to Windows;
- changing click, drag, gesture or vibration decisions from pressure;
- changing the VHF descriptor or reinstalling the driver;
- assigning a Windows action to a five-finger gesture.

## Acceptance criteria

1. Pure tests cover pressure/geometry accumulation, one five-contact reach per
   gesture and transactional rejection of invalid/ambiguous samples.
2. Android debug unit tests, assembly and lint pass.
3. APK permissions remain exactly `INTERNET` and `VIBRATE`.
4. A physical run records a non-empty pressure range and either reaches five
   contacts or retains the phone/OEM limitation as evidence.
5. Existing one- through four-finger input remains unchanged.

## Current evidence

- Android v1.10 adds a UI-only `TouchpadInputDiagnostics` observer. It reads
  `getPressure`, `getTouchMajor` and `getTouchMinor` for currently visible
  contacts, but the existing JNI record session receives the original
  `MotionEvent` independently.
- Three pure tests pass. The debug APK, unit-test task and lint pass. The APK is
  version code 20 / version 1.10 with SHA-256
  `C80E7718D342C919959C2D45DB4F482DAD1444B35A25E43F1FDDFBEDF1BAA474`.
- Version 1.10 was installed through the explicitly authorized wireless ADB
  endpoint `100.66.157.119:44801`; its permissions remain exactly `INTERNET`
  and `VIBRATE`. The VHF session is connected and the Activity is top-resumed.
- Initial UI evidence at
  `target/lab-evidence/CAPY-PTP-0042-android-pressure-ui.png` shows 675 samples
  with pressure fixed at `1.00..=1.00`, maximum reported contact axes
  `1.60 x 1.60`, and zero five-contact frames. This was preliminary rather than
  a controlled light/normal/firm-pressure comparison; the subsequent user test
  below supersedes it.
- After the user deliberately completed the pressure and five-finger procedure,
  `target/lab-evidence/CAPY-PTP-0042-android-pressure-after-test.png` records
  9,074 accepted diagnostic samples, pressure fixed at `1.00..=1.00`, maximum
  contact axes `2.40 x 1.60`, 828 five-contact frames, 11 separate gestures
  reaching five contacts, and `maxContactCount=5`. The receiver acknowledged
  2,530 unchanged input frames. The user observed no distinct pressure feedback
  and no visible Windows five-finger action.
- This device therefore supplies no useful finger-pressure dynamic range for a
  pressure-controlled haptic/button experiment. Five-contact capture and the
  existing Android-to-VHF path are working; Windows has no configured standard
  five-finger action in this lab. No pressure behavior or five-finger mapping is
  added.
