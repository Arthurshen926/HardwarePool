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

`capyio-service` is the first headless lifecycle slice. It builds an SCM service
host named `CapyIOBroker` that owns and reaps the dedicated virtual-speaker
Broker. This slice intentionally has no desktop management IPC or installer;
its bounded console mode exists for development verification only.
