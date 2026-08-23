# ADR 0016: EasyTier is an external overlay first

- Status: Accepted
- Date: 2026-08-23

## Context

CapyIO may benefit from virtual IP connectivity, but overlay routing is not I/O
domain logic and embedding it would expand Core/network security scope.

## Decision

Initial Adapters use ordinary IP transports and may run over an independently
managed EasyTier virtual address. Future integration may probe/manage an
external process behind a Connection Adapter.

## Alternatives

Embed EasyTier into Core now; build a new Mesh; forbid overlays.

## Consequences

LAN/manual-IP development remains simple. Overlay lifecycle/security is deferred
without blocking later integration.

