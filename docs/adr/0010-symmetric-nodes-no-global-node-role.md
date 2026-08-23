# ADR 0010: Symmetric Nodes have no global role

- Status: Accepted
- Date: 2026-08-23

## Context

Provider/consumer Node roles fail when one device sends one capability while
receiving another.

## Decision

Remove `NodeRole` from Core and protocol. Both peers exchange catalogs and may
own Source, Sink and Control Ports simultaneously.

## Alternatives

Provider/consumer/duplex enum; infer a Node role from its current catalog.

## Consequences

Authorization and UI direction are evaluated per Port/Route. Code cannot branch
on a global provider/client identity.

