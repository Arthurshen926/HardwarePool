# CAPY-PTP-003E Report — Runtime-admitted VHF Sink factory

Date: 2026-08-31

Status: complete (hardware-free driver path)

## Outcome

The existing validate-before-open Runtime worker can now select
`WindowsVhfTouchpadSinkFactory`. The factory is a zero-sized, side-effect-free
type. Its `open` is reached only after exact Route/Session/endpoints, current
authorization expiry, Stream/epoch, descriptor, first sequence and queue limits
have passed the shared preflight.

Route epoch advance sends Broker Close first, causing the driver to release
active contacts, then sends a fresh Hello whose generation is the new non-zero
epoch. The snapshot timestamp base is reset only after both acknowledgements
succeed. Unknown delivery keeps the session terminal.

## Evidence

```text
cargo test -p capyio-windows-input
  19 library tests passed

cargo test -p capyio-remote-touchpad-adapter --test touchpad_runtime_worker
  14 passed; 7 exact physical tests ignored

cargo clippy -p capyio-windows-input -p capyio-remote-touchpad-adapter
  --all-targets -- -D warnings
  PASS
```

The new ignored test
`authorized_vhf_factory_opens_and_closes_real_driver_interface_without_frames`
is compiled by ordinary CI but was not invoked. The VHF interface remains
absent because no driver was installed.

## Android endpoint

Wireless ADB reconnected successfully to `100.66.157.119:46143` and reported
the API-36 vivo `V2419A` device online. The installed lab package is
`dev.capyio.touchpad.lab` version `0.6`/code `6`; its declared `INTERNET`
permission is granted. The existing reverse mapping is `tcp:38173` to device
`tcp:38173`. The Activity was not launched and the APK was not changed.

## Remaining work

Prepare the exact unsigned local-lab driver package, recovery/rollback evidence
and deployment command for separate approval. After enumeration, invoke only
the ignored Hello/Close acceptance first. A later separately gated test can
compose the current Android transport with the VHF factory and submit fixed
three-/four-finger fixtures.

## Read-only deployment preflight

The current compile artifact remains unsigned and no catalog file exists, so it
is not an installable Windows package yet:

```text
CapyIOVhfTouchpad.sys
  size: 24064
  SHA-256: 5BC76BDAF62FED4CE94779B888B604C3CED6D42139A4A4980D8F3C7E3BBE4CCE
  Authenticode: NotSigned

CapyIOVhfTouchpad.inf
  size: 1407
  SHA-256: CC034E0FE8DEA161B47DD8C6E84218419F3F62CB70DD8B5DC18BEC9E3EE47514
  hardware ID: Root\CapyIOVhfTouchpad
  expected catalog: CapyIOVhfTouchpad.cat (absent)
```

The x64 WDK `devcon.exe` is available at version 10.0.26100.0, but it was not
run. The read-only interface probe reports `absent`, and no matching installed
third-party package was found. Host CIM recovery inventory was denied in the
non-elevated sandbox. Therefore ADR 0029 deployment readiness is not yet met:
an elevated WinRE/BitLocker/Secure Boot inventory, independent recovery path,
catalog/signing decision and exact resulting package hashes are still required.
