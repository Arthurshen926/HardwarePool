# CAPY-CAMERA-001C18 — Explicit logical/physical Camera2 source selection

Date: 2026-08-31

Status: implementation and hardware-free validation complete; V2419A physical
evidence shows the direct dual-physical-output strategy is not usable.

## Objective

Turn the C17 inventory into a bounded foreground source selector so one AVC
Route can test each directly openable Camera2 ID and each physical lens exposed
by a logical multi-camera. This slice does not claim simultaneous capture or
create multiple Windows virtual-camera devices.

## Implementation

- `CameraSourceSelection` deterministically expands the bounded C17 inventory
  into a directly openable source followed by its declared physical lenses.
  A stable key uses `cameraId` or `cameraId/physicalCameraId`; cycling returns
  to automatic front/back selection after the last entry.
- The Camera Lab adds a foreground source button. Selecting a source while
  streaming uses the existing full-close restart, so the replacement stream
  receives a new CAVC identity and epoch.
- A direct source opens its Camera ID unchanged. A physical source opens its
  owning logical Camera ID and applies the physical Camera ID to both bounded
  `OutputConfiguration` instances before session creation.
- Stream sizes are selected from the physical lens characteristics when a
  physical source is selected. Facing and orientation remain properties of the
  owning logical camera.
- No permission, service, socket, wire format, queue bound or Windows
  registration behavior changed.

## Automated evidence

- The no-dependency contract test covers logical/physical enumeration, stable
  keys, focal metadata, bounded cycling, automatic fallback and malformed IDs.
- Offline contract tests pass.
- Offline API-29 debug APK assembly and warnings-as-errors lint pass. The final
  Android build was run outside the filesystem sandbox because Windows ZipFS
  could not close the generated `R.jar` inside the sandbox; the fixed Gradle,
  Android plugin and offline dependency set were unchanged.
- `aapt2` reports minSdk 29, targetSdk 36 and exactly CAMERA plus INTERNET.
- Debug APK SHA-256:
  `5700E23B46DE48C571B0949250D2122E8215234FEE933979BA692400633CF471`.

## Physical evidence

After exact authorization, the APK was installed on the identified V2419A and
the bounded C17 inventory reported logical back camera `0`, physical IDs
`2/3/4` with 6.54/13.30/2.32 mm focal metadata, front camera `1`, and advertised
concurrency group `[0,1]`. Logical source `0` completed and decoded 90 changing
1280x720 frames. Each direct physical source `0/2`, `0/3` and `0/4` configured
but stopped at one capture callback with zero encoded access units; the bounded
Windows receiver timed out. The Activity, receiver and reverse mapping were
then stopped/removed. C19 replaces this device-incompatible mechanism with an
explicit logical-camera Zoom target and makes no exact-sensor claim.
