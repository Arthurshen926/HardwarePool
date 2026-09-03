# CAPY-CAMERA-001C17 — Bounded Android multi-camera inventory

Date: 2026-08-30

Status: implementation and hardware-free validation complete; physical V2419A
inventory remains pending a fresh ADB endpoint and exact install approval.

## Objective

Determine what Camera2 actually exposes before designing long/wide/ultra-wide
Routes or multiple Windows virtual-camera slots. Camera count, vendor marketing
names and the vendor camera application's behavior are not used as concurrency
evidence.

## Implementation

- `CameraInventory` is Android-free and bounds 32 directly openable cameras,
  16 physical lenses per logical device, 16 focal lengths, 32 common sizes, 16
  concurrent groups, eight IDs per group and 65,536 JSON characters.
- The version-1 JSON includes Camera ID, facing, hardware level, sensor
  orientation, logical-camera physical IDs, per-physical-lens focal lengths and
  sensor dimensions, Zoom ratio range, common SurfaceTexture/MediaCodec sizes
  and CameraManager concurrent groups.
- Floating Camera2 values are validated and converted to deterministic integer
  milli-millimetre focal lengths, micro-millimetre sensor dimensions and milli
  Zoom ratios.
- The foreground **Inspect camera capabilities** action requires the existing
  CAMERA grant. It calls only CameraManager/CameraCharacteristics queries. It
  never opens a CameraDevice, creates a Surface, starts MediaCodec, writes a
  file, emits an ordinary log or changes Android settings.
- The manifest remains exactly CAMERA plus INTERNET with no service.

## Automated evidence

- The no-dependency contract executable covers bounded construction, logical/
  physical identity consistency, JSON escaping, focal/sensor/Zoom bounds,
  concurrent groups and malformed orientation.
- Offline `contractTest`, `:app:assembleDebug` and warnings-as-errors
  `:app:lintDebug` pass with minSdk 29 and targetSdk 36.
- `aapt2` reports exactly `android.permission.CAMERA` and
  `android.permission.INTERNET`.
- Debug APK SHA-256:
  `AA6BB8C3B150873494BA006952C7898C843A151FFA10DFDE76AF3FA2911005E5`.

## Remaining controlled evidence

After a new exact ADB endpoint and hash-specific install approval, open the
inventory action on V2419A and retain only the bounded metadata JSON. The result
must identify directly openable IDs, physical-only lenses and advertised
concurrent groups before a multi-camera capture or multi-slot Windows design is
selected. No camera pixels are required.
