# ADR 1467: Live publication-catalog synchronization

- Status: Accepted for the current single-link topology
- Date: 2026-09-18

## Context

Initial mutual catalog exchange worked, but later Local sharing changes updated
only the local Runtime. Remote resources therefore became stale until a full
disconnect and reconnect. The heartbeat loop also assumed that its next inbound
frame must be the acknowledgement, preventing safe catalog interleaving.

## Decision

`CatalogControlLink` exposes a bounded typed dispatcher for CatalogSnapshot and
ControlHeartbeat. It validates the exact Session and catalog peer identity
before returning either type. Exact-step helpers remain fail-closed when the
other valid type is received.

Windows shares the latest local selection with each active controller through
a bounded coalescing signal. The worker prepares the complete filtered catalog
under Runtime observation, releases locks before sending, and admits peer
catalog updates while waiting for its exact heartbeat acknowledgement.

Android associates the retained link with its native Runtime. A valid local
catalog replacement marks publication dirty without doing socket I/O on the UI
or Binder caller. After acknowledging the next heartbeat, the Service worker
sends the latest Source-filtered complete catalog. It also applies Windows
catalog updates through the existing Runtime admission boundary.

## Consequences

- Local sharing changes converge over the already authenticated connection in
  both directions without reconnecting.
- Multiple rapid Windows changes coalesce to the latest complete snapshot.
- Catalog synchronization still conveys inventory only; Route creation, Port
  leases, authorization and hardware activation remain separate controls.
- Current Android ownership remains one retained peer, and physical timing/UI
  acceptance remains an external gate.
