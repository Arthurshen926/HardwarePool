# ADR 1552: Read-only authenticated Linux catalog inspection

Status: experimental; isolated peer, protected-store and terminal acceptance required.

## Scope

The paired-store Node adds `linux-catalog --trusted-lab` with the existing explicit
local Node, peer, state directory, Unix bus and collection selectors. Its private
FIFO bootstrap is a strict public config followed by the existing paired-store
marker. The console adds `catalog --peer NODE_UUID --endpoint IPv4:PORT`, using
its trusted fixed sibling and inherited anonymous pipes. No user-selected
executable, default bus, discovery, credentials in arguments or identity override
is introduced. The server name and exact certificate pin come from the selected
committed active paired record.

This closes the missing authenticated Port-ID inspection step, not automatic
media setup. Manifest generation and selection remain a later increment.
Pairing/listing a peer does not imply that any particular resource is published,
available locally, approved or usable. Inspection requires an explicit existing
peer endpoint and a peer implementation which exchanges catalogs on this control
connection. It does not start a listener, enrollment or reconnect loop.

## Existing protocol, no resource authority

One empty local Node catalog is sent first, as the existing dialer protocol
requires. Exactly one authenticated remote catalog is admitted through the
existing control-link/host validation. Unexpected messages fail rather than
being dispatched. The connection and ephemeral control Session are closed and
reconciled after inspection. The local Node has no media capabilities or Ports.
No Route, proposal, grant, lease, media endpoint, helper or device is constructed.
No persistent pairing mutation, recovery, credential deletion or backend unlock
is performed.

The new opt-in TLS dial deadline API shares the existing socket-deadline shutdown
and joined watcher mechanism used by acceptance. Existing dial APIs retain their
behavior. Catalog inspection uses one absolute operation deadline across the
finite exchange, including TLS and binding trickle protection; it never retries
or renews. Protected-backend calls retain their bounded per-operation contract,
and ordinary filesystem latency cannot be promised a hard wall-clock bound.
The console has a bounded cancellation/child-reap envelope; cancellation after
network I/O begins is not claimed to erase an already sent empty catalog.

## Bounded public projection

The protocol's existing 1 MiB envelope and descriptor-count bounds remain
enforced before a smaller application projection. The following are accepted
projection limits, not lower raw control-wire allocation limits: at most 64 Ports, 4096 bytes per event and
65536 total framed output bytes. Oversize projection fails, never silently
truncates a supposedly complete snapshot. All events are prepared before the
first result is emitted. The console additionally validates exact peer, event
order, count and unique capability/Port identity and waits for successful child
exit before displaying the complete result.

Output contains actual Node, Capability and Port IDs, direction, profile,
formats, availability and narrowly labeled catalog-only launcher hints. Arbitrary
display names, schema and metadata are omitted. Included text is bounded and
ASCII-escaped, including terminal controls and non-ASCII text. No certificate,
private credential, endpoint PIN or raw error appears. Hints say either
`needs_local_selection` or `not_supported`; neither is a generic compatibility,
readiness, counterpart-consent or resource-authorization claim. No synthetic
local endpoint or Route is created just to calculate a hint.

## Acceptance and remaining prerequisites

Mandatory hosted tests use synthetic identities, an owned private fake Secret
Service and loopback peer. Actual Node and private-PTY console cases must prove
an empty local catalog followed by connection closure, with no resource control
messages, helper or device activation. Wrong pin/Node, unavailable trust/backend,
malformed/oversize/unexpected catalog traffic and incomplete results fail closed.
Pure tests validate schema, bounds, escaping, event binding and finite deadline
behavior. Existing media, pairing, staging and platform gates remain required.

Real user identity provisioning, an existing unlocked Secret Service collection,
peer publication configuration and physical hardware acceptance remain separate.
The interactive pairing console exists; these prerequisites do not make this an
install-and-use package. Camera's existing independent 120-frame budget at 30 fps
is approximately four seconds of continuous video, regardless of the separate
60-second upper time ceiling. This increment changes neither camera budgets nor
any media transport, consent, lease or renewal policy.
