# CAPY-PTP-003C Report — Win32 VHF Broker client

Date: 2026-08-31

Status: complete (code/read-only enumeration only)

## Outcome

The Windows input crate now contains the user-mode half of the fixed VHF
Broker ABI. It can enumerate exactly one present CapyIO interface with SetupAPI,
open it directly and exclusively with `CreateFileW`, send one synchronous
50-byte `DeviceIoControl`, require a canonical 50-byte Ack and close all Win32
handles through RAII.

No real interface was opened and no IOCTL was sent in this slice. The read-only
probe confirmed `vhf_broker_interface=absent`, which is expected because the
new driver has not been installed.

## Boundaries

- SetupAPI distinguishes absent, single, multiple and failed enumeration;
- a detail path is capped at 4,096 bytes and copied from aligned bounded
  storage only after the documented size query;
- zero or multiple interfaces fail closed rather than selecting an arbitrary
  device;
- the device opens without sharing and only through the compiled interface
  GUID; no device path comes from UI, network or shell text;
- synchronous `DeviceIoControl` requires exactly 50 returned bytes;
- transport failure or malformed/mismatched Ack poisons the Broker client, so
  a possibly delivered Data record is never optimistically retried;
- abandoned handle cleanup causes the driver's file-close release path when a
  device is eventually deployed.

## Snapshot projection

`VhfBrokerSnapshotProjector` validates each direction-neutral `TouchpadFrame`,
maps the declared himetric surface deterministically into HID coordinates
`0..4095`, converts source-time deltas to the required wrapping 100 µs scan-time
unit, restricts IDs to the driver's 4-bit range and maps the integrated click
to Button 1. CancelAll produces an empty released snapshot. Timestamp or
contract failure does not commit projector state.

## Evidence

```text
cargo clippy -p capyio-windows-input --all-targets -- -D warnings
  PASS

cargo test -p capyio-windows-input
  15 library tests passed
  11 integration/binary tests passed

cargo run -p capyio-windows-input --bin capyio-ptp-probe -- --vhf-interface
  vhf_broker_interface=absent
  device_opened=false
  ioctl_sent=false
```

The renewed Android wireless-debug endpoint was recorded as
`100.66.157.119:46143`. A direct ADB connection attempt returned WinSock 10060
and `adb devices -l` remained empty. A TCP probe also failed. No APK or device
state was changed. The phone endpoint/IP or wireless-debug availability must be
reconfirmed before the next physical Android run.

## Remaining work

The Win32 client is not yet composed with the existing Runtime-admitted
touchpad Sink/receiver. The next code-only slice should implement a VHF Sink
session around the projector and Broker client, prove cancel/epoch/Drop
behavior with a fake transport, and add an ignored exact-name real-interface
smoke that remains disabled until driver deployment is separately approved.
