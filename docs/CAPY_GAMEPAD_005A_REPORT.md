# CAPY-GAMEPAD-005A — Shared touch Controller and desktop Projection lab

Date: 2026-08-29
Branch: `codex/capyio-gamepad`
Status: hardware-free slice complete

## Result

CapyIO now has an explicit top-level `Controller` experience instead of only
an IMU numeric panel. The shared Vue surface presents D-pad, face and shoulder
buttons, system buttons, two radial sticks and two analog triggers. The desktop
host composes those semantic changes into complete fixed-epoch
`capyio.input.gamepad-state/1` snapshots and exposes a read-only inspector for
controls, epoch, sequence and Projection counters.

The Tauri build can optionally bind the existing bounded DSU Worker on
`127.0.0.1:<explicit-port>`. It seeds a clearly simulated stationary motion
sample so complete control snapshots can be tested with a local DSU consumer
before an Android producer exists. Browser Mock never opens the socket and
labels the Projection unsupported.

This report does not claim an Android application, phone input, physical IMU,
emulator subscription or game compatibility.

## Clean-room design review

The interaction model was reviewed against the official repositories for
Moonlight Android, Sunshine, chiaki-ng, scrcpy and PadConnect. Useful behaviors
were complete controller snapshots, neutral-on-cancel, radial stick clamping,
layout editing, independent motion/control handling and host-side counters.

Moonlight Android, Sunshine and PadConnect are GPL-3.0; chiaki-ng is AGPL-3.0.
No source, layout, art or packet implementation from those projects was copied
or linked. scrcpy is Apache-2.0 but its gamepad direction is PC-to-Android, so it
was used only as a platform-boundary reference. The implementation here is new
CapyIO Vue/TypeScript and Rust code over the already reviewed CapyIO contracts
and DSU Adapter.

Reviewed upstream entry points:

- `https://github.com/moonlight-stream/moonlight-android`
- `https://github.com/LizardByte/Sunshine`
- `https://github.com/streetpea/chiaki-ng`
- `https://github.com/Genymobile/scrcpy`
- `https://github.com/Ishan09811/PadConnect`

## Implemented behavior

- top-level `Controller` navigation and `?view=controller` launch surface;
- per-control pointer ownership, radial dead zone/clamp and 60 Hz bounded stick
  updates;
- complete-state Rust composition with host-owned stream ID, epoch and sequence;
- automatic neutral state on pointer cancellation, view teardown, explicit
  reset, DSU start and DSU stop;
- a desktop inspector for every exposed control and stream identity;
- explicit local DSU start/stop with port validation and bounded queue,
  subscriber, packet and error counters;
- Browser Mock/Tauri DTO parity and visible simulation labels.

## Automated and visual evidence

```text
cargo test -p capyio-desktop gamepad_lab --lib
pnpm --dir apps/desktop typecheck
pnpm --dir apps/desktop build:web
cargo clippy -p capyio-desktop --all-targets -- -D warnings
```

Focused Rust tests: 3 passed, 0 failed. They cover complete composition/reset,
invalid-update sequence preservation and simulator-to-DSU-worker acceptance.
Frontend typecheck and production build passed. Browser checks at the default
desktop viewport and 844x390 found no horizontal overflow or console errors;
one A-button click produced press/release sequences 0 and 1.

## Remaining work

- integrate the shared Controller view with the unified Android host once its
  module/lifecycle seam is available in the common branch;
- add the Android platform sensor Adapter and keep IMU and controls as
  independent typed streams;
- select and document a versioned authenticated peer data plane; the desktop
  DSU fixture is loopback-only and is not that transport;
- build, install and test an APK only after separate approval for the exact
  Android target and any manifest permission/service changes;
- retain physical acceptance for multi-touch, focus/pause cancellation,
  disconnect/reconnect, emulator subscription, axes and stuck-control safety.
