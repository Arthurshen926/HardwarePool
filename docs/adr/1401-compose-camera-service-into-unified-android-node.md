# ADR 1401: Compose the camera service into the unified Android Node

Status: accepted

Date: 2026-09-16

## Context

The standalone Android camera lab already owns a bounded Camera2/MediaCodec AVC
pipeline, exporter, foreground-service lifecycle and stop notification. The
unified Android Node previously declared the camera as detected but unattached,
which forced users to switch applications and prevented the shared UI from
controlling the capability. Camera APIs used by the proven service require API
29, while the unified application previously declared minimum API 26.

The user explicitly authorized the CAMERA and camera foreground-service manifest
changes and installation on the designated V2419A development device. Display
integration remains excluded.

## Decision

The unified APK compiles the existing camera contract and service sources behind
`UnifiedCameraNodeFacade`. The camera stays a separate, unexported foreground
service and remains the sole owner of Camera2, MediaCodec and exporter resources;
the Activity and WebView only request permission and submit closed start/stop/
retry operations. The Node catalog now attaches one video Source Port using the
private CAVC access-unit format, and publication intent remains distinct from
capture lifecycle and Route creation.

The unified app declares optional camera hardware, `CAMERA` and
`FOREGROUND_SERVICE_CAMERA`, and raises its minimum SDK to 29. No runtime
dependency is added. The standalone lab remains buildable until the unified host
has equivalent source-selection, quality and end-to-end Route controls.

The shared Vue application is served from the fixed origin
`https://appassets.androidplatform.net/capyio-ui/` by an APK-asset-only
`WebViewClient`. It accepts only that exact origin and bounded subtree and returns
403 for every other request. File/content access stays disabled. This replaces
the `file:` origin, whose ES module and stylesheet requests are rejected by a
real WebView's CORS enforcement.

## Consequences

- One installed Android application and Node identity now own audio, IMU,
  gamepad, touchpad and camera lifecycles.
- Camera consent and privacy notification remain Android-owned and visible.
- The default camera endpoint remains the existing fixed ADB-reverse lab
  boundary; this decision does not add pairing, discovery, encryption or a
  product Route.
- Devices below Android 10 are no longer supported by the unified APK.
- Camera source/quality controls and retirement of the standalone lab require
  later parity evidence.

