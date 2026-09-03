# VIIPER Adapter

CAPY-GAMEPAD-004A adds the hardware-free codec boundary for VIIPER v0.7.0's
`dualshock4` device type. Complete normalized controls and a separately typed
canonical IMU sample become the documented 31-byte stream state. Acceleration
uses 512 counts per m/s² and angular velocity uses 16 counts per degree/second;
explicit axis permutations keep phone mounting outside the codec. Touch contacts
remain inactive, touchpad click is supported, and exact seven-byte rumble/LED
feedback is parsed without yet creating a reverse Route. This slice does not
provision or attach a real DS4 device.

The desktop Controller Lab selects the explicit portrait-native Android phone
held in fixed landscape mapping: source `+Y,+Z,+X` becomes DS4 body X,Y,Z
(pitch, yaw and roll) respectively. The Android IMU StandardPort remains in
its normative fixed device frame; mounting policy belongs only to the
Projection.

CAPY-GAMEPAD-003C1 adds one deliberately read-only network operation to the
pinned VIIPER boundary: a caller supplies an explicit IP loopback socket
and bounded connect/I/O deadlines, then `probe` sends exactly `ping\0`, reads at
most 4 KiB through connection close, and accepts only four closed identities:
the release `VIIPER` / `0.7.0`, or the local-lab-only
`VIIPER` / `0.7.0-capyio-88f66f1` build from reviewed upstream revision
`88f66f1ed0c3716c78f810d92b1924112093f896`. The latter contains upstream's
asynchronous, cancelable interrupt-URB work and is explicitly reported as
experimental. The third identity, `VIIPER` / `0.1.0-capyio-fd298a0`, is the
local-lab build of hbashton/VIIPER v0.1.0 at
`fd298a04d7d229293be15b2af664405c9e68114c`; that fork is used by its
DS4Windows integration and preserves the legacy 31-byte DS4 state and
seven-byte feedback stream. The fourth identity is the reviewed hbashton
release `VIIPER` / `0.1.2` at revision `f5d097b`; it preserves the DS4 stream
contract and remains experimental in CapyIO pending the complete phone-input
Gate. No prefix or other version is accepted. There is
no default port, hostname/DNS lookup, discovery, environment override or
generic request surface.

The experimental executables are built only below ignored `target/` lab
storage and are not imported or distributed. The hbashton build enabled the
standard HID interface and passed the bounded direct-HID, delayed-WGI and
RawInput-inventory Gates; the Alia5 release and current-main build did not. The
browser-control Chrome surface did not expose even a ViGEm Xbox control device,
so it is not treated as host Gamepad API absence evidence.

CAPY-GAMEPAD-003C2 adds the mutating boundary only as one owned operation. The
caller must explicitly assert that the separately supplied VIIPER process was
configured with localhost auto-attach disabled. `open_xbox360` then re-probes
one of the four closed compatible identities, creates one bus, adds only the
fixed default Xbox 360 device, opens its raw stream immediately and writes
neutral before returning a Worker. Once the bus ID is known, every failed open
attempts bounded bus removal. The crate does not expose arbitrary requests,
bus/device CRUD, VID/PID or device types.

The Worker validates and encodes a complete `GamepadState` before consuming its
sequence. A gap emits neutral before the recovered state; epoch advance emits
neutral before accepting the new epoch; sequence exhaustion emits a final
neutral and latches until explicit epoch advance. Stop is idempotent and orders
neutral, stream shutdown and owned-bus removal. Stream failure is terminal,
while stop still attempts cleanup. Raw two-byte rumble is read under an
absolute deadline; it is not yet a Route-owned haptics command.

An explicit `request_neutral` operation writes a complete neutral frame without
consuming sequence. The Windows host uses it before detaching an independently
owned USB/IP port; normal Worker stop still sends neutral again, closes the
stream and removes the VIIPER bus. Drop never performs network/process I/O.

CAPY-GAMEPAD-003B implements the deterministic Xbox 360 packet boundary:
complete normalized controls become one fixed 20-byte little-endian
device-stream `InputState` frame, and exact 2-byte rumble feedback is parsed
without inventing a haptics duration or Route identity. The external stream
layout is explicitly distinct from VIIPER's different host-facing USB
`BuildReport` layout, which happens to have the same byte count.

Source-axis signs are explicit. Unsupported touchpad and paddle buttons fail
closed, trigger and signed-axis endpoint scaling is fixed, and reserved bytes
remain zero. No upstream source, generated client, library or binary is
imported. All session tests bind only `127.0.0.1:0` and emulate every response;
they do not start or connect to VIIPER. Starting VIIPER, USB/IP attachment,
Windows driver/certificate changes and system-device claims remain outside this
slice and require separate lifecycle, security and physical-lab review.

`platform/windows/capyio-input` now owns the fixture-proven Runtime Route
composition. This protocol crate remains independent of `capyio-runtime`; it
does not register catalogs, choose retry policy or mutate unrelated Routes.
