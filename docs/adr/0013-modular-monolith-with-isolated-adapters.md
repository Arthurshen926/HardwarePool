# ADR 0013: Modular-monolith Runtime with isolated Adapters

- Status: Accepted
- Date: 2026-08-23

## Context

Full microservices add deployment/version overhead; one giant process makes
large third-party and native components share failure and language constraints.

## Decision

Run one logical Node Runtime for catalog/Route/Session state and isolate
substantial desktop integrations as Sidecars. Mobile hosts may use in-process
Adapters under platform lifecycle rules.

## Alternatives

All microservices; one process for everything; a UI-only launcher.

## Consequences

Users get one node identity and state model, while process supervision,
packaging and diagnostics become explicit engineering work.

