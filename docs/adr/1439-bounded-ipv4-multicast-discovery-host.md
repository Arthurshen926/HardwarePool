# ADR 1439: Bound IPv4 multicast discovery hosting

- Status: Accepted transport foundation; application lifecycle pending
- Date: 2026-09-18

## Context

The exact discovery record and bounded ingress perform no I/O. Windows and
Android still need the same narrow network policy without putting sockets in
Core, the UI or a kernel component. Discovery remains an unauthenticated hint
and must not silently expand beyond the local administrative domain.

## Decision

Add `capyio-discovery-udp`, a standard-library IPv4 multicast host with explicit
construction and no background thread. For portable multicast reception it
binds wildcard IPv4 UDP port 54785 from the dynamic or private range, then joins
administratively scoped group `239.255.67.80` on one caller-selected concrete
non-loopback IPv4 interface. It uses multicast TTL 1, disables local
multicast loopback and polls with a 250 ms timeout. The socket sends only the
exact 32-byte version-1 announcement. Receive buffers are 33 bytes so oversized
frames reach the existing exact-length rejection path.

The caller owns interface selection, start/stop, monotonic timestamps and the
`DiscoveryIngress`. Unspecified, loopback, broadcast and multicast interface
addresses fail before socket creation. A timeout is an idle poll, not a
connectivity error. Binding failure is surfaced; the host does not silently
choose another port. Because the portable receive socket is wildcard-bound,
unicast datagrams reaching that fixed port on another interface may also be
observed; they remain rate-limited, unauthenticated hints and never bypass later
exact pairing plus mTLS.

This slice does not automatically start a listener, change a firewall, add an
Android permission or acquire a platform multicast lock. It does not create a
peer, Session, catalog or Route.

## Consequences

- Windows and Android can reuse one wire and socket policy while retaining
  platform lifecycle ownership.
- The chosen private multicast group needs no globally unique allocation, and
  the dynamic/private port may conflict with another local process; such a
  conflict fails visibly.
- Application integration must still enumerate reviewed interfaces, start one
  bounded owner, expire candidates and authenticate an exact paired match over
  mTLS before admitting any remote state.
