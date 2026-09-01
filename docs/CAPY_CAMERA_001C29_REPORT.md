# CAPY-CAMERA-001C29: Service-owned Android camera capture

## Outcome

The Android camera, encoder and exporter are now owned by one unexported
foreground service instead of `MainActivity`. Capture is started only by a
visible, user-initiated action after CAMERA permission is granted. Once active,
Activity pause, task removal and handled portrait/landscape changes do not stop
the session. An ongoing notification provides an explicit Stop action.

The service uses the C28 encoder-only `Camera2Session`, so Activity surface loss
cannot invalidate the capture request. The current UI intentionally has no
local phone preview. Capture configuration is not persisted and the service is
`START_NOT_STICKY`; process or device restart therefore cannot silently resume
the camera.

## Android boundary

- package: `io.capyio.camera.lab`
- service: `.CameraCaptureService`
- exported: `false`
- foreground type: `camera`
- stop with Activity task: `false`
- permissions: CAMERA, INTERNET, FOREGROUND_SERVICE,
  FOREGROUND_SERVICE_CAMERA
- APK SHA-256:
  `8487317735E6EBDBB45B0AC4938826B6EEB529BA5F6FEBC3E31EDEA46E62E2B0`

## Validation

- `:camera-contract:contractTest`: PASS.
- isolated offline `:app:assembleDebug`: PASS.
- isolated offline `:app:lintDebug`: PASS with warnings treated as errors.
- `cargo xtask ci`: PASS.
- `python scripts/validate_repository.py`: PASS.
- APK manifest permissions inspected with Android build-tools `aapt2`: PASS.
- ADB target `100.66.157.119:35491` authenticated as product PD2419, model
  V2419A, device PD2419.

The initial pairing attempt against `100.66.157.119:41071` returned an ADB
protocol fault, but the previously paired authenticated connect endpoint
`100.66.157.119:35491` was already online. No pairing material was written to
the repository.

## Pending physical evidence

The exact C29 APK was installed after a post-hash approval. It started one
foreground `camera` service and Camera ID 0 remained active for a 15-second
background interval. Returning to the Activity preserved the same process and
showed advancing 1280x720 encoder counters. A subsequent Stop/Start exposed a
separate UI defect: service/camera state restarted correctly but the Activity
retained old counters because its polling loop had stopped. C30 replaces this
single-owner polling dependency with a coalesced package-scoped state signal.

After installation, the remaining regression is:

1. start trusted-LAN capture while the Activity is visible;
2. verify changing Windows Camera pixels;
3. background the Activity and verify the same service/camera stream continues;
4. return and rotate portrait/landscape/portrait without stream loss;
5. use the notification Stop action and verify placeholder fallback;
6. restart capture and verify live recovery;
7. remove temporary Windows registration/deployment and verify FrameServer,
   mapping, process, port and camera state return to preflight.
