# SensorServer protocol Adapter

`CAPY-IMU-001B0` implements the bounded protocol-message parser and the
accelerometer/gyroscope pairing policy for the first real IMU Source Adapter.
`CAPY-IMU-001B1` adds a synchronous worker-owned WebSocket client with an
IP-literal endpoint, fixed sensor paths, connect/read/write deadlines and 4 KiB
frame/message limits.

The upstream external service is
[UmerCodez/SensorServer](https://github.com/UmerCodez/SensorServer), pinned for
protocol review at commit `5ae401780d99debcabb8dc259256c2652dada0a6` and
licensed `GPL-3.0-only`. No upstream source or binary is imported, linked,
repackaged or distributed by CapyIO. Provenance is recorded in
`third_party/THIRD_PARTY.yml`.

The documented per-sensor WebSocket message has `accuracy`, `timestamp` and a
sensor-specific `values` array. The Adapter treats the endpoint's selected
Android string sensor type as trusted configuration, bounds every JSON message,
requires exactly three finite axes, preserves Android elapsed-realtime source
timestamps and rejects timestamp regression.

The client uses `tungstenite` 0.30.0 with only its handshake feature; no async
executor or TLS stack is enabled. Local mock-server tests cover valid text,
malformed/binary/oversized data, ping/pong, close, timeout and oversized
handshake behavior.

Physical SensorServer connection, automatic reconnect/epoch orchestration, APK
installation and live data claims remain follow-up work. Plain `ws://` is
permitted only in an explicitly labeled trusted local lab and is not production
security.
