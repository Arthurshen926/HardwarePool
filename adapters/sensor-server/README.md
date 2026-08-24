# SensorServer protocol Adapter

`CAPY-IMU-001B0` implements only the bounded protocol-message parser and the
accelerometer/gyroscope pairing policy for the first real IMU Source Adapter.
It does not yet open a socket or access a phone.

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

Network/WebSocket connection, authentication, reconnect, APK installation and
physical-device claims remain follow-up work. Plain `ws://` is permitted only
in an explicitly labeled trusted local lab and is not production security.
