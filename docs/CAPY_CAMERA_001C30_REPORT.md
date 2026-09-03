# CAPY-CAMERA-001C30: Foreground-service UI state delivery

## Outcome

C29 proved service-owned Camera2 continuity while the Activity was backgrounded
for 15 seconds. It also exposed a UI-only defect: after a service Stop/Start,
the service record and CameraService client were fresh but the visible frame
counters remained from the preceding session.

C30 keeps the 250 ms visible-Activity poll as a fallback and adds a coalesced
state signal. Camera/codec callbacks only replace the immutable snapshot and
post one replaceable main-thread task. That task sends a package-scoped explicit
broadcast with no payload. `MainActivity` subscribes only while started, uses a
non-exported dynamic receiver on API 33+, and reads the same immutable in-process
snapshot. On API 29-32 the platform has no receiver export flag; the sender's
fixed package is documented next to the narrow lint suppression.

## Artifacts

- Android C30 APK SHA-256:
  `B5CE536B29B24018B31E1A5CFE742CA9E473682322B7719E3028167DD9744A64`
- Windows receiver SHA-256:
  `E1541F739572294D14FB740290A7AF09E334D0D05A888FE8C62805C4E0C81833`
- Windows virtual-lab SHA-256:
  `831B54C85903E64D88F993FC35852D75BD85B84C5218F427832394D32D56C82D`
- Windows COM DLL SHA-256:
  `4C236858C5223B4A1303E825496EBE6799C52E9EAE366DC6DE41C8E9A88F70F0`

## Validation

- Android contract test: PASS.
- isolated offline APK assembly: PASS.
- strict Android lint: PASS.
- APK permission inspection: CAMERA, INTERNET, FOREGROUND_SERVICE and
  FOREGROUND_SERVICE_CAMERA only.
- structural validation and the full C29 repository CI baseline passed before
  this focused UI repair; final structural validation is required after the
  hash-pin update.

## Physical evidence on 2026-09-01

The approved C30 APK was installed on the V2419A at
`100.66.157.119:35491`. A fresh Activity start opened camera ID 0 from a
camera-typed foreground service. The approved elevated Windows lab accepted a
trusted-LAN session at 1280x720, 30 fps and 4 Mbit/s, created the production
Global mapping, and enumerated exactly one CapyIO virtual camera.

The receiver decoded and published 3,600 NV12 frames. Its first and last frame
checksums differed (`d9f80140e935d059` and `af12e65e238d1e7a`), which proves
changing camera pixels reached the Windows shared ingress rather than a static
placeholder. During a 12-second Home/background interval and an explicit
portrait-to-landscape rotation, Android retained the same PID (`31153`), the
foreground-service record and the active CameraService client. Rotation settings
were restored to their original `accelerometer_rotation=1` and
`user_rotation=0` values.

## Ordinary Windows Camera evidence and cleanup

The Windows Camera package does not appear in Start Apps or the current-user
Appx inventory, but its registered `microsoft.windows.camera:` protocol opened
the installed application. Windows Camera displayed changing phone pixels from
the exact CapyIO virtual device. During an additional Home/background plus
portrait/landscape exercise, Android retained PID `5574`, the camera foreground
service and the active CameraService client; Windows Camera continued showing
the live desk scene.

The visual run exposed a remaining orientation defect: V2419A camera ID 0 emits
sensor-native pixels that appear 90 degrees counter-clockwise in the fixed
landscape virtual-camera profile. The device reports only rotate-and-crop mode
`0`, so Camera2 cannot correct it in the HAL. That defect is carried into C31.

The 3,600-access-unit receiver bound also terminated at roughly 120 seconds,
before the advertised 180-second GUI hold, so this run did not prove the planned
stop-placeholder/restart transition. Android was then force-stopped and had no
active CameraService client. Windows Camera was closed, the explicitly approved
`RemoveWithFrameServerRestart` action returned exit code 0, and the final
read-only preflight proved clean ProgramData, CLSID, process and TCP 38173 state.
