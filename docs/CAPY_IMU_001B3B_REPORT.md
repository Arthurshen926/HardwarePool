# CAPY-IMU-001B3B Runtime-owned physical IMU Route report

Status: local implementation and authorized physical-lab verification complete

Date: 2026-08-24

## Outcome

The Windows Tauri physical IMU worker now reports all lifecycle outcomes to the
desktop Node's single `NodeRuntime`. The Runtime owns the SensorServer
ExternalService Adapter catalog, built-in Panel Adapter catalog and one typed
`ExternalProtocol` Route. The WebView receives only the projected Route ID,
state, epoch and latest stable Problem code.

The successful path is explicit:

`Draft -> Prepared -> Starting -> Active -> Stopping -> Stopped`

A connection or read failure retains
`CAPY.IMU.SENSORSERVER_DISCONNECTED`, changes the Route to `Offline`, and
invalidates the current epoch. Explicit retry recovers the Route and begins a
new attempt with a later epoch. Worker threads are signalled and joined before
the stop completion is recorded.

## Automated evidence

- Desktop loopback success drives the Runtime Route to `Active`, emits paired
  samples and reaches `Stopped` after explicit shutdown.
- Loopback connection failure reaches `Offline`, retains the Route-related
  retryable Problem and proves retry epoch is later than the offline epoch.
- Runtime/testkit tests cover the staged lifecycle, bounded monotonic events and
  unrelated Route isolation.
- The physical test is ignored by default and requires explicit
  `CAPYIO_LIVE_IMU_IP` and `CAPYIO_LIVE_IMU_PORT` values.

## Authorized physical evidence

Wireless ADB trust was reused through the user-supplied main debug endpoint;
the pairing endpoint and code were not needed. SensorServer v7.2.1 initially
displayed a running state while its TCP listener was stale. The failed client
attempt correctly produced `Offline` instead of activity. After a verified
stop/start of the phone service, TCP became reachable and the ignored physical
test received real accelerometer/gyroscope pairs, reached `Active`, then stopped
cleanly in `Stopped`.

Private addresses, pairing codes and raw device identifiers are intentionally
excluded from this report.

## Verification commands

```text
cargo check -p capyio-runtime -p capyio-desktop --all-targets
cargo test -p capyio-desktop
cargo test -p capyio-desktop physical_live_imu_worker_updates_the_tauri_dto_and_stops_cleanly -- --ignored --nocapture
cargo xtask ci
pnpm --dir apps/desktop typecheck
pnpm --dir apps/desktop build
```

## Remaining boundaries

- SensorServer is an external GPL-3.0-only development dependency, not a
  CapyIO Android application.
- The transport is private-LAN WebSocket without CapyIO pairing, authentication
  or encryption.
- The Node Runtime is still hosted by the desktop process rather than a
  long-lived headless service.
- Automatic retry policy, 3D rendering, product Recorder UI, microphone,
  speaker and Windows virtual-device projection remain future work.
