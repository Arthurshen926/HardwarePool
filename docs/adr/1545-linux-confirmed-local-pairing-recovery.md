# ADR 1545: Confirmed local pairing recovery and optional staging profile

Status: experimental finite local repair and mandatory isolated acceptance gate.
Builds on ADRs 1539–1544; no remote-completion or production-release claim.

## Local recovery is not another enrollment

A durable pending journal means that this node staged a candidate after the
existing bilateral approval exchange. It does not prove the peer finished saving
or that a control service is available. The explicit `recover` action replays
only that local record with its original protected credential. It never joins a
peer, generates an identity, silently renews consent, changes a pin, creates a
keyring item, deletes a protected item or grants a resource.

Recovery requires a fresh human decision on the exact local transaction. The
private-pipe launcher must obtain that decision; echoing a challenge is not
itself proof that a person approved. A compromised same-user launcher already
controls this trust boundary, as in ADR 1544.

## Metadata inspection and exact confirmation

The `linux-pairing` invocation and four public selectors remain ADR 1544's.
All frames retain the four-byte big-endian length prefix and 4096-byte input
limit. `list` now adds `pending_peer` (null when absent), containing only
`peer_node_id`, `display_name` and `transaction_digest`. The existing
`pending_enrollment` boolean remains. This query does not contact D-Bus.

`transaction_digest` is lowercase SHA-256 of a domain separator, the canonical
local owner ID and the exact bounded validated nonsecret journal bytes. It is
not a private-key digest exported from protected storage. Changed bytes,
including formatting, require a new decision. Invalid/ambiguous metadata,
wrong ownership, revoked targets, conflicting existing records and capacity
failures are rejected before credential access.

Send `{"version":1,"action":"recover"}` to inspect and request repair. The
`recovery_confirmation_required` event contains a fresh `request_id`, exact
peer identity and transaction digest. After deliberate inspection, send:

```json
{"version":1,"action":"confirm_recover","request_id":"<fresh request UUID>","peer_node_id":"<exact pending peer UUID>","transaction_digest":"<exact 64 lowercase hexadecimal characters>"}
```

Missing, mismatched, stale, preplayed or replayed confirmation is rejected.
Before confirmation is accepted, `{"version":1,"action":"cancel"}`, EOF or
expiry performs no repair. After acceptance, synchronous backend/commit work
does not promise to observe a newly arriving cancellation or EOF before writing.
Accepted local replay is not a cancellable transaction and cannot promise
rollback once commit begins; filesystem scheduling can also outlast the checked
deadline. The pending journal is not cleared just because inspection or
confirmation failed.

The lifetime store lock remains held across inspection, confirmation, protected
backend initialization and commit. The implementation does not drop and reopen
the store to switch from metadata-only inspection to protected loading. The
journal is revalidated before any backend access, then the core confirmed API
validates the peer/digest again and commits that same validated snapshot. It
checks journal bytes again after protected loading and before removal. The flock
serializes cooperating callers; it cannot prevent malicious same-UID writers
from racing between checks and filesystem operations. This is not protection
against a compromised account or general transactional filesystem isolation.

## Fail-closed persistence and honest status

Only after exact confirmation does recovery connect to the explicit native
Secret Service. Missing/locked service, missing/changed credential, corrupt
journal, revoked peer or conflicting pin fails without generating replacement
identity or deleting protected storage. The existing digest, bundle validation,
metadata fsync/rename order and local manager recovery notification are reused.
There is no remote network handshake or hidden enrollment retry.

Success emits `locally_recovered`, `local_saved:true`,
`remote_completion_confirmed:false`, `control_endpoint_available:false`.
It is not `paired` or a claim of remote readiness. An exact stale journal left
after directory commit is repairable idempotently, but an absent journal cannot
be confirmed again. A late filesystem failure may leave committed metadata;
error reporting inspects local saved/pending state where possible, with null
meaning unknown. It never promises rollback or instructs silent regeneration.
The person may subsequently attempt a normal finite connection using the
original identity; that requires the existing independent approval and leases.

The fixed 180-second management deadline and one-second protected-operation
budgets remain. No bus discovery, prompt, unlock, collection creation or file
fallback is added. Filesystem latency is not a hard real-time bound. Recovery
is local-only, including when the peer never saved or is offline.

## Explicit staged build choice

The default staging build still omits `paired-store`. Use
`python3 scripts/linux_staging.py build --paired-store --output /absolute/new/stage`
to explicitly add it to the Node binary while retaining native camera/touchpad
features. The four-binary inventory and existing native requirements remain.
Manifest schema 1 admits exactly the default and paired feature maps, not
arbitrary helper or test-fixture features. Passive preflight reports the selected
map without executing the staged binaries. The unsigned manifest is inventory,
not authenticity, authorization or permission to enroll devices.

Hosted staging checks build both profiles, inspect the same bounded inventory
and check command availability using help only. Neither profile creates a bus,
accesses a keyring, provisions permissions or enrolls a peer during staging.
See LINUX_BUILD_AND_OPERATIONS.md for exact commands and prerequisites.

## Verification boundary

Core fault tests cover exact-byte confirmation, conflicts, unavailable or changed
protected entries, stale committed journals and replacement during a protected
read. Actual Node subprocess tests use synthetic records and a private owned
fake Secret Service to exercise inspection, confirmation, rejection, local replay
and unchanged protected bytes/counts. Existing enrollment tests remain mandatory.
No management IPC runs on a restricted local executor; hosted execution is
required in addition to compilation. Passing does not establish physical-device
behavior, real keyring vendor compatibility, power-loss durability, remote
completion, production installation or long-lived service readiness.
