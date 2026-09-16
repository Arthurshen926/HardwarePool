# ADR 1409: Desktop owns bounded Android receiver sidecars

Status: accepted

Date: 2026-09-16

## Context

ADR 1408 moved the touchpad, touch-gamepad and IMU senders into the unified
Android APK. The corresponding Windows touchpad and camera receivers still had
to be started manually, so the physical product required more than one
user-facing Windows entrypoint even though the receiver binaries were already
bounded repository components.

The existing touchpad and camera transports are controlled-lab contracts. They
use exact loopback ports and ADB reverse rather than authenticated product
pairing. Moving their lifecycle under the desktop host must not broaden them
into an arbitrary shell surface or silently enable driver deployment.

## Decision

The Windows desktop process owns two fixed Quick Actions for the Android
touchpad and camera receivers.

- Each action locates `adb.exe` only through `CAPYIO_ADB`, `ANDROID_HOME` or
  `ANDROID_SDK_ROOT`, accepts exactly one device in the `device` state, and
  creates only its fixed ADB reverse mapping.
- The desktop launches only a fixed sibling executable with fixed bounded
  arguments. UI input cannot select an executable, append arguments or choose
  a port.
- Touchpad uses the existing user-mode synthetic projection on loopback port
  61000. It never selects `--vhf` and therefore does not install or open the
  protected VHF path.
- Camera uses the existing AVC receiver on loopback port 38173 with Windows
  NV12 decode, a maximum of 7200 access units and a 60-second reconnect grace.
  It does not register or install a virtual camera.
- Stop and desktop shutdown reap the owned child and remove the exact mapping.
  Unexpected child exit is surfaced as a failed Quick Action.

The sidecars remain data-plane boundaries. Stdout is not used for media and the
Vue client receives only typed lifecycle DTOs.

## Consequences

- Normal testing now starts from one Android app and one Windows app; the
  Windows app invisibly owns the two bounded helper processes.
- The current debug artifact must place the two receiver executables next to
  `capyio-desktop.exe`. Installer/bundle work remains open.
- Automatic authenticated discovery, long-running reconnect, virtual-camera
  registration and production touchpad authorization remain separate work.

