# ADR 0022: Use bounded synchronous Tungstenite for the SensorServer lab client

- Status: Accepted
- Date: 2026-08-24

## Context

`CAPY-IMU-001B0` deliberately stopped before selecting a concrete WebSocket
implementation. The next slice needs RFC 6455 handshake/frame correctness,
control-frame behavior, size limits and a deterministic mock server. Rewriting
WebSocket framing would add security and interoperability risk unrelated to
CapyIO's hardware model.

The user explicitly approved `tungstenite` 0.30.0. The crate is maintained at
`snapview/tungstenite-rs`, declares `MIT OR Apache-2.0`, supports Rust 1.85 and
provides blocking client/server APIs plus configurable frame/message limits.

## Decision

Add `tungstenite` 0.30.0 with default features disabled and only `handshake`
enabled. The first SensorServer client is synchronous and worker-thread-owned;
it does not introduce Tokio or an async Runtime dependency. The Adapter uses a
preconnected `TcpStream` with explicit connect/read/write timeouts, then performs
the WebSocket handshake with a small `WebSocketConfig`.

Only IP-literal `ws://` endpoints are accepted in this local-lab slice. DNS,
`wss://`, proxies, redirects, credentials and arbitrary URI paths are rejected.
Each sensor kind maps to one fixed documented path. Message and frame limits are
4 KiB, binary/data frames fail explicitly, and ping/pong/close are handled
without entering the IMU JSON parser.

The connection object emits validated SensorServer readings only. Pairing,
stream epoch, fan-out, Panel and Recorder remain separate layers. A reconnect
creates a new Adapter session and must advance the data epoch in the later
orchestration slice.

## Alternatives

Implement RFC 6455 locally; use `tokio-tungstenite`; use a browser/WebView
WebSocket; use Python as the production Adapter; enable a TLS stack before the
application authorization model exists.

## Consequences

The dependency adds HTTP handshake, SHA-1, randomness and byte-buffer crates to
the lockfile, but avoids an async executor and TLS stack. A local mock server can
exercise real framing and limits on all desktop CI platforms. This remains an
insecure trusted-lab connection and does not satisfy production authentication,
authorization, encryption or replay-defense requirements.
