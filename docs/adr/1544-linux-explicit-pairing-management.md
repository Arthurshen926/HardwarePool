# ADR 1544: Explicit finite Linux pairing management

Status: experimental private-pipe CLI and mandatory isolated acceptance gate.
Builds on ADRs 1539–1543 and the existing cross-platform enrollment protocol.

## Scope and prerequisites

With `paired-store`, `capyio-node linux-pairing --trusted-lab` adds explicit
join, list and revoke actions. It is not a daemon, enrollment UI, discovery
service, invitation host, credential export or resource-approval mechanism.
The default Node command remains unchanged. This increment adds no cryptographic
or network protocol and no new external dependency family: the Linux host
optionally links the existing workspace `capyio-enrollment` manager, already
used by the store and other platforms.

The caller supplies an existing local NodeId, owner-only state directory,
explicit authenticated local Unix bus and existing unlocked persistent Secret
Service collection. Provisioning directories, collection permissions or a real
identity is not performed automatically. A peer must separately open its
existing enrollment window and provide its explicit endpoint and eight-digit
PIN through an approved private channel. Linux supports joining that window,
not displaying a newly hosted invitation or discovering nearby devices.

The existing manager generates a fresh local enrollment identity during the
attempt and retains its private key locally. Both peers must explicitly approve
before protected storage is staged or committed. This is real persistent trust
when operated by a person; development tests use only temporary synthetic peers
and an isolated fake service, never user credentials or devices. A NodeId is
not an existing certificate, proof of identity or resource permission.

## Explicit invocation and frames

```sh
capyio-node linux-pairing --trusted-lab \
  --pairing-state /absolute/owner-only/state \
  --pairing-node 00000000-0000-0000-0000-000000000001 \
  --pairing-bus-address unix:path=/absolute/owned/bus \
  --pairing-collection /org/freedesktop/secrets/collection/existing <&3 >&4
```

These are public selectors. Descriptors 3/4 must be exclusively owned inherited
FIFO pipes, not terminals or ordinary files. Use the existing four-byte
big-endian framing; management input is bounded to 4096 bytes and the whole
attempt to 180 seconds. PINs belong only in the private input frame, never
arguments, environment, stdout, stderr or logs. No raw manager status, discovery
candidate, endpoint, key bundle or backend error is returned.

The first version-1 action is one of:

- `join`, with `name`, `endpoint` and `pin` fields;
- `list`, with no additional fields;
- `revoke`, with exact `peer_node_id`.

Unknown, duplicate or action-inappropriate fields fail closed. Join uses the
existing PIN-authenticated protocol and presents the authenticated peer identity
for a fresh human decision. Its `approve` command must echo the fresh
`request_id` and exact `peer_node_id`. Revoke separately requires
`confirm_revoke` with the fresh request and exact peer. These are decisions on
one attempt, not reusable consent. Preplayed approval, mismatched request/peer,
replayed commands, EOF, cancellation and timeout do not authorize trust.
The private-pipe launcher must actually obtain the person's decision; possession
of a pipe or receipt of a confirmation event is not proof of human consent.

## Persistence and failure semantics

Join checks metadata and backend prerequisites before opening enrollment
networking. Existing peers, including revoked tombstones, are passed to the
manager's duplicate guard; enrollment cannot silently replace their pins.
A pending or corrupt recovery journal blocks a new join. The management command
never auto-recovers, deletes credentials or recreates a missing committed key.
List returns bounded public peer metadata and pending-state information only.
List and revoke use a metadata-only backend: they do not contact the bus or read
protected credentials and remain available with a locked or missing keyring.
The four public CLI selectors remain required, but bus/collection are unused
for these two actions.
Revoke changes the selected local tombstone and retains its protected credential;
it is not remote propagation or termination of an already running session.

The existing bilateral prepare/save receipts and Linux stage/commit order are
preserved. After local persistence the manager publishes an explicit null
control-port handoff: no control listener or resource service is started.
`locally_paired` reports local persistence without remote completion; `paired`
requires the existing bilateral receipt/handoff to finish. Both explicitly report
`control_endpoint_available:false`. Failure/cancellation report `local_saved` and
`pending_enrollment` from bounded metadata inspection; null means unknown, not
absent. List reports `peers` with public ID/name/active-or-revoked state only.

Bilateral completion is distinct from local persistence. A later failure or
missing remote receipt must not claim rollback, regenerate an identity or
silently retry. The pending journal and/or committed metadata remain for the
existing recovery boundary; ADR 1545 adds a separately confirmed local recovery command without claiming remote completion.

Backend calls retain a one-second timeout and no prompt, unlock, activation,
collection creation, ambient bus lookup or plaintext-file fallback. Metadata
filesystem calls are not hard real-time. The overall deadline bounds polling
and private-pipe operations, not OS scheduling or filesystem stalls. Existing
plain local D-Bus and service-acknowledgement durability limits remain.

## Acceptance limits

Tests invoke the actual CLI with temporary state, a private owned D-Bus service
and the real enrollment manager on a synthetic loopback peer. They require
bilateral approval, protected storage/readback, redacted output, exact list and
explicit revoke behavior, plus malformed/preplayed/mismatched confirmation and
backend/cancellation failures. No local resource grant or media activation is
introduced or inferred from pairing. Hosted execution is mandatory when local
socket creation is restricted; compilation is not protocol acceptance.

This does not prove compatibility with every keyring vendor, physical devices,
power-loss durability, a pairing UI, real-device onboarding, standalone Linux
invitation hosting, automatic recovery, long-lived readiness or lease renewal.
