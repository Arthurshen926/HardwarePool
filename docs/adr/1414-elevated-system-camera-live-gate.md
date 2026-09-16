# ADR 1414: Elevated system-camera live gate

Status: accepted

Date: 2026-09-16

## Decision

The current Windows camera ingress retains its fixed `Global\` mapping because
the Media Foundation camera source can be activated outside the desktop
process. Creating that page-file-backed mapping requires a token with the
Windows global-object privilege on the approved lab host. The unified desktop
action therefore reports
`CAPY.WINDOWS.SYSTEM_CAMERA_ELEVATION_REQUIRED` when its fixed receiver exits
with `CreateFileMappingW` error 5. The current prototype is tested from an
elevated user session; silently falling back to a session-local mapping would
break cross-process/system-camera semantics and is rejected.

Add an ignored, explicitly physical desktop test that starts the composed
system-camera action, waits at most 5–60 seconds for an actual decoded frame in
the production ingress mapping, launches a separate fixed camera-consumer
probe, then stops both owned processes and removes the exact ADB reverse. The
probe accepts no identity or path input: it requires exactly one currently
enumerated camera with the fixed CapyIO name (including the Windows-added
parenthesized suffix) and virtual-camera symbolic-link shape. It activates the
camera through Media Foundation and reads bounded samples.

This is a local integration gate, not a production privilege architecture.
A background broker/service with least privilege remains preferable to making
the eventual desktop UI generally elevated. The test records only bounded
frame metadata and a checksum; it does not retain pixels or video.

## Evidence

On the approved `DESKTOP-AT8EVE9` / unified Android 0.5.6 lab, the elevated
gate received decoded phone frame sequence 3, timestamp 66,666,000 ns,
1,382,400 NV12 bytes and checksum 112,335,401. The separate Media Foundation
consumer passed while the composed action was active. Stop left no Android
camera client, no `tcp:38173` reverse mapping and no current-user virtual-camera
session registration. A non-elevated diagnostic run failed at the expected
Global mapping creation boundary with Windows error 5.
