# ADR 1485: Explicit renewal of an existing paired Node

- Status: Accepted for integrated-alpha manual recovery
- Date: 2026-09-19

## Context

Old one-role control certificates cannot support Android-initiated TLS. New
dual-role invitations are already issued, but enrollment rejects the same
Node ID as a duplicate. A silent replacement would break a working pair and
could let a process running under another Windows account overwrite public
metadata it can read but whose DPAPI credential it cannot decrypt.

## Decision

Windows Settings offers a separate, acknowledged renewal action for an exact
existing Node ID. The process must decrypt the current pair's DPAPI bundle,
and the target must have no active control link or inbound listener. Renewal
preserves unrelated peers, issues fresh dual-role identities, writes the
target-bound invitation before changing trust metadata, then stores the new
protected credential and atomically commits the new directory. If the new
credential or directory commit fails, it removes only the new invitation and
credential. Old credential files are retained, not deleted automatically.

Windows and Android will be temporarily disconnected after Windows commits
and before the user imports the new invitation in Android's existing system
file picker. The app does not auto-import, silently trust a new certificate,
or copy keys across Windows accounts. Reissuing a further explicit renewal is
the alpha recovery path if the invitation is lost; a seamless dual-trust
rotation protocol is not claimed.

## Verification

Directory-level tests require only the selected record to change and reject
renewal of an unknown peer. Desktop Cargo tests, shared UI typecheck and
standalone build cover the command and UI. Physical old-pair renewal remains
unperformed until the user runs the Windows app under the original DPAPI
account and completes the Android import.
