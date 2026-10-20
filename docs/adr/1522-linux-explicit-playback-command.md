# ADR 1522: One-shot explicit Linux trusted-lab playback command

Status: implemented; review and hosted executable/TLS validation pending.
Date: 2026-10-20
Plan: [LINUX-A10](../plans/active/LINUX-A10-explicit-playback-command.md)

## Decision and deployment gate

Add `capyio-node linux-playback --trusted-lab`, a separate executable path using
ADR 1521 supervision. No default command or inherited-stdio management behavior
changes. Only explicitly invoking this command can bind a finite private lab
listener. This implementation work neither provisions credentials nor deploys
or executes the command with user credentials or devices.

Deployment is a separate approval gate: an authorized local launcher must already
possess an issued credential bundle and the paired peer record, protect both
processes and their inherited pipes, and obtain explicit local approval for the
exact challenge. Credential issuance, export/access, persistent storage, system
permissions, actual listener deployment and physical Android/PipeWire testing
are not authorized by this ADR. Do not paste keys into a terminal, CLI argument,
environment variable, log or ordinary management request. No credential files
are loaded or written by the command.

Inherited stdin is **not inherently secure or proof of a human's identity**.
The local launcher/invoker is the trusted consent principal and can supply any
input, including approval. Another process with pipe/process access can obtain
secrets or impersonate that principal. The command assumes exclusive trusted
FIFO ownership, not protection against that attacker. It rejects terminals,
regular files and other descriptor types. Duplicates share file-status flags;
nonblocking mode therefore affects the inherited originals too and restoration
on Drop is best effort. Concurrent readers/writers/flag changes are unsupported.

## Local protocol v1

Every stdin/stdout message is a four-byte unsigned big-endian length followed by
exactly that many bytes. Zero, excessive length, malformed content, unknown JSON
fields, unexpected message order, EOF and read errors are terminal. No newline
or shell parsing is involved. A local partial frame has a nonrenewable one-second
deadline starting at its first prefix byte. True idle starts no frame deadline.
Stdout JSON is capped at 65,536 bytes and each complete write at one second;
a blocked launcher cannot leave playback running indefinitely. All error events
and returned command errors are fixed strings with no parser/TLS/runtime details.
Syntax errors when the first subcommand is `linux-playback` are also fixed and
never echo supplied argument values; ordinary other CLI diagnostics are unchanged.

The one-shot bootstrap has a shared five-second absolute deadline and exactly
three frames, in this order:

1. Nonsecret launch JSON, at most 4,096 bytes, with exactly these fields:
   `version` (1), `node_id` (non-nil local Node UUID), `source` (existing PortRef
   object: `node_id`, `capability_id`, `port_id`), `listen` (literal IPv4:port),
   `media` (literal IPv4:nonzero-port), `target` (explicit PipeWire name/serial),
   `duration_secs` (integer 1–300). Source Node must differ from local Node.
   Listen and media IP must match and satisfy the existing concrete
   private/loopback/shared-overlay policy; unspecified/public/broadcast targets
   are rejected. Listen port zero explicitly requests an ephemeral port. Target
   policy excludes shell syntax and implicit `auto`/default targets.
2. Existing `PairedPeerDirectory::encode_json` representation, at most 65,536
   bytes. It must contain exactly one active peer, matching the selected Source
   Node; a revoked or additional record fails. Its expected certificate pin is
   passed to existing mutual TLS. This imports no record into persistent storage.
3. Existing binary `ControlCredentialBundle::encode` bytes, at most 65,536 bytes.
   Only the already-issued local identity/CA bundle is accepted. No certificate
   generation, enrollment, fallback identity or unauthenticated mode exists.

The raw credential frame is held by a non-Debug/non-Clone buffer and overwritten
on error/Drop immediately after guarded bundle decoding. The bundle remains
protected until TLS configuration consumes it. Server configuration guards the
transient private DER on early CA/key errors, validates borrowed DER, and moves
the same allocation into Rustls. This reduces unnecessary retained copies; it is
**not secure erasure**. Rust `fill(0)` is best effort, compiler/swap/process memory
are not protected, and Rustls retains key material needed during the channel
lifetime. Existing public identity/client APIs are otherwise unchanged.

After bootstrap, only bounded local JSON command frames are accepted:

- `{"version":1,"command":"status"}`: request current phase or owned Route
  state and packets-written count. It creates no authority.
- `{"version":1,"command":"stop"}`: terminal local cancellation.
- `{"version":1,"command":"approve","request_id":"<challenge UUID>",
  "route_id":"<challenged Route UUID>"}`: valid only during the pending exact
  challenge after authenticated catalog exchange. No other fields or wildcard
  approval are accepted. A fresh random request ID prevents preplayed approval.

Output event names are `listening` (actual bound address), `approval_required`,
`awaiting_peer_offer`, `status`, `stopped`, and `failed`. No event includes
credential bytes, paired directory, arbitrary incoming error text or secret
handles. A `failed` event is best effort only while the launcher channel remains
usable. EOF/framing failure poisons the channel, so launchers must use bounded
process exit plus the fixed stderr diagnostic, not require another framed event. `approval_required` includes the fresh request ID, Route/Session IDs,
exact Source/Sink, explicit target, exact mono format, finite duration, 30-second
approval timeout and explicit insecure-UDP warning. `awaiting_peer_offer` is not
readiness. Only actual owned worker opening yields Active in later Route status;
Active is not proof of audible output.

## Authenticated admission, consent and lifecycle

The opt-in listener has a 15-second accept window. New `try_accept_until` performs
one nonblocking accept and bounds the entire accepted TLS handshake plus bind
exchange with the existing absolute socket watchdog, capped by the command at
one second. Mutual TLS1.3, ALPN, paired certificate pin and fresh Session with
exact expected Node identities remain mandatory. The listener is dropped after
one accepted peer; errors terminate, with no retry/reconnect or persistent
accept loop. Existing `accept` callers retain their behavior.

The real link admits a provisional authenticated peer, receives/applies its
catalog within five seconds, and sends the explicit local software playback
catalog under the same deadline. No remote descriptor is synthesized from config.
The selected catalog Source must be available mono audio-frames with Interactive
QoS; normal Runtime Route compatibility validation also applies. The local Sink
is an explicitly selected software process destination with fresh per-invocation
IDs, not a discovered hardware port or claim that PipeWire has been probed.

A new Route remains unauthorized while the challenge is pending. Local approval
must match both fresh request ID and exact Route ID, before the 30-second
monotonic deadline. The pending challenge cannot be changed by later catalog or
peer offers; unexpected peer messages fail closed, while heartbeats may be
acknowledged without authorization. Only then does the local consent owner create
the finite grant and exclusive local Sink lease, prepare/start the selected
Route, and transfer the same authenticated catalog link to the playback owner.
No network Ready/status, config flag, TLS connection or catalog alone grants
access. There is no automatic renewal, consent reuse or reconnect.

The existing supervisor now accepts a crate-private bounded local-control poll,
keeping launcher Stop/status on the same owner thread as control/media. No
blocking stdin reader or detached worker is added. EOF/error, local cancellation,
peer Stop/disconnect, expired authority and media failure terminate and reconcile
the dedicated Session/leases. Phase deadlines are checked between steps; a
control/status write or accepted handshake may delay cancellation by up to its
one-second operation budget plus scheduling. Resource opening and process
cleanup retain ADR 1521 limitations; this is not a hard-real-time stop guarantee.

Authenticated control still **does not authenticate/encrypt UDP media**. This
command is a trusted-lab executable path, not completed product pairing/UI,
secure media, guaranteed Android interoperability or hardware acceptance.

## Dependencies and compatibility

Add Linux-only direct `rustix = 1.1.4` with `fs,std` features, already present at
that exact version in Cargo.lock, for safe descriptor duplication/type checking
and nonblocking flags without unsafe code in this crate. License from the pinned
package: Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT. Upstream is
Bytecode Alliance rustix (`https://github.com/bytecodealliance/rustix`); this is an
upstream package dependency, not vendored or modified code. Retain the exact pin
and audit upstream changes before upgrades; no claim about latest release.
Std alone does not expose the required safe nonblocking FIFO flags; unsafe libc
or detached blocking reader threads were rejected. Existing workspace pairing,
credential, TLS/link/protocol crates are reused. Tests reuse the existing pinned
rcgen solely for ephemeral in-memory fixtures. No other new package version,
wire protocol or Windows/Android behavior change.

## Verification boundary

Pure tests cover closed config/command parsing, no pre-consent authority,
nonblocking framing/timers/secret-buffer error cleanup, bounded output and
server transient key cleanup. Hosted-only tests cover actual subprocess FIFO
bootstrap, paired TLS/catalog, nonce-bound local approval and finite offer,
wrong/preplayed approval, redacted malformed secret input and unchanged default
CLI; no reciprocal media offer is sent in these cases, so no PipeWire process is
opened. Additional actual TCP/TLS tests cover idle accept and trickled handshake
and binding deadlines. IPC cases are compiled, not run, in the restricted cloud
workspace and must pass exact-head hosted CI before acceptance. Real credentials,
persistent stores, devices and physical playback remain outside validation.
