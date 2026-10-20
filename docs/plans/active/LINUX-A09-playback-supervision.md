# LINUX-A09: Supervise admitted Linux lab playback

Date: 2026-10-20
Status: implemented; review and exact-head hosted validation pending.
Base: 23c3131c42a70f21b92a66738911092ee2955c49
ADR: [1521](../../adr/1521-linux-playback-supervision.md)

## Scope and acceptance

- Explicit host entry consumes real accepted paired TLS owner and existing local
  Runtime authority; no synthetic admission, CLI credentials or default listener.
- Fair synchronous loop checks local Stop/owner loss and lease/grant/lifetime,
  polls bounded authenticated control and media, and cleans on every exit.
- Incremental TLS receive preserves bounded frames and original partial-frame
  deadline; idle is not an error and encrypted-only traffic cannot starve work.
- Preserve existing synchronous TLS and platform behavior. Validate no new wire
  or external production dependency; record remaining executable prerequisites.
- Socket-free tests, strict Clippy, formatting, compile-only IPC and repository
  validation locally; actual TLS/IPC via dedicated hosted workflow before acceptance.

## Limitations and next gate

This is supervision, not a finished Linux host integration. No real listener,
credential provisioning, pairing/consent frontend or device setup occurs. The
next executable composition must implement the five explicit prerequisites in
ADR 1521 without manufacturing grants from input. UDP remains insecure lab
transport. Actual Android/PipeWire and hard real-time latency are not verified.
