# CAPY-GAMEPAD-004A — DualShock 4 motion projection codec

Date: 2026-09-01

Status: hardware-free codec complete; owned VIIPER session and physical Windows
attachment remain pending.

## Outcome

CapyIO now has a deterministic codec for VIIPER v0.7.0's `dualshock4` device
stream. It combines a complete normalized gamepad snapshot with a separately
typed canonical IMU sample only at the Projection boundary. DSU remains an
independent IMU-only output and does not require any virtual controller.

The product scope is now deliberately limited to:

- DSU motion-only for pairing a phone with a real controller in a DSU-aware
  emulator;
- existing Xbox 360 plus DSU compatibility;
- one native-style virtual DualShock 4 carrying controls and motion.

DualSense, adaptive triggers and IMU-to-mouse/right-stick mapping are excluded.

## Changed files

- `adapters/viiper/src/dualshock4.rs`
- `adapters/viiper/src/lib.rs`
- `adapters/viiper/tests/dualshock4_projection.rs`
- `adapters/viiper/Cargo.toml`
- `adapters/viiper/README.md`
- `docs/plans/active/0015-gamepad-projections.md`
- `docs/TESTING.md`
- `third_party/THIRD_PARTY.yml`
- `Cargo.lock`

## Evidence

Commands executed from the isolated `codex/capyio-gamepad` worktree:

```text
cargo fmt --all -- --check
cargo test -p capyio-viiper-adapter
cargo clippy -p capyio-viiper-adapter --all-targets -- -D warnings
cargo xtask validate-docs
cargo xtask validate-manifests
git diff --check
```

Results:

- 6/6 DualShock 4 codec tests passed;
- 22/22 pre-existing VIIPER Xbox/session/probe tests passed;
- VIIPER doc tests passed;
- Clippy passed with warnings denied;
- documentation and Adapter manifest validation passed;
- no whitespace errors were reported.

No live VIIPER process, USB/IP command, Windows device mutation, driver
operation, APK operation or phone connection occurred in this slice.

## Remaining work and risks

- add a bounded owned `dualshock4` session with independent gamepad and IMU
  sequence ownership, initial neutral/stationary report and exact cleanup;
- extend the USB/IP inventory and attachment identity from Xbox
  `045e:028e` to DS4 `054c:09cc` without weakening exact-device checks;
- expose DSU motion-only versus motion+controls as an explicit desktop choice;
- add the desktop DS4 projection control and telemetry state;
- run a separately authorized physical attachment gate and verify Windows PnP,
  buttons, axes and changing gyro/accelerometer reports in a compatible client;
- phone mounting-to-DS4 axes still needs physical calibration. Codec identity is
  deterministic fixture policy, not a claim about the correct handheld pose.
