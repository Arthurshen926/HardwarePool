# ADR 1506: Fileless peer enrollment

- Status: Accepted for opt-in local Alpha implementation; delivery gates below remain open
- Date: 2026-09-30

## Problem

ADR 1469 offline CPI invitations carry the target private key. ADR 1485 renewal
commits the Windows record before Android imports its invitation. Generating a
new invitation alone therefore splits the pair generation. An unauthenticated
network copy of CPI would neither fix this lifecycle nor be an acceptable new
trust boundary. Existing paired control TLS must not accept unknown certificates.

## User flow (approved 2026-09-30)

Either Node explicitly opens a short-lived "Add device" window. The other selects
the discovered candidate, or enters its address on a network without multicast.
The welcoming Node displays a one-time numeric PIN; the initiating Node enters
it. Both show the requested peer and require local approval before trust is saved.
Initiator/welcomer are temporary handshake roles, not platform roles.

Once enrolled, reconnect uses the persisted paired identities without another
PIN. Hardware access remains governed by independent remembered resource consent.
Old CPI pairs continue to work without forced renewal. This initial protocol must
reject attempts to replace an existing Node's pin; recovery/rotation stays an
explicit separate action until a safe transition protocol is implemented.

## Security and implementation prerequisites

Use a reviewed PAKE implementation with explicit key confirmation, not a PIN hash
sent over the network, a self-designed key exchange, or a certificate-verification
bypass. RFC 9382 specifies SPAKE2 with key confirmation:
https://www.rfc-editor.org/rfc/rfc9382.html

The exact Rust implementation, revision, supported suite, license, maintenance
and alternatives require a dependency note before addition. Merely claiming
SPAKE2 support does not establish conformance to RFC 9382 or an audited product.

Each endpoint generates and retains its own private key in native storage.
Bind the authenticated transcript to protocol/version, both Node IDs, temporary
handshake roles, nonces and the exchanged identity certificates. Require key
possession evidence before committing public peer identities. UI sees only
bounded status, PIN entry/display and user-visible peer information; it never
receives credential bundles or private keys.

The pairing endpoint is separate from admitted control sessions. It exposes no
catalog, Routes or hardware commands. Only an explicit enrollment window enables
it; short deadlines and bounded online PIN attempts limit guessing. Cancellation
and expiry remove pending material but preserve existing pairs and settings.

Do not describe two machines' storage as an atomic transaction. Persist a pending
enrollment ID and exact transcript binding, exchange authenticated completion
receipts, and allow idempotent completion after disconnection. Until completion,
show "waiting for peer confirmation", not "connected". Never resolve a mismatched
transcript by overwriting an existing pin. Protocol details and recovery must be
specified before activating network enrollment.

## Compatibility boundaries

No driver, Android permission or media transport changes. This proposal does not
modify the existing accepted offline enrollment contract. Production code must
not expose a new unpaired listener until this decision and its dependency note
are accepted and the pending/commit/recovery sequence is implemented.

## Acceptance required before completion

1. Android initiates and Windows initiates using the same enrollment semantics.
2. No invitation file or private key transfer; both ends explicitly approve.
3. Restart reconnects with saved trust; previously paired devices still connect.
4. Wrong PIN, refusal, expiry and interruption leave no silently trusted peer.
5. Interrupting completion can finish the same transaction without reissuing keys.
6. Resource start still uses the existing per-resource authorization workflow.
7. Two-device physical validation over LAN/Tailscale, not ADB port forwarding.

## Implementation evidence and remaining gates

The shared `capyio-enrollment` crate implements an eight-digit, one-attempt,
three-minute window; PAKE transcript confirmation; exact-leaf pinned normal
mutual TLS; bilateral user approval; native local credential storage; and
authenticated saved receipts. Each endpoint creates its own key. CPI encoding is
reused only as an in-memory local installation DTO; no CPI travels over the wire.
Windows process and Android Service own completion, independently of the panel.

The selected implementation and limitations are recorded in
`1506-enrollment-dependencies.md`. Shared UI has both initiation choices and
local network address selection. Pairing discovery and control connection remain
separate: users still enter the welcoming endpoint address/port. Android retains
its one-active-pair limit and refuses first enrollment when already paired.

Loopback tests cover successful bilateral approval/storage and wrong PIN. Both
platform builds pass. This is not yet full physical acceptance.

**Original Alpha limitation (partially superseded by ADR 1507):** durable interrupted-commit recovery was not implemented. A
participant that already saved keeps its approved credential and reports a
missing receipt rather than silently issuing another identity. Both-saved pairs
can use ordinary control connection. If only one saved before a process/storage
failure, explicit recovery is still required; completion must not be claimed.
The opt-in Alpha entry is for controlled validation, not production activation.

ADR 1507 now adds durable native preparation, bilateral prepared receipts, and
explicit local recovery using the same protected keys. Recovery after both
preparations has physical evidence; an interruption before both preparations,
automatic network resumption and complete UI acceptance remain open.

## 2026-10-01 control handoff slice

Protocol version 2 / ALPN `capyio-enrollment/2` adds a post-receipt record carrying
only an optional nonzero control-listener port. Both native stores have committed
before this record is sent over the existing authenticated enrollment TLS stream.
The peer IP is the observed TCP peer address, not a remote-supplied third-party host.
Android publishes its actual bound port after service-owned listener reload;
failure publishes no port. A node without a listener publishes no port.
Version 1 first-enrollment peers must update together; established control pairs
are unaffected. There is no implicit trust upgrade or hardware authorization.

The shared UI fills this completed peer's control endpoint and, if automatic
connection is enabled and no connection action is busy, invokes the existing
paired control authentication/catalog exchange. The native runtime owns that
connection after initiation. This UI handoff does not claim headless automatic
dialing while the panel is closed. Routing cache is updated only after ordinary
control authentication succeeds. Interrupted enrollment recovery remains open.

### 2026-10-01 visible application handoff follow-up

The shared application's lifecycle poll now observes an in-progress enrollment
after leaving Settings. Completion is deferred while another connection action
is busy and consumed once per completed peer/endpoint in this application instance.
It respects explicit manual disconnect, the automatic-connect preference, and an
active inbound listener. Local recovery refreshes paired metadata without claiming
remote completion. Hidden/full-screen input surfaces defer this UI work.
This supersedes the Settings-panel-only restriction above, but is not headless
process-owned automatic dialing: the shared application must remain mounted.
