# CAPY-IMU-001B2 — Physical SensorServer lab

Status: complete

Owner: Codex

Created: 2026-08-24

Completed: 2026-08-24

Requirements: `FR-PORT-002..005`, `FR-ROUTE-003`, `FR-DIAG-001`, `NFR-SEC-005`, `NFR-STAB-002..005`

## Objective

Prove that the bounded B0 parser/pairing and B1 WebSocket client can consume an
authorized physical Android device and deliver one StandardPort stream to
independent Panel and Recorder consumers without claiming production security.

## In scope

- verify and install the fixed external SensorServer APK;
- start the service under human-approved notification permission;
- connect accelerometer and gyroscope through bounded worker clients;
- pair and fan out live IMU envelopes;
- prove graceful repeated runs and explicit physical disconnect;
- retain sanitized evidence without device secrets or addresses.

## Out of scope

- CapyIO Android APK, automatic permissions or background lifecycle;
- Runtime/Tauri live UI and automatic retry/epoch advancement;
- WAN/Tailscale application authorization, TLS or public deployment;
- microphone, speaker, virtual device or driver work.

## Completion record

- official v7.2.1 APK matched the publisher SHA-256 and fixed revision;
- wireless ADB reused the existing trust and installed/launched the app;
- direct LAN WebSocket transport produced 16 paired envelopes in 3.2 seconds;
- the finalized bounded command delivered 8/8 samples to Panel and Recorder
  with zero missing sequences;
- two consecutive finalized runs succeeded without service restart;
- stopping the physical service caused an explicit close with code 4004;
- client-initiated Close gained a loopback contract test.

Follow-up: `CAPY-IMU-001B3` Runtime/Tauri live state and lifecycle integration.
