# ADR 1451: Android atomic paired-directory persistence

- Status: Accepted persistence primitive; application admission pending
- Date: 2026-09-18

## Context

ADR 1450 protects Android credential bundles, but Android still lacked durable
non-secret trust metadata. Duplicating the paired-directory parser in Java
would risk accepting a different schema, capacity or Node relationship from the
Windows and portable Rust paths.

## Decision

Add `AndroidPairingDirectoryStore` below the unified application. It places
`peers-v1.json` in the app's no-backup pairing directory and uses `AtomicFile`
for replacement and interrupted-write recovery. An absent store returns the
canonical empty schema without creating a file. Reads stop at the shared
64-KiB bound.

Java does not interpret the trust document. A narrow byte-array JNI method in
`capyio-android-jni` parses the supplied local Node UUID, applies the existing
`capyio-pairing` schema-1 decoder, then returns canonical encoded JSON. Unknown
fields, invalid Node IDs, a local Node listed as its own peer, duplicate peers,
invalid credential handles, fingerprints and oversized inputs fail before file
replacement. The Java constructor also canonicalizes its fixed local Node UUID.

This store is not yet attached to `AudioNodeService`; Android pairing remains
reported as not configured. Enrollment must write every referenced Keystore
credential first and commit this directory last, then an application connection
manager may resolve handles. No WebView API, permission, network listener or
foreground lifecycle is added.

## Consequences

- Android and Windows now use the same Rust trust-document semantics while
  retaining platform-specific atomic storage.
- Native unit tests cover canonicalization and malformed input; the physical
  test gate covers absent read-only startup, commit/reload, rejected replacement,
  corrupt-file failure and test-directory cleanup.
- Enrollment, Service ownership, rotation/revocation UX, connection management
  and orphan cleanup remain separate acceptance gates.
