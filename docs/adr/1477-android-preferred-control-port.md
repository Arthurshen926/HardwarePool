# ADR 1477: Preferred Android control port for repeat peer connections

- Status: Accepted for the integrated alpha
- Date: 2026-09-19

## Context

Android currently binds its control listener to an operating-system-selected
port on every Service start. On networks without multicast discovery, a
previously authenticated peer can remember the host but cannot reconnect
after the port changes. The numeric endpoint is only a routing hint; ADR 1470
already requires exact active pairing and pinned mTLS for every attempt.

## Decision

The Android JNI control listener first tries TCP port 43193 on the same
all-interface bind used previously. If that port is already occupied, it
falls back to an ephemeral port and publishes the actual bound port through
the existing status and discovery record. Any other bind error remains a
failure; the listener never claims the preferred port when it is not bound.

The preferred port does not select a peer, certificate, permission or Route.
It is not a new public unauthenticated API. Existing per-peer credentials,
exact Node binding, admission and resource authorization remain unchanged.
The current all-interface exposure and unpaired TLS pressure remain alpha
risks; production enrollment and network-scope policy are separate work.

## Consequences

- ADR 1476 endpoint hints can survive an ordinary Android Service restart
  when its host address is stable and the preferred port is free.
- Discovery and manual endpoint fallback remain necessary after address
  changes or preferred-port conflicts.
- This does not provide Android outbound dialing, Windows inbound listening
  or interactive first pairing; Node transport symmetry remains incomplete.
