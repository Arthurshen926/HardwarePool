# ADR 1466: Platform control-heartbeat owners

- Status: Accepted for the Windows-dials-Android topology
- Date: 2026-09-18

## Context

ADR 1465 defined and tested a Session-bound heartbeat, but a successfully
exchanged application link was otherwise idle. UI commands owned the Windows
socket, while Android retained its accepted link without servicing more frames.
Neither platform therefore reconciled a half-open authenticated Session.

## Decision

After initial catalog exchange, Windows Desktop transfers the link to one
socket-owning background controller per peer. Every two seconds it sends the
next nonzero heartbeat request and requires an acknowledgement with the exact
sequence. Stop, transport failure, invalid payload, wrong Session or mismatched
acknowledgement closes the link and reconciles the exact Runtime Session. The
controller updates the bounded control-plane counters; UI snapshot calls never
perform network I/O.

The Android Service's existing listener worker services the retained accepted
link. It accepts only a heartbeat request, echoes its sequence as an
acknowledgement and returns to accept mode after failure and Session cleanup.
The five-second socket timeout bounds shutdown and failed-peer detection.

## Consequences

- An idle but healthy application control link is now active and observable.
- Disconnect and half-open cleanup converge on the same Runtime lifecycle.
- This slice deliberately supports the current Windows-dials-Android direction;
  simultaneous initiators, multi-peer Android ownership and arbitrary message
  dispatch remain separate work.
- Packaging and loopback tests do not replace a physical network-loss timing
  test.
