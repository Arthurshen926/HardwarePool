# CAPY-IMU-001B2 Completion Report

Date: 2026-08-24

Status: physical lab complete; Gate 5 remains open

## Outcome

An authorized Android phone streamed physical accelerometer and gyroscope data
to the Windows development host through the bounded SensorServer Adapter. The
new `sensor-server-live` command creates separate worker-owned connections,
pairs asynchronous readings, publishes standard IMU envelopes to bounded
independent consumers, and prints both numeric Panel state and Recorder JSONL.

## Device and supply-chain evidence

- wireless ADB was already trusted and reconnected without a pairing operation;
- official SensorServer v7.2.1 corresponded to the repository-pinned upstream revision;
- the downloaded APK SHA-256 matched the digest published with the release;
- Android reported package `github.umer0586.sensorserver`, version 7.2.1;
- the human approved the Android notification permission before service use.

The external APK and physical network addresses are not committed or
distributed by CapyIO.

## Live data evidence

The initial physical run emitted 16 sequential `capyio.motion.imu-samples/1`
envelopes in approximately 3.2 seconds. Each envelope retained independent
accelerometer and gyroscope Android elapsed-realtime timestamps, SI units,
Android device coordinates, accuracy, sequence and stream epoch.

The finalized command then ran twice consecutively without restarting the
phone service. Each run reported:

```text
panel_received=8
panel_missing_sequences=0
recorder_records=8
emitted_samples=8
```

The two runs used distinct Stream IDs. Client-initiated WebSocket Close allowed
the second run to connect cleanly. During a separate long run, stopping the
physical service produced an explicit close with code 4004 instead of fabricated
continuity or timestamp repair. The service was restarted after the test.

## Transport finding

The phone's SensorServer port was reachable through its same-LAN address. TCP
to the phone's Tailscale address was not exposed, while ADB itself remained
reachable over Tailscale. ADB port forwarding accepted TCP but did not complete
the SensorServer WebSocket handshake on this device. Therefore the retained
payload run used direct LAN WebSocket transport and wireless ADB only for
deployment/control. No Tailscale forwarding or device firewall setting was
changed.

## Automated evidence

- SensorServer Adapter tests: 17 pass (9 parser/pairing, 8 WebSocket);
- focused Clippy with warnings denied: pass;
- client-initiated Close and non-reuse behavior: automated loopback test;
- full repository format/check/Clippy, 96 Rust tests, fixture demo, docs,
  manifests, Adapter Smoke and frontend typecheck/build passed through
  `cargo xtask ci`.

## Remaining work

The command is a lab surface, not the desktop product. It does not implement
automatic retry, epoch advancement after reconnect, Runtime Route lifecycle or
Tauri rendering. Plain `ws://` is not application authentication. These limits
remain for `CAPY-IMU-001B3` and the later security Gate. Audio work still follows
the completed end-to-end IMU Gate 5 path.
