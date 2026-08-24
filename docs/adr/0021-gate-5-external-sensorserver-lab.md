# ADR 0021: Gate 5 starts with an external SensorServer protocol Adapter

- Status: Accepted
- Date: 2026-08-24

## Context

The v0.3 foundation PRD listed every real SensorServer and physical-device
integration as a current non-goal, while the accepted Roadmap and Backlog made
`CAPY-IMU-001B` the active Gate 5 slice. Proceeding without resolving that
conflict would make the repository claim two incompatible scopes.

SensorServer is an independently maintained Android WebSocket service licensed
GPL-3.0-only. Its documented sensor message contains an Android accuracy value,
an elapsed-realtime nanosecond timestamp and a sensor-specific values array.
CapyIO is Apache-2.0 and does not need to copy or link SensorServer code to
consume that documented external protocol.

## Decision

Advance the PRD to v0.4-pre-alpha and authorize one narrow Gate 5 exception:
an externally installed SensorServer instance may supply IMU readings in an
explicitly approved local laboratory. CapyIO implements its own bounded parser,
pairing policy and Adapter boundary. The reviewed upstream revision is
`5ae401780d99debcabb8dc259256c2652dada0a6`.

No SensorServer source or binary is imported, linked, modified, repackaged or
distributed. Plain WebSocket is treated as insecure local-lab transport only;
Tailscale connectivity does not turn the current CapyIO session/control model
into production authorization. A production path still needs CapyIO identity,
authenticated encryption, Route authorization, replay defense and downgrade
binding.

Implementation remains staged: parser/pairing contract, reviewed WebSocket
dependency and mock server, physical phone evidence, then Runtime/UI lifecycle.
Audio, drivers and other hardware classes remain outside this Gate slice.

## Alternatives

Build a new Android app immediately; vendor or fork the GPL application; keep
all real devices out of scope; start audio before validating a low-bandwidth
data path.

## Consequences

The normative scope now matches the active Backlog. Parser and pairing tests can
run without a phone or network dependency. GPL distribution obligations remain
outside CapyIO while the app stays separately installed, but any future bundling
or source import requires a fresh distribution and legal review. Physical tests
must identify the approved target and cannot be generalized into production or
security claims.
