# ADR 1406: Own the Android catalog in a disconnected native Runtime

Status: accepted

Date: 2026-09-16

## Context

ADR 1405 closes the versioned Java-to-Rust catalog data contract, but decoding
to a temporary `NodeDescriptor` does not establish Runtime ownership. Wiring
the unified Service directly to network/session operations at the same time
would combine catalog validation, native packaging, lifecycle and transport
trust in one high-risk change.

## Decision

`capyio-android-jni` provides `AndroidCatalogRuntimeV1`, which decodes one
schema-v1 Java catalog and constructs a real `NodeRuntime` whose local Node is
that catalog. Before any peer, Session or Route exists, the owner may replace
the catalog transactionally, but only with the same persisted Node identity.
Malformed input and identity changes leave the prior Runtime unchanged.

The Android JNI surface owns Runtime instances behind process-local positive
handles in a mutex-protected registry capped at four live owners. Java can create,
replace, inspect the capability count and close a handle. JNI entrypoints catch
Rust panics and translate failures into Java exceptions; no Core object layout
crosses JNI.

The Java wrapper is compile-checked but is not yet constructed by
`AudioNodeService`. The ARM64 library is build-verified separately with Cargo
NDK and is not yet part of the unified APK. Service lifecycle activation and
native library packaging require a subsequent explicit slice.

## Consequences

- Android now has a tested native Runtime ownership seam rather than only a
  decoder.
- Catalog changes cannot silently switch the persisted Node identity.
- Disconnected replacement intentionally cannot preserve future connected
  state; once Session support is exposed, Runtime needs a catalog reconciliation
  operation instead of reconstruction.
- This change opens no socket, admits no peer, authorizes no Route and installs
  no APK or driver.

## Dependency note

`capyio-android-jni` adds the internal workspace `capyio-runtime` crate to own
the validated local Node. It has the repository's Apache-2.0 license and is
maintained with Core and Protocol. No external package or Android runtime
library is added. Keeping only a `NodeDescriptor` in JNI was rejected because
it would create a second platform state owner; constructing transport or
Session state in Java was rejected because those policies belong to Runtime.
