# CAPY-CAMERA-001C11 — Android front/back camera selection

Date: 2026-08-30

Status: implementation, build validation and bounded V2419A switch verification
complete.

## Objective

Add a user-visible front/back camera choice without changing Android
permissions, background behavior, CAVC framing or Windows projection.

## Implementation

- `CameraFacingPolicy` provides a pure Java, 32-candidate-bounded deterministic
  selection policy. It selects the requested facing when available, otherwise
  falls back to a back camera and then the first valid candidate.
- Camera2 selection now gathers only cameras with a common preview/MediaCodec
  output size and applies that policy before opening the device.
- The foreground Activity adds a front/back switch button. Switching a running
  stream closes the existing CameraCaptureSession, CameraDevice, encoder,
  loopback sender and preview Surface before starting a fresh session.
- Each restarted session creates a new random stream ID and monotonic epoch via
  the existing `LoopbackAvcSender`; camera provenance never changes silently
  within one CAVC stream.
- Pausing the Activity clears a pending restart and executes the existing
  visible-lifecycle close path.

## Evidence

- The no-dependency camera contract test covers toggling, requested-facing
  selection, fallback and the candidate bound.
- Gradle 9.5.0 offline `contractTest :app:assembleDebug :app:lintDebug` passed.
- Android lint verifies compatibility with the unchanged API 29 minimum.
- The debug APK SHA-256 is
  `A143D30009CD071F0CEFD32E3ADF5448D42251406CE9408D369E0D42D2AFD9C0`.
- The APK still declares exactly CAMERA and INTERNET; no service was added.

## Physical evidence

Under the existing exact-device authorization, the rebuilt APK was installed on
`V2419A` at `100.66.157.119:36275`. Existing CAMERA and INTERNET grants remained
true. A temporary ADB reverse lab mapping carried three bounded decode-only
streams:

- back camera ID 0: stream `43a1eb697cb93d7be4fc581f1d4e08f9`, epoch
  `14303803327011`, 90/90 decoded frames;
- front camera ID 1: stream `7a8523a7dcdb92607d51d037405b8740`, epoch
  `14346536204321`, 90/90 decoded frames;
- switched back to camera ID 0: stream
  `08e286227c9f6fadbbe8d1a2eb158cb9`, epoch `14381184968323`, 90/90 decoded
  frames.

All streams were 1280x720 at 30 fps, contained key frames, produced distinct
first/last decoded checksums, enabled Windows decoder low-latency mode and had
zero pending-sample backlog. Camera Service reported the active ID sequence
`0 → 1 → 0`, while the three distinct stream IDs and epochs prove each switch
crossed a new stream boundary.

No camera pixels were retained. Final cleanup force-stopped the app, removed the
ADB reverse mapping and temporary UI XML, and left Camera Service with no active
client or receiver process.
