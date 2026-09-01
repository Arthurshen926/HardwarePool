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
reaps the dedicated native audio Brokers. Its local-only, ACL-protected named
pipe accepts only bounded `status`, `start` and `stop` requests. CapyIO Desktop
uses that boundary before its environment-configured development fallback, so
normal use does not elevate Tauri and closing the window does not stop sharing.

The service has two closed launch modes. The omitted/default mode retains the
Audio Share compatibility Broker and its established-TCP activation evidence.
`--mode native-speaker` launches `capyio-native-virtual-speaker` with explicit
local and peer IPv4 socket addresses. The native supervisor requires the
Broker's bounded readiness line before activation and reports
`receiverPresent=false`: UDP process readiness is not evidence that Android is
receiving. Android packet/render counters provide that separate evidence.

Native mode may additionally supply one complete microphone child
configuration. It launches `capyio-native-virtual-microphone`, which receives
the fixed 48 kHz mono S16LE lab stream and writes the service-owned capture
ring. All five microphone options are required together. Speaker startup is
rolled back if microphone startup fails, and both children must remain alive.
This mode is mutually exclusive with the MicYou capture producer.

```text
capyio-windows-service.exe --mode native-speaker \
  --broker capyio-native-virtual-speaker.exe \
  --bind-ip 100.64.x.y --port 46001 \
  --peer-ip 100.64.a.b --peer-port 46000 \
  --microphone-broker capyio-native-virtual-microphone.exe \
  --microphone-bind-ip 100.64.x.y --microphone-port 46011 \
  --microphone-peer-ip 100.64.a.b --microphone-peer-port 46010
```

The installed local-lab service can also be inspected directly:

```powershell
target\release\capyio-windows-service.exe --control status
target\release\capyio-windows-service.exe --control start
target\release\capyio-windows-service.exe --control stop
```

Service/driver installation and configuration still require an administrator
and are not yet integrated into a signed product installer.

`capyio-microphone-host` is the ordinary-user headless owner selected by ADR
0040. It reads the fixed trusted MicYou host configuration and exposes a
separate owner-scoped local pipe with only `status`, `start` and `stop`. The
privileged service still owns the global capture ring; it does not launch the
third-party MicYou process.

For source/development inspection after starting the host in the same user
session:

```powershell
target\release\capyio-microphone-host.exe --control status
target\release\capyio-microphone-host.exe --control start
target\release\capyio-microphone-host.exe --control stop
```

Login autostart and signed installation are intentionally deferred. Until that
deployment slice is complete, CapyIO Desktop retains its direct MicYou
supervisor as a development fallback.
