# CapyIO Windows Input

CAPY-GAMEPAD-004B extends the bounded usbip-win2 inventory and owned-port
verification to the exact DualShock 4 identity `054c:09cc`. Xbox and DS4 use
separate typed selectors; a matching bus ID with the wrong VID:PID fails closed.

CAPY-GAMEPAD-004C adds `ViiperDs4RouteController`. It owns two independent
Runtime Routes (`gamepad-state/1` controls and `imu-samples/1` motion) and one
shared DS4 projection. Both anchors must match their separate fixed epochs
before exact VIIPER provisioning, and optional one-shot USB/IP attachment must
finish before either Route is Active. Failure of either source sends a safe
state, cleans the shared projection and offlines both dependent Routes while
leaving unrelated Routes untouched. Retry creates advancing epochs and a fresh
owned bus. The desktop now exposes typed selection/read-only inventory and a
bounded complete-state ingress. The physical DS4 executable is implemented;
live VIIPER/USB-IP PnP validation passed on the authorized local host. Native
application-level report validation uses the SDK-only probe described below.

`CAPY-GAMEPAD-007A` keeps that complete DS4 as the default and adds an optional
user-mode ViGEm Xbox 360 direct-XInput companion. When explicitly enabled, the
host sends the same controls to the DS4 Worker and to a separately owned sidecar
through the existing fixed 20-byte Xbox state contract. The companion is for
legacy applications that directly require XInput; it is not recommended for
Steam/browser use on the recorded host because WGI enumerated both tested Xbox
alternatives without advancing their reports. DS4-aware and DSU consumers
retain the independent IMU Routes. Unsupported touchpad/paddle buttons are
omitted only from the Xbox projection. Start failure rolls back both
projections, stream failure neutralizes and stops both, and explicit stop
removes the companion before exact DS4 USB/IP cleanup. The sidecar build
consumes a separately reviewed local-lab ViGEm.NET package and does not install,
remove or configure ViGEmBus.

For a repeatable local debug build, run:

```powershell
.\platform\windows\capyio-input\tools\vigem-x360-sidecar\stage-desktop-debug.ps1
```

The command builds `capyio-desktop`, compiles the reviewed sidecar, stages the
sidecar and its managed runtime assembly beside `target\debug\capyio-desktop.exe`,
runs the sidecar self-test and fails if any of the three runtime files is
missing. It is a local-lab staging command, not a production installer, and it
does not install or remove a driver.

User-mode Windows input Projection composition boundary.

`CAPY-GAMEPAD-001D` composes a fixed-epoch
`capyio.motion.imu-samples/1` Source with one in-process DSU v1001 loopback
Projection. The Runtime Route reaches `Active` only after the bounded Worker
has bound the configured IPv4-loopback port and validated a source anchor for
the exact Route epoch. Upstream failure and explicit stop join the Worker and
release the UDP port before the corresponding Runtime transition; retry uses a
new epoch and a new Worker.

`CAPY-GAMEPAD-001E` adds the `capyio-gamepad-dsu-lab` executable for the first
physical gate: SensorServer accelerometer and gyroscope WebSockets are paired
into the standard IMU Profile and submitted to the Runtime-owned DSU Route.
The command reports success only after it observes a real DSU client
subscription and at least one motion packet, with loss/contract/transport
counters remaining zero. Its endpoint IP is explicit but omitted from output.
See `docs/CAPY_GAMEPAD_DSU_LAB.md`.

`CAPY-GAMEPAD-003D` composes one Runtime-owned
`capyio.input.gamepad-state/1` Route with the bounded VIIPER Xbox 360 session.
The Route becomes `Active` only after exact-version probing, owned bus/device
provisioning, device-stream handshake and initial neutral succeed. Upstream
disconnect, terminal stream failure and explicit stop clean the owned Worker
before the corresponding Runtime transition; retry creates a fresh Worker on
the new Route epoch.

This crate does not start or configure VIIPER. The caller must independently
verify `--api.auto-attach-local-client=false` before constructing the required
assertion token. It does not install/remove a driver, use SendInput or another
Windows injection API, or map reverse rumble into a haptics Route.

`CAPY-GAMEPAD-006B` adds the bounded user-mode usbip-win2 attachment owner. It
accepts only an absolute `usbip.exe`, exact version `0.9.7.7`, explicit IPv4
loopback, bounded command/output limits, a VIIPER-derived bus ID and the fixed
Xbox 360 identity `045e:028e`. It invokes the executable directly without a
shell, uses `attach --once`, retains the returned hub port and detaches only
that port. Mutation additionally requires a caller assertion covering the
verified package/driver, completion of any required restart and explicit
attachment authorization; read-only probe/list remain available without it.
`ViiperGamepadRouteController::install_with_usbip` keeps the Route
Starting until both VIIPER initial neutral and the Windows attachment succeed;
stop orders pre-detach neutral, exact-port detach and owned-bus removal.

The `capyio-gamepad-usbip-lab` executable exposes read-only `preflight` /
`preflight-ds4` and bounded 5–300 second `attach` / `attach-ds4` gates. The DS4
forms require exact `054c:09cc`; both attach forms retain and detach only their
reported port. Driver deployment, restart and each real attachment remain
explicitly authorized host-lab operations. See
`docs/CAPY_GAMEPAD_006A_REPORT.md` and `docs/CAPY_GAMEPAD_006B_REPORT.md`.

`tools/raw-game-controller-probe` is a local-lab-only Windows Gaming Input
consumer. `build.ps1` compiles it with the installed .NET Framework compiler
and newest installed Windows SDK `Windows.winmd`; it downloads nothing and
adds no production dependency. `CapyIO.RawGameControllerProbe.exe` fails closed
unless exactly one requested VID:PID is visible through Windows Gaming Input;
a passing run also requires an advancing report timestamp, finite axes and at
least one changed button, switch or axis during the bounded 5–300 second
observation. Windows documents that the static inventory is initially empty
even for an already connected controller, so the probe first polls it for a
fixed five-second discovery window and prints the final inventory evidence.
`CapyIO.HidReportProbe.exe` independently enumerates the exact HID
interface and performs bounded overlapped input-report reads, requiring at
least one changed report. It also records the Win32 RawInput inventory and
tests the same shared read/write HID open used by Chromium's Windows DS4 path.
This distinguishes Gaming Input filtering, RawInput publication and a missing
HID data stream. A direct-HID pass is not a RawInput or browser Gamepad API
pass. Neither probe claims DS4 motion semantics, deploys a driver or owns
attachment. Its x64 interface-detail handling writes the
required eight-byte `cbSize` but reads the variable UTF-16 device path at
offset four, immediately after the DWORD, so the valid leading `\\` is not
truncated from the Win32 HID path.
`CapyIO.XInputProbe.exe` independently requires exactly one connected XInput
slot and fails unless its packet number and complete state change during the
bounded observation.

`tools/gamepad-api-probe/index.html` is the manual foreground-browser Gate.
Serve its directory from loopback (for example, `python -m http.server 8765`),
focus the page and generate a controller input. It reports connection events,
changing frames and every browser-visible axis/button value. The standard Web
Gamepad API does not expose DS4 accelerometer or gyroscope samples, so this Gate
proves controls only; motion still requires a DS4-aware or DSU consumer.
The recorded automated Gate proves the same DS4 through Windows Gaming Input;
it does not claim that a particular foreground Chromium profile enumerated it.

`capyio-ds4-synthetic-lab` is a debug-only deterministic consumer Gate. It
creates exactly one loopback VIIPER DS4, advances controls and finite IMU at a
bounded cadence, prints the closed bus/device identity, holds for 5–300 seconds
and removes only its Worker-owned bus. It exists to validate the Windows
consumer chain without depending on wireless Android availability and is not
a production source or controller emulator by itself.
The debug-only direct DS4 Gate detaches the otherwise unused production host
ingress immediately after capturing its anchors, preventing its bounded queue
from reporting false pressure while the Gate-owned Worker consumes snapshots.

`CAPY-GAMEPAD-006C` adds a read-only inventory method used by the desktop host.
It performs the same pinned version/list commands but returns only exact Xbox
360 `045e:028e` exports, retains multiple matches for fail-closed ambiguity
handling and never invokes attach. The parameter-free Tauri command keeps the
absolute executable and both loopback endpoints out of WebView input. See
`docs/CAPY_GAMEPAD_006C_REPORT.md`.

`CAPY-GAMEPAD-006D` closes the first post-restart physical Windows gate and
hardens attachment ownership. A successful one-shot attach now returns only
after `usbip port <owned-port>` confirms the exact loopback server, VIIPER bus
ID and Xbox `045e:028e` identity. The lab rechecks that closed inventory once
per second during its bounded hold; disappearance fails immediately and still
attempts exact-port cleanup. See `docs/CAPY_GAMEPAD_006D_REPORT.md`.

All controller operations perform bounded blocking socket I/O and belong on a
host-owned Adapter worker. They must not run in a UI or real-time callback.
The DSU endpoint accepts only IPv4-loopback clients; the live SensorServer lab
still uses unauthenticated `ws://` and is not a production network transport.
