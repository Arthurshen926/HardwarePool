# ADR 1489: Persistent desktop Node identity without invalidating existing pairs

Status: accepted for the integrated delivery work

Date: 2026-09-26

## Context

The Windows application and paired-control CLI bootstrapped from the same
deterministic testkit Node ID on every machine/account. Two new installations
could therefore advertise the same identity. Existing invitations and exact
TLS Node bindings already name that historical ID; changing it silently would
invalidate the owner's recently repaired pairing.

## Decision

- A host-only module loads `node-identity-v1.json` from the current process
  user's existing CapyIO pairing directory, located using the same Windows
  Known Folder API as `WindowsPairingStore`. The closed schema contains only
  version 1 and a canonical non-nil Node UUID, capped at 1 KiB.
- An existing valid identity always wins. Missing identity plus a valid existing
  `peers-v1.json` preserves the historical Windows ID, including empty or fully
  revoked directories. Residual credential entries also preserve it without
  reading their contents. Pairing records and credential bytes are untouched.
- A fresh installation with neither identity nor pairing history creates a
  random Node ID. A complete, synced same-directory temporary file is published
  using a hard link that cannot overwrite an existing destination. Concurrent
  first-launch losers read the winner's complete identity. Unsupported atomic
  publication, missing permissions, corrupt identity or unreadable/corrupt old
  metadata fail explicitly; there is no random per-process fallback.
- The production application, invitation creation/renewal, outgoing-control,
  inbound-control and resource diagnostic entrypoints share this initializer.
  UI demo reset retains the running Node's identity. Hardware-only broker/lab
  helpers that establish no paired Node are unaffected.
- Compatibility catalogs remain explicitly fixture-backed. A host constructor
  rebinds their local Node, bootstrap Sessions and four local Route endpoints
  before installing real Adapters. Testkit fixtures remain deterministic and
  the Core gains no filesystem, platform or cryptography dependency.

## Consequences

The current paired Windows account keeps `<LAB_ID_01>`;
new account installations stop colliding. Historical duplicates are deliberately
not migrated: their later migration needs an explicit re-pair transaction.
This is not a Windows hardware-catalog rewrite or new trust enrollment. The
identity file is ordinary account-local metadata, never a credential. Deleting
it is not an approved recovery workflow. Concurrent use with an older executable
that does not understand this identity record is unsupported.

## Validation

Hardware-free tests cover distinct new roots, stable restarts, eight concurrent
first launches, active/revoked/empty legacy directory preservation, credential
byte preservation, corrupt/oversized/unknown/nil identity rejection and write
failure without fallback. Bootstrap tests require every local Session/Route
endpoint and the installed touchpad catalog to use the selected Node ID while
the deterministic legacy fixture remains unchanged. No test opens an input
device, changes a driver, or migrates the owner's real pairing.
