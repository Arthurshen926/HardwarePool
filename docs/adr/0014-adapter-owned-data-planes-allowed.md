# ADR 0014: Adapter-owned data planes are allowed

- Status: Accepted
- Date: 2026-08-23

## Context

Forcing MicYou, Audio Share, VCamdroid, USB/IP and research systems through one
new transport would delay usable integration and discard mature mechanisms.

## Decision

Unify control/lifecycle but allow `AdapterManaged` and `ExternalProtocol` data
planes. `StandardPort` is claimed only when arbitrary compatible endpoints can
really interoperate.

## Alternatives

One universal data plane immediately; independent tools with no common control.

## Consequences

Early Routes may have limited composability and UI must label it. Valuable paths
can migrate to CapyDataPlane later based on evidence.

