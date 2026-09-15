# ADR 1403: Expose bounded camera source selection in the shared UI

Status: accepted

Date: 2026-09-16

## Context

The unified camera lifecycle initially fixed the back camera and balanced AVC
quality. The retained lab could enumerate directly openable Camera2 IDs and
vendor Zoom targets, but users could not choose them from the common Android/
Windows presentation.

## Decision

Reuse the existing bounded read-only `CameraInventoryCollector` and
`CameraSourceSelection` contract behind `UnifiedCameraNodeFacade`. Inventory is
refreshed only by a visible user action after CAMERA permission. Selection is
session-local, exact-key allow-listed, and accepted only while capture is
stopped. Quality accepts only `economy`, `balanced` or `clear`. Neither command
accepts an endpoint, Route, path or arbitrary camera parameters.

The shared Vue code adds an Android Camera2 settings panel through a versioned
JSON bridge. Tauri and Browser Mock return no Android camera settings, preserving
one presentation source without inventing platform support. Start copies the
selected source and quality into the existing foreground-service configuration.

## Consequences

- Users can inspect and select concrete openable Camera2 IDs and bounded vendor
  Zoom targets from the unified UI.
- Settings cannot mutate a running encoder/camera session; Stop is required.
- Inventory does not claim physical-lens identity beyond the reviewed vendor
  Zoom contract.
- Endpoint selection, authenticated peer exchange and Route creation remain
  separate work.

