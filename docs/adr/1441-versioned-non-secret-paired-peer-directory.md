# ADR 1441: Versioned non-secret paired-peer directory

- Status: Accepted persistence contract; platform storage pending
- Date: 2026-09-18

## Context

The bounded paired-peer directory existed only in memory. A host needs a stable
restart format before it can connect discovery candidates to credentials held by
an operating-system security store. Ordinary configuration must never contain
private keys, trust roots or one-time pairing secrets.

## Decision

`capyio-pairing` encodes and decodes a deterministic schema-1 JSON document of
at most 64 KiB and 64 peers. Each peer contains the bounded Node ID, display
name, TLS server name, lowercase SHA-256 peer-certificate fingerprint, pairing
time, active/revoked state and opaque credential-store handle already defined by
the pairing model.

Unknown fields, malformed JSON, unsupported schema, duplicate or local Node IDs,
invalid handles/names/fingerprints and revocation before pairing fail closed.
Decoding reconstructs records through the same validated constructors used by
in-memory enrollment. Encoding remains deterministic through Node-ID ordering.

The document contains no certificate DER, trust root, private key or pairing
code. This slice defines bytes only: it does not choose a filesystem location,
write a file, access an OS credential manager or implement enrollment.

## Consequences

- Windows and Android can persist identical non-secret trust metadata while
  keeping cryptographic material in their platform security stores.
- A copied directory is not sufficient to authenticate because its credential
  handles are opaque and the later exact mTLS binding remains mandatory.
- Platform owners still need atomic storage, corruption recovery, credential
  lifecycle and a user-visible pairing ceremony.
