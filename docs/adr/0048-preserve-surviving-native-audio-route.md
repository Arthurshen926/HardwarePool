# ADR 0048: Preserve a surviving native audio Route

Date: 2026-09-01

Status: Accepted for `CAPY-AUDIO-NATIVE-001G1`

## Context

The Windows native-audio service starts its speaker and microphone children as
one configured pair, but the children own different Routes, UDP endpoints and
ring projections. The initial composition treated `speaker && microphone` as
the Broker process liveness result. After either child exited, the generic
service Runtime therefore failed and stopped both children. That contradicted
the independent-Route failure boundary already enforced by the Core and
Android Node.

## Decision

Initial pair startup remains transactional: if microphone startup fails, the
newly started speaker is rolled back and the service never publishes a partly
started generation.

After successful startup, liveness and readiness are distinct:

- the native-audio process is live while either child remains running;
- it is ready only while both configured children are running;
- loss of one child moves the service from `active` to `starting` without
  stopping the surviving child or publishing a receiver-presence claim;
- loss of both children remains a terminal Broker exit and uses the existing
  bounded stop/failure path.

This slice deliberately does not add automatic child restart, independent
Desktop controls or a new public service snapshot schema. A later 001G slice
must add bounded per-direction recovery and diagnostics before claiming
hands-off reconnect.

## Consequences

One failed media direction no longer tears down the opposite healthy Route.
The existing schema honestly reports that the configured pair is not fully
ready, but cannot yet identify which direction failed. Physical partial-exit
and recovery evidence is still required; unit tests alone qualify only the
service orchestration boundary.
