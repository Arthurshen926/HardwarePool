# CAPY-GAMEPAD-004B — Owned DS4 session and DSU motion-only mode

Date: 2026-09-01

Status: hardware-free complete; Runtime composition and physical attachment are
pending in `CAPY-GAMEPAD-004C`.

## Outcome

- Added a bounded `dualshock4` VIIPER session that owns probe, bus creation,
  exact device creation, initial safe state, streaming, feedback and cleanup.
- Kept gamepad and IMU sequence ownership independent inside the DS4
  projection. A gap in one input clears only that input's projected state.
- Extended usbip-win2 selection and owned-port verification to exact DS4
  `054c:09cc`, without weakening the Xbox `045e:028e` boundary.
- Added an explicit desktop DSU mode selector. `motion_only` sends phone IMU but
  never submits touch controls; `motion_and_controls` preserves the existing
  combined behavior.

## Evidence

```text
cargo test -p capyio-viiper-adapter
cargo test -p capyio-windows-input --lib
cargo test -p capyio-desktop --lib
pnpm --dir apps/desktop typecheck
```

Observed results:

- 2/2 DS4 session fixtures passed, in addition to 28 codec/Xbox/probe tests;
- 11/11 Windows input library tests passed, including exact DS4 inventory and
  owned-port identity;
- 35 desktop unit tests passed and 5 separately gated physical tests remained
  ignored;
- Vue TypeScript validation passed.
- Clippy passed with warnings denied; frontend production build, documentation,
  manifests, formatting and whitespace validation passed.

No live VIIPER process, USB/IP attachment, driver operation, APK operation or
phone connection occurred.

## Remaining

- add a Runtime-owned DS4 projection controller that composes the paired
  gamepad/IMU streams with the one-shot attachment lifecycle;
- expose DS4 readiness and activation separately from the existing Xbox status;
- run physical PnP, input and motion verification after an explicit mutating
  attachment authorization;
- calibrate the phone mounting axes against a real DS4-aware consumer.
