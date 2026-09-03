# CAPY-CAMERA-001C19 — Logical-camera Zoom lens targets

Date: 2026-08-31

Status: implementation, hardware-free validation and exact-hash V2419A
single-route validation complete.

## Objective

Provide a usable main/tele/ultra-wide foreground choice after C18 evidence
showed that V2419A advertises physical IDs but stalls when both preview and AVC
encoder OutputConfigurations are bound directly to one physical ID.

## Implementation

- The bounded source model retains the inventory physical ID and focal metadata
  as a user-facing lens target, but does not claim that Android will lock the
  corresponding sensor.
- Each physical focal length is divided by the logical camera's baseline focal
  length and rounded to a milli-Zoom target, then clamped to the advertised
  `CONTROL_ZOOM_RATIO_RANGE`.
- V2419A inventory therefore produces approximate targets `0/2 = 1.000x`,
  `0/3 = 2.034x` and `0/4 = 0.670x`.
- Preview and encoder remain ordinary logical-camera outputs. On API 30 or
  later, the repeating request applies `CONTROL_ZOOM_RATIO`; API 29 retains
  automatic logical-camera selection rather than invoking an unavailable API.
- UI and stream status include the physical inventory key and Zoom target so a
  target is never represented as proof of a particular active sensor.
- No permission, service, socket, CAVC record, queue or Windows registration
  behavior changed.

## Automated evidence

- Contract tests cover focal-ratio calculation, rounding, advertised-range
  clamping, stable cycling and invalid target construction.
- Offline contract tests, API-29 debug APK assembly and warnings-as-errors lint
  pass.
- Debug APK SHA-256:
  `502064D71F017B2D5592E2B158BFEB79447709958A7C9F9A52B081D3857850D8`.

## Physical evidence

After exact-hash approval, the device-side installed APK hash matched the C19
hash and the existing CAMERA grant remained present. Three isolated runs each
created a fresh CAVC stream/epoch and completed 30 changing 1280x720 access
units through the inbox Windows decoder:

- `0/2@1.000x`: first/last checksums `16cb11571139509d` /
  `4f1b87b2d57fb133`;
- `0/3@2.034x`: first/last checksums `ae87ad8493c545ab` /
  `b12ddc3cf98b79a2`;
- `0/4@0.670x`: first/last checksums `f2c7340ae7f3c6c5` /
  `6903c13737e0485c`.

Every run reported low-latency decoder mode and zero maximum pending samples.
Changing checksums prove live pixels, not which physical sensor the vendor
selected. Exact active-sensor attribution needs additional trustworthy capture
metadata and is outside this slice. Cleanup stopped the Activity and removed
the reverse mapping; the explicitly authorized APK remains installed.
