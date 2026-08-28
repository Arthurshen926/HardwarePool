# Windows User-Mode Adapter / Broker

This directory will host Windows user-mode integration. The Broker reuses the shared Rust Runtime and is separate from the kernel driver.

Responsibilities:

- Windows service/process lifecycle;
- remote peer and capability sessions;
- authenticated control and audio transport;
- codec, jitter buffer, resampling and clock-drift correction;
- connection to the virtual-audio driver IPC;
- device state, metrics and diagnostics;
- future tray/UI integration through a narrow management API.

It must treat the driver as an untrusted privileged boundary and validate every IPC response before using it.

`capyio-service` builds an SCM service host named `CapyIOBroker` that owns and
reaps the dedicated virtual-speaker Broker. Its local-only, ACL-protected named
pipe accepts only bounded `status`, `start` and `stop` requests. CapyIO Desktop
uses that boundary before its environment-configured development fallback, so
normal use does not elevate Tauri and closing the window does not stop sharing.

The installed local-lab service can also be inspected directly:

```powershell
target\release\capyio-windows-service.exe --control status
target\release\capyio-windows-service.exe --control start
target\release\capyio-windows-service.exe --control stop
```

Service/driver installation and configuration still require an administrator
and are not yet integrated into a signed product installer.
