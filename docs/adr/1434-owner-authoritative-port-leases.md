# ADR 1434: Owner-authoritative Port leases

Status: Accepted

## Context

Catalog availability and UI Route counts cannot determine whether a shared
hardware Port is free, especially when several remote Nodes may use different
resources on one provider. Route authorization also does not by itself define
exclusive versus bounded shared use.

## Decision

The Runtime that owns a local Port is the only lease authority for that Port.
It may configure an explicit exclusive policy or a shared policy with a
non-zero maximum Route count. A claim binds the exact local PortRef, Route ID,
ready Session ID and finite expiry. The Route must reference the Port and be
authorized; a lease cannot outlive finite Route authorization.

When a Port has a policy, `begin_route_start` fails unless that Route holds a
current claim. Same-Route/same-Session renewal is idempotent; another Session
cannot reuse the Route identity. Stop, Offline, catalog invalidation and peer
loss release only that Route's claims. Explicit expiry removes the claim,
invalidates the Route epoch by moving it Offline and emits bounded Runtime
events. Policies cannot change while claims exist.

The registry retains at most 512 policies and 1,024 claims. Snapshots expose
claims, not policy mutation authority, so the UI can show exact locally owned
leases without inferring remote/global occupancy.

## Consequences

Independent Ports on one Node can be leased independently, and a deliberately
shared Port can serve a bounded number of Routes. No control-wire lease request,
automatic policy selection or remote configuration grant is added yet. Hosts
must choose policy from Adapter/resource semantics and carry authenticated,
authorized requests to these Runtime methods.
