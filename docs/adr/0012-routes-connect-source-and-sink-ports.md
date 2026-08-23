# ADR 0012: Routes connect Source to Sink Ports

- Status: Accepted
- Date: 2026-08-23

## Context

Binding/Projection lifecycle was tailored to consuming a remote capability and
did not model symmetric or local pipelines cleanly.

## Decision

A Route connects one compatible Source Port to one Sink Port and owns backend,
format/QoS, authorization, state, epoch and diagnostics. Opposite directions use
separate Routes.

## Alternatives

Magic duplex stream; generic graph edges with no direction; retain per-remote
Capability Bindings.

## Consequences

Independence and compatibility are directly testable. Pre-alpha Binding APIs
are replaced rather than maintained as a second active model.

