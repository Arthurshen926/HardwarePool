# ADR 1445: Authenticated catalog link composition

- Status: Accepted foundation; application integration pending
- Date: 2026-09-18

## Context

The TLS transport and admitted catalog host were separate APIs. Platform code
could accidentally select a different Session or peer for their composition,
or hold the shared Runtime lock while waiting for network I/O.

## Decision

`capyio-control-link` composes only already-authenticated TLS channels with
`PeerCatalogControlHost`. Local and remote Node identities derive from the TLS
binding, never discovery or UI input. Dialed/accepted describe a connection,
not a global Node role. This initial link accepts CatalogSnapshot only.

The operations are deliberately split:

1. Prepare publication under the Runtime lock: require the exact Ready Session
   and local/remote identities, then project selected Source capabilities.
2. Release the lock and send/receive bounded TLS records on the host worker.
3. Reacquire Runtime ownership to apply the received catalog. Recheck readiness
   and identity; atomically admit through the existing host boundary.

Opaque prepared/received records also bind a fresh process-local link instance
ID, preventing queued work from being transferred to a replacement connection.
The dialer sends first and the acceptor receives first to avoid symmetric large
write deadlock. The host must serialize publication changes and discard queued
outbound snapshots when selection, Session authority or connection ownership
changes. This crate has no unbounded queue or autonomous thread.

Malformed or identity-mismatched inbound catalogs, transport errors and failed
Runtime admission close the link. A received catalog never registers trust,
opens a Session, authorizes/starts a Route or captures hardware. Runtime Session
creation and peer registration must already have been performed by the trusted
platform owner after authentication. The host remains responsible for marking
peers offline and reconciling dependent Routes after connection loss.

## Dependency note

The new composition crate depends only on existing CapyIO Core, Protocol,
Runtime, control-host and control-tls crates plus workspace-pinned thiserror.
No new third-party production package is added. Ephemeral test certificates use
the existing test-only rcgen 0.14.5 (MIT OR Apache-2.0). Testkit is test-only.
Keeping composition separate avoids making Runtime depend on network/crypto or
making the TLS framing boundary depend on Runtime. Copying this logic into both
platform hosts would duplicate identity checks and publication policy.

## Evidence and remaining work

Real loopback mTLS tests use synthetic Windows/Android catalogs and independent
Runtimes. They prove bidirectional filtered catalog admission, deselection
updates, offline-before-apply rejection, wrong Runtime/selection rejection,
cross-link rejection and explicit close. No Route or lease is created.

This does not claim Android socket/JNI ownership, desktop service wiring,
first-run enrollment, credential storage, fresh Session negotiation, automatic
reconnect or physical resource delivery. UI readiness must remain not connected
until platform owners actually run this composition with real catalogs.
