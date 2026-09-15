# ADR 1404: Derive stable Android Core catalog identities

Status: accepted

Date: 2026-09-16

## Context

The Android Node owns one persisted UUID, but its service catalog currently
uses bounded human-readable platform keys such as `android.imu`. Core and the
public CatalogSnapshot protocol require typed UUIDs for Adapter instances,
Capabilities and Ports. Generating fresh IDs for each snapshot would break
catalog replacement and Route identity; treating display names or untyped keys
as wire IDs would bypass the Core identity model.

## Decision

Use the persisted Node UUID as an RFC 4122 UUIDv5 namespace. Derive one current
Adapter identity per Android Capability key with `adapter/<capability-key>`, the
Capability identity with `capability/<capability-key>`, and each Port identity
with `port/<capability-key>/<port-key>`. Keys retain the Android contract's
96-byte canonical lowercase ASCII grammar. Java and Rust implementations share
committed fixed vectors and domain-separation tests.

This ADR defines identity only. A later catalog projection must still explicitly
map Adapter lifecycle/ownership, QoS, interoperability mode, platform/runtime
versions and permission state before producing a valid Core NodeDescriptor.

## Consequences

- Repeated snapshots for the same Node and platform key retain typed identity.
- Adapter, Capability and Port domains cannot collide merely because their
  platform keys match; a different Node produces different descendants.
- Renaming a platform key is an identity migration, while changing a display
  name is not.
- UUIDv5 SHA-1 is used only for deterministic name derivation. These IDs are
  public identifiers, not secrets, credentials, signatures or peer
  authentication evidence.

## Dependency note

- Purpose: implement the RFC 4122 UUIDv5 digest used by Java and Rust fixed
  vectors.
- Dependency: the already locked `sha1` 0.11 crate is added to
  `capyio-android-host`; `uuid` remains the existing workspace UUID type without
  enabling its additional v5 dependency.
- License/maintenance: RustCrypto `sha1` is dual MIT/Apache-2.0 and already
  present in the workspace lock graph; it is maintained with the RustCrypto
  digest ecosystem.
- Alternatives: random persisted child IDs require a new migration/storage
  surface; ad-hoc hashes are non-standard; enabling `uuid/v5` introduced an
  otherwise unused `sha1_smol` package; using platform keys as UUIDs is invalid.
