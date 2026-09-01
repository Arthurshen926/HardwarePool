# CAPY-CAMERA-001C13 — Bounded foreground AVC quality choices

Date: 2026-08-30

Status: implementation, build validation and exact-build V2419A physical
switch verification complete.

## Objective

Add a minimal foreground quality control to the working Android-to-Windows
camera lab without changing permissions, resolution negotiation, CAVC v1, the
loopback-only transport, Windows projection, or background lifecycle.

## Implementation

- `AvcQualityPreset` defines exactly Economy, Balanced and Clear. Their
  1280x720 targets are 2, 4 and 6 Mbit/s.
- Other negotiated sizes scale bitrate by pixel count and clamp it to the
  existing `AvcEncoderConfig` bounds. Resolution remains the existing largest
  common Camera2/MediaCodec size at or below 1280x720 and frame rate remains
  30 fps.
- The visible Activity cycles the preset and reports the active requested
  bitrate. It retains Balanced as the default for compatibility with all prior
  device evidence.
- A running change closes Camera2, MediaCodec, sender and surfaces before
  opening a replacement session. The replacement therefore receives a fresh
  stream ID and epoch and cannot silently change codec config inside an active
  CAVC stream.
- Queue capacities, key-frame interval, late-receiver retry, fixed loopback
  destination and permission set are unchanged.

## Evidence

- The no-dependency Java contract test covers all three 720p bitrates,
  pixel-count scaling, deterministic cycling and invalid dimension rejection.
- Offline Gradle 9.5.0 `contractTest :app:assembleDebug :app:lintDebug` passed
  with Android Gradle Plugin 9.3.1 from the existing dependency cache.
- The manifest still declares exactly CAMERA and INTERNET and no service.
- The rebuilt debug APK SHA-256 is
  `906AEC1FAB81856826168637B4D07FE63593A0E40A6889C3EFBB6CC9F442DEA4`.
- Repository structural validation, `git diff --check` and the full
  `cargo xtask ci` passed.

## Physical evidence

After exact hash/target authorization, the APK was installed on
`V2419A / PD2419` at `100.66.157.119:42567` and ADB reverse mapped device
loopback port 38173 to the Windows loopback receiver. The device-reported
permissions remained CAMERA and INTERNET.

The Activity started at Balanced and changed to Clear and then Economy while
streaming. Each requested setting produced an independently guarded and decoded
1280x720, 30 fps session:

- Balanced: bitrate 4,000,000; stream
  `f13e76c76d85e0ad5e30a710cbb0f49e`; epoch `35423495800776`;
- Clear: bitrate 6,000,000; stream
  `4984971bfbb28177ee23330f47022d66`; epoch `35463549041701`;
- Economy: bitrate 2,000,000; stream
  `d42173e98ee9d61d32062cce46aee4e9`; epoch `35500376061627`.

Every target session accepted and decoded 90/90 access units with changing
first/last NV12 checksums, one explicit discontinuity and zero decoder pending
sample backlog. The distinct stream IDs and epochs confirm that quality changes
did not mutate codec configuration inside an existing CAVC stream.

Final cleanup force-stopped the Activity, removed the ADB reverse mapping and
left no active Camera Service client or receiver process. The authorized APK
remains installed and no camera pixels were retained.
