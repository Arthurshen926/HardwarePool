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

## Consequences

- Android platform code must make Route/QoS/interoperability claims explicitly
  before a capability can enter the Core catalog.
- Malformed profiles, duplicate identities, empty/unbounded collections and
  contradictory attachment/availability state fail before publication.
- A later Java/JNI bridge can copy this narrow DTO without constructing Core or
  Protobuf objects on the Android framework side.
- The current Java service catalog still needs a versioned bridge that fills
  this DTO; this ADR does not claim device-to-device catalog delivery.
