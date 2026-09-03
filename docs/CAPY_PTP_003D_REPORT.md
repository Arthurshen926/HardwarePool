# CAPY-PTP-003D Report — VHF touchpad Sink session

Date: 2026-08-31

Status: complete (hardware-free)

## Outcome

`capyio-windows-input` now composes `TouchpadFrame` validation/projection and
the fixed VHF Broker client into one typed Sink lifecycle. `open_win32` can
locate and open the protected driver interface only when an admitted Runtime
composition explicitly calls it. No driver, device, service, APK, permission
or desktop input changed in this slice.

## Fail-closed behavior

- descriptor validation occurs before the transport is consumed;
- each accepted frame becomes one bounded complete snapshot;
- a transport failure or malformed Ack makes the client and session terminal;
- explicit close sends the driver's Close record, which releases contacts;
- dropping an active session makes one bounded best-effort Close attempt;
- Closed and Failed sessions reject all later work.

Projection errors remain transactional so a trusted composition layer may
reject a malformed frame without corrupting the prior session state. Route
authorization and authenticated transport admission remain outside this low-
level platform object and are required before `open_win32` is called.

## Evidence

```text
cargo test -p capyio-windows-input
  17 library tests passed; all binary/integration tests passed

cargo clippy -p capyio-windows-input --all-targets -- -D warnings
  PASS
```

The read-only VHF interface probe still reports `absent`; therefore neither the
new helper nor any real IOCTL path was exercised. The last full repository CI
passed immediately before this slice; repository validation is rerun for this
slice and recorded in the final task handoff.

## Remaining work

The next code-only slice is a validate-before-open Adapter Sink factory that
reuses the existing Runtime-owned Route admission tuple. After that, an exact-
name ignored real-interface test can be prepared. Installing the current
unsigned driver remains a separate high-risk lab action requiring the exact
package, recovery posture and rollback command plus fresh approval.
