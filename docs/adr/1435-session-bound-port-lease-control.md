# ADR 1435: Session-bound Port lease control

Status: Accepted

## Context

ADR 1434 established owner-authoritative Port lease policy and Runtime claims,
but there was no bounded Node-to-Node command. A UI or platform component must
not gain authority by directly editing a remote occupancy projection, and a
lease request must not implicitly create, authorize or start a Route.

## Decision

Protocol v1.1 appends `PortLeaseRequest` and `PortLeaseResponse`. Requests bind
one existing Route ID and PortRef to acquire, renew or release. Acquire and
renew use a relative TTL of 1 second through 5 minutes; release uses zero. The
owner computes absolute expiry from owner time. Responses correlate the request
Message ID and return only acquired, renewed, released, conflict or denied.

`capyio-control-host` handles the command only after a caller constructs an
exact admitted peer binding. It requires that the Runtime Session is Ready and
has the same remote Node, that the Route belongs to that Session and that the
Port is local. Runtime authorization, owner-selected lease policy, capacity and
authorization expiry remain authoritative. Renew requires an unexpired claim;
release requires the exact Route/Port/Session claim. Valid denials are retained
in the same bounded 256-message duplicate set as admitted catalogs.

## Consequences

The resource owner can now make deterministic occupancy decisions once a
production transport authenticates a peer. Requests cannot change policy,
authorize a Route, start hardware, or control a remote-owned Port. The host
still opens no socket, performs no pairing or cryptography and provides only
bounded accidental duplicate suppression; transport authentication, framing,
deadlines and cryptographic replay defense remain required integration work.
