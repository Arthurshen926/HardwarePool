# ADR 1405: Project an explicit Android catalog DTO into Core

Status: accepted

Date: 2026-09-16

## Context

ADR 1404 defines stable typed identities for Android catalog descendants, but
identity alone is insufficient for a Core `NodeDescriptor`. Core also requires
Adapter ownership and lifecycle, deployment mode, supported Route backends,
Capability permission/availability, Port direction/Profile/formats/QoS/clock
domain and interoperability mode. Inferring those fields inside JNI or a
network worker would make platform behavior implicit and difficult to test.

## Decision

`capyio-android-host` owns a versioned, platform-specific catalog DTO and a pure
projection into Core. Its enums are closed Android boundary values rather than
Rust memory layouts. The DTO explicitly supplies QoS, interoperability and
supported Route modes; the projector never guesses them from a display name.

The current identity contract creates one in-process Adapter descriptor per
Android Capability key. Attached, non-Offline capabilities map to Ready/Healthy;
attached Offline capabilities map to Degraded/Unhealthy; unattached capabilities
must remain Offline and map to Stopped/Unknown. Core owns the final validation
and Adapter-Capability ownership reconstruction.

The projector opens no socket, performs no JNI call and does not select an
authenticated transport. Internal `capyio-control-host` and `capyio-protocol`
dependencies are test-only and prove that a projected catalog can pass the
existing publication filter and Session-bound codec.

The unified Java Service now serializes its already filtered publication
catalog into that boundary as schema-v1 JSON. The top-level and nested objects
reject unknown fields on the Rust side, the complete UTF-8 message is limited
to 64 KiB, and a schema version mismatch fails before Core mutation. The Java
policy table is keyed by canonical Capability key, never display text. Existing
private audio, camera and touchpad paths declare `adapter_managed`; IMU,
gamepad and haptics declare only `local_pipeline` until a remote path exists.
The Binder returns this local handoff value but still performs no delivery.

## Consequences

- Android platform code must make Route/QoS/interoperability claims explicitly
  before a capability can enter the Core catalog.
- Malformed profiles, duplicate identities, empty/unbounded collections and
  contradictory attachment/availability state fail before publication.
- Java does not construct Core or Protobuf objects on the Android framework
  side; Rust alone validates and projects the narrow JSON DTO.
- The JSON handoff is now implemented and compile-tested, but invoking it from
  JNI/native Runtime ownership and authenticated device-to-device delivery
  remain separate work.

## Dependency note

`capyio-android-host` uses the existing workspace-pinned `serde` and
`serde_json` crates to decode the closed handoff. They are maintained broadly,
licensed MIT/Apache-2.0, perform no I/O, and add no Android runtime component or
network behavior. Direct JNI field-by-field construction was rejected because
it couples Java object layouts to Rust calls; using the network Protobuf
catalog inside Java was rejected because transport/session concerns do not
belong in this local platform boundary.
