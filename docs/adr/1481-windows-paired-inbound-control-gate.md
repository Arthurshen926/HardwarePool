# ADR 1481: Bounded Windows paired inbound control gate

- Status: Accepted as an opt-in integrated-alpha diagnostic, not app auto-listening
- Date: 2026-09-19

## Context

Windows currently dials Android for authenticated control. ADR 1480 makes
newly issued credentials usable in either TLS role, but there is no Windows
accept path. A reverse-dial implementation needs an independently testable
Windows server boundary before it can be attached to process-owned discovery
and connection arbitration.

## Decision

Add an explicit one-peer, one-connection CLI diagnostic. It requires an exact
paired Node ID and numeric local bind IP/port, reloads the current Windows
user's pairing directory and DPAPI credential, rejects unknown/revoked peers,
and constructs a TLS 1.3 server pinned to that peer's exact leaf and ordered
Node binding. It waits for at most 20 seconds. Invalid handshakes do not
consume the accept gate; the subsequent valid peer may still connect. A
successful connection admits one fresh Session, receives and applies the peer
catalog, sends a local empty-publication catalog, acknowledges one heartbeat,
then closes and reconciles. No hardware Route is started.

The TLS server resets each accepted socket to blocking mode with finite
read/write timeouts. This is necessary on Windows when the listening socket
is nonblocking; otherwise TLS may abort on inherited `WouldBlock` despite a
valid peer. The accept loop itself remains nonblocking and time-bounded.

## Limits and verification

The CLI does not enable a persistent Windows listener, advertise Windows, or
make Android dial. It requires dual-EKU new-pair credentials; old role-specific
Windows leaves are not migrated. The loopback regression sends a non-TLS
connection, then a newly issued Android identity, and verifies pinned mTLS,
catalog application, heartbeat acknowledgement and cleanup. Existing control
TLS tests continue to cover certificate, ALPN and binding rejection. Physical
Android-initiated acceptance remains unproven.
