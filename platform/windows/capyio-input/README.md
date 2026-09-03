# CapyIO Windows Input

This crate contains the `CAPY-PTP-000` user-mode synthetic Precision Touchpad
API probe and the hardware-free `CAPY-PTP-002A` native frame projection. The
probe dynamically resolves these `user32.dll` exports because the installed
Windows SDK can lag the operating system API:

- `CreateSyntheticPointerDevice2`;
- `InjectSyntheticPointerInput`;
- `InjectTouchpadAction`;
- `DestroySyntheticPointerDevice`.

The default probe only reports symbol availability:

```text
cargo run -p capyio-windows-input --bin capyio-ptp-probe -- --symbols-only
```

`CAPY-PTP-003C` also provides read-only SetupAPI enumeration for the VHF
Broker interface. It neither opens the interface nor sends an IOCTL:

```text
cargo run -p capyio-windows-input --bin capyio-ptp-probe -- --vhf-interface
```

The corresponding `VhfWin32Transport` requires exactly one present interface,
opens it without sharing and performs only the fixed 50-byte synchronous IOCTL.
`VhfBrokerClient` validates every exact Ack and becomes permanently poisoned
after uncertain delivery. `VhfBrokerSnapshotProjector` maps validated himetric
complete snapshots deterministically to the driver's 0..4095 axes.

`CAPY-PTP-003D` composes those pieces as `VhfTouchpadSession`. It validates the
descriptor before opening, performs the Hello handshake, projects and submits
one complete snapshot per accepted frame, and exposes terminal Failed/Closed
states. Explicit close and active Drop ask the driver to release contacts.
`open_win32` is intentionally a low-level trusted-host operation; callers must
complete Runtime Route authorization before invoking it.

The explicit smoke option creates a five-contact 100 x 60 mm synthetic
touchpad and immediately destroys it without injecting any contact frame:

```text
cargo run -p capyio-windows-input --bin capyio-ptp-probe -- --create-device
```

`WindowsTouchpadProjector` consumes validated `TouchpadFrame` snapshots and
produces at most two fixed-capacity batches of at most five contacts. It emits
one-shot release/cancel records, clears retained contacts at sequence gaps and
epoch changes, and suppresses updates until `cancel_all`. On Windows,
`NativeTouchpadBatch::encode` creates PT_TOUCHPAD `POINTER_TYPE_INFO` values
with himetric coordinates and documented active/release/cancel flags. It leaves
pixel coordinates and cross-device timestamp fields zero.

`CAPY-PTP-002B` adds a separate controlled harness:

```text
cargo run -p capyio-windows-input --bin capyio-ptp-inject -- --gesture one-finger-motion --dry-run
```

Dry-run is the default. The only accepted gestures are fixed one-shot
`one-finger-tap`, `one-finger-motion`, `two-finger-pan`, `three-finger-swipe` and
`four-finger-swipe` fixtures. Arbitrary coordinates, frame counts and repeats
are not accepted. Actual submission is unavailable unless both `--inject` and
`--acknowledge-desktop-input` are present. Injection can move the pointer,
scroll content or switch UI state, so it is a separate controlled-lab action.

The harness owns System32 `user32.dll` and the synthetic device through one
RAII object. It destroys the handle on success or error and attempts a bounded
cancel batch after submission failure. Ordinary tests and CI run dry-run only.

`CAPY-PTP-002C` exposes the same lifecycle to a future Windows Sink through
`SyntheticTouchpadSession`. A caller opens one validated stream/descriptor,
submits complete `TouchpadFrame` snapshots, explicitly advances epochs and
closes the session. Native submission failure poisons the session; explicit
close and abandoned-session drop attempt bounded cancellation before the
device owner destroys the handle. This object is a platform lifecycle boundary,
not peer authorization, transport or reconnect policy.

`CAPY-PTP-002E` implements the remote-touchpad Adapter's platform-neutral Sink
trait for this session on Windows. That implementation only forwards validated
frame, epoch and close operations; it never opens a device by itself. The
Runtime must authorize the Route and explicitly construct the session before
building the Adapter receiver.

The first separately approved host acceptance submitted all nine batches of
the fixed one-finger fixture on Windows build 26200.9168. The same command was
denied with Win32 error 5 when run as the isolated `CodexSandboxOffline`
account, then succeeded from the authorized interactive host context. API
success does not by itself prove that a human observer saw the cursor move.

The crate still does not expose a network/UI injection path, install a driver,
wire the VHF session into an Adapter factory, map a separate physical button,
or claim PTP certification or VHF three-/four-finger physical acceptance.
