# ADR 1548: Interactive finite paired-store playback

Status: experimental; isolated terminal/graph acceptance is mandatory.
Extends ADR 1547's terminal controller without changing the Node protocol.

## Scope

The console adds exactly one media command: `playback --config ABSOLUTE_PATH
--peer NODE_UUID`. It supports the existing playback Sink and virtual-microphone
endpoint projection. Capture, virtual speaker, camera, touchpad, incoming peer
Drafts, renewal and session restart are not included. Pairing-management commands
retain ADR 1547's no-resource behavior. This explicitly supersedes only that ADR's
console-wide pairing-only scope, not its secret, terminal or process boundaries.

The four explicit paired-store selectors remain required. The console invokes
only its trusted sibling `capyio-node linux-playback --trusted-lab`, adding the
exact selected peer and sending the existing config plus distinct paired-store
marker over anonymous pipes. It never receives or exports the protected bundle.
The Node independently loads the active exact peer, establishes pinned mTLS,
checks the advertised catalog, and enforces existing local/remote grants and
finite leases. Pairing alone is never playback consent.

## Nonsecret intent and current-session confirmation

The manifest is a bounded regular JSON file, not a credential input. It uses the
existing Node playback schema, with version 1, local node ID, exact remote Source
PortRef, explicit listen/media endpoints, endpoint mode/target and finite duration.
The file must be absolute, non-symlinked at its final component, owner/root-owned
and not group/other-writable. File contents are read once and bounded to 4096 bytes;
unknown fields and invalid or conflicting values fail before a media child starts.
The byte bound does not promise a wall-clock bound on kernel/filesystem latency;
config reading happens before a terminal, listener or resource child is opened.
The local and remote IDs must match the explicit selectors. Only the existing
loopback/private/overlay IPv4 lab address policy and 1–300-second duration are
accepted. Automatic/default device selection is not offered. A playback target is
explicit; virtual-microphone mode creates only the existing session-owned endpoint
and cannot nominate an external target. Only Linux-led proposal mode is supported.

Starting the explicit command permits the finite authenticated listener, not a
resource grant. Only the Node's current `approval_required` event opens a trust
prompt. The console verifies its source, local Sink, target, endpoint mode, format,
duration and security label against the immutable manifest; IDs must be nonnil.
It shows the exact peer, Source-to-local-Sink direction, device/projection, duration,
route/session IDs and the unauthenticated plaintext UDP warning. The user must
type `start REQUEST_ID ROUTE_ID PEER_ID` after the fresh prompt. Generic approval,
pretyped input and mismatched identifiers fail closed. Terminal input is flushed
before each new authorization prompt. The existing Node request/route recheck and
the counterpart's independent resource approval remain mandatory.

This does not secure audio media: only control uses pinned TLS. The existing UDP
media remains unauthenticated and unencrypted, suitable only for the explicitly
trusted lab network. The displayed warning and explicit per-session decision do
not convert that limitation into production security.

## Status, Stop and cleanup

Validated public playback events are rendered only on the controlling terminal,
with a human-readable scope summary and a canonical `Playback event:` line. The
canonical line contains only the allowlisted nonsecret Node event schema; it is
not a new transport protocol, secret/status export or a claim of activation. In
particular, authenticated connection, local approval and waiting for an offer are
not `Active`; activity is shown only from the existing runtime status.

`status` observes the current session; `stop`, cancellation, terminal EOF and
interruption terminate this invocation. No persistent grant, implicit renewal,
reconnect or retry exists. The Node's lease/duration remains authoritative; the
console adds a 55-second preapproval envelope, the existing 30-second consent
window, and at most the chosen duration plus a three-second Stop grace after
approval. Framing is capped at 4096 bytes/2048 events, partial frames at five
seconds, status requests at 1024 and individual terminal writes at one second.
These bounds never extend the Node's permission. It requests the existing Stop command and closes input, waits boundedly
for its own child and escalates to killing/reaping only that child when necessary.
The original terminal state is restored under ADR 1547's normal-shutdown limits.
Protected credentials are retained; stopping a resource does not revoke pairing.

## Acceptance boundary

The hosted console profile reuses the existing Android-policy peer and private
Secret Service fixture, but drives the actual console through an owned PTY. It
never writes approval directly to the Node's private input. The existing isolated
PipeWire graph, independent ordered waveform oracle, owner/PID checks and cleanup
checks remain in force. Both playback and virtual-microphone profiles exercise
waveform plus Stop, no approval, pretyped input, mismatched request/route/peer,
finite expiry and authenticated peer EOF. Expiry is checked against the actual
negotiated Source lease, which production also applies to the local Sink. The
four-second grant intentionally yields a shorter lease through the existing
one-second transit reserve; the test rejects failure before that exact lease
expiry and bounds termination afterward without extending authority. The existing legacy and paired-store
media profiles remain separate and mandatory. Console evidence has its own exact
marker and cannot be substituted by a paired-store-only run.

These are synthetic owned test endpoints, not user speakers, microphones or a
personal keyring. The change does not claim physical-device operation, production
network security, a media-selection GUI, all four media directions, or unattended
service lifecycle. No additional production dependency is introduced.
