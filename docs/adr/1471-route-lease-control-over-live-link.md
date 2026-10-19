# ADR 1471: Route and lease control over the live authenticated link

The publication-only Android policy below is superseded for local Touchpad
Sources by ADR 1472's explicit owner confirmation. Other alpha policies remain.

## Context

The integrated UI could discover, authenticate and synchronize catalogs, but a
remote Capability detail had no executable action. Route and Port-lease wire
messages already existed, while the long-lived platform link admitted only
catalogs and heartbeats. Treating an online catalog entry as permission or as
globally idle would violate the Runtime and security model.

## Decision

The authenticated `CatalogControlLink` now carries typed `RouteStatus`,
`RouteCommand`, `PortLeaseRequest` and `PortLeaseResponse` messages in addition
to catalogs and heartbeats. Every message must match the TLS-bound Session.

The Windows UI action **申请并使用** performs this bounded sequence:

1. choose a compatible remote Source and local Sink and create a pending Route;
2. send the pending Route as a proposal;
3. let the owner mirror and authorize it under its platform publication policy;
4. acquire a five-minute owner-authoritative exclusive lease for the remote
   Port, with conflict and denial returned explicitly;
5. send Start and require an Active Route status before the local mirror becomes
   active. Stop releases the lease before sending Stop.

The Android alpha policy authorizes only a Route that resolves against the
currently publication-filtered native catalog. Start additionally requires a
non-expired lease for its owner-local Port. These checks do not grant access to
an unpublished Capability and do not weaken certificate/Node pinning.

## Consequences

- The product UI exposes a real Route/authorization/lease control action and
  displays Runtime state instead of inferring occupancy.
- A second Route targeting an exclusive Port receives `Conflict` from the owner.
- This composes control state only. Existing feature Adapters still own their
  media/data transports and must separately bind endpoints to the accepted Route.
- Lease renewal is not automatic. The alpha uses the maximum five-minute TTL.
- Android ADR 1461 still limits one active paired control peer.

## Verification

`cargo test -p capyio-protocol -p capyio-control-link -p capyio-desktop`
includes a real mutual-TLS Route proposal, authorization, owner lease and Start
round trip, plus exact message/session correlation and existing regressions.
