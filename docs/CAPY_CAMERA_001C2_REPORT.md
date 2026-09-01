# CAPY-CAMERA-001C2 Camera2/MediaCodec composition report

- Date: 2026-08-30
- Branch: `codex/capyio-camera`
- Base commit: `fc3da3636ca6c969667e71a9b596dcc944380146`
- Scope: Android camera-to-AVC composition plus one explicitly authorized
  foreground device validation

## Outcome

The Android app's active Camera2 request now targets two Surfaces: the visible
`TextureView` preview and the owned MediaCodec AVC input Surface. Camera
selection intersects the sizes advertised for `SurfaceTexture` and
`MediaCodec`, preferring the largest positive even common size no larger than
1280×720. Camera2 capture results supply sensor timestamps and sequence;
encoded access-unit sequence/key-frame/drop metadata is displayed to the user.

The complete Android capture-to-encoder source path compiles, packages and
passes strict lint. An explicitly selected vivo V2419A running Android 16 / API
36 accepted the two-Surface session and its vendor AVC encoder produced output.
This validates the local phone capture/encode slice only; no encoded payload
was exported, decoded on Windows or exposed through the virtual camera.

## Runtime bounds

- Exactly two Camera2 request targets: visible preview and encoder Surface.
- No raw YUV copy or image storage in the composed path.
- At most eight encoded queue entries drained per camera capture result.
- Activity displays only counts and metadata; encoded bytes remain internal.
- Stop/pause closes capture session and camera before codec/Surface release.
- No Internet permission, network listener, service or background capture.

## Evidence

The following command passed after composition:

```text
gradle contractTest :app:assembleDebug :app:lintDebug
```

The device-installed debug APK is 2,603,310 bytes with SHA-256
`e78d4bc575f2db633665035663ab17e8eb496eaa5ac17e276bde74bf20293cdf`.
The device-side `sha256sum` and `stat` results match those exact values.
`aapt2 dump permissions` reports only `android.permission.CAMERA`.

The post-run local regression rebuild is 2,586,825 bytes with SHA-256
`18fb16c499ad767f47bd2d84ec2eeb94448e9c2aec4fa9756af71197486511b7`.
It also declares only CAMERA and was not installed; the device evidence below
therefore refers only to the first hash-recorded APK.

The approved device run produced the following bounded evidence without
capturing or retaining preview images:

- installation returned `Success`; package version was `0.1.0` and the CAMERA
  permission was initially `granted=false`;
- after the user granted the visible system prompt, Camera Service recorded two
  camera-0 connect/disconnect pairs for `io.capyio.camera.lab`;
- MediaMetrics recorded `c2.mtk.avc.encoder`, AVC encoder mode, 1280x720,
  4,000,000 bit/s and Surface color format for both sessions;
- the longer completed session encoded 779 frames / 12,974,312 bytes over
  25,931,114 microseconds of encoded duration;
- after the user switched away, Camera Service reported
  `Active Camera Clients: []`, while the app process remained alive and the
  process-exit history contained no crash entry.

Vendor logcat emitted unavailable optional media-quality/performance-service
messages, but no application exception or fatal process exit was observed. The
wireless ADB endpoint and all preview content are intentionally omitted from
repository evidence.

Offline repository validation now requires the MediaCodec size inventory,
encoder request target and capture callback source boundaries. It also retains
the exact-permission/no-service and no-network/file/log callback checks.

## Remaining gates

1. Add a deterministic device-test observation path for application-level
   key-frame, codec-configuration and queue-drop counters. The OS-level device
   evidence proves AVC output, but does not preserve those three app counters.
2. Fix private H.264 parameter-set/access-unit framing and add authenticated
   transport without routing payloads through JSON-RPC.
3. Decode on Windows to packed 720p30 NV12 and feed `camera-host`.
4. Switch registered MF activation from fixture to the shared consumer and run
   the approved ordinary-camera application roundtrip.
