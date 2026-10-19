# ADR 1432: Bounded unauthenticated LAN discovery candidate

Status: Accepted

## Context

The shared UI needs automatic nearby-node discovery, while the current product
has no authenticated control transport. A discovery message must not be treated
as proof of Node identity, authorization, reachability or catalog ownership.

## Decision

Add a transport-free `capyio-discovery` contract with one exact 32-byte,
version-1 announcement. It contains only magic/version/reserved flags, a
non-zero proposed control port, a non-nil Node ID and a non-zero random
per-process instance nonce. It contains no IP address, name, capability list,
credential, pairing code or stable secret. The receiving platform must take the
candidate IP from the datagram source address and must authenticate the later
control connection before opening a Session or admitting a catalog.

The codec accepts exactly 32 bytes. Unknown magic, versions, flags, trailing
bytes, zero ports, nil identities and zero nonces fail closed. The nonce only
distinguishes process restarts; it is not authentication or replay protection.
This slice opens no socket, chooses no multicast group/UDP port, changes no
Android permission and performs no background work.

The same pure contract retains at most 64 candidates for 15 seconds using a
caller-supplied monotonic timestamp. It derives the endpoint IP from packet
metadata, rejects unspecified/loopback/multicast sources and the local Node ID,
ignores older observations, and distinguishes refresh from a changed process
nonce. Capacity fails closed rather than silently evicting another candidate.

Before platform socket integration, a deterministic `DiscoveryIngress` adds the
receive-loop boundary: at most 256 datagrams per monotonic one-second window,
with malformed datagrams consuming the same budget. Backward time, excess rate
and malformed frames fail closed before candidate admission. The ingress still
performs no I/O and selects no UDP address, port or interface.

## Consequences

Windows and Android can later share one candidate format without placing
network behavior in Core or the UI. The platform discovery host must still add
bounded rate/retention/expiry, self-filtering and interface policy. Production
use additionally requires authenticated encrypted control transport, pairing,
replay/downgrade defense and explicit catalog/Route authorization.

Eight unit tests cover exact round trip, framing/version/flags, invalid
identity/port/nonce rejection, source policy, restart/stale observations and
capacity/expiry plus rate-limited ingress. Warnings-denied Clippy passes without
adding an external dependency.
