# ADR 0033: Own the dedicated speaker Broker in a Windows service

Status: accepted

Amends: ADR 0032's temporary desktop-host lifecycle ownership.

## Context

The post-mix APO normally runs inside AudioDG in Session 0. Its fixed
`Global\\CapyIO.RenderRing.v1` mapping must also be created by a principal that
can cross Windows sessions. Elevating the complete Tauri desktop host proves
the path but gives the WebView host unnecessary privilege and makes window
closure terminate an otherwise headless Route.

## Decision

Add a headless `CapyIOBroker` Windows own-process service. The service directly
supervises the fixed `capyio-virtual-speaker` Broker, waits for its listener,
observes only process-owned receiver presence and reaps the child on service
stop or failure. It accepts an explicit broker path, non-unspecified IPv4 bind
address and non-zero port from trusted service launch configuration; it does
not accept an endpoint identifier or shell command.

The first slice deliberately does not expose local management IPC. It proves
that lifecycle and the privileged global-ring owner can live outside Tauri.
Until a narrow ACL-protected management API is added, the service starts the
Broker with its own lifecycle and the desktop's existing direct mode remains a
development fallback. The two owners must not run on the same bind port.

Use `windows-service` 0.8.1 (MIT OR Apache-2.0) for Service Control Manager
dispatch, status and stop handling. It is maintained by Mullvad, targets
`windows-sys` 0.61 and avoids hand-written SCM FFI. Alternatives considered
were raw Win32 FFI, scheduled tasks and keeping the elevated GUI owner; the
first adds unsafe maintenance surface and the latter two retain the wrong
lifecycle/privilege boundary.

## Consequences

- Daily operation can later require elevation only during installation, not
  for the desktop UI.
- Closing Tauri no longer has to define Broker lifetime once the desktop uses
  the service control boundary.
- A bounded `--console --run-for-ms` mode allows hardware-free/process-fixture
  validation without installing a service.
- Service installation, service ACLs, persisted configuration and desktop
  management IPC remain separate reviewed slices. No service or driver is
  installed by this ADR.
