# ADR 0023: Keep the physical IMU lab behind narrow Tauri commands

- Status: Accepted
- Date: 2026-08-24

## Context

`CAPY-IMU-001B2` proved the SensorServer Adapter against an authorized phone,
but its command-line surface did not prove that a Windows user could observe
and stop the stream through the desktop application. The Tauri host rules do
not permit broad or arbitrary network powers, and the production Node Runtime
must eventually outlive any WebView window.

## Decision

Expose three typed Tauri commands for this lab only: read the latest physical
IMU state, start a stream for an IP-literal address and non-zero port, and stop
the active stream. The WebView receives structured values and Problems; it does
not receive a socket, URL, platform handle or generic fetch capability.

The Rust host owns the bounded worker, stop flag and latest-state snapshot. It
uses the existing SensorServer Adapter, whose sensor paths, frame limits and
deadlines are fixed. DNS, arbitrary paths, credentials, TLS options, shell and
filesystem access are not exposed. Closing the application signals and joins
the worker.

This is an explicitly labelled trusted-LAN physical lab. It is not the
production Runtime Route lifecycle and does not satisfy authentication,
authorization or encrypted-transport requirements. A later slice will move
long-lived Adapter and Route orchestration behind the Node Runtime boundary.

## Alternatives

Open WebSockets directly from Vue; add a general-purpose Tauri HTTP/WebSocket
plugin; reuse the deterministic demo Route state as if it were physical; block
the Tauri command thread while receiving samples; defer all user-visible live
evidence until the production service exists.

## Consequences

The desktop application can now prove real numeric rendering, failure recovery
and explicit stop without broadening the WebView's authority. The temporary
host-owned lifecycle is intentionally small and must not become the permanent
service architecture. Plain `ws://` remains limited to an authorized trusted
lab.
