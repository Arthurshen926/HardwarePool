# ADR 1454: Project local pairing state onto discovery candidates

- Status: Accepted observer metadata; connection action pending
- Date: 2026-09-18

## Context

The shared Remote resources page could observe bounded discovery candidates,
and the Windows host could recover a paired-peer directory, but the two views
were disconnected. Every observed node was displayed as generically unverified,
so an operator could not distinguish an unpaired node from an active paired
record or an explicitly revoked peer.

## Decision

The Windows control-plane owner classifies each still-fresh discovery candidate
against its in-memory paired directory at snapshot time. Its narrow DTO appends
exactly one closed `pairingState`: `unpaired`, `paired` or `revoked`. Expired
candidates remain absent. The UI parser rejects unknown states and extra fields,
and the Remote page explains that `paired` means only a local metadata match;
it is waiting for certificate and Session authentication and grants no hardware
access.

No display name, credential handle, certificate fingerprint, discovery nonce or
catalog is exposed. There is still no Pair or Connect action, and Android/Mock
continue returning no candidates until their discovery hosts exist.

## Consequences

- The unified UI can accurately separate first-use, known and revoked devices
  without treating unauthenticated broadcast content as authority.
- A future connection manager can select only a `paired` candidate while still
  repeating classification and credential resolution inside the trusted host.
- Enrollment, authenticated Session establishment, Android advertisement and
  application catalog exchange remain pending.
