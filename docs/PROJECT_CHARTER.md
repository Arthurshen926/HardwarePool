# CapyIO Project Charter

## Mission

Reuse hardware people already own by connecting cross-device I/O capabilities
through a common identity, catalog, Route lifecycle and user experience.

## Product promise

CapyIO makes multiple I/O classes independently discoverable and composable.
Where possible it presents them as system devices; where not, it supplies an
honest API, Panel, standard-protocol or recording fallback.

## Current proof strategy

The foundation first proves symmetric Nodes, typed Ports, independent Routes,
Mock Quick Actions/Workspace and isolated Adapter lifecycle. It deliberately
does not prove physical I/O or performance.

## Engineering posture

- modular monolith per Node, selective process isolation;
- small pure-Rust Core and replaceable mechanisms;
- reuse complete third-party vertical slices behind recorded boundaries;
- one independently verifiable Gate at a time;
- evidence-backed claims and explicit platform limitations;
- safe-by-default development commands.

## Governance

The owner accepts product scope and architecture decisions and authorizes
physical/privileged operations. Contributors and Agents implement scoped plans,
tests and evidence. Accepted ADRs and stable Requirement IDs are the durable
decision trail.

## License baseline

Foundation code is Apache-2.0. Third-party source is not imported until its
license, revision, integration mode and modifications are recorded. GPL code is
not included in the foundation migration.

