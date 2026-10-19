# ADR 1483: Android outbound paired control Service bridge

- Status: Accepted as an integrated-alpha manual diagnostic path
- Date: 2026-09-19

## Context

ADR 1482 proved an Android Rust host can dial a pinned Windows control TLS
endpoint, but the installed Android Service and shared UI could not request it.
The Windows inbound side in ADR 1481 is still a one-shot diagnostic, not a
persistent product listener.

## Decision

The Android Node Service exposes one bounded, explicit dial request through
its existing WebView bridge. The request names a numeric IP and port as a
routing hint; the Service retrieves the sole active peer credential from its
Android Keystore-backed store and queues the request on the existing control
worker. JNI repeats endpoint and directory validation, calls the pinned
outbound TLS/catalog exchange, and owns the live link in the same listener
registry as accepted sessions. The worker services inbound messages and
outbound heartbeat acknowledgements, reconciles on failure, and reports the
bounded error to the UI. The UI distinguishes queued work from authenticated
connection. No credential bytes or certificate fingerprint cross to the
WebView, and no Route or hardware starts on dial.

Only one active peer and one control link are supported on Android. A pending
dial cannot overwrite an active link. Existing pairing credentials are not
rotated automatically. The Windows one-shot listener will close after one
exchange, so this path is not a persistent peer session yet.

## Verification and follow-up

The JNI host tests, Java connection-cycle contract, shared UI typecheck, and
ARM64 debug APK build cover this slice. Physical reverse dialing remains
unproved until a new dual-role pair is installed on both sides and a Windows
inbound listener is available. Next: persistent Windows listener in the
desktop node lifecycle, deterministic simultaneous-dial arbitration,
connection-state UI, and first-pair/rotation flow.
