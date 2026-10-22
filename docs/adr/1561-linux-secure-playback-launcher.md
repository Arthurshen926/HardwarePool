# ADR 1561: Explicit finite secure mono playback launcher

## Status and scope

Implementation candidate, requiring isolated hosted Node/console and PipeWire
acceptance. This follows the reusable transport owners in ADR 1559 and Android
capture binding in ADR 1560. Physical phone recording and audible output on a
user's device are not claimed.

The first production Linux integration is remote mono Source to an explicitly
selected existing PipeWire playback target. Secure virtual microphone, reverse
stereo capture, virtual speaker, Windows secure playback and long sessions remain
outside this slice. Existing trusted-lab UDP modes remain separate and default.

## Explicit selection, not fallback

The bounded version-1 nonsecret playback manifest gains optional
`media_security`. Omission retains `unauthenticated_udp_trusted_lab_only`.
The only secure value is `pinned_tls13_mono_v1`, selecting the exact existing
`pcm-s16le-48000-mono-tls13-v1` format with empty parameters and Interactive QoS.
Unknown values, mixed formats and unsupported endpoint modes fail closed. There
is no opportunistic upgrade, UDP fallback or retry with a different profile.

Secure launching requires explicit paired-store selectors, a local target,
`endpoint_mode: playback`, `proposal_mode: linux_led`, at most 60 seconds and a
zero media port. Port zero is local allocation intent only: the existing owner
holds its actual listener and advertises the allocated nonzero port. It is not
an accepted peer endpoint or a reserve-close-rebind pattern. The old private
credential bootstrap cannot select this new secure path.

The Node's historical `linux-playback --trusted-lab` flag remains the explicit
experimental-operation gate. It does not choose the transport; the validated
manifest does. The console displays the exact secure profile, peer, direction,
target and finite duration instead of the plaintext UDP warning. The warning
remains mandatory for legacy audio.

Authenticated catalog preparation may explicitly select this same secure mode.
It filters actual current exact compatible Sources, requires fresh select/save
decisions, validates the resulting manifest and uses the existing create-new
file boundary. A secure-only Source cannot produce a legacy manifest. Export
creates no Route, listener, grant, lease or device and does not confer authority.
The snapshot can become stale immediately; the launcher must revalidate it.

## Authority, state and ownership

Pairing is authentication only. Existing fresh exact request/Route/peer local
confirmation and independent counterpart consent remain required. No helper or
data offer is created before local approval; the secure owner additionally
requires both original finite leases and the authenticated data binding ACK.
Identity comes from the exact protected paired-store record and accepted control
link. No secret or PCM is copied into the manifest, command arguments, environment,
ordinary status or diagnostic logs.

An additive `SecurePlaybackOwner::new_until` carries the original local consent
monotonic deadline through control negotiation. The earlier of that deadline,
the original grant and both original leases bounds the existing transport and
every helper write. No later handshake, status poll, packet timestamp, profile
change or renewal widens that authority. The reusable `new` entry remains
available with its previous finite admission contract.

The Route stays Starting. Authenticated receive/helper-write counters describe
actual transport/local write progress only, not audible playback or application
consumption. Partial writes are accounted truthfully. TLS readiness or queued PCM
must never manufacture Active state. Local Stop, input EOF, peer loss, malformed
media, revocation or exact expiry close the owned transport and helper; the host
continues supervising cleanup. Already buffered audio and arbitrary platform
scheduling are not an exact physical sample-stop guarantee.

Network data uses the existing pinned TLS 1.3 CPYA protocol and its independent
frame/rate/queue/stall limits, with no new wire format or cryptography. Plain PCM
exists at the authorized capture/playback boundaries, and endpoint/traffic timing
metadata remains visible. Existing Secret Service and filesystem trust limits
remain unchanged; loading paired credentials is not live monitoring of later
metadata edits or an automatic persistent resource grant.

## Dependencies and acceptance

The console now depends on the existing first-party `capyio-protocol` crate to
reuse the canonical secure format constant rather than duplicate it. No new
third-party production dependency, backend, permission or service is introduced.

Pure tests cover exact configuration/profile/format selection, unsupported modes,
fresh approval binding, closed status schemas and secure Starting-only semantics.
Mandatory hosted tests must use the actual terminal controller and fixed sibling
Node, an isolated fake Secret Service with synthetic paired identities, the
production Rust Source sender and an independent private PipeWire monitor.
Ordered tone/silence/tone is separate evidence from counters. Negative cases must
show no helper before consent and owned process/graph cleanup after cancellation,
peer loss, revocation and original lease expiry. Terminal observation precedes
reaping when checking expiry; cleanup delay is not expiry evidence. Closed typed
terminal causes distinguish observed control/media closure from timeout, without
printing raw errors. Source-revocation proof combines independent sender authority
rejection with receiver media closure while the lease is live; EOF alone does not
prove why the peer closed. At the exact expiry boundary an already queued media
closure/timeout may race the owner check. Acceptance requires first owner terminal
observation at or after the original lease, not a claim about worker error-generation
time. It never adds an early-deadline tolerance.

A separate actual terminal Ctrl-D case checks controller unwind, Source-side
authenticated control transport closure while its authority remains live, and independent process/graph
cleanup. The controller does not relay an owner terminal record while unwinding;
that case explicitly reports owner cause/time as unobserved and labels its last
counters as a pre-input-EOF status snapshot. It neither fabricates a terminal
observation nor claims whether best-effort Stop or subsequent pipe EOF reached
the owner first.

The retained nine reusable-owner graphs, legacy media suites and Android
SDK/ARM64/Java/JNI gates remain distinct. Synthetic PCM through the production
Rust sender does not execute a physical Android AudioRecord or establish phone
interoperability. Compilation or listing tests is not hosted wire/graph execution.
