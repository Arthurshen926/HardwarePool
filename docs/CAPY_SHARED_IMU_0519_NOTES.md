# Shared IMU and optional gamepad motion — 0.5.19-dev

## Implemented

- Android gamepad motion defaults on, has an app-private persisted switch and
  explicitly acquires one shared accelerometer/gyroscope registration. The
  independent IMU lifecycle remains separate. The last user releases sampling.
- Sensor stop clears cached values; samples predating a registration or older
  than 350 ms are unavailable, including the initial warm-up period.
- Controller UDP v2 separates motion availability from gamepad motion selection.
  Missing measurements retain zero timestamps; real component timestamps remain
  unchanged. This is still the trusted-lab/Tailscale compatibility transport,
  not new encrypted media or independent resource Route negotiation.
- Windows accepts v2 and retains v1 controls compatibility, without treating v1
  sentinel timestamps as real IMU. DS4 projects a neutral pose when gamepad
  motion is disabled/unavailable even if independent IMU continues sampling.
- DSU stops on absent motion rather than reusing cached motion. Restart is
  available after sampling resumes; automatic DSU resumption is not implemented.
- Shared UI distinguishes Touch gamepad (optional motion) from Motion sensors
  (independent IMU), and shows sensor availability and independent use separately.

## Checks and deployment

Date: 2026-09-28. Windows build revision: integrated-0519-shared-imu.
Android versionCode 31 / 0.5.19-dev.

- Vue typecheck/Vite build passed.
- Android capabilityCompositionTest passed (319 assertions), including absent
  motion encoding and release order of the two logical sampling users.
- Offline Android assembleDebug passed; authorized Y700 in-place install passed.
- Rust controller tests passed: 21 passed, 1 physical test ignored.
  Two older audio fixture initializers needed their missing optional frame
  counters set to None before the existing lib test target could compile.
- Windows custom-protocol build passed and replaced target/debug/capyio-desktop.exe
  after user confirmed closure. Existing camera-broker dead-code warnings remain.
- The user reported reopening the Windows app and starting reception.

## Physical evidence obtained through normal Android UI

1. Starting only Touch gamepad registered the accelerometer and gyroscope at
   20,000 us requested sampling periods. UI: live motion, independent IMU stopped.
2. Disabling gamepad motion left zero active CapyIO sensor connections.
3. Starting independent IMU with gamepad motion disabled produced one connection.
4. Re-enabling gamepad motion while independent IMU was active kept one connection;
   UI separately showed both users active.

ADB was used for installation, normal touch navigation and read-only sensor
diagnostics, not as the application network transport. No pairing, driver,
platform permission or service declaration was changed.

Pending: Windows physical tilt/rotation confirmation, DS4 motion-off behavior
while independent IMU stays active, both full resource stop orders on-device,
DSU loss/restart physical behavior and long-duration multi-touch stability.
Unit checks and sensor registration alone do not establish game consumption.
Deploy the Android and Windows builds together: old Windows rejects UDP v2;
data-version negotiation/downgrade is not implemented.
