# ADR 1408: Reuse bounded input lab transports in the unified Android app

Status: accepted

Date: 2026-09-16

## Context

The standalone Android Controller Lab and Touchpad Lab already proved physical
phone-to-Windows input paths, but the unified Android Node exposed only local
control state. Keeping those senders in separate APKs prevents a single-package
acceptance run and gives their lifecycles different owners.

The existing transports remain controlled-lab contracts. The controller UDP
token filters unrelated trusted-network traffic but is not production pairing,
and the touchpad private records depend on the ADB reverse security boundary in
ADR 1106.

## Decision

The unified `AudioNodeService` owns both input senders.

- Touchpad start opens only `127.0.0.1:61000`, through the operator-created ADB
  reverse mapping. The APK reuses the existing ARM64 record JNI entrypoints,
  exact Route/Session identities, Hello/Data/Ack/Close framing and post-Sink ACK
  sequence validation. A bounded eight-frame queue coalesces same-contact
  motion. If transition pressure fills it, it sends an explicit empty reset
  followed by the latest complete snapshot rather than dropping a newly added
  finger; a final release always wins.
- Complete shared-UI contact frames are deterministically converted back into
  Android down/move/up/pointer/cancel transitions. A frame that adds several
  fingers is expanded into ordered down/pointer-down records. Contact
  replacement or multi-contact release first cancels the old gesture before
  rebuilding a non-empty latest snapshot.
- Gamepad and standalone IMU share the existing bounded Controller Lab UDP JSON
  contract. The Android UI accepts the Windows IPv4, port and freshly displayed
  hexadecimal token before either capability starts. One 8 ms worker publishes
  complete controls and the latest independently captured accelerometer and
  gyroscope values. Either Source may own the worker; it stops only after both
  Sources are inactive and sends three final neutral control snapshots.
- These senders are data-plane workers. They do not serialize the catalog and
  do not execute inside sensor callbacks.

The integration does not install an APK, create an ADB mapping, launch the
Windows receiver, install a driver or claim production authentication.

## Consequences

- The same Android APK now contains the previously separate touchpad,
  touch-gamepad and IMU sending paths.
- Touchpad testing still requires the explicit ADB reverse mapping and desktop
  acknowledgement flags from ADR 1106.
- Controller testing still requires starting the Windows listener first and
  copying its per-run token into Android.
- Automatic peer discovery, authenticated catalog exchange, reconnect policy
  and production background execution remain open integration work.
