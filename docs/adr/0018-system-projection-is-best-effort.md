# ADR 0018: System Projection is best effort

- Status: Accepted
- Date: 2026-08-23

## Context

System virtual devices provide the most transparent UX but differ by OS and are
often unavailable to ordinary mobile applications.

## Decision

Prefer system Projection when supportable, then degrade through system
route/injection, standard API/protocol, and CapyIO Panel/Recorder. UI reports the
actual level.

## Alternatives

Require system devices for every capability; never build system projections.

## Consequences

Capabilities stay useful across restricted platforms without false claims.
Platform-specific projection work remains independently testable and risky
driver actions stay out of Core.

