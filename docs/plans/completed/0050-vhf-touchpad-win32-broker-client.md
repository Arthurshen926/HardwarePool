# CAPY-PTP-003C — Win32 VHF Broker client

Status: complete

Owner: Codex

Created: 2026-08-31

Requirements: `FR-SCEN-006`, `FR-PLAT-004`, `NFR-STAB-001..004`,
`NFR-SEC-001..003`, `NFR-PERF-001..003`, `NFR-MAINT-001..003`

## Objective

Implement and hardware-free test the Windows user-mode client that locates the
single CapyIO VHF device interface, opens it exclusively, submits canonical
Broker records with `DeviceIoControl`, validates exact acknowledgements and
projects direction-neutral touchpad snapshots into the driver's fixed ABI.

## In scope

- bounded SetupAPI device-interface enumeration with zero/one/many outcomes;
- direct `CreateFileW` and synchronous `DeviceIoControl`, never a shell;
- RAII handle/device-information cleanup and exact Win32 error mapping;
- transport-generic Broker session that poisons on unknown delivery or Ack
  mismatch;
- deterministic himetric-to-0..4095 coordinate projection;
- fake-transport tests for Hello/Data/Close, failed writes and malformed Acks;
- a read-only probe that reports whether exactly one driver interface exists;
- record the renewed Android wireless-debug endpoint as
  `100.66.157.119:46143` without installing or changing the APK.

## Out of scope

- driver install/remove, signing, root-device creation or security changes;
- sending real desktop input;
- connecting the client to the Android network receiver or Runtime Route;
- claiming three-/four-finger gesture compatibility.

## Safety

This slice is code, compile and read-only enumeration only. It must not call
driver deployment/signing tools, create a device, open an injection session or
send an IOCTL to a real driver interface.

Completed evidence: `docs/CAPY_PTP_003C_REPORT.md`.
