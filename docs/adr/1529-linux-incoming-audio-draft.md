# ADR 1529: Explicit one-shot incoming Android audio Draft admission

Status: implemented locally; independent review and exact-head hosted validation pending.
Date: 2026-10-20
Base: b5334d3e94b5104f2b79cb0f6629d3f29c96d523

## Bounded scope

The existing explicit trusted-lab playback launcher accepts optional
`proposal_mode: "incoming_draft"`; omission or `"linux_led"` retains ADR 1524's
Linux-led path. Unknown modes fail. This does not introduce a daemon, automatic
reconnect, credential discovery, hardware probing, a second peer, a second Sink,
reverse audio, or a virtual microphone. The configured exact remote Source,
paired certificate binding, finite lifetime and explicit PipeWire target remain
mandatory. Existing defaults and standalone playback CLI stay unchanged.

Only incoming mode advertises capability metadata
`dev.capyio.native-audio.inbound-draft = pcm-s16le-48000-mono-v1` on its single
random-ID Speaker Sink. This version identifies the implemented native mono PCM
admission sequence, not generic audio Profile interoperability, consent or media
security. Android recognizes that exact version plus the existing full endpoint
compatibility checks and uses the advertised PortRef. Unknown versions, invalid
ports, duplicate compatible endpoints and stereo/reverse directions fail closed.
The legacy Windows admission predicate remains a separate compatibility seam.

## Admission and independent authority

After fresh paired TLS and actual catalogs, incoming mode waits at most 30 seconds
for one Draft. Before import it checks the exact authenticated Ready Session,
configured Source, current local Sink, audio Profile, AdapterManaged backend,
Pending authorization, epoch zero, no selected format/QoS, the exact compatible
mono PCM/Interactive contract and absence of all media offers. No existing Route
may be present. It calls `apply_route_status(..., false)` only after those checks;
transport admission never auto-authorizes or acquires a lease.

The launcher then displays a fresh request nonce bound to that actual Route,
Session, endpoints, target, format and finite duration. Only the matching local
approval starts its finite clock. Exact duplicate Pending statuses while waiting
are idempotent and cannot create another approval or reset the deadline. Any
different binding, terminal/authorized peer status, preplayed approval, malformed
command, EOF or timeout stops the one-shot negotiation without media.

Android initiation is explicit local user intent, now implemented in a shared
`begin_audio_use` coordinator method also used by JNI and hosted tests. It creates
only a Pending Draft. Because that initiator waits for the receiver's finite
grant, Linux must send its independently approved Authorized Draft first. Android
then applies its existing local intent and bounded-consent policy. Linux waits
for the exact reciprocal finite grant before leases. A smaller peer expiry may
only narrow Linux's existing finite Draft grant and is acknowledged; expired,
unbounded or larger grants fail, and no peer message replaces local approval.

The existing one-second lease headroom, exact remote Source lease acquisition,
local Sink lease, prepare/start and authenticated playback owner remain in use.
No worker or media offer precedes both consent boundaries and leases. The owner
still opens the worker only after its exact reciprocal offer and sends Android
Start only after worker readiness. Status is not proof of sound. Local Stop is
polled throughout bounded phases; failures close and reconcile the exact Session.
Neither approval lifetime nor deadlines are extended by duplicates or polling.

## Protocol and evidence

The metadata uses existing bounded catalog maps; no protobuf field or wire enum
changes. Codec roundtrip tests ensure advertisement preserves permission and
availability fields. The shared selector checks exact contract version and
actual IDs without depending on Linux OS names. No permissions, credential store,
trust policy, service manifest, or UDP security is changed. Native UDP remains
unauthenticated and unencrypted, restricted to the existing explicit trusted-lab
contract.

New pure tests cover default/explicit mode, fresh Pending proposal predicates,
no mutation before approval, finite reciprocal grants, metadata version/Port
binding, and unsupported reverse directions. Hosted subprocess tests exercise the
actual Android initiator and coordinator, separate local approval, no early
leases/offers/capture, malformed/stale/replayed input, Stop and grant expiry.
Missing cached dependencies prevent the Linux/JNI integration suite from running
locally; exact-head hosted compile, lint and execution are required before a
software acceptance claim. Existing isolated PipeWire gate remains unchanged and
covers Linux-led playback only, not incoming-mode real audio. Physical Android
capture and actual user-device acceptance remain postponed and unclaimed.
