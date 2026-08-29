# CAPY-IO-CONTRACTS-001 Report

Date: 2026-08-29

Status: implementation and available validation complete; local baseline commit
awaits explicit human approval.

Base: `main` at `f89e173`

Worktree: `target/worktrees/capyio-contracts`

Branch: `codex/capyio-contracts`

## Outcome

The slice establishes portable, platform-independent Video and Input contracts
without adding a real camera, input device, network protocol, codec, driver or
Android application implementation. The pre-existing microphone worktree was
not edited by this slice.

The baseline adds:

- `capyio-video`, with raw packed NV12/BGRA8 stream negotiation, bounded frame
  descriptors, camera metadata and observability-aware metrics;
- `capyio-input`, with pointer, touch, keyboard, gamepad and haptics semantics,
  bounded validation, epoch/sequence tracking and fail-safe reset/neutral/stop
  states;
- canonical Profile helpers in `capyio-core` and aligned fixtures in
  `capyio-testkit`;
- non-functional workspace boundaries for camera, DSU, VIIPER, remote
  touchpad, Windows input/camera and Android host work;
- normative Video/Input profile documentation, ADR 0040 and active plan 0014;
- reference-only third-party records for VCamdroid and DSU protocol research.

## Contract boundaries

- Standard Video Ports carry only negotiated raw frames. Codec, RTP/RTSP,
  resize, rotation and color-conversion behavior remain explicit Adapter-owned
  concerns.
- Core camera metadata is limited to cross-platform identity, orientation,
  facing, stream and control semantics. Camera2-specific identifiers and
  concurrency claims remain Adapter DTOs.
- Touch samples are complete active-contact snapshots. Keyboard samples use
  CapyIO semantic keys rather than USB HID or platform scan codes.
- Gamepad state excludes battery, IMU, touch and haptics. Those use separate
  profiles or control paths.
- Frame descriptors carry metadata and payload length, not media bytes. A
  future data-plane implementation must supply the bounded payload transport.

## Validation evidence

The following commands completed successfully in the contracts worktree:

```text
cargo test -p capyio-video -p capyio-input -p capyio-core -p capyio-testkit
cargo check --workspace
cargo xtask validate-docs
cargo xtask validate-manifests
cargo xtask ci
git diff --check
```

`cargo xtask ci` covered formatting, workspace check, Clippy with warnings
denied, Rust tests, the deterministic IMU fixture demo, documentation and
repository validation, Adapter crash isolation/smoke, desktop TypeScript
checking and the desktop production build.

`cargo xtask doctor` found all required bootstrap tools. Optional tools absent
on this host were `adb`, `java`, `msbuild` and `windbg`; none is required for
this portable contract slice.

Hardware-dependent tests remained ignored as designed. No driver or APK was
installed, no virtual camera was registered, no Android permission was changed
and no external source or binary was imported.

## Third-party evidence

- VCamdroid: `https://github.com/VCam-droid/VCamDroid`, pinned reference
  `f53d2f67691d468d89697cbc0e4178d3ed1082c4`, MIT, no imported paths.
- DSU reference: `https://github.com/v1993/cemuhook-protocol`, pinned reference
  `82bf8a837cc7d2254e9257729f462a233d9ad184`, Unlicense, protocol version 1001,
  no imported paths.

## Unresolved risks and next gates

- The generated camera fixture and bounded payload queue are not part of this
  baseline; they are the first `codex/capyio-camera` slice.
- Windows Media Foundation projection requires a dedicated platform ADR plus
  SDK/tool availability. Registration, removal and lifecycle rollback must be
  reviewed before any system mutation.
- Android camera inventory and capture remain blocked on an approved device,
  command path and privacy/retention boundary.
- DSU, VIIPER and remote-touchpad directories are reservation markers only and
  do not claim interoperability.
- Downstream worktrees must be created from one approved baseline commit so
  they cannot silently diverge from these contracts.

## Approval boundary

No commit, push, pull request, driver deployment, APK installation or system
registration was performed. After explicit approval for a local commit, the
same commit SHA can seed the camera, gamepad, Android-node and touchpad
worktrees. Push and pull-request operations remain separately prohibited.
