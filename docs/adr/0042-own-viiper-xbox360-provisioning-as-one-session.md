# ADR 0042: Own VIIPER Xbox 360 provisioning as one session

Status: accepted

## Context

VIIPER v0.7.0 exposes management requests and a separate long-lived device
stream. After `bus/{id}/add`, the client must connect that stream within a
bounded server timeout. Local device creation may also invoke upstream USB/IP
auto-attach by default.

The fixed-release documentation says auto-attach failure does not affect the
add response. The reviewed fixed-revision handler can instead return 409 after
creating the device without rolling it back. A public create/add/stream CRUD
sequence would therefore permit abandoned resources, ambiguous ownership and
accidental USB/IP mutation.

## Decision

Keep read-only `ping` independent. Expose mutation only through one loopback
`open_xbox360` operation that:

1. requires an explicit caller assertion that the separately supplied server
   was configured and verified with localhost auto-attach disabled;
2. re-probes exact `VIIPER` / `0.7.0` compatibility;
3. creates one automatically allocated bus;
4. adds only the default Xbox 360 type and validates the returned bus, numeric
   device ID, VID, PID, type and subtype;
5. immediately sends the device-stream handshake and an initial neutral frame;
6. returns a Worker owning the stream and created bus.

Do not expose generic management requests, bus/device CRUD, inventory, device
type, VID/PID or path construction. Once a positive bus ID is known, every
later open failure attempts bounded removal of that bus. Explicit stop orders
neutral, stream shutdown and bus removal and retains neutral/cleanup failures.
Drop performs socket shutdown only; it does not perform management I/O.

The Worker validates the complete state and Xbox codec before observing a
copied sequence tracker. Gap recovery sends neutral before the recovered state.
Epoch advance sends neutral before committing the new epoch. Sequence
exhaustion sends a final neutral and latches. Stream I/O failure is terminal;
cleanup remains available through explicit stop.

## Consequences

- create/add cannot be separated from stream ownership or delegated to the UI;
- the assertion token records a trusted-host precondition but cannot inspect or
  prove the external process flag, so a real lab must retain independent launch
  and configuration evidence;
- a malformed create success response may create a bus without revealing a
  safely validated ID. The client must not guess, enumerate or remove another
  owner's bus; this remains an upstream-protocol failure risk;
- if the device stream is already broken, the client cannot guarantee that a
  final neutral reaches the virtual device. It still shuts down locally and
  removes the owned bus;
- no real VIIPER, USB/IP client, driver, certificate or system device is used by
  repository tests. Those operations remain separately authorized.

## References

- <https://github.com/Alia5/VIIPER/blob/v0.7.0/docs/api/overview.md>
- <https://github.com/Alia5/VIIPER/blob/v0.7.0/internal/server/api/handler/device.go>
- ADR 0019: third-party vertical-slice reuse
- ADR 0040: separate video/input contracts from platform projections
