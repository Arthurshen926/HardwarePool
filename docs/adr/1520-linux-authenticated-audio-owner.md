# ADR 1520: Accepted authenticated control owner for Linux lab playback

Status: implemented; local socket-free verification; hosted TLS/IPC pending.
Date: 2026-10-20
Plan: [LINUX-A08](../plans/active/LINUX-A08-authenticated-audio-owner.md)

## Decision

Compose ADR 1519's real Runtime playback worker with the existing
`AuthenticatedControlServerChannel` and `CatalogControlLink`. The new
`AuthenticatedPlaybackOwner` is an explicitly invoked **library** owner for one
already admitted, locally authorized Route on a dedicated Session. It does not
add a Linux listener, pairing UI, credential loader/store, catalog publication,
new wire message, CLI option or inherited-stdio host command.

The host supplies a Runtime containing the already negotiated Starting Route,
finite grant and live local Sink lease. The owner never registers a synthetic
peer, turns a Ready assertion into authentication, grants access or acquires a
lease. Existing admission/catalog/local-consent composition remains a caller
prerequisite, not a completed Linux product flow.

## Authentication and authority

Construction consumes an actual authenticated TLS server channel and compares
its complete Session/server Node/client Node binding with the host's explicit
expected paired binding. Route endpoint Nodes and Runtime Session must match.
The accepted channel must have been created using the existing paired-peer
certificate fingerprint through the existing TLS authentication implementation; accepting an
arbitrary trusted certificate is not a substitute. No public API accepts raw
peer `ValidatedRouteStatus`/Ready objects. Reciprocal offers are obtained only
by reading the owned authenticated channel.

Only the caller's explicit local IPv4 endpoint, finite whole-second 1–300 second
budget, PipeWire target and Sink/mono offer are configurable. The peer IP comes
from the authenticated TCP socket, never supplied offer metadata. Only its UDP
port comes from the bounded typed reciprocal offer. All ADR 1518 lab address and
target policies still apply. `prepare_native_audio_worker` validates matching
Session, Route endpoints/backend/profile/epoch, Stream, exact selected format
and QoS, reciprocal direction, finite grant and current local lease before any
media open. Unexpected offers/commands, changed active UDP port and wrong
bindings fail closed. No authority is created from these checks.

Authenticated control **does not authenticate or encrypt UDP media**. Its
expected source IP/port and dynamic identifiers remain lab scope, not protection
against spoofing. The public constructor name explicitly requires trusted-lab
intent. No fallback to static bindings is introduced.

## Lifecycle and threading

- `send_local_offer` sends the preauthorized Starting Route's Sink offer once.
- `receive_control` reads one real TLS frame. A matching reciprocal audio offer
  opens the existing UDP/PipeWire worker and sends Active only after successful
  resource opening. Active remains readiness, not audible playback.
- Heartbeats are acknowledged without grant/lease renewal. A Stop command is
  accepted only for the exact owned Route and Session. Other message families
  fail closed on this deliberately audio-only connection. Exact repeated offers
  do not restart the worker or replace its active endpoint.
- `poll_media` drives actual bounded delivery. Current grant/lease validation
  and the finite monotonic owner/worker budgets still apply. No public Runtime
  mutation interface permits a peer to fabricate local authorization.
- Timeout, EOF, malformed/mismatched offer or media failure is terminal: close
  the exact Session, invalidate its data epoch/release leases, and stop owned
  media. Cleanup errors are not silently reported as success. Stop/Drop also
  close control and media; explicit Stop is terminal and does not promise a
  graceful wire acknowledgement. Other Sessions remain untouched. Construction
  rejects a Session containing multiple Routes so this close cannot silently
  terminate unrelated traffic on the same channel.

This is caller-driven and has **no persistent control reader or between-call
lifetime supervisor**.
Each control operation has an absolute deadline: the earliest of a one-second
step cap, the monotonic owner lifetime, finite grant and local Sink lease. New
opt-in TLS/link deadline methods start a cancellable watcher holding a clone of
the same owned TCP socket. Expiry shuts down only that connection, interrupting
`read_exact`/TLS even if fragments trickle within per-syscall timeouts. Completion
cancels and joins the watcher; no sleeper remains. The same deadline covers a
received frame and its heartbeat/readiness reply. Existing TLS per-syscall
methods and their callers are unchanged. Timeout is terminal, not an idle poll
to retry after a partially consumed frame. A blocking control read still pauses
media polling for its bounded step. Hosts must schedule these calls deliberately;
this slice does not claim continuous low-jitter playback while control I/O is
blocked. Inactivity without calls does not independently close resources, and
remote disconnect is observed at the next control I/O, not magically by UDP.
A future scheduled host composition must address that before product integration.

## Compatibility and dependencies

Windows/Android/Core/wire behavior is unchanged. TLS and control-link gain only
additive opt-in whole-operation deadline methods; existing call paths retain
their previous behavior. Reuse the
existing workspace control-link, control-tls and protocol dependencies and the
same already-pinned rcgen 0.14.5 dev-only fixture dependency used by control-link.
No new external production dependency or credential provisioning is added.
Synthetic test keys/certificates remain in memory and are never persisted.

## Verification boundary

Socket-free tests validate expected paired binding, current local authority,
explicit target/address/preset policy and dedicated-Session isolation. Hosted-only
tests perform real pinned TLS offer exchange, dynamic UDP-to-owned-child PCM,
readiness/Stop, wrong-Stream rejection before media open, disconnect cleanup,
terminal timeout clamped to the current lease and Drop cleanup. Separate hosted
TLS tests prove trickled framing cannot renew the absolute deadline and successful
completion cancels the watcher without closing the connection. They use
`/bin/cat`, not PipeWire or Android.
Compile-only verification is not a TLS/IPC pass. Default host behavior remains
closed; actual Linux pairing/device acceptance and secure media remain separate.
