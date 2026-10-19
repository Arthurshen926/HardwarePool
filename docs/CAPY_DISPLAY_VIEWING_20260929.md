# Display viewing controls, 2026-09-29

The user attributed the remote pointer restriction to their Moonlight mouse
mode. No cursor clipping, display rearrangement, or remote application setting
was changed in this slice.

## Implemented

The shared DisplayReceiverPanel exposes fullscreen, keep-screen-on, and
presented-frame statistics switches. Android persists these private presentation
preferences and reports them through the existing display-state DTO. The
CapyIOApi boundary applies them before opening the receiver. Defaults are
fullscreen/keep-awake enabled and statistics disabled. Startup/error status
remains visible; GameStream successful-frame status can be hidden. A visible
Exit reception button closes the receiving Activity. UI explicitly explains
that this does not stop the Windows display service or restore its topology.
Aspect ratio remains preserved. No permissions or service declarations changed.

These are receiver-local presentation choices, not synchronized stream settings.
Existing APKs reject the new options with an update message rather than silently
ignoring the controls. Windows sender controls retain their existing semantics.

## Evidence

- Shared Vue typecheck/build passed.
- Three existing display-control tests passed (not physical settings coverage).
- Full Android debug build and native GameStream packaging checks passed after
  retrying outside the sandbox: the first attempt could not read SDK
  `core-lambda-stubs.jar` (AccessDeniedException).
- APK SHA256: A56993143152C04BBEE44FDFF7FE160DD0B449FC032B2DE13784AC730DAD4825.
- No APK installation or physical verification of these new options in this slice.

## Remaining integration

Resolution, refresh rate, bitrate and codec are currently part of the provisioned
two-ended stream contract. A UI-only change must not claim to renegotiate them.
Next implement synchronized stopped-session configuration, then peer-scoped
display start/stop using existing trust and authorization, with receiver readiness
distinct from service readiness. Do not silently reuse another peer's connection
bundle. Mirror mode, HDR and input redirection are not provided by these switches.

Reference for selecting common stream settings:
https://github.com/moonlight-stream/moonlight-docs/wiki/Setup-Guide

## Follow-up build and resource packaging

The metrics overlay now shows provisioned resolution, sampled presented FPS and
cumulative frames (not network latency or encoder FPS). Build revision
`display-viewing-metrics-20260929` passed shared UI and complete Android/native
packaging. APK SHA256:
946BE1FBAE619EDFBBDB02966854C28F5353B39480F5464A4E67D295A27328A0.

The local-debug package manifest now accepts explicitly hashed shaders under
assets/shaders/directx, includes under include, assets/apps.json and the WGC
helper at tools/sunshine_wgc_capture.exe. Required resource checks reject the old
EXE/DLL-only package shape. Upgrade and rollback retain those nested paths;
the filesystem fixture verifies their installation and recoverable rollback.
The manifest producer must still enumerate the complete matching runtime, not
merely the minimum sentinel files. No actual Windows package deployment was
performed by the fixture. This does not change the independent display pairing
or implement cross-node start/stop.
