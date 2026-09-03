# CAPY-GAMEPAD-001E report

Date: 2026-08-29

Branch: `codex/capyio-gamepad`

Base: `fc3da3636ca6c969667e71a9b596dcc944380146`

## Result

The `capyio-gamepad-dsu-lab` executable is ready for the first physical gate.
It opens independent bounded SensorServer accelerometer/gyroscope readers,
pairs them under the Runtime-selected epoch, starts the DSU Projection on an
explicit IPv4-loopback port and submits a bounded sample count. Signed axis
permutations are explicit and validated. The phone IP is never printed.

Success requires a real DSU subscription, motion delivery, complete Worker
drain and zero queue/contract/projection/transport error counters. Cleanup
joins readers, stops the Runtime Route and releases the port. A separate
preflight checks and releases the exact intended loopback port without
contacting a device.

## Dependency note

The Windows composition crate adds the internal Apache-2.0
`capyio-sensor-server-adapter` dependency and the already workspace-pinned
Tungstenite 0.30.0 dependency only for fixture tests. The production binary
reuses the SensorServer Adapter's existing pinned `handshake`-only transport;
no new external version, feature or license enters the graph. Copying its JSON
or WebSocket protocol into the lab binary was rejected.

## Automated evidence

The end-to-end fixture uses two real loopback WebSockets and a real UDP DSU
subscriber. It observed:

```text
requested_samples=16
accepted_samples=16
queue_full=0
projection_errors=0
subscriptions_added=1
motion_packets_sent=16
transport_failures=0
route_state_after_cleanup=stopped
lab_result=pass
```

Parser/preflight fixtures close command/port/sample/mapping bounds and prove
port release. The conventional-port preflight also passed locally for UDP
26760. `cargo xtask ci` passes the full workspace format, check, Clippy, tests,
deterministic IMU demo, documentation/manifests, Adapter smoke, structural
validation and desktop typecheck/build gates. Physical phone/emulator direction
and reconnect evidence remain the next manual action in
`docs/CAPY_GAMEPAD_DSU_LAB.md`; they are not claimed by these fixtures.
