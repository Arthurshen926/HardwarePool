# CAPY-GAMEPAD-005B — Android Controller Lab report

Status: authorized physical-device gate passed for Android touch + IMU ->
desktop listener/inspector -> reconnecting DSU v1001 subscriber. Emulator game
behavior remains a separate manual observation because no Cemu/Dolphin binary
was found in `PATH` or the checked common install locations.

Date: 2026-08-30

## Result

The gamepad worktree contains a native foreground-only Android application with
an explicit controller surface, accelerometer/gyroscope collection and a
bounded complete-state UDP sender. The desktop Controller view starts a
token-filtered LAN listener, shows live controls plus raw SI-unit IMU values and
feeds the exact accepted state into the loopback DSU motion+controls Worker.

An authorized vivo phone completed the physical gate. The retained aggregate
result was:

```text
accepted=11
rejected=0
replayed=0
timeouts=0
dsu_packets_after_reconnect=1
dsu_packet_numbers=0..=0
non_neutral_controls=true
finite_imu=true
source=android_touch
last_event=packet_accepted
gate=passed
```

The gate first registered a real DSUC pad-data client, received a DSUS packet,
closed that client, then registered a second client after the server had
published to the stale Windows UDP endpoint. The second client received a
100-byte DSUS packet containing both an A-button/stick stimulus and finite IMU
values. This covers the Windows `ConnectionReset` recovery path as well as the
phone-to-DSU data path.

The common unified Android Node branch still has no landed Gradle/JNI shell, so
this lab stays isolated under `apps/android/controller-lab`. Its portable wire
and Core inputs remain narrow enough to move behind that host later.

## Android artifact evidence

- package: `io.capyio.controllerlab`
- version: `0.1.0-lab` (`versionCode` 1)
- min/target/compile SDK: 26 / 36 / 36
- manifest permission: only `android.permission.INTERNET`
- backups disabled; no foreground service is declared
- physically installed APK copy:
  `target/evidence/gamepad-005b/installed-controller-lab-current.apk`
- installed APK size: 889585 bytes
- installed APK SHA-256:
  `2230552CD54B2E6E74E07D76567A5406A76CD2077290644BF38957BEF2FE1162`
- the authorized old-package uninstall and new APK installation both returned
  `Success`; the pulled installed APK is byte-identical to the current build:
  `apps/android/controller-lab/app/build/outputs/apk/debug/app-debug.apk`,
  also 889585 bytes with the same SHA-256
- package inspection reported `android.permission.INTERNET: granted=true`
- the launched Activity was visible in landscape at 2640x1216; the device
  exposed accelerometer and gyroscope sensors
- retained UI screenshots:
  `target/evidence/gamepad-005b/android-controller.png` and
  `target/evidence/gamepad-005b/android-controller-final.png`
- rollback: `adb uninstall io.capyio.controllerlab`

Private addresses, the wireless-debug endpoint, pairing tokens and the device
serial are intentionally not copied into this report text. Local ignored
screenshots contain the ephemeral lab configuration shown by the Activity.

## Desktop and DSU evidence

The desktop inspector accepted thousands of real phone frames during the first
interactive run. Its observed counters included 3758 accepted, one intentionally
malformed probe rejected, zero replays, zero peer timeouts, a current packet age
of 0 ms and an advancing remote sequence. Controls and IMU were presented by
the same `android_touch` snapshot used by the DSU projection.

The repository-owned DSU/Cemuhook subscriber also collected 360 consecutive
100-byte DSUS pad-data packets from the physical stream. The run observed a
stable stationary acceleration vector, finite gyro values and monotonically
increasing packet numbers. A later debug-only gate proved non-neutral controls
and replacement-subscriber delivery after the Windows UDP reset condition.

The first subscriber run exposed a Windows-specific failure: after a UDP client
closed, a subsequent server receive could report `ConnectionReset`, which the
Worker treated as fatal. `DsuLoopbackServer::poll` now treats that condition as
an idle receive, matching nonblocking UDP semantics, with a focused regression
test and the physical replacement-subscriber gate.

## Automated evidence

- `cargo test -p capyio-dsu-adapter` including UDP reset recovery;
- `cargo test -p capyio-desktop --lib`: 29 passed, 5 explicitly ignored
  physical gates;
- `cargo clippy -p capyio-desktop --all-targets -- -D warnings`;
- `cargo check -p capyio-dsu-adapter --example dsu_subscriber_probe`;
- `pnpm --dir apps/desktop build:web`;
- Android Gradle `:app:lintDebug :app:assembleDebug`;
- `aapt2 dump badging` and `aapt2 dump permissions`;
- repository document and manifest validation.

The debug desktop executable additionally accepts the operator-assisted command
below. It prints a fresh token, requires physical touch+IMU data, replaces its
DSU subscriber mid-run and exits nonzero unless the complete gate passes:

```powershell
target/debug/capyio-desktop.exe --gamepad-physical-gate 31581 26761
```

The command is compiled only with `debug_assertions`; release builds do not
expose this lab entry point.

## Remaining limits

- Cemu/Dolphin UI and an actual game's response were not observed on this host;
  the evidence ends at a conforming DSUC/DSUS subscriber.
- ADB provides one synthetic pointer at a time, so independent multi-pointer
  ownership is covered by Android code structure and host tests, not by this
  physical run. A manual two-thumb session remains useful.
- Axis mounting/calibration, latency under congested Wi-Fi, background process
  death, production pairing/encryption and reverse haptics remain out of this
  slice.
- This is a debug lab application, not yet an Adapter inside the unified Android
  Node lifecycle.
