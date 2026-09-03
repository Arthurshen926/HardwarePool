# CAPY-GAMEPAD-002A Report

Date: 2026-08-29

Status: implementation and automated validation complete

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

Worktree: `target/worktrees/capyio-gamepad`

Branch: `codex/capyio-gamepad`

## Objective

Project the existing normalized `GamepadControls` contract and an already
validated `DsuMotionSample` into one deterministic DSU v1001 pad-data packet.
This supplies the protocol boundary needed by a later Android touch-control UI
and dual-Route worker without prematurely adding either lifecycle.

## Boundary and mapping

- the packet remains fixed at 100 bytes and retains the existing header,
  packet number, motion timestamp, acceleration, gyroscope and CRC behavior;
- all 12 DSU-representable semantic buttons are projected to their digital
  fields and, where present, their 0/255 analog fields;
- D-pad digital and analog fields are generated from the same validated
  complete state;
- signed sticks use a deterministic piecewise mapping where
  `-32767 / 0 / 32767` become `0 / 128 / 255` exactly;
- unsigned triggers use rounded `0..=65535` to `0..=255` scaling; any non-zero
  source value sets the corresponding digital trigger bit even when value `1`
  rounds to analog zero;
- `Paddle1..4` fail with `UnsupportedGamepadButtons` because DSU v1001 has no
  corresponding fields;
- touch-contact bytes remain zero. Raw contacts still belong to the separate
  `capyio.input.touch-events/1` Profile;
- invalid controls are rejected before subscriber iteration, so acceptance
  does not depend on whether an emulator is currently subscribed.

`DsuControlsMapping` makes two otherwise ambiguous policies explicit:

1. CapyIO v1 specifies signed-axis ranges but not a controller source's
   positive direction, while DSU requires X rightward and Y upward. Each D-pad
   and stick axis therefore has a caller-selected sign.
2. The pinned protocol reference names the four face fields Y/B/A/X, while
   Dolphin's current consumer structure names the same analog bytes
   Square/Cross/Circle/Triangle. Callers select `ProtocolNamed` or
   `DualShockPhysical`; this slice does not claim one layout interoperates with
   every client.

References used for the field boundary only; no upstream source was imported:

- pinned DSU reference:
  <https://github.com/v1993/cemuhook-protocol/tree/82bf8a837cc7d2254e9257729f462a233d9ad184>
- Dolphin consumer structure:
  <https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/InputCommon/ControllerInterface/DualShockUDPClient/DualShockUDPProto.h>

## Files

- `adapters/dsu/src/protocol.rs`
- `adapters/dsu/src/transport.rs`
- `adapters/dsu/src/lib.rs`
- `adapters/dsu/tests/controls_projection.rs`
- `adapters/dsu/tests/udp_loopback.rs`
- `adapters/dsu/README.md`
- `docs/CAPY_GAMEPAD_002A_REPORT.md`

## Automated evidence

`cargo test --locked -p capyio-dsu-adapter` passed 35 tests. The new evidence
covers:

- every supported button isolated to the expected digital/analog fields;
- deterministic and distinct protocol-named and DualShock-physical layouts;
- all nine D-pad combinations;
- minimum, neutral and maximum for every stick axis plus explicit Y inversion;
- trigger values `0`, `1`, `32767`, `32768` and `65535`, including the digital
  non-zero threshold;
- neutral-encoder byte compatibility, unsupported paddles, invalid controls,
  untouched touch/motion bytes and CRC;
- combined controls and motion crossing the loopback UDP endpoint;
- zero-subscriber invalid controls failing closed.

Additional passing commands:

- `cargo check --locked -p capyio-dsu-adapter --all-targets`
- `cargo clippy --locked -p capyio-dsu-adapter --all-targets -- -D warnings`
- `cargo xtask validate-docs`
- `cargo xtask validate-manifests`
- `cargo xtask ci`
- `git diff --check`

The full CI run passed on its first attempt, including workspace checks/tests,
Adapter smoke/structural validation and desktop typecheck/build.

## Remaining risks

- The current IMU worker still submits neutral controls. A following dual-input
  slice must independently guard the `GamepadState` stream/epoch/sequence and
  publish neutral on gap, overflow, stop, failure and peer loss.
- There is no Android touch-control UI, Runtime Gamepad Route or live phone
  input in this slice.
- Cemu/Dolphin face-button behavior has not been exercised. The layout must be
  selected and retained by an integration owner based on target evidence.
- DSU touch contacts, calibration commands, unofficial rumble and VIIPER remain
  separate later work.
- Loopback DSU remains unauthenticated local interoperability, not production
  CapyIO network transport.

No commit or push was performed.
