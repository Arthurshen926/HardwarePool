# CAPY-CAMERA-001C20 — Vendor-neutral Camera ID and Zoom presets

Date: 2026-08-31

Status: implementation, hardware-free validation and exact-hash V2419A
single-route validation complete.

## Objective

Remove physical-camera topology from the ordinary source-selection workflow.
The baseline should work on Android devices that expose useful logical-camera
Zoom but hide physical IDs, report incomplete focal metadata or reject direct
physical OutputConfigurations.

## Implementation

- The selectable source is now only a directly openable Camera ID plus an
  optional standard Zoom target. Physical IDs and focal lengths remain in the
  read-only C17 diagnostic inventory but do not control capture.
- Each Camera ID contributes one automatic choice and at most three bounded,
  deterministic presets derived only from its advertised Zoom range:
  minimum Zoom when below 1x, 1x when supported, and 2x when supported.
- Unsupported targets are omitted and duplicate targets are never emitted.
  A camera without a Zoom range still retains its automatic choice.
- Stable labels use `cameraId@auto` or `cameraId@N.NNNx`. Front/back selection,
  full-close stream restart, quality, CAVC transport and Windows decode remain
  unchanged.
- The capture request continues to use `CONTROL_ZOOM_RATIO` only on API 30+;
  no physical OutputConfiguration is created.
- No permission, service, network endpoint, wire format or Windows system
  behavior changed.

## Automated evidence

- The dependency-free contract test covers the automatic/minimum/1x/2x order,
  exact labels, bounded cycling, unsupported targets and invalid ratios.
- Offline contract tests, API-29 debug APK assembly and warnings-as-errors lint
  pass.
- `aapt2` reports exactly CAMERA and INTERNET, minSdk 29 and targetSdk 36.
- Debug APK SHA-256:
  `6618D3823985110267D032B75138FD9B6FD6D3870D54ACF30CF2187C6BE19E68`.

## Physical evidence

After exact-hash approval, the device-side APK hash matched C20 and the
existing CAMERA grant remained present. The complete UI cycle was exactly:

`automatic`, `0@auto`, `0@0.670x`, `0@1.000x`, `0@2.000x`, `1@auto`,
`1@1.000x`, `1@2.000x`.

Two isolated endpoint runs created fresh CAVC streams/epochs and completed 30
changing 1280x720 frames through the inbox Windows decoder:

- `0@0.670x`: first/last checksums `afa41e062f7c5f33` /
  `9203449974c4d990`;
- `0@2.000x`: first/last checksums `dbee8b0afcf84484` /
  `ecc7be237fdea9a8`.

Both runs reported low-latency decoder mode and zero pending-sample backlog.
Cleanup stopped the Activity and removed the reverse mapping; the explicitly
authorized C20 APK remains installed.
