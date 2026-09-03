# CAPY-PTP-003A Report — VHF Precision Touchpad compile baseline

Date: 2026-08-30

Status: complete (compile/static-validation only)

## Outcome

The dedicated touchpad worktree now contains a from-scratch KMDF source driver
that links the Windows in-box Virtual HID Framework and builds as an unsigned
x64 Universal driver. No driver was installed, no device was created, and no
certificate, boot or security setting was changed.

The 214-byte report descriptor exposes the mandatory Digitizers/Touch Pad and
Digitizers/Configuration top-level collections. It uses bounded hybrid input
reporting, advertises five contacts, and includes capabilities, 256-byte device
certification status, input-mode, surface-switch and button-switch features.

## Implemented boundary

- one `VhfCreate` / `VhfStart` lifecycle per KMDF device and `VhfDelete` during
  cleanup;
- GET/SET feature handling for capabilities and Configuration collections;
- fixed-size 10-byte contact input report and five-contact capability;
- fixed-size, versioned 16-byte broker header and 34-byte five-contact data
  skeleton; it is not exposed through an IOCTL in this slice;
- no network, Android, JSON, Protobuf, reconnect or gesture policy in kernel;
- hardware-root INF retained only for static validation and later controlled
  deployment review.

## Evidence

The WDK build used Visual Studio Build Tools 17.14 and WDK/SDK 10.0.26100.0:

```text
MSBuild CapyIOVhfTouchpad.vcxproj /m /t:Rebuild
  /p:Configuration=Debug /p:Platform=x64 /p:SignMode=Off
  PASS

python scripts/validate_windows_touchpad_descriptor.py
  CAPY-PTP-003A descriptor validation: PASS (214 bytes)

InfVerif.exe /v CapyIOVhfTouchpad.inf
  INF is VALID
```

Artifact:

```text
drivers/windows-touchpad/x64/Debug/CapyIOVhfTouchpad.sys
size: 17920 bytes
SHA-256: 6C3592B0740511C656BA3F305ADF02627AB600A4424FA8C99D06F3FA9CE503F4
Authenticode: NotSigned
```

The host process provided both case variants of the `Path` environment entry,
which causes MSBuild's .NET tool launcher to fail before `cl.exe`. The recorded
successful build removed only the duplicate `PATH` entry from the child
process environment; it made no persistent environment change.

## Remaining risks and next Gate

At completion of 003A, the device-certification feature returned an explicit
zero-filled placeholder and the Broker IOCTL was absent. `CAPY-PTP-003B`
subsequently replaced that placeholder with Microsoft's documented default
pre-certification blob and added the bounded Broker submission boundary. This
still does not claim HLK certification.

Accordingly, this compile result does not yet prove Windows enumeration or
three-/four-finger Shell gestures. A subsequent code-only slice should add the
bounded broker control device, access policy, queue/backpressure, hybrid scan
submission tests and certification-response decision. Driver installation must
remain a separately approved ADR 0029 Gate 7B operation with an exact package
hash and rollback command.
