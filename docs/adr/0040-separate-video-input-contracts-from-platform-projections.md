# ADR 0040: Separate video and input contracts from platform projections

Status: accepted

## Context

Camera, gamepad, touchpad and a future unified Android Node need shared semantic
types before their feature worktrees diverge. Starting those worktrees from the
unfinished microphone branch would also couple them to unrelated Windows audio
driver and MicYou work. Conversely, putting Camera2, Media Foundation, HID,
DSU, VIIPER or a codec into Core would violate the existing portable-domain
boundary.

The initial Profile registry already reserves video and input identities, but
the repository had no authoritative types and the mock testkit used two stale
gamepad/haptics Profile names.

## Decision

Create two deterministic portable crates:

- `capyio-video` owns the decoded packed-raw video specification, exact
  negotiation, frame descriptors, metrics and minimal cross-platform camera
  capability metadata;
- `capyio-input` owns stream epoch/sequence guards, normalized coordinates,
  pointer, touch snapshot, semantic keyboard, fixed gamepad state and haptics
  feedback types.

Neither crate opens devices, sockets or processes, selects a platform API,
implements a codec, injects input or registers a system Projection. Core keeps
only canonical Profile ID helpers. Platform IDs and camera concurrency remain
Adapter inventory; IMU, gamepad, touch and haptics remain distinct Ports.

Video StandardPort v1 is intentionally limited to packed NV12/BGRA. Encoded
VCamdroid-compatible H.264/RTSP remains `AdapterManaged` until an explicit
codec/access-unit contract and Converter exist. Input Projection Adapters map
semantic keys/buttons/coordinates to DSU, VIIPER, HID or platform injection.

Reserve workspace ownership boundaries for VCamdroid, DSU, VIIPER, remote
touchpad, Windows camera/input and the Android host with compile-only marker
crates. These markers make no functionality claim.

## Consequences

- Camera/input worktrees can share canonical Profile names and avoid root
  Workspace/Cargo.lock conflicts.
- Exact candidate intersection cannot silently resize, rotate, decode, change
  color or rewrite QoS.
- Encoded video and platform-specific input remain isolated, so an
  `AdapterManaged` integration cannot be mistaken for universal StandardPort
  interoperability.
- The semantic surface is pre-alpha. Breaking changes still require a new
  Profile major or an explicit pre-release ADR/migration.
- Real Camera2, Media Foundation, network, HID/USB-IP, Android lifecycle and
  system registration remain separately scoped and tested platform work.

## References

- Microsoft `MFCreateVirtualCamera` documentation:
  <https://learn.microsoft.com/en-us/windows/win32/api/mfvirtualcamera/nf-mfvirtualcamera-mfcreatevirtualcamera>
- pinned VCamdroid reference:
  <https://github.com/darusc/VCamdroid/tree/f53d2f67691d468d89697cbc0e4178d3ed1082c4>
- pinned DSU protocol reference:
  <https://github.com/v1993/cemuhook-protocol/tree/82bf8a837cc7d2254e9257729f462a233d9ad184>
