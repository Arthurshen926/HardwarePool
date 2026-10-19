# ADR 1457: Windows Desktop dials only a fresh paired discovery candidate

- Status: Accepted outbound host path; peer listener and enrollment pending
- Date: 2026-09-18

## Context

Windows Desktop could observe candidates, load its paired directory and report
storage readiness, but the shared Remote page had no real connection action.
Operators still had separate per-feature address forms, and the common control
plane never consumed the platform credential store.

## Decision

The Remote candidate card offers `认证连接` only for a locally active paired
record. The WebView submits exactly one canonical Node ID. The Tauri host then
expires discovery state, finds the source-derived endpoint, reclassifies it
against the paired directory, obtains the paired display label/server name/
leaf fingerprint/opaque handle, and loads the typed credential from the retained
Windows DPAPI store. The UI cannot submit an address, certificate, credential
handle, key, Session ID or catalog.

The host creates the mTLS client identity, establishes an ADR 1455 fresh binding,
transactionally admits it through ADR 1456, sends the publication-filtered local
catalog and receives/applies the peer's exact bound catalog. Only after all of
those steps succeed is the link retained and the shared snapshot changed to
connected/active. Every intermediate failure closes and reconciles the Session.
Disconnect removes the retained link and invokes the exact Runtime cleanup.
Application shutdown performs the same bounded cleanup.

Connection and initial exchange use a fixed five-second socket deadline and one
host mutex to serialize this first slice. There is no retry loop, inbound Windows
listener, Android control listener, enrollment or background reconnection yet.

## Consequences

- For a pre-enrolled compatible peer, one discovered Node action replaces
  manual IP/port entry for the common control connection and catalog exchange.
- Per-feature lab transports are not silently redirected; they remain separate
  until their Adapters consume an authorized common Route.
- The button will fail honestly against the current Android application because
  it does not yet advertise or accept this control channel.
- Multi-peer concurrency beyond serialized establishment, reconnect, listener
  ownership and enrollment UX remain follow-on work.
