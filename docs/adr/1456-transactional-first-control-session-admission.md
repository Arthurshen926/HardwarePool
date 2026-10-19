# ADR 1456: Transactionally admit a first authenticated control Session

- Status: Accepted composition boundary; application owner pending
- Date: 2026-09-18

## Context

ADR 1455 can establish a shared Session/Node binding inside paired mTLS, but
`CatalogControlLink` previously required the Runtime Session and peer to exist
before it could own the channel. Creating those objects piecemeal could leave a
peer without its Session or retain state after a binding mismatch.

## Decision

`capyio-control-link` adds symmetric dialed and accepted admission methods for
the first authenticated connection to a peer. Each derives local and remote
Node IDs only from the TLS binding, verifies the Runtime's local identity and
rejects an already-present peer. It clones Runtime, registers a minimal online
peer with the paired display name, opens the exact negotiated Session, validates
the resulting link, then commits the clone only after every step succeeds.

The provisional peer is explicitly `pending_authenticated_catalog`, with no
Adapters or Capabilities. Platform/runtime version fields are also pending; the
first authenticated complete catalog replaces this placeholder through the
existing exact-Session catalog admission. Caller-provided data cannot preseed
hardware resources.

Existing peers fail closed because the current Runtime online transition would
resume every suspended Session for that Node. Reconnect needs a separate policy
that closes/reconciles the prior connection and therefore is not silently
implemented by first-admission code.

`NodeRuntime::close_session` supplies the matching idempotent transaction. It
moves the exact Session through Closing to Closed, invalidates live dependent
Routes and their epochs, releases their leases, preserves already Stopped or
Failed Routes, and marks the peer offline only when no other live Session exists.
`CatalogControlLink::close_and_reconcile` drops its socket before invoking that
exact bound cleanup, so a dead transport is never retained because cleanup
fails.

## Consequences

- A fresh paired mTLS channel can now become one Ready Runtime Session without
  partial mutation on identity, descriptor or duplicate failure.
- Dialer/listener direction still does not create global client/server roles.
- Application ownership, reconnect reconciliation and long-lived catalog loops
  remain pending; first-connection teardown now has an explicit boundary.
