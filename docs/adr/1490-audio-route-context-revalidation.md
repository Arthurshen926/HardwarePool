# ADR 1490: Audio media binding revalidation against live Route context

Status: accepted for the integrated audio groundwork only

Date: 2026-09-26

## Decision

Add a transport-independent `AudioMediaStreamBinding::validate_route_context`
check. A trusted runtime owner supplies its current Route and Session snapshot.
The check requires matching nonzero binding identities, a Ready Session, its
two endpoint nodes, audio frames Profile v1, Starting/Active Route state,
finite unexpired authorization and an exact representable Route epoch. It is
direction-neutral and rejects stale bindings after stop, expiry or reconnect.
This is a control/worker-thread operation, not an audio callback operation.

This helper is not an authorization grant or complete admission decision.
Catalog direction, negotiated specification, local Port leases, authenticated
transport and repeated live checks remain the runtime owner's responsibility.
The helper does not start hardware, acquire leases, mutate a Route, or change
the native UDP backend's explicitly insecure lab contract.

## Integration boundary

Existing native audio ConfigLoader/lab bindings still use fixed Session, Route
and Stream IDs. This helper is not yet wired to those deployed workers. Follow-up
work must negotiate a bounded audio offer, feed identical dynamic bindings to
both workers, bind endpoint selection to the authenticated peer, and enforce
lease/Stop/disconnection cleanup before exposing automatic use in the UI.
Received/played data, not a logical Route transition, must establish readiness.
No new dependency or public network field is introduced in this slice.

## Verification

`cargo test --offline -p capyio-audio` covers both endpoint directions, exact
expiry, all non-running states, pending/denied/revoked/unbounded grants, wrong
Route/Session/Profile/node, closed Session, and zero/changed/overflowed epoch.
These tests do not prove audio transport authentication or product integration.
