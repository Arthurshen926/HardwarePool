# ADR 1476: Reuse authenticated endpoint hints without making them trust

- Status: Accepted for the integrated alpha
- Date: 2026-09-19

## Context

ADR 1468 reconnects active paired peers observed by multicast. ADR 1470
allows a manual numeric endpoint when multicast is unavailable, but the user
must re-enter an ephemeral CapyIO port after every desktop launch. This is
particularly awkward on overlay networks that do not carry the discovery
broadcast. Remembering an endpoint must not turn an address into a credential.

## Decision

After a successful native authenticated control connection, the desktop UI
may persist only the peer Node ID, canonical numeric host and control port in
a bounded local routing-hint cache. It does not persist certificate material,
grant state, an observed-but-unverified announcement or a failed endpoint.
Hints are removed when their peer is no longer actively paired. A corrupt
cache is ignored. Automatic attempts are bounded by the existing per-peer
cooldown and manual-disconnect suppression.

On launch, a fresh paired discovery candidate takes precedence. If discovery
is absent, the UI may try a cached endpoint. If a discovered endpoint fails
and a distinct cached endpoint exists for the same peer, the latter may be
tried. Every attempt still calls the ADR 1470 native dial, which selects the
credential and pinned certificate from the current active pairing directory,
performs exact mTLS Node binding and opens a new Session. A cache hit is never
evidence of trust, reachability, a resource grant or a live data Route.

The UI continues to allow explicit manual endpoints as a recovery fallback.
This ADR does not implement interactive enrollment, Android outbound dialing,
Windows inbound control listening, a relay or automatic port migration when
both discovery and the cached endpoint are stale.

## Consequences

- An already paired device can reconnect without repeated IP/port entry when
  its last authenticated control endpoint remains reachable.
- Network address changes still require discovery or a new manual connection.
- The cache is non-secret but identifies a peer address, so it remains local
  UI state and is not committed to repository fixtures.
