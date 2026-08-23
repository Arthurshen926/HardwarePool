# ADR 0019: Reuse third-party vertical slices behind Adapter boundaries

- Status: Accepted
- Date: 2026-08-23

## Context

Several hardware classes have mature vertical projects. Rewriting them is costly;
copying scattered functions loses lifecycle/threading assumptions and provenance.

## Decision

Prefer external Sidecar, then a recorded vendored vertical slice, then a thin
native Adapter. Preserve upstream revision/license/notices and record imported
paths/modifications before code enters the repository.

## Alternatives

Rewrite every stack; copy selected functions into Core; ship independent tools
without common lifecycle.

## Consequences

Packaging and license review are first-class. Gates 0–3 add only placeholders
and tracking; no third-party vertical source is imported.

