# 0.5.15 camera and UI repair slice

- Windows camera compatibility receivers are presented under Remote Resources,
  although their host operation remains local capability control. Catalog detail
  links open the existing compatibility action; they neither bind a peer nor
  authorize/start a Route. Existing endpoint settings remain authoritative.
- Unsupported Route authorization no longer says that the disabled button can
  request access. Camera capture/busy state is not system projection readiness.
- Visible-panel polling skips unrelated telemetry. Native state events mark one
  pending snapshot refresh, coalesced at one second; telemetry batches cannot
  overlap. Hidden documents do not poll live panels. Runtime/hardware ownership
  is unchanged.
- Trusted-LAN camera export retries while explicitly enabled capture remains
  alive, using bounded individual connects and a one-second retry delay. Stop
  still closes the socket, interrupts the worker and joins it. The legacy ADB
  lab retry bound is unchanged. This fixes a receiver-start-order failure mode,
  not every possible blank-camera failure. No ADB data forwarding is introduced.
- Camera UI exposes existing capture/encode/send/drop counters and transport
  status without frame payloads. Sending is not proof of Windows consumer output.

Validation: use shared Vue typecheck/build and unified APK compilation. Physical
acceptance must start Android before Windows, observe subsequent sent-frame
growth, view live output in Windows Camera, and check audio+camera UI response.
No broad test suite is requested for this slice. Physical outcomes remain open
until observed; no driver, permission, credential or security policy is changed.

## Observed on 2026-09-28

- Shared Vue production build and unified Android `assembleDebug` passed. The
  initial sandbox build could not read SDK `core-lambda-stubs.jar`; the approved
  host-context rebuild succeeded. Desktop custom-protocol build succeeded with
  existing unused camera-broker warnings.
- Installed 0.5.15-dev (27) on the authorized Y700, preserving app data. The
  user started the rebuilt desktop system-camera entry from Remote Resources.
- Android UI reported trusted-LAN sending with captured=30, encoded=32,
  sent=31, queue drops=0. User then explicitly confirmed live picture in Windows
  Camera. This is short physical camera acceptance, not long-run reliability.
- Follow-up source changes add an explicit `--continuous` receiver mode for
  desktop-managed sessions (standalone lab frame limits remain the default),
  and preserve Android's sent count across reconnects. Receiver Cargo check and
  APK rebuild passed. These follow-up changes are NOT yet installed into the
  currently running desktop/phone; do not confuse them with the live picture
  acceptance above.
- Concurrent speaker/camera responsiveness is awaiting user observation.

## 0.5.16 rendering follow-up

User reported continuing visible stutter with concurrent resources. Android
gfxinfo on the existing process reported 2966 frames, 2096 janky (70.67%),
2084 slow draw-command frames, 887 high-input-latency counts, median 23 ms,
and 99th percentile 42 ms. These are cumulative counters, not a controlled
before/after benchmark and not proof of one exclusive cause.

Removed backdrop blur from the sticky header and all cards/panels, using opaque
surfaces and smaller shadows without changing layout. MainActivity no longer
formats/repaints the hidden legacy audio controls or self-schedules their 250 ms
poll behind the shared UI. Opening native diagnostics still refreshes them.
Shared Vue build and unified APK 0.5.16-dev (28) compilation passed. Real
concurrent-resource responsiveness still requires post-install measurement.
