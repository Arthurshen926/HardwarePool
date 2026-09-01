# CAPY-CAMERA-001C25 — foreground rotation continuity

Date: 2026-08-31

Status: complete as an authorized V2419A/Windows local-lab regression.
Implementation, structural validation, offline Android contract/build/lint,
full repository CI, exact-hash installation, foreground rotation continuity,
ordinary Windows Camera output, background release and full Windows rollback
all passed.

## Trigger

The authorized C24 regression established the trusted-LAN config, Global
mapping and one Session/CurrentUser virtual-camera enumeration, but rotating the
V2419A recreated `MainActivity`. Android called the existing `onPause` safety
path, which correctly closed Camera2, MediaCodec and the exporter. The Activity
then rebuilt its UI with the deliberately non-persisted Windows address blank,
so the user-visible result was an exited camera session.

This was an Activity configuration-lifecycle defect. It was not a CAVC, TCP,
decoder, virtual-camera or physical-lens-selection failure.

## Outcome

- `MainActivity` declares that it handles `orientation`, `screenSize` and
  `smallestScreenSize` configuration changes. Rotation and bounded display-size
  changes therefore retain the same visible Activity, `TextureView`, Camera2
  session, encoder, exporter, stream identity and in-memory trusted-LAN address.
- `onConfigurationChanged` refreshes only current UI labels and streaming
  status. It never opens, closes or restarts capture.
- The Activity does not lock orientation. Portrait and landscape remain user
  controlled.
- The foreground security boundary is unchanged: a real `onPause`, preview-
  surface destruction, user Stop or failure still closes the complete session.
  The trusted-LAN address remains non-persistent across real Activity/process
  destruction.
- No permission, service, codec, wire format, address, port or Windows binary
  changes are included.

## Automated evidence

- `python scripts/validate_repository.py` — PASS, including exact manifest
  configuration-change tokens, an explicit `onConfigurationChanged` handler
  and rejection of an orientation lock.
- Offline `contractTest :app:assembleDebug :app:lintDebug` — PASS. The first
  sandboxed build reproduced the documented JDK ZipFS `R.jar` access denial;
  the same network-disabled command passed outside the filesystem sandbox.
- `aapt2 dump permissions` — exactly `android.permission.CAMERA` and
  `android.permission.INTERNET`.
- Packaged manifest `configChanges=0x00000c80`, the combined Android flags for
  orientation, screen size and smallest screen size; no `screenOrientation` is
  present.
- `cargo xtask ci` — PASS, including format, check, clippy, workspace tests,
  docs/manifests, Adapter smoke, repository validation and desktop build.

## Exact artifact

- Android debug APK, 2,720,686 bytes:
  `223063EE43B3B956F09BCDF30A795576DA2FB3ABC134E1F8116950414B0E334F`.

The C24 Windows receiver, orchestrator and COM DLL are byte-for-byte unchanged.
The prior authorized Windows deployment was fully rolled back before this APK
was built; the clean-package preflight passed afterward.

## Authorized physical evidence

- The exact C25 APK above was installed on the authorized V2419A at
  `100.66.157.119:46143`; the installed `base.apk` SHA-256 matched the approved
  hash. No camera `adb reverse tcp:38173` mapping was created.
- Android rotation history recorded `ROTATION_0 -> ROTATION_90` at
  `03:25:37.141` and `ROTATION_90 -> ROTATION_0` at `03:26:17.745`. The same
  `MainActivity` record (`57893458`, task `4000`) remained in place instead of
  being recreated.
- The receiver observed the same CAVC stream
  `89f71f50b95af3e0ba79e69574e5b419` and epoch `13862327667183` when it
  reconnected after the rotation exercise. The configuration remained
  1280x720 at 30 fps, 4 Mbit/s, Annex-B with low-latency decode enabled.
- The elevated lab published a validated Global mapping and enumerated exactly
  one Session/CurrentUser `CapyIO Camera`. Ordinary Windows Camera displayed a
  real phone-camera preview. Two UI captures 1.2 seconds apart showed plainly
  different scene content, proving changing pixels rather than a frozen frame.
- Moving Camera Lab behind the unrelated Touchpad Lab produced a real pause.
  Android app-ops reported a completed CAMERA use duration, CameraService
  recorded the disconnect, and `Active Camera Clients` was empty. The Camera
  Lab process was then force-stopped as cleanup.
- The final reverse list still contained the unrelated
  `tcp:61000 -> tcp:61000` mapping and no `tcp:38173` mapping.
- On Windows, the receiver and virtual-camera processes exited and port 38173
  had no listener. The authorized administrator removal returned exit code 0;
  `C:\ProgramData\CapyIO` and fixed CLSID
  `{35754be3-54b6-4133-a1c7-1716395c6f1c}` were absent. The final
  `capyio-camera-live-lab-preflight.ps1` run reported artifact hash lock,
  deployment state, port and process cleanup all passing.

The first Windows Camera launch completed only after its 60-second temporary
camera hold had expired and correctly showed `NoCamerasAreAttached`; a second,
warm launch inside the next hold produced the live changing preview. The second
wrapper log stopped after `hold_seconds=60` without its normal textual cleanup
footer, so cleanup is claimed from the direct process/listener/deployment
checks and final preflight, not from a missing log line. The inspected Windows
Camera UI captures were not retained as repository artifacts.

## Remaining limits

This slice does not add background capture, persistent endpoint storage,
pairing, authentication, encryption or production installation.
