# ADR 1442: Windows user-scoped credential protection

- Status: Accepted protection boundary; storage composed by ADR 1449
- Date: 2026-09-18

## Context

The paired-peer document can retain only an opaque handle. Windows still needs
a platform boundary capable of protecting the certificate and private-key
bundle referenced by that handle without putting plaintext secrets in ordinary
configuration or requiring the GUI to implement cryptography.

## Decision

Add `capyio-windows-credential-protection`, a bounded wrapper over Windows
DPAPI `CryptProtectData` and `CryptUnprotectData`. Protection is scoped to the
current Windows user, uses a fixed CapyIO context as optional entropy and sets
`CRYPTPROTECT_UI_FORBIDDEN` so background use cannot display an unexpected
system prompt.

Plaintext bundles are non-empty and at most 64 KiB; protected blobs are at most
68 KiB. DPAPI errors are mapped to non-sensitive typed errors. The DPAPI-owned
plaintext output allocation is overwritten before `LocalFree`. Returned
plaintext bytes remain caller-owned and must be retained only for the bounded
TLS configuration operation.

The component performs no filesystem I/O, chooses no credential handle, enrolls
no peer and never logs bytes. It does not use machine scope, so copying a blob
to another account or host is not a supported recovery mechanism.

## Consequences

- Windows has a tested OS-backed confidentiality and integrity boundary for a
  future paired credential bundle.
- ADR 1449 supplies fixed current-user placement and atomic replacement. Custom
  ACL hardening, deletion, rotation, backup/recovery and pairing UX remain
  application-owned gates.
- Android requires its own Keystore-backed implementation behind the same
  conceptual credential-handle boundary.
