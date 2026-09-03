# CapyIO VHF Precision Touchpad

This directory contains the compile-only `CAPY-PTP-003A` baseline for a
Windows Precision Touchpad source driver. The driver uses the in-box Virtual
HID Framework (VHF) and exposes the mandatory Touch Pad and Configuration HID
top-level collections.

The driver exposes one administrator-restricted, exclusive device interface
with a canonical 50-byte Hello/Data/Ack/Close IOCTL. It accepts complete
snapshots of at most five contacts and submits bounded hybrid HID reports. It
accepts no network, Android, Route, JSON or Protobuf data. The user-mode encoder
is in `platform/windows/capyio-input/src/vhf_broker.rs`; bounded SetupAPI
enumeration and the exact `DeviceIoControl` client are implemented and tested
without opening a real interface in default CI.

## Build (no installation)

```powershell
& 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Current\Bin\MSBuild.exe' `
  CapyIOVhfTouchpad.vcxproj /m /p:Configuration=Debug /p:Platform=x64
```

Do not install the INF or create the root-enumerated device without the
separate deployment approval required by ADR 0029 and ADR 0048.

The separately authorized `CAPY-PTP-003F` helper
`scripts/build_windows_touchpad_test_package.ps1` hash-pins the INF/unsigned SYS
and writes only under `target/lab-packages`. Its temporary private key is
deleted and its self-signed public certificate is deliberately not trusted.
This does not authorize installation or boot/security changes. The exact
scoped rollback helper is `scripts/remove_windows_touchpad_test_driver.ps1`.
