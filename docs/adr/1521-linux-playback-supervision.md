# ADR 1521: Fair supervision of admitted Linux lab playback

Status: implemented; hosted TLS/IPC validation pending.
Date: 2026-10-20
Plan: [LINUX-A09](../plans/active/LINUX-A09-playback-supervision.md)

## Decision

Add `LinuxPlaybackHost::from_admitted_trusted_lab` as an explicit host library
entry consuming ADR 1520's real authenticated/admitted owner. The returned bounded
`PlaybackStopHandle` is local lifecycle authority, not a network command. `run`
owns the calling thread until local Stop, loss of all Stop handles, authenticated
peer Stop, disconnect, authority/lifetime expiry or another terminal failure.
Every exit attempts media and dedicated-Session cleanup before returning; cleanup
failure is not successful Stop. Snapshot and media metrics remain inspectable.
There is no detached control reader, thread that outlives the run, public
synthetic admission path, listener, credential loader or new CLI option.

Each loop checks local cancellation, polls at most one authenticated control
frame and one bounded media receive, and waits at most 2ms for local cancellation.
The audio owner's new `poll` validates authority before control and again before
media. The existing worker revalidates after its at-most-10ms UDP receive before
writing PCM. Continuous heartbeats cannot monopolize the loop or renew grants.
A local cancellation arriving during bounded control reply, resource open or
media receive is observed when that step returns, not asynchronously inside it.
Control writes retain the absolute deadline capped by one second and current
lease/grant/lifetime. Existing bounded child cleanup can take up to 1.1 seconds
per attempt. OS scheduling and process creation are not hard real-time bounded;
this does not promise a hard stop latency or gap-free playback.

## Additive incremental authenticated framing

Both existing TLS channel types expose `poll_envelope_until`, returning `None`
for idle or bounded partial progress. Rustls still authenticates/decrypts all
bytes. A temporary nonblocking socket adapter bounds wire bytes and I/O calls
per poll, including TLS traffic producing no plaintext. Existing synchronous
and deadline APIs remain unchanged for callers that never use polling. Mixing
blocking receive with a partial polled frame fails closed rather than losing
frame boundaries. Writes remain permitted with partial receive state.

The bounded length prefix and body are retained across polls. The application
frame deadline starts at its first plaintext byte and can only become earlier,
never be renewed by subsequent polls or trickle. Expiry, EOF, malformed/oversized
frames and I/O failure close the channel. A complete frame clears its state; already buffered coalesced plaintext keeps
the earlier deadline and is returned one frame per poll. No raw
TLS parsing or peek-based readiness assumptions are introduced.

Encrypted-only partial TLS records have no separate application-frame timer.
They cannot monopolize a poll, and the host continues cancellation, media and
finite grant/lease/lifetime checks. This is deliberately not advertised as a
first-encrypted-byte deadline. The polling API by itself has no background
watchdog: its embedding owner must drive it. The new host does so continuously.

## Compatibility and security

No wire changes, authority creation, automatic renewals, reconnect, certificate
provisioning, environment secrets, new external production packages or platform
permission changes. Linux host reuses the existing workspace Linux audio crate.
Windows/Android and the inherited-stdio mock host continue unchanged. Default
CLI behavior remains closed. Authenticated control does not authenticate or
encrypt the UDP media; private/loopback/overlay scope is still a trusted-lab
restriction, not spoof protection. Real Android/PipeWire hardware is unverified.

## Concrete remaining executable-host prerequisites

A usable executable launch path still requires an explicit trusted composition
of these existing responsibilities, not more unvalidated Ready/config input:

1. Load an already enrolled local identity and paired peer certificate pin from
   an approved protected credential provider. Do not put keys in CLI arguments,
   logs, ordinary JSON management requests or environment variables. Credential
   enrollment/persistence remains a separate authorized action.
2. Resolve stable local/remote Node IDs and the exact authenticated Session using
   the existing paired admission/control flow. Catalog discovery must provide the
   real remote Source and local Sink descriptors, not DemoLab fixtures.
3. Obtain explicit local consent for the exact Route/profile/format/duration and
   target. Existing Runtime admission must create the Starting Route's finite
   grant and local exclusive Sink lease; neither peer Ready nor config values
   may manufacture authorization. Reject stale/revoked/mismatched admission.
4. Pass the admitted channel/Runtime/offer and explicit trusted-lab destination
   into this supervisor, with a trusted local Stop owner wired to executable
   shutdown/EOF. Any future listener requires explicit opt-in and a separately
   reviewed bind/accept lifetime; this milestone opens none.
5. Validate real paired Android control and PipeWire playback on an explicitly
   authorized test device. Synthetic TLS/UDP/child tests do not establish this.

## Verification

Socket-free tests exercise host cancellation precedence, peer-stop iteration,
cleanup on offer/poll/cleanup errors, and bounded idempotent Stop signaling.
Incremental framing tests cover bounded storage and resumable partial progress.
Hosted-only actual TLS tests cover idle/fragmented/coalesced control. Audio owner
fixtures exercise idle media delivery, polled authenticated offer/readiness,
remote Stop, idle lease expiry and disconnect cleanup, using `/bin/cat` rather
than PipeWire. Host integration fixtures supervise real pinned TLS while waiting
for peer offer and test cancellation/lifecycle termination without hardware.
No sandbox-denied socket operation is retried locally; compile is not execution.
