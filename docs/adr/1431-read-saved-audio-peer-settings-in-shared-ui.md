# ADR 1431: Read saved audio peer settings in the shared UI

Status: Accepted

## Context

The Android audio configuration form could write per-direction settings but
could not distinguish saved settings from example defaults when reopened.

## Decision

Expose a read-only Android bridge method through CapyIOApi. Its version-1 DTO
contains only the requested microphone/speaker direction and a nullable saved
peer (private/overlay IPv4 and two UDP ports). Read the existing native store
under the same service lock used by configuration. Do not expose credentials,
arbitrary preferences, effective build defaults or new lifecycle operations.

The shared client validates exact keys, direction, schema and bounded values.
Missing legacy support, read errors and absent saved settings remain distinct.
Opening the form reads settings; saving is followed by readback comparison.
Direction changes invalidate pending reads. Neither operation starts hardware.

## Consequences and verification

Android remains authoritative; the browser mock and Windows return unsupported.
This adds no dependency, permission or foreground-service declaration. Native
golden contract tests and frontend malformed-payload tests cover the DTO.
Persistence is not proof of reachability, pairing, encryption or discovery.
