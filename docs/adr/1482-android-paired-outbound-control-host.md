# ADR 1482: Android paired outbound control host boundary

- Status: Accepted as a host-level capability, not yet Android UI/Service wiring
- Date: 2026-09-19

## Context

ADR 1481 provides a bounded Windows accepted-control diagnostic and ADR 1480
issues dual-role certificates for new pairs. Android's Rust host still only
accepts control connections. A reverse path must prove identity, Session and
catalog behavior before it is owned by the Android Service or exposed in UI.

## Decision

`AndroidCatalogRuntimeV1` can dial one explicitly configured active paired
peer at a typed endpoint. It decodes the Rust-owned pairing directory and
credential bundle, rejects zero or multiple active peers, pins the exact
Windows leaf, binds the ordered server/client Node IDs to a fresh Session, and
only then admits the peer into the Runtime. As dialer it sends a publication-
filtered Android catalog, receives and validates the Windows catalog, then
returns the live `CatalogControlLink` to its caller. Failed catalog exchange
closes and reconciles the Session. The caller can explicitly close and
reconcile a successful link. No Route or hardware operation is implicit.

This is not a new credential format, permission or Android JavaScript command.
Endpoint selection remains a routing hint and cannot change peer trust.

## Verification and remaining work

Loopback tests cover Android dialing a pinned Windows TLS server, bidirectional
catalog exchange, fresh Session with no Route, explicit close, rejection of an
unpaired directory or invalid credential before network I/O, and rejection of
the wrong server leaf before Session admission. Existing Android accepted-path
tests remain required. JNI/Service ownership, Android UI initiation, Windows
persistent listening, old-pair rotation and physical Android-to-Windows
connection are separate slices.
