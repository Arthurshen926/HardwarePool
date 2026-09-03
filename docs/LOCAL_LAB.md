# CapyIO Local Lab

## Named fixture devices

- HP OmniBook Ultra Flip 14: Windows Node fixture.
- vivo X200 Pro mini: Android Node fixture.

The names in testkit are deterministic sample identities, not proof that either
physical device was connected. Each Node fixture owns both Source and Sink Ports.

## Foundation loop

Gates 0–3 require no hardware. Run Core/UI/Adapter smoke tests on the development
host and retain failures in the active plan.

## Future Windows inventory

Before native/driver work, record Windows edition/build/architecture, SDK/WDK,
CPU/RAM, network and the exact test target. For a local-host exception also
record WinRE/recovery access, BitLocker status/recovery-key availability,
Secure Boot state, current audio endpoints, the exact package hash and a tested
uninstall command. Do not change BitLocker, Secure Boot, boot policy or Verifier
without separate explicit approval.

## Android inventory baseline

`CAPY-IMU-001A` uses explicit-serial, read-only ADB commands. The current vivo
fixture was observed as Android 16/API 36, arm64-v8a, 1216x2640, security patch
2026-06-01, with 63 bounded SensorService inventory rows. The ignored evidence
contains no ADB address/port, serial or build fingerprint.

This inventory proves only that the authorized target was online and exposed
sensor metadata. It is not live CapyIO data-plane, SensorServer, permission,
background-lifecycle or APK evidence. Use:

```text
cargo xtask android-doctor --serial <explicit-serial>
cargo xtask android-baseline --serial <explicit-serial>
cargo xtask android-collect --serial <explicit-serial>
```

## Network evidence

Real Adapter tests record interface/link type, addresses, access point/band,
client isolation, baseline latency/jitter, firewall changes and whether an
external overlay is used. Foundation Mock Sidecars use local process pipes only.

## Gamepad DSU physical gate

The first body-motion gamepad run is driver-free: an explicitly configured
SensorServer phone source feeds the Runtime-owned DSU loopback Projection, and
Cemu/Dolphin supplies the local DSU subscription. Use the closed preflight,
run, evidence and rollback procedure in `CAPY_GAMEPAD_DSU_LAB.md`. A passing
command still requires a human axis-direction observation and proves neither a
Windows virtual controller nor DualSense/VIIPER/haptics behavior.

## Windows Xbox 360 USB/IP gate

The authorized host has signed usbip-win2 v0.9.7.7 installed with a retained
restore point and rollback. The installer-required restart was completed by
the operator before the recorded `006D` physical gate. A read-only check while
a VIIPER Xbox session is alive remains:

```text
capyio-gamepad-usbip-lab preflight <viiper-bus-id> <viiper-device-id> 3241 "C:\Program Files\USBip\usbip.exe"
```

## Native DualShock 4 motion gate

With VIIPER v0.7.0 already running on loopback and both automatic attachment
modes disabled, the DS4-specific desktop Gate is:

```text
capyio-desktop --gamepad-ds4-physical-gate <android-port> 3242 <hold-seconds>
```

It prints the Android pairing token and exact VIIPER bus/device IDs, then keeps
one `dualshock4` export alive while complete touch+IMU states arrive. During
that bounded hold, read-only identity verification is:

```text
capyio-gamepad-usbip-lab preflight-ds4 <viiper-bus-id> <viiper-device-id> 3241 "C:\Program Files\USBip\usbip.exe"
```

After a separate attachment authorization, the one-shot liveness Gate is:

```text
capyio-gamepad-usbip-lab attach-ds4 <viiper-bus-id> <viiper-device-id> <hold-seconds> 3241 "C:\Program Files\USBip\usbip.exe"
```

It accepts only `054c:09cc`, records the returned hub port, rechecks that exact
port once per second and detaches only that port. It does not install/remove a
driver or change boot policy.

Build the zero-download Windows Gaming Input consumer before the authorized
attachment window:

```text
powershell -File platform/windows/capyio-input/tools/raw-game-controller-probe/build.ps1 -OutputDirectory target/raw-game-controller-probe
target/raw-game-controller-probe/CapyIO.RawGameControllerProbe.exe --self-test
target/raw-game-controller-probe/CapyIO.HidReportProbe.exe --self-test
```

While exact DS4 attachment `1-1` is alive, move or press a phone control and
run:

```text
target/raw-game-controller-probe/CapyIO.RawGameControllerProbe.exe 054c 09cc 15
target/raw-game-controller-probe/CapyIO.HidReportProbe.exe 054c 09cc 15
```

The first command requires exactly one DS4 in the Windows
`RawGameController` inventory, an advancing native report timestamp, finite
axes and a changed button, direction switch or axis. The second independently
requires exactly one matching HID interface, live input reports and a changed
report. Both must pass before claiming the browser-independent button/axis
consumer Gate. DS4 motion sensors require a separate report parser or a
DS4-aware consumer and are not claimed by either probe.

For a deterministic host-only diagnostic, run
`capyio-ds4-synthetic-lab 3242 <hold-seconds>` before the exact `attach-ds4`
command. It toggles one face button and advances finite stationary IMU samples,
so Windows inventory/report failures can be separated from an unavailable
phone. The direct-HID probe also prints RawInput inventory evidence. Chromium
routes recognized DS4 devices through RawInput rather than WGI; consequently a
WGI/direct-HID pass with zero RawInput matches must not be reported as browser
Gamepad API compatibility.

The WGI probe waits through a fixed five-second discovery window before
evaluating the inventory. Microsoft documents that `RawGameControllers` is
initially empty even when a controller is already connected; an immediate
single snapshot is therefore not valid absence evidence.

On 64-bit Windows, `SP_DEVICE_INTERFACE_DETAIL_DATA_W.cbSize` is eight bytes,
but its variable UTF-16 `DevicePath` still starts immediately after the
four-byte DWORD. The direct-HID probe deliberately reads the path at offset
four; using offset eight truncates the leading `\\` and makes `CreateFileW`
reject an otherwise valid interface path.

The desktop host exposes the same fixed, non-mutating readiness check without
accepting any endpoint or path argument:

```text
target/debug/capyio-desktop.exe --gamepad-windows-read-only-preflight
```

It may report `host_gate_required` with zero exports or `export_ready` with one
bus. It never performs USB/IP attachment; the post-restart attach gate remains
separate.

The separately authorized `attach` form uses `--once`, prints one owned hub
port only after exact status verification, rechecks it once per second for a
bounded 5–300 seconds and detaches only that port. If interrupted, inspect that
exact port before detaching it; do not use all-device detach. Package, command
and evidence details are in
`CAPY_GAMEPAD_006A_REPORT.md`, `CAPY_GAMEPAD_006B_REPORT.md` and
`CAPY_GAMEPAD_006C_REPORT.md`; post-restart evidence is in
`CAPY_GAMEPAD_006D_REPORT.md`.

## Driver target

Prefer a Hyper-V Generation 2 VM or dedicated Windows installation with
snapshots, debugging and recovery planning. ADR 0029 permits
`DESKTOP-AT8EVE9` as a Gate 7B controlled local-lab exception only after the
preflight above passes. Remote-only access without a verified recovery path is
not sufficient for a kernel-driver deployment.
