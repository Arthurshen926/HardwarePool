# CAPY-GAMEPAD-006B — bounded usbip-win2 attachment ownership

Status: user-mode implementation and real read-only preflight passed. Driver
restart and the first mutating attachment remain pending separate authorization.

Date: 2026-08-31

## Result

The Windows input composition boundary now owns the user-mode half of a real
Xbox 360 USB/IP Projection instead of leaving attachment to an unbounded shell
command. The boundary:

- accepts only an absolute executable named `usbip.exe`;
- requires exact `usbip-win2 0.9.7.7`;
- accepts only an explicit IPv4-loopback server with a non-zero port;
- derives one closed `bus-device` ID from the already validated VIIPER Worker;
- lists the export before mutation and requires exact `045e:028e`;
- launches the executable directly without a shell;
- drains stdout and stderr concurrently under independent 64 KiB maximums;
- imposes a positive command deadline no greater than 60 seconds, kills a
  timed-out child and waits for it before returning;
- attaches with `--once`, which prevents persistent/background retry;
- requires an explicit caller assertion that package/signature verification,
  driver health, the installer-required restart and attachment authorization
  have all completed; the assertion is required only for mutation and cannot
  itself prove external Windows state;
- retains the exact returned hub port and detaches only that port;
- writes neutral before USB/IP detach, then stops the stream and removes the
  owned VIIPER bus;
- leaves explicit cleanup retry possible if exact-port detach fails.

`ViiperGamepadRouteController::install_with_usbip` keeps the Runtime Route in
`Starting` until VIIPER probe/provision/neutral and the exact Windows USB/IP
attachment all succeed. Attachment failure performs Worker cleanup and creates
the gamepad-only `CAPY.GAMEPAD.USBIP_ATTACH_FAILED` Problem. The existing
VIIPER-only constructor remains available for hardware-free and DSU-focused
composition.

## Real pre-reboot evidence

The installed CLI reported exact version `0.9.7.7`. Against a real VIIPER
v0.7.0 server bound only to `127.0.0.1`, with both upstream automatic-attach
modes disabled, the read-only export was:

```text
Exportable USB devices
======================
    1-1    : Microsoft Corp. : Xbox360 Controller (045e:028e)
```

The new Rust lab entry accepted that output and printed:

```text
CAPYIO_USBIP_VERSION=0.9.7.7
CAPYIO_USBIP_SERVER=127.0.0.1:3241
CAPYIO_USBIP_BUS_ID=1-1
CAPYIO_USBIP_DEVICE=045e:028e:Microsoft Corp. : Xbox360 Controller
CAPYIO_USBIP_PREFLIGHT_PASSED
```

The concurrently running Android/DSU/VIIPER gate accepted eight valid complete
states, observed non-neutral controls plus finite IMU data, emitted the 350 ms
timeout neutral and removed the owned VIIPER bus. Retained aggregate evidence
is under `target/evidence/gamepad-006b/`. Ephemeral tokens and VIIPER's local
authentication key are intentionally excluded from this report.

The desktop Controller view now has a read-only Windows projection status
model for the expected `Xbox 360 Controller` identity (`045e:028e`), VIIPER and
USB/IP loopback endpoints, the exact owned USB/IP port, lifecycle state and a
gamepad-scoped Problem. The browser mock deliberately reports `unsupported`
and empty bus/port fields; it does not imply that Windows has enumerated a
controller. An in-app browser check at an 844 x 390 viewport found no horizontal
overflow in either the document or the status card, and reported no console
warnings or errors.

No `usbip attach` or `usbip detach` operation was executed in this slice. The
signed driver install still has a recorded pending-restart requirement.

## Commands

Read-only export verification while a VIIPER device session is alive:

```powershell
target/debug/capyio-gamepad-usbip-lab.exe preflight 1 1 3241 "C:\Program Files\USBip\usbip.exe"
```

After the separately authorized restart, the bounded attachment gate is:

```powershell
target/debug/capyio-gamepad-usbip-lab.exe attach 1 1 90 3241 "C:\Program Files\USBip\usbip.exe"
```

The command uses one-shot attachment, prints `CAPYIO_USBIP_OWNED_PORT`, holds
the device for 5–300 seconds and detaches only that port. If the process is
forcibly terminated after attachment, resolve the printed port with
`usbip.exe port <port>` before running `usbip.exe detach --port <port>`; never
substitute `detach --all` in this lab.

## Automated evidence

- exact real-list parsing and malformed/ambiguous export rejection;
- absolute executable, loopback endpoint, timeout and output bounds;
- closed VIIPER bus/device ID conversion;
- exact no-shell argument vectors for version, list, `attach --once` and
  exact-port detach;
- ordered fixture lifecycle: probe -> list -> attach -> detach, including
  idempotent explicit stop;
- bounded concurrent output draining;
- pre-detach neutral request remains sequence-neutral and terminal on write
  failure;
- desktop Windows-projection status serialization, browser-mock fallback and
  844 x 390 responsive rendering;
- all `capyio-viiper-adapter` and `capyio-windows-input` tests plus strict
  Clippy pass.

## Remaining physical gate

Only the pending restart and post-restart mutation/observation are unresolved:

1. attach the exact live export and retain its reported port;
2. verify `USB\VID_045E&PID_028E` and the Xbox 360 controller interfaces in PnP;
3. verify `joy.cpl`, XInput/browser Gamepad API and live phone controls;
4. confirm DSU IMU remains active concurrently;
5. detach the exact owned port, remove the VIIPER bus and verify both disappear.
