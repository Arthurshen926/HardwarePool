# ADR 0003: Thin Windows driver and user-mode Broker

Status: accepted

Render-path amendment: ADR 0027 prefers standard WASAPI loopback for `CapyIO
Speaker`; the custom PCM IPC remains a fallback rather than a Gate 7B default.

## Context

Windows system-level audio endpoints require driver work, while networking, protocols, codecs and reconnect logic are complex and process untrusted input.

## Decision

The Windows driver exposes fixed virtual playback/capture endpoints and a minimal bounded PCM/status IPC. A user-mode Rust Broker owns all network, identity, protocol, codec, buffering, drift and reconnect logic.

## Consequences

- Driver changes and signing frequency are minimized.
- Kernel attack surface is smaller.
- Linux/macOS can reuse Broker/Core logic while replacing Projection Adapter.
- IPC contract requires its own versioning, fuzzing and lifecycle tests.
