# ADR 0020: Adapter Manifest v2 uses deployment-mode bindings

- Status: Accepted
- Date: 2026-08-24

## Context

Manifest v1 stored one top-level `entrypoints` map and required every Adapter to
declare `Sidecar`. That represented the Gate 3 Mock binaries, but contradicted
the normative `InProcess`, `ExternalService` and `DriverBacked` deployment
modes. It could not describe a mobile in-process module, an independently
managed service, or the required user-mode boundary for a driver dependency.

The repository is pre-alpha and has no released manifest consumer, so preserving
the misleading v1 shape would create more compatibility cost than replacing it.

## Decision

Adopt the intentionally breaking Adapter Manifest v2 contract. Replace the
top-level `entrypoints` map with a closed `mode_bindings` object whose optional
sections correspond exactly to the modes listed in `deployment_modes`:

- `in_process` maps platforms to module and/or library identifiers;
- `sidecar` maps platforms to opaque executable paths;
- `external_service` maps platforms to typed probe and connection descriptors;
- `driver_backed` maps platforms to a user-mode controller and descriptive
  driver dependency metadata.

The top-level `platforms` set is the union of platforms supported by all mode
bindings. Each platform is covered by at least one binding, and bindings cannot
reference undeclared platforms. All fixed-shape JSON objects deny unknown
fields.

Entrypoints are direct process paths, not shell command lines. Driver metadata
contains no install, update, signing or privilege-escalation command. Driver
deployment remains outside this manifest and requires a separately authorized,
platform-specific workflow.

Rust validation is authoritative for cross-field semantics. The committed JSON
Schema remains the structural distribution contract and is protected by parity
tests rather than a new production JSON Schema dependency.

## Alternatives

Keep requiring `Sidecar`; make every binding a free-form JSON object; retain one
global entrypoint map; add a runtime JSON Schema validator dependency.

## Consequences

Android and iOS Adapters can declare in-process bindings, existing services can
be probed without pretending they are bundled executables, and future driver
integrations must keep privileged/kernel work behind a named user-mode
controller boundary. Manifest v1 files now fail explicitly and must be migrated
to v2. Schema, Rust types, Mock manifests and contract tests must move together.
