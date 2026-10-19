# ADR 1463: Android advertises its retained control listener

- Status: Accepted send-only automatic discovery
- Date: 2026-09-18

## Context

ADR 1462 retained a working Android control listener on an ephemeral port, but
Windows could not learn that port automatically. The repository already defines
a fixed 32-byte unauthenticated discovery hint and a one-hop administratively
scoped IPv4 multicast transport.

## Decision

`capyio-discovery-udp` now provides a send-only announcer bound to an ephemeral
socket on the operating system's active IPv4 route. It uses the same fixed
group `239.255.67.80`, port `54785`, TTL one, disabled multicast loopback and
exact record encoder as the receiver.

The Android native listener creates one random nonzero per-process nonce and an
announcement containing only its stable Node ID and actual retained control
port. The Service worker sends immediately and every five seconds, below the
15-second candidate TTL. Announcement success projects shared discovery state
`listening`; failure leaves it `not_started` but does not close the secure TCP
listener. Android only sends and therefore adds no multicast-receive permission
or lock.

The announcement remains an untrusted hint. Windows must match its Node ID to
an active local paired record, use the packet source address, and complete mTLS
leaf pinning plus Node/Session binding before catalog admission.

## Consequences

- A paired Windows desktop with discovery enabled can automatically obtain the
  Android control endpoint without manual IP/port entry.
- Route choice follows Android's active IPv4 routing policy; unusual VPN or
  multi-homed configurations may require future explicit interface selection.
- Enrollment remains required before the announcement can become connectable.
