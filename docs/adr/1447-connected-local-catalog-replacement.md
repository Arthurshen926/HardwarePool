# ADR 1447: Local inventory updates preserve connected Runtime state

- Status: Accepted
- Date: 2026-09-18

## Context

The Android Service updated its native publication snapshot through a
disconnected-only replacement that reconstructed NodeRuntime. Once a peer,
Session or Route existed, every hardware/sharing update would fail. Composing
the authenticated catalog link without changing this lifecycle would leave
native local inventory stale after connection.

## Decision

NodeRuntime gains a transactional `replace_local_catalog` command. It validates
the complete descriptor and unchanged local Node ID, preserves Runtime-owned
online state and reconciles all dependent Routes using the same endpoint
checks as complete peer-catalog admission. Invalid replacement leaves all
state unchanged. Compatible metadata updates preserve Route state and epochs;
removed/incompatible endpoints offline only affected Routes and release their
leases. Restoring an endpoint does not automatically recover or start a Route.

Peers, Sessions, unrelated Routes, operations and event history are retained.
Successful changed replacement emits LocalCatalogReplaced; an identical update
is idempotent and emits no event. No socket, Android framework or platform
dependency enters Runtime.

AndroidNativeCatalogRuntime.replaceCatalog and the existing nativeReplace JNI
entrypoint now use this command. AudioNodeService publishes inventory through
that live path. The existing Rust disconnected-only method remains as a strict
compatibility API; it still rejects a connected Runtime and no longer resets
disconnected event history. No permissions, foreground types or wire layout
change, and no peer is enrolled or granted by updating local inventory.

## Acceptance and limits

Tests prove preservation of connected peer/Session state, independent Route
lifecycle, affected lease release, epoch invalidation, no automatic recovery,
metadata idempotence and transactional rejection of identity/ownership errors.
The Android native wrapper test retains a synthetic peer and Session while
updating the exact shared Java fixture. Android build validates the Service/JNI
entrypoint; it is not evidence of physical connected operation.

The current Service still prepares a publication-projected catalog. A future
platform connection owner must serialize selection changes with outbound
publication and handle actual Adapter stop/completion separately from Runtime
state. This slice does not start an authenticated app connection, implement
first-run pairing or claim ordinary-user delivery.
