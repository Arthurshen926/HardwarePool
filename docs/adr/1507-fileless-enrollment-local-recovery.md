# ADR 1507: Recover approved, durably prepared enrollment

- Status: Accepted for the existing controlled Alpha
- Date: 2026-10-01

## Decision

Enrollment v3 adds a native durable preparation before either peer commits its
paired directory. After both user approvals, each host stores its own credential
in the existing DPAPI/Android Keystore store and atomically records the exact
single-peer directory in a node-identity-scoped pending journal. No new private
key format, permission, dependency, driver or network trust bypass is introduced.
Only public peer metadata and an opaque credential handle are in the journal.

The authenticated enrollment channel exchanges `prepared/1` only after native
preparation succeeds. Both preparations must precede normal directory commit.
The existing saved receipt and actual-control-port handoff then follow.
All first-enrollment participants must update together; existing control pairs
remain compatible.

After a process interruption, the settings panel offers explicit recovery of
the already-approved local pending record. It reloads the same protected key and
replays directory commit idempotently. An existing identical record is accepted;
any different record for that node or conflicting Android active peer is rejected.
Recovery never rotates a certificate or replays hardware permission grants.
Successful local commit clears only the pending metadata, not the live key.
UI says local recovery, not remote completion or connected.

## Limits

This is not a distributed atomic transaction or automatic network-resume protocol.
If only one peer prepared before interruption, the other may have lost its
ephemeral identity; local recovery alone cannot recreate that remote private key.
Once both peers prepared, both can recover the original identities locally and
connect normally. Automatic rediscovery of control endpoints after restart and
completion-receipt replay remain separate work.

## Checks

Require no commit before both preparations, stable prepared credentials across
restart, idempotent repeated recovery, and rejection rather than replacement of
a different stored peer. Physical tests use independent lab identities only.
