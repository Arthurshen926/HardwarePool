# ADR 1455: Establish a fresh Session binding inside paired mTLS

- Status: Accepted transport handshake; application ownership pending
- Date: 2026-09-18

## Context

The control TLS transport originally required both endpoints to supply the same
complete Session/server/client binding before connecting. That is appropriate
for reconnecting an already admitted Session, but neither discovery nor the
paired-peer directory carries ephemeral Session IDs. An application connection
manager therefore had no secure way to create the first shared Session without
an out-of-band value.

## Decision

The existing fixed 64-byte binding record gains a second host-selected server
mode without changing its bytes or version. After TLS 1.3, ALPN, mutual chain
verification and exact paired-leaf pinning succeed, the client generates a
non-nil random `SessionId` and sends the existing request containing that ID and
the expected server/client Node IDs. A fresh-session server decodes the record,
requires canonical header/reserved/length fields and exact configured Node IDs,
then acknowledges the complete proposed binding. Both channels expose the same
accepted value.

The original exact-binding constructors remain for an already known Session.
The fresh server never accepts caller-selected Node identities, and a changed
Node fails after mTLS without producing a channel. TLS early data and tickets
remain disabled. A platform connection owner must reject a Session ID already
active for that peer, create/admit the Runtime Session transactionally and close
the channel if admission fails.

This handshake agrees on a connection Session identifier only. It does not
enroll a peer, grant a Capability, create a Route or authorize hardware.

## Consequences

- Paired applications can establish a shared fresh binding without placing an
  ephemeral Session ID in discovery or persistent trust metadata.
- Exact reconnect mode and the wire record remain backward compatible.
- Listener scheduling, active-ID replay checks, Runtime admission, reconnect
  reconciliation and application lifecycle ownership remain follow-on work.
