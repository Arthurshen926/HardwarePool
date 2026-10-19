# ADR 1465: Session-bound control heartbeat

- Status: Accepted protocol/link primitive; platform loops pending
- Date: 2026-09-18

## Context

After initial catalog exchange, an idle authenticated TLS connection carried no
traffic. A host therefore could not distinguish a healthy idle peer from a
half-open connection without abusing CatalogSnapshot or waiting for a later
unrelated command to fail.

## Decision

Protocol v1 minor advances from 1 to 2 and appends `ControlHeartbeat` at the
previously reserved Envelope field 47. It contains only a nonzero
connection-local sequence and an acknowledgement flag. The normal Envelope
Session ID is mandatory; it carries no timestamp, authorization, catalog or
hardware data.

The initiator sends `(sequence, false)` and the peer echoes `(sequence, true)`.
`CatalogControlLink` encodes, sends, receives and verifies the heartbeat under
its exact authenticated Session binding. Invalid payloads, transport failure or
wrong Session close the link. Sequence scheduling, timeout count and retry are
platform-loop responsibilities.

Field numbers 48 through 60 remain reserved. Major version remains 1 because
older implementations may ignore the additive payload but cannot claim
heartbeat support without minor 2 negotiation.

## Consequences

- Platforms gain a semantically clean liveness primitive without changing TLS
  identity or catalog semantics.
- Heartbeat success proves only recent possession of the live authenticated
  channel; it grants no capability or lease.
- Windows and Android still need socket-owning long-lived dispatch loops before
  half-open cleanup is automatic.
