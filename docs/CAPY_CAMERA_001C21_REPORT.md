# CAPY-CAMERA-001C21 — Bounded encoded-progress watchdog and MVP closure

Date: 2026-08-31

Status: implementation, automated validation and exact-hash V2419A normal-flow
regression complete.

## Objective

Close the last observed baseline lifecycle gap before freezing camera feature
scope. A Camera2 session that configures but stops producing AVC must fail and
close instead of displaying `Streaming` indefinitely.

## Implementation

- `CameraProgressWatchdog` is an Android-free monotonic-time contract with a
  one-second check interval and five-second encoded-progress timeout.
- The timeout begins only after the repeating Camera2 request starts. Every
  accepted encoded access unit advances the monotonic progress timestamp.
- One reusable Handler runnable performs the periodic check. Expiry reports one
  explicit error through the existing Activity failure transition, which closes
  Camera2, MediaCodec and the loopback exporter.
- Session close removes the pending watchdog callback before stopping its
  HandlerThread. A paused/stopped Activity therefore cannot receive a stale
  watchdog failure.
- The watchdog observes encoding, not receiver connectivity: a temporarily
  absent Windows receiver remains governed by the existing bounded reconnect
  policy and does not falsely classify a healthy encoder as stalled.
- No permission, service, camera control, wire format, queue, network endpoint
  or Windows system behavior changed.

## Automated evidence

- Pure tests cover the exact pre-timeout/timeout boundary and reject negative or
  regressing monotonic timestamps.
- Repository validation pins the constants, scheduled check, failure text and
  close-time callback removal.
- Offline contract tests, API-29 debug APK assembly and warnings-as-errors lint
  pass.
- `aapt2` reports exactly CAMERA and INTERNET, minSdk 29 and targetSdk 36.
- Debug APK SHA-256:
  `4E446C6A6CA5AA765BCA2D0FA9192212185AAF887FB3FA32BFB76E0DD231E990`.

## Camera MVP closure

The controlled baseline now covers Camera2 permission/lifecycle, visible
preview, front/back and vendor-neutral Camera ID/Zoom selection, bounded quality
presets, MediaCodec AVC, private bounded CAVC transport, ADB-reverse reconnect,
Windows inbox decode, shared-memory publication, a Session/CurrentUser virtual
camera, Windows inbox Camera pixels, stream restart and explicit stall failure.

This is a local-lab MVP, not production delivery. Pairing/encryption, transport
without ADB, installer/signing, unified Runtime/desktop lifecycle, long-duration
reliability and cross-vendor/application matrices remain future productization.

## Physical evidence

After exact hash/target approval, the authorized APK hash matched this report
and the device remained `V2419A / PD2419` with CAMERA and INTERNET granted. A
direct Windows receiver decoded 30 changing 1280x720 frames at 30 fps with
low-latency mode enabled, first/last checksums
`c0c97a4de393995a` / `f0b1d8aa9259d44e`, and zero pending samples.

A separate fixed `live-hold` run created the Global mapping, enumerated exactly
one `CapyIO Camera`, retained it for 60 seconds and reported `live_hold=pass`,
`receiver_cleanup=pass` and `cleanup=pass`. The subsequent front/back continuity
run exposed Windows socket error 10035 and was corrected in
`CAPY-CAMERA-001C22` rather than being hidden as a successful C21 result.
