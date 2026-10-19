# Display closeout — 2026-09-30

Implemented and deployed; current-device acceptance confirmed by the user on
2026-09-30. Screen-sharing initial feature scope is closed.

- Desktop Stop is generation-bound through service stop_peer(generation), so a
  stale coordinator cannot stop a newer session. A finite stopping wait leaves
  an explicit unconfirmed-cleanup error rather than permanently blocking retry.
- Android Home/lock stops the node display session. Rotation keeps existing
  configuration handling. Startup exceptions notify the peer and clean up.
- Receiver has explicit Stop, configured FPS/bitrate, separately labelled
  measured presentation rate, and actionable failure wording.
- Complete local-debug package includes embedded desktop UI, matching APK,
  helpers, display DLL dependencies, shaders/includes and WGC helper. The
  packager rejects non-MZ placeholders; the Xbox helper was rebuilt and passed
  its no-device self-test. A legacy staging command now enables custom-protocol.

Checks: desktop display workflow 6 passed; stale-generation service Stop test
passed; Android Java and presentation harness 4 assertions passed; full desktop,
helper and APK builds passed; bundle upgrade/rollback fixture passed. Package
integrity passed for 69 files. Build retains known unused camera helper warnings.
An initial package attempt rejected libstdc++-6.dll; filename rules now accept
the '+' in real runtime DLL names, and complete packaging succeeded.

Package: target/integrated-test/CapyIO-debug-20260929-164006-0666
(UTC timestamp). It is local debug only, NOT a production installer or approved
public redistribution. Existing drivers, broker configuration, camera
registration and VIIPER remain prerequisites. Do not run helper EXEs manually.
Normal user's current entry remains target/debug/capyio-desktop.exe; use only
one app window and the original pairing account. No pairing data is bundled.

APK SHA256 F8805440E8B66D841D49035E6EE743D6554F405AC71ED822EC338FA3C66BE1A3
installed with -r successfully. Service stage display-recovery-20260930 deployed,
SHA256 85D47940ECC1778F9E99084D0952DF5F9E8C217CE0D955026E53DDC95CC6A445.
Old service stages remain recoverable. No driver or permission changes.

Manual workflow and automatic picture were previously accepted by the user.
Following the requested Home/restart, lock/unlock, and disconnection/reconnection
checks (including peer state and desktop restoration), the user reported:
“可以的，验收都正常”. This is user-reported physical acceptance; no independent
per-case telemetry or measured duration is claimed. It closes the current-device
initial feature acceptance, not arbitrary-device coverage or production release.
120 FPS, live mode switching and network-adaptive bitrate remain future features.
