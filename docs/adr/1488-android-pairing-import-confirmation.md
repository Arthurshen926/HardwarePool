# ADR 1488: Preview and confirm Android pairing invitation replacement

- Status: Accepted for integrated-alpha pairing recovery
- Date: 2026-09-26

## Context

Importing a target-bound CPI invitation previously replaced the entire Android
paired directory immediately after file selection. The same stable Node ID
can be accompanied by a different certificate generation, so a user could not
compare an existing pairing with the incoming invitation. Unrelated records
were dropped. A failed directory commit could also remove a credential that
was already in use when the same invitation handle was imported again.

## Decision

File selection only prepares a bounded in-memory candidate. Existing native
validation still checks the target Node, closed directory schema and typed
credential bundle. The native Android confirmation dialog shows the existing
and offered peer names and pairing timestamps, formatted with milliseconds
and an explicit UTC label. An invitation older than the stored pairing is
rejected before preview or writes, preventing accidental certificate rollback.
Same-generation re-import remains possible. Clock corrections that make a new
invitation appear older require correcting the issuing clock and issuing a new
invitation; this diagnostic guard does not replace certificate authentication.
The user must press the import/update button before any credential or directory
write. A timestamp is diagnostic metadata, not evidence of authenticated trust.

Cancel, dialog dismissal, Activity stop or destruction clears the candidate's
credential bytes. Candidate data is neither saved in Activity state nor sent
to WebView. The preview exposes no private key, credential handle or raw CPI.
After confirmation, the installer re-reads the directory and rejects if it
differs from the preview baseline. A candidate is consumed once, including on
failure, so a dismissed or previously committed plan cannot be replayed.

The import replaces only the matching peer record and preserves other records
unchanged. Because the current Android listener can only use one active peer,
an invitation that would add a second active peer is rejected before writing;
it never removes the existing peer to make room. Inactive records remain
retained. More than one offered peer in an invitation is rejected.

The directory is committed atomically after the new credential is stored. A
handle already referenced by the existing directory must recover the exact
same credential bytes; a different credential under that handle is rejected.
Re-import does not rewrite that credential. Directory-write rollback removes
only a credential newly written by this transaction, never an existing live
credential. Obsolete old-generation credentials are not deleted by this slice.

The explicit import may update pairing but grants no resource use. Successful
storage and successful listener reload are distinguished; failure to reload
reports that the pairing was saved and an application restart is needed.

## Verification and limits

`pairingImportContractTest` is hardware-free Java coverage for cancel/no write,
same-peer replacement and unrelated-record retention, rejection of a second
active peer, stale-preview rejection, one-use commit, existing-handle mismatch,
new-credential rollback and preservation of an existing credential on failed
re-import. Android Java compilation checks the native dialog, JSON projection
and storage integration. No test installs an APK or mutates a real pairing.

Native dialog appearance, cancellation, process/background handling and an
actual Keystore-backed import remain physical acceptance checks. Confirmation
is local; the other node can still retain a different invitation generation.
Automatic enrollment, confirmation acknowledgements between nodes and
multi-active-peer listener support are separate work.
