# ADR 0011: Capabilities own typed Ports

- Status: Accepted
- Date: 2026-08-23

## Context

Capability-level local/stream roles mixed physical meaning, flow direction and
projection mechanism, and could not express a gamepad with feedback.

## Decision

Capability is user-facing ownership/metadata. One or more Ports carry direction,
Profile, format/QoS, clock and interoperability semantics.

## Alternatives

One Capability per direction; keep `LocalRole`/`StreamRole`; make Port a UI-only
concept.

## Consequences

Compound Capabilities become natural, while validators and protocol need typed
Port IDs and explicit Profile compatibility.

